import Foundation
import Observation
import TaktCore
import TaktWorkspace

/// One task's editor: the desktop's `WorkspaceTaskEditor`, for a single task.
///
/// Edits go into a `TaskEditorDraft` — never straight into the store — and
/// are saved through `WorkspaceStore.saveTaskEditor`, the call the Mac's Save
/// button makes, so a field edited here and on the Mac is the same write and
/// the same undo step. Saving is automatic: a little after the last change,
/// and at once when the inspector goes away.
///
/// The draft outlives the screen. An unsaved draft is written to
/// `TaskEditorDraftStore` in the app group, so an edit interrupted by the
/// app being killed is still there when the task is opened again. When the
/// task changes underneath it (a sync, a swipe in the outline, the widget),
/// `refresh` reconciles: untouched fields follow the store, and a field
/// changed on both sides becomes a conflict to resolve by hand.
@MainActor
@Observable
final class InspectorModel {
  let taskID: String
  private(set) var draft: TaskEditorDraft?
  private(set) var task: WorkspaceTask?
  /// Why the last save or read failed, shown in the inspector rather than as
  /// an alert over everything.
  private(set) var error: String?
  private(set) var conditions: [TaskCondition] = []
  /// What the task needs because of the lists it is in, outermost first:
  /// shown read-only, since it is changed on the list.
  private(set) var inherited: [TaskInheritedRequirement] = []
  private(set) var kanbanColumn: String?
  private(set) var boardColumns: [WorkspaceKanbanColumn] = WorkspaceKanbanColumn.blitzitDefaults
  private(set) var matrix = TaskMatrixPosition(urgency: nil, importance: nil)
  private(set) var isPlanned = false
  private(set) var loggedSeconds = 0
  private(set) var progress = TaskProgress()
  private(set) var listName = ""

  /// How long after the last keystroke a text field saves.
  var textSaveDelay: Duration = .milliseconds(1_200)
  /// How long after a toggle or picker change the edit saves.
  var controlSaveDelay: Duration = .milliseconds(250)

  @ObservationIgnored private let draftStore: TaskEditorDraftStore
  @ObservationIgnored private var pendingSave: Task<Void, Never>?

  init(taskID: String, draftsURL: URL = AppGroup.draftsURL) {
    self.taskID = taskID
    self.draftStore = TaskEditorDraftStore(fileURL: draftsURL)
  }

  var values: TaskEditorValues? { draft?.values }
  var isDirty: Bool { draft?.isDirty ?? false }
  var conflicts: [TaskEditorField] {
    TaskEditorField.allCases.filter { draft?.conflicts.contains($0) == true }
  }
  var isUnavailable: Bool { draft?.isUnavailable ?? false }

  // MARK: - Loading

  /// Opens the task: restores a draft left from last time, or starts one from
  /// the saved values, then reconciles it with what is saved now.
  func load(_ model: WorkspaceModel) {
    let store = model.store
    do {
      let saved = try store.taskEditorSnapshot(for: taskID)
      var draft = (try? draftStore.load())?.first { $0.baseline.taskId == taskID } ?? TaskEditorDraft(snapshot: saved)
      draft.reconcile(with: saved)
      self.draft = draft
      error = nil
    } catch {
      self.error = error.localizedDescription
    }
    loadSurroundings(model)
  }

  /// The store changed: follow it where the draft has not been touched.
  func refresh(_ model: WorkspaceModel) {
    guard var draft else { return load(model) }
    do {
      draft.reconcile(with: try model.store.taskEditorSnapshot(for: taskID))
    } catch WorkspaceStoreError.missingTask {
      draft.isUnavailable = true
    } catch {
      self.error = error.localizedDescription
    }
    if draft != self.draft { self.draft = draft }
    loadSurroundings(model)
  }

  /// What sits around the draft: the task itself, its board and matrix
  /// placement, the condition catalogue and the time logged against it.
  private func loadSurroundings(_ model: WorkspaceModel) {
    let store = model.store
    task = try? store.task(id: taskID)
    guard let task else { return }
    listName = model.listName(for: task.listId)
    conditions = (try? store.conditions(in: model.workspace.id)) ?? []
    inherited = (try? store.inheritedRequirements(taskId: taskID)) ?? []
    let metadata = try? store.boardMetadata(for: [taskID])
    kanbanColumn = metadata?.columns[taskID]
    matrix = metadata?.positions[taskID] ?? TaskMatrixPosition(urgency: nil, importance: nil)
    isPlanned = kanbanColumn == NextUpSelector.todayColumnID
    progress = (try? TaskProgress.load(store: store, taskID: taskID)) ?? TaskProgress()
    loggedSeconds = progress.loggedSeconds
    let key = "\(task.listId)/\(task.parentTaskId ?? "root")"
    // The board the task is a card on, as the core resolves it: its own
    // layout, its list's or the default. Rule columns are left out — a task
    // is not filed into one, it matches it.
    if let read = try? store.boardColumns(key: key, listIDs: [task.listId]) {
      let filable = read.columns.filter { !$0.isRuleColumn }
      if !filable.isEmpty { boardColumns = filable }
    }
  }

  // MARK: - Editing

  /// Changes the draft and schedules the save. `immediate` is for controls —
  /// a toggle has nothing more coming — rather than typing.
  func edit(_ model: WorkspaceModel, immediate: Bool = false, _ change: (inout TaskEditorValues) -> Void) {
    guard var draft else { return }
    let wasUnavailable = draft.isUnavailable
    change(&draft.values)
    draft.reconcile(with: draft.baseline)
    draft.isUnavailable = wasUnavailable
    guard draft != self.draft else { return }
    self.draft = draft
    error = nil
    persistDraft()
    scheduleSave(model, after: immediate ? controlSaveDelay : textSaveDelay)
  }

  func resolve(_ field: TaskEditorField, useSaved: Bool, model: WorkspaceModel) {
    guard var draft else { return }
    draft.resolve(field, useSaved: useSaved)
    self.draft = draft
    persistDraft()
    scheduleSave(model, after: controlSaveDelay)
  }

  private func scheduleSave(_ model: WorkspaceModel, after delay: Duration) {
    pendingSave?.cancel()
    pendingSave = Task { @MainActor [weak self, weak model] in
      do { try await Task.sleep(for: delay) } catch { return }
      guard let self, let model else { return }
      self.save(model)
    }
  }

  /// Saves now if there is anything to save. Returns whether the draft is
  /// clean afterwards.
  ///
  /// A save moving a repeating task onto the day it comes round again asks
  /// first, on `surface`: the inspector while it is up, the app once it has
  /// gone. Until the person chooses, the draft stays as it is, unsaved.
  @discardableResult
  func save(_ model: WorkspaceModel, askingOn surface: PendingRecurrenceClash.Surface = .inspector) -> Bool {
    pendingSave?.cancel()
    pendingSave = nil
    guard let draft, draft.isDirty else { return true }
    guard draft.conflicts.isEmpty, !draft.isUnavailable else { return false }
    // An empty title is a draft in progress, not a failed save.
    guard !draft.values.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return false }
    // Already asking here: a field losing focus under the dialog is not a second question.
    if surface == .inspector, model.recurrenceClash?.clash.taskId == taskID { return false }
    // Held strongly: asked as the inspector goes, the answer outlives it.
    if model.askAboutRecurrenceClash(draft, surface: surface, save: { self.commit(draft, model: model) }) {
      return false
    }
    return commit(draft, model: model)
  }

  private func commit(_ draft: TaskEditorDraft, model: WorkspaceModel) -> Bool {
    do {
      let committed = try model.store.saveTaskEditor(draft)
      self.draft = TaskEditorDraft(snapshot: committed)
      error = nil
      persistDraft()
      model.didChange()
      return true
    } catch {
      self.error = error.localizedDescription
      refresh(model)
      return false
    }
  }

  /// Throws the draft away and returns to the saved values.
  func revert(_ model: WorkspaceModel) {
    pendingSave?.cancel()
    do {
      draft = TaskEditorDraft(snapshot: try model.store.taskEditorSnapshot(for: taskID))
      error = nil
      persistDraft()
    } catch {
      self.error = error.localizedDescription
    }
  }

  /// Writes this task's draft into the shared draft file, leaving every
  /// other task's draft as it was. Clean drafts are dropped by the store.
  private func persistDraft() {
    var drafts = (try? draftStore.load()) ?? []
    drafts.removeAll { $0.baseline.taskId == taskID }
    if let draft { drafts.append(draft) }
    try? draftStore.save(drafts)
  }

  // MARK: - Planning that is not part of the draft

  // The board column, the matrix and "planned today" live in task metadata
  // the editor snapshot does not carry, so they are written straight away as
  // their own undo steps, the way the Mac's board and matrix write them.

  func setColumn(_ column: String?, model: WorkspaceModel) {
    model.perform { try $0.setKanbanColumn(column, for: taskID) }
    loadSurroundings(model)
  }

  func setQuadrant(_ quadrant: MatrixQuadrant?, model: WorkspaceModel) {
    let position = quadrant.map {
      TaskMatrixPosition(urgency: Int($0.representativeCoordinate.urgency), importance: Int($0.representativeCoordinate.importance))
    } ?? TaskMatrixPosition(urgency: nil, importance: nil)
    model.perform { try $0.setMatrixPosition(position, for: taskID) }
    loadSurroundings(model)
  }

  var quadrant: MatrixQuadrant? {
    guard let urgency = matrix.urgency, let importance = matrix.importance,
      MatrixGeometry.isPlaced(urgency: Double(urgency), importance: Double(importance))
    else { return nil }
    return MatrixGeometry.quadrant(urgency: Double(urgency), importance: Double(importance))
  }

  func setPlanned(_ planned: Bool, model: WorkspaceModel) {
    model.perform { try $0.setPlannedForToday(planned, taskIds: [taskID]) }
    loadSurroundings(model)
  }

  /// Copies the saved conditions, start and block rules down to subtasks.
  func applyPlanningToDescendants(model: WorkspaceModel) {
    save(model)
    if model.perform({ try $0.applyPlanningToDescendants(of: taskID) }) {
      model.showToast("Applied to subtasks")
    }
  }

  // MARK: - Due, as the Mac's inspector edits it

  /// Due is one of three things: nothing, a day (`dueDate`, which stays the
  /// same calendar day across time zones), or an exact time (`dueAt`).
  enum DueKind: String, CaseIterable, Identifiable {
    case none, day, time
    var id: String { rawValue }
    var title: String {
      switch self {
      case .none: "None"
      case .day: "Day"
      case .time: "Time"
      }
    }
  }

  var dueKind: DueKind {
    if values?.dueAt != nil { return .time }
    if values?.dueDate != nil { return .day }
    return .none
  }

  func setDueKind(_ kind: DueKind, model: WorkspaceModel) {
    edit(model, immediate: true) { values in
      switch kind {
      case .none:
        values.dueAt = nil
        values.dueDate = nil
      case .day:
        values.dueDate = TaskCalendarDate.string(values.dueAt ?? .now)
        values.dueAt = nil
      case .time:
        let day = values.dueDate.flatMap { TaskCalendarDate.date($0) } ?? Calendar.current.startOfDay(for: .now)
        values.dueAt = values.dueAt ?? Calendar.current.date(byAdding: .hour, value: 17, to: day)
        values.dueDate = nil
      }
    }
  }

  // MARK: - Conditions

  func addRequirement(_ conditionID: String, toGroup index: Int? = nil, model: WorkspaceModel) {
    edit(model, immediate: true) { values in
      var groups = values.requirementGroups ?? []
      if let index, groups.indices.contains(index) {
        if !groups[index].contains(conditionID) { groups[index].append(conditionID) }
      } else {
        groups.append([conditionID])
      }
      values.requirementGroups = groups
    }
  }

  func removeRequirement(_ conditionID: String, fromGroup index: Int, model: WorkspaceModel) {
    edit(model, immediate: true) { values in
      var groups = values.requirementGroups ?? []
      guard groups.indices.contains(index) else { return }
      groups[index].removeAll { $0 == conditionID }
      groups.removeAll(where: \.isEmpty)
      values.requirementGroups = groups.isEmpty ? nil : groups
    }
  }

  /// Ticks a condition on the task or off, as the Mac's `cc` picker: a tick
  /// is a group of its own, but a second place joins the first as "or",
  /// since you are in one place at a time. `places` are the conditions that
  /// are places. Saved straight away, as every other requirement edit is.
  func toggleRequirement(_ conditionID: String, places: Set<String>, model: WorkspaceModel) {
    edit(model, immediate: true) { values in
      values.requirementGroups = TaskRequirementToggle.toggling(conditionID, in: values.requirementGroups, places: places)
    }
  }

  /// `!` on a condition: ticks it as not holding ("not Home"), turns one
  /// required as itself round, or ticks a "not" off. Saved straight away.
  func negateRequirement(_ conditionID: String, model: WorkspaceModel) {
    edit(model, immediate: true) { values in
      values.requirementGroups = TaskRequirementToggle.negating(conditionID, in: values.requirementGroups)
    }
  }

  func conditionName(_ id: String) -> String {
    conditions.first { $0.id == id }?.name ?? "Missing condition"
  }

  /// A requirement as written: "Home", or "not Home".
  func requirementLabel(_ member: String) -> String {
    TaskRequirement.label(member) { id in conditions.first { $0.id == id }?.name }
  }

  /// A conflicting field's two values, written out for the banner.
  func display(_ field: TaskEditorField, in values: TaskEditorValues) -> String {
    switch field {
    case .title: values.title
    case .notes: values.notes.isEmpty ? "Empty" : values.notes
    case .dueAt: values.dueAt?.formatted(date: .abbreviated, time: .shortened) ?? "None"
    case .estimateMinutes: values.estimateMinutes.isEmpty ? "None" : "\(values.estimateMinutes) minutes"
    case .priority: TaskEditorValues.priorityName(values.priority)
    case .tags: values.tags.isEmpty ? "None" : values.tags
    case .recurrenceRule: values.recurrenceRule.isEmpty ? "None" : values.recurrenceRule
    case .links: values.links.isEmpty ? "None" : values.links
    case .dailyProgress: values.dailyProgress ? "On" : "Off"
    case .startAt: values.startAt?.formatted(date: .abbreviated, time: .shortened) ?? "None"
    case .dueDate: values.dueDate ?? "None"
    case .requirements:
      (values.requirementGroups ?? []).map { $0.map(requirementLabel).joined(separator: " or ") }
        .joined(separator: " and ").nonEmpty ?? "None"
    case .minimumBlock: values.minimumBlockMinutes ?? "None"
    case .singleSitting: values.requiresSingleSitting == true ? "Required" : "Not required"
    }
  }
}

fileprivate extension String {
  var nonEmpty: String? { isEmpty ? nil : self }
}
