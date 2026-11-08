//! Shared lists (docs/shared-lists-plan.md): the spaces an account is in,
//! inviting people to one, joining, leaving and stopping sharing.
//!
//! A space is a set of rows with its own members. Every account has a
//! personal space whose id is the account's, which needs no row here; a
//! shared list is a space of its own, owned by the account that shared it.
//! The server still knows nothing about tasks: it only checks that a row
//! pushed to a shared space is one a shared list may hold, and that the
//! pusher is a member.
//!
//! Every push, and every change of membership that moves rows into someone's
//! feed, holds the one push lock (`push::lock`). That keeps `seq` in commit
//! order across every space, so a device's single cursor stays right however
//! many spaces it reads.

use crate::AppState;
use crate::auth::Caller;
use crate::error::{AppError, Result};
use crate::{push, realtime};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Executor, Postgres};
use std::collections::{BTreeSet, HashMap};
use takt_sync_rules::wire::{WireMember, WireSpace};
use uuid::Uuid;

/// The tables a shared list's rows come from. Everything else an account
/// syncs (conditions, habits, focus, themes, preferences) stays personal.
pub const SHARED_TABLES: [&str; 5] = [
    "task_lists",
    "tasks",
    "task_metadata",
    "task_dependencies",
    "agent_instructions",
];

/// How long an invite stays usable.
const INVITE_DAYS: i64 = 7;

/// How many unused, unexpired invites one space may have at once.
const MAX_OPEN_INVITES: i64 = 20;

/// A space the caller is in, as the push needs it.
#[derive(Debug, Clone)]
pub struct Membership {
    pub list_id: String,
    pub role: String,
}

/// The shared spaces `account` is in, by id.
pub async fn memberships<'e, E>(executor: E, account: Uuid) -> Result<HashMap<Uuid, Membership>>
where
    E: Executor<'e, Database = Postgres>,
{
    let rows = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT s.id, s.list_id, m.role FROM space_members m JOIN spaces s ON s.id = m.space_id \
         WHERE m.account_id = $1",
    )
    .bind(account)
    .fetch_all(executor)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, list_id, role)| (id, Membership { list_id, role }))
        .collect())
}

/// Every space `account` reads: its own and the shared ones it is in.
pub async fn readable<'e, E>(executor: E, account: Uuid) -> Result<Vec<Uuid>>
where
    E: Executor<'e, Database = Postgres>,
{
    let mut spaces: Vec<Uuid> = sqlx::query_scalar(
        "SELECT space_id FROM space_members WHERE account_id = $1 ORDER BY space_id",
    )
    .bind(account)
    .fetch_all(executor)
    .await?;
    spaces.insert(0, account);
    Ok(spaces)
}

/// Every shared space `account` is in, with its members, as a page of
/// changes carries them.
pub async fn wire_spaces<'e, E>(executor: E, account: Uuid) -> Result<Vec<WireSpace>>
where
    E: Executor<'e, Database = Postgres>,
{
    type Tuple = (Uuid, String, String, String, Uuid, Option<String>, String);
    let rows = sqlx::query_as::<_, Tuple>(
        "SELECT s.id, s.list_id, s.name, me.role, m.account_id, m.email, m.role \
         FROM space_members me \
         JOIN spaces s ON s.id = me.space_id \
         JOIN space_members m ON m.space_id = s.id \
         WHERE me.account_id = $1 \
         ORDER BY s.created_at, s.id, (m.role = 'owner') DESC, m.joined_at, m.account_id",
    )
    .bind(account)
    .fetch_all(executor)
    .await?;
    let mut spaces: Vec<WireSpace> = Vec::new();
    for (id, list_id, name, role, member, email, member_role) in rows {
        let id = id.to_string();
        if spaces.last().is_none_or(|space| space.id != id) {
            spaces.push(WireSpace {
                id,
                list_id,
                name,
                role,
                members: Vec::new(),
            });
        }
        let space = spaces.last_mut().expect("just pushed");
        space.members.push(WireMember {
            account_id: member.to_string(),
            email,
            role: member_role,
            you: member == account,
        });
    }
    Ok(spaces)
}

/// The accounts in `space`, owner first.
async fn members_of<'e, E>(executor: E, space: Uuid) -> Result<Vec<Uuid>>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar(
        "SELECT account_id FROM space_members WHERE space_id = $1 \
         ORDER BY (role = 'owner') DESC, joined_at",
    )
    .bind(space)
    .fetch_all(executor)
    .await?)
}

/// Every account to nudge when rows in `spaces` change: the members of each
/// shared one, and the owner of each personal one (whose id is the account's).
pub async fn audience(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    spaces: &BTreeSet<Uuid>,
) -> Result<BTreeSet<Uuid>> {
    let mut accounts = BTreeSet::new();
    for space in spaces {
        let members = members_of(&mut **tx, *space).await?;
        if members.is_empty() {
            accounts.insert(*space);
        } else {
            accounts.extend(members);
        }
    }
    Ok(accounts)
}

fn forbidden(what: &str) -> AppError {
    AppError::Forbidden(what.to_owned())
}

async fn space_for_owner(
    executor: impl Executor<'_, Database = Postgres>,
    space: Uuid,
    account: Uuid,
) -> Result<()> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM space_members WHERE space_id = $1 AND account_id = $2",
    )
    .bind(space)
    .bind(account)
    .fetch_optional(executor)
    .await?;
    match role.as_deref() {
        Some("owner") => Ok(()),
        Some(_) => Err(forbidden("only the list's owner can do that")),
        None => Err(AppError::NotFound("no such shared list".into())),
    }
}

async fn one_space(
    executor: impl Executor<'_, Database = Postgres>,
    account: Uuid,
    space: Uuid,
) -> Result<WireSpace> {
    wire_spaces(executor, account)
        .await?
        .into_iter()
        .find(|candidate| candidate.id == space.to_string())
        .ok_or_else(|| AppError::NotFound("no such shared list".into()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareRequest {
    pub list_id: String,
    #[serde(default)]
    pub name: String,
}

/// `POST /v1/spaces` `{ "listId", "name" }`: shares one of the caller's
/// lists. Sharing a list that is already shared answers the space it has, so
/// a device that lost the first answer can ask again.
pub async fn share(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Json(request): Json<ShareRequest>,
) -> Result<Json<WireSpace>> {
    let list_id = request.list_id.trim();
    if list_id.is_empty() || list_id.len() > 256 {
        return Err(AppError::BadRequest("bad list id".into()));
    }
    let name: String = request.name.trim().chars().take(200).collect();
    let mut tx = state.pool.begin().await?;
    // A member of a shared list can't share it again as their own.
    let shared_elsewhere: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM spaces s JOIN space_members m ON m.space_id = s.id \
         WHERE s.list_id = $1 AND m.account_id = $2 AND m.role = 'member')",
    )
    .bind(list_id)
    .bind(caller.account)
    .fetch_one(&mut *tx)
    .await?;
    if shared_elsewhere {
        return Err(forbidden("only the list's owner can share it"));
    }
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM spaces WHERE owner_account_id = $1 AND list_id = $2")
            .bind(caller.account)
            .bind(list_id)
            .fetch_optional(&mut *tx)
            .await?;
    let space = match existing {
        Some(space) => space,
        None => {
            let space = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO spaces (id, owner_account_id, list_id, name) VALUES ($1, $2, $3, $4)",
            )
            .bind(space)
            .bind(caller.account)
            .bind(list_id)
            .bind(&name)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO space_members (space_id, account_id, role, email) \
                 VALUES ($1, $2, 'owner', $3)",
            )
            .bind(space)
            .bind(caller.account)
            .bind(&caller.email)
            .execute(&mut *tx)
            .await?;
            space
        }
    };
    let answer = one_space(&mut *tx, caller.account, space).await?;
    tx.commit().await?;
    tracing::info!(account = %caller.account, %space, "shared a list");
    Ok(Json(answer))
}

/// `GET /v1/spaces`: the shared lists the caller is in, with their members.
pub async fn list(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
) -> Result<Json<Value>> {
    let spaces = wire_spaces(&state.pool, caller.account).await?;
    Ok(Json(json!({ "spaces": spaces })))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invite {
    pub code: String,
    pub space_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

/// `POST /v1/spaces/<id>/invites`: a single-use code that lets one person
/// join, for a week. Only the owner invites.
pub async fn invite(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(space): Path<Uuid>,
) -> Result<Json<Invite>> {
    let mut tx = state.pool.begin().await?;
    space_for_owner(&mut *tx, space, caller.account).await?;
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM space_invites WHERE space_id = $1 AND used_by IS NULL \
         AND expires_at > now()",
    )
    .bind(space)
    .fetch_one(&mut *tx)
    .await?;
    if open >= MAX_OPEN_INVITES {
        return Err(AppError::TooMany(format!(
            "this list already has {MAX_OPEN_INVITES} invites waiting; wait for some to be used \
             or to expire"
        )));
    }
    let code = invite_code();
    let expires_at = Utc::now() + Duration::days(INVITE_DAYS);
    sqlx::query(
        "INSERT INTO space_invites (code, space_id, created_by, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(&code)
    .bind(space)
    .bind(caller.account)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(Invite {
        code,
        space_id: space,
        expires_at,
    }))
}

/// 128 random bits, in Crockford's base32 without padding: 26 characters
/// that survive being read aloud or retyped, case-insensitively.
fn invite_code() -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let bits = u128::from_be_bytes(*Uuid::new_v4().as_bytes())
        ^ (u128::from_be_bytes(*Uuid::new_v4().as_bytes()) >> 6);
    (0..26)
        .map(|index| ALPHABET[((bits >> (index * 5)) & 31) as usize] as char)
        .collect()
}

/// Reads a code as a person might type it: any case, with spaces or dashes,
/// and the letters Crockford's alphabet leaves out read as the digits they
/// look like.
fn normalise_code(code: &str) -> String {
    code.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitePreview {
    pub space_id: Uuid,
    pub name: String,
    pub owner_email: Option<String>,
    pub expires_at: DateTime<Utc>,
    /// The caller is in the list already.
    pub joined: bool,
}

type InviteTuple = (Uuid, DateTime<Utc>, Option<Uuid>, String, Option<String>);

async fn find_invite(
    executor: impl Executor<'_, Database = Postgres>,
    code: &str,
) -> Result<InviteTuple> {
    let code = normalise_code(code);
    let found = sqlx::query_as::<_, InviteTuple>(
        "SELECT i.space_id, i.expires_at, i.used_by, s.name, o.email FROM space_invites i \
         JOIN spaces s ON s.id = i.space_id \
         LEFT JOIN space_members o ON o.space_id = s.id AND o.role = 'owner' \
         WHERE i.code = $1",
    )
    .bind(&code)
    .fetch_optional(executor)
    .await?;
    found.ok_or_else(|| AppError::NotFound("that invite isn't one this server made".into()))
}

/// `GET /v1/invites/<code>`: what an invite is for, so the app can ask
/// before joining.
pub async fn preview(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(code): Path<String>,
) -> Result<Json<InvitePreview>> {
    let (space, expires_at, _, name, owner_email) = find_invite(&state.pool, &code).await?;
    let joined = memberships(&state.pool, caller.account)
        .await?
        .contains_key(&space);
    Ok(Json(InvitePreview {
        space_id: space,
        name,
        owner_email,
        expires_at,
        joined,
    }))
}

/// `POST /v1/invites/<code>/accept`: joins the list the invite is for.
///
/// Every row already in the list is renumbered, under the push lock, so it
/// comes after the cursor of each of the joiner's devices and they pull it.
/// The members already in it pull the same rows again, which changes
/// nothing for them.
pub async fn accept(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(code): Path<String>,
) -> Result<Json<WireSpace>> {
    let mut tx = state.pool.begin().await?;
    push::lock(&mut tx).await?;
    let (space, expires_at, used_by, _, _) = find_invite(&mut *tx, &code).await?;
    let already = memberships(&mut *tx, caller.account)
        .await?
        .contains_key(&space);
    if !already {
        if used_by.is_some() {
            return Err(AppError::Gone(
                "that invite has been used; ask for a new one".into(),
            ));
        }
        if expires_at <= Utc::now() {
            return Err(AppError::Gone(
                "that invite has expired; ask for a new one".into(),
            ));
        }
        sqlx::query("UPDATE space_invites SET used_by = $1, used_at = now() WHERE code = $2")
            .bind(caller.account)
            .bind(normalise_code(&code))
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO space_members (space_id, account_id, role, email) \
             VALUES ($1, $2, 'member', $3)",
        )
        .bind(space)
        .bind(caller.account)
        .bind(&caller.email)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE rows SET seq = nextval('row_seq') WHERE space_id = $1")
            .bind(space)
            .execute(&mut *tx)
            .await?;
        let audience = audience(&mut tx, &BTreeSet::from([space])).await?;
        realtime::nudge(&mut tx, state.realtime, &audience).await?;
    }
    let answer = one_space(&mut *tx, caller.account, space).await?;
    tx.commit().await?;
    tracing::info!(account = %caller.account, %space, "joined a shared list");
    Ok(Json(answer))
}

/// `DELETE /v1/spaces/<id>/members/<account>`: the owner removes someone, or
/// a member leaves. The owner can't leave their own list; they stop sharing
/// it instead.
pub async fn remove_member(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path((space, member)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    push::lock(&mut tx).await?;
    let roles: HashMap<Uuid, String> = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT account_id, role FROM space_members WHERE space_id = $1",
    )
    .bind(space)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let Some(caller_role) = roles.get(&caller.account) else {
        return Err(AppError::NotFound("no such shared list".into()));
    };
    let Some(member_role) = roles.get(&member) else {
        return Err(AppError::NotFound("they aren't in this list".into()));
    };
    if member_role == "owner" {
        return Err(forbidden(
            "the owner can't leave their own list; stop sharing it instead",
        ));
    }
    if member != caller.account && caller_role != "owner" {
        return Err(forbidden("only the list's owner can remove people"));
    }
    let audience = audience(&mut tx, &BTreeSet::from([space])).await?;
    sqlx::query("DELETE FROM space_members WHERE space_id = $1 AND account_id = $2")
        .bind(space)
        .bind(member)
        .execute(&mut *tx)
        .await?;
    realtime::nudge(&mut tx, state.realtime, &audience).await?;
    tx.commit().await?;
    tracing::info!(account = %caller.account, %space, %member, "removed a member");
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /v1/spaces/<id>`: the owner stops sharing. The members lose the
/// list; the owner's devices keep it, as a private list again, and push it
/// back into the owner's own space.
pub async fn stop_sharing(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Path(space): Path<Uuid>,
) -> Result<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    push::lock(&mut tx).await?;
    space_for_owner(&mut *tx, space, caller.account).await?;
    let audience = audience(&mut tx, &BTreeSet::from([space])).await?;
    delete_space(&mut tx, space).await?;
    realtime::nudge(&mut tx, state.realtime, &audience).await?;
    tx.commit().await?;
    tracing::info!(account = %caller.account, %space, "stopped sharing a list");
    Ok(Json(json!({ "ok": true })))
}

async fn delete_space(tx: &mut sqlx::Transaction<'_, Postgres>, space: Uuid) -> Result<()> {
    sqlx::query("DELETE FROM rows WHERE space_id = $1")
        .bind(space)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM spaces WHERE id = $1")
        .bind(space)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// What deleting an account does to the lists it shares or is in: each list
/// it owns passes to the member who has been in it longest, or goes when
/// nobody else is in it, and it leaves every other list. Run inside the
/// account deletion's transaction.
pub async fn leave_all(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    realtime_on: bool,
    account: Uuid,
) -> Result<()> {
    push::lock(tx).await?;
    let spaces = memberships(&mut **tx, account).await?;
    let mut nudged = BTreeSet::new();
    for (space, membership) in spaces {
        let others: Vec<Uuid> = members_of(&mut **tx, space)
            .await?
            .into_iter()
            .filter(|member| *member != account)
            .collect();
        nudged.extend(others.iter().copied());
        if membership.role == "owner" {
            let heir: Option<Uuid> = sqlx::query_scalar(
                "SELECT account_id FROM space_members WHERE space_id = $1 AND account_id <> $2 \
                 ORDER BY joined_at, account_id LIMIT 1",
            )
            .bind(space)
            .bind(account)
            .fetch_optional(&mut **tx)
            .await?;
            match heir {
                Some(heir) => {
                    sqlx::query("UPDATE spaces SET owner_account_id = $1 WHERE id = $2")
                        .bind(heir)
                        .bind(space)
                        .execute(&mut **tx)
                        .await?;
                    sqlx::query(
                        "UPDATE space_members SET role = 'owner' \
                         WHERE space_id = $1 AND account_id = $2",
                    )
                    .bind(space)
                    .bind(heir)
                    .execute(&mut **tx)
                    .await?;
                }
                None => {
                    delete_space(tx, space).await?;
                    continue;
                }
            }
        }
        sqlx::query("DELETE FROM space_members WHERE space_id = $1 AND account_id = $2")
            .bind(space)
            .bind(account)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("DELETE FROM space_invites WHERE created_by = $1 AND used_by IS NULL")
        .bind(account)
        .execute(&mut **tx)
        .await?;
    realtime::nudge(tx, realtime_on, &nudged).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_is_26_characters_of_crockford_base32() {
        let code = invite_code();
        assert_eq!(code.len(), 26);
        assert!(
            code.chars()
                .all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c))
        );
        assert_ne!(code, invite_code());
    }

    #[test]
    fn a_retyped_code_reads_back() {
        assert_eq!(normalise_code("abcd-efgh ijkl o"), "ABCDEFGH1JK10");
    }
}
