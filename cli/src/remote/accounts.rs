//! Each account's workspace, kept as a device of the account would keep it.
//!
//! The sync server stores rows and knows nothing about tasks, so the tools
//! can't run there. Instead this server joins each account it serves as one
//! more device — "AI assistants" in the account's device list — with a
//! SQLite workspace of its own, made and migrated by the core like any
//! client's, and runs the same sync cycle the Mac does
//! (`desktop/src/sync/mod.rs`): push, then pull, with the core making every
//! body and applying every page. Every tool call runs between a cycle before
//! (so it sees what the phone did a moment ago) and a push after (so the
//! phone hears of it at once, through the server's Realtime broadcast).
//!
//! Requests to the sync server carry the caller's own token, so it decides
//! what this server may read exactly as it would for the user's phone —
//! including the `402` that says the account hasn't got Takt Sync while the
//! sync server charges. This server is a paid feature even while sync is
//! free: with `REQUIRE_SUBSCRIPTION` it asks `GET /v1/account` whether the
//! account has bought Takt Sync or been given it (`entitled`), before the
//! first call and every [`ENTITLEMENT_RECHECK`] after.
//!
//! A workspace lives in the data directory while its account keeps calling
//! and is dropped after a quiet spell; a new instance, or a dropped account,
//! starts again from a fresh database and a full pull, which joins the
//! account the way a phone signing in for the first time does. The device id
//! is derived from the account, so that is always the same device.

use crate::checkvist::{CheckvistClient, CheckvistConfig};
use crate::local::LocalState;
use crate::tools::Tools;
use crate::workspace::Workspace;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use takt_core::sync::wire::{self, SyncPull};
use takt_core::workspace::CoreWorkspace;
use uuid::Uuid;

const DEVICE_HEADER: &str = "X-Priority-Device";
/// What the account's device list calls this server.
pub const DEVICE_NAME: &str = "AI assistants (remote MCP)";
pub const DEVICE_PLATFORM: &str = "mcp";
const PUSH_BATCH: u32 = 100;
const PULL_PAGE: u32 = 1000;
/// How long an account's "has Takt Sync" is believed before asking again:
/// a lapse is noticed within this, a purchase on the first call after it.
pub const ENTITLEMENT_RECHECK: Duration = Duration::from_secs(10 * 60);
/// What an account without Takt Sync is told.
pub const SUBSCRIPTION_NEEDED: &str = "Takt for AI assistants comes with Takt Sync (£1 a year). Subscribe in Takt on your iPhone or Android phone (Settings → Sync), then try again.";

/// The namespace the per-account device id is derived in; fixed for good,
/// since changing it would make every account a new device.
const DEVICE_NAMESPACE: Uuid = Uuid::from_u128(0x6b1f_0c52_8e1d_4a0f_9b7e_2d4c_6a18_f3e5);

/// Why a cycle stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncFail {
    /// The sync server refused the token.
    Unauthorized,
    /// `402`: the account hasn't got Takt Sync; the server's words.
    Subscription(String),
    Other(String),
}

impl SyncFail {
    pub fn message(&self) -> String {
        match self {
            SyncFail::Unauthorized => {
                "The sync server didn't accept this sign-in. Reconnect Takt in your assistant's settings."
                    .into()
            }
            SyncFail::Subscription(text) => text.clone(),
            SyncFail::Other(text) => text.clone(),
        }
    }
}

/// The sync server this one is a client of.
#[derive(Debug, Clone)]
pub struct SyncServer {
    pub url: String,
}

impl SyncServer {
    /// One request with the caller's token, as this account's device.
    fn call(
        &self,
        method: &str,
        path: &str,
        token: &str,
        device: &str,
        body: Option<&str>,
    ) -> Result<String, SyncFail> {
        let url = format!("{}/{path}", self.url.trim_end_matches('/'));
        let request = ureq::request(method, &url)
            .timeout(Duration::from_secs(60))
            .set("Authorization", &format!("Bearer {token}"))
            .set(DEVICE_HEADER, device);
        let sent = match body {
            Some(body) => request
                .set("Content-Type", "application/json")
                .send_string(body),
            None => request.call(),
        };
        match sent {
            Ok(response) => response
                .into_string()
                .map_err(|e| SyncFail::Other(format!("Reading the sync server's answer: {e}"))),
            Err(ureq::Error::Status(401, _)) => Err(SyncFail::Unauthorized),
            Err(ureq::Error::Status(402, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(SyncFail::Subscription(wire::sync_failure_message(
                    402, body,
                )))
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                Err(SyncFail::Other(wire::sync_failure_message(
                    i32::from(status),
                    body,
                )))
            }
            Err(error) => Err(SyncFail::Other(format!(
                "The sync server couldn't be reached: {error}"
            ))),
        }
    }
}

/// `subscription.entitled` from a `GET /v1/account` body.
pub fn entitled(account_body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(account_body)
        .ok()
        .and_then(|body| body.get("subscription")?.get("entitled")?.as_bool())
        .unwrap_or(false)
}

/// The device id this server is, on `account`.
pub fn device_id(account: Uuid) -> Uuid {
    Uuid::new_v5(&DEVICE_NAMESPACE, account.as_bytes())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// One account's workspace.
struct Slot {
    dir: PathBuf,
    core: Option<Arc<CoreWorkspace>>,
    last_used: Instant,
    /// When the account was last found to have Takt Sync.
    entitled_at: Option<Instant>,
}

/// Every account this instance holds a workspace for.
pub struct Accounts {
    root: PathBuf,
    sync: SyncServer,
    idle: Duration,
    /// `REQUIRE_SUBSCRIPTION`: only accounts with Takt Sync are served.
    require_subscription: bool,
    slots: Mutex<HashMap<Uuid, Arc<Mutex<Slot>>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic inside a tool leaves its slot usable: the database is
    // transactional, and the next cycle starts from what it holds.
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Accounts {
    pub fn new(
        root: PathBuf,
        sync: SyncServer,
        idle: Duration,
        require_subscription: bool,
    ) -> Self {
        Accounts {
            root,
            sync,
            idle,
            require_subscription,
            slots: Mutex::default(),
        }
    }

    /// Runs `work` on `account`'s workspace between a full cycle before and
    /// a push after. A failure of the cycle before stops it; one of the push
    /// after is returned beside the work's own answer, which stands (the
    /// write is made, and the next call's cycle sends it).
    pub fn with_workspace<T>(
        &self,
        account: Uuid,
        token: &str,
        work: impl FnOnce(&Tools) -> T,
    ) -> Result<(T, Option<SyncFail>), SyncFail> {
        let slot = {
            let mut slots = lock(&self.slots);
            slots
                .entry(account)
                .or_insert_with(|| {
                    Arc::new(Mutex::new(Slot {
                        dir: self.root.join(account.to_string()),
                        core: None,
                        last_used: Instant::now(),
                        entitled_at: None,
                    }))
                })
                .clone()
        };
        let mut slot = lock(&slot);
        slot.last_used = Instant::now();
        let device = device_id(account).to_string();
        if self.require_subscription
            && slot
                .entitled_at
                .is_none_or(|at| at.elapsed() > ENTITLEMENT_RECHECK)
        {
            self.check_entitled(token, &device)?;
            slot.entitled_at = Some(Instant::now());
        }
        let (core, fresh) = match &slot.core {
            Some(core) => (core.clone(), false),
            None => {
                let core = self.join(&slot.dir, token, &device)?;
                slot.core = Some(core.clone());
                (core, true)
            }
        };
        // Nothing of the account's is read past a failed cycle: a refusal
        // (401, 402) is the sync server's word on this caller, and the
        // workspace may be stale after any other failure.
        self.cycle(&core, token, &device)?;
        if fresh {
            // As every app does at launch, once it has the account's rows: an
            // account no device has made a workspace for yet gets one, and
            // one that has keeps it (and gets its Inbox back if it lost it).
            core.bootstrap(now_ms())
                .map_err(|e| SyncFail::Other(format!("Setting the workspace up: {e}")))?;
        }
        let tools = tools_for(&slot.dir);
        let answer = work(&tools);
        let after = self.push(&core, token, &device).err();
        Ok((answer, after))
    }

    /// Whether the account has bought Takt Sync or been given it, as the
    /// sync server's account record says, whether or not sync itself is
    /// charged for. A server too old to say counts as no.
    fn check_entitled(&self, token: &str, device: &str) -> Result<(), SyncFail> {
        let body = self.sync.call("GET", "v1/account", token, device, None)?;
        if entitled(&body) {
            Ok(())
        } else {
            Err(SyncFail::Subscription(SUBSCRIPTION_NEEDED.into()))
        }
    }

    /// A fresh workspace for the account, paired as its device and
    /// registered on the account.
    fn join(
        &self,
        dir: &std::path::Path,
        token: &str,
        device: &str,
    ) -> Result<Arc<CoreWorkspace>, SyncFail> {
        let failed = |what: &str, error: String| SyncFail::Other(format!("{what}: {error}"));
        // Whatever an earlier process left is stale: the cycle that follows
        // pulls everything, and a half-pushed outbox from a dead instance
        // can't be told apart from one that finished.
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).map_err(|e| failed("Making the workspace", e.to_string()))?;
        let path = dir.join("workspace.sqlite");
        let path_text = path.to_string_lossy().into_owned();
        takt_core::schema::migrate_workspace(path_text.clone())
            .map_err(|e| failed("Making the workspace", e.to_string()))?;
        let core = CoreWorkspace::open(path_text)
            .map_err(|e| failed("Opening the workspace", e.to_string()))?;
        let body = wire::sync_register_device_body(
            device.to_owned(),
            DEVICE_NAME.into(),
            DEVICE_PLATFORM.into(),
        );
        self.sync
            .call("POST", "v1/devices", token, device, Some(&body))?;
        core.begin_sync(device.to_owned(), self.sync.url.clone())
            .map_err(|e| failed("Pairing the workspace", e.to_string()))?;
        Ok(core)
    }

    /// Push then pull; the first after pairing snapshots and pulls first, so
    /// the workspace adopts the account's (`docs/sync.md`, "A client sync
    /// cycle").
    fn cycle(&self, core: &CoreWorkspace, token: &str, device: &str) -> Result<(), SyncFail> {
        let state = core
            .sync_state()
            .map_err(|e| SyncFail::Other(e.to_string()))?
            .ok_or_else(|| SyncFail::Other("The workspace isn't paired.".into()))?;
        if state.needs_snapshot {
            core.enqueue_sync_snapshot(now_ms())
                .map_err(|e| SyncFail::Other(e.to_string()))?;
            self.pull(core, token, device, state.cursor)?;
            self.push(core, token, device)
        } else {
            self.push(core, token, device)?;
            let cursor = core
                .sync_state()
                .ok()
                .flatten()
                .map_or(0, |state| state.cursor);
            self.pull(core, token, device, cursor)
        }
    }

    fn push(&self, core: &CoreWorkspace, token: &str, device: &str) -> Result<(), SyncFail> {
        let core_fail = |e: takt_core::CoreError| SyncFail::Other(e.to_string());
        while let Some(batch) = core
            .prepare_sync_push(PUSH_BATCH, now_ms())
            .map_err(core_fail)?
        {
            self.sync
                .call("POST", "v1/push", token, device, Some(&batch.body))?;
            core.finish_sync_push(batch.through_seq, batch.hlc, now_ms())
                .map_err(core_fail)?;
        }
        Ok(())
    }

    fn pull(
        &self,
        core: &CoreWorkspace,
        token: &str,
        device: &str,
        cursor: i64,
    ) -> Result<(), SyncFail> {
        let pull = Arc::new(SyncPull::new());
        let mut next = cursor;
        loop {
            let path = format!("v1/changes?since={next}&limit={PULL_PAGE}&wait=0");
            let body = self.sync.call("GET", &path, token, device, None)?;
            let page = pull
                .add_page(body)
                .map_err(|e| SyncFail::Other(e.to_string()))?;
            next = page.cursor;
            if !page.has_more {
                break;
            }
        }
        core.apply_sync_pull(pull, now_ms(), now_ms())
            .map_err(|e| SyncFail::Other(e.to_string()))?;
        Ok(())
    }

    /// Drops the workspaces of accounts quiet for longer than the idle
    /// spell, and their files: the data directory is memory on Cloud Run.
    /// One in use is skipped.
    pub fn evict_idle(&self) -> usize {
        let mut slots = lock(&self.slots);
        let quiet: Vec<Uuid> = slots
            .iter()
            .filter(|(_, slot)| {
                // A workspace whose last push failed keeps its outbox until
                // a call sends it; dropping it would drop the change.
                slot.try_lock().is_ok_and(|slot| {
                    slot.last_used.elapsed() > self.idle
                        && slot.core.as_ref().is_none_or(|core| {
                            core.latest_sync_outbox_seq().is_ok_and(|seq| seq.is_none())
                        })
                })
            })
            .map(|(account, _)| *account)
            .collect();
        for account in &quiet {
            if let Some(slot) = slots.remove(account) {
                let dir = lock(&slot).dir.clone();
                let _ = std::fs::remove_dir_all(dir);
            }
        }
        quiet.len()
    }

    /// How many accounts have a workspace here now.
    pub fn held(&self) -> usize {
        lock(&self.slots).len()
    }
}

/// The tool table over the workspace in `dir`. Nothing here is Checkvist's
/// or the Mac's own files: the client has no key and the local state points
/// at files that don't exist, and the tools that need either are not
/// offered (`host.rs`).
fn tools_for(dir: &std::path::Path) -> Tools {
    Tools {
        client: CheckvistClient::new(CheckvistConfig {
            username: String::new(),
            remote_key: String::new(),
            default_list_id: String::new(),
            base_url: String::new(),
        }),
        local: LocalState {
            store_directory: dir.join("no-local-store"),
            prefs_path: dir.join("no-prefs.plist"),
        },
        workspace: Workspace {
            database_path: dir.join("workspace.sqlite"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_is_always_the_same_device_and_no_other_accounts() {
        let one = Uuid::from_u128(1);
        let two = Uuid::from_u128(2);
        assert_eq!(device_id(one), device_id(one));
        assert_ne!(device_id(one), device_id(two));
    }

    #[test]
    fn only_a_bought_or_given_takt_sync_is_entitled() {
        let account = |subscription: &str| {
            format!(r#"{{"accountId":"a","devices":[],"subscription":{subscription}}}"#)
        };
        assert!(entitled(&account(
            r#"{"required":false,"active":true,"entitled":true,"comp":false}"#
        )));
        // Sync free for everyone says nothing about having paid.
        assert!(!entitled(&account(
            r#"{"required":false,"active":true,"entitled":false,"comp":false}"#
        )));
        // A sync server from before `entitled`, or no subscription at all.
        assert!(!entitled(&account(r#"{"required":false,"active":true}"#)));
        assert!(!entitled(r#"{"accountId":"a"}"#));
        assert!(!entitled("not json"));
    }
}
