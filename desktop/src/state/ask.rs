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
    /// Checkvist's account email, then its API key.
    CheckvistEmail,
    CheckvistKey {
        email: String,
    },
    /// The Google OAuth client the integrations sign in through.
    GoogleClientId,
    /// The morning or evening reminder's time.
    ReminderTime {
        dailies: bool,
    },
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
            Ask::SyncServer => "Sync server",
            Ask::SyncDelete => "Delete the sync account",
            Ask::CheckvistEmail | Ask::CheckvistKey { .. } => "Connect Checkvist",
            Ask::GoogleClientId => "Google OAuth client ID",
            Ask::ReminderTime { dailies: false } => "Morning reminder at",
            Ask::ReminderTime { dailies: true } => "Evening reminder at",
        }
    }

    pub fn placeholder(&self) -> &'static str {
        match self {
            Ask::SyncEmail { .. } | Ask::SyncResetEmail | Ask::CheckvistEmail => "Email address",
            Ask::SyncPassword { .. } | Ask::SyncNewPassword => "Password",
            Ask::SyncServer => "https://… (empty for the hosted server)",
            Ask::SyncDelete => "delete",
            Ask::CheckvistKey { .. } => "API key (checkvist.com → Profile → Open API key)",
            Ask::GoogleClientId => "….apps.googleusercontent.com",
            Ask::ReminderTime { .. } => "09:00",
        }
    }

    /// What the sheet says under the field.
    pub fn hint(&self) -> String {
        match self {
            Ask::SyncPassword { email, .. } => format!("For {email}."),
            Ask::SyncDelete => {
                "Deletes the account, everything synced to it and its devices. This Mac keeps its workspace. Type delete to go ahead.".into()
            }
            Ask::SyncServer => {
                "A self-hosted server must trust the same Supabase project.".into()
            }
            Ask::CheckvistKey { email } => format!("For {email}. Kept in the keychain."),
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

    /// What the field starts with.
    pub fn ask_prefill(&self, ask: &Ask) -> String {
        match ask {
            Ask::SyncEmail { .. } | Ask::SyncResetEmail => {
                self.sync.status().email.unwrap_or_default()
            }
            Ask::SyncServer => self.sync.status().server,
            Ask::GoogleClientId => self.google_client_id(),
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
            Ask::GoogleClientId => {
                self.set_google_client_id(text);
                self.open_overlay(Overlay::Settings);
            }
            Ask::ReminderTime { dailies } => {
                self.set_reminder_time(dailies, text);
                self.open_overlay(Overlay::Settings);
            }
        }
    }

    /// Esc on the sheet: back to settings.
    pub fn leave_ask(&mut self) {
        self.editing = None;
        self.open_overlay(Overlay::Settings);
    }
}
