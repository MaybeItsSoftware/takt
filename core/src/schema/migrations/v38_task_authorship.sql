-- v38_task_authorship: who added and finished a task in a shared list, and
-- who it is assigned to (docs/shared-lists-plan.md). Each is an account id
-- the list's members name; NULL — every task before this, and every task in
-- a list of your own — is nobody in particular. All three are shared: they
-- go to the list's space, so every member sees the same.
-- Written for the Rust core; the journal and outbox triggers are reinstalled after it.
ALTER TABLE "tasks" ADD COLUMN "createdBy" TEXT;
-- statement
ALTER TABLE "tasks" ADD COLUMN "completedBy" TEXT;
-- statement
ALTER TABLE "task_metadata" ADD COLUMN "assigneeAccountId" TEXT;
