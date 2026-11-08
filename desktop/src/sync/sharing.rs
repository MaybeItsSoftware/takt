//! Shared lists (`docs/shared-lists-plan.md`): sharing a list, inviting,
//! joining, leaving and stopping sharing. Each is one or two requests on a
//! thread of its own; the core makes and reads the bodies and keeps what the
//! device knows of each shared list. The rows themselves travel in the
//! ordinary cycle, which each of these starts once it is done.

use super::{Fail, Inner, Sync, Wake, current_server, lock};
use std::sync::Arc;
use std::time::Duration;
use takt_core::sync::spaces::{self as core_spaces, SharedList};

const TIMEOUT: Duration = Duration::from_secs(30);

impl Sync {
    /// Shares `list_id` if it isn't yet, then makes an invite and copies its
    /// link. Run again for each person to invite: an invite is used once.
    pub fn share_list(&self, list_id: &str) {
        let inner = self.inner.clone();
        let list_id = list_id.to_string();
        inner.working("Making an invite…");
        std::thread::spawn(move || {
            let outcome = inner.share_and_invite(&list_id);
            inner.idle();
            match outcome {
                Ok(text) => inner.announce_change(&text),
                Err(text) => inner.say(&text),
            }
        });
    }

    /// Joins the list an invite link or code names.
    pub fn join(&self, typed: &str) -> Result<(), String> {
        let code = core_spaces::sync_invite_code(typed.into())
            .ok_or_else(|| "That isn't an invite link or code.".to_string())?;
        let inner = self.inner.clone();
        inner.working("Joining the list…");
        std::thread::spawn(move || {
            let outcome = inner.join(&code);
            inner.idle();
            match outcome {
                Ok(text) => inner.announce_change(&text),
                Err(text) => inner.say(&text),
            }
        });
        Ok(())
    }

    /// Leaves a list someone else shared: it goes from this account's
    /// devices, and the others keep it.
    pub fn leave(&self, list_id: &str) {
        let inner = self.inner.clone();
        let list_id = list_id.to_string();
        std::thread::spawn(move || {
            let outcome = inner.leave(&list_id);
            match outcome {
                Ok(text) => inner.announce_change(&text),
                Err(text) => inner.say(&text),
            }
        });
    }

    /// Stops sharing a list you own: it stays yours and goes from everyone
    /// else's devices.
    pub fn stop_sharing(&self, list_id: &str) {
        let inner = self.inner.clone();
        let list_id = list_id.to_string();
        std::thread::spawn(move || {
            let outcome = inner.stop_sharing(&list_id);
            match outcome {
                Ok(text) => inner.announce_change(&text),
                Err(text) => inner.say(&text),
            }
        });
    }

    /// Takes someone out of a list you own.
    pub fn remove_member(&self, list_id: &str, account_id: &str) {
        let inner = self.inner.clone();
        let list_id = list_id.to_string();
        let account_id = account_id.to_string();
        std::thread::spawn(move || {
            let outcome = inner.remove_member(&list_id, &account_id);
            match outcome {
                Ok(text) => inner.announce_change(&text),
                Err(text) => inner.say(&text),
            }
        });
    }
}

impl Inner {
    fn announce_change(&self, text: &str) {
        super::announce(text, true);
        self.wake(Wake::Now);
    }

    fn shared(&self, list_id: &str) -> Result<Option<SharedList>, String> {
        self.core
            .shared_list(list_id.to_string())
            .map_err(|error| error.to_string())
    }

    fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<String, String> {
        self.call(method, path, body, TIMEOUT)
            .map_err(|fail| match fail {
                Fail::Unauthorized => "Sign in to sync first.".to_string(),
                other => other.message(),
            })
    }

    fn share_and_invite(self: &Arc<Self>, list_id: &str) -> Result<String, String> {
        let name = self
            .core
            .shareable_list_name(list_id.to_string())
            .map_err(|error| error.to_string())?;
        let shared = match self.shared(list_id)? {
            Some(shared) => shared,
            None => {
                let body = core_spaces::sync_share_list_body(list_id.into(), name.clone());
                let answer = self.request("POST", &core_spaces::sync_spaces_path(), Some(&body))?;
                self.core
                    .record_shared_space(answer)
                    .map_err(|error| error.to_string())?
            }
        };
        let answer = self.request(
            "POST",
            &core_spaces::sync_space_invites_path(shared.space_id.clone()),
            Some("{}"),
        )?;
        let invite = core_spaces::sync_decode_invite(answer).map_err(|error| error.to_string())?;
        let server = lock(&self.status).server.clone();
        let link = core_spaces::sync_invite_link(current_server(&server), invite.code.clone());
        let grouped = core_spaces::sync_invite_code_grouped(invite.code);
        if crate::state::mcp_clients::copy(&link) {
            Ok(format!(
                "Invite link for \u{201c}{name}\u{201d} copied. Send it to one person; it works once, for seven days. Code {grouped}."
            ))
        } else {
            Ok(format!(
                "Invite for \u{201c}{name}\u{201d}: {link} (code {grouped}). It works once, for seven days."
            ))
        }
    }

    fn join(self: &Arc<Self>, code: &str) -> Result<String, String> {
        let preview = self.request("GET", &core_spaces::sync_invite_path(code.into()), None)?;
        let preview =
            core_spaces::sync_decode_invite_preview(preview).map_err(|error| error.to_string())?;
        let answer = self.request(
            "POST",
            &core_spaces::sync_accept_invite_path(code.into()),
            Some("{}"),
        )?;
        let shared = self
            .core
            .record_shared_space(answer)
            .map_err(|error| error.to_string())?;
        let from = preview
            .owner_email
            .map(|email| format!(" from {email}"))
            .unwrap_or_default();
        Ok(if preview.joined {
            format!("You're already in \u{201c}{}\u{201d}.", shared.name)
        } else {
            format!(
                "Joined \u{201c}{}\u{201d}{from}. Its tasks arrive with the next sync.",
                shared.name
            )
        })
    }

    fn leave(self: &Arc<Self>, list_id: &str) -> Result<String, String> {
        let shared = self
            .shared(list_id)?
            .ok_or_else(|| "That list isn't shared.".to_string())?;
        if shared.is_owner() {
            return Err("It's your list: stop sharing it instead.".into());
        }
        let you = shared
            .your_account_id()
            .ok_or_else(|| "Sync hasn't said who you are in this list yet.".to_string())?
            .to_string();
        self.request(
            "DELETE",
            &core_spaces::sync_space_member_path(shared.space_id.clone(), you),
            None,
        )?;
        self.core
            .forget_shared_space(shared.space_id, false)
            .map_err(|error| error.to_string())?;
        Ok(format!("Left \u{201c}{}\u{201d}.", shared.name))
    }

    fn stop_sharing(self: &Arc<Self>, list_id: &str) -> Result<String, String> {
        let shared = self
            .shared(list_id)?
            .ok_or_else(|| "That list isn't shared.".to_string())?;
        if !shared.is_owner() {
            return Err("Only its owner can stop sharing it. You can leave it.".into());
        }
        self.request(
            "DELETE",
            &core_spaces::sync_space_path(shared.space_id.clone()),
            None,
        )?;
        self.core
            .forget_shared_space(shared.space_id, true)
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "\u{201c}{}\u{201d} isn't shared any more. It stays here.",
            shared.name
        ))
    }

    fn remove_member(self: &Arc<Self>, list_id: &str, account_id: &str) -> Result<String, String> {
        let shared = self
            .shared(list_id)?
            .ok_or_else(|| "That list isn't shared.".to_string())?;
        let who = shared
            .members
            .iter()
            .find(|member| member.account_id == account_id)
            .and_then(|member| member.email.clone())
            .unwrap_or_else(|| "Them".into());
        self.request(
            "DELETE",
            &core_spaces::sync_space_member_path(shared.space_id, account_id.into()),
            None,
        )?;
        Ok(format!("{who} is out of \u{201c}{}\u{201d}.", shared.name))
    }
}
