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
  type it fuzzy-matches notes and folders in your Obsidian vaults and the
  Drive files already linked or picked before; "Pick from Google Drive…"
  opens Google's own file picker in the browser for anything else. A
  pasted address is offered as the first row. `↑`/`↓` choose, `Return`
  links, `Tab` cycles the source (all, Obsidian, Drive), `Esc` closes.
- **Opening**: `gx` / `Cmd+O` opens the first link, as before.
- **Rows** that have links show a small link mark.

The Rust app (`desktop/`, installed as `Takt.app`) does the same in
`desktop/src/state/links.rs`: the picker searches the vaults' notes and
folders as you type, and offers Drive when Settings → Plugins → Google
Drive → "Link Google Drive files you pick" is on. The vaults are the
Obsidian plugin and Drive the Google Drive plugin; either switched off
leaves the picker. Its vault folders added by hand and the
Drive switch are its own settings (`desktop.json`). The Swift app's version is
`Takt/Plugins/Native/Links/` (`NativeLinksPlugin`), retired with it.

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

Takt asks Google for one Drive scope, `https://www.googleapis.com/auth/drive.file`,
which reaches only the files a person picks in Google's Picker (or that Takt
made). It is non-sensitive, so it needs neither Google's verification of a
restricted scope nor the yearly CASA security assessment that
`drive.metadata.readonly` (what Drive search used to ask for) would need past
100 users. The cost is that nothing can list or search all of a Drive: a file
is first linked by picking it.

**The Picker.** Google shows its JavaScript Picker only on a web page, no
longer inside an embedded web view, and GPUI has no web view anyway, so every
client uses Google's
[Picker for desktop and mobile apps](https://developers.google.com/workspace/drive/picker/guides/desktop-mobile-picker):
an OAuth consent for `drive.file` alone, with `prompt=consent` and
`trigger_onepick=true` (plus `allow_multiple` and `allow_folder_selection`),
that shows the person their Drive and comes back with the ids picked as
`picked_file_ids` beside the code. Each id is then read with Drive API v3
`files.get` (`fields=id,name,mimeType,webViewLink`) and linked through the
client's ordinary "Add Link" path, with the name and type going into
`linkDetailsJSON`. The address stored is the `webViewLink` when it is a
`https://docs.google.com/` or `https://drive.google.com/` one, else
`https://drive.google.com/file/d/<id>/view` (`…/drive/folders/<id>` for a
folder). This flow needs no API key: only the OAuth client, and the Google
Picker API enabled on its project.

Google lets `drive.file` be asked for only alone in the Picker, so it is a
sign-in of its own, kept apart from Calendar's and Tasks':

- **Mac**: the browser, back to the loopback listener Google sign-in uses
  (`google::pick_drive_files` in `desktop/src/google/mod.rs`, then
  `desktop/src/google/drive.rs`), through the person's own OAuth client ID
  (Settings → Google). The token is its own keychain item (`google-drive`);
  disconnecting Google forgets both.
- **iPhone**: the sign-in sheet (`ASWebAuthenticationSession`) against the
  iOS OAuth client `GOOGLE_IOS_CLIENT_ID` that Calendar uses
  (`GoogleCalendarAuth.drive()`, `Features/Inspector/DriveLinkButton.swift`),
  in its own keychain item.
- **Android**: the Authorization API with `drive.file`,
  `setOptOutIncludingGrantedScopes(true)`, `Prompt.CONSENT` and the
  `PICKER_OAUTH_TRIGGER`, `PICKER_ALLOW_MULTIPLE` and
  `PICKER_ALLOW_FOLDER_SELECTION` resource parameters (play-services-auth
  21.6), against the package's Android OAuth clients as Calendar is
  (`settings/GoogleDriveAuth.kt`, `ui/inspector/InspectorDriveLink.kt`,
  `:data`'s `GoogleDriveFiles`). The ids come back in the result's
  `tokenResponseParams`.

Where a client has no OAuth client to sign in with (no client ID on the Mac,
no `GOOGLE_IOS_CLIENT_ID` in an iPhone build, an Android build whose package
and signing key aren't registered), "Pick from Google Drive" says Drive
linking isn't set up.

**Finding a file again.** The Mac's picker matches what is typed against
every Drive link the workspace already holds, by its stored name, which needs
no Google access; and, once a Drive token exists, against Drive's
`files.list` by name, which under `drive.file` answers only for files picked
before. A file linked once is found by name from then on.

**Moving off the old scope.** A Mac signed in while Drive search existed
holds `drive.metadata.readonly`. Google adds every scope granted before to a
new consent, so a grant can't be narrowed: the next "Connect Google" revokes
the old grant first and asks afresh for Calendar and Tasks. The app says so
once, at launch, when it finds the old scope (`links_drive_legacy_told` in
`desktop.json`), and Settings → Links says "connect Google again to drop the
old Drive access" until it's gone.

Console steps (project `takt-511416`):

1. APIs & Services → Library → **Google Picker API** → Enable, and
   **Google Drive API** → Enable (for `files.get` and `files.list`).
2. Google Auth Platform → Data access: remove
   `.../auth/drive.metadata.readonly`, and add `.../auth/drive.file`
   (non-sensitive).
3. The OAuth clients Calendar uses serve Drive too: the Mac's Desktop
   client, the iOS client and the Android clients. No API key is needed for
   this Picker.

## Phones

Both phones show a task's links in the inspector, drawn by the core, and open
them (Android `Intent.ACTION_VIEW`, iOS `openURL`). Adding is by pasting an
address into the inspector's link field, or into any add field, or with
"Pick from Google Drive" under the field, which runs Google's Picker (above)
and links each file picked by name and type. The inspector's draft is saved
first, so an address being typed isn't lost under the new links.

## Not yet

- MCP and CLI tools for links by name: they can already set addresses
  (`external_links`); naming them would need a tool entry, a CLI command, a
  read/write classification in `core/src/agent_tools.rs` and the tool count.
- Reading a link out of a retitled task on the phones.
