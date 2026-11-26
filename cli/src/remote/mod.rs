//! The hosted MCP server: Takt's tools over the web, for assistants that
//! can't run a local process (Claude and ChatGPT on the web and on phones),
//! signed in to the user's own Takt account. `docs/remote-mcp.md` is the
//! whole picture; in short:
//!
//! - **Transport**: MCP's Streamable HTTP, `POST /mcp`, answered with plain
//!   JSON. No server-sent stream (`GET /mcp` is 405, which the spec allows):
//!   the tools only answer, and Cloud Run bills while a request is open.
//!   Stateless: no `Mcp-Session-Id`, so any instance can take any request.
//! - **Sign-in**: OAuth 2.1, with the Supabase project the apps sign in
//!   with as the authorization server. `/mcp` without a valid token answers
//!   401 with `WWW-Authenticate` naming this server's protected-resource
//!   metadata (RFC 9728), which names Supabase; the client registers itself
//!   there, sends the user through Google, Apple or email sign-in and the
//!   consent page (`site/oauth/consent/`), and comes back with a token.
//! - **Payment**: Takt Sync. The workspace is the account's synced one, and
//!   the sync server answers `402` to an account without the subscription,
//!   which a tool call passes on as its error. No second check to keep in
//!   step with the first.
//! - **The work**: `accounts.rs` keeps each account's workspace as a device
//!   of the account and syncs around every call; `host.rs` says which tools
//!   are offered. The JSON-RPC itself is the stdio server's (`mcp::respond`).

mod accounts;
mod auth;
mod host;

pub use accounts::{Accounts, SyncServer};
pub use auth::Verifier;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// The most a request body may be: tool arguments are small.
const MAX_BODY: usize = 4 * 1024 * 1024;

/// The server's settings, from the environment (`docs/remote-mcp.md`).
#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    /// `https://<ref>.supabase.co`, the project whose accounts are served.
    pub supabase_url: String,
    /// The legacy HS256 secret, for a project still on it (and the tests).
    pub jwt_secret: Option<String>,
    /// The sync server, e.g. `https://api.takt.maybeitssoftware.co.uk`.
    pub sync_url: String,
    /// This server's public address, e.g. `https://mcp.takt…`; without it,
    /// taken from each request's `Host`, as `https`.
    pub public_url: Option<String>,
    /// Where the accounts' workspaces are kept.
    pub data_dir: PathBuf,
    /// How long an account's workspace is kept after its last call.
    pub idle: Duration,
    /// `REQUIRE_SUBSCRIPTION`: serve only accounts with Takt Sync. Off
    /// unless set, as on the sync server, so a self-hosted one is free.
    pub require_subscription: bool,
}

impl Config {
    pub fn from_env() -> Result<Config, String> {
        let var = |key: &str| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        let required = |key: &str| var(key).ok_or_else(|| format!("{key} is required"));
        Ok(Config {
            port: var("PORT")
                .map(|port| port.parse().map_err(|_| "PORT is not a port".to_owned()))
                .transpose()?
                .unwrap_or(8080),
            supabase_url: required("SUPABASE_URL")?,
            jwt_secret: var("SUPABASE_JWT_SECRET"),
            sync_url: required("SYNC_SERVER_URL")?,
            public_url: var("PUBLIC_URL").map(|url| url.trim_end_matches('/').to_owned()),
            data_dir: var("DATA_DIR")
                .map_or_else(|| std::env::temp_dir().join("takt-mcp"), PathBuf::from),
            idle: Duration::from_secs(
                var("IDLE_SECONDS")
                    .and_then(|seconds| seconds.parse().ok())
                    .unwrap_or(30 * 60),
            ),
            require_subscription: var("REQUIRE_SUBSCRIPTION").is_some_and(|flag| {
                matches!(
                    flag.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            }),
        })
    }
}

pub struct AppState {
    pub config: Config,
    pub verifier: Verifier,
    pub accounts: Accounts,
}

/// One log line, as JSON, which Cloud Run reads as structured.
pub fn log(severity: &str, message: &str) {
    eprintln!(
        "{}",
        json!({ "severity": severity.to_ascii_uppercase(), "message": message })
    );
}

pub fn state(config: Config) -> Arc<AppState> {
    let verifier = Verifier::for_project(&config.supabase_url, config.jwt_secret.as_deref());
    let accounts = Accounts::new(
        config.data_dir.clone(),
        SyncServer {
            url: config.sync_url.clone(),
        },
        config.idle,
        config.require_subscription,
    );
    Arc::new(AppState {
        config,
        verifier,
        accounts,
    })
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/health", get(health))
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        // RFC 9728's path-suffixed form, which some clients ask for first.
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(protected_resource),
        )
        .route("/mcp", get(no_stream).post(mcp).delete(no_stream))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY))
        .with_state(state)
}

/// This server's address as the client reached it.
fn base_url(state: &AppState, headers: &HeaderMap) -> String {
    if let Some(url) = &state.config.public_url {
        return url.clone();
    }
    let host = headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .unwrap_or("localhost");
    let scheme = if host.starts_with("localhost") || host.starts_with("127.0.0.1") {
        "http"
    } else {
        "https"
    };
    format!("{scheme}://{host}")
}

/// RFC 9728: what this resource is and who issues its tokens.
pub fn resource_metadata(base: &str, supabase_url: &str) -> Value {
    json!({
        "resource": format!("{base}/mcp"),
        "resource_name": "Takt",
        "authorization_servers": [format!("{}/auth/v1", supabase_url.trim_end_matches('/'))],
        "bearer_methods_supported": ["header"],
        "scopes_supported": ["openid", "email", "profile"],
        "resource_documentation": "https://takt.maybeitssoftware.co.uk/",
    })
}

async fn protected_resource(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let base = base_url(&state, &headers);
    (
        [(header::CACHE_CONTROL, "public, max-age=3600")],
        axum::Json(resource_metadata(&base, &state.config.supabase_url)),
    )
        .into_response()
}

async fn health(State(state): State<Arc<AppState>>) -> Response {
    axum::Json(json!({ "ok": true, "accounts": state.accounts.held() })).into_response()
}

async fn home(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let base = base_url(&state, &headers);
    format!(
        "Takt's MCP server.\n\nAdd {base}/mcp to your assistant as a custom connector \
         (in Claude: Settings → Connectors → Add custom connector), then sign in with your \
         Takt account. It needs Takt Sync.\n"
    )
    .into_response()
}

async fn no_stream() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, "POST")],
        "This server answers POST /mcp with JSON and keeps no stream open.",
    )
        .into_response()
}

/// The 401 that starts a client's sign-in: RFC 9728's pointer to the
/// metadata, and `invalid_token` when a token was sent and refused.
fn unauthorized(base: &str, had_token: bool) -> Response {
    let mut challenge =
        format!("Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource\"");
    if had_token {
        challenge.push_str(", error=\"invalid_token\"");
    }
    let mut response = (
        StatusCode::UNAUTHORIZED,
        axum::Json(
            json!({ "error": "unauthorized", "message": "Sign in to Takt to use this server." }),
        ),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&challenge) {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, value);
    }
    response
}

async fn mcp(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> Response {
    let base = base_url(&state, &headers);
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    // Everything from here blocks — the key fetch, the sync server, SQLite —
    // so it runs on the blocking pool, one thread per request.
    let worker = tokio::task::spawn_blocking(move || {
        let Some(token) = auth::bearer(authorization.as_deref()).map(str::to_owned) else {
            return unauthorized(&base, false);
        };
        let Some(caller) = state.verifier.verify(&token) else {
            return unauthorized(&base, true);
        };
        let Ok(message) = serde_json::from_slice::<Value>(&body) else {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({
                    "jsonrpc": "2.0", "id": null,
                    "error": { "code": -32700, "message": "Invalid JSON payload." },
                })),
            )
                .into_response();
        };
        let host = host::AccountHost {
            accounts: &state.accounts,
            account: caller.account,
            token: &token,
        };
        let replies = answer_all(&host, &message);
        state.accounts.evict_idle();
        match replies {
            None => StatusCode::ACCEPTED.into_response(),
            Some(reply) => axum::Json(reply).into_response(),
        }
    });
    worker.await.unwrap_or_else(|error| {
        log("error", &format!("a request failed: {error}"));
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })
}

/// The reply to a message or a batch of them; none when nothing in it
/// wants one (notifications and responses), which is answered `202`.
fn answer_all(host: &impl crate::mcp::ToolHost, message: &Value) -> Option<Value> {
    // Stateless: whatever version `initialize` asks for is echoed, and
    // nothing later depends on it.
    let mut version = "2025-06-18".to_owned();
    match message {
        Value::Array(batch) => {
            let replies: Vec<Value> = batch
                .iter()
                .filter_map(|message| crate::mcp::respond(host, message, &mut version))
                .collect();
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        message => crate::mcp::respond(host, message, &mut version),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_metadata_names_the_projects_auth_server_and_this_resource() {
        let metadata = resource_metadata("https://mcp.example", "https://ref.supabase.co/");
        assert_eq!(metadata["resource"], json!("https://mcp.example/mcp"));
        assert_eq!(
            metadata["authorization_servers"],
            json!(["https://ref.supabase.co/auth/v1"])
        );
    }

    #[test]
    fn a_refusal_points_the_client_at_the_metadata() {
        let response = unauthorized("https://mcp.example", true);
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let challenge = response.headers()[header::WWW_AUTHENTICATE]
            .to_str()
            .expect("ascii");
        assert!(challenge.contains(
            "resource_metadata=\"https://mcp.example/.well-known/oauth-protected-resource\""
        ));
        assert!(challenge.contains("invalid_token"));
    }
}
