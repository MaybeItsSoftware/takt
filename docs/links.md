# Links

A task can link to a Google Drive file or folder (a Doc, a Sheet, Slides, a
PDF, anything), an Obsidian note or folder, or any web page. Links sync with
the task to every device, open in the browser or in Obsidian, and are added
from a keyboard-first picker on the Mac or by pasting an address anywhere.

## Data

Two columns on `task_metadata`, both synced and journalled:

| Column | Holds |
| --- | --- |
| `externalLinksJSON` | The addresses, in the order they were added. Unchanged: every client already read, opened and edited it, and the CLI and MCP tools set it as `external_links`. |
| `linkDetailsJSON` (`v31_link_details`) | What the picker knew when it made a link, keyed by address: `{"<url>": {"title": "Budget", "mimeType": "application/vnd.google-apps.spreadsheet"}}`. NULL for nothing. |

Keeping the addresses where they were means nothing that predates links
breaks: an older client drops the new column on pull and still lists and
opens every link, it just draws them by address. A link is never a
`{kind, target, …}` record in the list, because older clients would try to
open it.

The kind, the title, the line under it and the icon are not stored: the Rust
core reads them off the address each time (`core/src/links.rs`,
`link_describe`), sharpened by the stored name and type.

| Address | Kind | Drawn as |
| --- | --- | --- |
| `docs.google.com/document/d/…`, `/spreadsheets/`, `/presentation/`, `/forms/` | drive | Google Doc, Sheet, Slides, Form |
| `drive.google.com/drive/folders/…` | drive | Drive folder |
| `drive.google.com/file/d/…` | drive | Drive file (PDF, image… from the stored type) |
| `obsidian://open?vault=V&file=path/Note.md` | obsidian | the note's name, "V · path" under it |
| `obsidian://search?vault=V&query=path:"Folder"` | obsidian | a folder: Obsidian's URI scheme cannot open one, so it opens a search scoped to it |
| anything else `http(s)` | web | the last path component or the host |

The core's `CoreWorkspace` has `task_links`, `linked_task_ids`,
`add_task_link` ("Add Link", one undo step, renames an address already there)
and `remove_task_link` ("Remove Link"). `link_from_text` says whether pasted
text is a link; `obsidian_link_url` builds a note or folder address.

### Pasting an address

The add field's parser (`core/src/capture.rs`) reads a trailing `http(s)://`
or `obsidian://` word as a link, like a trailing `#tag`: `Review the budget
https://docs.google.com/spreadsheets/d/…` files "Review the budget" with the
Sheet linked. An address alone names the task for what it points at (an
Obsidian note's name, "Google Doc"). This works in every add field on every
client, because they all parse through the core. On the Mac, editing a title
in place reads the same endings, so an address typed after a title joins the
task's links.

## Mac

- **Inspector**: the Links row lists each link with its icon, title and
  subtitle. Click or `Return` opens it; `x` removes it; "Add link…" opens the
  picker. The raw address list is still editable under "Edit addresses".
- **Picker** (`pl`, or `Cmd+Ctrl+L`, palette "Link…"): one field. As you
  type it fuzzy-matches notes and folders in your Obsidian vaults and, when a
  Google account with Drive search is connected, searches Drive by name. A
  pasted address is offered as the first row. `↑`/`↓` choose, `Return`
  links, `Tab` cycles the source (all, Obsidian, Drive), `Esc` closes.
- **Opening**: `gx` / `Cmd+O` opens the first link, as before.
- **Rows** that have links show a small link mark.

The Rust app (`desktop/`, installed as `Takt.app`) does the same in
`desktop/src/state/links.rs`: the picker searches the vaults' notes and
folders as you type, and Drive by name when Settings → Links → "Search Google
Drive in the link picker" is on. Its vault folders added by hand and the
Drive switch are its own settings (`desktop.json`). The Swift app's version is
`Takt/Plugins/Native/Links/` (`NativeLinksPlugin`), with a settings page for
the vaults and Drive search.

### Obsidian vaults

The vaults are the ones Obsidian itself knows
(`~/Library/Application Support/obsidian/obsidian.json`), plus the vault
holding the daily log's notes folder (the nearest ancestor with an
`.obsidian` folder), plus any added by hand on the settings page. The index
is a walk of each vault's Markdown files and folders, skipping dot-folders,
refreshed when the picker opens.

### Google Drive

Pasting a Drive, Docs or Sheets address needs no Google access at all, and is
the path that always works.

Searching Drive by name uses Drive API v3 `files.list` with `q=name contains
'…' and trashed = false`, through the shared `GoogleAccount`, and needs the
`https://www.googleapis.com/auth/drive.metadata.readonly` scope. It is off
until switched on in the Links settings, because asking for it makes the
next Google sign-in ask for it too.

Console steps (project `takt-511416`), only if Drive search is wanted:

1. APIs & Services → Library → **Google Drive API** → Enable.
2. OAuth consent screen → Data access → Add scope →
   `.../auth/drive.metadata.readonly`.
3. In Takt: Settings → Links → switch on Drive search, then sign in to Google
   again (an existing sign-in does not grow new scopes).

`drive.metadata.readonly` is a **restricted** scope: an app in production
using it needs Google's verification (and possibly a security assessment).
In testing mode with yourself as a test user it works without that. The
non-sensitive alternative is the **Google Picker API** with the `drive.file`
scope: Google's own file chooser, which grants access only to the files the
user picks. It needs a browser surface (a web view on the Mac) and an API key,
so it is a follow-up rather than what ships.

## Phones

Both phones show a task's links in the inspector, drawn by the core, and open
them (Android `Intent.ACTION_VIEW`, iOS `openURL`). Adding is by pasting an
address into the inspector's link field, or into any add field.

## Not yet

- MCP and CLI tools for links by name: they can already set addresses
  (`external_links`); naming them would need a tool entry, a CLI command, a
  read/write classification in `core/src/agent_tools.rs` and the tool count.
- Reading a link out of a retitled task on the phones.
- Google Picker with `drive.file`.
