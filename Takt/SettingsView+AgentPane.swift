import SwiftUI
import TaktCore
import TaktWorkspace

/// The Agent page: which agent the panel runs, the standing instructions for
/// the whole account, and where the narrower ones are written.
extension SettingsView {
  @ViewBuilder
  var agentPane: some View {
    agentClientSection
    let instructions = workspace.agentInstructions
    Section {
      AgentInstructionsField(
        prompt: "e.g. Write task titles as verbs. I work Monday to Thursday.",
        text: Binding(get: { instructions.account }, set: { instructions.account = $0 }))
    } header: {
      SettingsHeader("Instructions for every thread")
    } footer: {
      Text(
        "Sent ahead of your first message in a thread, and again whenever they change. "
          + "A folder's and a list's own instructions are written in its settings (oo on the list) "
          + "and follow these, the narrowest last, so they win where they disagree.")
    }

    let agent = workspace.agent
    Section {
      SettingsToggleRow(
        "Ask before changing tasks",
        isOn: Binding(get: { agent.asksBeforeChangesSetting }, set: { agent.asksBeforeChangesSetting = $0 }))
    } header: {
      SettingsHeader("Changes")
    } footer: {
      Text(
        agent.asksBeforeChangesSetting
          ? agent.wire(for: agent.clientSetting).canAskBeforeChanges
            ? "Each change the agent wants to make waits for Approve. Takes effect from the next thread."
            : "\(agent.clientSetting.displayName) cannot stop to ask, so with this on it only reads: it says what "
              + "it would change and leaves the change to you. Takes effect from the next thread."
          : "The agent's changes apply straight away, each shown in the panel with an Undo. "
            + "Takes effect from the next thread.")
    }

    let written = agentWrittenScopes
    if !written.isEmpty {
      Section {
        ForEach(written, id: \.id) { scope in
          SettingsRow(scope.label, detail: scope.text) {
            Button("Edit…") {
              workspace.editAgentInstructions(scope)
              AppDelegate.shared.showMainWindow()
            }
          }
        }
      } header: {
        SettingsHeader("Written for folders and lists")
      }
    }
  }

  /// Which agent the panel and the quick ask run, with whether each is on
  /// this Mac. The panel's header has the same choice.
  @ViewBuilder
  private var agentClientSection: some View {
    let agent = workspace.agent
    Section {
      SettingsRow("Agent", detail: agentClientDetail) {
        ThemedMenuPicker(
          "Agent",
          selection: Binding(get: { agent.clientSetting }, set: { agent.clientSetting = $0 }),
          options: AgentClient.allCases.map { ThemedPickerOption(agent.menuLabel(for: $0), value: $0) })
      }
      if agent.clientSetting == .custom {
        customCommandRows
      }
      ForEach(AgentClient.allCases.filter { $0 != .custom }) { client in
        SettingsRow(client.displayName, detail: agent.executablePath(for: client) ?? "Not found where Takt looks") {
          ThemedTag(
            agent.executablePath(for: client) == nil ? "Not installed" : "Installed",
            tint: agent.executablePath(for: client) == nil ? nil : theme.success)
        }
      }
    } header: {
      SettingsHeader("Agent")
    } footer: {
      Text(
        "The panel runs your own agent, signed in as you, with Takt's tools and nothing else. "
          + "A thread keeps the agent it started with; a change applies from the next thread. "
          + "The palette's \"Use agent\" commands and the panel's header switch it too.")
    }
  }

  /// The typed command and the format it speaks. The command is stored
  /// whole, arguments and all, where the others keep a hand-set path.
  @ViewBuilder
  private var customCommandRows: some View {
    let agent = workspace.agent
    let command = agent.userExecutablePaths[.custom] ?? ""
    let resolved = agent.executablePath(for: .custom)
    VStack(alignment: .leading, spacing: theme.space.xs) {
      Text("Command")
        .font(theme.bodyFont())
        .foregroundStyle(theme.ink)
      TextField(
        "",
        text: Binding(get: { command }, set: { agent.userExecutablePaths[.custom] = $0 }),
        prompt: Text("dense claude"))
        .themedTextField()
        .font(theme.monoFont(size: theme.scale.caption))
        .labelsHidden()
        .autocorrectionDisabled()
      Text(
        command.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
          ? "What you would type in a terminal. Aliases aren't seen, so name what the alias runs."
          : resolved.map { "Runs \($0)" }
            ?? "\(AgentClient.words(of: command).first ?? command) isn't an executable Takt can find. "
            + "Give its full path, or put it in ~/.local/bin.")
        .font(theme.captionFont)
        .foregroundStyle(command.isEmpty || resolved != nil ? theme.muted : theme.danger)
        .fixedSize(horizontal: false, vertical: true)
    }
    SettingsRow(
      "Speaks",
      detail: "The format the command reads and writes. Takt adds that format's arguments after what you typed."
    ) {
      ThemedMenuPicker(
        "Speaks",
        selection: Binding(get: { agent.customWireSetting }, set: { agent.customWireSetting = $0 }),
        options: AgentWire.forCustomCommands.map { ThemedPickerOption($0.displayName, value: $0) })
    }
  }

  private var agentClientDetail: String {
    let agent = workspace.agent
    if agent.isRunning, agent.client != agent.clientSetting {
      return "The thread in the panel stays on \(agent.client.displayName) until you start a new one."
    }
    if agent.clientSetting == .custom {
      return "Runs the command below, speaking \(agent.customWireSetting.displayName)'s format."
    }
    if agent.executablePath == nil {
      return "\(agent.clientSetting.displayName) isn't installed where Takt looks. The agent panel takes a path to it."
    }
    return "Which coding agent the agent panel and quick ask (aa) run."
  }

  /// Every folder and list with instructions, folders first, by name.
  private var agentWrittenScopes: [AgentInstructionScope] {
    let instructions = workspace.agentInstructions
    let folders = workspace.folders
      .compactMap { folder -> AgentInstructionScope? in
        let text = instructions.folder(folder.id)
        return text.isEmpty ? nil : AgentInstructionScope(kind: .folder, id: folder.id, name: folder.name, text: text)
      }
    let lists = workspace.lists
      .compactMap { list -> AgentInstructionScope? in
        let text = instructions.list(list.id)
        return text.isEmpty ? nil : AgentInstructionScope(kind: .list, id: list.id, name: list.name, text: text)
      }
    return (folders + lists).sorted { ($0.kind == .folder ? 0 : 1, $0.name) < ($1.kind == .folder ? 0 : 1, $1.name) }
  }
}
