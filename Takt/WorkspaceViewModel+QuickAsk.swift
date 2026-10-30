import Foundation
import TaktCore
import TaktWorkspace

/// Quick ask: `aa` on a task opens a one-line popup, and what is typed there
/// goes to the agent with that task named as the subject. See
/// `WorkspaceQuickAskOverlay` and `docs/agent-panel.md`.
///
/// It runs on a thread of its own (`quickAgent`), not the panel's: a request
/// about one task is not part of whatever the panel is in the middle of, and
/// sending it there would leave a "make it due friday" in that conversation's
/// memory, or wait behind its turn. Each ask starts a fresh process, which
/// ends when its turn does. The last ask stays readable — and its changes
/// undoable — at the top of the agent panel.
extension WorkspaceViewModel {

  /// `aa`: the popup, on the selected task. While an ask is still under way
  /// it reopens on that one, whatever is selected now, since Esc closed the
  /// popup without stopping it.
  func presentQuickAsk() {
    if quickAgent.isWorking || quickAgent.pendingApproval != nil, let task = quickAskTask {
      presentOverlay(.quickAsk(WorkspaceQuickAskRequest(task: task)))
      return
    }
    guard let task = selectedTask else { return }
    presentOverlay(.quickAsk(WorkspaceQuickAskRequest(task: task)))
  }

  /// Whether the popup on `task` shows the last ask: it was about that task.
  func quickAskShowsTranscript(for task: WorkspaceTask) -> Bool {
    quickAskTask?.id == task.id && !quickAgent.items.isEmpty
  }

  /// Sends `text` about `task` on a fresh quick-ask thread. The panel's
  /// settings — which agent, where it is, and whether changes wait for
  /// Approve — are the quick thread's too.
  func sendQuickAsk(_ text: String, about task: WorkspaceTask) {
    guard !quickAgent.isWorking, quickAgent.pendingApproval == nil else { return }
    agentInstructions.saveAccount()
    quickAgent.newThread()
    quickAgent.clientSetting = agent.clientSetting
    quickAgent.userExecutablePaths = agent.userExecutablePaths
    quickAgent.customWireSetting = agent.customWireSetting
    quickAgent.asksBeforeChangesSetting = agent.asksBeforeChangesSetting
    quickAgent.endsAfterTurn = true
    quickAgent.currentValues = { [weak self] tool, input in self?.agentCurrentValues(tool: tool, input: input) ?? [:] }
    quickAgent.onTurnFinished = { [weak self] in self?.agentTurnFinished() }
    quickAskTask = task
    let list = (lists + archivedLists).first { $0.id == task.listId }
    quickAgent.send(
      text,
      context: AgentSystemPrompt.quickAskSubject(
        taskTitle: task.title, taskID: task.id, listName: list?.name, listID: list?.id),
      blocks: [
        AgentContextBlock(
          key: "instructions",
          text: AgentStandingInstructions.block(agentInstructionScopes(forListID: task.listId)),
          cleared: AgentStandingInstructions.cleared)
      ])
  }

  /// The session whose transcript holds `itemID`: the panel's, or the quick
  /// ask's.
  func agentSession(holding itemID: UUID) -> WorkspaceAgentSession? {
    [agent, quickAgent].first { session in session.items.contains { $0.id == itemID } }
  }

  /// Return on a selected card in the popup: Approve while it waits, then
  /// Undo, or Redo once undone — whatever the card's button says.
  func pressAgentCard(_ itemID: UUID) {
    guard let session = agentSession(holding: itemID),
      let item = session.items.first(where: { $0.id == itemID }),
      case .write(let approval) = item.entry
    else { return }
    if approval.state == .pending {
      session.approve(itemID)
      return
    }
    guard approval.outcome == .applied, let step = approval.undoStep, let plan = agentUndoPlan(for: step),
      plan.isAvailable
    else { return }
    if plan.isUndone {
      if plan.canRedo { redoAgentChange(itemID) }
    } else {
      undoAgentChange(itemID)
    }
  }
}

/// The task a quick ask is about.
struct WorkspaceQuickAskRequest: Identifiable {
  let id = UUID()
  let task: WorkspaceTask
}
