//! What a key press means: spelled the way the catalogue spells its keys,
//! looked up for the surface on screen, and run through the two-letter
//! sequence machine.
//!
//! A port of the Swift app's `WorkspaceCommandCatalog+Dispatch.swift`,
//! `WorkspaceKeyBindings.swift` and `DesktopShortcutSequence.swift`, so a key
//! does the same thing here as there. The catalogue itself is the core's
//! (`takt_core::keymap`); nothing here lists a key.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use takt_core::keymap::{CommandEntry, CommandSurface, workspace_command_catalog};

/// The modifiers and key of one press, toolkit-free so it can be tested.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Press {
    pub cmd: bool,
    pub ctrl: bool,
    pub option: bool,
    pub shift: bool,
    /// The toolkit's name for the key: a character, or `enter`, `backspace`…
    pub key: String,
}

impl Press {
    pub fn from_keystroke(keystroke: &gpui::Keystroke) -> Self {
        Press {
            cmd: keystroke.modifiers.platform,
            ctrl: keystroke.modifiers.control,
            option: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
            key: keystroke.key.clone(),
        }
    }
}

/// Keys spelled by name, which Shift does not turn into another character.
const NAMED: [&str; 27] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "comma", "space", "enter", "tab",
    "escape", "f2", "delete", "left", "right", "down", "up", "home", "end", "pageup", "pagedown",
];

/// A shifted character back to its key, for the named keys a toolkit may
/// report by what Shift made of them.
fn unshifted_named(key: &str) -> Option<&'static str> {
    Some(match key {
        "!" => "1",
        "@" => "2",
        "#" => "3",
        "$" => "4",
        "%" => "5",
        "^" => "6",
        "&" => "7",
        "*" => "8",
        "(" => "9",
        ")" => "0",
        "_" => "-",
        "+" => "=",
        "<" => "comma",
        _ => return None,
    })
}

/// What Shift makes of a key on a US layout, for the keys not named.
fn shifted(key: &str) -> Option<&'static str> {
    Some(match key {
        "/" => "?",
        "[" => "{",
        "]" => "}",
        ";" => ":",
        "'" => "\"",
        "." => ">",
        "`" => "~",
        "\\" => "|",
        _ => return None,
    })
}

/// The press as the catalogue spells it: `cmd`, `ctrl`, `option`, `shift`,
/// then the key. Shift stays only on a named key, a letter, or in a chord;
/// otherwise it is already in the character (`?`, not `shift+/`). In a chord
/// the full stop stays `.`, so ⇧⌘. reads `cmd+shift+.`.
pub fn spell(press: &Press) -> Option<String> {
    let raw = press.key.as_str();
    let toolkit_name = match raw {
        "backspace" | "delete" => Some("delete"),
        "enter" | "return" => Some("enter"),
        "escape" => Some("escape"),
        "tab" => Some("tab"),
        "space" | " " => Some("space"),
        "," => Some("comma"),
        _ => None,
    };
    let chord = press.cmd || press.ctrl;
    let (base, named) = if let Some(name) = toolkit_name {
        (name.to_string(), true)
    } else if let Some(name) = unshifted_named(raw) {
        (name.to_string(), true)
    } else if NAMED.contains(&raw) {
        (raw.to_string(), true)
    } else if raw.is_empty() {
        return None;
    } else if press.shift && !(chord && raw == ".") {
        (shifted(raw).unwrap_or(raw).to_lowercase(), false)
    } else {
        (raw.to_lowercase(), false)
    };
    let is_letter = !named && base.chars().count() == 1 && base.chars().all(char::is_alphabetic);
    let keeps_shift = press.shift && (named || chord || press.option || is_letter);
    let mut parts: Vec<&str> = Vec::new();
    if press.cmd {
        parts.push("cmd");
    }
    if press.ctrl {
        parts.push("ctrl");
    }
    if press.option {
        parts.push("option");
    }
    if keeps_shift {
        parts.push("shift");
    }
    parts.push(&base);
    Some(parts.join("+"))
}

/// Carries `cmd` or `ctrl`, so it cannot be typed into a field by accident.
pub fn is_chord(key: &str) -> bool {
    key.starts_with("cmd+") || key.starts_with("ctrl+")
}

fn is_letter_sequence(key: &str) -> bool {
    let mut chars = key.chars();
    let letter = |c: Option<char>| c.is_some_and(|c| c.is_alphabetic() && c.is_lowercase());
    letter(chars.next()) && letter(chars.next()) && chars.next().is_none() && !NAMED.contains(&key)
}

pub fn is_sequence(key: &str) -> bool {
    takt_core::keymap::is_sequence(key)
}

/// The `Anywhere` commands still live on a surface that owns the keyboard.
pub const REACHABLE_FROM_FULL_PANE_SCREENS: [&str; 26] = [
    "goToday",
    "goBoard",
    "goOutline",
    "goMatrix",
    "goEverything",
    "goInbox",
    "goFocus",
    "goTimeline",
    "goSearch",
    "goCommandPalette",
    "goKeyboardReference",
    "windowUndo",
    "windowRedo",
    "windowToggleSidebar",
    "windowToggleInspectorPane",
    "windowToggleRightDock",
    "windowToggleDoneRail",
    "windowToggleAgentPanel",
    "windowToggleProgressDock",
    "windowCloseAllDocks",
    "focusFinish",
    "focusStop",
    "goSidebarRegion",
    "goTaskRegion",
    "goInspectorRegion",
    "goCycleRegion",
];

/// Commands whose chord still works with the caret in a text field.
pub const REACHABLE_FROM_TEXT_FIELD: [&str; 30] = [
    "goToday",
    "goBoard",
    "goOutline",
    "goMatrix",
    "goEverything",
    "goInbox",
    "goFocus",
    "goTimeline",
    "goSidebarRegion",
    "goTaskRegion",
    "goInspectorRegion",
    "goCycleRegion",
    "taskNew",
    "listNew",
    "folderNew",
    "listNewTaskDestination",
    "goKeyboardReference",
    "goCommandPalette",
    "goSearch",
    "goListNavigator",
    "windowToggleAgentPanel",
    "windowToggleSidebar",
    "windowToggleProgressDock",
    "windowToggleRightDock",
    "windowCloseAllDocks",
    "windowDockNextTab",
    "windowDockPreviousTab",
    "focusFinish",
    "focusStop",
    "focusSetAvailable",
];

/// Bare keys a region takes from `Anywhere`.
const REGION_BARE_KEYS: [&str; 6] = ["/", "?", "i", "r", "escape", "enter"];

fn inherits(command: &CommandEntry, key: &str, surface: CommandSurface) -> bool {
    use CommandSurface::*;
    match surface {
        Anywhere | Today | Board | Outline | Matrix => true,
        Timeline => REACHABLE_FROM_FULL_PANE_SCREENS.contains(&command.id.as_str()),
        Sidebar | Done | Conditions => {
            is_chord(key) || is_letter_sequence(key) || REGION_BARE_KEYS.contains(&key)
        }
        Inspector => is_chord(key) || REGION_BARE_KEYS.contains(&key),
    }
}

/// The catalogue's keys in force, indexed for lookup by surface.
pub struct Bindings {
    pub commands: Vec<CommandEntry>,
    own_by_key: HashMap<CommandSurface, HashMap<String, usize>>,
    claimed: HashSet<String>,
    unbound: HashMap<CommandSurface, HashSet<String>>,
}

impl Bindings {
    pub fn shipped() -> Self {
        Self::new(workspace_command_catalog(), HashMap::new())
    }

    /// The catalogue with `keymap.json` laid over it, and what in the file
    /// could not be applied.
    pub fn with_user_keymap(text: &str) -> (Self, Vec<String>) {
        let parsed = takt_core::keymap::keymap_parse(text.to_string());
        let resolved =
            takt_core::keymap::keymap_resolve(parsed.bindings, workspace_command_catalog());
        let unbound = resolved
            .unbound
            .into_iter()
            .map(|entry| (entry.surface, entry.keys.into_iter().collect()))
            .collect();
        let issues = parsed
            .issues
            .into_iter()
            .chain(resolved.issues)
            .map(|issue| issue.message)
            .collect();
        (Self::new(resolved.commands, unbound), issues)
    }

    pub fn new(
        commands: Vec<CommandEntry>,
        unbound: HashMap<CommandSurface, HashSet<String>>,
    ) -> Self {
        let mut own: HashMap<CommandSurface, HashMap<String, usize>> = HashMap::new();
        let mut claimed = HashSet::new();
        // First claimant wins, in catalogue order.
        for (index, command) in commands.iter().enumerate() {
            for key in &command.keys {
                claimed.insert(key.clone());
                own.entry(command.surface)
                    .or_default()
                    .entry(key.clone())
                    .or_insert(index);
            }
            for surface_keys in &command.surface_keys {
                for key in &surface_keys.keys {
                    claimed.insert(key.clone());
                    own.entry(surface_keys.surface)
                        .or_default()
                        .entry(key.clone())
                        .or_insert(index);
                }
            }
        }
        Bindings {
            commands,
            own_by_key: own,
            claimed,
            unbound,
        }
    }

    pub fn entry(&self, id: &str) -> Option<&CommandEntry> {
        self.commands.iter().find(|command| command.id == id)
    }

    /// The command a key means on a surface. `shift+x` nothing binds is `x`.
    pub fn command(&self, key: &str, surface: CommandSurface) -> Option<&CommandEntry> {
        if let Some(command) = self.bound(key, surface) {
            return Some(command);
        }
        if let Some(rest) = key.strip_prefix("shift+")
            && rest.chars().count() == 1
        {
            return self.bound(rest, surface);
        }
        None
    }

    fn bound(&self, key: &str, surface: CommandSurface) -> Option<&CommandEntry> {
        let own = self.own_by_key.get(&surface);
        if let Some(&index) = own.and_then(|own| own.get(key)) {
            return Some(&self.commands[index]);
        }
        if surface == CommandSurface::Anywhere
            || self
                .unbound
                .get(&surface)
                .is_some_and(|keys| keys.contains(key))
        {
            return None;
        }
        let &index = self.own_by_key.get(&CommandSurface::Anywhere)?.get(key)?;
        let fallback = &self.commands[index];
        if !inherits(fallback, key, surface) {
            return None;
        }
        // A letter the surface binds on its own is not held for an inherited
        // sequence beginning with it.
        if is_sequence(key)
            && let Some(first) = key.chars().next()
            && own.is_some_and(|own| own.contains_key(&first.to_string()))
        {
            return None;
        }
        Some(fallback)
    }

    /// Whether the key stops at the window though nothing answers to it.
    pub fn swallows_unhandled(&self, key: &str, surface: CommandSurface) -> bool {
        surface == CommandSurface::Timeline && (!is_chord(key) || self.claimed.contains(key))
    }

    /// Whether a chord is one of those that work from inside a text field.
    pub fn reaches_into_text_field(&self, key: &str, surface: CommandSurface) -> bool {
        if key == "escape" && surface == CommandSurface::Inspector {
            return self.command(key, surface).is_some();
        }
        if !is_chord(key) {
            return false;
        }
        self.command(key, surface)
            .is_some_and(|command| REACHABLE_FROM_TEXT_FIELD.contains(&command.id.as_str()))
    }

    /// The two-letter sequences live on a surface.
    pub fn sequences(&self, surface: CommandSurface) -> HashSet<String> {
        self.claimed
            .iter()
            .filter(|key| is_sequence(key) && self.command(key, surface).is_some())
            .cloned()
            .collect()
    }

    /// How a key is drawn in a menu or palette row: `⌘⇧Z`, `gh`.
    pub fn glyphs(key: &str) -> String {
        if is_sequence(key) {
            return key.to_string();
        }
        let mut out = String::new();
        for part in key.split('+') {
            out.push_str(match part {
                "cmd" => "⌘",
                "ctrl" => "⌃",
                "option" => "⌥",
                "shift" => "⇧",
                "enter" => "↩",
                "escape" => "esc",
                "delete" => "⌫",
                "tab" => "⇥",
                "space" => "␣",
                "up" => "↑",
                "down" => "↓",
                "left" => "←",
                "right" => "→",
                "comma" => ",",
                "pageup" => "⇞",
                "pagedown" => "⇟",
                "home" => "↖",
                "end" => "↘",
                other => {
                    out.push_str(&other.to_uppercase());
                    continue;
                }
            });
        }
        out
    }
}

/// What the sequence machine made of a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Not part of a sequence: dispatch the key as it is.
    Pass,
    /// Held, waiting for a second letter.
    Pending,
    /// Two keys made a sequence.
    Command(String),
    /// Run the held key on its own, then dispatch this one.
    Flush(String),
    /// Run the held key, and hold this one.
    FlushAndHold(String),
}

/// Checkvist-style two-letter commands. A held key is never lost: it runs on
/// its own when the next key does not complete it, or when the hold expires.
#[derive(Default)]
pub struct Sequence {
    prefix: String,
    held_at: Option<Instant>,
}

impl Sequence {
    pub const TIMEOUT: Duration = Duration::from_millis(1200);

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn reset(&mut self) {
        self.prefix.clear();
        self.held_at = None;
    }

    pub fn flush(&mut self) -> Option<String> {
        if self.prefix.is_empty() {
            return None;
        }
        let held = std::mem::take(&mut self.prefix);
        self.held_at = None;
        Some(held)
    }

    /// The timer's half of `flush`: releases only the hold begun at `at`.
    pub fn expire(&mut self, at: Instant) -> Option<String> {
        if self.held_at == Some(at) {
            self.flush()
        } else {
            None
        }
    }

    pub fn advance(&mut self, key: &str, at: Instant, sequences: &HashSet<String>) -> Step {
        let key = key.to_lowercase();
        let mut held = None;
        if !self.prefix.is_empty() {
            let candidate = format!("{}{key}", self.prefix);
            let in_time = self
                .held_at
                .is_some_and(|held_at| at.duration_since(held_at) <= Self::TIMEOUT);
            if in_time && sequences.contains(&candidate) {
                self.reset();
                return Step::Command(candidate);
            }
            held = self.flush();
        }
        if key.chars().count() == 1 && sequences.iter().any(|sequence| sequence.starts_with(&key)) {
            self.prefix = key;
            self.held_at = Some(at);
            return match held {
                Some(held) => Step::FlushAndHold(held),
                None => Step::Pending,
            };
        }
        match held {
            Some(held) => Step::Flush(held),
            None => Step::Pass,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: &str) -> Press {
        Press {
            key: key.into(),
            ..Press::default()
        }
    }

    #[test]
    fn spells_presses_as_the_catalogue_does() {
        assert_eq!(spell(&press("j")).unwrap(), "j");
        assert_eq!(spell(&press("backspace")).unwrap(), "delete");
        assert_eq!(spell(&press(",")).unwrap(), "comma");
        let shift = |key: &str| Press {
            shift: true,
            ..press(key)
        };
        assert_eq!(spell(&shift("/")).unwrap(), "?");
        assert_eq!(spell(&shift("?")).unwrap(), "?");
        assert_eq!(spell(&shift("d")).unwrap(), "shift+d");
        assert_eq!(spell(&shift("1")).unwrap(), "shift+1");
        assert_eq!(spell(&shift("!")).unwrap(), "shift+1");
        assert_eq!(spell(&shift("enter")).unwrap(), "shift+enter");
        let cmd_shift = |key: &str| Press {
            cmd: true,
            shift: true,
            ..press(key)
        };
        assert_eq!(spell(&cmd_shift("[")).unwrap(), "cmd+shift+{");
        assert_eq!(spell(&cmd_shift(".")).unwrap(), "cmd+shift+.");
        assert_eq!(spell(&cmd_shift("z")).unwrap(), "cmd+shift+z");
        let ctrl_shift_minus = Press {
            ctrl: true,
            shift: true,
            ..press("-")
        };
        assert_eq!(spell(&ctrl_shift_minus).unwrap(), "ctrl+shift+-");
    }

    #[test]
    fn every_catalogue_key_resolves_on_its_own_surface() {
        let bindings = Bindings::shipped();
        for command in &bindings.commands {
            for key in &command.keys {
                let found = bindings.command(key, command.surface);
                assert!(
                    found.is_some(),
                    "{key} on {:?} for {}",
                    command.surface,
                    command.id
                );
            }
        }
    }

    #[test]
    fn regions_take_only_chords_sequences_and_their_bare_keys() {
        let bindings = Bindings::shipped();
        // `x` ticks a task off on the task panes, and means nothing in the
        // sidebar, whose cursor is not on a task.
        assert!(bindings.command("x", CommandSurface::Outline).is_some());
        assert!(
            bindings
                .command("x", CommandSurface::Sidebar)
                .is_none_or(|c| c.surface == CommandSurface::Sidebar)
        );
        // ⌘K is the palette everywhere.
        for surface in CommandSurface::ALL {
            let command = bindings.command("cmd+k", surface);
            if surface != CommandSurface::Timeline {
                assert_eq!(command.map(|c| c.id.as_str()), Some("goCommandPalette"));
            }
        }
    }

    #[test]
    fn a_held_key_is_never_lost() {
        let sequences: HashSet<String> = ["gh".to_string(), "gg".to_string()].into();
        let now = Instant::now();
        let mut sequence = Sequence::default();
        assert_eq!(sequence.advance("g", now, &sequences), Step::Pending);
        assert_eq!(
            sequence.advance("h", now, &sequences),
            Step::Command("gh".into())
        );
        assert_eq!(sequence.advance("g", now, &sequences), Step::Pending);
        assert_eq!(
            sequence.advance("j", now, &sequences),
            Step::Flush("g".into())
        );
        assert_eq!(sequence.advance("g", now, &sequences), Step::Pending);
        assert_eq!(sequence.expire(now), Some("g".into()));
        let late = now + Sequence::TIMEOUT + Duration::from_millis(1);
        assert_eq!(sequence.advance("g", now, &sequences), Step::Pending);
        assert_eq!(
            sequence.advance("g", late, &sequences),
            Step::FlushAndHold("g".into())
        );
    }

    #[test]
    fn glyphs_read_as_a_mac_menu() {
        assert_eq!(Bindings::glyphs("cmd+shift+z"), "⌘⇧Z");
        assert_eq!(Bindings::glyphs("gh"), "gh");
        assert_eq!(Bindings::glyphs("ctrl+-"), "⌃-");
    }
}
