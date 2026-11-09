//! The settings rows the integrations add (sync, Checkvist, reminders),
//! what each row does, and what they add to diagnostics.

use super::App;
use super::ask::Ask;
use crate::sync::Phase;

fn ago(ms: i64) -> String {
    let minutes = (super::store::now_ms() - ms).max(0) / 60_000;
    match minutes {
        0 => "just now".into(),
        1 => "a minute ago".into(),
        n if n < 60 => format!("{n} minutes ago"),
        n if n < 120 => "an hour ago".into(),
        n if n < 60 * 24 => format!("{} hours ago", n / 60),
        n => format!("{} days ago", n / (60 * 24)),
    }
}

impl App {
    pub fn platform_settings(&self) -> Vec<(&'static str, &'static str, &'static str, String)> {
        let status = self.sync.status();
        let mut rows = Vec::new();
        rows.push((
            "sync-self-hosted",
            "Sync",
            "Use your own server",
            if self.sync_hosted() {
                "Off · Takt Sync"
            } else {
                "On"
            }
            .to_string(),
        ));
        if !self.sync_hosted() {
            rows.push((
                "sync-server",
                "Sync",
                "Server address",
                status.server.clone(),
            ));
        }
        let phase = match &status.phase {
            Phase::Unpaired => "Off".to_string(),
            Phase::NeedsSignIn => "Signed out: sign in again".to_string(),
            Phase::Working(what) => what.clone(),
            Phase::Syncing => super::google::SYNCING.to_string(),
            Phase::Idle(Some(at)) => format!("Synced {}", ago(*at)),
            Phase::Idle(None) => "On".to_string(),
            Phase::Failed(why) => why.clone(),
            Phase::NeedsSubscription => "Paused: needs Takt Sync".to_string(),
        };
        if self.sync.signed_in() {
            let who = status.email.clone().unwrap_or_else(|| "signed in".into());
            rows.push(("sync-now", "Sync", "Sync now", format!("{who} · {phase}")));
            let devices = status
                .account
                .as_ref()
                .map(|a| {
                    a.devices
                        .iter()
                        .map(|d| {
                            let name = d.name.clone().unwrap_or_else(|| "Device".into());
                            if d.current {
                                format!("{name} (this Mac)")
                            } else {
                                name
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| "Return to look".into());
            rows.push(("sync-devices", "Sync", "Devices", devices));
            if let Some(subscription) = status.subscription.as_ref().filter(|s| s.required) {
                rows.push((
                    "sync-subscription",
                    "Sync",
                    "Takt Sync",
                    subscription_line(subscription),
                ));
            }
            if status.needs_new_password {
                rows.push((
                    "sync-new-password",
                    "Sync",
                    "Choose a new password",
                    String::new(),
                ));
            } else {
                rows.push((
                    "sync-new-password",
                    "Sync",
                    "Change the password",
                    String::new(),
                ));
            }
            rows.push(("sync-sign-out", "Sync", "Sign out of sync", String::new()));
            rows.push((
                "sync-delete",
                "Sync",
                "Delete the sync account",
                String::new(),
            ));
        } else {
            rows.push(("sync-google", "Sync", "Sign in with Google", phase));
            rows.push(("sync-apple", "Sync", "Sign in with Apple", String::new()));
            rows.push((
                "sync-email",
                "Sync",
                "Sign in with email",
                status.email.clone().unwrap_or_default(),
            ));
            rows.push((
                "sync-create",
                "Sync",
                "Create a sync account",
                String::new(),
            ));
            rows.push(("sync-reset", "Sync", "Forgot the password", String::new()));
        }
        match self.checkvist_connected() {
            Some(user) => {
                rows.push((
                    "checkvist-import",
                    "Checkvist",
                    "Import from Checkvist",
                    user,
                ));
                rows.push((
                    "checkvist-disconnect",
                    "Checkvist",
                    "Disconnect Checkvist",
                    String::new(),
                ));
            }
            None => rows.push((
                "checkvist-connect",
                "Checkvist",
                "Connect Checkvist",
                String::new(),
            )),
        }
        rows.extend(self.google_rows());
        rows.extend(self.links_rows());
        rows.extend(self.reminder_rows());
        rows.extend(self.celebration_rows());
        rows
    }

    /// Whether sync talks to Takt's own server.
    pub fn sync_hosted(&self) -> bool {
        self.sync.status().server == crate::sync::HOSTED_SERVER
    }

    /// The switch: on asks for the server's address, off goes back to
    /// Takt Sync at once.
    fn toggle_self_hosted(&mut self) {
        if self.sync_hosted() {
            self.ask(Ask::SyncServer);
        } else if let Err(problem) = self.sync.use_server("") {
            self.message = Some(problem);
        }
    }

    pub fn change_platform_setting(&mut self, id: &str) {
        self.message = None;
        if self.change_shared_list_setting(id)
            || self.change_google_setting(id)
            || self.change_links_setting(id)
            || self.change_celebration_setting(id)
        {
            return;
        }
        if self.change_reminder_setting(id) {
            if self.editing.is_none() {
                self.open_overlay(super::Overlay::Settings);
            }
            return;
        }
        match id {
            "sync-now" => {
                self.sync.sync_now();
                self.sync.refresh_account();
            }
            "sync-devices" | "sync-subscription" => self.sync.refresh_account(),
            "sync-new-password" => self.ask(Ask::SyncNewPassword),
            "sync-sign-out" => self.sync.sign_out(),
            "sync-delete" => self.ask(Ask::SyncDelete),
            "sync-google" => self.sync.sign_in_with("google"),
            "sync-apple" => self.sync.sign_in_with("apple"),
            "sync-email" => self.ask(Ask::SyncEmail { create: false }),
            "sync-create" => self.ask(Ask::SyncEmail { create: true }),
            "sync-reset" => self.ask(Ask::SyncResetEmail),
            "sync-server" => self.ask(Ask::SyncServer),
            "sync-self-hosted" => self.toggle_self_hosted(),
            "checkvist-connect" => self.ask(Ask::CheckvistEmail),
            "checkvist-import" => self.import_checkvist(),
            "checkvist-disconnect" => self.disconnect_checkvist(),
            client if client.starts_with("mcp:") => {
                self.install_mcp_client(&client["mcp:".len()..])
            }
            _ => {}
        }
        if self.editing.is_none() {
            self.open_overlay(super::Overlay::Settings);
        }
    }

    pub fn platform_diagnostics(&self) -> Vec<(String, String)> {
        let mut rows = Vec::new();
        let state = self.store.core.sync_state().ok().flatten();
        match state {
            Some(state) => {
                rows.push(("Sync device".into(), state.device_id));
                rows.push(("Sync cursor".into(), state.cursor.to_string()));
                rows.push((
                    "Sync outbox".into(),
                    self.store
                        .core
                        .pending_sync_changes(1000)
                        .map(|p| p.changes.len().to_string())
                        .unwrap_or_default(),
                ));
            }
            None => rows.push(("Sync".into(), "not paired".into())),
        }
        rows
    }
}

/// Takt Sync in a line: paid until when, given, or what to do.
fn subscription_line(subscription: &crate::sync::Subscription) -> String {
    if subscription.comp {
        return "Included".into();
    }
    if !subscription.active {
        return "Not subscribed. £1 a year, from the iPhone or Android app".into();
    }
    match subscription
        .expires_at
        .as_deref()
        .and_then(|at| at.get(..10))
    {
        Some(day) => format!("£1 a year · paid until {day}"),
        None => "£1 a year · active".into(),
    }
}
