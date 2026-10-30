import TaktCore
import TaktWorkspace
import SwiftUI

/// The left dock: the sidebar of lists and the agent panel, as tabs of one
/// column — the right dock's shape, mirrored.
///
/// The agent is a tab rather than a third column because it is used the way
/// the sidebar is: open beside the tasks while you work, put away when you
/// don't need it, and never both at once on a laptop screen. See
/// `WorkspaceViewModel+Dock.swift` for how the tab and the sidebar's keyboard
/// region stay in step.
struct WorkspaceLeftDock: View {
  @Environment(WorkspaceViewModel.self) private var model
  @Environment(\.theme) private var theme
  var focusedArea: FocusState<WorkspaceFocusArea?>.Binding

  var body: some View {
    VStack(spacing: 0) {
      tabBar
      switch model.leftDockTab {
      case .lists:
        WorkspaceSidebarPane(focusedArea: focusedArea)
      case .agent:
        WorkspaceAgentPane()
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .background(theme.paper)
  }

  /// The right dock's tab bar, on the same shared band, with the showing
  /// tab's own actions on its right: the sidebar's new list, new folder,
  /// archive and history glyphs, or the agent's new thread and stop.
  private var tabBar: some View {
    WorkspaceHeaderBand(inset: 0) {
      HStack(spacing: 0) {
        ForEach(WorkspaceLeftDockTab.allCases) { tab in
          WorkspaceDockTabButton(title: tab.title, command: tab.command, isCurrent: model.leftDockTab == tab) {
            model.showLeftDock(tab)
            if tab == .agent { model.focusAgentInput() }
          }
        }
      }
      Spacer(minLength: theme.space.xs)
      Group {
        switch model.leftDockTab {
        case .lists: WorkspaceSidebarActions()
        case .agent: WorkspaceAgentActions()
        }
      }
      .padding(.trailing, theme.space.xs)
    }
  }
}

/// The sidebar's actions as glyphs, the way an editor's project panel carries
/// its own: there is no footer, because the only strip along the foot of the
/// window is the status bar.
struct WorkspaceSidebarActions: View {
  @Environment(WorkspaceViewModel.self) private var model
  @Environment(\.theme) private var theme

  var body: some View {
    HStack(spacing: theme.space.xxs) {
      WorkspacePaneIconButton("plus", title: "New list", command: .listNew) {
        model.requestCreation(.list)
      }
      WorkspacePaneIconButton("folder.badge.plus", title: "New folder", command: .folderNew) {
        model.requestCreation(.folder)
      }
      if !model.archivedLists.isEmpty || !model.archivedNestedLists.isEmpty {
        archiveMenu
      }
      historyMenu
    }
  }

  /// Archived lists, one item each, to put back. Only there while there is
  /// something archived: a menu that opens on nothing is a control that lies.
  private var archiveMenu: some View {
    Menu {
      ForEach(model.archivedLists) { list in
        Button("Restore \(list.name)") { model.restoreList(list) }
      }
      ForEach(model.archivedNestedLists) { task in
        Button("Restore \(task.title)") { model.archiveNestedList(task, archived: false) }
      }
    } label: {
      WorkspacePaneIconGlyph(symbol: "archivebox")
    }
    .menuStyle(.borderlessButton)
    .menuIndicator(.hidden)
    .fixedSize()
    .commandHelp(.listRestore, note: "Restore archived lists")
    .accessibilityLabel("Archived lists")
  }

  /// Undo and redo, each named for what it would undo.
  private var historyMenu: some View {
    Menu {
      Button(model.undoLabel.map { "Undo \($0)" } ?? "Undo") { model.run(.windowUndo) }
        .disabled(model.undoLabel == nil)
        .commandShortcut(.windowUndo)
      Button(model.redoLabel.map { "Redo \($0)" } ?? "Redo") { model.run(.windowRedo) }
        .disabled(model.redoLabel == nil)
        .commandShortcut(.windowRedo)
    } label: {
      WorkspacePaneIconGlyph(symbol: "arrow.uturn.backward")
    }
    .menuStyle(.borderlessButton)
    .menuIndicator(.hidden)
    .fixedSize()
    .commandHelp(.windowUndo, note: "Undo or redo workspace changes")
    .accessibilityLabel("Workspace history")
  }
}

/// The agent's actions: which agent, a new thread, and stop while one is
/// running.
struct WorkspaceAgentActions: View {
  @Environment(WorkspaceViewModel.self) private var model
  @Environment(\.theme) private var theme

  var body: some View {
    HStack(spacing: theme.space.xxs) {
      clientMenu
      WorkspacePaneIconButton("stop.fill", title: "Stop", note: "Stop the assistant. Nothing waiting for approval runs") {
        model.agent.stop()
      }
      .disabled(!model.agent.isRunning)
      WorkspacePaneIconButton("square.and.pencil", title: "New thread", note: "New thread — ends this one") {
        model.agent.newThread()
        model.focusAgentInput()
      }
      .disabled(model.agent.items.isEmpty && !model.agent.isRunning)
    }
  }

  /// The agent new threads run, ticked, each saying whether it is on this
  /// Mac; the palette's "Use agent" commands are the same choice. While a
  /// thread runs on another agent, a way to move to the chosen one now.
  private var clientMenu: some View {
    let agent = model.agent
    return Menu {
      Picker(
        "Agent",
        selection: Binding(get: { agent.clientSetting }, set: { model.useAgentClient($0) })
      ) {
        ForEach(AgentClient.allCases) { client in
          Text(agent.menuLabel(for: client)).tag(client)
        }
      }
      .pickerStyle(.inline)
      if agent.isRunning, agent.client != agent.clientSetting {
        Divider()
        Button("New thread on \(agent.clientSetting.displayName)") {
          agent.newThread()
          model.focusAgentInput()
        }
      }
      Divider()
      Button("Agent Settings…") { AppDelegate.shared.menuSettings(pane: .agent) }
    } label: {
      WorkspacePaneIconGlyph(symbol: "cpu")
    }
    .menuStyle(.borderlessButton)
    .menuIndicator(.hidden)
    .fixedSize()
    .help("Agent: \(agent.clientSetting.displayName) · which agent new threads run")
    .accessibilityLabel("Agent")
    .accessibilityValue(agent.clientSetting.displayName)
  }
}
