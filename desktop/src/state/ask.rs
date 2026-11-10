//! A sheet that asks for one thing: an email, then a password, a server's
//! address, a word to confirm. Return answers it, Esc leaves it. What each
//! answer does is the integration's: sync's sign-in, Checkvist's key.

use super::{App, Editing, Overlay};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// Sync sign-in's email, then its password; `create` makes the account.
    SyncEmail {
        create: bool,
    },
    SyncPassword {
        email: String,
        create: bool,
    },
    /// Where a reset link goes.
    SyncResetEmail,
    SyncNewPassword,
    SyncServer,
    /// "delete", typed, to delete the sync account.
    SyncDelete,
    /// "remove", typed, to delete the lists Takt made in Google Tasks.
    GoogleTasksRemove,
    /// Checkvist's account email, then its API key.
    CheckvistEmail,
    CheckvistKey {
        email: String,
    },
    /// A line to the agent about the quick-ask task (`aa`).
    Agent,
    /// The Google OAuth client the integrations sign in through.
    GoogleClientId,
    /// The Desktop client's secret, from the same page as its ID.
    GoogleClientSecret,
    /// A folder to search as an Obsidian vault in the link picker.
    ObsidianVault,
    /// The morning or evening reminder's time.
    ReminderTime {
        dailies: bool,
    },
    /// An invite link or code for a shared list.
    JoinList,
}

impl Ask {
    pub fn title(&self) -> &'static str {
        match self {
            Ask::SyncEmail { create: false } => "Sign in to sync",
            Ask::SyncEmail { create: true } | Ask::SyncPassword { create: true, .. } => {
                "Create a sync account"
            }
            Ask::SyncPassword { .. } => "Sign in to sync",
            Ask::SyncResetEmail => "Reset the sync password",
            Ask::SyncNewPassword => "Choose a new password",
            Ask::SyncServer => "Self-hosted sync server",
            Ask::SyncDelete => "Delete the sync account",
            Ask::GoogleTasksRemove => "Remove Takt's lists from Google Tasks",
            Ask::CheckvistEmail | Ask::CheckvistKey { .. } => "Connect Checkvist",
            Ask::Agent => "Ask the agent",
            Ask::GoogleClientId => "Google OAuth client ID",
            Ask::GoogleClientSecret => "Google OAuth client secret",
            Ask::ObsidianVault => "Add an Obsidian vault",
            Ask::ReminderTime { dailies: false } => "Morning reminder at",
            Ask::ReminderTime { dailies: true } => "Evening reminder at",
            Ask::JoinList => "Join a shared list",
        }
    }

    pub fn placeholder(&self) -> &'static str {
        match self {
            Ask::SyncEmail { .. } | Ask::SyncResetEmail | Ask::CheckvistEmail => "Email address",
            Ask::SyncPassword { .. } | Ask::SyncNewPassword => "Password",
            Ask::SyncServer => "https://sync.example.com",
            Ask::SyncDelete => "delete",
            Ask::GoogleTasksRemove => "remove",
            Ask::CheckvistKey { .. } => "API key (checkvist.com → Profile → Open API key)",
            Ask::Agent => "Make it due Friday, split it up, explain it…",
            Ask::GoogleClientId => "….apps.googleusercontent.com",
            Ask::GoogleClientSecret => "GOCSPX-…",
            Ask::ObsidianVault => "~/Documents/Notes",
            Ask::ReminderTime { .. } => "09:00",
            Ask::JoinList => "The invite link or code",
        }
    }

    /// What the sheet says under the field.
    pub fn hint(&self) -> String {
        match self {
            Ask::SyncPassword { email, .. } => format!("For {email}."),
            Ask::SyncDelete => {
                "Deletes the account, everything synced to it and its devices. This Mac keeps its workspace. Type delete to go ahead.".into()
            }
            Ask::GoogleTasksRemove => {
                "Deletes from Google Tasks every list Takt mirrors there, with their tasks, and turns Google Tasks off. Your lists and tasks in Takt stay, and lists you made in Google yourself are left alone. Type remove to go ahead.".into()
            }
            Ask::SyncServer => {
                "Run sync-server/ from Takt's repository; it must trust the same Supabase project. Leave it empty for Takt Sync.".into()
            }
            Ask::CheckvistKey { email } => format!("For {email}. Kept in the keychain."),
            Ask::GoogleClientSecret => {
                "From the Desktop app client in Google Cloud → Clients. Google asks for it on every sign-in.".into()
            }
            Ask::JoinList => {
                "Someone who shares a list with you sends an invite. Its tasks are everyone's to edit; where it sits in your sidebar and your board columns stay yours.".into()
            }
            _ => String::new(),
        }
    }

    pub fn secret(&self) -> bool {
        matches!(
            self,
            Ask::SyncPassword { .. } | Ask::SyncNewPassword | Ask::CheckvistKey { .. }
        )
    }
}

impl App {
    pub fn ask(&mut self, ask: Ask) {
        self.overlay = None;
        self.editing = Some(Editing::Ask(ask));
    }

    /// What the sheet says under the field.
    pub fn ask_hint(&self, ask: &Ask) -> String {
        match ask {
            Ask::Agent => self.quick_ask_transcript(),
            other => other.hint(),
        }
    }

    /// What the field starts with.
    pub fn ask_prefill(&self, ask: &Ask) -> String {
        match ask {
            Ask::SyncEmail { .. } | Ask::SyncResetEmail => {
                self.sync.status().email.unwrap_or_default()
            }
            Ask::SyncServer if self.sync_hosted() => String::new(),
            Ask::SyncServer => self.sync.status().server,
            Ask::GoogleClientId => self.prefs.google_client_id.clone().unwrap_or_default(),
            Ask::JoinList => clipboard_invite().unwrap_or_default(),
            Ask::ReminderTime { dailies } => super::reminders::clock(if *dailies {
                self.prefs.remind_dailies_left_minute
            } else {
                self.prefs.remind_due_today_minute
            }),
            _ => String::new(),
        }
    }

    /// Return on the sheet.
    pub fn answer(&mut self, ask: Ask, text: &str) {
        self.editing = None;
        let text = text.trim();
        match ask {
            Ask::SyncEmail { create } => {
                if text.is_empty() {
                    self.message = Some("Enter your email address.".into());
                    return self.ask(Ask::SyncEmail { create });
                }
                self.ask(Ask::SyncPassword {
                    email: text.into(),
                    create,
                });
            }
            Ask::SyncPassword { email, create } => {
                self.sync.sign_in(&email, text, create);
                self.open_overlay(Overlay::Settings);
            }
            Ask::SyncResetEmail => {
                self.sync.reset_password(text);
                self.open_overlay(Overlay::Settings);
            }
            Ask::SyncNewPassword => {
                self.sync.set_new_password(text);
                self.open_overlay(Overlay::Settings);
            }
            Ask::SyncServer => {
                if let Err(problem) = self.sync.use_server(text) {
                    self.message = Some(problem);
                }
                self.open_overlay(Overlay::Settings);
            }
            Ask::GoogleTasksRemove => {
                if text.eq_ignore_ascii_case("remove") {
                    self.remove_google_tasks_mirror();
                } else {
                    self.message = Some("Left Google Tasks as it is.".into());
                }
                self.open_overlay(Overlay::Settings);
            }
            Ask::SyncDelete => {
                if text.eq_ignore_ascii_case("delete") {
                    self.sync.delete_account();
                } else {
                    self.message = Some("Left the account as it is.".into());
                }
                self.open_overlay(Overlay::Settings);
            }
            Ask::CheckvistEmail => {
                self.ask(Ask::CheckvistKey { email: text.into() });
            }
            Ask::CheckvistKey { email } => {
                self.connect_checkvist(&email, text);
                self.open_overlay(Overlay::Settings);
            }
            Ask::Agent => {
                self.send_quick_ask(text);
                self.ask(Ask::Agent);
            }
            Ask::GoogleClientId => {
                self.set_google_client_id(text);
                self.open_overlay(Overlay::Settings);
            }
            Ask::GoogleClientSecret => {
                self.set_google_client_secret(text);
                self.open_overlay(Overlay::Settings);
            }
            Ask::ObsidianVault => {
                self.add_obsidian_vault(text);
                self.open_overlay(Overlay::Settings);
            }
            Ask::ReminderTime { dailies } => {
                self.set_reminder_time(dailies, text);
                self.open_overlay(Overlay::Settings);
            }
            Ask::JoinList => self.answer_join(text),
        }
    }

    /// Esc on the sheet: back to settings.
    pub fn leave_ask(&mut self) {
        if matches!(self.editing, Some(Editing::Ask(Ask::Agent | Ask::JoinList))) {
            self.editing = None;
            return;
        }
        self.editing = None;
        self.open_overlay(Overlay::Settings);
    }
}

/// The clipboard, when it holds an invite link or code.
fn clipboard_invite() -> Option<String> {
    let output = std::process::Command::new("/usr/bin/pbpaste")
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    takt_core::sync::spaces::sync_invite_code(text.clone())?;
    Some(text.trim().to_string())
}
