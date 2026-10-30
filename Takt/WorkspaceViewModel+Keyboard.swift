import AppKit
import Foundation
import TaktWorkspace
import TaktCore

/// The desktop window's key handling: spell the key the way the catalogue
/// does, run the two-letter sequence machine over it, and ask
/// `WorkspaceCommandCatalog` what it means on the surface on screen.
///
/// There is no `switch` over key codes here any more, and that is the point.
/// The router used to be one, a few hundred lines long, beside a catalogue it
/// was meant to agree with and a second copy of the sequence list — so the
/// reference advertised keys that did nothing (`⌘⌃C`, the ladder's `o`), and
/// the focus screen "owned the keyboard" while `⌫` fell through it to delete a
/// task hidden behind it. A key now does what its catalogue row says, on the
/// surface the row names, and what a command *does* lives in `run(_:key:)`.
extension NSEvent {
  /// This key press as the catalogue spells it — `cmd+shift+c`, `?`, `enter`.
  var workspaceCommandKey: String? {
    let flags = modifierFlags.intersection([.command, .option, .control, .shift])
    return WorkspaceCommandCatalog.key(
      keyCode: keyCode,
      charactersIgnoringModifiers: charactersIgnoringModifiers ?? "",
      shift: flags.contains(.shift),
      ctrl: flags.contains(.control),
      cmd: flags.contains(.command),
      option: flags.contains(.option))
  }
}

@MainActor
extension WorkspaceViewModel {
  /// Handles keys that belong to the desktop workspace only. Text editing is
  /// filtered by `MainWindowController` before this method is reached.
  @discardableResult
  func handleDesktopKey(_ event: NSEvent) -> Bool {
    guard let key = event.workspaceCommandKey else { return false }
    // A delete waiting to be confirmed owns the next key: Return deletes,
    // anything else lets it go — and is swallowed, so a stray letter meant
    // for the prompt does not also act on the list behind it.
    if pendingTaskDeletionID != nil {
      if key == "enter" { confirmPendingTaskDeletion() } else { cancelPendingTaskDeletion() }
      return true
    }
    // `f` and `F` own the next key, as in vim: it is the letter to find, not
    // a command. Esc, or anything that is not a letter or digit, lets go.
    if let forward = pendingFindForward {
      pendingFindForward = nil
      if key != "escape", let character = event.characters?.first {
        find(character, forward: forward)
      }
      return true
    }
    // `:` owns the keys until Return, as vim's command line does: digits,
    // ⌫ to take one back, Esc — or anything else — to give up.
    if numberJump != nil {
      handleNumberJumpKey(key)
      return true
    }
    // Sequences are bare letters. Anything chorded — Shift included, so `⇧X`
    // is the way to run `x` without waiting — releases a held key first, so
    // the two run in the order they were pressed.
    guard event.modifierFlags.isDisjoint(with: [.command, .option, .control, .shift]) else {
      if let held = desktopShortcutSequence.flush() { dispatchKey(held) }
      return dispatchKey(key)
    }
    let sequences = WorkspaceCommandCatalog.sequences(on: commandSurface)
    switch desktopShortcutSequence.advance(key, at: event.timestamp, sequences: sequences) {
    case .pending:
      releaseHeldKeyAfterTimeout(heldAt: event.timestamp)
      return true
    case .command(let sequence):
      dispatchKey(sequence)
      return true
    case .flush(let held):
      dispatchKey(held)
      return dispatchKey(key)
    case .flushAndHold(let held):
      dispatchKey(held)
      releaseHeldKeyAfterTimeout(heldAt: event.timestamp)
      return true
    case .pass:
      return dispatchKey(key)
    }
  }

  /// Runs what the key means on the surface on screen *now* — asked afresh
  /// each time, because a flushed key can change the surface for the key
  /// behind it (`l` entering a task, say).
  @discardableResult
  private func dispatchKey(_ key: String) -> Bool {
    let surface = commandSurface
    if let command = WorkspaceCommandCatalog.command(forKey: key, on: surface) {
      run(command.id, key: key)
      return true
    }
    return WorkspaceCommandCatalog.swallowsUnhandledKey(key, on: surface)
  }

  /// A key held for a sequence that never came still has to do its own job.
  /// The hold's timestamp identifies it, so a timer armed for an earlier hold
  /// cannot release a later one early.
  private func releaseHeldKeyAfterTimeout(heldAt time: TimeInterval) {
    Task { @MainActor [weak self] in
      try? await Task.sleep(for: .seconds(DesktopShortcutSequence.timeout))
      guard let self, let held = self.desktopShortcutSequence.expire(heldAt: time) else { return }
      self.dispatchKey(held)
    }
  }

  // MARK: - What the direction keys mean, per region

  /// ⌘N: below the selected task — on the outline, the matrix's rows and the
  /// board — and at the foot of the day on Today, which draws no draft
  /// beside a task. Return used to do this on the outline while it opened
  /// the card on the board and started it on Today; Return now opens the
  /// task everywhere.
  ///
  /// On the board a card adds a card below it, and a subtask row drawn on a
  /// card adds a sibling subtask below that row, drawn on the same card.
  func addTaskBelowSelection() {
    guard viewMode == .board else {
      requestRelativeTaskComposerFocus(reference: nil)
      return
    }
    // The card the selected row is drawn on in the column the keys are in:
    // a subtask filed in a later column is a card there too, and the draft
    // belongs beside the row you are looking at, not over in that column.
    let column = boardColumns.first { $0.id == activeBoardColumnID }
    let cardID = selectedTaskID.flatMap { id in
      column.flatMap { boardRows(in: $0).cardByRow[id] } ?? boardCardID(owning: id)
    }
    let reference = selectedTaskID.flatMap(task(withID:)) ?? cardID.flatMap(task(withID:))
    requestRelativeTaskComposerFocus(reference: reference)
  }

  /// Whether the board's draft is a subtask row on a card rather than a card:
  /// it was asked for beside a row that is not a card in the column the keys
  /// are in.
  var boardDraftIsSubtaskRow: Bool {
    guard viewMode == .board, isDraftingTask, let reference = taskInsertionReference, !taskInsertionIsChild,
      let column = boardColumns.first(where: { $0.id == activeBoardColumnID })
    else { return false }
    return !tasks(in: column).contains { $0.id == reference.id }
  }

  /// Escape, which means "back out one step" from wherever the keyboard is.
  func dismissFromKeyboard() {
    // A draft row left open after clicking away is the first thing to go.
    if isDraftingTask { endTaskDraft(); return }
    switch keyboardFocusArea {
    case .sidebar:
      // Leaving a folder has to land somewhere. It used to fall back on
      // whichever list was still selected behind it; a folder scope has no
      // such list, so clearing it on its own would empty the pane.
      if selectedFolderID != nil { run(.goEverything) }
    case .inspector:
      // Back to the work, leaving the dock where it is. It used to close:
      // a pane that vanishes whenever you leave it is one you reopen all day.
      requestKeyboardFocus(.tasks)
    default:
      dismissKeyboardContext()
    }
  }

  func collapseSidebarCursor() {
    if let folder = selectedFolder, expandedFolderIDs.contains(folder.id) {
      setFolderExpanded(folder, expanded: false)
    } else if let scope = scopeTask, scope.isList {
      leaveTaskScope()
    } else if let parentID = selectedFolder?.parentFolderId,
      let parent = folders.first(where: { $0.id == parentID }) {
      selectFolder(parent)
    } else if let parentID = selectedList?.folderId,
      let parent = folders.first(where: { $0.id == parentID }) {
      selectFolder(parent)
    } else if !isEverythingSelected {
      selectEverything()
    }
  }

  func moveSidebarCursor(_ key: String) {
    switch CursorStep(key: key) {
    case .by(let offset): moveSidebarSelection(by: offset)
    case .first: moveSidebarSelectionToEnd(first: true)
    case .last: moveSidebarSelectionToEnd(first: false)
    case nil: break
    }
  }

  func moveDoneCursor(_ key: String) {
    switch CursorStep(key: key) {
    case .by(let offset): moveDoneCursor(by: offset)
    case .first: doneCursorID = completedTasks.first?.id
    case .last: doneCursorID = completedTasks.last?.id
    case nil: break
    }
  }

  func moveTimelineCursor(_ key: String) {
    let summaries = timelineSummaries
    switch CursorStep(key: key) {
    case .by(let offset): moveTimelineCursor(by: offset)
    case .first: timelineCursorID = summaries.first?.id
    case .last: timelineCursorID = summaries.last?.id
    case nil: break
    }
  }

  /// `⌥1`–`⌥4`, in the order the matrix is read: urgent and important first.
  func placeSelectionInQuadrant(_ key: String) {
    guard let task = selectedTask else { return }
    switch key.last {
    case "1": setMatrixPosition(.init(urgency: 1, importance: 1), for: task)
    case "2": setMatrixPosition(.init(urgency: 0, importance: 1), for: task)
    case "3": setMatrixPosition(.init(urgency: 1, importance: 0), for: task)
    case "4": setMatrixPosition(.init(urgency: 0, importance: 0), for: task)
    default: break
    }
  }

  func quickEdit(_ kind: WorkspaceTaskQuickEditKind) {
    guard let task = selectedTask else { return }
    presentOverlay(.quickEdit(WorkspaceTaskQuickEditRequest(task: task, kind: kind)))
  }

  /// The habit form, on the selected task: its habit if it has one, a new
  /// habit made from it if not. With no task (or a list) selected, a
  /// standalone habit.
  func presentHabitForm() {
    guard let store else { return }
    let task = selectedTask.flatMap { $0.isList ? nil : $0 }
    do {
      let context = try store.habitFormContext(forTaskId: task?.id)
      presentOverlay(.habit(WorkspaceHabitRequest(context: context)))
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  /// `ee`/F2: the title turns into a field in its own row, the way Checkvist
  /// edits — in the outline, on Today and in the matrix. Where the task is not
  /// on screen as a row the quick-edit overlay does the job.
  func editSelectedTaskTitle(caret: WorkspaceTitleCaret = .selectAll) {
    if keyboardFocusArea == .sidebar { beginRenamingSelection(); return }
    if viewMode != .board, let task = selectedTask, isShowing(task.id) {
      editingTaskTitleCaret = caret
      editingTaskTitleID = task.id
    } else if viewMode == .board, let task = selectedTask, isDrawnOnBoard(task.id) {
      // On the board too: the card's title, or the subtask row's on a card.
      editingTaskTitleCaret = caret
      editingTaskTitleID = task.id
    } else {
      quickEdit(.title)
    }
  }

  /// Saves an in-row title edit. Trailing tokens are read the way the add
  /// field reads them — `45m`, `@fri`, `^mon`, `#tag`, `!1` — and set the
  /// estimate, due day, start, tags (added to the ones it has) and priority,
  /// all as one undo step with the new title. A start on or after the due
  /// day is refused as the editor refuses it.
  func commitTaskTitleEdit(_ task: WorkspaceTask, text: String) {
    editingTaskTitleID = nil
    returnKeyboardToBoardAfterTitleEdit()
    let capture = TaskCapture.parse(text)
    guard !capture.title.isEmpty else { return }
    guard capture.title != task.title || capture.hasDetails else { return }
    // A repeating task asks first whether the new name is this turn's alone.
    if capture.title != task.title, let schedule = try? store?.periodicSchedule(for: task.id) {
      presentOverlay(.seriesRename(WorkspaceSeriesRenameRequest(task: task, capture: capture, schedule: schedule)))
      return
    }
    saveTitleEdit(capture, of: task)
  }

  /// Saves a parsed in-row title edit; `afterSave` runs inside the same write.
  func saveTitleEdit(
    _ capture: TaskCapture, of task: WorkspaceTask, afterSave: ((WorkspaceStore) throws -> Void)? = nil
  ) {
    editTaskValues(of: task, afterSave: afterSave) { values in
      values.title = capture.title
      if let dueAt = capture.dueAt { values.dueAt = dueAt; values.dueDate = nil }
      if let startAt = capture.startAt { values.startAt = startAt }
      if let seconds = capture.estimateSeconds { values.estimateMinutes = String(seconds / 60) }
      if let priority = capture.priority { values.priority = priority }
      if !capture.tags.isEmpty {
        let existing = values.tags.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
          .filter { !$0.isEmpty }
        values.tags = (existing + capture.tags).joined(separator: ", ")
      }
      // A pasted address joins the task's links (docs/links.md).
      if !capture.links.isEmpty {
        let existing = values.links.split(whereSeparator: \.isNewline).map(String.init)
        values.links = (existing + capture.links.filter { !existing.contains($0) }).joined(separator: "\n")
      }
    }
  }

  func cancelTaskTitleEdit() {
    editingTaskTitleID = nil
    returnKeyboardToBoardAfterTitleEdit()
  }

  /// Whether a task is on the board: a card, or a subtask row drawn on one.
  func isDrawnOnBoard(_ id: String) -> Bool {
    boardVisibleTaskIDs.contains(id) || boardCardID(owning: id) != nil
  }

  /// The field held the keyboard; a card takes it back on a focus request,
  /// so the arrows carry on from the row that was edited.
  private func returnKeyboardToBoardAfterTitleEdit() {
    guard viewMode == .board, keyboardFocusArea == .tasks else { return }
    requestKeyboardFocus(.tasks)
  }

  func editTaskValues(_ change: (inout TaskEditorValues) -> Void) {
    guard let task = selectedTask else { return }
    editTaskValues(of: task, change)
  }

  /// One change saved straight away — asking first what the deadline does
  /// when it starts the task on a later day, and when it moves a repeating
  /// task onto the day it comes round again anyway.
  func editTaskValues(
    of task: WorkspaceTask, afterSave: ((WorkspaceStore) throws -> Void)? = nil,
    _ change: (inout TaskEditorValues) -> Void
  ) {
    guard let store else { return }
    var draft: TaskEditorDraft
    do {
      draft = TaskEditorDraft(snapshot: try store.taskEditorSnapshot(for: task.id))
    } catch {
      errorMessage = error.localizedDescription
      return
    }
    change(&draft.values)
    if askAboutStartingLater(draft, title: task.title, proceed: { [weak self] answered, keepingDue in
      self?.saveEditedValues(answered, of: task, keepingDue: keepingDue, afterSave: afterSave)
    }) {
      return
    }
    saveEditedValues(draft, of: task, keepingDue: false, afterSave: afterSave)
  }

  private func saveEditedValues(
    _ draft: TaskEditorDraft, of task: WorkspaceTask, keepingDue: Bool,
    afterSave: ((WorkspaceStore) throws -> Void)?
  ) {
    guard let store else { return }
    let save: @MainActor () -> Bool = { [weak self, draft] in
      guard let self else { return false }
      var saved = false
      perform {
        _ = try store.saveTaskEditor(draft, keepingDue: keepingDue)
        try afterSave?(store)
        saved = true
        self.reloadOutline(refreshSidebar: task.isList)
        self.reloadDailies()
        self.reloadNextUp()
      }
      return saved
    }
    if askAboutRecurrenceClash(draft, title: task.title, save: save) { return }
    _ = save()
  }

  func invalidateTask(_ task: WorkspaceTask) {
    guard let store else { return }
    perform {
      try store.setStatus(task.status == .cancelled ? .open : .cancelled, for: task.id)
      reloadOutline(refreshSidebar: task.isList)
      reloadNextUp()
      // Cancelling a commitment's subtask skips its day rather than closing
      // it: it rests, so the dailies and boards are reread too.
      if isHabitTask(task) {
        reloadDailies()
        refresh([.board])
      }
    }
  }

  /// `f` / `F`: the next key is the letter to find.
  func beginFind(forward: Bool) {
    desktopShortcutSequence.reset()
    pendingFindForward = forward
  }

  /// `;`: the last find again, the same way.
  func repeatFind() {
    guard let last = lastFind else { return }
    find(last.character, forward: last.forward)
  }

  /// Moves the cursor to the next row beginning with `character` — a task on
  /// the task surfaces, a row in the sidebar.
  func find(_ character: Character, forward: Bool) {
    lastFind = (character, forward)
    let found: Bool
    if keyboardFocusArea == .sidebar {
      let rows = sidebarRows
      let current = sidebarCursorRow.flatMap { row in rows.firstIndex { $0.id == row.id } }
      let index = VimFind.index(
        in: rows.map(sidebarRowTitle), from: current, character: character, forward: forward)
      if let index { applySidebarCursor(rows[index]) }
      found = index != nil
    } else {
      let rows = navigationRowIDs()
      let index = VimFind.index(
        in: rows.map { task(withID: $0)?.title ?? "" }, from: selectedTaskID.flatMap(rows.firstIndex(of:)),
        character: character, forward: forward)
      if let index { selectedTaskID = rows[index] }
      found = index != nil
    }
    if !found { onStatusMessage?("Nothing starts with \(character)") }
  }

  private func sidebarRowTitle(_ row: WorkspaceSidebarRow) -> String {
    switch row.kind {
    case .everything: "Everything"
    case .today: "Today"
    case .list(let id): lists.first { $0.id == id }?.name ?? ""
    case .nestedList(let id): nestedLists.first { $0.task.id == id }?.task.title ?? ""
    case .folder(let id): folders.first { $0.id == id }?.name ?? ""
    case .filterList(let id): filterLists.first { $0.id == id }?.name ?? ""
    }
  }

  // MARK: - Going to a row by its number

  /// The rows `:` numbers, in order: Today's own tasks, as its rows number
  /// them — a subtask drawn under one is not a place in the day — and on
  /// every other surface the rows the cursor walks.
  func numberJumpRowIDs() -> [String] {
    viewMode == .today
      ? dayTree.filter { $0.row.depth == 0 }.map(\.task.id)
      : navigationRowIDs()
  }

  /// `:`: the next keys are a row's number.
  func beginNumberJump() {
    guard keyboardFocusArea == .tasks else { return }
    desktopShortcutSequence.reset()
    numberJump = ""
    numberJumpNumbers = viewMode == .today ? [:] : Dictionary(
      numberJumpRowIDs().enumerated().map { ($1, $0 + 1) }, uniquingKeysWith: { first, _ in first })
  }

  private func handleNumberJumpKey(_ key: String) {
    guard let digits = numberJump else { return }
    switch key {
    case _ where key.count == 1 && key.first?.isNumber == true:
      // Five digits is more rows than any list has; more is a held key.
      if digits.count < 5 { numberJump = digits + key }
    case "delete":
      if digits.isEmpty { endNumberJump() } else { numberJump = String(digits.dropLast()) }
    case "enter":
      endNumberJump()
      if let number = Int(digits) { selectRow(numbered: number) }
    default:
      endNumberJump()
    }
  }

  private func endNumberJump() {
    numberJump = nil
    numberJumpNumbers = [:]
  }

  /// Vim's `:n`: past the last row is the last row, and `:0` the first.
  func selectRow(numbered number: Int) {
    let rows = numberJumpRowIDs()
    guard !rows.isEmpty else { return }
    selectedTaskID = rows[min(max(number, 1), rows.count) - 1]
  }

  func selectTaskAtEnd(first: Bool) {
    let rows = navigationRowIDs()
    selectedTaskID = first ? rows.first : rows.last
  }
}

/// Where a list-walking key moves a cursor. Shared by the sidebar, the
/// done rail, the timeline and the Conditions tab, whose catalogue rows list the same keys — the sidebar adds
/// `gg` and `⇧G`, the ends in Zed's vim project panel.
enum CursorStep {
  case by(Int), first, last

  init?(key: String) {
    switch key {
    case "down", "j": self = .by(1)
    case "up", "k": self = .by(-1)
    case "pagedown", "ctrl+d": self = .by(8)
    case "pageup", "ctrl+u": self = .by(-8)
    case "home", "cmd+up", "gg": self = .first
    case "end", "cmd+down", "shift+g": self = .last
    default: return nil
    }
  }
}

/// Where an in-row title edit starts typing.
enum WorkspaceTitleCaret {
  case selectAll, start, end
}

extension WorkspaceViewModel {
  /// ⌥⌘← / ⌥⌘→ and ⌘{ / ⌘}, Zed's previous and next tab: the list above or
  /// below in the sidebar, folders opened through. From Today, Everything or
  /// a folder there is no current list, so it starts at either end.
  func selectAdjacentList(by offset: Int) {
    let ordered = listsInSidebarOrder
    guard !ordered.isEmpty else { return }
    let current = (isEverythingSelected || selectedFolderID != nil)
      ? nil : ordered.firstIndex { $0.id == selectedListID }
    let target: Int
    if let current {
      target = min(max(0, current + offset), ordered.count - 1)
      guard target != current else { return }
    } else {
      target = offset > 0 ? 0 : ordered.count - 1
    }
    selectList(ordered[target].id)
  }
}

extension WorkspaceViewModel {
  /// Records a move between lists for ⌃- / ⌃⇧- and for ⌘P's recent order.
  func noteListVisit(leaving previous: String?, arriving id: String) {
    guard previous != id else { return }
    if !isWalkingListHistory {
      if let previous { listBackStack.append(previous) }
      if listBackStack.count > 50 { listBackStack.removeFirst(listBackStack.count - 50) }
      listForwardStack.removeAll()
    }
    var recent = recentListIDs.filter { $0 != id }
    recent.insert(id, at: 0)
    recentListIDs = Array(recent.prefix(30))
    UserDefaults.standard.set(recentListIDs, forKey: "workspaceRecentListsV1")
  }

  /// Zed's `pane::GoBack`, for lists: the one you were in before this one.
  func goBackInListHistory() {
    walkListHistory(from: &listBackStack, to: &listForwardStack)
  }

  func goForwardInListHistory() {
    walkListHistory(from: &listForwardStack, to: &listBackStack)
  }

  private func walkListHistory(from source: inout [String], to destination: inout [String]) {
    // Lists deleted since are skipped rather than ending the walk.
    while let id = source.popLast() {
      guard lists.contains(where: { $0.id == id }) else { continue }
      if let current = selectedListID, !isMultiListScope { destination.append(current) }
      isWalkingListHistory = true
      selectList(id)
      isWalkingListHistory = false
      return
    }
  }
}
