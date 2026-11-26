//! The catalogue proper: every key the desktop workspace answers to, once.
//!
//! Ordering within a group is roughly most-pressed-first, because the
//! palette shows this order when nothing has been typed and the reference
//! sheet reads top to bottom. A new command needs a case in the Mac's
//! `WorkspaceCommandID` (and its `run` arm) and an entry here; a test on each
//! side holds the two to one another. Keys are spelled as `keymap.rs` says.
//! Moved from `Sources/TaktCore/WorkspaceCommandCatalog+Entries.swift`.

use super::CommandSurface::*;
use super::{Spec, c};

pub(super) fn specs() -> Vec<Spec> {
    [
        go(),
        task(),
        focus(),
        plan(),
        lists(),
        window(),
        timeline(),
        done(),
        conditions(),
        motions(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn go() -> Vec<Spec> {
    vec![
        c("goToday", "Go to Today", "Go", &["cmd+t"]),
        // Up and down the sidebar's lists without going to the sidebar, and
        // Zed's previous / next tab (`cmd-{` / `cmd-}`), a list being what a tab is.
        c(
            "goPreviousList",
            "Go to the list above",
            "Go",
            &["cmd+option+up", "cmd+shift+{"],
        ),
        c(
            "goNextList",
            "Go to the list below",
            "Go",
            &["cmd+option+down", "cmd+shift+}"],
        ),
        // Zed's `pane::GoBack` / `GoForward`: the lists you were just in.
        c("goBack", "Back to the list you were in", "Go", &["ctrl+-"])
            .note("Like a browser's back: again goes further back"),
        c("goForward", "Forward again", "Go", &["ctrl+shift+-"]),
        // The three ways to draw a list, on ⌘1–⌘3. Today is a list, in the
        // sidebar beside Everything and the inbox, not a way of drawing one, so
        // it is ⌘T rather than a fourth view.
        c("goBoard", "Show as a board", "Go", &["cmd+1"]),
        c("goOutline", "Show as an outline", "Go", &["cmd+2"]),
        c("goMatrix", "Show as a matrix", "Go", &["cmd+3"]),
        c(
            "goEverything",
            "Open Everything",
            "Go",
            &["cmd+e", "cmd+0", "gh"],
        )
        .note("Every task across all active lists"),
        c("goInbox", "Open the Inbox", "Go", &["cmd+i"]).note("Where quick capture lands"),
        c("goFocus", "Open the focus panel", "Go", &["cmd+8"])
            .note("Pick a task and start it, or bring back the block already running"),
        c("goTimeline", "Show the day's timeline", "Go", &["cmd+9"]).note(
            "A tab of the bottom dock, with the keyboard in it; again from inside it puts it away",
        ),
        c(
            "goListNavigator",
            "Find or create a list",
            "Go",
            &["cmd+p", "gl"],
        )
        .note("Zed's file finder, for lists"),
        c(
            "goSearch",
            "Search every task",
            "Go",
            &["cmd+f", "cmd+shift+f", "/"],
        )
        .note("Titles and notes. ⇧⌘F is Zed's find in project"),
        c(
            "goCommandPalette",
            "Show the command palette",
            "Go",
            &["cmd+shift+p", "cmd+k"],
        )
        .also(Sidebar, &[":"])
        .note(
            "⇧⌘P as in Zed; ⌘K as it always was. : in the sidebar, as in Zed's vim project panel",
        ),
        c(
            "goKeyboardReference",
            "Show the keyboard reference",
            "Go",
            &["cmd+/", "?"],
        )
        .note("The same list this palette shows"),
        c(
            "goSidebarRegion",
            "Focus the sidebar",
            "Go",
            &["ctrl+1", "cmd+shift+e"],
        )
        .note("⇧⌘E is Zed's focus the project panel"),
        c("goTaskRegion", "Focus the task surface", "Go", &["ctrl+2"]),
        c(
            "goInspectorRegion",
            "Focus the inspector",
            "Go",
            &["ctrl+3"],
        ),
        c(
            "goCycleRegion",
            "Cycle between regions",
            "Go",
            &["ctrl+tab", "ctrl+shift+tab"],
        )
        .motion(),
    ]
}

fn task() -> Vec<Spec> {
    vec![
        c("taskNew", "Add a task", "Task", &["enter", "cmd+n"])
            .note("Return always adds, as in Checkvist: below the selected task, at the foot of the day on Today"),
        c("taskNewAbove", "Add a task above", "Task", &["shift+enter", "option+enter"])
            .note("⇧↩ is Checkvist's"),
        c("taskNewChild", "Add a subtask", "Task", &["tab", "option+shift+enter"])
            .note("Tab on its own; while typing a new task Tab nests it under the one above"),
        // Space always completes, as in Checkvist; Return always adds. One
        // meaning each, on every surface, so neither has to be looked up.
        c("taskComplete", "Complete or reopen the task", "Task", &["space", "x"]),
        c("taskInvalidate", "Cancel or reinstate the task", "Task", &["shift+space"])
            .note("Cancelled, rather than done — it stopped mattering"),
        c("taskDelete", "Delete the task", "Task", &["delete", "cmd+delete", "cmd+shift+k"])
            .note("Takes its subtasks with it. ⇧⌘K is Zed's delete line"),
        c("taskRename", "Rename the task", "Task", &["ee", "f2", "cmd+ctrl+e"]),
        c("taskRenameAppend", "Edit the title, typing at the end", "Task", &["ea"])
            .note("Like vim's a: the cursor lands after the last character"),
        c("taskRenameInsert", "Edit the title, typing at the start", "Task", &["ei"])
            .note("Like vim's i: the cursor lands before the first character"),
        c("taskReplace", "Replace the title", "Task", &["r", "shift+r"])
            .note("Like vim's R in Zed: the title is cleared and what you type replaces it. Esc keeps the old one"),
        c("taskEditDue", "Edit the due date", "Task", &["dd", "cmd+d"]),
        c("taskEditNotes", "Edit the notes", "Task", &["nn", "cmd+ctrl+n"]),
        // The Markdown editor plugin, on the Mac. `n` started only `nn`.
        c("taskMaximiseNotes", "Open the notes in the Markdown editor", "Task", &["nm", "cmd+ctrl+m"])
            .note("The notes full size, with a preview; or the linked Obsidian note itself. Esc returns. From inside the notes field too"),
        c("taskEditTags", "Edit the tags", "Task", &["tt", "cmd+ctrl+t"]),
        c("taskEditRecurrence", "Edit how it repeats", "Task", &["dr", "cmd+ctrl+r"]),
        c("taskEditStart", "Edit the start date", "Task", &["ds", "option+s"]),
        c("taskEditEstimate", "Edit the time estimate", "Task", &["option+t"]),
        c("taskDueToday", "Due today", "Task", &["td", "cmd+option+t"]),
        c("taskDueTomorrow", "Due tomorrow", "Task", &["tm", "cmd+shift+t"]),
        // `b` starts only `bb`, so `bu` was free.
        c("taskBury", "Bury until tomorrow", "Task", &["bu"])
            .note("Starts it tomorrow: greyed below its siblings until then, and on Today when it arrives"),
        c("taskClearDue", "Clear the due date", "Task", &["cd"]),
        // `c` starts only `cc`, `cd`, `cn` and `ct`, so `cs` was free.
        c("taskClearStart", "Clear the start date", "Task", &["cs"])
            .note("Unburies it: back among its siblings at once"),
        c("taskClearNotes", "Clear the notes", "Task", &["cn"]),
        c("taskClearTags", "Clear the tags", "Task", &["ct"]),
        c("taskClearPriority", "Clear the priority", "Task", &["0"])
            .note("0 is Checkvist's spelling"),
        // Priority -1, as `!-1` writes it: behind every task with no priority,
        // and last among its siblings. `-` is free off the sidebar, where it
        // is Zed's select parent. It waits briefly, as the start of `-1`–`-4`.
        c("taskSendToBack", "Send to the back (priority −1)", "Task", &["-"])
            .note("Ranks it behind everything without a priority and moves it to the bottom of its list. -2 to -4 go just ahead of it"),
        c("taskToggleDaily", "Commit to this periodically, or edit the commitment", "Task", &["cmd+shift+d"])
            .note("Asks for a name, how often and an end, then adds a subtask that comes round in Dailies"),
        // `gb`, not Checkvist's `hh`: no sequence starts with `h`, so `h` leaves
        // a task's subtasks at once instead of waiting to see if a second follows.
        c("taskHabit", "Make a habit from the task, or edit its habit", "Task", &["gb", "cmd+shift+h"])
            .note("How often, which column, an estimate, and when it ends. With nothing selected, a habit of its own"),
        // `w` was free, so `ww` is a sequence of its own.
        c("taskWaiting", "Waiting on: who, and when to follow up", "Task", &["ww"])
            .note("Moves the task to Waiting on. At the follow-up time, if it is still waiting, a follow-up lands in Today"),
        // `b` was free too: `bb` for "blocked by", and a chord for the menu.
        c("taskAddBlocker", "Blocked by: wait on another task", "Task", &["bb", "cmd+shift+b"])
            .note("Finds the task it waits on. Until that is done it is greyed, sinks below its siblings and is never next up"),
        // `c` starts only `cd`, `cn` and `ct`, so `cc` was free.
        c("taskConditions", "Conditions: what the task needs before it is worth starting", "Task", &["cc"])
            .note("Type to find one, Space or Return ticks it, ! ticks it as not holding (\"not Home\"), a new name makes it. With no task selected, the Conditions tab"),
        // ⌃T rather than T: bare `t` starts `td`, `tm` and `tt`, and ⌘T goes to
        // Today.
        // `a` starts nothing on a task (only the Conditions tab's own `a`),
        // so `aa`, for "ask the agent", is free.
        c("taskAskAgent", "Ask the agent to do something to the task", "Task", &["aa"])
            .note("A one-line popup: say what to do, and it does it, with an Undo and the keys that would have done the same"),
                c("taskTogglePlannedToday", "Plan for today, or take it off", "Task", &["ctrl+t"])
            .note("Puts it in the day you arrange by hand. On Today, plans an overdue or dated row"),
        c("taskStartFocus", "Start focus on the task", "Task", &["gf"])
            .note("Or add it to the queue behind a running block"),
        c("taskMove", "Move the task or list", "Task", &["mm", "cmd+shift+m"])
            .note("To any list, or under any task, with everything under it"),
        c("taskConvertToList", "Convert to a list, or back", "Task", &["cmd+shift+l"]),
        c("taskPromoteList", "Promote or unpin a nested list", "Task", &["cmd+option+p"]),
        // `ex`, not Checkvist's `xx`: `x` completes, and as the first letter of
        // `xx` it was held 1.2 s every time, and a second `x` to reopen a task
        // ticked by mistake took its branch out with no question asked. `e`
        // already starts `ee`, `ea` and `ei`, so `ex` holds nothing new.
        c("taskExtractBranch", "Extract the branch as a list", "Task", &["ex"]),
        c("taskOpenLink", "Open the task's first link", "Task", &["gx", "cmd+o"]),
        // `p` starts only `pc`; `ll` would shadow Today's and the sidebar's `l`.
        c("taskAddLink", "Link…: a Drive file, an Obsidian note, a page", "Task", &["pl", "cmd+ctrl+l"])
            .note("Type to find a note in your vaults or a file in Drive, or paste an address"),
        c("taskShowProgress", "Show the task's progress", "Task", &["pc"]),
        c("taskToggleInspector", "Open or close the inspector", "Task", &["i", "sd"]),
        // `go`, as `o` opens a task where it lives from the done rail and the
        // timeline; `g` started only `ga gb gf gg gh gl gw gx`.
        c("taskGoToList", "Go to the task's list", "Task", &["go", "cmd+shift+o"])
            .note("Opens the list it belongs to with the task selected — from Today, a filter list, Everything or a folder"),
        // The modifier says what kind of move it is. Bare arrows move the cursor
        // and ⌘ moves it far; ⌥ moves the task within its list (↑↓ among its
        // siblings, ←→ out of or into the one above, as an outliner does); ⇧⌥
        // carries it somewhere else (←→ a board column, ↑↓ another list). ⌃ is
        // left alone: macOS gives ⌃ and the arrows to Spaces and Mission Control.
        c("taskIndent", "Indent the task", "Task", &["option+right", "cmd+option+right"])
            .note("Under the task above it"),
        c("taskOutdent", "Outdent the task", "Task", &["shift+tab", "option+left", "cmd+option+left"]),
        // ⌥↑ and ⌥↓ are Zed's move line; ⌘↑ and ⌘↓ go to the ends, as they do
        // in an editor.
        // On Today there are no siblings to move among, so they arrange the day;
        // on the matrix they move the task within its quadrant.
        c("taskMoveUp", "Move the task up", "Task", &["option+up"])
            .note("On Today, earlier in the day you planned; on the matrix, up its quadrant"),
        c("taskMoveDown", "Move the task down", "Task", &["option+down"])
            .note("On Today, later in the day you planned; on the matrix, down its quadrant"),
        c("taskMoveToPreviousList", "Move the task to the list above", "Task", &["option+shift+up"])
            .note("The list above its own in the sidebar. mm picks any list"),
        c("taskMoveToNextList", "Move the task to the list below", "Task", &["option+shift+down"])
            .note("The list below its own in the sidebar. mm picks any list"),
    ]
}

fn focus() -> Vec<Spec> {
    vec![
        c("focusFinish", "Finish the focus block", "Focus", &["cmd+shift+enter"])
            .note("Completes its task and asks how it went, as Done does. Works from inside a field"),
        c("focusStop", "Stop the focus block", "Focus", &["cmd+shift+."])
            .note("Ends the session: the time so far is logged unscored, the task stays open and the queue is dropped"),
        // Beside ⌘8, which opens the panel these conditions decide what it offers.
        c("focusSetAvailable", "Set what's available now…", "Focus", &["cmd+option+8", "ga"])
            .note("The right dock's Conditions tab, with the keyboard in it: the conditions that hold here — Home, Campus, Floor space. A task that needs one waits until it is on"),
        // "Go where": the place finder, without leaving the work.
        c("focusSetPlace", "Say where you are…", "Focus", &["gw"])
            .note("A finder of the places, and Anywhere: the place you choose replaces the last one. Tasks that need somewhere else wait"),
        c("focusManageConditions", "Manage conditions…", "Focus", &[])
            .note("The Conditions tab, on its new-condition row: add, rename or archive the places and circumstances tasks can require"),
    ]
}

fn plan() -> Vec<Spec> {
    vec![
        c(
            "planEnterTask",
            "Open the task",
            "Plan",
            &["right", "shift+right", "]"],
        )
        .note("Its subtasks, as a board"),
        c(
            "planLeaveTask",
            "Leave the task's subtasks",
            "Plan",
            &["left", "shift+left", "["],
        )
        .note("With nothing left to leave, hands the keyboard back to the sidebar"),
        c(
            "planHideCompleted",
            "Hide or show completed tasks",
            "Plan",
            &["tc"],
        ),
        c(
            "planHideLater",
            "Hide or show buried tasks",
            "Plan",
            &["tl"],
        )
        .note("A task whose start is still to come is buried: shown greyed below its blocked siblings until it starts. tl hides them"),
        // Checkvist folds a branch where it stands, so a task's subtasks can be
        // read and walked without opening the task. `za` is vim's fold toggle.
        c(
            "planToggleFold",
            "Show or hide the task's subtasks",
            "Plan",
            &["za"],
        )
        .note("In the outline, or on the board card or row you are on"),
        c("planGrowTask", "Show the task's subtasks", "Plan", &["."])
            .note("Grows a folded branch, then the next layer under it a press at a time. On the board: opens a card to all its subtasks, then every subtask row a layer at a time"),
        c(
            "planShrinkTask",
            "Hide the task's subtasks",
            "Plan",
            &["comma"],
        )
        .note("Folds the deepest open layer under the task first, then the task. On a task with none, folds the task it sits under. On the board: shuts the deepest open subtask rows, then the card back to its preview, then folds it"),
        c(
            "planFoldAll",
            "Hide every task's subtasks",
            "Plan",
            &["cmd+left"],
        )
        .on(Outline),
        c(
            "planUnfoldAll",
            "Show every task's subtasks",
            "Plan",
            &["cmd+right"],
        )
        .on(Outline),
        c(
            "planBoardNewColumn",
            "Add a board column",
            "Plan",
            &["cmd+shift+c"],
        )
        .on(Board),
        c(
            "planBoardMoveCardLeft",
            "Move the card a column left",
            "Plan",
            &["cmd+left", "option+shift+left"],
        )
        .on(Board),
        c(
            "planBoardMoveCardRight",
            "Move the card a column right",
            "Plan",
            &["cmd+right", "option+shift+right"],
        )
        .on(Board),
        c(
            "planBoardRemoveColumn",
            "Delete this board column…",
            "Plan",
            &["cmd+ctrl+c"],
        )
        .on(Board)
        .note("The column you are in; asks where its tasks go. ⇧⌘⌫ still deletes the list"),
        // The rest of a column header's menu, keyless: the palette reaches them,
        // and they act on the column you are in, as ⌃⌘C does.
        c(
            "planBoardRenameColumn",
            "Rename this board column",
            "Plan",
            &[],
        )
        .on(Board),
        c(
            "planBoardMoveColumnLeft",
            "Move this board column left",
            "Plan",
            &["cmd+shift+left"],
        )
        .on(Board),
        c(
            "planBoardMoveColumnRight",
            "Move this board column right",
            "Plan",
            &["cmd+shift+right"],
        )
        .on(Board),
        c(
            "planBoardAddColumnLeft",
            "Add a board column to the left",
            "Plan",
            &[],
        )
        .on(Board),
        c(
            "planBoardAddColumnRight",
            "Add a board column to the right",
            "Plan",
            &[],
        )
        .on(Board),
        c(
            "planBoardEditColumnRules",
            "Edit this board column's rules…",
            "Plan",
            &[],
        )
        .on(Board)
        .note("Tasks the rules match show in the column; dropping one in edits it to match"),
        c(
            "planBoardMakeDefaultColumns",
            "Make these the default board columns",
            "Plan",
            &[],
        )
        .on(Board)
        .note("Every board without columns of its own uses them"),
        c(
            "planBoardResetColumns",
            "Reset this board to the default columns",
            "Plan",
            &[],
        )
        .on(Board),
        c(
            "planMatrixPlace",
            "Place the task in a quadrant",
            "Plan",
            &["option+1", "option+2", "option+3", "option+4"],
        )
        .on(Matrix)
        .motion()
        .note("1 urgent and important, 2 important, 3 urgent, 4 neither"),
    ]
}

fn lists() -> Vec<Spec> {
    vec![
        // The sidebar is Zed's project panel: a list is a file there and a folder
        // a directory, so ⌘N, ⌥⌘N, ⌫ and F2 act on the row, and so do the vim
        // panel's netrw keys — % new file, d new directory, ⇧D delete, ⇧R rename.
        c("listNew", "Create a list", "Lists", &["cmd+shift+n"])
            .also(Sidebar, &["cmd+n", "shift+5"])
            .note("Beside the sidebar row you are on. ⌘N and % (⇧5) in the sidebar, as Zed's new file"),
        // Only in the sidebar: on a task pane F2 renames the task, and ⌘R is the
        // right dock's.
        c("listRename", "Rename the list or folder", "Lists", &[])
            .also(Sidebar, &["f2", "shift+r"])
            .note("From a task pane, the list you are in"),
        c("listSettings", "Open list settings", "Lists", &["oo", "cmd+option+i"]),
        c("listIcon", "Choose the sidebar row's icon", "Lists", &[])
            .also(Sidebar, &["shift+i"])
            .note("A symbol or an emoji, for a list, nested list or folder; a symbol for a filter list"),
        c("listArchive", "Archive the current list", "Lists", &["cmd+option+a"]),
        c("listRestore", "Restore the last archived list", "Lists", &["cmd+option+shift+r"]),
        c("listComplete", "Complete or reopen the list", "Lists", &["cmd+shift+x"]),
        c("listDelete", "Delete the list or folder", "Lists", &["cmd+shift+delete"])
            .also(Sidebar, &["delete", "cmd+delete", "shift+d"])
            .note("Asks first. ⌫ and ⇧D on the sidebar row, as in Zed's project panel"),
        c("listShare", "Share the list", "Lists", &[])
            .note("Copies an invite link for another Takt account; each run makes a new one"),
        c("listJoin", "Join a shared list", "Lists", &[])
            .note("Asks for an invite link or code"),
        c("listMembers", "Show who the list is shared with", "Lists", &[]),
        c("listLeave", "Leave the shared list", "Lists", &[])
            .note("Asks first. The list goes from your devices; the others keep it"),
        c("listStopSharing", "Stop sharing the list", "Lists", &[])
            .note("Asks first. Owner only: the list stays yours and leaves everyone else's"),
        c("taskAssign", "Assign the task to someone in the list", "Lists", &[])
            .note("A task in a shared list. One assigned to someone else leaves your Today and focus"),
        c("folderNew", "Create a folder", "Lists", &["cmd+option+n"])
            .also(Sidebar, &["d"])
            .note("d in the sidebar, as in Zed's vim project panel"),
        c("filterListNew", "Create a filter list", "Lists", &["cmd+option+shift+n"])
            .note("A saved set of rules over every task, under Today, Everything and the Inbox"),
        c("filterListEdit", "Edit the filter list", "Lists", &["cmd+option+shift+e"])
            .note("Its name, icon and rules; makes a new one when no filter list is open"),
        c("folderCollapseAll", "Collapse every folder", "Lists", &["cmd+left"])
            .on(Sidebar),
        c("folderExpandAll", "Expand every folder", "Lists", &["cmd+right"])
            .on(Sidebar),
        c("folderMoveUp", "Move the folder up", "Lists", &["cmd+option+up"])
            .on(Sidebar),
        c("folderMoveDown", "Move the folder down", "Lists", &["cmd+option+down"])
            .on(Sidebar),
        c("folderSelectPrevious", "Select the previous folder", "Lists", &["ctrl+option+up"])
            .also(Sidebar, &["{"]),
        c("folderSelectNext", "Select the next folder", "Lists", &["ctrl+option+down"])
            .also(Sidebar, &["}"]),
        c("listMoveUp", "Move the sidebar row up", "Lists", &["option+up"])
            .on(Sidebar)
            .note("Whatever you are on — a list, a folder, or a nested list"),
        c("listMoveDown", "Move the sidebar row down", "Lists", &["option+down"])
            .on(Sidebar)
            .note("Whatever you are on — a list, a folder, or a nested list"),
        c("listPrioritise", "Prioritise the list", "Lists", &[])
            .note("Last among your priorities, at the top of the lists. Up next leads each board column with them, in order"),
        c("listPriorityUp", "Move the list up your priorities", "Lists", &["option+shift+up"])
            .on(Sidebar)
            .note("A prioritised list, one place more important"),
        c("listPriorityDown", "Move the list down your priorities", "Lists", &["option+shift+down"])
            .on(Sidebar)
            .note("A prioritised list, one place less important"),
        c("listUnprioritise", "Stop prioritising the list", "Lists", &[]),
        c("listNewTaskDestination", "Choose where new tasks go", "Lists", &["cmd+option+[", "cmd+option+]"])
            .motion()
            .note("In a folder, which of its lists a new task lands in"),
    ]
}

fn window() -> Vec<Spec> {
    vec![
        c("windowUndo", "Undo", "Window", &["cmd+z", "uu", "ctrl+z"]),
        c("windowRedo", "Redo", "Window", &["cmd+shift+z", "ctrl+shift+z"]),
        c("windowToggleSidebar", "Show or hide the sidebar", "Window", &["cmd+b", "cmd+ctrl+s"])
            .note("⌘B toggles the left dock, as in Zed"),
        c("windowToggleInspectorPane", "Show or hide the inspector", "Window", &["cmd+ctrl+i"])
            .note("Same pane as i or sd on a task, without selecting one"),
        c("windowToggleRightDock", "Show or hide the right dock", "Window", &["cmd+r", "cmd+option+b"])
            .note("As ⌘B and ⌘J: shown on whichever tab it was on, with the keyboard in it; up without the keyboard, it takes it; from inside it, it goes away. ⌘R works from inside a field too; ⌥⌘B is Zed's toggle right dock"),
        c("windowSyncNow", "Sync now", "Window", &["cmd+shift+r"])
            .note("Takt Sync, and Google when it is on, at once rather than on their next turn. Works from inside a field too"),
        c("windowToggleDoneRail", "Show or hide what you have finished", "Window", &["cmd+ctrl+d"])
            .note("The bottom dock's Done tab: opens it and takes the keyboard; again from inside it closes it"),
        c("windowToggleAgentPanel", "Show or hide the agent panel", "Window", &["cmd+shift+a", "cmd+ctrl+a"])
            .note("Your own Claude Code or Codex in the left dock: reads freely, each change shown with an Undo"),
        // Which agent new threads run, for the palette; the panel's header
        // and Settings ▸ Agent have the same choice.
        c("windowUseClaudeCode", "Use agent: Claude Code", "Window", &[])
            .note("New agent threads run Claude Code; a running thread keeps its agent"),
        c("windowUseCodex", "Use agent: Codex", "Window", &[])
            .note("New agent threads run Codex; a running thread keeps its agent"),
        c("windowToggleProgressDock", "Show or hide the bottom dock", "Window", &["cmd+j"])
            .note("Progress, Done and Timeline, on the tab it was on. ⌘J toggles the bottom dock, as in Zed"),
        c("windowCloseAllDocks", "Close all docks", "Window", &["cmd+option+y"])
            .note("The sidebar, the right dock and the bottom dock, as in Zed"),
        // Zed's previous / next tab, ⌘{ and ⌘}, on the dock's own tabs while the
        // dock has the keyboard; on a task pane the same chords change list, a
        // list being the tab there. [ and ] as well wherever no field takes them.
        c("windowDockNextTab", "Next tab in the dock", "Window", &[])
            .also(Inspector, &["cmd+shift+}", "]"])
            .also(Done, &["cmd+shift+}", "]"])
            .also(Timeline, &["cmd+shift+}", "]"])
            .also(Conditions, &["cmd+shift+}", "]"])
            .note("Inspector and Conditions on the right, Done and Timeline below, and round again, with the keyboard in it"),
        c("windowDockPreviousTab", "Previous tab in the dock", "Window", &[])
            .also(Inspector, &["cmd+shift+{", "["])
            .also(Done, &["cmd+shift+{", "["])
            .also(Timeline, &["cmd+shift+{", "["])
            .also(Conditions, &["cmd+shift+{", "["]),
        c("windowOpenKeymap", "Open the keymap file", "Window", &[])
            .note("keymap.json — your own keys over these. Created empty if it is missing"),
        c("windowReloadKeymap", "Reload the keymap", "Window", &[])
            .note("It reloads by itself when saved; this is for when it did not"),
        c("windowShowDiagnostics", "Show diagnostics", "Window", &[])
            .note("Connection, integration health, and this session's problems"),
        c("windowOpenThemesFolder", "Open the themes folder", "Window", &[])
            .note("Each .json there is a theme in the picker. Created if it is missing"),
        c("windowReloadThemes", "Reload themes", "Window", &[])
            .note("They reload by themselves when saved; this is for when they did not"),
        c("windowExportTheme", "Export the current theme as JSON", "Window", &[])
            .note("A complete, editable copy in the themes folder, put in force"),
    ]
}

fn timeline() -> Vec<Spec> {
    vec![
        c(
            "timelinePreviousDay",
            "Step back a day",
            "Timeline",
            &["left", "h"],
        )
        .on(Timeline),
        c(
            "timelineNextDay",
            "Step forward a day",
            "Timeline",
            &["right", "l"],
        )
        .on(Timeline),
        c("timelineToday", "Return to today", "Timeline", &["t"]).on(Timeline),
        c(
            "motionTimelineSelect",
            "Move through the day's tasks",
            "Timeline",
            &[
                "down", "up", "j", "k", "home", "end", "cmd+up", "cmd+down", "pageup", "pagedown",
            ],
        )
        .on(Timeline)
        .motion()
        .note("The breakdown under the chart, most time first"),
        c(
            "timelineReveal",
            "Open it where it lives",
            "Timeline",
            &["o"],
        )
        .on(Timeline)
        .note("Leaves the timeline and selects the task in its own list"),
        c(
            "timelineClose",
            "Leave the timeline",
            "Timeline",
            &["escape"],
        )
        .on(Timeline)
        .note("Back to the work, leaving it open; ⌘9 or ⌘J puts it away"),
    ]
}

fn done() -> Vec<Spec> {
    vec![
        c(
            "motionDoneSelect",
            "Move through what you finished",
            "Done",
            &[
                "down", "up", "j", "k", "home", "end", "cmd+up", "cmd+down", "pageup", "pagedown",
            ],
        )
        .on(Done)
        .motion(),
        c("doneReveal", "Open it where it lives", "Done", &["o"])
            .on(Done)
            .note("Leaves the rail and selects the task in its own list"),
        c("doneReopen", "Put it back on the list", "Done", &["space"]).on(Done),
        c("doneClose", "Leave the rail", "Done", &["escape", "left"])
            .on(Done)
            .note("Back to the work, leaving the rail open; ⌘⌃D or ⌘J puts it away"),
    ]
}

fn conditions() -> Vec<Spec> {
    vec![
        c(
            "motionConditionsSelect",
            "Move through the conditions",
            "Conditions",
            &[
                "down", "up", "j", "k", "home", "end", "cmd+up", "cmd+down", "pageup", "pagedown",
            ],
        )
        .on(Conditions)
        .motion(),
        c(
            "conditionsToggle",
            "Turn the condition on or off",
            "Conditions",
            &["space"],
        )
        .on(Conditions)
        .note("Location replaces the last place; anything else combines. Off on one a setting provides is off for now, until that setting changes"),
        c(
            "conditionsAdjust",
            "Change where you are, your free time, or when it expires",
            "Conditions",
            &["left", "right", "h", "l"],
        )
        .on(Conditions)
        .note("On Location the place, on Free time a quarter of an hour less or more, on Finish and Forget the choice"),
        c(
            "conditionsTypeTime",
            "Type your free time",
            "Conditions",
            &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"],
        )
        .on(Conditions)
        .note("On the Free time row: 45, 1h30, 17:00 or 5pm, then Return. Return there types one too"),
        c(
            "conditionsEdit",
            "Rename the condition",
            "Conditions",
            &["enter", "f2", "shift+r"],
        )
        .on(Conditions)
        .note("On the new-condition row, puts the caret in its field: Return makes it"),
        c(
            "conditionsNew",
            "New condition",
            "Conditions",
            &["cmd+n", "a"],
        )
        .on(Conditions),
        c(
            "conditionsCategory",
            "File the condition under a category",
            "Conditions",
            &["t"],
        )
        .on(Conditions)
        .note("Location, Equipment… or a new one. Location is one at a time: turning a place on turns the last one off"),
        c(
            "conditionsProvides",
            "Choose what the condition provides",
            "Conditions",
            &["p"],
        )
        .on(Conditions)
        .note("Tick what holds whenever it does: Home provides Floor space and Quiet, the Laptop Typing. Saying it holds turns them on"),
        c(
            "conditionsArchive",
            "Archive or restore the condition",
            "Conditions",
            &["e"],
        )
        .on(Conditions)
        .note("Archived ones are not offered for new requirements; tasks that need one keep it"),
        c(
            "conditionsMoveUp",
            "Move the condition up",
            "Conditions",
            &["option+up"],
        )
        .on(Conditions)
        .note("Within its category; the order is kept for every device"),
        c(
            "conditionsMoveDown",
            "Move the condition down",
            "Conditions",
            &["option+down"],
        )
        .on(Conditions),
        c(
            "conditionsMoveCategoryUp",
            "Move the category up",
            "Conditions",
            &["option+shift+up"],
        )
        .on(Conditions)
        .note("The whole category the cursor is in, above the one before it"),
        c(
            "conditionsMoveCategoryDown",
            "Move the category down",
            "Conditions",
            &["option+shift+down"],
        )
        .on(Conditions),
        c(
            "conditionsRenameCategory",
            "Rename the category",
            "Conditions",
            &["shift+t"],
        )
        .on(Conditions)
        .note("Every condition in it moves with it; a name already in use merges the two. Location keeps its name"),
        c(
            "conditionsMerge",
            "Merge into another condition",
            "Conditions",
            &["m"],
        )
        .on(Conditions)
        .note("Every task that needs this one needs the other instead, and this one is archived"),
        c(
            "conditionsShowTasks",
            "Show the tasks that need it",
            "Conditions",
            &["o"],
        )
        .on(Conditions)
        .note("The open tasks that require the condition, under its row; again hides them"),
        c(
            "conditionsFold",
            "Fold or unfold the category",
            "Conditions",
            &["z"],
        )
        .on(Conditions)
        .note("In the Conditions half; on the Archived row, shows or hides the archived ones"),
        c(
            "conditionsFoldAll",
            "Fold or unfold every category",
            "Conditions",
            &["shift+z"],
        )
        .on(Conditions),
        c(
            "conditionsClear",
            "Clear what's available",
            "Conditions",
            &["c"],
        )
        .on(Conditions)
        .note("Nothing beyond the laptop: only tasks with no conditions are offered"),
        c(
            "conditionsUseAgain",
            "Use the last context again",
            "Conditions",
            &["shift+u"],
        )
        .on(Conditions)
        .note("The conditions that held before the last one expired"),
        c(
            "conditionsClose",
            "Leave the conditions",
            "Conditions",
            &["escape"],
        )
        .on(Conditions)
        .note("Back to the work, leaving the dock open; r puts it away"),
    ]
}

fn motions() -> Vec<Spec> {
    vec![
        c("motionSelectNext", "Select the next task", "Moving around", &["down", "j"])
            .motion(),
        c("motionSelectPrevious", "Select the previous task", "Moving around", &["up", "k"])
            .motion(),
        c("motionSelectEnds", "Jump to the first or last task", "Moving around", &["home", "end", "cmd+up", "cmd+down", "gg", "shift+g"])
            .motion(),
        c("motionSelectPage", "Move eight tasks at a time", "Moving around", &["pageup", "pagedown", "ctrl+u", "ctrl+d"])
            .motion()
            .note("⌃U and ⌃D are vim's half-page up and down"),
        // Vim's f, F and ;, over rows: the letter typed after f is the first
        // letter of the row to land on. In the sidebar they find a list.
        c("motionFindForward", "Find the next task starting with a letter", "Moving around", &["f"])
            .also(Sidebar, &["f"])
            .motion()
            .note("Vim's f: press f, then the letter. In the sidebar it finds a list"),
        c("motionFindBackward", "Find the previous task starting with a letter", "Moving around", &["shift+f"])
            .also(Sidebar, &["shift+f"])
            .motion()
            .note("Vim's F"),
        c("motionFindRepeat", "Repeat the last find", "Moving around", &[";"])
            .also(Sidebar, &[";"])
            .motion()
            .note("Vim's ;, the same letter in the same direction"),
        // Vim's :12, over rows. In the sidebar : is still the palette, as in
        // Zed's vim project panel, because a list has no number.
        c("motionGoToNumber", "Go to a task by its number", "Moving around", &[":"])
            .motion()
            .note("Vim's :12 — type the number, then Return. Today numbers its rows; elsewhere the numbers show while you type"),
        c("motionSidebarSelect", "Move through the sidebar", "Moving around", &["down", "up", "j", "k", "home", "end", "cmd+up", "cmd+down", "gg", "shift+g", "pageup", "pagedown", "ctrl+u", "ctrl+d"])
            .on(Sidebar)
            .motion()
            .note("Every row, including Focus and the timeline; Home and End, or gg and ⇧G, are the two ends"),
        c("motionSidebarExpand", "Open the row you are on", "Moving around", &["right", "l"])
            .on(Sidebar)
            .motion()
            .note("Focus and the timeline open their screens, a folder expands, a list takes the keyboard"),
        c("motionSidebarCollapse", "Collapse the folder, or go up a level", "Moving around", &["left", "h", "-"])
            .on(Sidebar)
            .motion()
            .note("Nothing to leave on Focus or the timeline, so it stays put. - is Zed's select parent"),
        // The inspector is vim's: a row cursor in normal mode that stands on
        // a field without giving it the keyboard, and a caret only once you
        // ask to edit. Walking down it by handing each row the keyboard put
        // the caret in the notes, which kept ↓ for themselves, and the walk
        // stopped there for good.
        c("motionInspectorNext", "Go to the next field", "Moving around", &["down", "j"])
            .on(Inspector)
            .motion()
            .note("Every row, the notes included: standing on a field only highlights it. In a one-line field ↓ moves on too"),
        c("motionInspectorPrevious", "Go to the previous field", "Moving around", &["up", "k"])
            .on(Inspector)
            .motion()
            .note("In a one-line field ↑ moves back too"),
        c("motionInspectorEnds", "Go to the first or last field", "Moving around", &["gg", "shift+g", "home", "end"])
            .on(Inspector)
            .motion(),
        c("motionInspectorEdit", "Edit the field", "Moving around", &["enter", "i", "e"])
            .on(Inspector)
            .motion()
            .note("A text field takes the caret — Return or Esc gives it back, in the notes Esc or ⌘Return; Tab moves on to the next field. A switch flips, a button, menu or date opens"),
        c("motionInspectorClear", "Clear the field", "Moving around", &["x", "delete"])
            .on(Inspector)
            .motion()
            .note("Empties a text field, takes a date or a blocker away, sets the priority back to none, turns a switch off"),
        c("motionInspectorLeave", "Back to the tasks", "Moving around", &["left", "h"])
            .on(Inspector)
            .motion()
            .note("On a picker ← steps its value instead. Esc leaves too, from the row cursor; in a field it only ends the edit"),
        c("motionBoardColumn", "Focus the next or previous column", "Moving around", &["left", "right"])
            .on(Board)
            .motion(),
        // A card's subtask rows are a column of their own: `l` steps the
        // cursor in, ↑/↓ walk them, and `h` (or Esc) steps back onto the card.
        // ←/→ stay the columns', so the pair is vim's other one.
        c("motionBoardIntoCard", "Step into the card's subtasks", "Moving around", &["l"])
            .on(Board)
            .motion()
            .note("↑ and ↓ then walk the subtask rows the card shows, and what acts on a task acts on the row you are on; a folded card unfolds first"),
        c("motionBoardOutOfCard", "Step out of the subtasks, back onto the card", "Moving around", &["h"])
            .on(Board)
            .motion()
            .note("Esc does the same"),
        c("motionOutlineUnfold", "Show the subtasks, step into them, or go to the inspector", "Moving around", &["right"])
            .on(Outline)
            .motion()
            .note("On a task with no subtasks it moves to the inspector; l or ] still opens the task"),
        c("motionOutlineFold", "Hide the subtasks, or step out to the parent", "Moving around", &["left"])
            .on(Outline)
            .motion()
            .note("At the top level it leaves, as ← does elsewhere"),
        c("motionDismiss", "Clear the selection, or leave the scope", "Moving around", &["escape"])
            .motion(),
        // The four a task can have, as `!1`–`!4` writes them. It listed 1–9,
        // left from the Checkvist panel's rank queue: the workspace's priority
        // stops at 4, so 5–9 ran and said "Choose a priority between None and
        // Urgent" instead. The minus ones are the back four, as `!-1`–`!-4`
        // writes them, so the list sets every priority the title can.
        c(
            "motionSetPriority",
            "Set the task's priority",
            "Moving around",
            &["1", "2", "3", "4", "-1", "-2", "-3", "-4"],
        )
        .motion()
        .note("Type the digit, at once, and it shows on the row; a minus first puts it behind the tasks with none; 0 clears it"),
        // Today's list is walked with the ordinary selection keys, the way every
        // other pane's is — it used to keep a selection of its own behind a search
        // field, and these keys only handed that field the caret back. What is
        // left of its own is gf, which queues behind a running block rather than
        // replacing it, and the arrows across, which open a task where it stands
        // to show its subtasks, as Checkvist does, and fold it again.
        c("todayStart", "Start the task, or finish the running one", "Today", &["gf"])
            .on(Today)
            .note("Or add it to the queue behind a running block"),
        c("motionTodayUnfold", "Show the subtasks, step into them, or go to the inspector", "Moving around", &["right", "l"])
            .on(Today)
            .motion()
            .note("On a task with no subtasks it moves to the inspector; ] still opens the task"),
        c("motionTodayLeave", "Hide the subtasks, step out to the parent, or back to the sidebar", "Moving around", &["left", "h"])
            .on(Today)
            .motion()
            .note("At the top of the day it goes back to the list in the sidebar"),
    ]
}
