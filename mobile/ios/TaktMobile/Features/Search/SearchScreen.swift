import TaktCore
import TaktWorkspace
import SwiftUI

/// Find: the Mac's command palette, list navigator and search in one field.
/// What you type is matched against the phone's commands and every list by
/// the core's palette scoring, and against task titles and notes by full-text
/// search. With nothing typed it is a launcher: recent commands first, then
/// every place. Return runs the best row.
struct SearchScreen: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.isPadLayout) private var isPad
  @State private var search = SearchModel()
  @State private var recents = FindPalette.recents
  @FocusState private var isFieldFocused: Bool

  var body: some View {
    let query = search.trimmedQuery
    let commands = FindPalette.commands(query, recents: recents)
    let places = FindPalette.places(query, in: FindPlace.all(in: model.structure))
    List {
      if !commands.isEmpty {
        Section {
          ForEach(commands) { command in
            findRow(symbol: "chevron.right", title: command.title, detail: command.group, tint: nil) {
              run(command.id)
            }
            .accessibilityIdentifier("find.command.\(command.id)")
          }
        } header: {
          header("Commands")
        }
      }
      if !places.isEmpty {
        Section {
          ForEach(places) { place in
            findRow(
              symbol: place.symbol, title: place.name, detail: place.detail.isEmpty ? nil : place.detail,
              tint: ListColor.color(hex: place.colorHex)
            ) { open(place) }
            .accessibilityIdentifier("find.place.\(place.name)")
          }
        } header: {
          header("Lists")
        }
      }
      if !query.isEmpty {
        Section {
          ForEach(search.results) { result in
            resultRow(result)
          }
          if search.results.isEmpty && search.searchedQuery == query {
            Text(search.includesCompleted ? "No task matches" : "No open task matches")
              .font(theme.type.caption)
              .foregroundStyle(theme.muted)
              .listRowBackground(theme.paper)
          }
        } header: {
          HStack {
            header(taskHeader)
            Spacer()
            Toggle(isOn: $search.includesCompleted) {
              Text("Include completed").font(theme.type.caption)
            }
            .toggleStyle(.button)
            .buttonStyle(.borderless)
            .tint(theme.primary)
            .accessibilityIdentifier("search.includeCompleted")
          }
        }
      }
    }
    .listStyle(.plain)
    .scrollContentBackground(.hidden)
    .scrollDismissesKeyboard(.immediately)
    .background(theme.paper)
    .searchable(text: $search.query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Commands, lists and tasks")
    .searchFocused($isFieldFocused)
    .onSubmit(of: .search) { submit(commands: commands, places: places) }
    .navigationTitle("Find")
    .navigationBarTitleDisplayMode(.inline)
    .task(id: search.key(revision: model.revision)) {
      // Debounce: a keystroke within 150ms cancels this read before it runs.
      try? await Task.sleep(for: .milliseconds(150))
      guard !Task.isCancelled else { return }
      await search.search(store: model.store, workspaceID: model.workspace.id)
    }
    .onChange(of: model.navigation.searchFocusRequest) { _, _ in isFieldFocused = true }
    .onAppear {
      recents = FindPalette.recents
      if model.navigation.searchFocusRequest > 0 { isFieldFocused = true }
    }
  }

  private var taskHeader: String {
    guard search.searchedQuery == search.trimmedQuery else { return "Tasks" }
    let count = search.results.count
    return count == 1 ? "1 task" : "\(count) tasks"
  }

  private func header(_ text: String) -> some View {
    Text(text).font(theme.type.caption).foregroundStyle(theme.muted).textCase(nil)
  }

  private func findRow(
    symbol: String, title: String, detail: String?, tint: Color?, action: @escaping () -> Void
  ) -> some View {
    Button(action: action) {
      HStack(spacing: theme.space.md) {
        Image(systemName: symbol)
          .font(theme.type.glyph(12.5))
          .foregroundStyle(tint ?? theme.muted)
          .frame(width: 20)
        VStack(alignment: .leading, spacing: 0) {
          Text(title).font(theme.type.body).foregroundStyle(theme.ink).lineLimit(1)
          if let detail {
            Text(detail).font(theme.type.footnote).foregroundStyle(theme.muted).lineLimit(1)
          }
        }
        Spacer(minLength: 0)
      }
      .frame(minHeight: theme.touchTarget)
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .listRowBackground(theme.paper)
    .listRowSeparatorTint(theme.borderMuted)
  }

  /// Return: the best command, else the best place, else the first task.
  private func submit(commands: [PhonePalette.Command], places: [FindPlace]) {
    guard !search.trimmedQuery.isEmpty else { return }
    if let command = commands.first {
      run(command.id)
    } else if let place = places.first {
      open(place)
    } else if let result = search.results.first {
      reveal(result)
    }
  }

  private func run(_ id: String) {
    isFieldFocused = false
    FindPalette.record(id)
    recents = FindPalette.recents
    search.query = ""
    PhoneCommandRunner.run(id, model: model, isPad: isPad)
  }

  private func open(_ place: FindPlace) {
    isFieldFocused = false
    search.query = ""
    model.navigation.open(place.scope, isPad: isPad)
  }

  private func resultRow(_ result: TaskSearchResult) -> some View {
    let task = result.task
    // Waiting on another task, or starting later: greyed as in the lists,
    // and saying which.
    let isBlocked = model.isBlocked(task.id, status: task.status)
    let starts = model.startsLabel(task.id, status: task.status)
    let waiting = model.waitingBadge(task.id, status: task.status)
    return Button {
      reveal(result)
    } label: {
      HStack(alignment: .top, spacing: theme.space.sm) {
        TaskCheckbox(status: task.status, isList: task.isList).padding(.top, theme.space.xxs)
        VStack(alignment: .leading, spacing: theme.space.xxs) {
          Text(task.title)
            .font(theme.type.body)
            .foregroundStyle(task.status == .open && !isBlocked && starts == nil ? theme.ink : theme.muted)
            .strikethrough(task.status != .open, color: theme.dim)
            .lineLimit(2)
          if let snippet = result.notesSnippet, !snippet.isEmpty {
            Text(snippet).font(theme.type.caption).foregroundStyle(theme.muted).lineLimit(2)
          }
          HStack(spacing: theme.space.xs) {
            Image(systemName: "list.bullet").font(theme.type.glyph(9))
            Text(result.list.name)
            if isBlocked {
              Text("Blocked").foregroundStyle(theme.dim)
            }
            if let starts {
              Text(starts).font(theme.type.numeral).foregroundStyle(theme.warning).lineLimit(1)
            }
            if let waiting { WaitingBadgeView(badge: waiting) }
          }
          .font(theme.type.footnote)
          .foregroundStyle(ListColor.color(hex: result.list.colorHex) ?? theme.muted)
        }
        Spacer(minLength: 0)
        if let due = task.dueAt {
          Tag(
            text: Format.due(due),
            tint: task.status == .open && Format.isOverdue(due) ? theme.danger : theme.muted)
        }
      }
      .padding(.vertical, theme.space.xs)
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .listRowBackground(theme.paper)
    .listRowSeparatorTint(theme.borderMuted)
    .contextMenu {
      TaskContextMenu(
        context: TaskMenuContext(
          taskID: task.id, title: task.title, status: task.status, isList: task.isList,
          isPromoted: task.isPromoted == true, isPlanned: model.isPlannedToday(task.id), allowsStructure: false))
    }
    .accessibilityIdentifier("search.result.\(task.title)")
  }

  /// Opens the task's list with the task selected, unfolded to and shown
  /// even when it is done: the same reveal as "Go to the task's list".
  private func reveal(_ result: TaskSearchResult) {
    isFieldFocused = false
    model.revealInList(result.task, isPad: isPad)
  }
}
