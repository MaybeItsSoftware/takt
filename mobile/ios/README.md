# Takt for iPhone and iPad

The native iOS app. It links the repo's own Swift package (`../..`), so the
data layer is the Mac's: `WorkspaceStore` (GRDB) from `TaktWorkspace`, the
editor drafts from `TaktWorkspaceEditing`, the ranking, day plan, capture
syntax and command catalogue from `TaktCore`, and sync from `TaktSync`.
Nothing under `Sources/` is re-implemented here.

## Build and run

```bash
brew install xcodegen                      # once
cd mobile/ios && xcodegen                  # the .xcodeproj is generated, not committed
xcodebuild -project TaktMobile.xcodeproj -scheme TaktMobile \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro' build test

../../scripts/install_ios.sh               # Release build, install + launch on the iPhone 17 Pro simulator
DEVICE_ID=<udid> ../../scripts/install_ios.sh   # a connected device (xcrun devicectl list devices)
```

Run `xcodegen` again after adding or removing a file.

The scheme's tests are the unit tests (`TaktMobileTests`, the view models
over a throwaway workspace), the widget render tests (`TaktWidgetsTests`,
which render the widgets and the Live Activity's lock-screen view from the
extension's sources and leave PNGs in `/tmp/iosshots` when that directory
exists) and the UI tests (`TaktMobileUITests`): an add → subtask → fold →
complete → undo smoke, and a scroll measurement over a 5,000-task outline
(it seeds the list first, so it takes a couple of minutes).

DEBUG launch arguments: `-uiTesting` (a fresh workspace in a temp directory),
`-demo` (a small sample workspace), `-seed5000`, and `-open <takt://…>` to
start on a screen, e.g. `-open takt://lists/Work`.

## Layout

| Path | What |
|---|---|
| `project.yml` | XcodeGen spec: app `TaktMobile` (product "Takt", bundle id `uk.co.maybeitssoftware.takt`), `TaktWidgets`, tests |
| `Shared/` | Compiled into the app and the widget: app group paths, widget snapshot, Live Activity attributes, widget palette, quick-add intent |
| `TaktMobile/Model/` | `WorkspaceModel` (the one write funnel, `revision`), `StoreQuery`, `AppNavigation`, task and list commands |
| `TaktMobile/Features/` | One folder per screen: Lists, Outline, Board, Matrix, Today, Focus, Review, Search (the Find tab: commands, lists and tasks; see `docs/mobile-ux.md`), Inspector, QuickAdd, History, Settings (incl. sync) |
| `TaktMobile/App/` | Entry point, iPhone tabs / iPad split view, deep links, keyboard shortcuts |
| `TaktMobile/Bridge/` | Widget snapshot writer and the focus Live Activity |
| `TaktMobile/Intents/` | "Add task to Takt" App Intent and Siri phrases |
| `TaktWidgets/` | Next-up and Today-count widgets, Live Activity / Dynamic Island, Control Center quick add |

The database lives in the app group `group.uk.co.maybeitssoftware.takt`
(`Takt/priority.sqlite`), so the widgets and intents read the same file.

## How data stays current

Every write goes through `WorkspaceModel.perform`, which bumps `revision`.
Each screen reads its own scope on a background task keyed on
`(revision, scope)` and lands the result only if it changed, so only what is
on screen is read. Writes from other processes (the add-task intent, sync)
are picked up by polling `PRAGMA data_version` while the app is active, and
sync's `onRemoteChanges` bumps `revision` directly.

`WorkspaceStore` keeps its `DatabasePool` internal, so GRDB `ValueObservation`
can't be attached from outside the package. A public observation hook on the
store would let the screens use it instead of the revision counter.

## Design

The Mac's Zed look: IBM Plex Sans for UI, Lilex for numbers and clocks
(bundled from `shared/fonts` with their OFL licences), sentence-case muted
labels, hairlines instead of shadows, and the Chalk palette, read from
`BuiltInThemeSpecifications.chalk` and following the system's light or dark
mode. Navigation, sheets, swipe actions and haptics stay native.
