-- v36_task_agent_context: what the agent should know about one task, kept
-- apart from its notes, which are the user's own. NULL — every task written
-- before this one — is no context. It is read into the agent's prompt beside
-- the task and written by the inspector and the agent_instructions tools'
-- task scope. See core/src/agent_instructions.rs `set_task_agent_context`.
-- Written for the Rust core; the journal and outbox triggers are reinstalled after it.
ALTER TABLE "task_metadata" ADD COLUMN "agentContext" TEXT;
