//! The Markdown editor plugin: a task's notes maximised into a pane the
//! size of the window, with the Markdown drawn beside them
//! (`docs/markdown-editor.md`).
//!
//! When the task links an Obsidian note, the editor works one of two ways,
//! chosen per task:
//!
//! - **Additional** (the default): the notes stay Takt's, synced with the
//!   task, and the note is drawn read-only beside them. Nothing on disk is
//!   touched, so this is the one that can lose nothing.
//! - **Same as the Obsidian file**: the editor edits the `.md` file in the
//!   vault, so the note is the notes. Takt's own notes are left as they
//!   were, and come back on switching again.
//!
//! The choice is this Mac's (`Prefs::notes_same_as_obsidian`, task id to
//! the note's address): the file is only on this Mac, so the choice means
//! nothing on another device, and keeping it here needs no schema change.
//!
//! Whatever it edits, the editor holds the text it last read or wrote
//! (`saved`) and compares the source with it each second and before each
//! save, so a change made elsewhere is never written over unasked: with
//! nothing typed it is taken in, and with something typed the editor says
//! so and waits for ⌘S (keep yours) or ⌘R (take theirs, yours to the
//! clipboard).

use super::plugins::Plugin;
use super::{App, Editing};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// How the editor's pane is split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    /// Text on the left, the drawn Markdown on the right.
    #[default]
    Split,
    Edit,
    Preview,
}

impl Layout {
    fn next(self) -> Self {
        match self {
            Layout::Split => Layout::Edit,
            Layout::Edit => Layout::Preview,
            Layout::Preview => Layout::Split,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Layout::Split => "Split",
            Layout::Edit => "Edit",
            Layout::Preview => "Preview",
        }
    }
}

/// The Obsidian note a task links, as this Mac finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedNote {
    pub url: String,
    /// Its file name without `.md`.
    pub name: String,
    pub vault: String,
    /// Where it is on disk; none when its vault isn't one this Mac knows.
    pub path: Option<PathBuf>,
}

/// What the source said when it was last looked at, against `saved`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    InStep,
    /// The note isn't there (yet, or any more): ⌘S writes it.
    Missing,
    /// It was changed elsewhere while something was typed here: theirs.
    Changed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesEditor {
    pub task_id: String,
    pub title: String,
    pub note: Option<LinkedNote>,
    /// Editing the note's file rather than Takt's notes.
    pub same: bool,
    pub layout: Layout,
    /// In additional mode, the right-hand side draws the note rather than
    /// the text being typed.
    pub showing_note: bool,
    /// What was last read from, or written to, what is being edited.
    pub saved: String,
    pub source: Source,
    /// The file's stamp when `saved` was read, to look again only when it
    /// moves.
    stamp: Option<(SystemTime, u64)>,
    /// The note's text, drawn beside Takt's notes in additional mode.
    pub note_text: Option<String>,
    /// Where the keyboard goes back to.
    pub came_from: super::Region,
}

impl NotesEditor {
    /// What the editor is editing, for its header.
    pub fn editing_what(&self) -> String {
        match (&self.note, self.same) {
            (Some(note), true) => format!("{}.md in {}", note.name, note.vault),
            _ => "Takt's notes".into(),
        }
    }
}

/// `vault` and `file` from an `obsidian://open?vault=…&file=…` address, or
/// the absolute `path=` one form of it carries.
pub fn parse_obsidian(url: &str) -> Option<(String, String, Option<PathBuf>)> {
    let rest = url.get("obsidian://".len()..)?;
    if !url[.."obsidian://".len()].eq_ignore_ascii_case("obsidian://") {
        return None;
    }
    let (action, query) = rest.split_once('?')?;
    if !action.trim_end_matches('/').eq_ignore_ascii_case("open") {
        return None;
    }
    let mut vault = String::new();
    let mut file = String::new();
    let mut path = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = decode(value);
        match key {
            "vault" => vault = value,
            "file" => file = value,
            "path" => path = Some(PathBuf::from(value)),
            _ => {}
        }
    }
    (!file.is_empty() || path.is_some()).then_some((vault, file, path))
}

/// Percent-decoding, as `obsidian_link_url` encodes.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut ix = 0;
    while ix < bytes.len() {
        if bytes[ix] == b'%'
            && let Some(hex) = text.get(ix + 1..ix + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            ix += 3;
            continue;
        }
        out.push(bytes[ix]);
        ix += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The note's file in `vaults`: the vault of that name, the file in it,
/// with `.md` when the address leaves it off. None for a canvas or a vault
/// this Mac doesn't have.
pub fn resolve(url: &str, vaults: &[PathBuf]) -> Option<LinkedNote> {
    let (vault, file, absolute) = parse_obsidian(url)?;
    let with_md = |p: PathBuf| {
        if p.extension().is_none() {
            p.with_extension("md")
        } else {
            p
        }
    };
    let path = match absolute {
        Some(path) => Some(with_md(path)),
        None => vaults
            .iter()
            .find(|dir| {
                dir.file_name()
                    .is_some_and(|n| n.to_string_lossy() == vault)
            })
            .map(|dir| with_md(dir.join(&file))),
    };
    let named = path
        .clone()
        .unwrap_or_else(|| with_md(PathBuf::from(&file)));
    if !named
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
    {
        return None;
    }
    Some(LinkedNote {
        url: url.to_string(),
        name: named
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        vault,
        path,
    })
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// The file written whole or not at all: beside it, then renamed over it.
fn write_file(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.takt-saving"));
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path)
}

/// What the source holds now.
enum Read {
    Text(String),
    Missing,
}

impl App {
    pub fn notes_editor_on(&self) -> bool {
        self.plugin_on(Plugin::MarkdownEditor)
    }

    /// The vaults the link picker reads, without walking them.
    fn note_vaults(&self) -> Vec<PathBuf> {
        if self.plugin_on(Plugin::Obsidian) {
            self.obsidian_vaults()
        } else {
            Vec::new()
        }
    }

    /// The task's first link to an Obsidian note (not a folder or canvas),
    /// or the one its "same" choice names when it still links it.
    pub fn linked_note(&self, task_id: &str) -> Option<LinkedNote> {
        let links = self.store.core.task_links(task_id.into()).ok()?;
        let vaults = self.note_vaults();
        let chosen = self.prefs.notes_same_as_obsidian.get(task_id);
        let notes: Vec<LinkedNote> = links
            .iter()
            .filter(|l| l.kind == takt_core::links::LinkKind::Obsidian && !l.is_folder)
            .filter_map(|l| resolve(&l.url, &vaults))
            .collect();
        chosen
            .and_then(|url| notes.iter().find(|n| &n.url == url).cloned())
            .or_else(|| notes.into_iter().next())
    }

    /// Whether the task's notes are its Obsidian note, on this Mac.
    pub fn notes_are_obsidian_file(&self, task_id: &str) -> bool {
        self.notes_editor_on()
            && self.prefs.notes_same_as_obsidian.contains_key(task_id)
            && self
                .linked_note(task_id)
                .is_some_and(|n| self.prefs.notes_same_as_obsidian.get(task_id) == Some(&n.url))
    }

    /// The inspector's row under the notes that opens the editor, saying
    /// what it will edit: none with the plugin off.
    pub fn add_notes_editor_stop(&self, stops: &mut Vec<super::inspector::Stop>, task_id: &str) {
        if !self.notes_editor_on() {
            return;
        }
        let Some(at) = stops.iter().position(|s| s.id == "notes") else {
            return;
        };
        let value = match self.linked_note(task_id) {
            Some(note) if self.notes_are_obsidian_file(task_id) => {
                format!("Notes are {}.md · open", note.name)
            }
            Some(note) => format!("Open with {}.md beside", note.name),
            None => "Open in the Markdown editor".into(),
        };
        stops.insert(
            at + 1,
            super::inspector::Stop {
                id: "notes-editor",
                label: "",
                value,
                empty: false,
                kind: super::inspector::StopKind::Action,
                group: false,
                links: Vec::new(),
            },
        );
    }

    /// `nm` / `⌃⌘M`: the selected task's notes, maximised.
    pub fn open_notes_editor(&mut self) {
        if !self.notes_editor_on() {
            self.message =
                Some("Switch on the Markdown editor in Settings → Plugins first.".into());
            return;
        }
        let Some(task_id) = self.selected_task.clone() else {
            return;
        };
        let Ok(snapshot) = self.store.core.editor_snapshot(task_id.clone()) else {
            return;
        };
        let note = self.linked_note(&task_id);
        let mut same = self.notes_are_obsidian_file(&task_id);
        if same && note.as_ref().is_some_and(|n| n.path.is_none()) {
            same = false;
            self.message = Some(format!(
                "The vault {} isn't on this Mac, so these are Takt's notes.",
                note.as_ref().map(|n| n.vault.as_str()).unwrap_or_default()
            ));
        }
        let mut editor = NotesEditor {
            task_id,
            title: snapshot.title.clone(),
            note,
            same,
            layout: self.notes_layout(),
            showing_note: false,
            saved: snapshot.notes,
            source: Source::InStep,
            stamp: None,
            note_text: None,
            came_from: self.region,
        };
        self.load_source(&mut editor);
        self.notes_editor = Some(editor);
        self.editing = Some(Editing::NotesEditor);
    }

    fn notes_layout(&self) -> Layout {
        match self.prefs.notes_editor_layout.as_str() {
            "edit" => Layout::Edit,
            "preview" => Layout::Preview,
            _ => Layout::Split,
        }
    }

    /// Reads what the editor edits into `saved`, and the note beside it.
    fn load_source(&self, editor: &mut NotesEditor) {
        editor.source = Source::InStep;
        match self.read_source(editor) {
            Read::Text(text) => editor.saved = text,
            Read::Missing => {
                editor.saved = String::new();
                editor.source = Source::Missing;
            }
        }
        editor.stamp = self.file_of(editor).and_then(|p| stamp(&p));
        editor.note_text = self.side_note(editor);
    }

    /// The file being edited, in "same" mode.
    fn file_of(&self, editor: &NotesEditor) -> Option<PathBuf> {
        editor
            .same
            .then(|| editor.note.as_ref()?.path.clone())
            .flatten()
    }

    fn read_source(&self, editor: &NotesEditor) -> Read {
        match self.file_of(editor) {
            Some(path) => match std::fs::read_to_string(&path) {
                Ok(text) => Read::Text(text),
                Err(_) => Read::Missing,
            },
            None => match self.store.core.editor_snapshot(editor.task_id.clone()) {
                Ok(snapshot) => Read::Text(snapshot.notes),
                Err(_) => Read::Missing,
            },
        }
    }

    /// The note's text for the side pane, in additional mode.
    fn side_note(&self, editor: &NotesEditor) -> Option<String> {
        if editor.same {
            return None;
        }
        let path = editor.note.as_ref()?.path.as_ref()?;
        std::fs::read_to_string(path).ok()
    }

    /// Writes `text` to what the editor edits, unless what is there moved
    /// on since it was read and `force` isn't set: then it is left alone
    /// and the editor says so. True when what is there is now `text`.
    pub fn save_notes_editor(&mut self, text: &str, force: bool) -> bool {
        let Some(mut editor) = self.notes_editor.take() else {
            return false;
        };
        let saved = self.save_source(&mut editor, text, force);
        self.notes_editor = Some(editor);
        saved
    }

    fn save_source(&mut self, editor: &mut NotesEditor, text: &str, force: bool) -> bool {
        let now = self.read_source(editor);
        let theirs = match &now {
            Read::Text(t) => Some(t.clone()),
            Read::Missing => None,
        };
        if theirs.as_deref() == Some(text) {
            editor.saved = text.to_string();
            editor.source = Source::InStep;
            editor.stamp = self.file_of(editor).and_then(|p| stamp(&p));
            return true;
        }
        // Moved on elsewhere since it was read: not over it unasked.
        if let Some(theirs) = &theirs
            && *theirs != editor.saved
            && !force
        {
            editor.source = Source::Changed(theirs.clone());
            self.message = Some(format!(
                "{} changed while you were typing: ⌘S keeps yours, ⌘R takes it.",
                self.source_name(editor)
            ));
            return false;
        }
        // Gone, with nothing typed since: not made again unasked.
        if theirs.is_none() && text == editor.saved && !force {
            return true;
        }
        let written = match self.file_of(editor) {
            Some(path) => write_file(&path, text).map_err(|e| e.to_string()),
            None => self.save_takt_notes(&editor.task_id, text),
        };
        match written {
            Ok(()) => {
                editor.saved = text.to_string();
                editor.source = Source::InStep;
                editor.stamp = self.file_of(editor).and_then(|p| stamp(&p));
                true
            }
            Err(error) => {
                self.message = Some(format!(
                    "Couldn't save {}: {error}",
                    self.source_name(editor)
                ));
                false
            }
        }
    }

    fn source_name(&self, editor: &NotesEditor) -> String {
        match (&editor.note, editor.same) {
            (Some(note), true) => format!("{}.md", note.name),
            _ => "The notes".into(),
        }
    }

    /// The task's notes, as the inspector saves them: one undo step.
    fn save_takt_notes(&mut self, task_id: &str, text: &str) -> Result<(), String> {
        let previous = self.selected_task.replace(task_id.to_string());
        let notes = text.to_string();
        self.edit_selected(|edit| edit.notes = notes);
        self.selected_task = previous;
        match self.message.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Esc, ⌘↩ or the command again: saved, and back to the inspector.
    /// What could not be saved comes back, for the clipboard.
    pub fn close_notes_editor(&mut self, text: &str) -> Option<String> {
        let saved = self.save_notes_editor(text, false);
        let editor = self.notes_editor.take();
        self.editing = None;
        self.inspected = None;
        if let Some(editor) = &editor {
            self.region = editor.came_from;
        }
        if saved {
            return None;
        }
        let name = editor
            .as_ref()
            .map(|e| self.source_name(e))
            .unwrap_or_default();
        self.message = Some(format!(
            "{name} was left as it is: what you typed is on the clipboard."
        ));
        Some(text.to_string())
    }

    /// ⌘⇧O: from Takt's notes to the note's file, or back. What was typed
    /// is saved first; the text to edit next comes back.
    pub fn switch_notes_source(&mut self, text: &str) -> Option<String> {
        let editor = self.notes_editor.as_ref()?;
        let Some(note) = editor.note.clone() else {
            self.message =
                Some("Link an Obsidian note first (pl): then its file can be the notes.".into());
            return None;
        };
        if !editor.same && note.path.is_none() {
            self.message = Some(format!("The vault {} isn't on this Mac.", note.vault));
            return None;
        }
        if !self.save_notes_editor(text, false) {
            return None;
        }
        let mut editor = self.notes_editor.take()?;
        editor.same = !editor.same;
        editor.showing_note = false;
        if editor.same {
            self.prefs
                .notes_same_as_obsidian
                .insert(editor.task_id.clone(), note.url.clone());
        } else {
            self.prefs.notes_same_as_obsidian.remove(&editor.task_id);
        }
        self.prefs.save();
        self.load_source(&mut editor);
        self.message = Some(if editor.same {
            format!(
                "The notes are now {}.md. Takt's own notes are kept: ⌘⇧O shows them again.",
                note.name
            )
        } else {
            format!("Takt's own notes, with {}.md beside them.", note.name)
        });
        let next = editor.saved.clone();
        self.notes_editor = Some(editor);
        Some(next)
    }

    /// ⌘R after a change elsewhere: theirs, in place of what was typed.
    pub fn take_their_notes(&mut self) -> Option<String> {
        let mut editor = self.notes_editor.take()?;
        let Source::Changed(theirs) = std::mem::take(&mut editor.source) else {
            self.notes_editor = Some(editor);
            return None;
        };
        editor.saved = theirs.clone();
        editor.stamp = self.file_of(&editor).and_then(|p| stamp(&p));
        self.notes_editor = Some(editor);
        self.message = Some("Took the saved version; yours is on the clipboard.".into());
        Some(theirs)
    }

    /// Each second while it is open: has what it edits moved on? With
    /// nothing typed (`text` is what was read) the new text comes back to
    /// edit; with something typed, the editor says so.
    pub fn poll_notes_editor(&mut self, text: &str) -> Option<String> {
        let mut editor = self.notes_editor.take()?;
        let reloaded = self.poll_source(&mut editor, text);
        if !editor.same && editor.note.is_some() {
            editor.note_text = self.side_note(&editor);
        }
        self.notes_editor = Some(editor);
        reloaded
    }

    fn poll_source(&mut self, editor: &mut NotesEditor, text: &str) -> Option<String> {
        if let Some(path) = self.file_of(editor) {
            let now = stamp(&path);
            if now == editor.stamp && now.is_some() {
                return None;
            }
            editor.stamp = now;
        }
        match self.read_source(editor) {
            Read::Missing => {
                if self.file_of(editor).is_some() {
                    editor.source = Source::Missing;
                }
                None
            }
            Read::Text(theirs) if theirs == editor.saved => {
                if editor.source == Source::Missing {
                    editor.source = Source::InStep;
                }
                None
            }
            Read::Text(theirs) if text == editor.saved => {
                editor.saved = theirs.clone();
                editor.source = Source::InStep;
                self.message = Some(format!(
                    "{} changed elsewhere; the editor has the new text.",
                    self.source_name(editor)
                ));
                Some(theirs)
            }
            Read::Text(theirs) if theirs == text => {
                editor.saved = theirs;
                editor.source = Source::InStep;
                None
            }
            Read::Text(theirs) => {
                if editor.source != Source::Changed(theirs.clone()) {
                    editor.source = Source::Changed(theirs);
                }
                None
            }
        }
    }

    /// ⌘E: split, the text alone, the drawn Markdown alone.
    pub fn cycle_notes_layout(&mut self) {
        let Some(editor) = self.notes_editor.as_mut() else {
            return;
        };
        editor.layout = editor.layout.next();
        self.prefs.notes_editor_layout = editor.layout.name().to_lowercase();
        self.prefs.save();
    }

    /// ⌃Tab, in additional mode: the right side draws the note, or the
    /// text being typed.
    pub fn toggle_notes_side(&mut self) {
        let Some(editor) = self.notes_editor.as_mut() else {
            return;
        };
        if editor.same || editor.note.is_none() {
            return;
        }
        editor.showing_note = !editor.showing_note;
        if editor.layout == Layout::Edit {
            editor.layout = Layout::Split;
        }
    }

    /// ⌘O: the note in Obsidian.
    pub fn open_notes_in_obsidian(&mut self) {
        match self.notes_editor.as_ref().and_then(|e| e.note.as_ref()) {
            Some(note) => super::pickers::open_address(&note.url),
            None => self.message = Some("No Obsidian note is linked to this task.".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_obsidian_address_back_to_its_vault_and_file() {
        let url = takt_core::links::obsidian_link_url(
            "My Vault".into(),
            "Projects/Plan ü.md".into(),
            false,
        );
        let (vault, file, path) = parse_obsidian(&url).expect("an open address");
        assert_eq!(vault, "My Vault");
        assert_eq!(file, "Projects/Plan ü.md");
        assert_eq!(path, None);
        let search = takt_core::links::obsidian_link_url("V".into(), "Folder".into(), true);
        assert_eq!(parse_obsidian(&search), None, "a folder is a search");
        assert_eq!(parse_obsidian("https://example.com/?file=x"), None);
    }

    #[test]
    fn resolves_in_the_vault_of_that_name_and_adds_md() {
        let vaults = vec![PathBuf::from("/v/Life"), PathBuf::from("/v/Work")];
        let note = resolve("obsidian://open?vault=Work&file=Plans%2FQ3", &vaults).unwrap();
        assert_eq!(note.path, Some(PathBuf::from("/v/Work/Plans/Q3.md")));
        assert_eq!(note.name, "Q3");
        let elsewhere = resolve("obsidian://open?vault=Other&file=Q3.md", &vaults).unwrap();
        assert_eq!(elsewhere.path, None, "a vault this Mac doesn't have");
        assert_eq!(
            resolve("obsidian://open?vault=Work&file=Board.canvas", &vaults),
            None
        );
    }

    #[test]
    fn a_file_is_written_whole_and_its_folder_made() {
        let dir = std::env::temp_dir().join(format!("takt-notes-{}", uuid::Uuid::new_v4()));
        let path = dir.join("sub/Note.md");
        write_file(&path, "# One").unwrap();
        write_file(&path, "# Two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Two");
        let left: Vec<_> = std::fs::read_dir(dir.join("sub")).unwrap().collect();
        assert_eq!(left.len(), 1, "nothing left beside it");
        let _ = std::fs::remove_dir_all(dir);
    }
}
