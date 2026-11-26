-- v40_list_priority: the lists that matter most, in order. A prioritised
-- list has a rank, 1 the most important, and the prioritised lists of a
-- workspace are numbered 1..n without gaps; NULL — every list before this —
-- is not prioritised. Next up ranks a task in a prioritised list ahead of
-- one in the same board column of any other list (`ranking.rs`). It is one
-- person's own: in a shared list it is a personal column, so it goes to
-- the account's own space and the other members never see it. A client
-- from before this drops the column on pull and ranks as it always did.
-- Written for the Rust core; the journal and outbox triggers are reinstalled after it.
ALTER TABLE "task_lists" ADD COLUMN "priorityRank" INTEGER;
