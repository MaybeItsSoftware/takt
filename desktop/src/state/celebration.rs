//! What a row does as its task is finished (`Takt/Plugins/Native/Celebration/`):
//! a line drawn through it (Strike), sparks thrown along it (Spark), a tint
//! folding shut over it (Fold), or nothing. Optionally a quiet system sound
//! (`CelebrationSound.swift`), off unless asked for. Reduce Motion skips the
//! animation and keeps the sound.
//!
//! The state is only which rows are mid-celebration and since when; the
//! drawing is `ui::celebrate`'s.

use super::App;
use super::store::now_ms;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Strike,
    Spark,
    Fold,
    None,
}

impl Style {
    pub const ALL: [Style; 4] = [Style::Strike, Style::Spark, Style::Fold, Style::None];

    pub fn id(self) -> &'static str {
        match self {
            Style::Strike => "strike",
            Style::Spark => "spark",
            Style::Fold => "fold",
            Style::None => "none",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Style::Strike => "Strike",
            Style::Spark => "Spark",
            Style::Fold => "Fold",
            Style::None => "None",
        }
    }

    /// The Swift app's plugin id (`native.celebration.spark`) or our own.
    pub fn from_id(id: &str) -> Option<Self> {
        let id = id.strip_prefix("native.celebration.").unwrap_or(id);
        Style::ALL.into_iter().find(|style| style.id() == id)
    }

    pub fn duration_ms(self) -> i64 {
        match self {
            Style::Strike => 340,
            Style::Spark => 650,
            Style::Fold => 420,
            Style::None => 0,
        }
    }

    /// The system sound and its volume, as `CelebrationSound.swift` pairs
    /// them with each preset.
    fn sound(self) -> Option<(&'static str, f32)> {
        match self {
            Style::Strike => Some(("Pop", 0.3)),
            Style::Spark => Some(("Glass", 0.25)),
            Style::Fold => Some(("Tink", 0.25)),
            Style::None => None,
        }
    }
}

impl App {
    pub fn celebration_style(&self) -> Style {
        self.prefs
            .celebration
            .as_deref()
            .and_then(Style::from_id)
            .unwrap_or(Style::Strike)
    }

    /// The Swift app's choice of preset and sound, once.
    pub fn adopt_swift_celebration(&mut self) {
        if self.prefs.celebration.is_some() || std::env::var_os("TAKT_DESKTOP_PREFS").is_some() {
            return;
        }
        let chosen = super::adopt::swift_default("completionCelebrationIdentifier")
            .and_then(|id| Style::from_id(&id))
            .unwrap_or(Style::Strike);
        self.prefs.celebration = Some(chosen.id().into());
        self.prefs.celebration_sound =
            super::adopt::swift_default("completionSoundEnabled").is_some_and(|v| v == "1");
        self.prefs.save();
    }

    /// A task just finished: its row starts to celebrate.
    pub fn start_celebration(&mut self, task_id: &str) {
        let style = self.celebration_style();
        if style == Style::None {
            return;
        }
        if !crate::platform::reduce_motion() {
            self.celebrating.insert(task_id.to_string(), now_ms());
        }
        if self.prefs.celebration_sound
            && let Some((name, volume)) = style.sound()
        {
            crate::platform::play_sound(name, volume);
        }
    }

    /// When the row's celebration started, while it is still playing.
    pub fn celebrating(&self, task_id: &str) -> Option<(i64, Style)> {
        let style = self.celebration_style();
        self.celebrating
            .get(task_id)
            .filter(|started| now_ms() - **started < style.duration_ms() + 100)
            .map(|started| (*started, style))
    }

    /// Forgets the celebrations that have played out.
    pub fn prune_celebrations(&mut self) {
        let keep = self.celebration_style().duration_ms() + 1000;
        let now = now_ms();
        self.celebrating.retain(|_, started| now - *started < keep);
    }

    pub fn celebration_rows(&self) -> Vec<(&'static str, &'static str, &'static str, String)> {
        vec![
            (
                "celebration",
                "Completion",
                "Celebration as a task is finished",
                self.celebration_style().name().into(),
            ),
            (
                "celebration-sound",
                "Completion",
                "Play a quiet sound with it",
                if self.prefs.celebration_sound {
                    "On"
                } else {
                    "Off"
                }
                .into(),
            ),
        ]
    }

    /// Return on a celebration row; false when it isn't one.
    pub fn change_celebration_setting(&mut self, id: &str) -> bool {
        match id {
            "celebration" => {
                let now = self.celebration_style();
                let at = Style::ALL.iter().position(|s| *s == now).unwrap_or(0);
                let next = Style::ALL[(at + 1) % Style::ALL.len()];
                self.prefs.celebration = Some(next.id().into());
            }
            "celebration-sound" => {
                self.prefs.celebration_sound = !self.prefs.celebration_sound;
                if self.prefs.celebration_sound
                    && let Some((name, volume)) = self.celebration_style().sound()
                {
                    crate::platform::play_sound(name, volume);
                }
            }
            _ => return false,
        }
        self.prefs.save();
        self.open_overlay(super::Overlay::Settings);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_swift_plugin_ids_name_the_same_presets() {
        assert_eq!(
            Style::from_id("native.celebration.spark"),
            Some(Style::Spark)
        );
        assert_eq!(Style::from_id("fold"), Some(Style::Fold));
        assert_eq!(Style::from_id("native.celebration.confetti"), None);
    }
}
