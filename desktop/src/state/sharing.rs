//! Shared lists from the window: the Lists commands (share, join, members,
//! leave, stop sharing) and the Settings rows under Shared lists. The
//! requests are sync's (`crate::sync::sharing`); this decides which list a
//! command means and asks before anything leaves someone's devices.

use super::actions::PendingDelete;
use super::ask::Ask;
use super::sidebar::RowKind;
use super::{App, Region, Scope};
use takt_core::sync::spaces::SharedList;

impl App {
    /// The list a Lists command means: the sidebar row under the cursor, or
    /// the list on screen.
    pub fn commanded_list_id(&self) -> Option<String> {
        if self.region == Region::Sidebar {
            return match self.sidebar_cursor_row().map(|row| row.kind.clone()) {
                Some(RowKind::List(id)) => Some(id),
                _ => None,
            };
        }
        match &self.scope {
            Scope::List { list_id, .. } => Some(list_id.clone()),
            _ => None,
        }
    }

    pub fn shared_list(&self, list_id: &str) -> Option<SharedList> {
        self.store
            .core
            .shared_list(list_id.to_string())
            .ok()
            .flatten()
    }

    pub fn shared_lists(&self) -> Vec<SharedList> {
        self.store.core.shared_lists().unwrap_or_default()
    }

    fn needs_sync(&mut self) -> bool {
        if self.sync.signed_in() {
            return false;
        }
        self.message = Some("Sharing needs sync. Sign in under Settings → Sync first.".into());
        true
    }

    /// `listShare`: shares the list if it isn't, and copies an invite link.
    pub fn share_commanded_list(&mut self) {
        let Some(list_id) = self.commanded_list_id() else {
            self.message = Some("Pick a list to share.".into());
            return;
        };
        self.share_list(&list_id);
    }

    pub fn share_list(&mut self, list_id: &str) {
        if self.needs_sync() {
            return;
        }
        if let Err(problem) = self.store.core.shareable_list_name(list_id.to_string()) {
            self.message = Some(problem.to_string());
            return;
        }
        self.sync.share_list(list_id);
    }

    /// `listJoin`: asks for the invite, with one already filled in when the
    /// clipboard holds it.
    pub fn begin_join_list(&mut self) {
        if self.needs_sync() {
            return;
        }
        self.ask(Ask::JoinList);
    }

    /// An invite link opened (`takt://join/<code>`): joins straight away,
    /// since following the link was the asking.
    pub fn join_from_link(&mut self, url: &str) {
        if self.needs_sync() {
            return;
        }
        if let Err(problem) = self.sync.join(url) {
            self.message = Some(problem);
        }
    }

    pub fn answer_join(&mut self, text: &str) {
        if let Err(problem) = self.sync.join(text) {
            self.message = Some(problem);
        }
    }

    /// `listMembers`: who the list is shared with, in the status bar.
    pub fn show_list_members(&mut self) {
        let Some(shared) = self
            .commanded_list_id()
            .and_then(|list_id| self.shared_list(&list_id))
        else {
            self.message = Some("This list isn't shared. Share it to invite someone.".into());
            return;
        };
        self.message = Some(format!(
            "\u{201c}{}\u{201d}: {}.",
            shared.name,
            members_line(&shared)
        ));
    }

    /// `listLeave` and `listStopSharing`: asks first; Return goes ahead.
    pub fn ask_to_unshare(&mut self, leaving: bool) {
        let Some(shared) = self
            .commanded_list_id()
            .and_then(|list_id| self.shared_list(&list_id))
        else {
            self.message = Some("This list isn't shared.".into());
            return;
        };
        self.ask_to_unshare_list(&shared.list_id, leaving);
    }

    pub fn ask_to_unshare_list(&mut self, list_id: &str, leaving: bool) {
        let Some(shared) = self.shared_list(list_id) else {
            return;
        };
        if self.needs_sync() {
            return;
        }
        match (leaving, shared.is_owner()) {
            (true, true) => {
                self.message = Some("It's your list: stop sharing it instead.".into());
            }
            (false, false) => {
                self.message = Some("Only its owner can stop sharing it. You can leave it.".into());
            }
            (true, false) => {
                self.pending_delete = Some(PendingDelete::LeaveList(shared.list_id.clone()));
                self.message = Some(format!(
                    "Leave \u{201c}{}\u{201d}? It goes from your devices; the others keep it. Return leaves, any other key stays.",
                    shared.name
                ));
            }
            (false, true) => {
                self.pending_delete = Some(PendingDelete::StopSharing(shared.list_id.clone()));
                self.message = Some(format!(
                    "Stop sharing \u{201c}{}\u{201d}? It stays yours and goes from everyone else's devices. Return stops, any other key keeps sharing.",
                    shared.name
                ));
            }
        }
    }

    pub fn ask_to_remove_member(&mut self, list_id: &str, account_id: &str) {
        let Some(shared) = self.shared_list(list_id) else {
            return;
        };
        let who = shared
            .members
            .iter()
            .find(|member| member.account_id == account_id)
            .and_then(|member| member.email.clone())
            .unwrap_or_else(|| "them".into());
        self.pending_delete = Some(PendingDelete::RemoveMember {
            list_id: list_id.into(),
            account_id: account_id.into(),
        });
        self.message = Some(format!(
            "Take {who} out of \u{201c}{}\u{201d}? Return removes, any other key keeps them.",
            shared.name
        ));
    }

    /// The Settings rows under Shared lists: each list, who is in it, and
    /// what can be done with it; then joining one.
    pub fn shared_list_rows(&self) -> Vec<(String, &'static str, String, String)> {
        let mut rows = Vec::new();
        if !self.sync.signed_in() {
            return rows;
        }
        for shared in self.shared_lists() {
            let id = &shared.list_id;
            if shared.is_owner() {
                rows.push((
                    format!("share:{id}"),
                    "Shared lists",
                    shared.name.clone(),
                    members_line(&shared),
                ));
                for member in shared.members.iter().filter(|member| !member.you) {
                    rows.push((
                        format!("member:{id}:{}", member.account_id),
                        "Shared lists",
                        format!(
                            "\u{2003}{}",
                            member.email.clone().unwrap_or_else(|| "Someone".into())
                        ),
                        String::new(),
                    ));
                }
                rows.push((
                    format!("unshare:{id}"),
                    "Shared lists",
                    format!("Stop sharing \u{201c}{}\u{201d}", shared.name),
                    String::new(),
                ));
            } else {
                rows.push((
                    format!("leave:{id}"),
                    "Shared lists",
                    shared.name.clone(),
                    members_line(&shared),
                ));
            }
        }
        rows.push((
            "share-join".into(),
            "Shared lists",
            "Join a shared list".into(),
            String::new(),
        ));
        rows
    }

    /// Return on one of [`App::shared_list_rows`]. False when `id` isn't one.
    pub fn change_shared_list_setting(&mut self, id: &str) -> bool {
        if id == "share-join" {
            self.begin_join_list();
            return true;
        }
        if let Some(list_id) = id.strip_prefix("share:") {
            self.share_list(list_id);
        } else if let Some(list_id) = id.strip_prefix("unshare:") {
            self.overlay = None;
            self.ask_to_unshare_list(list_id, false);
        } else if let Some(list_id) = id.strip_prefix("leave:") {
            self.overlay = None;
            self.ask_to_unshare_list(list_id, true);
        } else if let Some(rest) = id.strip_prefix("member:") {
            let Some((list_id, account_id)) = rest.split_once(':') else {
                return true;
            };
            self.overlay = None;
            self.ask_to_remove_member(list_id, account_id);
        } else {
            return false;
        }
        true
    }

    /// Return on a pending leave, stop or removal.
    pub fn carry_out_unshare(&mut self, pending: PendingDelete) {
        match pending {
            PendingDelete::LeaveList(list_id) => {
                if matches!(&self.scope, Scope::List { list_id: open, .. } if *open == list_id) {
                    self.select_everything();
                }
                self.sync.leave(&list_id);
            }
            PendingDelete::StopSharing(list_id) => self.sync.stop_sharing(&list_id),
            PendingDelete::RemoveMember {
                list_id,
                account_id,
            } => self.sync.remove_member(&list_id, &account_id),
            _ => {}
        }
    }
}

/// "You (owner), sam@example.com": everyone in a shared list.
pub fn members_line(shared: &SharedList) -> String {
    let mut names: Vec<String> = shared
        .members
        .iter()
        .map(|member| {
            let name = if member.you {
                "You".to_string()
            } else {
                member.email.clone().unwrap_or_else(|| "Someone".into())
            };
            if member.role == "owner" {
                format!("{name} (owner)")
            } else {
                name
            }
        })
        .collect();
    if names.is_empty() {
        names.push("Just you so far".into());
    }
    names.join(", ")
}
