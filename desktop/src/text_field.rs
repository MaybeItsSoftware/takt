//! A one-line text field, since GPUI ships none: typing, the cursor, word
//! and line deletion, select-all. What it does not do yet is IME
//! composition (`docs/rust-desktop-plan.md`, stage 2).

use gpui::Keystroke;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextField {
    pub text: String,
    /// A byte offset on a char boundary.
    pub cursor: usize,
    /// Everything is selected: the next character replaces it.
    pub all_selected: bool,
}

/// What a key did to the field, for its owner to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKey {
    Edited,
    Commit,
    Cancel,
    Tab {
        shift: bool,
    },
    Up,
    Down,
    /// Not the field's: the owner may route it on.
    Ignored,
}

impl TextField {
    pub fn new(text: &str) -> Self {
        TextField {
            text: text.into(),
            cursor: text.len(),
            all_selected: false,
        }
    }

    pub fn selecting_all(text: &str) -> Self {
        TextField {
            all_selected: !text.is_empty(),
            ..Self::new(text)
        }
    }

    pub fn at_start(text: &str) -> Self {
        TextField {
            cursor: 0,
            ..Self::new(text)
        }
    }

    fn previous_boundary(&self, from: usize) -> usize {
        self.text[..from]
            .char_indices()
            .next_back()
            .map_or(0, |(ix, _)| ix)
    }

    fn next_boundary(&self, from: usize) -> usize {
        self.text[from..]
            .chars()
            .next()
            .map_or(from, |c| from + c.len_utf8())
    }

    /// The start of the word before the cursor, skipping spaces first.
    fn word_start(&self) -> usize {
        let before = &self.text[..self.cursor];
        let trimmed = before.trim_end();
        trimmed
            .rfind(|c: char| c.is_whitespace())
            .map_or(0, |ix| ix + 1)
    }

    fn word_end(&self) -> usize {
        let after = &self.text[self.cursor..];
        let skipped = after.len() - after.trim_start().len();
        let rest = &after[skipped..];
        let word = rest.find(char::is_whitespace).unwrap_or(rest.len());
        self.cursor + skipped + word
    }

    fn insert(&mut self, typed: &str) {
        if self.all_selected {
            self.text.clear();
            self.cursor = 0;
            self.all_selected = false;
        }
        let typed: String = typed.chars().filter(|c| !c.is_control()).collect();
        self.text.insert_str(self.cursor, &typed);
        self.cursor += typed.len();
    }

    fn delete_range(&mut self, from: usize, to: usize) {
        if from < to {
            self.text.replace_range(from..to, "");
            self.cursor = from;
        }
    }

    pub fn handle(&mut self, keystroke: &Keystroke) -> FieldKey {
        let m = keystroke.modifiers;
        let key = keystroke.key.as_str();
        match key {
            "enter" => return FieldKey::Commit,
            "escape" => return FieldKey::Cancel,
            "tab" => return FieldKey::Tab { shift: m.shift },
            "up" if !m.platform => return FieldKey::Up,
            "down" if !m.platform => return FieldKey::Down,
            _ => {}
        }
        if self.all_selected {
            match key {
                "backspace" | "delete" => {
                    self.text.clear();
                    self.cursor = 0;
                    self.all_selected = false;
                    return FieldKey::Edited;
                }
                "left" | "home" | "up" => {
                    self.all_selected = false;
                    self.cursor = 0;
                    return FieldKey::Edited;
                }
                "right" | "end" | "down" => {
                    self.all_selected = false;
                    self.cursor = self.text.len();
                    return FieldKey::Edited;
                }
                _ => {}
            }
        }
        match (key, m.platform, m.alt, m.control) {
            ("a", true, _, _) => self.all_selected = !self.text.is_empty(),
            ("a", false, false, true) | ("home", ..) | ("left", true, _, _) => {
                self.all_selected = false;
                self.cursor = 0;
            }
            ("e", false, false, true) | ("end", ..) | ("right", true, _, _) => {
                self.all_selected = false;
                self.cursor = self.text.len();
            }
            ("left", false, true, _) => self.cursor = self.word_start(),
            ("right", false, true, _) => self.cursor = self.word_end(),
            ("left", ..) => self.cursor = self.previous_boundary(self.cursor),
            ("right", ..) => self.cursor = self.next_boundary(self.cursor),
            ("backspace", true, _, _) | ("u", false, false, true) => {
                self.delete_range(0, self.cursor)
            }
            ("backspace", false, true, _) | ("w", false, false, true) => {
                let start = self.word_start();
                self.delete_range(start, self.cursor);
            }
            ("backspace", ..) => {
                let start = self.previous_boundary(self.cursor);
                self.delete_range(start, self.cursor);
            }
            ("delete", ..) => {
                let end = self.next_boundary(self.cursor);
                self.delete_range(self.cursor, end);
            }
            _ if m.platform || m.control => return FieldKey::Ignored,
            _ => match &keystroke.key_char {
                Some(typed) if !typed.is_empty() => self.insert(typed),
                _ if key == "space" => self.insert(" "),
                _ => return FieldKey::Ignored,
            },
        }
        FieldKey::Edited
    }

    /// The text either side of the cursor, for drawing it.
    pub fn split(&self) -> (&str, &str) {
        self.text.split_at(self.cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(spec: &str) -> Keystroke {
        let mut keystroke = Keystroke::parse(spec).expect("a keystroke");
        if keystroke.key.chars().count() == 1 && !keystroke.modifiers.platform {
            keystroke.key_char = Some(keystroke.key.clone());
        }
        keystroke
    }

    fn typed(field: &mut TextField, text: &str) {
        for c in text.chars() {
            let spec = if c == ' ' {
                "space".to_string()
            } else {
                c.to_string()
            };
            let mut keystroke = key(&spec);
            keystroke.key_char = Some(c.to_string());
            field.handle(&keystroke);
        }
    }

    #[test]
    fn types_and_deletes() {
        let mut field = TextField::default();
        typed(&mut field, "plan the week");
        assert_eq!(field.text, "plan the week");
        field.handle(&key("alt-backspace"));
        assert_eq!(field.text, "plan the ");
        field.handle(&key("backspace"));
        assert_eq!(field.text, "plan the");
        field.handle(&key("cmd-backspace"));
        assert_eq!(field.text, "");
    }

    #[test]
    fn a_selected_title_is_replaced_by_typing() {
        let mut field = TextField::selecting_all("old");
        typed(&mut field, "new");
        assert_eq!(field.text, "new");
    }

    #[test]
    fn moves_by_character_and_word_across_multibyte_text() {
        let mut field = TextField::new("café crème");
        field.handle(&key("alt-left"));
        assert_eq!(field.split().1, "crème");
        field.handle(&key("left"));
        field.handle(&key("left"));
        assert_eq!(field.split().1, "é crème");
        field.handle(&key("cmd-right"));
        assert_eq!(field.cursor, field.text.len());
    }

    #[test]
    fn return_escape_and_tab_are_the_owners() {
        let mut field = TextField::new("x");
        assert_eq!(field.handle(&key("enter")), FieldKey::Commit);
        assert_eq!(field.handle(&key("escape")), FieldKey::Cancel);
        assert_eq!(
            field.handle(&key("shift-tab")),
            FieldKey::Tab { shift: true }
        );
        assert_eq!(field.text, "x");
    }
}
