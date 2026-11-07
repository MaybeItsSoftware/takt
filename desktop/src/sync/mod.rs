//! Sync with the account's other devices (`docs/sync.md`): the client half
//! of the cycle, which the Swift app ran in `Sources/TaktSync`. The core
//! makes every body and applies every page (`takt_core::sync::wire`); this
//! moves the bytes, holds the sign-in and decides when.
//!
//! Two threads while signed in. One runs cycles, one at a time: at once,
//! two seconds after a local write goes quiet, when the other says the
//! server has news, and every five minutes regardless. The other holds a
//! long-poll open while a window is up (`/v1/changes?wait=25&limit=1`),
//! which only *notices* changes: it never applies them, so it never races
//! a cycle, and nothing has to be cancelled to push a local edit.
//!
//! The window learns of a sync through [`crate::platform::Event::Sync`].

pub mod auth;
pub mod http;
pub mod keychain;

use auth::{AuthError, Session, Supabase};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use takt_core::sync::wire::{self, SyncAccountBody, SyncPull};
use takt_core::workspace::CoreWorkspace;

/// The hosted server, and the Supabase project whose accounts it trusts.
/// The publishable key is meant to ship in apps.
pub const HOSTED_SERVER: &str = "https://api.takt.maybeitssoftware.co.uk";
const LEGACY_SERVERS: [&str; 2] = [
    "https://takt-sync.up.railway.app",
    "https://priority-sync.up.railway.app",
];
const SUPABASE_URL: &str = "https://rsckzmldfpfjdrvulwke.supabase.co";
const SUPABASE_KEY: &str = "sb_publishable_htC171zOquUGx7bBi8MrIQ_ZuIdqL5U";

const DEVICE_HEADER: &str = "X-Priority-Device";
const PUSH_BATCH: u32 = 100;
const PULL_PAGE: u32 = 1000;
const QUIET: Duration = Duration::from_secs(2);
const EVERY: Duration = Duration::from_secs(300);

/// Who this device is signed in as, and where. Kept in the keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub server_url: String,
    pub device_id: String,
    pub account_id: Option<String>,
    pub email: Option<String>,
    /// Supabase refused to refresh the session. Kept, so signing in again
    /// offers the same email.
    #[serde(default)]
    pub signed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Unpaired,
    NeedsSignIn,
    Working(String),
    Syncing,
    /// Synced, when, in ms.
    Idle(Option<i64>),
    Failed(String),
}

/// What the settings rows read.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub phase: Phase,
    pub email: Option<String>,
    pub server: String,
    pub account: Option<SyncAccountBody>,
    /// After a password-reset link: the next thing is a new password.
    pub needs_new_password: bool,
}

enum Wake {
    Now,
    Local,
    Remote,
}

enum Fail {
    Unauthorized,
    Other(String),
}

impl Fail {
    fn message(&self) -> String {
        match self {
            Fail::Unauthorized => "Signed out. Sign in again to keep syncing.".into(),
            Fail::Other(text) => text.clone(),
        }
    }
}

struct Inner {
    core: Arc<CoreWorkspace>,
    status: Mutex<Status>,
    credentials: Mutex<Option<Credentials>>,
    session: Mutex<Option<Session>>,
    /// The PKCE verifier waiting for the browser, and whether it is for a
    /// password reset.
    pending: Mutex<Option<(String, bool)>>,
    foreground: AtomicBool,
    /// Bumped to stop the threads of a run.
    run: AtomicU64,
    wake: Mutex<Option<Sender<Wake>>>,
}

pub struct Sync {
    inner: Arc<Inner>,
    last_seq: Option<i64>,
}

/// What to say in the status bar once, taken by the window.
static NOTICE: Mutex<Option<String>> = Mutex::new(None);

/// Says `text` in the status bar from any thread; `workspace` when the data
/// changed under the window.
pub fn announce(text: &str, workspace: bool) {
    *lock(&NOTICE) = Some(text.to_string());
    crate::platform::push(crate::platform::Event::Sync { workspace });
}

fn now_ms() -> i64 {
    crate::state::store::now_ms()
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn supabase() -> Supabase {
    Supabase {
        url: SUPABASE_URL.into(),
        key: SUPABASE_KEY.into(),
    }
}

/// The hosted server's old addresses read as its new one.
fn current_server(url: &str) -> String {
    if LEGACY_SERVERS.contains(&url.trim_end_matches('/')) {
        HOSTED_SERVER.into()
    } else {
        url.trim_end_matches('/').into()
    }
}

fn device_name() -> String {
    std::process::Command::new("/usr/sbin/scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Mac".into())
}

impl Sync {
    /// Reads the keychain and, when signed in, starts syncing.
    pub fn new(core: Arc<CoreWorkspace>) -> Self {
        let credentials: Option<Credentials> = keychain::load("credentials");
        let session: Option<Session> = keychain::load("session");
        let server = credentials
            .as_ref()
            .map(|c| current_server(&c.server_url))
            .or_else(|| keychain::load::<String>("server"))
            .unwrap_or_else(|| HOSTED_SERVER.into());
        let signed_in = credentials
            .as_ref()
            .is_some_and(|c| !c.signed_out && c.account_id.is_some())
            && session.is_some();
        let phase = match &credentials {
            Some(_) if signed_in => Phase::Idle(None),
            Some(_) => Phase::NeedsSignIn,
            None => Phase::Unpaired,
        };
        let status = Status {
            phase,
            email: credentials.as_ref().and_then(|c| c.email.clone()),
            server,
            ..Default::default()
        };
        let inner = Arc::new(Inner {
            core,
            status: Mutex::new(status),
            credentials: Mutex::new(credentials.filter(|_| signed_in)),
            session: Mutex::new(session.filter(|_| signed_in)),
            pending: Mutex::new(None),
            foreground: AtomicBool::new(true),
            run: AtomicU64::new(0),
            wake: Mutex::new(None),
        });
        let sync = Sync {
            inner,
            last_seq: None,
        };
        if signed_in && std::env::var_os("TAKT_DESKTOP_SMOKE").is_none() {
            sync.inner.start();
        }
        sync
    }

    pub fn status(&self) -> Status {
        lock(&self.inner.status).clone()
    }

    pub fn signed_in(&self) -> bool {
        lock(&self.inner.credentials).is_some()
    }

    /// A message for the status bar, once.
    pub fn take_notice(&self) -> Option<String> {
        lock(&NOTICE).take()
    }

    /// Whether a window is up: the long-poll runs only then.
    pub fn set_foreground(&self, foreground: bool) {
        self.inner.foreground.store(foreground, Ordering::Relaxed);
    }

    /// Called every second: a write anyone made (this app, the CLI) lands
    /// in the outbox, and a cycle follows once it goes quiet.
    pub fn note_outbox(&mut self) {
        if !self.signed_in() {
            return;
        }
        let seq = self.inner.core.latest_sync_outbox_seq().ok().flatten();
        if seq.is_some() && seq != self.last_seq {
            self.last_seq = seq;
            self.inner.wake(Wake::Local);
        }
    }

    pub fn sync_now(&self) {
        self.inner.wake(Wake::Now);
    }

    /// Email and password; `create` makes the account first.
    pub fn sign_in(&self, email: &str, password: &str, create: bool) {
        let email = email.trim().to_string();
        let password = password.to_string();
        if email.is_empty() || password.is_empty() {
            self.inner.say("Enter your email address and password.");
            return;
        }
        let inner = self.inner.clone();
        inner.working(if create {
            "Creating the account…"
        } else {
            "Signing in…"
        });
        std::thread::spawn(move || {
            let answer = if create {
                match supabase().sign_up(&email, &password) {
                    Ok(Some(session)) => Ok(session),
                    Ok(None) => {
                        inner.settle_unpaired();
                        inner.say(&format!(
                            "Check {email} for a link to confirm the address, then sign in."
                        ));
                        return;
                    }
                    Err(error) => Err(error),
                }
            } else {
                supabase().sign_in(&email, &password)
            };
            match answer {
                Ok(session) => inner.finish_sign_in(session),
                Err(error) => {
                    inner.settle_unpaired();
                    inner.say(&error.message());
                }
            }
        });
    }

    /// Google or Apple, in the browser.
    pub fn sign_in_with(&self, provider: &str) {
        let (verifier, challenge) = auth::pkce();
        *lock(&self.inner.pending) = Some((verifier, false));
        let url = supabase().authorize_url(provider, &challenge);
        let _ = std::process::Command::new("/usr/bin/open").arg(url).spawn();
        self.inner.working("Finish signing in in the browser…");
    }

    /// Mails a link to set a new password.
    pub fn reset_password(&self, email: &str) {
        let email = email.trim().to_string();
        let (verifier, challenge) = auth::pkce();
        *lock(&self.inner.pending) = Some((verifier, true));
        let inner = self.inner.clone();
        std::thread::spawn(move || match supabase().recover(&email, &challenge) {
            Ok(()) => inner.say(&format!(
                "Sent a link to {email}. Open it on this Mac to set a new password."
            )),
            Err(error) => inner.say(&error.message()),
        });
    }

    /// `takt://auth-callback?code=…`, back from the browser or an email.
    pub fn callback(&self, url: &str) {
        if let Some(problem) = http::parameter(url, "error_description") {
            self.inner.settle_unpaired();
            self.inner.say(&problem);
            return;
        }
        let Some(code) = http::parameter(url, "code") else {
            return;
        };
        let Some((verifier, recovery)) = lock(&self.inner.pending).take() else {
            self.inner
                .say("That link was for an earlier sign-in. Start again from Settings.");
            return;
        };
        let inner = self.inner.clone();
        std::thread::spawn(move || match supabase().exchange(&code, &verifier) {
            Ok(session) => {
                if recovery {
                    lock(&inner.status).needs_new_password = true;
                    inner.say("Choose a new password in Settings → Sync.");
                }
                if lock(&inner.credentials).is_some() {
                    *lock(&inner.session) = Some(session.clone());
                    let _ = keychain::save("session", &session);
                    inner.changed(false);
                } else {
                    inner.finish_sign_in(session);
                }
            }
            Err(error) => {
                inner.settle_unpaired();
                inner.say(&error.message());
            }
        });
    }

    pub fn set_new_password(&self, password: &str) {
        let password = password.to_string();
        let inner = self.inner.clone();
        std::thread::spawn(move || {
            let token = match inner.token() {
                Ok(token) => token,
                Err(fail) => return inner.say(&fail.message()),
            };
            match supabase().update_password(&token, &password) {
                Ok(()) => {
                    lock(&inner.status).needs_new_password = false;
                    inner.say("Password changed.");
                }
                Err(error) => inner.say(&error.message()),
            }
        });
    }

    /// Who this device is signed in as, and the account's devices.
    pub fn refresh_account(&self) {
        let inner = self.inner.clone();
        std::thread::spawn(move || {
            match inner.call("GET", "v1/account", None, Duration::from_secs(30)) {
                Ok(body) => match wire::sync_decode_account(body) {
                    Ok(account) => {
                        lock(&inner.status).account = Some(account);
                        inner.changed(false);
                    }
                    Err(error) => inner.say(&error.to_string()),
                },
                Err(fail) => inner.say(&fail.message()),
            }
        });
    }

    /// Leaves sync on this device, telling the server and Supabase.
    pub fn sign_out(&self) {
        let inner = self.inner.clone();
        std::thread::spawn(move || {
            let _ = inner.call("POST", "v1/sign-out", Some("{}"), Duration::from_secs(15));
            inner.forget();
            inner.say("Signed out of sync.");
        });
    }

    /// Deletes the account, its rows and devices, and signs out.
    pub fn delete_account(&self) {
        let inner = self.inner.clone();
        inner.working("Deleting the account…");
        std::thread::spawn(move || {
            match inner.call(
                "POST",
                "v1/account/delete",
                Some("{}"),
                Duration::from_secs(60),
            ) {
                Ok(_) => {
                    inner.forget();
                    inner.say("The account is deleted. This Mac keeps its workspace.");
                }
                Err(fail) => {
                    inner.idle();
                    inner.say(&fail.message());
                }
            }
        });
    }

    /// A different server, as typed; signs out of the old one first.
    pub fn use_server(&self, typed: &str) -> Result<(), String> {
        let url = if typed.trim().is_empty() {
            HOSTED_SERVER.to_string()
        } else {
            takt_core::sync::endpoints::sync_http_url(typed.into())
                .ok_or_else(|| "That isn't a server address.".to_string())?
        };
        let url = current_server(&url);
        if self.signed_in() && lock(&self.inner.status).server != url {
            self.sign_out();
        }
        lock(&self.inner.status).server = url.clone();
        keychain::save("server", &url)?;
        Ok(())
    }
}

impl Inner {
    fn wake(&self, wake: Wake) {
        if let Some(sender) = lock(&self.wake).as_ref() {
            let _ = sender.send(wake);
        }
    }

    fn changed(&self, workspace: bool) {
        crate::platform::push(crate::platform::Event::Sync { workspace });
    }

    fn say(&self, text: &str) {
        announce(text, false);
    }

    fn set_phase(&self, phase: Phase) {
        lock(&self.status).phase = phase;
        self.changed(false);
    }

    fn working(&self, what: &str) {
        self.set_phase(Phase::Working(what.into()));
    }

    fn idle(&self) {
        let last = self
            .core
            .sync_state()
            .ok()
            .flatten()
            .and_then(|s| s.last_synced_at_ms);
        self.set_phase(Phase::Idle(last));
    }

    fn settle_unpaired(&self) {
        if lock(&self.credentials).is_some() {
            self.idle();
        } else {
            self.set_phase(Phase::Unpaired);
        }
    }

    /// A Supabase session in hand: the device introduces itself to the
    /// sync server, and only then counts as signed in.
    fn finish_sign_in(self: &Arc<Self>, session: Session) {
        let server = lock(&self.status).server.clone();
        let device_id = keychain::load::<Credentials>("credentials")
            .map(|c| c.device_id)
            .or_else(|| self.core.sync_state().ok().flatten().map(|s| s.device_id))
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let credentials = Credentials {
            server_url: server.clone(),
            device_id: device_id.clone(),
            account_id: Some(session.user_id.clone()),
            email: session.email.clone(),
            signed_out: false,
        };
        *lock(&self.session) = Some(session.clone());
        *lock(&self.credentials) = Some(credentials.clone());
        let body =
            wire::sync_register_device_body(device_id.clone(), device_name(), "macos".into());
        if let Err(fail) = self.call("POST", "v1/devices", Some(&body), Duration::from_secs(30)) {
            supabase().sign_out(&session.access_token);
            *lock(&self.session) = None;
            *lock(&self.credentials) = None;
            self.set_phase(Phase::Unpaired);
            return self.say(&fail.message());
        }
        if let Err(error) = self.core.begin_sync(device_id, server) {
            *lock(&self.credentials) = None;
            self.set_phase(Phase::Unpaired);
            return self.say(&error.to_string());
        }
        let saved = keychain::save("credentials", &credentials)
            .and_then(|()| keychain::save("session", &session));
        if let Err(error) = saved {
            self.say(&error);
        }
        {
            let mut status = lock(&self.status);
            status.email = credentials.email.clone();
            status.account = None;
        }
        self.say(&format!(
            "Signed in as {}.",
            credentials.email.as_deref().unwrap_or("you")
        ));
        self.start();
    }

    /// Stops syncing and forgets the sign-in; the workspace stays.
    fn forget(&self) {
        self.stop();
        if let Some(session) = lock(&self.session).take() {
            supabase().sign_out(&session.access_token);
        }
        *lock(&self.credentials) = None;
        keychain::clear("credentials");
        keychain::clear("session");
        let _ = self.core.end_sync();
        {
            let mut status = lock(&self.status);
            status.account = None;
            status.needs_new_password = false;
        }
        self.set_phase(Phase::Unpaired);
    }

    /// Supabase won't refresh the session: stop, and keep the email for
    /// signing in again. The outbox keeps recording meanwhile.
    fn refused(&self) {
        self.stop();
        let credentials = lock(&self.credentials).take();
        *lock(&self.session) = None;
        if let Some(mut credentials) = credentials {
            credentials.signed_out = true;
            let _ = keychain::save("credentials", &credentials);
        }
        keychain::clear("session");
        self.set_phase(Phase::NeedsSignIn);
    }

    fn stop(&self) {
        self.run.fetch_add(1, Ordering::SeqCst);
        *lock(&self.wake) = None;
    }

    fn start(self: &Arc<Self>) {
        let generation = self.run.fetch_add(1, Ordering::SeqCst) + 1;
        let (sender, receiver) = mpsc::channel();
        let _ = sender.send(Wake::Now);
        *lock(&self.wake) = Some(sender.clone());
        let cycles = self.clone();
        std::thread::spawn(move || cycles.cycles(receiver, generation));
        let watch = self.clone();
        std::thread::spawn(move || watch.watch(sender, generation));
    }

    fn live(&self, generation: u64) -> bool {
        self.run.load(Ordering::SeqCst) == generation
    }

    fn cycles(&self, receiver: mpsc::Receiver<Wake>, generation: u64) {
        let mut failures = 0u32;
        let mut next = EVERY;
        loop {
            let woke = receiver.recv_timeout(next);
            if !self.live(generation) {
                return;
            }
            match woke {
                Err(RecvTimeoutError::Disconnected) => return,
                Ok(Wake::Local) => {
                    // Until the writes go quiet.
                    while receiver.recv_timeout(QUIET).is_ok() {}
                    if !self.live(generation) {
                        return;
                    }
                }
                Ok(Wake::Now | Wake::Remote) | Err(RecvTimeoutError::Timeout) => {}
            }
            self.set_phase(Phase::Syncing);
            match self.cycle() {
                Ok(changed) => {
                    failures = 0;
                    next = EVERY;
                    self.idle();
                    if changed {
                        self.changed(true);
                    }
                }
                Err(Fail::Unauthorized) => {
                    self.refused();
                    return;
                }
                Err(Fail::Other(message)) => {
                    failures += 1;
                    next = Duration::from_secs(wire::sync_backoff_seconds(failures).into());
                    self.set_phase(Phase::Failed(message));
                }
            }
        }
    }

    /// Notices news on the server while a window is up.
    fn watch(&self, sender: Sender<Wake>, generation: u64) {
        while self.live(generation) {
            if !self.foreground.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
            let cursor = self.cursor();
            let path = format!("v1/changes?since={cursor}&limit=1&wait=25");
            match self.call("GET", &path, None, Duration::from_secs(60)) {
                Ok(body) => {
                    let news = serde_json::from_str::<serde_json::Value>(&body)
                        .ok()
                        .and_then(|v| {
                            v.get("rows")
                                .and_then(|r| r.as_array())
                                .map(|r| !r.is_empty())
                        })
                        .unwrap_or(false);
                    if news && self.live(generation) {
                        let _ = sender.send(Wake::Remote);
                        // Give the cycle a moment to move the cursor on.
                        for _ in 0..20 {
                            std::thread::sleep(Duration::from_millis(500));
                            if self.cursor() != cursor || !self.live(generation) {
                                break;
                            }
                        }
                    }
                }
                Err(Fail::Unauthorized) => return,
                Err(Fail::Other(_)) => std::thread::sleep(Duration::from_secs(15)),
            }
        }
    }

    fn cursor(&self) -> i64 {
        self.core
            .sync_state()
            .ok()
            .flatten()
            .map(|s| s.cursor)
            .unwrap_or(0)
    }

    /// One push-then-pull. The first after pairing snapshots and pulls
    /// first, so a device joining an account adopts it. True when the pull
    /// changed the workspace.
    fn cycle(&self) -> Result<bool, Fail> {
        let core = |e: takt_core::CoreError| Fail::Other(e.to_string());
        let state = self
            .core
            .sync_state()
            .map_err(core)?
            .ok_or_else(|| Fail::Other("This device isn't signed in to sync.".into()))?;
        if state.needs_snapshot {
            self.core.enqueue_sync_snapshot(now_ms()).map_err(core)?;
            let changed = self.pull(state.cursor)?;
            self.push()?;
            Ok(changed)
        } else {
            self.push()?;
            let cursor = self.cursor();
            self.pull(cursor)
        }
    }

    fn push(&self) -> Result<(), Fail> {
        let core = |e: takt_core::CoreError| Fail::Other(e.to_string());
        while let Some(batch) = self
            .core
            .prepare_sync_push(PUSH_BATCH, now_ms())
            .map_err(core)?
        {
            self.call(
                "POST",
                "v1/push",
                Some(&batch.body),
                Duration::from_secs(60),
            )?;
            self.core
                .finish_sync_push(batch.through_seq, batch.hlc, now_ms())
                .map_err(core)?;
        }
        Ok(())
    }

    fn pull(&self, cursor: i64) -> Result<bool, Fail> {
        let pull = Arc::new(SyncPull::new());
        let mut next = cursor;
        loop {
            let path = format!("v1/changes?since={next}&limit={PULL_PAGE}&wait=0");
            let body = self.call("GET", &path, None, Duration::from_secs(60))?;
            let page = pull
                .add_page(body)
                .map_err(|_| Fail::Other("The sync server's answer couldn't be read.".into()))?;
            next = page.cursor;
            if !page.has_more {
                break;
            }
        }
        let outcome = self
            .core
            .apply_sync_pull(pull, now_ms(), now_ms())
            .map_err(|e| Fail::Other(e.to_string()))?;
        Ok(outcome.changed)
    }

    /// A token good for now, refreshed first if it has run out.
    fn token(&self) -> Result<String, Fail> {
        let session = lock(&self.session).clone().ok_or(Fail::Unauthorized)?;
        if session.fresh(now_ms() / 1000) {
            return Ok(session.access_token);
        }
        self.refreshed()
    }

    fn refreshed(&self) -> Result<String, Fail> {
        let session = lock(&self.session).clone().ok_or(Fail::Unauthorized)?;
        match supabase().refresh(&session.refresh_token) {
            Ok(fresh) => {
                let _ = keychain::save("session", &fresh);
                let token = fresh.access_token.clone();
                *lock(&self.session) = Some(fresh);
                Ok(token)
            }
            Err(AuthError::Refused(_)) => Err(Fail::Unauthorized),
            Err(AuthError::Failed(text)) => Err(Fail::Other(text)),
        }
    }

    /// A request to the sync server with the token and the device, sent
    /// once more with a refreshed token if the first is refused.
    fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
        timeout: Duration,
    ) -> Result<String, Fail> {
        let (server, device) = {
            let credentials = lock(&self.credentials);
            let credentials = credentials.as_ref().ok_or(Fail::Unauthorized)?;
            (
                credentials.server_url.clone(),
                credentials.device_id.clone(),
            )
        };
        let url = format!("{}/{path}", current_server(&server));
        let send = |token: &str| {
            let bearer = format!("Bearer {token}");
            http::request(
                method,
                &url,
                &[("Authorization", &bearer), (DEVICE_HEADER, &device)],
                body,
                timeout,
            )
            .map_err(Fail::Other)
        };
        let mut reply = send(&self.token()?)?;
        if reply.status == 401 {
            reply = send(&self.refreshed()?)?;
            if reply.status == 401 {
                return Err(Fail::Unauthorized);
            }
        }
        if !reply.ok() {
            return Err(Fail::Other(wire::sync_failure_message(
                reply.status.into(),
                reply.body,
            )));
        }
        Ok(reply.body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_hosted_addresses_read_as_the_new_one() {
        assert_eq!(
            current_server("https://takt-sync.up.railway.app/"),
            HOSTED_SERVER
        );
        assert_eq!(
            current_server("https://sync.example.com/"),
            "https://sync.example.com"
        );
    }
}
