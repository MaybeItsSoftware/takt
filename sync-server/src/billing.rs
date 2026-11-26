//! Takt Sync: the hosted server's £1-a-year subscription.
//!
//! It is bought only in the iPhone and Android apps, through the stores, and
//! RevenueCat merges the two into one entitlement, `sync`, reported here by
//! its webhook ([`revenuecat`]). The apps log in to RevenueCat with the
//! Supabase account id, so the webhook's `app_user_id` is the account.
//!
//! Nothing is enforced unless the server runs with `REQUIRE_SUBSCRIPTION`:
//! a self-hosted server is free and has every feature. When it is on, the
//! routes that move data (push, changes, the shared-list calls) answer `402`
//! to an account without an active, unexpired entitlement
//! ([`require_subscription`]); signing in, the account page, signing out and
//! deleting the account keep working, so a lapsed device can say why and
//! let its owner leave. A shared list's members each need their own.

use crate::AppState;
use crate::auth::Caller;
use crate::error::{AppError, Result};
use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use axum::{Extension, Json};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

/// The entitlement RevenueCat grants for the yearly product.
pub const ENTITLEMENT: &str = "sync";
/// The store product: `takt_sync_yearly`, £1 a year.
pub const PRODUCT: &str = "takt_sync_yearly";

/// How the server charges, from the environment.
#[derive(Debug, Clone, Default)]
pub struct Billing {
    /// `REQUIRE_SUBSCRIPTION`: off unless set, so self-hosting is free.
    pub required: bool,
    /// `COMP_ACCOUNTS`: accounts that sync free whatever the table says.
    pub comp: HashSet<Uuid>,
    /// `REVENUECAT_WEBHOOK_SECRET`: the Authorization header RevenueCat is
    /// set to send. Without it the webhook route answers 404.
    pub webhook_secret: Option<String>,
}

impl Billing {
    pub fn from_env() -> Billing {
        let flag = std::env::var("REQUIRE_SUBSCRIPTION").unwrap_or_default();
        let comp = std::env::var("COMP_ACCOUNTS").unwrap_or_default();
        Billing {
            required: matches!(
                flag.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ),
            comp: parse_accounts(&comp),
            webhook_secret: std::env::var("REVENUECAT_WEBHOOK_SECRET")
                .ok()
                .map(|secret| secret.trim().to_owned())
                .filter(|secret| !secret.is_empty()),
        }
    }
}

/// Comma- or space-separated account ids; anything that isn't one is
/// skipped with a warning, so a typo can't take the server down.
pub fn parse_accounts(text: &str) -> HashSet<Uuid> {
    text.split([',', ' ', '\n'])
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .filter_map(|id| match Uuid::parse_str(id) {
            Ok(id) => Some(id),
            Err(_) => {
                tracing::warn!(id, "COMP_ACCOUNTS holds something that isn't an account id");
                None
            }
        })
        .collect()
}

/// An account's subscription, as `GET /v1/account` reports it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    /// Whether this server charges at all.
    pub required: bool,
    /// Whether this account may sync here now.
    pub active: bool,
    /// Whether the account has Takt Sync, bought and current or given,
    /// whether or not this server charges for sync. The hosted MCP server
    /// (`cli/src/remote`) is a paid feature even while sync is free, and
    /// reads this.
    pub entitled: bool,
    pub expires_at: Option<DateTime<Utc>>,
    /// Granted by hand rather than bought.
    pub comp: bool,
}

pub async fn subscription(state: &AppState, account: Uuid) -> Result<Subscription> {
    let row = sqlx::query_as::<_, (bool, Option<DateTime<Utc>>, String)>(
        "SELECT active, expires_at, source FROM entitlements WHERE account_id = $1",
    )
    .bind(account)
    .fetch_optional(&state.pool)
    .await?;
    let listed = state.billing.comp.contains(&account);
    let (bought, expires_at, comp) = match row {
        Some((active, expires_at, source)) => {
            let comp = source == "comp";
            let current = comp || (active && expires_at.is_none_or(|at| at > Utc::now()));
            (current, expires_at, comp)
        }
        None => (false, None, false),
    };
    Ok(Subscription {
        required: state.billing.required,
        active: !state.billing.required || listed || bought,
        entitled: listed || bought,
        expires_at,
        comp: listed || comp,
    })
}

/// Middleware for the routes that move data: `402` without a subscription
/// when the server charges, and straight through when it doesn't.
pub async fn require_subscription(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    request: Request,
    next: Next,
) -> Result<Response> {
    if state.billing.required && !subscription(&state, caller.account).await?.active {
        return Err(AppError::PaymentRequired(
            "Hosted sync needs Takt Sync (£1 a year). Subscribe in the iPhone or Android app, or self-host the server."
                .into(),
        ));
    }
    Ok(next.run(request).await)
}

#[derive(Debug, Deserialize)]
pub struct Webhook {
    pub event: Event,
}

/// The parts of a RevenueCat webhook event the server reads.
#[derive(Debug, Deserialize)]
pub struct Event {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub app_user_id: Option<String>,
    #[serde(default)]
    pub original_app_user_id: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub product_id: Option<String>,
    #[serde(default)]
    pub expiration_at_ms: Option<i64>,
    #[serde(default)]
    pub store: Option<String>,
    #[serde(default)]
    pub entitlement_ids: Option<Vec<String>>,
    #[serde(default)]
    pub transferred_from: Vec<String>,
    #[serde(default)]
    pub transferred_to: Vec<String>,
}

impl Event {
    /// The Supabase account the event is about: the first of its ids that is
    /// a uuid. RevenueCat's anonymous ids (`$RCAnonymousID:…`) aren't.
    fn account(&self) -> Option<Uuid> {
        self.app_user_id
            .iter()
            .chain(self.original_app_user_id.iter())
            .chain(self.aliases.iter())
            .find_map(|id| Uuid::parse_str(id).ok())
    }

    fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expiration_at_ms
            .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
    }

    /// Whether the event is about Takt Sync at all: an event naming other
    /// entitlements only is someone else's.
    fn concerns_sync(&self) -> bool {
        self.entitlement_ids
            .as_ref()
            .is_none_or(|ids| ids.is_empty() || ids.iter().any(|id| id == ENTITLEMENT))
    }
}

fn authorised(headers: &HeaderMap, secret: &str) -> bool {
    let Some(given) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
    else {
        return false;
    };
    let given = given.strip_prefix("Bearer ").unwrap_or(given).trim();
    // Constant time in the length of the secret.
    given.len() == secret.len()
        && given
            .bytes()
            .zip(secret.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

/// `POST /v1/webhooks/revenuecat`: a purchase, renewal, cancellation or
/// expiry, from RevenueCat. Answers 200 to anything it has read, so
/// RevenueCat stops retrying; an event it has seen before changes nothing.
pub async fn revenuecat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>> {
    let Some(secret) = state.billing.webhook_secret.as_deref() else {
        return Err(AppError::NotFound("not found".into()));
    };
    if !authorised(&headers, secret) {
        return Err(AppError::Unauthorized);
    }
    let webhook: Webhook = serde_json::from_value(body)
        .map_err(|error| AppError::BadRequest(format!("not a RevenueCat event: {error}")))?;
    let event = webhook.event;
    let mut tx = state.pool.begin().await?;
    let fresh =
        sqlx::query("INSERT INTO entitlement_events (id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(&event.id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
    if !fresh {
        return Ok(Json(json!({ "ok": true, "duplicate": true })));
    }
    let outcome = apply(&mut tx, &event).await?;
    tx.commit().await?;
    tracing::info!(event = %event.id, kind = %event.kind, outcome, "revenuecat");
    Ok(Json(json!({ "ok": true, "outcome": outcome })))
}

async fn apply(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: &Event,
) -> Result<&'static str> {
    if event.kind == "TRANSFER" {
        return transfer(tx, event).await;
    }
    if !event.concerns_sync() {
        return Ok("another entitlement");
    }
    let Some(account) = event.account() else {
        return Ok("no account");
    };
    let source = event
        .store
        .clone()
        .unwrap_or_else(|| "unknown".into())
        .to_lowercase();
    let product = event.product_id.clone().unwrap_or_else(|| PRODUCT.into());
    match event.kind.as_str() {
        // Bought, renewed or kept: active until the new expiry. A
        // cancellation stops the renewal, not the year already paid for, and
        // a billing problem keeps it through the store's grace period, which
        // RevenueCat reports as the expiry.
        "INITIAL_PURCHASE"
        | "RENEWAL"
        | "PRODUCT_CHANGE"
        | "UNCANCELLATION"
        | "NON_RENEWING_PURCHASE"
        | "CANCELLATION"
        | "BILLING_ISSUE" => {
            sqlx::query(
                "INSERT INTO entitlements (account_id, active, expires_at, source, product, updated_at) \
                 VALUES ($1, true, $2, $3, $4, now()) \
                 ON CONFLICT (account_id) DO UPDATE SET \
                   active = true, \
                   expires_at = CASE \
                     WHEN EXCLUDED.expires_at IS NULL THEN entitlements.expires_at \
                     WHEN entitlements.expires_at IS NULL THEN EXCLUDED.expires_at \
                     ELSE GREATEST(entitlements.expires_at, EXCLUDED.expires_at) END, \
                   source = EXCLUDED.source, product = EXCLUDED.product, updated_at = now() \
                 WHERE entitlements.source <> 'comp'",
            )
            .bind(account)
            .bind(event.expires_at())
            .bind(&source)
            .bind(&product)
            .execute(&mut **tx)
            .await?;
            Ok("active")
        }
        // Ended. An expiry older than a renewal already recorded arrived late
        // and changes nothing.
        "EXPIRATION" => {
            sqlx::query(
                "UPDATE entitlements SET active = false, updated_at = now() \
                 WHERE account_id = $1 AND source <> 'comp' \
                   AND (expires_at IS NULL OR $2::timestamptz IS NULL OR expires_at <= $2)",
            )
            .bind(account)
            .bind(event.expires_at())
            .execute(&mut **tx)
            .await?;
            Ok("expired")
        }
        _ => Ok("ignored"),
    }
}

/// The purchase moved to another account (a restore signed in as someone
/// else): the first account it came from loses it, the new one gains it.
async fn transfer(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: &Event,
) -> Result<&'static str> {
    let from: Vec<Uuid> = event
        .transferred_from
        .iter()
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect();
    let Some(to) = event
        .transferred_to
        .iter()
        .find_map(|id| Uuid::parse_str(id).ok())
    else {
        return Ok("no account");
    };
    let moved = sqlx::query_as::<_, (Option<DateTime<Utc>>, String, Option<String>)>(
        "SELECT expires_at, source, product FROM entitlements \
         WHERE account_id = ANY($1) AND active AND source <> 'comp' \
         ORDER BY expires_at DESC NULLS FIRST LIMIT 1",
    )
    .bind(&from)
    .fetch_optional(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE entitlements SET active = false, updated_at = now() \
         WHERE account_id = ANY($1) AND source <> 'comp'",
    )
    .bind(&from)
    .execute(&mut **tx)
    .await?;
    let (expires_at, source, product) = moved.unwrap_or((
        event.expires_at(),
        event
            .store
            .clone()
            .unwrap_or_else(|| "unknown".into())
            .to_lowercase(),
        event.product_id.clone(),
    ));
    sqlx::query(
        "INSERT INTO entitlements (account_id, active, expires_at, source, product, updated_at) \
         VALUES ($1, true, $2, $3, $4, now()) \
         ON CONFLICT (account_id) DO UPDATE SET active = true, expires_at = EXCLUDED.expires_at, \
           source = EXCLUDED.source, product = EXCLUDED.product, updated_at = now() \
         WHERE entitlements.source <> 'comp'",
    )
    .bind(to)
    .bind(expires_at)
    .bind(source)
    .bind(product)
    .execute(&mut **tx)
    .await?;
    Ok("transferred")
}

/// Grants an account the subscription by hand (`source = 'comp'`), which no
/// webhook overwrites. For tests and the odd gift; `COMP_ACCOUNTS` does the
/// same without touching the table.
pub async fn grant_comp(pool: &PgPool, account: Uuid) -> Result<()> {
    sqlx::query(
        "INSERT INTO entitlements (account_id, active, expires_at, source, product) \
         VALUES ($1, true, NULL, 'comp', NULL) \
         ON CONFLICT (account_id) DO UPDATE SET active = true, expires_at = NULL, source = 'comp'",
    )
    .bind(account)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comp_accounts_skip_what_isnt_an_id() {
        let id = Uuid::new_v4();
        let parsed = parse_accounts(&format!(" {id}, nonsense ,\n"));
        assert_eq!(parsed, HashSet::from([id]));
    }

    #[test]
    fn the_secret_is_checked_bare_or_as_a_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "s3cret".parse().unwrap());
        assert!(authorised(&headers, "s3cret"));
        headers.insert("authorization", "Bearer s3cret".parse().unwrap());
        assert!(authorised(&headers, "s3cret"));
        headers.insert("authorization", "Bearer s3cres".parse().unwrap());
        assert!(!authorised(&headers, "s3cret"));
        assert!(!authorised(&HeaderMap::new(), "s3cret"));
    }

    #[test]
    fn an_event_finds_its_account_past_anonymous_ids() {
        let id = Uuid::new_v4();
        let event: Event = serde_json::from_value(json!({
            "id": "e", "type": "RENEWAL", "app_user_id": "$RCAnonymousID:abc",
            "aliases": ["$RCAnonymousID:abc", id.to_string()],
        }))
        .unwrap();
        assert_eq!(event.account(), Some(id));
        assert!(event.concerns_sync());
    }
}
