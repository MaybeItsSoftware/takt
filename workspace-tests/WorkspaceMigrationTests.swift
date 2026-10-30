import Foundation
import TestSQLite
import TaktWorkspace
import XCTest

/// The migration ladder itself, as opposed to what any one migration does.
///
/// Identifiers and their order are effectively permanent: a database in the
/// wild records which ones it has run, so renaming or reordering one silently
/// re-runs it, or silently skips it, on every existing install.
final class WorkspaceMigrationTests: XCTestCase {
  private static let expectedLadder = [
    "v1_local_workspace",
    "v2_metadata_and_focus",
    "v3_dailies_as_contributions",
    "v4_manual_focus_order",
    "v5_task_source_identity",
    "v6_inbox_as_a_system_list",
    "v7_task_full_text_search",
    "v8_undo_journal",
    "v9_focus_block_start",
    "v10_focus_points",
    "v11_stable_visible_roots",
    "v12_task_conditions_and_work",
    "v13_legacy_visible_roots",
    "v14_nested_lists",
    "v15_kanban_board_history",
    "v16_task_completion_time",
    "v17_sync",
    "v18_themes_and_preferences",
    "v19_habit_options",
    "v20_waiting_follow_ups",
    "v21_task_dependencies",
    "v22_matrix_order",
    "v23_filter_lists",
    "v24_condition_categories",
    "v25_agent_instructions",
    "v26_commitment_missed_stack",
    "v27_condition_order_and_context",
    "v28_synced_focus_context",
    "v29_daily_contribution_skips",
    "v30_google_calendar",
    "v31_link_details",
    "v32_follow_up_repeat",
    "v33_recurrence_series",
    "v34_condition_provides",
    "v35_series_title",
  ]

  private var directoryURL: URL!

  override func setUpWithError() throws {
    directoryURL = FileManager.default.temporaryDirectory
      .appendingPathComponent("TaktMigrationTests-\(UUID().uuidString)", isDirectory: true)
  }

  override func tearDownWithError() throws {
    try? FileManager.default.removeItem(at: directoryURL)
  }

  private func appliedMigrations(at url: URL) throws -> [String] {
    try DatabaseQueue(path: url.path).read { db in
      try String.fetchAll(db, sql: "SELECT identifier FROM grdb_migrations ORDER BY rowid")
    }
  }

  func testEveryMigrationAppliesInOrderToANewDatabase() throws {
    let url = directoryURL.appendingPathComponent("new.sqlite")
    _ = try WorkspaceStore(databaseURL: url)

    XCTAssertEqual(try appliedMigrations(at: url), Self.expectedLadder)
  }

  func testLegacyBulkImportedWrapperIsRecognisedOnUpgrade() throws {
    let url = directoryURL.appendingPathComponent("legacy-wrapper.sqlite")
    let store = try WorkspaceStore(databaseURL: url)
    let workspace = try store.bootstrapIfNeeded()
    let batchDate = Date(timeIntervalSince1970: 1_700_000_000)
    let list = try store.createList(workspaceId: workspace.id, name: "Work", now: batchDate)
    let root = try store.createTask(listId: list.id, title: "Work", now: batchDate)
    let child = try store.createTask(listId: list.id, title: "Proposal", parentTaskId: root.id, now: batchDate)
    try DatabaseQueue(path: url.path).write { db in
      try db.execute(sql: "DELETE FROM grdb_migrations WHERE identifier = 'v13_legacy_visible_roots'")
    }
    let reopened = try WorkspaceStore(databaseURL: url)
    XCTAssertEqual(try reopened.visibleRootParentTaskID(for: list), root.id)
    XCTAssertEqual(try reopened.visibleRootTasks(in: workspace.id).map(\.id), [child.id])
    XCTAssertEqual(try reopened.task(id: child.id)?.parentTaskId, root.id)
  }

  func testReopeningAppliesNothingFurther() throws {
    let url = directoryURL.appendingPathComponent("reopen.sqlite")
    let store = try WorkspaceStore(databaseURL: url)
    let workspace = try store.bootstrapIfNeeded()
    let task = try store.createTask(
      listId: try XCTUnwrap(store.inbox(in: workspace.id)).id, title: "Survives a reopen")

    let reopened = try WorkspaceStore(databaseURL: url)

    XCTAssertEqual(try appliedMigrations(at: url), Self.expectedLadder)
    XCTAssertEqual(try reopened.task(id: task.id)?.title, "Survives a reopen")
    XCTAssertEqual(try reopened.workspaces().map(\.id), [workspace.id])
  }

  func testUpgradingAnImportedWrapperPreservesItsIdentityInOlderUndoSnapshots() throws {
    let url = directoryURL.appendingPathComponent("wrapper-upgrade.sqlite")
    let store = try WorkspaceStore(databaseURL: url)
    let workspace = try store.bootstrapIfNeeded()
    let list = try XCTUnwrap(store.importTasks(
      workspaceId: workspace.id, listName: "Work", sourceSystem: "checkvist",
      seeds: [
        .init(sourceId: "root", parentSourceId: nil, title: "Work", status: .open, sortOrder: 0),
        .init(sourceId: "child", parentSourceId: "root", title: "Proposal", status: .open, sortOrder: 0),
      ])).list
    let rootID = try XCTUnwrap(list.visibleRootTaskId)
    try store.updateList(id: list.id, name: "Work", colorHex: "#abcdef")

    // Recreate the previous schema and its snapshots without the new field.
    try DatabaseQueue(path: url.path).write { db in
      try db.rollBackSyncMigration()
      for suffix in ["insert", "update", "delete"] {
        try db.execute(sql: "DROP TRIGGER change_log_task_lists_\(suffix)")
      }
      try db.execute(sql: "ALTER TABLE task_lists DROP COLUMN visibleRootTaskId")
      try db.execute(sql: """
        UPDATE change_log SET
          beforeJSON = json_remove(beforeJSON, '$.visibleRootTaskId'),
          afterJSON = json_remove(afterJSON, '$.visibleRootTaskId')
        WHERE tableName = 'task_lists'
        """)
      try db.execute(sql: "DELETE FROM grdb_migrations WHERE identifier = 'v11_stable_visible_roots'")
    }

    let migrated = try WorkspaceStore(databaseURL: url)
    XCTAssertEqual(try migrated.visibleRootParentTaskID(for: list), rootID)
    XCTAssertEqual(try migrated.undo(), "Edit List")
    XCTAssertEqual(try migrated.visibleRootParentTaskID(for: list), rootID)
    XCTAssertNil(try migrated.lists(in: workspace.id).first { $0.id == list.id }?.colorHex)
    try migrated.redo()
    XCTAssertEqual(try migrated.visibleRootParentTaskID(for: list), rootID)
  }

  /// Each migration has to be able to run against a database that already has
  /// work in it, not only against an empty one. Rewinding the ladder and
  /// letting it re-run is the closest a test can get to an old install.
  func testTheLadderRerunsOverExistingWorkWithoutLosingIt() throws {
    let url = directoryURL.appendingPathComponent("populated.sqlite")
    let store = try WorkspaceStore(databaseURL: url)
    let workspace = try store.bootstrapIfNeeded()
    let list = try XCTUnwrap(store.inbox(in: workspace.id))
    let parent = try store.createTask(listId: list.id, title: "Parent task")
    let child = try store.createTask(listId: list.id, title: "Child task", parentTaskId: parent.id)
    try store.setKanbanColumn("today", for: child.id)
    let sessionStart = Date(timeIntervalSince1970: 1_700_000_000)
    let session = try store.startFocusSession(taskId: parent.id, now: sessionStart)

    // Rewind the additive migrations: drop what they created, forget that they
    // ran, and reopen. What v1–v4 built, and the data in it, stays put.
    try DatabaseQueue(path: url.path).write { db in
      try db.rollBackSyncMigration()
      let triggers = try String.fetchAll(
        db, sql: "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'change\\_log%' ESCAPE '\\'")
      for trigger in triggers { try db.execute(sql: "DROP TRIGGER \(trigger)") }
      for table in ["kanban_boards", "focus_work_blocks", "task_conditions", "focus_awards", "change_log", "undo_control", "tasks_fts"] {
        try db.execute(sql: "DROP TABLE \(table)")
      }
      for trigger in try String.fetchAll(
        db, sql: "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE '%tasks_fts%'") {
        try db.execute(sql: "DROP TRIGGER \(trigger)")
      }
      try db.execute(sql: "DROP INDEX task_lists_on_system_role")
      try db.execute(sql: "ALTER TABLE task_lists DROP COLUMN visibleRootTaskId")
      try db.execute(sql: "ALTER TABLE task_lists DROP COLUMN systemRole")
      try db.execute(sql: "ALTER TABLE focus_sessions DROP COLUMN activeTaskStartedAt")
      for column in ["activeBlockId", "accumulatedSeconds", "pausedAt", "checkpointAt"] {
        try db.execute(sql: "ALTER TABLE focus_sessions DROP COLUMN \(column)")
      }
      try db.execute(sql: "ALTER TABLE task_metadata DROP COLUMN planningJSON")
      try db.execute(sql: "ALTER TABLE task_lists DROP COLUMN completedAt")
      for column in ["completedAt", "itemKind", "isPromoted", "archivedAt"] {
        try db.execute(sql: "ALTER TABLE tasks DROP COLUMN \(column)")
      }
      try db.execute(sql: "DROP INDEX tasks_on_source")
      try db.execute(sql: "ALTER TABLE tasks DROP COLUMN sourceSystem")
      try db.execute(sql: "ALTER TABLE tasks DROP COLUMN sourceId")
      try db.execute(
        sql: "DELETE FROM grdb_migrations WHERE identifier IN (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        arguments: ["v5_task_source_identity", "v6_inbox_as_a_system_list",
                    "v7_task_full_text_search", "v8_undo_journal", "v9_focus_block_start",
                    "v10_focus_points", "v11_stable_visible_roots", "v12_task_conditions_and_work",
                    "v13_legacy_visible_roots", "v14_nested_lists", "v15_kanban_board_history",
                    "v16_task_completion_time"])
    }

    let migrated = try WorkspaceStore(databaseURL: url)

    XCTAssertEqual(try appliedMigrations(at: url), Self.expectedLadder)
    XCTAssertEqual(try migrated.outline(in: list.id).map { $0.task.title }, ["Parent task", "Child task"])
    XCTAssertEqual(try migrated.kanbanColumn(for: child.id), "today")
    // And everything the later migrations add works on that existing data.
    XCTAssertEqual(try migrated.inbox(in: workspace.id)?.id, list.id)
    // A session that predates the per-task clock keeps the only start it had.
    XCTAssertEqual(try migrated.activeFocusSession()?.id, session.id)
    XCTAssertEqual(try migrated.activeFocusSession()?.activeTaskStartedAt, sessionStart)
    XCTAssertEqual(try migrated.searchTasks(in: workspace.id, matching: "child").map(\.task.id), [child.id])
    try migrated.updateTask(id: parent.id, title: "Renamed", notes: "", dueAt: nil, estimateSeconds: nil)
    XCTAssertEqual(try migrated.undo(), "Edit Task")
    XCTAssertEqual(try migrated.task(id: parent.id)?.title, "Parent task")
  }
}
