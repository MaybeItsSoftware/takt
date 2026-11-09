# The Markdown editor

A built-in plugin on the Mac (Settings → Plugins → Markdown editor, on by
default): a task's notes maximised into a pane the size of the window, the
text on the left in the monospaced face and wrapped at words, the Markdown
drawn on the right. When the task links an Obsidian note, the notes can be
that note.

The code is `desktop/src/state/notes_editor.rs` (what it edits, saving and
conflicts), `desktop/src/state/markdown.rs` (the preview's reading of
Markdown) and `desktop/src/ui/notes_editor.rs` (the pane and its keys).

## Opening it

- `nm` or `Cmd+Ctrl+M` on a task (`taskMaximiseNotes` in the catalogue,
  palette "Open the notes in the Markdown editor").
- `Cmd+Ctrl+M` from inside the inspector's notes field: what was typed there
  is saved and carried in.
- The inspector row under the notes, which also says what the editor will
  edit: "Open in the Markdown editor", "Open with Plan.md beside", or "Notes
  are Plan.md · open".

With the plugin off the row goes, the palette greys the command and the keys
say to switch it on.

## Its keys

| Key | What |
| --- | --- |
| `Esc`, `Cmd+Return`, `Cmd+Ctrl+M` | Save and return to where the keyboard was |
| `Cmd+S` | Save and stay. After a change elsewhere, keep yours (write over it) |
| `Cmd+R` | After a change elsewhere: take theirs; what you typed goes to the clipboard |
| `Cmd+E` | Split, the text alone, the preview alone (remembered) |
| `Cmd+Shift+O` | Additional to the Obsidian note, or the same as it |
| `Ctrl+Tab` | In additional mode: the right side draws the note, or your text |
| `Cmd+O` | Open the note in Obsidian |
| `Tab` / `Shift+Tab` | Indent the line by two spaces / take them away (lists nest by it) |

Every other chord stops at the editor, so nothing behind it moves while the
text is unsaved. The header's buttons do the same with the mouse.

The preview draws headings, paragraphs line by line (as Obsidian shows a
single line break), bullet and numbered lists with their nesting, checkboxes,
quotes and callouts, fenced code, rules, a note's `---` properties, and
inline bold, italic, code, strikethrough, `==highlights==`, links,
`[[wiki links]]` and `#tags`. It is for reading by, not a second Obsidian:
tables, embeds and maths are drawn as their text.

## Additional, or the same as the Obsidian file

A task links an Obsidian note through the link picker (`pl`, see
[links.md](links.md)): `obsidian://open?vault=V&file=path/Note.md`. The
editor finds the file through the vaults the Obsidian plugin knows
(Obsidian's own `obsidian.json` and those added in Settings), by the vault's
folder name, adding `.md` when the address leaves it off. A folder or canvas
link is not a note. With several notes linked, it is the first, or the one
"same" was chosen for.

- **Additional (the default).** The notes are Takt's, synced with the task
  to every device, and the note is drawn read-only beside them (`Ctrl+Tab`),
  read again each second. Nothing on disk is touched.
- **Same as the Obsidian file.** The editor edits the `.md` file in the
  vault: the note is the notes. Takt's own notes are left exactly as they
  were and come back on switching again (`Cmd+Shift+O`), which also saves
  what was typed first.

Additional is the default because it cannot lose anything: the other mode
writes a file another app owns, so it is something you choose, task by task.

The choice is kept on this Mac, in `desktop.json`
(`notes_same_as_obsidian`: task id to the note's address), not in the
workspace. The file exists only on this Mac, so "same" means nothing to the
phones or another Mac, and keeping it here needs no schema change. A task
whose chosen note is unlinked, or whose vault isn't on this Mac, opens on
Takt's notes and says why.

## Saving and conflicts

The editor holds the text it last read from, or wrote to, what it edits, and
compares the source with it each second and before every save:

- **Changed elsewhere, nothing typed here:** the new text is taken in, and the
  status line says so.
- **Changed elsewhere while you typed:** a banner says so and nothing is
  written. `Cmd+S` keeps yours, `Cmd+R` takes theirs (yours to the
  clipboard). Closing then leaves the file alone and puts your text on the
  clipboard.
- **The file isn't there** (not made yet, or deleted while open): a banner
  says so; type and save to make it, folders and all. Opening and closing
  without typing never makes one.

A file is written beside itself and renamed over, so Obsidian never reads it
half written. Takt's notes are saved as the inspector saves them: one undo
step, synced.

## Phones

Neither the iPhone nor Android has plugins or the Obsidian vaults (a phone
opens an `obsidian://` link in Obsidian, and has no files of the vault), so
neither has the editor. The command is in the shared catalogue; the iPhone's
context menu leaves it out and Android takes only the commands it runs. A
phone shows and edits Takt's notes as before, and a task in "same" mode on
the Mac still has its own notes there. A phone version would be a Markdown
preview of the notes in the inspector, which needs no vault.
