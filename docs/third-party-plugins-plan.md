# Third-party plugins

A plan, written 2026-11-08. Nothing here is built. It describes how plugins
written by other people could run in the Mac app (`desktop/`), sandboxed as
WebAssembly in the way Zed runs its extensions, and what that would cost.

## Where it starts from

The integrations are already **built-in plugins**
(`desktop/src/state/plugins.rs`): Google Calendar, Google Tasks, Google
Drive, Checkvist, Obsidian, MCP and the completion celebration. Each has an
id, a name, a one-line description, a switch kept in `desktop.json`, and its
own settings rows, which Settings → Plugins shows beneath the switches under
the plugin's name. A third-party plugin should look exactly like one of
those to the person using it: a row with a switch, its settings beneath,
and nothing of it left running once it is off.

The other thing already in place is the **tool table**, `cli/src/tools.rs`.
The CLI, the terminal UI and the MCP server are three front ends onto it, it
writes through the Rust core inside an undo step, and `core/src/agent_tools.rs`
says which tools only read and which write. A plugin is a fourth front end.
It gets no access the table doesn't offer, and it gets no write the undo
history doesn't record.

## What a plugin is

A folder:

```
my-plugin/
  plugin.toml     # id, name, version, description, author, capabilities
  plugin.wasm     # a WebAssembly component built against takt's WIT world
  settings.toml   # optional: the settings rows it declares
  README.md
```

`plugin.toml`:

```toml
id = "todoist-import"           # permanent, lowercase, unique in the registry
name = "Todoist import"
version = "0.3.1"
description = "Copy your Todoist projects into the workspace."
authors = ["Someone <someone@example.com>"]
takt = ">=0.9"                  # the host API versions it was built for

[capabilities]
tools = ["workspace_tree", "workspace_tasks", "workspace_task_add"]
network = ["api.todoist.com"]
secrets = ["todoist-token"]
folder = false                  # asks for one folder the person chooses
background = "every-15-minutes" # or "none"

[contributes]
commands = [{ id = "import", title = "Import from Todoist" }]
link_provider = false           # offers rows in the link picker (pl)
```

The wasm is a **component** (the WebAssembly component model, a WIT
interface), run by `wasmtime` inside the app's process, as Zed does. A
component has no ambient access: no files, no sockets, no clock beyond
what the host passes in. Everything it can do is a host function it
imports, and the host checks each call against the capabilities it was
granted.

### The host interface

One WIT world, versioned (`takt:plugin@1`):

- `call-tool(name, arguments-json) -> result<json, error>`: a tool from
  the table, by the same name and with the same arguments as `takt` on the
  command line. Only tools named under `capabilities.tools`.
- `http-fetch(request) -> result<response, error>`: the host makes the
  request, only to the hosts under `capabilities.network`, HTTPS only. The
  plugin never holds a socket.
- `secret-get(key)`, `secret-set(key, value)`: a keychain item of the
  plugin's own (`uk.co.maybeitssoftware.takt.plugin.<id>`), only the keys it
  declared. One plugin cannot read another's, or the app's.
- `setting-get(key)`: its own declared settings' values.
- `folder-*`: reads and writes under the one folder the person chose for it,
  if it asked for one (a WASI preopened directory, nothing above it).
- `log(level, message)`: into Diagnostics, under the plugin's name.
- `notify(message)`: the status line.

And the exports the host calls:

- `run-command(id)`: a contributed command, from the palette.
- `tick()`: background work, at the declared cadence, with a time budget
  (a few seconds of fuel; over it the call is stopped and the plugin
  marked as failing).
- `on-event(event)`: task completed, task added, day started. Read-only
  notifications; a plugin that wants to act on one calls a tool.
- `link-search(query) -> list<candidate>`: if it is a link provider, rows in
  the link picker beside Obsidian's and Drive's.
- `setting-pressed(key)`: a button among its settings rows.

### Reaching the tool table

The table is in the CLI crate, and the app cannot link the CLI. Two ways
round that:

1. **Through the bundled helper (first).** The app already ships `takt` at
   `Contents/Helpers/takt`, and that binary is an MCP server. The plugin host
   starts one helper per running plugin (or one shared, with the plugin's id
   on each call) and turns `call-tool` into an MCP `tools/call`. Writes land
   as "MCP: …" undo steps today; the host would pass a label so they read
   "Plugin: Todoist import — …". No new code in the table, and the same
   checks the MCP server already makes.
2. **Through the core (later, if the helper's latency shows).** Move the
   dispatch in `tools.rs` that only touches the workspace into `core/`,
   which the app links. The Checkvist and day-log tools would stay in the
   CLI.

Either way the classification in `core/src/agent_tools.rs` is what decides
whether a call needs asking about (below), so a new tool is safe by default:
unclassified counts as a write.

## Permissions

Asked at three moments, never more often than that:

- **Installing.** A sheet lists what the plugin asks for, in plain words:
  "Read your lists and tasks", "Add and change tasks", "Talk to
  api.todoist.com", "Keep a sign-in in your keychain", "Read and write one
  folder you choose", "Run every 15 minutes". Install, or don't.
- **The first write.** Read tools named in the manifest are granted at
  install. The first call to a write tool shows a card like the agent
  panel's approval card, with the tool and its arguments, and the choice of
  *Allow once*, *Always allow for this plugin* or *Deny*. Allowed writes are
  undoable like any other, as one step per call.
- **An update that asks for more.** A new version with new capabilities is
  installed switched off until the person reads the difference and agrees.

Settings → Plugins shows each third-party plugin's grants under its
settings, and each can be taken back. The "Switch off all AI" switch has no
bearing on plugins; it would only if a plugin declared that it sends tasks
to a model, which the manifest would say (`sends_to_ai = true`) and the
master switch would then stop.

## Installing

1. **A folder (first).** Settings → Plugins → "Install from folder…" takes a
   folder with a `plugin.toml` and a built `plugin.wasm`. This is how a
   plugin's author works on it, and how anyone can run one they built from
   source. Installed copies live in
   `~/Library/Application Support/Takt/plugins/<id>/`. A folder install is
   unsigned and says so on its row.
2. **A registry (maybe).** A browsable list in the same settings page, with
   install and update. See *Hosting* for where it would live.

The plugin's switch is the same `Prefs::plugins` map the built-ins use, keyed
`ext:<id>` so a third-party id can never shadow a built-in one. Switched
off, the host never instantiates it: no tick, no events, no commands in the
palette, no rows in the link picker.

Plugins are this Mac's, not the workspace's: they don't sync. Two Macs can
run different plugins on the same workspace, and a phone sees only what
the plugins wrote.

## Signing

Plugins are not native code, so Apple's code signing and notarisation don't
apply to them; the app's own signature covers the runtime that runs them.
What needs signing is the claim "this is the file the registry reviewed":

- Each release is a `.tar.gz` of the folder. The registry index records its
  SHA-256 and an **Ed25519 signature** (minisign format) over it, made by
  the registry's key. The app embeds that public key and refuses a
  registry download whose hash or signature doesn't match.
- The index itself is signed the same way, so a compromised mirror can't
  list a different file.
- Authors may also sign with a key of their own, recorded the first time
  their plugin is accepted. An update signed by a different author key is
  held for a human to look at.
- Folder installs skip all of this and are labelled unsigned.

One app-side cost: wasmtime compiles wasm to machine code at run time, and
under the hardened runtime that needs the
`com.apple.security.cs.allow-jit` entitlement, which the app doesn't have
today (`scripts/install_desktop.sh` signs with `--options runtime` and no
entitlements). The alternative is wasmtime's Pulley interpreter, or
`wasmi`, which need no entitlement and are slower, but fast enough for
plugins that mostly wait on the network. Start with the interpreter. Turn
on JIT only if a plugin needs it.

## The Mac App Store

A Mac App Store build is being considered. App Review Guideline 2.5.2
says an app may not download, install or execute code that introduces or
changes its features, and the App Sandbox would also fence in the folder
and keychain access the host gives plugins. So:

- **Third-party plugins are DMG-only.** The App Store build is compiled
  without the host (a `third-party-plugins` cargo feature, off there).
  Its Settings → Plugins lists the built-ins and nothing else, with no
  "Install" button and no registry.
- The built-in plugins are unaffected: they are the app's own code, and
  switching one off is a setting.
- The phones get none of this. iOS has the same rule, and Google Play's
  policy on downloading executable code is close enough that it isn't worth
  testing. A plugin's work reaches the phones as the tasks it wrote, over
  sync.

## Hosting

- **A registry on GitHub releases: free.** One public repository
  (`takt-plugins`) holds `index.json` and its signature. Each plugin is a
  pull request that adds its entry. CI in that repository builds the wasm
  from the author's tagged source, signs the archive, and attaches it to a
  GitHub release. The app fetches the index from
  `raw.githubusercontent.com` (or GitHub Pages) and the archives from the
  release download URLs. Public repositories get Actions minutes and release
  bandwidth at no charge, and GitHub's limit of 2 GiB per release asset is
  far above any plugin. The only cost is the time spent reviewing pull
  requests.
- **A hosted registry: costs.** Search, download counts, author accounts and
  publishing without a pull request need a service. On the stack already in
  use, that is a Cloud Run service (billed per request, so close to nothing
  while it is quiet) in front of the existing Supabase Postgres, with the
  archives in object storage. Cloudflare R2 has no egress fees and charges
  cents per GB per month to store files. At hobby scale that is a few pounds
  a month. The bigger cost is the work: accounts, abuse handling, takedowns
  and uptime that someone has to own. Don't build it before the GitHub
  registry has more plugins than a pull request queue can handle.

## Steps, if it goes ahead

1. The WIT world and a host behind the `third-party-plugins` feature, run by
   the interpreter, with `call-tool` through the bundled helper. Folder
   installs only. One example plugin in the repository.
2. Permission sheet, write approval, the grants shown in Settings → Plugins.
3. Contributed commands in the palette, settings rows, `tick` and events.
4. Link providers in the picker.
5. The GitHub registry: index, signing in CI, browse and update in the app.
6. Only if needed: JIT, tools in the core, a hosted registry.
