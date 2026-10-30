import TestSQLite

extension Database {
  /// Takes `v17_sync` back off a database, and `v18_themes_and_preferences`
  /// and `v19_habit_options`, `v20_waiting_follow_ups`, `v21_task_dependencies`, `v22_matrix_order`, `v23_filter_lists`, `v24_condition_categories`, `v25_agent_instructions`, `v26_commitment_missed_stack`, `v27_condition_order_and_context`, `v28_synced_focus_context`, `v29_daily_contribution_skips`, `v30_google_calendar`, `v31_link_details`, `v32_follow_up_repeat`, `v33_recurrence_series`, `v34_condition_provides` and `v35_series_title` with it since they come later, so a test can rewind to an older schema. Its triggers name every column of the synced tables, so like the
  /// undo journal's they have to come off before a column can be taken away.
  /// Reopening the store runs the migration again.
  func rollBackSyncMigration() throws {
    let triggers = try String.fetchAll(
      self, sql: "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_outbox%' ESCAPE '\\'")
    for trigger in triggers { try execute(sql: "DROP TRIGGER \(trigger)") }
    // `v19_habit_options`: the journal's `dailies` triggers name its columns.
    for operation in ["insert", "update", "delete"] {
      try execute(sql: "DROP TRIGGER IF EXISTS change_log_dailies_\(operation)")
    }
    // `v26_commitment_missed_stack` adds one more column there.
    for column in ["sourceTaskId", "placementColumn", "dropsAtDayEnd", "expiryRule", "expiresAt", "stacksMissedFrom"] {
      try execute(sql: "ALTER TABLE dailies DROP COLUMN \(column)")
    }
    // `v29_daily_contribution_skips`: likewise for `daily_contributions`.
    for operation in ["insert", "update", "delete"] {
      try execute(sql: "DROP TRIGGER IF EXISTS change_log_daily_contributions_\(operation)")
    }
    try execute(sql: "ALTER TABLE daily_contributions DROP COLUMN skippedAt")
    // `v20_waiting_follow_ups`: likewise for `task_metadata`.
    for operation in ["insert", "update", "delete"] {
      try execute(sql: "DROP TRIGGER IF EXISTS change_log_task_metadata_\(operation)")
    }
    // `v22_matrix_order` and `v31_link_details` add one more column each
    // there, `v32_follow_up_repeat` two and `v33_recurrence_series` and
    // `v35_series_title` one each.
    for column in [
      "waitingOn", "waitingFollowUpAt", "waitingFollowUpTaskId", "followUpOfTaskId", "matrixRank", "linkDetailsJSON",
      "waitingFollowUpEveryDays", "waitingFollowUpCount", "seriesTaskId", "seriesTitle",
    ] {
      try execute(sql: "ALTER TABLE task_metadata DROP COLUMN \(column)")
    }
    // `v24_condition_categories`: the journal's `task_conditions` triggers
    // name its new column.
    for operation in ["insert", "update", "delete"] {
      try execute(sql: "DROP TRIGGER IF EXISTS change_log_task_conditions_\(operation)")
    }
    // `v27_condition_order_and_context` adds one more, and
    // `v34_condition_provides` another.
    for column in ["category", "sortOrder", "providesJSON"] {
      try execute(sql: "ALTER TABLE task_conditions DROP COLUMN \(column)")
    }
    // `v21_task_dependencies` and `v23_filter_lists` come later still: their
    // tables go, triggers and all.
    for table in ["calendar_event_links", "calendar_sync_tokens", "focus_contexts", "agent_instructions", "filter_lists", "task_dependencies", "sync_outbox", "sync_state", "sync_control", "themes", "preferences"] {
      try execute(sql: "DROP TABLE IF EXISTS \(table)")
    }
    try execute(
      sql: "DELETE FROM grdb_migrations WHERE identifier IN ('v17_sync', 'v18_themes_and_preferences', "
        + "'v19_habit_options', 'v20_waiting_follow_ups', 'v21_task_dependencies', 'v22_matrix_order', "
        + "'v23_filter_lists', 'v24_condition_categories', 'v25_agent_instructions', "
        + "'v26_commitment_missed_stack', 'v27_condition_order_and_context', 'v28_synced_focus_context', "
        + "'v29_daily_contribution_skips', 'v30_google_calendar', 'v31_link_details', 'v32_follow_up_repeat', "
        + "'v33_recurrence_series', 'v34_condition_provides', 'v35_series_title')")
  }
}
