import TaktWorkspace
import SwiftUI
import UIKit

/// The app's frame: tabs on iPhone, a three-column split view on iPad.
///
/// The iPad layout is chosen by idiom *and* width, so an iPad app in a narrow
/// Slide Over or split-screen column gets the phone's tabs.
struct RootView: View {
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.horizontalSizeClass) private var sizeClass

  private var isPad: Bool {
    UIDevice.current.userInterfaceIdiom == .pad && sizeClass == .regular
  }

  var body: some View {
    @Bindable var navigation = model.navigation
    Group {
      if isPad {
        PadRootView()
      } else {
        PhoneRootView()
      }
    }
    .environment(\.isPadLayout, isPad)
    .sheet(isPresented: $navigation.isQuickAddPresented) {
      QuickAddSheet()
        .environment(model)
        .environment(\.isPadLayout, isPad)
    }
    .sheet(item: movingTask) { item in
      MoveToListSheet(taskID: item.id)
        .environment(model)
    }
    .sheet(isPresented: $navigation.isHistoryPresented) {
      HistorySheet().environment(model)
    }
    .sheet(item: $navigation.habitRequest) { request in
      HabitSheet(request: request).environment(model)
    }
    .sheet(isPresented: $navigation.isSettingsPresented) {
      SettingsScreen().environment(model)
    }
    .sheet(item: pendingBlock) { pending in
      BlockQualityPrompt(pending: pending).environment(model)
    }
    .fullScreenCover(item: notesEditorTask) { item in
      NavigationStack {
        TaskNotesEditor(taskID: item.id)
      }
      .environment(model)
    }
    .modifier(ListsPresenters())
    .recurrenceClashDialog(model)
    .overlay(alignment: .bottom) { ToastView() }
    .sensoryFeedback(trigger: model.completionCount) { _, _ in
      CompletionHaptics.isEnabled() ? .success : nil
    }
    .sensoryFeedback(.impact(weight: .light), trigger: model.undoCount)
    .alert("Something went wrong", isPresented: errorBinding) {
      Button("OK", role: .cancel) { model.errorMessage = nil }
    } message: {
      Text(model.errorMessage ?? "")
    }
    .background(ShakeToUndo())
    .background(KeyboardCommands())
  }

  private var errorBinding: Binding<Bool> {
    Binding(get: { model.errorMessage != nil }, set: { if !$0 { model.errorMessage = nil } })
  }

  /// The one quality prompt, for whichever surface ended the block.
  private var pendingBlock: Binding<PendingBlockCompletion?> {
    Binding(
      get: { model.pendingBlock },
      set: { if $0 == nil, model.pendingBlock != nil { model.cancelBlockCompletion() } })
  }

  private var movingTask: Binding<IdentifiedString?> {
    Binding(
      get: { model.navigation.movingTaskID.map(IdentifiedString.init) },
      set: { model.navigation.movingTaskID = $0?.id })
  }

  private var notesEditorTask: Binding<IdentifiedString?> {
    Binding(
      get: { model.navigation.notesEditorTaskID.map(IdentifiedString.init) },
      set: { model.navigation.notesEditorTaskID = $0?.id })
  }
}

struct IdentifiedString: Identifiable, Hashable {
  let id: String
}

// MARK: - iPhone

struct PhoneRootView: View {
  @Environment(WorkspaceModel.self) private var model

  var body: some View {
    @Bindable var navigation = model.navigation
    TabView(selection: $navigation.tab) {
      Tab(RootTab.today.title, systemImage: RootTab.today.symbol, value: RootTab.today) {
        NavigationStack { TodayScreen() }
      }
      Tab(RootTab.lists.title, systemImage: RootTab.lists.symbol, value: RootTab.lists) {
        NavigationStack(path: $navigation.listPath) {
          ListsTreeView()
            .navigationDestination(for: ListScope.self) { scope in
              ListScreen(scope: scope)
            }
        }
      }
      Tab(RootTab.focus.title, systemImage: RootTab.focus.symbol, value: RootTab.focus) {
        NavigationStack { FocusScreen() }
      }
      Tab(RootTab.review.title, systemImage: RootTab.review.symbol, value: RootTab.review) {
        NavigationStack { ReviewScreen() }
      }
      Tab(RootTab.search.title, systemImage: RootTab.search.symbol, value: RootTab.search, role: .search) {
        NavigationStack { SearchScreen() }
      }
    }
    .sheet(item: inspected) { item in
      NavigationStack {
        TaskInspector(taskID: item.id)
      }
      .environment(model)
      .presentationDetents([.medium, .large])
      .presentationDragIndicator(.visible)
    }
  }

  private var inspected: Binding<IdentifiedString?> {
    Binding(
      get: { model.navigation.inspectedTaskID.map(IdentifiedString.init) },
      set: { model.navigation.inspectedTaskID = $0?.id })
  }
}

// MARK: - Shared chrome

/// The buttons every root screen carries. On iPhone that is Undo alone, with
/// Redo and History on a long press; add is at the thumb (`AddButton`). On
/// iPad, with room to spare, undo, redo and add sit in the bar.
struct WorkspaceToolbar: ToolbarContent {
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.isPadLayout) private var isPad
  /// What the add button does; nil opens quick add for the Inbox.
  var onAdd: (() -> Void)?

  var body: some ToolbarContent {
    ToolbarItemGroup(placement: .topBarTrailing) {
      Button { model.undo() } label: { Image(systemName: "arrow.uturn.backward") }
        .disabled(model.undoLabel == nil)
        .accessibilityLabel(model.undoLabel.map { "Undo \($0)" } ?? "Undo")
        .accessibilityIdentifier("toolbar.undo")
        .contextMenu {
          if model.redoLabel != nil {
            Button("Redo \(model.redoLabel ?? "")") { model.redo() }
          }
          Button("History…") { model.navigation.isHistoryPresented = true }
        }
      if isPad {
        Button { model.redo() } label: { Image(systemName: "arrow.uturn.forward") }
          .disabled(model.redoLabel == nil)
          .accessibilityLabel(model.redoLabel.map { "Redo \($0)" } ?? "Redo")
          .accessibilityIdentifier("toolbar.redo")
        Button {
          if let onAdd { onAdd() } else {
            model.navigation.quickAddListID = nil
            model.navigation.quickAddParentTaskID = nil
            model.navigation.isQuickAddPresented = true
          }
        } label: { Image(systemName: "plus") }
          .accessibilityLabel("Add task")
          .accessibilityIdentifier("toolbar.add")
      }
    }
  }
}

struct ToastView: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model

  var body: some View {
    if let toast = model.toast {
      HStack(spacing: theme.space.md) {
        Text(toast)
          .font(theme.type.callout)
          .foregroundStyle(theme.paper)
          .lineLimit(2)
          .frame(maxWidth: .infinity, alignment: .leading)
        if let action = model.toastAction {
          Button(action.title) { model.runToastAction() }
            .font(theme.type.callout.weight(.semibold))
            .foregroundStyle(theme.paper)
            .frame(minWidth: theme.touchTarget, minHeight: theme.touchTarget)
            .contentShape(Rectangle())
            .accessibilityIdentifier("toast.action")
        }
      }
      .padding(.leading, theme.space.lg)
      .padding(.trailing, model.toastAction == nil ? theme.space.lg : theme.space.sm)
      .padding(.vertical, model.toastAction == nil ? theme.space.md : 0)
      .background(theme.ink, in: RoundedRectangle(cornerRadius: theme.radius.control))
      .padding(.horizontal, theme.space.md)
      .padding(.bottom, 64)
      .transition(.opacity.combined(with: .move(edge: .bottom)))
      .accessibilityElement(children: .contain)
      .accessibilityIdentifier("toast")
      .allowsHitTesting(model.toastAction != nil)
    }
  }
}

/// Add, where the thumb is: a square azure tile at the bottom right of Today
/// and Lists on iPhone. From Today the task starts planned for today, so it
/// lands where it was typed; from a list, it goes into that list.
struct AddButton: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  var plansToday = false
  var action: (() -> Void)?

  var body: some View {
    Button {
      if let action { action() } else {
        let navigation = model.navigation
        navigation.quickAddListID = plansToday ? nil : navigation.currentScope?.listID
        navigation.quickAddParentTaskID = nil
        navigation.quickAddPlansToday = plansToday
        navigation.isQuickAddPresented = true
      }
    } label: {
      Image(systemName: "plus")
        .font(theme.type.glyph(17.5, .semibold))
        .foregroundStyle(theme.onAccent)
        .frame(width: 52, height: 52)
        .background(theme.primary, in: RoundedRectangle(cornerRadius: theme.radius.control))
    }
    .buttonStyle(.plain)
    .padding(theme.space.lg)
    .accessibilityLabel("Add task")
    .accessibilityIdentifier("toolbar.add")
  }
}

/// Shake to undo, the system's gesture, routed to the store's undo journal
/// rather than an `UndoManager` that knows nothing of it.
struct ShakeToUndo: UIViewControllerRepresentable {
  @Environment(WorkspaceModel.self) private var model

  func makeUIViewController(context: Context) -> ShakeController {
    let controller = ShakeController()
    controller.onShake = { [weak model] in model?.confirmShakeUndo() }
    return controller
  }

  func updateUIViewController(_ controller: ShakeController, context: Context) {}

  final class ShakeController: UIViewController {
    var onShake: (() -> Void)?
    override var canBecomeFirstResponder: Bool { true }
    override func viewDidAppear(_ animated: Bool) {
      super.viewDidAppear(animated)
      becomeFirstResponder()
    }
    override func motionEnded(_ motion: UIEvent.EventSubtype, with event: UIEvent?) {
      if motion == .motionShake { onShake?() } else { super.motionEnded(motion, with: event) }
    }
  }
}
