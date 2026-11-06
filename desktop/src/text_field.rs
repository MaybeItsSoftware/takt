//! A text field, since GPUI ships none: one line or several, a selection,
//! word and line motions, and IME composition.
//!
//! Typed text does not come through `handle`: a printable key is left to
//! the platform, whose input method composes it (kana, accents, dead keys)
//! and hands it back through the window's `EntityInputHandler`, which calls
//! `replace` and `mark`. `handle` keeps the keys that edit or move.

use gpui::Keystroke;
use std::ops::Range;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextField {
    pub text: String,
    /// A byte offset on a char boundary: where the caret is.
    pub cursor: usize,
    /// The other end of the selection, when there is one.
    pub anchor: Option<usize>,
    /// What the input method is composing, in bytes.
    pub marked: Option<Range<usize>>,
    /// Return makes a new line, and ⌘Return or Esc ends the edit.
    pub multiline: bool,
}

/// What a key did to the field, for its owner to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKey {
    Edited,
    Commit,
    Cancel,
    Tab {
        shift: bool,
    },
    Up,
    Down,
    /// Printable: leave it to the input method, which types it.
    Text(String),
    /// ⌘C, ⌘X, ⌘V: the owner reaches the clipboard.
    Copy,
    Cut,
    Paste,
    /// Not the field's: the owner may route it on.
    Ignored,
}

impl TextField {
    pub fn new(text: &str) -> Self {
        TextField {
            text: text.into(),
            cursor: text.len(),
            ..Default::default()
        }
    }

    pub fn multiline(text: &str) -> Self {
        TextField {
            multiline: true,
            ..Self::new(text)
        }
    }

    pub fn selecting_all(text: &str) -> Self {
        let mut field = Self::new(text);
        if !text.is_empty() {
            field.anchor = Some(0);
        }
        field
    }

    pub fn at_start(text: &str) -> Self {
        TextField {
            cursor: 0,
            ..Self::new(text)
        }
    }

    /// The selected bytes, or the caret as an empty range.
    pub fn selection(&self) -> Range<usize> {
        match self.anchor {
            Some(anchor) if anchor < self.cursor => anchor..self.cursor,
            Some(anchor) => self.cursor..anchor,
            None => self.cursor..self.cursor,
        }
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection()]
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

    /// The start of the word before `from`, skipping spaces first.
    fn word_start(&self, from: usize) -> usize {
        let before = &self.text[..from];
        let trimmed = before.trim_end();
        trimmed
            .rfind(|c: char| c.is_whitespace())
            .map_or(0, |ix| ix + 1)
    }

    fn word_end(&self, from: usize) -> usize {
        let after = &self.text[from..];
        let skipped = after.len() - after.trim_start().len();
        let rest = &after[skipped..];
        let word = rest.find(char::is_whitespace).unwrap_or(rest.len());
        from + skipped + word
    }

    fn line_start(&self, from: usize) -> usize {
        self.text[..from].rfind('\n').map_or(0, |ix| ix + 1)
    }

    fn line_end(&self, from: usize) -> usize {
        self.text[from..]
            .find('\n')
            .map_or(self.text.len(), |ix| from + ix)
    }

    /// The same column on the line above or below, or nothing at the edge.
    fn vertical(&self, from: usize, down: bool) -> Option<usize> {
        let start = self.line_start(from);
        let column = self.text[start..from].chars().count();
        let target_start = if down {
            let end = self.line_end(from);
            if end == self.text.len() {
                return None;
            }
            end + 1
        } else {
            if start == 0 {
                return None;
            }
            self.line_start(start - 1)
        };
        let target_end = self.line_end(target_start);
        let line = &self.text[target_start..target_end];
        Some(
            target_start
                + line
                    .char_indices()
                    .nth(column)
                    .map_or(line.len(), |(ix, _)| ix),
        )
    }

    /// Replaces the selection (or what is being composed) with `typed`.
    pub fn insert(&mut self, typed: &str) {
        let range = self.marked.take().unwrap_or_else(|| self.selection());
        self.replace(range, typed);
    }

    /// Replaces `range` with `text`, leaving the caret after it. What the
    /// input method calls once a character is decided.
    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        let text: String = text
            .chars()
            .filter(|c| !c.is_control() || (self.multiline && *c == '\n'))
            .collect();
        let range = self.clamp(range);
        self.text.replace_range(range.clone(), &text);
        self.cursor = range.start + text.len();
        self.anchor = None;
        self.marked = None;
    }

    /// Replaces `range` with text still being composed, selecting
    /// `selected` within it (relative to its start).
    pub fn mark(&mut self, range: Range<usize>, text: &str, selected: Option<Range<usize>>) {
        let range = self.clamp(range);
        self.text.replace_range(range.clone(), text);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        match selected {
            Some(selected) => {
                self.anchor = Some(self.clamp_offset(range.start + selected.start));
                self.cursor = self.clamp_offset(range.start + selected.end);
                if self.anchor == Some(self.cursor) {
                    self.anchor = None;
                }
            }
            None => {
                self.cursor = range.start + text.len();
                self.anchor = None;
            }
        }
    }

    fn clamp_offset(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn clamp(&self, range: Range<usize>) -> Range<usize> {
        let start = self.clamp_offset(range.start);
        start..self.clamp_offset(range.end.max(start))
    }

    fn delete_range(&mut self, from: usize, to: usize) {
        if from < to {
            self.text.replace_range(from..to, "");
        }
        self.cursor = from;
        self.anchor = None;
    }

    /// Moves the caret to `to`, extending the selection when `extend`.
    fn move_to(&mut self, to: usize, extend: bool) {
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = to;
        if self.anchor == Some(self.cursor) {
            self.anchor = None;
        }
    }

    pub fn select_all(&mut self) {
        if !self.text.is_empty() {
            self.anchor = Some(0);
            self.cursor = self.text.len();
        }
    }

    pub fn handle(&mut self, keystroke: &Keystroke) -> FieldKey {
        let m = keystroke.modifiers;
        let key = keystroke.key.as_str();
        let shift = m.shift;
        let selection = self.selection();
        let has_selection = !selection.is_empty();
        match key {
            "enter" if self.multiline && !m.platform && !m.control => {
                self.insert("\n");
                return FieldKey::Edited;
            }
            "enter" => return FieldKey::Commit,
            "escape" if self.multiline => return FieldKey::Commit,
            "escape" => return FieldKey::Cancel,
            "tab" => return FieldKey::Tab { shift: m.shift },
            _ => {}
        }
        match (key, m.platform, m.alt, m.control) {
            ("a", true, false, false) => self.select_all(),
            ("c", true, false, false) => return FieldKey::Copy,
            ("x", true, false, false) => return FieldKey::Cut,
            ("v", true, false, false) => return FieldKey::Paste,
            ("up", false, false, false) => {
                match self.multiline.then(|| self.vertical(self.cursor, false)) {
                    Some(Some(to)) => self.move_to(to, shift),
                    _ => return FieldKey::Up,
                }
            }
            ("down", false, false, false) => {
                match self.multiline.then(|| self.vertical(self.cursor, true)) {
                    Some(Some(to)) => self.move_to(to, shift),
                    _ => return FieldKey::Down,
                }
            }
            ("up", true, ..) => self.move_to(0, shift),
            ("down", true, ..) => self.move_to(self.text.len(), shift),
            ("a", false, false, true) | ("home", ..) | ("left", true, _, _) => {
                let to = if self.multiline {
                    self.line_start(self.cursor)
                } else {
                    0
                };
                self.move_to(to, shift)
            }
            ("e", false, false, true) | ("end", ..) | ("right", true, _, _) => {
                let to = if self.multiline {
                    self.line_end(self.cursor)
                } else {
                    self.text.len()
                };
                self.move_to(to, shift)
            }
            ("left", false, true, _) => self.move_to(self.word_start(self.cursor), shift),
            ("right", false, true, _) => self.move_to(self.word_end(self.cursor), shift),
            ("left", ..) if has_selection && !shift => self.move_to(selection.start, false),
            ("right", ..) if has_selection && !shift => self.move_to(selection.end, false),
            ("left", ..) => self.move_to(self.previous_boundary(self.cursor), shift),
            ("right", ..) => self.move_to(self.next_boundary(self.cursor), shift),
            ("backspace" | "delete", ..) if has_selection => {
                self.delete_range(selection.start, selection.end)
            }
            ("backspace", true, _, _) | ("u", false, false, true) => {
                let start = if self.multiline {
                    self.line_start(self.cursor)
                } else {
                    0
                };
                self.delete_range(start, self.cursor)
            }
            ("backspace", false, true, _) | ("w", false, false, true) => {
                let start = self.word_start(self.cursor);
                self.delete_range(start, self.cursor);
            }
            ("backspace", ..) => {
                let start = self.previous_boundary(self.cursor);
                self.delete_range(start, self.cursor);
            }
            ("delete", ..) => {
                let end = self.next_boundary(self.cursor);
                let start = self.cursor;
                self.delete_range(start, end);
            }
            _ if m.platform || m.control => return FieldKey::Ignored,
            _ => match &keystroke.key_char {
                Some(typed) if !typed.is_empty() => return FieldKey::Text(typed.clone()),
                _ if key == "space" => return FieldKey::Text(" ".into()),
                _ => return FieldKey::Ignored,
            },
        }
        FieldKey::Edited
    }

    #[cfg(test)]
    /// `handle`, typing printable keys itself: for tests and for a field
    /// with no input method behind it.
    pub fn handle_typing(&mut self, keystroke: &Keystroke) -> FieldKey {
        match self.handle(keystroke) {
            FieldKey::Text(text) => {
                self.insert(&text);
                FieldKey::Edited
            }
            other => other,
        }
    }

    #[cfg(test)]
    pub fn split(&self) -> (&str, &str) {
        self.text.split_at(self.cursor)
    }

    // MARK: UTF-16, which the input method counts in

    pub fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..self.clamp_offset(offset)]
            .chars()
            .map(char::len_utf16)
            .sum()
    }

    pub fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (ix, c) in self.text.char_indices() {
            if utf16 >= offset {
                return ix;
            }
            utf16 += c.len_utf16();
        }
        self.text.len()
    }

    pub fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    pub fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
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
            field.handle_typing(&keystroke);
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
    fn printable_keys_are_left_to_the_input_method() {
        let mut field = TextField::default();
        assert_eq!(field.handle(&key("k")), FieldKey::Text("k".into()));
        assert_eq!(field.text, "");
    }

    #[test]
    fn a_selected_title_is_replaced_by_typing() {
        let mut field = TextField::selecting_all("old");
        typed(&mut field, "new");
        assert_eq!(field.text, "new");
    }

    #[test]
    fn shift_extends_and_backspace_takes_the_selection() {
        let mut field = TextField::new("one two");
        field.handle(&key("shift-alt-left"));
        assert_eq!(field.selected_text(), "two");
        field.handle(&key("backspace"));
        assert_eq!(field.text, "one ");
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
    fn composition_marks_then_replaces() {
        let mut field = TextField::new("to ");
        let at = field.cursor;
        field.mark(at..at, "k", None);
        assert_eq!(field.marked, Some(3..4));
        let marked = field.marked.clone().unwrap();
        field.mark(marked, "か", None);
        assert_eq!(field.text, "to か");
        field.insert("家");
        assert_eq!(field.text, "to 家");
        assert_eq!(field.marked, None);
        assert_eq!(field.offset_to_utf16(field.cursor), 4);
    }

    #[test]
    fn several_lines_move_by_line_and_return_breaks_one() {
        let mut field = TextField::multiline("first\nsecond");
        field.handle(&key("up"));
        assert_eq!(field.split().0, "first");
        field.handle(&key("enter"));
        assert_eq!(field.text, "first\n\nsecond");
        assert_eq!(field.handle(&key("cmd-enter")), FieldKey::Commit);
        assert_eq!(field.handle(&key("escape")), FieldKey::Commit);
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
