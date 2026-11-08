//! Telling an account's devices there is news, through Supabase Realtime.
//!
//! Cloud Run bills for every second a request is open, so a device that held
//! a long-poll open all day (`/v1/changes?wait=25`) cost money while nothing
//! happened. Instead every push that writes a row sends a broadcast on the
//! account's private Realtime channel, [`topic`], and the apps hold a
//! websocket to Supabase rather than a request to this server. The message
//! only says "pull"; the rows still come from `/v1/changes`.
//!
//! The broadcast goes out through `realtime.send`, inside the push's own
//! transaction: Realtime reads it from the write-ahead log, so it is sent
//! only if the push commits, and only after it has.
//!
//! A server on plain Postgres (self-hosting, the tests) has no `realtime`
//! schema. Then [`prepare`] answers false, `/health` says so, and the apps
//! keep long-polling. They do the same when the channel's access policy is
//! missing, because a device that cannot join would otherwise never hear.

use sqlx::PgPool;

/// What the access policy on `realtime.messages` is called, so it is made
/// once and found again.
pub const POLICY: &str = "takt sync: an account hears its own nudges";

/// The event every broadcast carries.
pub const EVENT: &str = "changed";

/// The account's channel. A device joins it as `realtime:sync:<account>`.
pub fn topic(account: uuid::Uuid) -> String {
    format!("sync:{account}")
}

const SEND: &str = "realtime.send(jsonb,text,text,boolean)";

/// Readable only by a signed-in user, and only their own account's channel.
const CREATE_POLICY: &str = r#"CREATE POLICY "takt sync: an account hears its own nudges"
  ON realtime.messages FOR SELECT TO authenticated
  USING (
    realtime.messages.extension = 'broadcast'
    AND (SELECT realtime.topic()) = 'sync:' || (SELECT auth.uid())::text
  )"#;

/// Makes the channel usable if this database can carry it, and says whether
/// it can. Never fails the boot: without Realtime the apps long-poll.
pub async fn prepare(pool: &PgPool) -> bool {
    match try_prepare(pool).await {
        Ok(ready) => ready,
        Err(error) => {
            tracing::warn!(%error, "realtime nudges are off");
            false
        }
    }
}

async fn try_prepare(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let sendable: bool = sqlx::query_scalar(
        "SELECT CASE WHEN to_regprocedure($1) IS NULL THEN false \
         ELSE has_function_privilege(to_regprocedure($1), 'EXECUTE') END",
    )
    .bind(SEND)
    .fetch_one(pool)
    .await?;
    if !sendable {
        tracing::info!("no realtime.send here; the apps will long-poll");
        return Ok(false);
    }
    if !has_policy(pool).await? {
        if let Err(error) = sqlx::query(CREATE_POLICY).execute(pool).await {
            tracing::warn!(
                %error,
                "could not create the realtime access policy; run its SQL \
                 (sync-server/src/realtime.rs, CREATE_POLICY) in the Supabase \
                 SQL editor, and until then the apps long-poll"
            );
            return Ok(false);
        }
        tracing::info!("created the realtime access policy");
    }
    has_policy(pool).await
}

async fn has_policy(pool: &PgPool) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_policies \
         WHERE schemaname = 'realtime' AND tablename = 'messages' AND policyname = $1)",
    )
    .bind(POLICY)
    .fetch_one(pool)
    .await
}
