//! An account's devices, signing one out, and deleting the account.
//!
//! Signing in and up happen in Supabase, between the app and Supabase; the
//! server only learns of a device when it registers itself after signing in.

use crate::AppState;
use crate::auth::{Caller, DEVICE_HEADER};
use crate::error::{AppError, Result};
use axum::extract::State;
use axum::{Extension, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub id: Uuid,
    pub name: Option<String>,
    pub platform: Option<String>,
}

/// `POST /v1/devices`: records the calling device on the account, after each
/// sign-in. A device that was on another account moves to this one.
///
/// The body's `id` must be the device the request comes from — the
/// `X-Priority-Device` header, which every client already sends with the same
/// value. Otherwise any signed-in user could name an arbitrary id here and
/// move that device off whichever account it was on.
pub async fn register(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<Value>> {
    match caller.device {
        Some(device) if device == request.id => {}
        Some(_) => {
            return Err(AppError::BadRequest(format!(
                "id does not match the {DEVICE_HEADER} header"
            )));
        }
        None => {
            return Err(AppError::BadRequest(format!(
                "registering a device needs the {DEVICE_HEADER} header"
            )));
        }
    }
    sqlx::query(
        "INSERT INTO devices (id, account_id, name, platform, created_at, last_seen_at) \
         VALUES ($1, $2, $3, $4, now(), now()) \
         ON CONFLICT (id) DO UPDATE SET account_id = EXCLUDED.account_id, \
           name = EXCLUDED.name, platform = EXCLUDED.platform, last_seen_at = now()",
    )
    .bind(request.id)
    .bind(caller.account)
    .bind(request.name)
    .bind(request.platform)
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub account_id: Uuid,
    pub email: Option<String>,
    pub devices: Vec<DeviceInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: Uuid,
    pub name: Option<String>,
    pub platform: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    /// The device asking.
    pub current: bool,
}

/// `GET /v1/account`: the signed-in email and the account's devices.
pub async fn account(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
) -> Result<Json<AccountInfo>> {
    type DeviceTuple = (
        Uuid,
        Option<String>,
        Option<String>,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
    );
    let devices = sqlx::query_as::<_, DeviceTuple>(
        "SELECT id, name, platform, created_at, last_seen_at FROM devices \
         WHERE account_id = $1 ORDER BY created_at",
    )
    .bind(caller.account)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(
        |(id, name, platform, created_at, last_seen_at)| DeviceInfo {
            current: Some(id) == caller.device,
            id,
            name,
            platform,
            created_at,
            last_seen_at,
        },
    )
    .collect();
    Ok(Json(AccountInfo {
        account_id: caller.account,
        email: caller.email,
        devices,
    }))
}

/// `POST /v1/sign-out`: takes the calling device off the device list. The app
/// signs out of Supabase itself; this only tidies the list.
pub async fn sign_out(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
) -> Result<Json<Value>> {
    if let Some(device) = caller.device {
        sqlx::query("DELETE FROM devices WHERE id = $1 AND account_id = $2")
            .bind(device)
            .bind(caller.account)
            .execute(&state.pool)
            .await?;
    }
    Ok(Json(json!({ "ok": true })))
}

/// `POST /v1/account/delete`: deletes every row in the account's own space,
/// its devices, and then the Supabase user, after handing each list it
/// shares to the member longest in it, so the email is free and no token
/// can be refreshed. Each device keeps its own local copy.
pub async fn delete_account(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
) -> Result<Json<Value>> {
    let Some(admin) = state.admin.as_deref() else {
        return Err(AppError::Unavailable(
            "deleting accounts isn't set up on this server".into(),
        ));
    };
    let mut tx = state.pool.begin().await?;
    // Its shared lists pass to someone still in them, before its own rows go.
    crate::spaces::leave_all(&mut tx, state.realtime, caller.account).await?;
    sqlx::query("DELETE FROM rows WHERE space_id = $1")
        .bind(caller.account)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM devices WHERE account_id = $1")
        .bind(caller.account)
        .execute(&mut *tx)
        .await?;
    // The user goes before the commit: if Supabase refuses, nothing is lost
    // and the app can try again.
    admin
        .delete_user(caller.account)
        .await
        .map_err(AppError::Internal)?;
    tx.commit().await?;
    tracing::info!(account = %caller.account, "deleted an account");
    Ok(Json(json!({ "ok": true })))
}

/// What the server needs of Supabase's admin API. A trait so the tests can
/// stand in for it.
pub trait AccountAdmin: Send + Sync {
    fn delete_user(
        &self,
        user: Uuid,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<(), String>> + Send + '_>>;
}

/// Supabase's admin API, with the project's secret key.
pub struct SupabaseAdmin {
    client: reqwest::Client,
    base: String,
    secret_key: String,
}

impl SupabaseAdmin {
    pub fn new(supabase_url: &str, secret_key: String) -> Self {
        SupabaseAdmin {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("a reqwest client with only a timeout builds"),
            base: supabase_url.trim_end_matches('/').to_owned(),
            secret_key,
        }
    }
}

impl AccountAdmin for SupabaseAdmin {
    fn delete_user(
        &self,
        user: Uuid,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            let response = self
                .client
                .delete(format!("{}/auth/v1/admin/users/{user}", self.base))
                .header("apikey", &self.secret_key)
                .bearer_auth(&self.secret_key)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let status = response.status();
            // Already gone is as good as deleted.
            if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
                Ok(())
            } else {
                let body = response.text().await.unwrap_or_default();
                Err(format!("supabase answered {status}: {body}"))
            }
        })
    }
}
