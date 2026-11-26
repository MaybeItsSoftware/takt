//! Who a request is from: a Supabase Auth user, through an OAuth client.
//!
//! An assistant (Claude, ChatGPT, any MCP client) gets its token from the
//! Supabase project's OAuth 2.1 server, on the user's say-so at the consent
//! page (`site/oauth/consent/`). That token is an ordinary Supabase access
//! token for the user — `aud` and `role` both `authenticated` — with the
//! client's `client_id` beside them, so it is checked exactly as the sync
//! server checks an app's (`sync-server/src/auth.rs`, which this follows),
//! and the sync server accepts it as it stands when it is passed on.
//!
//! Blocking, like the rest of the remote server: every request runs on
//! tokio's blocking pool, and this fetches the key set with `ureq`.

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// How soon an unknown key id may trigger another fetch of the key set, so a
/// flood of tokens naming made-up keys can't become a flood of fetches.
const REFETCH_AFTER: Duration = Duration::from_secs(30);

/// The signed-in user a token proves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    pub account: Uuid,
    /// The OAuth client that holds the token, when it came through one.
    pub client_id: Option<String>,
}

/// Checks Supabase access tokens: asymmetric ones against the project's
/// published keys, and HS256 ones against the legacy shared secret when one
/// is configured (a project still on it, and the tests).
pub struct Verifier {
    issuer: String,
    jwks_url: Option<String>,
    secret: Option<DecodingKey>,
    keys: RwLock<HashMap<String, (DecodingKey, Algorithm)>>,
    fetched: RwLock<Option<Instant>>,
}

impl Verifier {
    /// For a project at `supabase_url` (`https://<ref>.supabase.co`).
    pub fn for_project(supabase_url: &str, legacy_secret: Option<&str>) -> Self {
        let base = supabase_url.trim_end_matches('/');
        let mut verifier = Self::new(format!("{base}/auth/v1"), legacy_secret);
        verifier.jwks_url = Some(format!("{base}/auth/v1/.well-known/jwks.json"));
        verifier
    }

    /// Accepts tokens from `issuer` signed by a key in `jwks`, never fetching.
    pub fn with_jwks(issuer: &str, jwks: &str) -> Result<Self, String> {
        let verifier = Self::new(issuer.to_owned(), None);
        verifier.load(jwks)?;
        Ok(verifier)
    }

    fn new(issuer: String, secret: Option<&str>) -> Self {
        Verifier {
            issuer,
            jwks_url: None,
            secret: secret.map(|secret| DecodingKey::from_secret(secret.as_bytes())),
            keys: RwLock::default(),
            fetched: RwLock::default(),
        }
    }

    fn load(&self, jwks: &str) -> Result<(), String> {
        let set: JwkSet =
            serde_json::from_str(jwks).map_err(|e| format!("unreadable key set: {e}"))?;
        let mut keys = HashMap::new();
        for jwk in &set.keys {
            let (Some(kid), Some(alg)) = (jwk.common.key_id.clone(), jwk.common.key_algorithm)
            else {
                continue;
            };
            let Ok(alg) = alg.to_string().parse::<Algorithm>() else {
                continue;
            };
            if let Ok(key) = DecodingKey::from_jwk(jwk) {
                keys.insert(kid, (key, alg));
            }
        }
        *self.keys.write().expect("keys lock") = keys;
        Ok(())
    }

    fn refresh(&self) -> Result<(), String> {
        let Some(url) = &self.jwks_url else {
            return Ok(());
        };
        if self
            .fetched
            .read()
            .expect("fetched lock")
            .is_some_and(|at| at.elapsed() < REFETCH_AFTER)
        {
            return Ok(());
        }
        *self.fetched.write().expect("fetched lock") = Some(Instant::now());
        let body = ureq::get(url)
            .timeout(Duration::from_secs(10))
            .call()
            .map_err(|e| format!("fetching the key set: {e}"))?
            .into_string()
            .map_err(|e| format!("reading the key set: {e}"))?;
        self.load(&body)
    }

    /// The caller a token proves, or nothing for anything short of a valid,
    /// unexpired user token from this project.
    pub fn verify(&self, token: &str) -> Option<Caller> {
        let header = decode_header(token).ok()?;
        let (key, alg) = if header.alg == Algorithm::HS256 {
            (self.secret.clone()?, Algorithm::HS256)
        } else {
            let kid = header.kid?;
            let known = self.keys.read().expect("keys lock").get(&kid).cloned();
            match known {
                Some(found) => found,
                None => {
                    // A key we haven't seen: the project may have rotated.
                    if let Err(error) = self.refresh() {
                        super::log(
                            "warn",
                            &format!("could not refresh the signing keys: {error}"),
                        );
                        return None;
                    }
                    self.keys.read().expect("keys lock").get(&kid).cloned()?
                }
            }
        };
        if alg != header.alg {
            return None;
        }
        let mut validation = Validation::new(alg);
        validation.set_audience(&["authenticated"]);
        validation.set_issuer(&[&self.issuer]);
        let claims = decode::<Value>(token, &key, &validation).ok()?.claims;
        if claims.get("role").and_then(Value::as_str) != Some("authenticated") {
            return None;
        }
        let account = Uuid::parse_str(claims.get("sub")?.as_str()?).ok()?;
        Some(Caller {
            account,
            client_id: claims
                .get("client_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }
}

/// The token in an `Authorization: Bearer …` header value.
pub fn bearer(header: Option<&str>) -> Option<&str> {
    header?
        .strip_prefix("Bearer ")
        .or_else(|| header?.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde_json::json;

    const ISSUER: &str = "https://example.supabase.co/auth/v1";
    const USER: &str = "6f1c1f9e-2a4b-4c55-9d0e-1a2b3c4d5e6f";

    /// The sync server's throwaway P-256 test key, used nowhere else.
    const EC_PRIVATE: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgypKY4u7/jCnQWKup
H3zFf/VgL3GwhAj9zYO04s+AdDyhRANCAAS3aR0qiG9wmU00685wwCMw7KKzUtmc
Y0n5D5E5J6BugCwN/1kNn5nOxyExZhNLvA8XCzu2nnaT3ntsZXmuMzvf
-----END PRIVATE KEY-----";

    fn jwks() -> String {
        json!({"keys": [{"kty": "EC", "crv": "P-256", "alg": "ES256", "kid": "k1", "use": "sig",
            "x": "t2kdKohvcJlNNOvOcMAjMOyis1LZnGNJ-Q-ROSegboA",
            "y": "LA3_WQ2fmc7HITFmE0u8DxcLO7aedpPee2xlea4zO98"}]})
        .to_string()
    }

    fn claims(issuer: &str, role: &str, exp_from_now: i64) -> Value {
        json!({"sub": USER, "role": role, "aud": "authenticated", "iss": issuer,
               "client_id": "claude", "exp": chrono::Utc::now().timestamp() + exp_from_now})
    }

    fn es256(kid: &str, claims: &Value) -> String {
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(kid.into());
        let key = EncodingKey::from_ec_pem(EC_PRIVATE.as_bytes()).expect("key");
        encode(&header, claims, &key).expect("token")
    }

    #[test]
    fn an_oauth_clients_token_is_its_user() {
        let verifier = Verifier::with_jwks(ISSUER, &jwks()).expect("jwks");
        let caller = verifier
            .verify(&es256("k1", &claims(ISSUER, "authenticated", 600)))
            .expect("valid");
        assert_eq!(caller.account.to_string(), USER);
        assert_eq!(caller.client_id.as_deref(), Some("claude"));
    }

    #[test]
    fn anything_else_is_refused() {
        let verifier = Verifier::with_jwks(ISSUER, &jwks()).expect("jwks");
        for token in [
            es256("k1", &claims(ISSUER, "authenticated", -600)),
            es256(
                "k1",
                &claims("https://other.supabase.co/auth/v1", "authenticated", 600),
            ),
            es256("k1", &claims(ISSUER, "anon", 600)),
            es256("unknown", &claims(ISSUER, "authenticated", 600)),
            "not a token".to_owned(),
        ] {
            assert!(verifier.verify(&token).is_none());
        }
        // HS256 with no secret configured: refused, not trusted.
        let hs = encode(
            &Header::new(Algorithm::HS256),
            &claims(ISSUER, "authenticated", 600),
            &EncodingKey::from_secret(b"guess"),
        )
        .expect("token");
        assert!(verifier.verify(&hs).is_none());
    }

    #[test]
    fn a_bearer_header_gives_its_token() {
        assert_eq!(bearer(Some("Bearer abc ")), Some("abc"));
        assert_eq!(bearer(Some("Bearer ")), None);
        assert_eq!(bearer(Some("Basic abc")), None);
        assert_eq!(bearer(None), None);
    }
}
