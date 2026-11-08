//! The row-sync server Takt's apps replicate their workspace through.
//!
//! docs/sync.md is the protocol; this is its server half. The server stores
//! rows and merges them by per-column last-write-wins. It knows nothing about
//! tasks, so a schema change on the clients needs no change here.
//!
//! Accounts are Supabase Auth users (`auth.rs`): the apps sign in with
//! Supabase and send its access token, and every row and device belongs to
//! the token's user. A device only ever sees its own account's rows.

pub mod auth;
pub mod changes;
pub mod config;
pub mod devices;
pub mod error;
/// The merge rule, shared with every client through `takt-sync-rules`.
pub use takt_sync_rules::merge;
pub mod notify;
pub mod pages;
pub mod push;
pub mod realtime;

use axum::extract::{DefaultBodyLimit, State};
use axum::routing::{get, post};
use axum::{Json, Router, middleware};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::watch;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

/// A first sync pushes every row a device has in batches of 500; this leaves
/// room for wide rows (long notes) without letting one request eat the memory
/// of a small container.
const BODY_LIMIT: usize = 10 * 1024 * 1024;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Checks the Supabase access token on every request.
    pub verifier: Arc<auth::Verifier>,
    /// Deletes Supabase users; `None` without the project's secret key, when
    /// deleting an account says it isn't set up.
    pub admin: Option<Arc<dyn devices::AccountAdmin>>,
    /// Bumped whenever any process commits a push. See `notify.rs`.
    pub changes: watch::Receiver<u64>,
    /// Flips to true on SIGTERM so long-polls answer at once rather than
    /// holding the shutdown up for their full 25 seconds.
    pub shutdown: watch::Receiver<bool>,
    /// Pushes broadcast on the account's Supabase Realtime channel, and
    /// `/health` tells the apps to listen there. See `realtime.rs`.
    pub realtime: bool,
}

pub fn router(state: AppState) -> Router {
    let user_routes = Router::new()
        .route("/v1/push", post(push::push))
        .route("/v1/changes", get(changes::changes))
        .route("/v1/devices", post(devices::register))
        .route("/v1/account", get(devices::account))
        .route("/v1/account/delete", post(devices::delete_account))
        .route("/v1/sign-out", post(devices::sign_out))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_user,
        ));
    Router::new()
        .route("/health", get(health))
        .route("/privacy", get(pages::privacy))
        .merge(user_routes)
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(CompressionLayer::new().gzip(true))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// `realtime` tells an app whether to wait for news on the account's
/// Realtime channel or by long-polling `/v1/changes`.
async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "ok": true, "realtime": state.realtime }))
}
