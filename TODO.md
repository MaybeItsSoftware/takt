# TODO

Outstanding work and open decisions. Ordered by who can act on it: decisions
only Adam can make, then checks that need a pair of hands, then code.

Things get deleted from here when they are done, not ticked — `git log` is the
record of what happened.

## Decisions owed

### What `→` should do within Today

Currently unbound. The candidate is "open the selected task in its list",
mirroring `→` in the outline meaning "go in" — but that switches view mode,
which is a lot for an arrow key, and it makes `←` not a true inverse.

**Recommendation: leave it unbound.**

### The typed-command language

`Sources/TaktCore/CommandEngine.swift` parses `due tomorrow 9am`, `tag x`,
`matrix do` and much more — about 1,400 tested lines with exactly one caller,
`AppCoordinator.executeCommandInput`, which nothing calls. It was deliberately
left out of the `Cmd+K` palette: it is a different interaction (arguments,
natural-language dates) and bolting a text grammar onto a row list would make
both worse. `README.md` marks it as currently unreachable.

Either give it a `:` prefix inside the palette that switches to text-command
mode, or delete it.

**Recommendation: the `:` prefix.** The parser is good and the due-date
language is genuinely faster than a picker.

### Agent worktrees — remove them

The three `.claude/worktrees/agent-*` checkouts (about 4.2 GB) and the five
`worktree-agent-*` branches hold nothing that is not already on `main` by
patch (`git cherry main <branch>` lists no `+` for any of them, checked
2026-10-07). Removing them is a one-liner that only you can run here:

```bash
for w in .claude/worktrees/agent-*; do git worktree remove --force "$w"; done
git worktree prune
git branch -D $(git branch --list 'worktree-agent-*' | tr -d ' *+')
```

## Checks that need a pair of hands

These cannot be verified from a build; they need someone to click.

- **The done rail.** `⌘⌃D`. It should open on the right of the inspector with the
  keyboard already in it, `↑↓`/`kj` should walk the list across day headings as
  one sequence, `⏎` should land on the task in its own list with completions no
  longer hidden, `R` should put it back on the list, and `←` should hand the
  keyboard back to the work. Pressing `⌘⌃D` again from inside it closes it;
  pressing it while the keyboard is elsewhere should bring the keyboard over
  rather than close the rail.
- **The rail's window is five weeks** (`WorkspaceViewModel.doneRailWindowDays`).
  Say if that is the wrong depth — too short to see a pattern, or long enough to
  feel like an archive.
- **Cancelled tasks appear in the rail**, drawn apart from completed ones. Say if
  they should not be there at all: they leave the open list the same way, which
  is the argument for listing them, but they are not progress.
- **One flat surface.** Sidebar, main pane and status bar are all
  `theme.paper` now, told apart only by the hairline in each resize handle and
  the rule over the status bar — the Zed arrangement. Check the sidebar still
  reads as its own pane without the `altRow` tint, and that the selection wash
  reads on paper.
- **Timeline then a list.** Click Timeline, then click any list. The list should
  open. This was silently broken.

- **The focus handoff.** Start a block from a board card. The window should
  close, the Dock icon should go, and the tray should come up with the running
  task at the top. Score the block on the tray: the window should stay closed.
  `Window → Priority` should reopen on the focus screen, not the plan.
- **A daily starting itself** should leave the window alone — only deliberate
  starts close it.
- **Menu-bar block controls.** Click the status item mid-block. Expect the task
  title, a clock row, Pause/Resume, Done and End Block. **Done** should raise
  the tray and ask how the block went, not silently close it.
- **The sidebar's two cues.** Arrow onto Focus: the list you were reading should
  stay open behind it, the ring should read as "you are here" and the fill as
  "this is open". If that distinction is too fine to carry, the fill should go
  and the cursor row should simply be unmistakable. These now carry the whole
  job: the current row no longer thickens its name, so if it reads too quietly
  the resting fill is one figure (`WorkspaceSelection.restingFill`, 0.13).
- **`⌘↑` / `⌘↓` on a nested list.** That path did nothing at all before, so it
  is the least-exercised thing in the sidebar work.
- **Folder scope.** Is the board the right mode to land in, or should it keep
  whichever mode you were already in? And the new-task picker inside a folder
  should offer only that folder's lists.
- **The chrome migration.** Key caps, the hint bars under the focus screens and
  the sidebar ring all changed rendering path, even on Chalk. Nothing should
  look different; check that nothing does.
- **The outline's last `←`.** With no task selected and Everything already
  showing, `←` drops into the sidebar. That may be one press too many.
- **The one selection treatment, on every surface.** A selected row now carries
  a ring when its region has the keyboard and only a fill when it does not. Six
  surfaces changed to get there — the outline, Today, the board, search, the
  palette, the move sheet — and the figures came from the sidebar, where they
  were tuned as a pair. Worth arrowing between the sidebar and the task surface
  to check the ring reads as "this responds to me" rather than as noise.
- **The menu keys, all of them.** Nineteen literal `keyboardShortcut` calls
  became catalogue lookups, so a wrong lookup is a menu item with the wrong key
  or none. Open every menu and check what it prints beside each item.
- **⌘⌃S and the inspector toggle.** Both were menu-only and are now catalogue
  entries, so they also answer from the window monitor and appear in the
  palette. Check they do not fire while you are typing in a field.
- **The three new keys.** `⌘⌃C` removes the column the selected card is in,
  `o` on the focus ladder drops your own order, and both now print their key in
  their tooltip. `⌘⌃C` is deliberately not `⇧⌘⌫`, which still deletes the list.
- **The one header band, in all five modes.** `⌘1`→`⌘2`→`⌘3`→`⌘4`→`⌘8`→`⌘9`
  in sequence: nothing should move but the content. The band is now one line
  (subtitle on the title's baseline, not under it) with a fixed minimum
  height, so a mode with a subtitle no longer sits taller than one without.
  The title is the theme's `title` role — 15pt semibold on Chalk, down from a
  17pt literal — so if the scope name reads too quiet, that is one figure in
  the theme's type scale, not in the view.
- **The inspector's new order.** Title, then Start focus, then Notes, Plan,
  When, Filing, Links, Structure, Save. Worth checking the order matches how
  you actually work through a task, because it was guessed from what the
  controls are rather than from watching anyone use them.
- **The board column's two rings.** Drag a card over a column, then arrow into
  one. The drop target is the primary hue at 2pt; the keyboard is the focus ring
  at a hairline. They used to be the same 2pt accent ring.
- **Did anything want the fifth view mode?** `WorkspaceViewMode.focus` and its
  dashboard are gone. Nothing reached them, but if a muscle-memory key lands on
  a pane that is now missing, that is where it went.

### One on-screen control still has no key

The subtask disclosure on a board card is `@State` local to the card, so no
command can reach it — the model does not know which cards are open. Giving it
a key means moving that state onto the workspace, keyed by task id, which is
worth doing but is not a tooltip change.

## Code

### Surfaces that still ignore the theme

What renders through it now: the shared primitives (`MicroLabel`, `KeyCap`,
`KeyHint`, `FocusRule`, `WorkspacePaneHeader`, `SheetTitle`,
`InspectorSection`), the toolbar, the board card and column, the one selection
treatment everywhere it is drawn, the timeline including its per-task hues, the
focus screens, the matrix, the inspector, the sidebar and its drop targets,
the task composer and quick capture, every overlay (palette, search, move,
quick edit, list finder), Settings and the plugin settings pages.

What does not: the focus quality prompt and most of the day tray (`DayView`),
where the remaining `Color.accentColor` uses are concentrated. A useful count
while migrating: `grep -c accentColor Takt/*.swift`.

The older accent-preset / `AppThemeColorToken` mechanism is gone: the theme
specification is the one way the app is coloured, and the reader's fonts are a
`ThemeTypographyOverride` laid over it (see `docs/plugins.md`).

### Fonts on the phones

The Mac's interface, heading and numeral fonts and text size are a per-Mac
override. The iPhone and Android apps bundle the same families but have no
equivalent control yet; the override is a `TaktCore` value, so the iPhone
could adopt it directly, and Android would need a Kotlin port of the merge.

### Theme values that were guessed

Worth an eye from someone who owns the palette:

- **Dark `altRow`, `borderMuted` and `inputBorder`** have no stated hex in the
  house style. They were derived from the grape ramp as `#211f29`, `#2d2b38`
  and `#403d4d`. These now carry more weight than they did, because Chalk Dark
  is a theme someone may sit in all day rather than only what Chalk becomes
  after sunset.
- **How status accents are scored.** Emerald is 2.08:1 on chalk and amber
  1.56:1, so holding every accent to 4.5:1 would condemn the house palette.
  The audit instead holds only `primary` to 3:1 — it carries focus rings and
  selection edges — and records any accent under 4.5:1 as a *note* saying it is
  unsafe for body copy. Success, danger and warning are always a tinted fill
  plus border plus text of the same hue, never a bare mark on paper, so this is
  defensible; the stricter alternative is to composite the tint over the paper
  and measure against that, which is more work and would still fail for amber.

### Known-good backlog, not yet claimed

- SwiftLint's standing warnings. With the Swift Mac app (`Takt/`) retired on
  2026-11-08 the count over `Sources/` is **0 warnings, 0 errors**; the test
  folders carry `line_length` warnings and one `line_length` error
  (`corelogic-tests/AgentAntigravityProtocolTests.swift:55`, 420 characters),
  which fails CI's lint job until it is wrapped. **This is the single place that number
  lives** — `ci.yml` and `ARCHITECTURE_IMPROVEMENT_PLAN.md` point here rather
  than carrying their own copy. Do not add to it;
  `ARCHITECTURE_IMPROVEMENT_PLAN.md` tracks the decomposition that would clear
  it.
