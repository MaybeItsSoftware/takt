-- v39_open_ended_focus: a timer, a focus block with no length that counts up
-- until it is finished, beside the pomodoro's block that counts down to a
-- break. A session's flag says the block in hand is one; a queue item's says
-- the task is timed when it takes over. `workDurationSeconds` stays NOT NULL
-- and holds a day for a timer, so a client from before this, which drops the
-- column, still reads a block too long to end on its own.
-- Written for the Rust core; the journal and outbox triggers are reinstalled after it.
ALTER TABLE "focus_sessions" ADD COLUMN "openEnded" BOOLEAN NOT NULL DEFAULT 0;
-- statement
ALTER TABLE "focus_queue_items" ADD COLUMN "openEnded" BOOLEAN NOT NULL DEFAULT 0;
