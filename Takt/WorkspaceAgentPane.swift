import AppKit
import TaktCore
import TaktWorkspace
import SwiftUI

/// The agent panel: a conversation with the user's own coding agent — Claude
/// Code or Codex, `WorkspaceAgentSession.clientSetting` — about their tasks, in
/// the left dock.
///
/// Zed's agent panel is the model — a transcript that reads top to bottom, the
/// assistant's tool calls as quiet one-line entries in it, and a message field
/// pinned to the foot. What is Takt's own is the change card: every change
/// the assistant makes shows in the transcript as a card saying what it
/// changed, before and after, with an Undo that takes back exactly that step.
/// With "Ask before changing tasks" on, the card comes first and the change
/// runs only when Approve is clicked (or Return is pressed with the card
/// holding the keyboard). See `WorkspaceAgentSession` and
/// `docs/agent-panel.md`.
struct WorkspaceAgentPane: View {
  @Environment(WorkspaceViewModel.self) private var model
  @Environment(\.theme) private var theme
  @State private var draft = ""
  @State private var inputHasFocus = false
  @FocusState private var focusedCard: UUID?

  private var agent: WorkspaceAgentSession { model.agent }

  var body: some View {
    VStack(spacing: 0) {
      // Ahead of the transcript too: a message sent with no agent leaves
      // a notice saying so, and the place to fix it has to stay on screen.
      if agent.executablePath == nil && !agent.isRunning {
        WorkspaceAgentSetup()
      } else if agent.items.isEmpty && model.quickAgent.items.isEmpty {
        emptyState
      } else {
        transcript
      }
      inputBar
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(theme.paper)
    .onChange(of: inputHasFocus) { _, _ in reportKeyboard() }
    .onChange(of: focusedCard) { _, _ in reportKeyboard() }
    .onChange(of: agent.finishedTurns) { _, _ in model.agentTurnFinished() }
    .onDisappear { model.agentHoldsKeyboard = false }
  }

  /// Said in the middle of the panel, the way every empty pane says it.
  private var emptyState: some View {
    WorkspaceEmptyMessage(
      "Ask about your tasks…",
      detail: agent.asksBeforeChanges
        ? "It can read your lists freely. Every change it wants to make is shown here first, and runs only if you approve it."
        : "It can read and change your lists. Every change it makes is shown here, with an Undo."
    ) {
      HStack(spacing: theme.space.xs) {
        KeyCap(WorkspaceCommandHelpText.firstKey(for: .windowToggleAgentPanel))
        Text("opens and closes this panel")
          .font(theme.captionFont)
          .foregroundStyle(theme.dim)
      }
    }
  }

  private var transcript: some View {
    ScrollViewReader { proxy in
      ScrollView {
        LazyVStack(alignment: .leading, spacing: theme.space.md) {
          if !model.quickAgent.items.isEmpty { quickAsk }
          ForEach(agent.items) { item in
            row(item)
              .id(item.id)
          }
          if agent.isWorking && agent.pendingApproval == nil {
            Text("Working…")
              .font(theme.monoCaptionFont)
              .foregroundStyle(theme.dim)
              .id("working")
          }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, theme.listGutter)
        .padding(.vertical, theme.space.md)
      }
      .onChange(of: agent.items.count) { _, _ in
        guard let last = agent.items.last else { return }
        withAnimation(.easeOut(duration: 0.12)) { proxy.scrollTo(last.id, anchor: .bottom) }
      }
    }
  }

  @ViewBuilder
  private func row(_ item: WorkspaceAgentSession.Item) -> some View {
    switch item.entry {
    case .write(let approval):
      WorkspaceAgentApprovalCard(
        itemID: item.id, approval: approval, focusedCard: $focusedCard,
        approve: { model.agent.approve(item.id); model.focusAgentInput() },
        deny: { model.agent.deny(item.id); model.focusAgentInput() },
        undo: { model.undoAgentChange(item.id) },
        redo: { model.redoAgentChange(item.id) },
        leave: { model.focusAgentInput() })
    default:
      WorkspaceAgentTranscriptLine(entry: item.entry)
    }
  }

  /// The last quick ask (`aa`), above the thread: its own one-shot
  /// conversation, kept here so it can be read again and its changes undone
  /// after the popup is put away.
  private var quickAsk: some View {
    VStack(alignment: .leading, spacing: theme.space.sm) {
      HStack(spacing: theme.space.xs) {
        MicroLabel("Quick ask")
        if let task = model.quickAskTask {
          Text(task.title)
            .font(theme.captionFont)
            .foregroundStyle(theme.muted)
            .lineLimit(1)
            .truncationMode(.middle)
        }
        Spacer(minLength: 0)
        if model.quickAgent.isWorking {
          Text("working…").font(theme.monoCaptionFont).foregroundStyle(theme.dim)
        }
      }
      ForEach(model.quickAgent.items) { item in
        if case .write(let approval) = item.entry {
          WorkspaceAgentApprovalCard(
            itemID: item.id, approval: approval, focusedCard: nil,
            approve: { model.quickAgent.approve(item.id) },
            deny: { model.quickAgent.deny(item.id) },
            undo: { model.undoAgentChange(item.id) },
            redo: { model.redoAgentChange(item.id) },
            leave: { model.focusAgentInput() })
        } else {
          WorkspaceAgentTranscriptLine(entry: item.entry)
        }
      }
    }
    .padding(.bottom, theme.space.md)
    .overlay(alignment: .bottom) { FocusRule() }
  }

  /// The field and what Return will do, pinned under the transcript.
  private var inputBar: some View {
    VStack(alignment: .leading, spacing: theme.space.xs) {
      instructionsLine
      ZStack(alignment: .topLeading) {
        WorkspaceAgentInputField(
          text: $draft,
          focusRequest: model.agentInputFocusRequest,
          font: WorkspaceTitleBarAddField.fieldFont(theme),
          textColor: NSColor(theme.ink),
          insertionColor: NSColor(theme.primary),
          onSubmit: send,
          onCancel: { model.requestKeyboardFocus(.tasks) },
          onTab: focusPendingCard,
          onFocusChange: { inputHasFocus = $0 })
        if draft.isEmpty {
          Text(placeholder)
            .font(theme.bodyFont())
            .foregroundStyle(theme.dim)
            .allowsHitTesting(false)
        }
      }
      .frame(height: Self.inputHeight)
      .padding(theme.space.sm)
      .overlay(
        RoundedRectangle(cornerRadius: theme.controlRadius)
          .strokeBorder(
            inputHasFocus ? theme.focusRing : theme.inputBorder,
            lineWidth: inputHasFocus ? theme.focusRingWidth : theme.hairline))
      Text(hint)
        .font(theme.monoCaptionFont)
        .foregroundStyle(theme.dim)
        .lineLimit(1)
    }
    .padding(theme.space.sm)
    .overlay(alignment: .top) { FocusRule() }
  }

  /// Which standing instructions apply where you are, each a link to where it
  /// is written. A scope with instructions is in the muted ink; one without is
  /// dim, and clicking it is how you write some.
  private var instructionsLine: some View {
    HStack(spacing: theme.space.xxs) {
      Text("instructions")
        .foregroundStyle(theme.dim)
      ForEach(Array(model.agentInstructionScopes.enumerated()), id: \.offset) { _, scope in
        let isWritten = !scope.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        Text("·").foregroundStyle(theme.dim)
        Button(scope.name) { model.editAgentInstructions(scope) }
          .buttonStyle(.plain)
          .foregroundStyle(isWritten ? theme.muted : theme.dim)
          .help(
            isWritten
              ? "\(scope.label)'s instructions are sent with your messages · click to edit"
              : "No instructions for \(scope.label.prefix(1).lowercased() + scope.label.dropFirst()) · click to write some")
      }
    }
    .font(theme.monoCaptionFont)
    .lineLimit(1)
    .truncationMode(.tail)
  }

  /// Room for about four lines before the field scrolls.
  static let inputHeight: CGFloat = 72

  /// Names the agent the next message goes to: the thread's while one is
  /// running, otherwise the one chosen.
  private var placeholder: String {
    if agent.pendingApproval != nil { return "Approve or deny the change above first" }
    if agent.isWorking { return "Working…" }
    return "Ask \((agent.isRunning ? agent.client : agent.clientSetting).displayName) about your tasks…"
  }

  private var hint: String {
    if agent.pendingApproval != nil { return "tab to the card · return approves · esc denies" }
    if agent.latestUndoableChange != nil { return "return sends · tab to the last change · esc back to tasks" }
    return "return sends · shift-return new line · esc back to tasks"
  }

  private func send() {
    guard !agent.isWorking, agent.pendingApproval == nil else { return }
    let text = draft
    guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
    // Kept when there is nothing to send it to, so it survives setting the
    // path and pressing Return again.
    if agent.isRunning || agent.executablePath != nil { draft = "" }
    model.sendAgentMessage(text)
  }

  /// Tab from the field: the card waiting for an answer, or else the newest
  /// change that can be taken back, where Return undoes it.
  private func focusPendingCard() -> Bool {
    guard let card = agent.pendingApproval ?? agent.latestUndoableChange else { return false }
    focusedCard = card.id
    return true
  }

  private func reportKeyboard() {
    model.agentHoldsKeyboard = inputHasFocus || focusedCard != nil
  }

  /// Inline markdown — bold, italics, code, links — with the line breaks
  /// kept, so lists and paragraphs still read as lists and paragraphs.
  /// `Text` draws no block elements, and full parsing folded them into one
  /// run of text.
  static func markdown(_ text: String) -> AttributedString {
    (try? AttributedString(
      markdown: text,
      options: AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)))
      ?? AttributedString(text)
  }
}

/// A change the assistant made, or wants to make.
///
/// It says what happens in the app's words — "Add a task", the title, the
/// list by name, what a field held struck through over what it holds now —
/// with the raw input behind a disclosure for anyone who wants to check it.
/// Once the change has run and made an undo step, Undo takes back exactly
/// that step, or says on the button which later changes have to go with it;
/// once undone, Redo puts it back. Asked for first (the setting), Approve
/// and Deny answer it; Return does what the card's button does while the
/// card itself holds the keyboard, and there is no timer and no default.
struct WorkspaceAgentApprovalCard: View {
  @Environment(WorkspaceViewModel.self) private var model
  @Environment(\.theme) private var theme
  let itemID: UUID
  let approval: WorkspaceAgentSession.Approval
  /// The panel's keyboard focus, which Tab gives a card. Nil where the keys
  /// are read elsewhere — the quick ask's popup, whose field keeps the
  /// keyboard and selects a card with `isSelected`.
  var focusedCard: FocusState<UUID?>.Binding?
  var isSelected = false
  let approve: () -> Void
  let deny: () -> Void
  let undo: () -> Void
  let redo: () -> Void
  /// Back to the message field.
  let leave: () -> Void
  @State private var showsInput = false
  /// What Undo would do, asked of the journal whenever it moves.
  @State private var plan: WorkspaceStepUndoPlan?

  private var isPending: Bool { approval.state == .pending }
  private var isFocused: Bool { isSelected || focusedCard?.wrappedValue == itemID }
  private var hasRun: Bool { approval.outcome == .applied }
  /// Undo or Redo is there to press.
  private var canTakeBack: Bool {
    guard hasRun, approval.undoStep != nil, let plan, plan.isAvailable else { return false }
    return !plan.isUndone || plan.canRedo
  }

  var body: some View {
    let summary = AgentToolSummary.describe(
      tool: approval.request.toolName, input: approval.request.input, name: model.agentDisplayName(for:),
      current: { approval.before[$0] })
    return VStack(alignment: .leading, spacing: theme.space.sm) {
      HStack(alignment: .firstTextBaseline, spacing: theme.space.xs) {
        MicroLabel(isPending ? "Wants to" : approval.state == .automatic ? "Changed" : "Asked to")
        Text(summary.title)
          .font(theme.bodyFont())
          .foregroundStyle(summary.isDestructive ? theme.danger : theme.ink)
      }
      if !summary.fields.isEmpty {
        Grid(alignment: .leading, horizontalSpacing: theme.space.sm, verticalSpacing: theme.space.xxs) {
          ForEach(Array(summary.fields.enumerated()), id: \.offset) { _, field in
            GridRow(alignment: .firstTextBaseline) {
              Text(field.label)
                .font(theme.captionFont)
                .foregroundStyle(theme.muted)
              VStack(alignment: .leading, spacing: 0) {
                // Before, struck through, over after: a change reads as one.
                if let previous = field.previous {
                  Text(previous)
                    .font(theme.bodyFont())
                    .foregroundStyle(theme.muted)
                    .strikethrough(color: theme.muted)
                    .lineLimit(3)
                    .help("Before: \(previous)")
                }
                Text(field.value)
                  .font(theme.bodyFont())
                  .foregroundStyle(theme.ink)
                  .textSelection(.enabled)
                  .fixedSize(horizontal: false, vertical: true)
              }
            }
          }
        }
      }
      if let hint = approval.keyHint, showsKeyHint {
        WorkspaceAgentKeyHint(hint: hint)
      }
      DisclosureGroup(isExpanded: $showsInput) {
        Text(approval.request.input.jsonString(pretty: true))
          .font(theme.monoCaptionFont)
          .foregroundStyle(theme.muted)
          .textSelection(.enabled)
          .frame(maxWidth: .infinity, alignment: .leading)
      } label: {
        Text(AgentToolPolicy.priorityToolName(approval.request.toolName) ?? approval.request.toolName)
          .font(theme.monoCaptionFont)
          .foregroundStyle(theme.dim)
      }
      footer(isDestructive: summary.isDestructive)
    }
    .padding(theme.space.sm)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(theme.raised)
    .overlay(
      Rectangle()
        .strokeBorder(borderColor, lineWidth: isFocused ? theme.focusRingWidth : theme.hairline))
    .modifier(WorkspaceAgentCardFocus(focusedCard: focusedCard, itemID: itemID, isFocusable: isPending || canTakeBack))
    .onKeyPress(.return) {
      if isPending {
        approve()
      } else if canTakeBack, let plan {
        if plan.isUndone { redo() } else { undo() }
      } else {
        return .ignored
      }
      return .handled
    }
    .onKeyPress(.escape) {
      if isPending { deny() } else { leave() }
      return .handled
    }
    .task(id: "\(approval.undoStep ?? "")·\(model.historyVersion)") {
      guard let step = approval.undoStep else { return }
      plan = model.agentUndoPlan(for: step)
    }
  }

  /// The keys go with a change that is waiting, running or made: one that was
  /// declined, withdrawn or failed made nothing to repeat.
  private var showsKeyHint: Bool {
    switch approval.state {
    case .denied, .withdrawn: return false
    case .pending, .approved, .automatic:
      if case .failed = approval.outcome { return false }
      return true
    }
  }

  private var borderColor: Color {
    if isFocused { return theme.focusRing }
    switch approval.state {
    case .pending: return theme.warning.opacity(Theme.statusBorderOpacity)
    case .approved, .automatic:
      switch approval.outcome {
      case nil: return theme.border
      case .applied:
        return plan?.isUndone == true ? theme.border : theme.success.opacity(Theme.statusBorderOpacity)
      case .failed: return theme.danger.opacity(Theme.statusBorderOpacity)
      }
    case .denied, .withdrawn: return theme.border
    }
  }

  /// A delete's Approve is in the danger hue, so approving one never looks
  /// like approving an add.
  @ViewBuilder
  private func footer(isDestructive: Bool) -> some View {
    switch approval.state {
    case .pending:
      HStack(spacing: theme.space.sm) {
        // The card's keys answer only while it has the keyboard, which Tab
        // from the message field gives it.
        Button("Approve", action: approve)
          .buttonStyle(WorkspaceAgentCardButtonStyle(tint: isDestructive ? theme.danger : theme.primary))
          .help("Approve · ↩, with the card focused (⇥ from the message field)")
        Button("Deny", action: deny)
          .buttonStyle(WorkspaceAgentCardButtonStyle(tint: nil))
          .help("Deny · esc, with the card focused (⇥ from the message field)")
        Spacer(minLength: 0)
        if isFocused {
          Text("return approves · esc denies")
            .font(theme.monoCaptionFont)
            .foregroundStyle(theme.dim)
            .lineLimit(1)
        }
      }
    case .approved, .automatic:
      let prefix = approval.state == .approved ? "Approved · " : ""
      switch approval.outcome {
      case nil: status("\(prefix)running…", theme.muted)
      case .applied: undoRow(prefix: prefix)
      case .failed(let message): status("\(prefix)failed: \(message)", theme.danger)
      }
    case .denied:
      status("Denied · nothing changed", theme.muted)
    case .withdrawn:
      status("Not answered before the thread ended · nothing changed", theme.muted)
    }
  }

  /// Done, and the way back: Undo, or Undone and Redo.
  @ViewBuilder
  private func undoRow(prefix: String) -> some View {
    if approval.undoStep == nil {
      status("\(prefix)done", theme.success)
    } else if let plan, plan.isAvailable {
      HStack(spacing: theme.space.sm) {
        if plan.isUndone {
          status("Undone", theme.muted)
          if plan.canRedo {
            Button("Redo", action: redo)
              .buttonStyle(WorkspaceAgentCardButtonStyle(tint: nil))
              .help("Put this change back · ↩, with the card focused (⇥ from the message field)")
          }
        } else {
          status("\(prefix)done", theme.success)
          Button(Self.undoTitle(plan), action: undo)
            .buttonStyle(WorkspaceAgentCardButtonStyle(tint: nil))
            .help(Self.undoHelp(plan))
        }
        Spacer(minLength: 0)
        if isFocused {
          Text(plan.isUndone ? "return redoes" : "return undoes")
            .font(theme.monoCaptionFont)
            .foregroundStyle(theme.dim)
            .lineLimit(1)
        }
      }
    } else if plan != nil {
      status("\(prefix)done · no longer in the undo history", theme.success)
    } else {
      status("\(prefix)done", theme.success)
    }
  }

  /// "Undo", or "Undo with 2 later changes" when this step can only go back
  /// together with the steps that built on it.
  static func undoTitle(_ plan: WorkspaceStepUndoPlan) -> String {
    switch plan.laterSteps.count {
    case 0: "Undo"
    case 1: "Undo with 1 later change"
    case let count: "Undo with \(count) later changes"
    }
  }

  static func undoHelp(_ plan: WorkspaceStepUndoPlan) -> String {
    let keys = "↩, with the card focused (⇥ from the message field)"
    guard !plan.laterSteps.isEmpty else { return "Take back exactly this change · \(keys)" }
    let later = plan.laterSteps.map(\.label).joined(separator: ", ")
    return "A later change built on this one, so they go back together: \(later) · \(keys)"
  }

  private func status(_ text: String, _ color: Color) -> some View {
    Text(text)
      .font(theme.monoCaptionFont)
      .foregroundStyle(color)
      .lineLimit(3)
      .textSelection(.enabled)
  }
}

/// Lets the panel's Tab reach a card; nothing where the card is selected by
/// the keys of the view it is in instead.
private struct WorkspaceAgentCardFocus: ViewModifier {
  let focusedCard: FocusState<UUID?>.Binding?
  let itemID: UUID
  let isFocusable: Bool

  func body(content: Content) -> some View {
    if let focusedCard {
      content
        .focusable(isFocusable)
        .focused(focusedCard, equals: itemID)
        .focusEffectDisabled()
    } else {
      content
    }
  }
}

/// The card's two buttons: flat and bordered at the control radius, regular
/// weight. `tint` is the status convention — a tinted fill, a border and text
/// of the same hue — for the one action the card is asking about; nil is the
/// plain bordered button beside it.
struct WorkspaceAgentCardButtonStyle: ButtonStyle {
  let tint: Color?

  func makeBody(configuration: Configuration) -> some View {
    WorkspaceAgentCardButtonBody(configuration: configuration, tint: tint)
  }
}

private struct WorkspaceAgentCardButtonBody: View {
  @Environment(\.theme) private var theme
  let configuration: ButtonStyleConfiguration
  let tint: Color?
  @State private var isHovering = false

  var body: some View {
    let shape = RoundedRectangle(cornerRadius: theme.controlRadius, style: .continuous)
    let lit = isHovering || configuration.isPressed
    return configuration.label
      .font(theme.bodyFont())
      .foregroundStyle(tint ?? theme.ink)
      .padding(.horizontal, theme.space.sm)
      .padding(.vertical, theme.space.xxs)
      .background(
        tint.map { $0.opacity(lit ? Theme.statusFillOpacity * 2 : Theme.statusFillOpacity) }
          ?? (lit ? theme.hover : Color.clear), in: shape
      )
      .overlay(
        shape.strokeBorder(
          tint.map { $0.opacity(Theme.statusBorderOpacity) } ?? theme.inputBorder, lineWidth: theme.hairline))
      .contentShape(shape)
      .onHover { isHovering = $0 }
  }
}

/// Shown when the chosen agent could not be found: what the panel needs, how
/// to give it one, and the other agent to switch to if that one is here.
struct WorkspaceAgentSetup: View {
  @Environment(WorkspaceViewModel.self) private var model
  @Environment(\.theme) private var theme
  @State private var path = ""

  var body: some View {
    let client = model.agent.clientSetting
    VStack(alignment: .leading, spacing: theme.space.md) {
      Text(
        client == .custom
          ? "The agent panel is set to run a typed command, and Takt can't find what it starts."
          : "The agent panel is set to run \(client.displayName), which isn't installed where Takt looked.")
        .font(theme.bodyFont())
        .foregroundStyle(theme.ink)
      Text(client.installHint)
        .font(theme.captionFont)
        .foregroundStyle(theme.muted)
      let installed = AgentClient.allCases.filter { $0 != client && model.agent.executablePath(for: $0) != nil }
      ForEach(installed) { other in
        Button("Use \(other.displayName) instead") { model.useAgentClient(other) }
          .buttonStyle(WorkspaceAgentCardButtonStyle(tint: theme.primary))
          .help("\(other.displayName) is installed · switch the panel to it")
      }
      VStack(alignment: .leading, spacing: theme.space.xxs) {
        MicroLabel("Looked in")
        ForEach(model.agent.executableCandidates, id: \.self) { candidate in
          Text(candidate)
            .font(theme.monoCaptionFont)
            .foregroundStyle(theme.dim)
            .lineLimit(1)
            .truncationMode(.middle)
        }
      }
      HStack(spacing: theme.space.xs) {
        TextField(client == .custom ? "dense claude" : "/path/to/\(client.executableName)", text: $path)
          .textFieldStyle(.plain)
          .font(theme.monoFont(size: theme.scale.caption))
          .padding(.horizontal, theme.space.sm)
          .padding(.vertical, theme.space.xxs)
          .overlay(
            RoundedRectangle(cornerRadius: theme.controlRadius)
              .strokeBorder(theme.inputBorder, lineWidth: theme.hairline))
          .onSubmit(usePath)
          .help(client == .custom ? "↩ uses this command" : "↩ uses this path")
        Button("Use", action: usePath)
          .buttonStyle(WorkspaceAgentCardButtonStyle(tint: nil))
          .help(client == .custom ? "Use this command · ↩ in the field" : "Use this path · ↩ in the field")
      }
      if !model.agent.userExecutablePath.isEmpty {
        Text(
          client == .custom
            ? "\(AgentClient.words(of: model.agent.userExecutablePath).first ?? "") is not an executable Takt can find."
            : "\(model.agent.userExecutablePath) is not an executable file.")
          .font(theme.captionFont)
          .foregroundStyle(theme.danger)
      }
    }
    .padding(theme.listGutter)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .onAppear { path = model.agent.userExecutablePath }
    .onChange(of: client) { _, _ in path = model.agent.userExecutablePath }
  }

  /// A typed command is kept as typed; `AgentClient.candidates` expands its
  /// binary's `~` when it looks.
  private func usePath() {
    model.agent.userExecutablePath =
      model.agent.clientSetting == .custom ? path : (path as NSString).expandingTildeInPath
  }
}
