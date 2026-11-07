//! Reminders: the core says what goes off and when (`reminders.rs`); the
//! app, which stays in the menu bar, shows each as its time passes.
//! `ReminderCoordinator.swift`, which handed the list to the system
//! instead. One that fell due while the app was closed is shown at launch
//! if it is less than ten minutes old, and not at all after that.

use super::App;
use super::ask::Ask;
use super::store::now_ms;
use takt_core::reminders::ReminderSettings;

const CHECK_EVERY_MS: i64 = 20_000;
const STALE_MS: i64 = 10 * 60_000;

pub fn clock(minute: i32) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

impl App {
    pub fn reminder_settings(&self) -> ReminderSettings {
        let p = &self.prefs;
        ReminderSettings {
            due_time: p.remind_due_time,
            due_today: p.remind_due_today,
            due_today_minute: p.remind_due_today_minute,
            dailies_left: p.remind_dailies_left,
            dailies_left_minute: p.remind_dailies_left_minute,
        }
    }

    /// Called every second; shows what fell due since the last look.
    pub fn tick_reminders(&mut self) {
        let now = now_ms();
        if now - self.reminders_looked_ms < CHECK_EVERY_MS {
            return;
        }
        self.reminders_looked_ms = now;
        let settings = self.reminder_settings();
        if !(settings.due_time || settings.due_today || settings.dailies_left) {
            return;
        }
        let from = self.prefs.reminders_checked_ms.max(now - STALE_MS);
        let Ok(due) = self
            .store
            .core
            .reminders(settings, from, self.store.zone.clone(), 2, 64)
        else {
            return;
        };
        let firing: Vec<_> = due.into_iter().filter(|r| r.fire_at_ms <= now).collect();
        for reminder in &firing {
            crate::platform::notifications::post(
                &reminder.id,
                &reminder.title,
                "",
                &reminder.body,
                true,
                reminder.task_id.as_deref(),
            );
        }
        self.prefs.reminders_checked_ms = now;
        if !firing.is_empty() || now / 60_000 % 10 == 0 {
            self.prefs.save();
        }
    }

    pub fn reminder_rows(&self) -> Vec<(&'static str, &'static str, &'static str, String)> {
        let p = &self.prefs;
        let on = |b: bool| {
            if b {
                "On".to_string()
            } else {
                "Off".to_string()
            }
        };
        vec![
            (
                "remind-due-time",
                "Reminders",
                "When a task's due time comes",
                on(p.remind_due_time),
            ),
            (
                "remind-due-today",
                "Reminders",
                "What's due today, each morning",
                if p.remind_due_today {
                    format!("At {}", clock(p.remind_due_today_minute))
                } else {
                    on(false)
                },
            ),
            (
                "remind-due-today-at",
                "Reminders",
                "Morning reminder time",
                clock(p.remind_due_today_minute),
            ),
            (
                "remind-dailies",
                "Reminders",
                "Dailies left, each evening",
                if p.remind_dailies_left {
                    format!("At {}", clock(p.remind_dailies_left_minute))
                } else {
                    on(false)
                },
            ),
            (
                "remind-dailies-at",
                "Reminders",
                "Evening reminder time",
                clock(p.remind_dailies_left_minute),
            ),
        ]
    }

    /// Return on a reminders row; false when it isn't one.
    pub fn change_reminder_setting(&mut self, id: &str) -> bool {
        let p = &mut self.prefs;
        match id {
            "remind-due-time" => p.remind_due_time = !p.remind_due_time,
            "remind-due-today" => p.remind_due_today = !p.remind_due_today,
            "remind-dailies" => p.remind_dailies_left = !p.remind_dailies_left,
            "remind-due-today-at" => {
                self.ask(Ask::ReminderTime { dailies: false });
                return true;
            }
            "remind-dailies-at" => {
                self.ask(Ask::ReminderTime { dailies: true });
                return true;
            }
            _ => return false,
        }
        // Turning one on looks from now, not from whenever it was last on.
        p.reminders_checked_ms = now_ms();
        p.save();
        true
    }

    /// "9:30", "21:00", "9pm" as minutes after midnight.
    pub fn set_reminder_time(&mut self, dailies: bool, text: &str) {
        let Some((hour, minute)) = takt_core::capture::time_of_day(text.trim()) else {
            self.message = Some("That isn't a time: try 09:00 or 8pm.".into());
            return;
        };
        let minutes = (hour * 60 + minute) as i32;
        if dailies {
            self.prefs.remind_dailies_left_minute = minutes;
        } else {
            self.prefs.remind_due_today_minute = minutes;
        }
        self.prefs.save();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn times_read_as_minutes() {
        assert_eq!(takt_core::capture::time_of_day("9:30"), Some((9, 30)));
        assert_eq!(super::clock(9 * 60 + 5), "09:05");
    }
}
