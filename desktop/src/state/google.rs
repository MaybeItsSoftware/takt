//! The Google integrations as the app runs them: one worker thread that
//! passes the Calendar and Tasks mirrors every five minutes and a few
//! seconds after local writes go quiet, and their settings rows. The
//! passes themselves are `crate::google`'s.

use super::{App, Overlay};
use crate::google::{self, CALENDAR_SCOPES, TASKS_SCOPES};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use takt_core::google_calendar::{GoogleCalendarInfo, GoogleCalendarSettings};
use takt_core::workspace::CoreWorkspace;

const EVERY: Duration = Duration::from_secs(300);
const QUIET: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Default)]
struct Config {
    client_id: String,
    calendar: bool,
    tasks: bool,
}

#[derive(Default)]
pub struct Google {
    config: Arc<Mutex<Config>>,
    /// What the last pass came to, for the settings row.
    status: Arc<Mutex<String>>,
    wake: Option<Sender<bool>>,
    seen_generation: u64,
    /// The account's calendars, once fetched for the picker.
    pub calendars: Arc<Mutex<Vec<GoogleCalendarInfo>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

impl Google {
    pub fn status(&self) -> String {
        lock(&self.status).clone()
    }
}

fn pass(core: &CoreWorkspace, workspace_id: &str, zone: &str, config: &Config) -> (String, bool) {
    let mut said = Vec::new();
    let mut wrote = false;
    if config.calendar {
        match google::calendar::run_pass(core, &config.client_id, zone) {
            Ok(Some(summary)) => {
                wrote |= summary.tasks_updated + summary.tasks_imported > 0;
                said.push(match summary.failures.first() {
                    Some(failure) => format!("Calendar: {failure}"),
                    None => format!("Calendar synced, {} written", summary.events_written),
                });
            }
            Ok(None) => said.push("Calendar: choose a calendar".into()),
            Err(error) => said.push(format!("Calendar: {}", error.message())),
        }
    }
    if config.tasks {
        match google::tasks::run_pass(core, workspace_id, &config.client_id, zone) {
            Ok(summary) => {
                wrote |= summary.wrote_locally;
                said.push(if summary.conflicts > 0 {
                    format!(
                        "Tasks: {} mirrored, {} Google edits overwritten",
                        summary.mirrored, summary.conflicts
                    )
                } else {
                    format!("Tasks: {} mirrored", summary.mirrored)
                });
            }
            Err(error) => said.push(format!("Tasks: {}", error.message())),
        }
    }
    (said.join(" · "), wrote)
}

impl App {
    fn google_config(&self) -> Config {
        Config {
            client_id: self.google_client_id(),
            calendar: self.prefs.google_calendar,
            tasks: self.prefs.google_tasks,
        }
    }

    pub fn google_client_id(&self) -> String {
        self.prefs
            .google_client_id
            .clone()
            .or_else(google::swift_client_id)
            .unwrap_or_default()
    }

    /// Starts the worker. Not in a smoke run, which must not reach Google.
    pub fn start_google(&mut self) {
        *lock(&self.google.config) = self.google_config();
        if std::env::var_os("TAKT_DESKTOP_SMOKE").is_some() || self.google.wake.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel::<bool>();
        self.google.wake = Some(sender.clone());
        let core = self.store.core.clone();
        let workspace_id = self.store.workspace_id.clone();
        let zone = self.store.zone.clone();
        let config = self.google.config.clone();
        let status = self.google.status.clone();
        std::thread::spawn(move || {
            let _ = sender.send(true);
            loop {
                match receiver.recv_timeout(EVERY) {
                    Err(RecvTimeoutError::Disconnected) => return,
                    // A local write: wait for the typing to stop.
                    Ok(false) => while receiver.recv_timeout(QUIET).is_ok() {},
                    Ok(true) | Err(RecvTimeoutError::Timeout) => {}
                }
                let config = lock(&config).clone();
                if !(config.calendar || config.tasks) || !google::connected() {
                    continue;
                }
                let (said, wrote) = pass(&core, &workspace_id, &zone, &config);
                *lock(&status) = said;
                crate::platform::push(crate::platform::Event::Sync { workspace: wrote });
            }
        });
    }

    /// Called every second: our own writes bring a pass forward.
    pub fn google_note_change(&mut self) {
        if self.generation != self.google.seen_generation {
            self.google.seen_generation = self.generation;
            if let Some(wake) = &self.google.wake {
                let _ = wake.send(false);
            }
        }
    }

    fn google_now(&mut self) {
        *lock(&self.google.config) = self.google_config();
        if let Some(wake) = &self.google.wake {
            let _ = wake.send(true);
        }
    }

    fn calendar_settings(&self) -> GoogleCalendarSettings {
        self.store
            .core
            .google_calendar_settings()
            .unwrap_or(GoogleCalendarSettings {
                calendar_id: None,
                import_events: false,
                agenda_calendar_ids: Vec::new(),
            })
    }

    pub fn google_rows(&self) -> Vec<(&'static str, &'static str, &'static str, String)> {
        let on = |b: bool| if b { "On" } else { "Off" }.to_string();
        let client = self.google_client_id();
        let mut rows = vec![(
            "google-client",
            "Google",
            "OAuth client ID",
            if client.is_empty() {
                "Not set".into()
            } else {
                format!("{}…", client.chars().take(16).collect::<String>())
            },
        )];
        if google::connected() {
            rows.push((
                "google-disconnect",
                "Google",
                "Disconnect Google",
                "Connected".into(),
            ));
        } else {
            rows.push(("google-connect", "Google", "Connect Google", String::new()));
        }
        let settings = self.calendar_settings();
        let chosen = settings.calendar_id.as_ref().map(|id| {
            lock(&self.google.calendars)
                .iter()
                .find(|c| &c.id == id)
                .map(|c| c.summary.clone())
                .unwrap_or_else(|| id.clone())
        });
        rows.extend([
            (
                "google-calendar",
                "Google",
                "Mirror timed tasks to Google Calendar",
                on(self.prefs.google_calendar),
            ),
            (
                "google-calendar-choose",
                "Google",
                "Two-way calendar",
                chosen.unwrap_or_else(|| "None".into()),
            ),
            (
                "google-import-events",
                "Google",
                "Import the calendar's events as tasks",
                on(settings.import_events),
            ),
            (
                "google-tasks",
                "Google",
                "Mirror lists to Google Tasks",
                on(self.prefs.google_tasks),
            ),
            (
                "google-sync-now",
                "Google",
                "Sync Google now",
                self.google.status(),
            ),
        ]);
        rows
    }

    /// Return on a Google row; false when it isn't one.
    pub fn change_google_setting(&mut self, id: &str) -> bool {
        match id {
            "google-client" => self.ask(super::ask::Ask::GoogleClientId),
            "google-connect" => {
                let client = self.google_client_id();
                let mut scopes: Vec<&str> = CALENDAR_SCOPES.to_vec();
                scopes.extend(TASKS_SCOPES);
                self.message = Some("Finish connecting Google in the browser…".into());
                std::thread::spawn(move || {
                    let said = match google::connect(&client, &scopes) {
                        Ok(()) => "Google connected.".to_string(),
                        Err(error) => error,
                    };
                    crate::sync::announce(&said, false);
                });
            }
            "google-disconnect" => {
                google::disconnect();
                self.message = Some("Google disconnected.".into());
            }
            "google-calendar" => {
                self.prefs.google_calendar = !self.prefs.google_calendar;
                self.prefs.save();
                self.google_now();
            }
            "google-tasks" => {
                self.prefs.google_tasks = !self.prefs.google_tasks;
                self.prefs.save();
                self.google_now();
            }
            "google-import-events" => {
                let mut settings = self.calendar_settings();
                settings.import_events = !settings.import_events;
                if let Err(error) = self
                    .store
                    .core
                    .set_google_calendar_settings(settings, super::store::now_ms())
                {
                    self.message = Some(error.to_string());
                }
                self.google_now();
            }
            "google-calendar-choose" => {
                let client = self.google_client_id();
                let calendars = self.google.calendars.clone();
                self.message = Some("Fetching your calendars…".into());
                std::thread::spawn(move || match google::calendar::calendars(&client) {
                    Ok(found) => {
                        *lock(&calendars) = found;
                        crate::platform::push(crate::platform::Event::Menu(
                            "pick-google-calendar".into(),
                        ));
                    }
                    Err(error) => crate::sync::announce(&error.message(), false),
                });
                return true;
            }
            "google-sync-now" => {
                self.google_now();
                self.message = Some("Syncing Google…".into());
            }
            _ => return false,
        }
        self.open_overlay(Overlay::Settings);
        true
    }

    pub fn set_google_client_id(&mut self, text: &str) {
        let text = text.trim();
        self.prefs.google_client_id = (!text.is_empty()).then(|| text.to_string());
        self.prefs.save();
        *lock(&self.google.config) = self.google_config();
    }

    /// The calendars fetched for the picker, as (id, name, detail).
    pub fn google_calendar_choices(&self) -> Vec<(String, String, String)> {
        lock(&self.google.calendars)
            .iter()
            .filter(|c| c.can_write)
            .map(|c| {
                (
                    c.id.clone(),
                    c.summary.clone(),
                    if c.is_primary {
                        "Primary".into()
                    } else {
                        String::new()
                    },
                )
            })
            .collect()
    }

    pub fn choose_google_calendar(&mut self, id: Option<String>) {
        let mut settings = self.calendar_settings();
        settings.calendar_id = id;
        if let Err(error) = self
            .store
            .core
            .set_google_calendar_settings(settings, super::store::now_ms())
        {
            self.message = Some(error.to_string());
        }
        self.google_now();
    }
}
