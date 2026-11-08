import Foundation
import Observation
import TaktWorkspace

@MainActor
@Observable
final class WorkspaceTaskEditor {
  private(set) var drafts: [String: TaskEditorDraft] = [:]
  private(set) var errors: [String: String] = [:]
  private(set) var persistenceError: String?
  @ObservationIgnored private let draftStore: TaskEditorDraftStore
  @ObservationIgnored private var pendingWrite: Task<Void, Never>?
  @ObservationIgnored private var canWrite = true
  @ObservationIgnored private var needsPersistence = false

  init(fileURL: URL = WorkspaceStore.defaultDatabaseURL().deletingLastPathComponent()
    .appendingPathComponent("task-editor-drafts.json")) {
    draftStore = TaskEditorDraftStore(fileURL: fileURL)
    do {
      for draft in try draftStore.load() { drafts[draft.baseline.taskId] = draft }
    } catch {
      // Preserve an unreadable file rather than overwriting recoverable edits.
      canWrite = false
      persistenceError = "Could not restore task drafts: \(error.localizedDescription)"
    }
  }

  func draft(for taskID: String) -> TaskEditorDraft? { drafts[taskID] }

  func open(_ taskID: String, store: WorkspaceStore) {
    flush()
    do {
      let saved = try store.taskEditorSnapshot(for: taskID)
      var draft = drafts[taskID] ?? TaskEditorDraft(snapshot: saved)
      draft.reconcile(with: saved)
      if drafts[taskID] != draft {
        let wasDirty = drafts[taskID]?.isDirty == true
        drafts[taskID] = draft
        if wasDirty || draft.isDirty { scheduleWrite() }
      }
    } catch {
      errors[taskID] = error.localizedDescription
    }
  }

  func edit(_ taskID: String, _ change: (inout TaskEditorValues) -> Void) {
    guard var draft = drafts[taskID] else { return }
    let wasUnavailable = draft.isUnavailable
    change(&draft.values)
    draft.reconcile(with: draft.baseline)
    draft.isUnavailable = wasUnavailable
    drafts[taskID] = draft
    errors[taskID] = nil
    scheduleWrite()
  }

  func resolve(_ taskID: String, field: TaskEditorField, useSaved: Bool) {
    guard var draft = drafts[taskID] else { return }
    draft.resolve(field, useSaved: useSaved)
    drafts[taskID] = draft
    errors[taskID] = nil
    scheduleWrite()
  }

  func refresh(store: WorkspaceStore) {
    for (taskID, original) in drafts {
      var draft = original
      do {
        draft.reconcile(with: try store.taskEditorSnapshot(for: taskID))
        if original.isUnavailable || (!original.conflicts.isEmpty && draft.conflicts.isEmpty) {
          errors[taskID] = nil
        }
      } catch WorkspaceStoreError.missingTask {
        draft.isUnavailable = true
      } catch {
        errors[taskID] = error.localizedDescription
      }
      if draft != original {
        drafts[taskID] = draft
        if original.isDirty || draft.isDirty { scheduleWrite() }
      }
    }
  }

  @discardableResult
  func save(_ taskID: String, store: WorkspaceStore, keepingDue: Bool = false) -> Bool {
    guard let draft = drafts[taskID] else { return false }
    do {
      let committed = try store.saveTaskEditor(draft, keepingDue: keepingDue)
      drafts[taskID] = TaskEditorDraft(snapshot: committed)
      errors[taskID] = nil
      scheduleWrite()
      flush()
      return true
    } catch {
      refresh(store: store)
      errors[taskID] = error.localizedDescription
      return false
    }
  }

  func revert(_ taskID: String, store: WorkspaceStore) {
    do {
      drafts[taskID] = TaskEditorDraft(snapshot: try store.taskEditorSnapshot(for: taskID))
      errors[taskID] = nil
      scheduleWrite()
      flush()
    } catch {
      errors[taskID] = error.localizedDescription
    }
  }

  func flush() {
    pendingWrite?.cancel()
    pendingWrite = nil
    guard canWrite, needsPersistence else { return }
    do {
      try draftStore.save(Array(drafts.values))
      needsPersistence = false
      persistenceError = nil
    } catch {
      persistenceError = "Task drafts could not be saved on this device: \(error.localizedDescription)"
    }
  }

  private func scheduleWrite() {
    needsPersistence = true
    pendingWrite?.cancel()
    pendingWrite = Task { @MainActor [weak self] in
      do { try await Task.sleep(for: .milliseconds(300)) } catch { return }
      self?.flush()
    }
  }
}
