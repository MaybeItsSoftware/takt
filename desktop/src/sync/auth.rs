//! Supabase Auth over its REST API: what the Swift app's Supabase client did
//! for it (`SyncAuth.swift`). Accounts live there; the sync server trusts its
//! access tokens.
//!
//! Google and Apple go through the browser with PKCE: the app opens
//! `/auth/v1/authorize`, Supabase sends the browser back to
//! `takt://auth-callback?code=…`, and the code is exchanged here with the
//! verifier kept since. The links in Supabase's emails come back the same way.

use super::http::{self, Reply, escape};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Digest;
use std::time::Duration;

pub const CALLBACK: &str = "takt://auth-callback";

/// A signed-in Supabase session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    /// Seconds since the epoch.
    pub expires_at: i64,
    pub user_id: String,
    pub email: Option<String>,
}

impl Session {
    /// Whether the access token still has a minute left.
    pub fn fresh(&self, now_s: i64) -> bool {
        self.expires_at - 60 > now_s
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// Supabase said no: a wrong password, a refresh it won't honour.
    Refused(String),
    /// It couldn't be asked: offline, a server error.
    Failed(String),
}

impl AuthError {
    pub fn message(&self) -> String {
        match self {
            AuthError::Refused(text) | AuthError::Failed(text) => text.clone(),
        }
    }
}

pub struct Supabase {
    pub url: String,
    pub key: String,
}

const TIMEOUT: Duration = Duration::from_secs(30);

impl Supabase {
    fn send(
        &self,
        method: &str,
        path: &str,
        body: Value,
        bearer: Option<&str>,
    ) -> Result<Value, AuthError> {
        let url = format!("{}/auth/v1/{path}", self.url.trim_end_matches('/'));
        let authorization = bearer.map(|token| format!("Bearer {token}"));
        let mut headers = vec![("apikey", self.key.as_str())];
        if let Some(authorization) = &authorization {
            headers.push(("Authorization", authorization.as_str()));
        }
        let reply = http::request(method, &url, &headers, Some(&body.to_string()), TIMEOUT)
            .map_err(AuthError::Failed)?;
        Self::read(reply)
    }

    fn read(reply: Reply) -> Result<Value, AuthError> {
        let value: Value = serde_json::from_str(&reply.body).unwrap_or(Value::Null);
        if reply.ok() {
            return Ok(value);
        }
        let said = ["error_description", "msg", "message", "error"]
            .iter()
            .find_map(|key| value.get(key).and_then(Value::as_str))
            .map(sentence)
            .unwrap_or_else(|| format!("Sign-in answered {}.", reply.status));
        if (400..500).contains(&reply.status) {
            Err(AuthError::Refused(said))
        } else {
            Err(AuthError::Failed(said))
        }
    }

    pub fn sign_in(&self, email: &str, password: &str) -> Result<Session, AuthError> {
        let value = self.send(
            "POST",
            "token?grant_type=password",
            json!({ "email": email, "password": password }),
            None,
        )?;
        session(&value)
    }

    /// A new account. None when Supabase wants the address confirmed first.
    pub fn sign_up(&self, email: &str, password: &str) -> Result<Option<Session>, AuthError> {
        let value = self.send(
            "POST",
            &format!("signup?redirect_to={}", escape(CALLBACK)),
            json!({ "email": email, "password": password }),
            None,
        )?;
        Ok(session(&value).ok())
    }

    /// Mails a reset link, which comes back to the app with a code for
    /// `challenge`'s verifier.
    pub fn recover(&self, email: &str, challenge: &str) -> Result<(), AuthError> {
        self.send(
            "POST",
            &format!("recover?redirect_to={}", escape(CALLBACK)),
            json!({
                "email": email,
                "code_challenge": challenge,
                "code_challenge_method": "s256",
            }),
            None,
        )
        .map(|_| ())
    }

    pub fn refresh(&self, refresh_token: &str) -> Result<Session, AuthError> {
        let value = self.send(
            "POST",
            "token?grant_type=refresh_token",
            json!({ "refresh_token": refresh_token }),
            None,
        )?;
        session(&value)
    }

    /// Where the browser goes to sign in with `provider` (`google`, `apple`).
    pub fn authorize_url(&self, provider: &str, challenge: &str) -> String {
        format!(
            "{}/auth/v1/authorize?provider={provider}&redirect_to={}&code_challenge={challenge}&code_challenge_method=s256",
            self.url.trim_end_matches('/'),
            escape(CALLBACK)
        )
    }

    pub fn exchange(&self, code: &str, verifier: &str) -> Result<Session, AuthError> {
        let value = self.send(
            "POST",
            "token?grant_type=pkce",
            json!({ "auth_code": code, "code_verifier": verifier }),
            None,
        )?;
        session(&value)
    }

    pub fn update_password(&self, access_token: &str, password: &str) -> Result<(), AuthError> {
        self.send(
            "PUT",
            "user",
            json!({ "password": password }),
            Some(access_token),
        )
        .map(|_| ())
    }

    pub fn sign_out(&self, access_token: &str) {
        let _ = self.send("POST", "logout", json!({}), Some(access_token));
    }
}

fn session(value: &Value) -> Result<Session, AuthError> {
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
    let (Some(access_token), Some(refresh_token)) = (text("access_token"), text("refresh_token"))
    else {
        return Err(AuthError::Failed("Sign-in gave back no session.".into()));
    };
    let now = chrono::Utc::now().timestamp();
    let expires_at = value
        .get("expires_at")
        .and_then(Value::as_i64)
        .or_else(|| {
            value
                .get("expires_in")
                .and_then(Value::as_i64)
                .map(|s| now + s)
        })
        .unwrap_or(now + 3600);
    let user = value.get("user").cloned().unwrap_or(Value::Null);
    Ok(Session {
        access_token,
        refresh_token,
        expires_at,
        user_id: user
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        email: user
            .get("email")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// Supabase's messages end without a full stop, which a status line wants.
fn sentence(text: &str) -> String {
    let text = text.trim();
    let mut out = text.to_string();
    if let Some(first) = out.get(..1) {
        out = first.to_uppercase() + &out[1..];
    }
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

/// A PKCE verifier and its challenge.
pub fn pkce() -> (String, String) {
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let digest = sha2::Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_reads_from_supabases_answer() {
        let value = json!({
            "access_token": "a", "refresh_token": "r", "expires_at": 100,
            "user": { "id": "u", "email": "e@x" }
        });
        let session = session(&value).unwrap();
        assert_eq!(session.user_id, "u");
        assert_eq!(session.email.as_deref(), Some("e@x"));
        assert!(session.fresh(39));
        assert!(!session.fresh(41));
    }

    #[test]
    fn refusals_carry_supabases_words() {
        let reply = Reply {
            status: 400,
            body: r#"{"error_description":"invalid login credentials"}"#.into(),
        };
        assert_eq!(
            Supabase::read(reply),
            Err(AuthError::Refused("Invalid login credentials.".into()))
        );
    }

    #[test]
    fn the_challenge_is_the_verifiers_hash() {
        let (verifier, challenge) = pkce();
        assert_eq!(verifier.len(), 64);
        assert_eq!(challenge.len(), 43);
    }
}

#[cfg(test)]
mod live {
    /// Asks the real project once, with an address that has no account:
    /// `cargo test -- --ignored supabase_answers`.
    #[test]
    #[ignore]
    fn supabase_answers() {
        let answer = super::super::supabase().sign_in("nobody@example.invalid", "wrong-password");
        println!("{answer:?}");
        assert!(matches!(answer, Err(super::AuthError::Refused(_))));
    }
}
