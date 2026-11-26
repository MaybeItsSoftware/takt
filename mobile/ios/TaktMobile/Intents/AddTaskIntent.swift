import AppIntents
import Foundation
import TaktWorkspace
import WidgetKit

/// "Add task to Takt", for Shortcuts and Siri.
///
/// Runs without opening the app: the workspace database is in the app group
/// container, so the intent writes it directly, through the same capture path
/// as the add field — `45m #work @fri !1` on the end of the title is read off
/// and filed with the task. A running app notices the commit through
/// `PRAGMA data_version`, and the widgets are refreshed here.
struct AddTaskIntent: AppIntent {
  static let title: LocalizedStringResource = "Add task to Takt"
  static let description = IntentDescription(
    "Adds a task to Takt. Details typed on the end — 45m, #tag, @fri, ^mon, !1 — are filed with it.")

  @Parameter(title: "Title", requestValueDialog: "What's the task?")
  var taskTitle: String

  @Parameter(title: "List", description: "Where to file it. The Inbox when left empty.")
  var list: ListEntity?

  static var parameterSummary: some ParameterSummary {
    Summary("Add \(\.$taskTitle) to \(\.$list)")
  }

  func perform() async throws -> some IntentResult & ProvidesDialog & ReturnsValue<String> {
    // It can run without the app having launched, and the widget snapshot
    // it writes ranks against what holds on this device.
    await ThisDevice.install()
    let store = try IntentStore.open()
    let workspace = try store.bootstrapIfNeeded()
    let listID: String
    if let list {
      listID = list.id
    } else if let inbox = try store.inbox(in: workspace.id) {
      listID = inbox.id
    } else {
      throw IntentError.noInbox
    }
    let task = try store.createTask(capturing: taskTitle, listId: listID)
    if let snapshot = try? WidgetSnapshotBuilder.make(store: store, workspaceID: workspace.id) {
      try? snapshot.write()
      WidgetCenter.shared.reloadAllTimelines()
    }
    let listName = list?.name ?? "Inbox"
    return .result(value: task.id, dialog: "Added “\(task.title)” to \(listName).")
  }
}

enum IntentError: Error, CustomLocalizedStringResourceConvertible {
  case noInbox

  var localizedStringResource: LocalizedStringResource {
    switch self {
    case .noInbox: "Takt has no Inbox to add to."
    }
  }
}

/// The workspace, opened for an intent. Its own connection pool, which SQLite's
/// WAL mode lets sit beside the app's.
enum IntentStore {
  static func open() throws -> WorkspaceStore {
    try WorkspaceStore(databaseURL: AppGroup.databaseURL)
  }
}

/// A list, as Shortcuts offers it.
struct ListEntity: AppEntity {
  static let typeDisplayRepresentation: TypeDisplayRepresentation = "List"
  static let defaultQuery = ListEntityQuery()

  let id: String
  let name: String

  var displayRepresentation: DisplayRepresentation { DisplayRepresentation(title: "\(name)") }
}

struct ListEntityQuery: EntityStringQuery {
  func entities(for identifiers: [String]) async throws -> [ListEntity] {
    try allLists().filter { identifiers.contains($0.id) }
  }

  func entities(matching string: String) async throws -> [ListEntity] {
    try allLists().filter { $0.name.localizedCaseInsensitiveContains(string) }
  }

  func suggestedEntities() async throws -> [ListEntity] {
    try allLists()
  }

  private func allLists() throws -> [ListEntity] {
    let store = try IntentStore.open()
    let workspace = try store.bootstrapIfNeeded()
    return try store.lists(in: workspace.id)
      .filter { $0.completedAt == nil }
      .map { ListEntity(id: $0.id, name: $0.name) }
  }
}

struct TaktShortcuts: AppShortcutsProvider {
  static var appShortcuts: [AppShortcut] {
    AppShortcut(
      intent: AddTaskIntent(),
      phrases: [
        "Add a task to \(.applicationName)",
        "Add to \(.applicationName)",
        "Capture in \(.applicationName)",
      ],
      shortTitle: "Add task",
      systemImageName: "plus.square")
    AppShortcut(
      intent: OpenQuickAddIntent(),
      phrases: ["Quick add in \(.applicationName)"],
      shortTitle: "Quick add",
      systemImageName: "square.and.pencil")
  }
}
