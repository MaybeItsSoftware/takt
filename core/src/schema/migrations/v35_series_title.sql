-- v35_series_title: the title a repeating task's next turn takes, when this
-- occurrence was renamed for itself alone ("just today"). NULL — every task
-- written before this one — means the next turn takes this occurrence's own
-- title, as it always has. The next turn is written with this as its title
-- and NULL here, so the rename lasts one turn. See core/src/tasks.rs
-- `schedule_next_occurrence` and `keep_series_title`.
-- Written for the Rust core; the journal and outbox triggers are reinstalled after it.
ALTER TABLE "task_metadata" ADD COLUMN "seriesTitle" TEXT;
