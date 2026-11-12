import SwiftUI
import TaktCore
import TaktWorkspace

/// What Today needs to say where you are: the context, as Focus keeps it,
/// and the catalogue with how many open tasks need each condition.
@MainActor
@Observable
final class NowConditionsModel {
  private(set) var focus: FocusModel?
  let catalogue = StoreQuery(ConditionsCatalogue.empty)

  /// Reads the catalogue again, and the context as the workspace holds it.
  func load(_ model: WorkspaceModel) async {
    if let focus { focus.restoreContext() } else { focus = FocusModel(model: model) }
    let workspaceID = model.workspace.id
    await catalogue.load(model.store) { store in try ConditionsCatalogue.load(store: store, workspaceID: workspaceID) }
  }

  /// What "where you are" offers: what an open task needs, and whatever is on.
  var available: [TaskConditionSection] {
    NowConditionsSheet.available(catalogue.value, active: Set(focus.map { Array($0.holdings.keys) } ?? []))
  }

  /// What holds, what the settings said provide included.
  var summary: String? {
    ConditionPicker.nowSummary(catalogue.value.conditions, selected: focus?.heldIDs ?? [])
  }
}

/// Where you are, from Today: one row saying what holds now — "Home ·
/// Laptop", or that nothing does — that opens the sheet to change it. Today
/// leaves it out when no open task needs a condition and none is on, since
/// then the answer changes nothing.
struct NowConditionsBar: View {
  @Environment(\.theme) private var theme
  let conditions: NowConditionsModel
  @State private var showsSheet = false

  var body: some View {
    let summary = conditions.summary
    Button {
      showsSheet = true
    } label: {
      HStack(spacing: theme.space.sm) {
        Image(systemName: "mappin.and.ellipse")
          .foregroundStyle(summary == nil ? theme.muted : theme.primary)
          .frame(width: 20)
        Text(summary ?? "Say where you are")
          .font(theme.type.body)
          .foregroundStyle(summary == nil ? theme.muted : theme.ink)
          .lineLimit(1)
          .frame(maxWidth: .infinity, alignment: .leading)
        if conditions.focus?.isContextStale == true {
          Tag(text: "Still true?", tint: theme.warning)
        }
        Image(systemName: "chevron.right").imageScale(.small).foregroundStyle(theme.dim)
      }
      .padding(.horizontal, theme.space.lg)
      .frame(minHeight: theme.touchTarget)
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .overlay(alignment: .bottom) { Hairline() }
    .accessibilityLabel("Where you are")
    .accessibilityValue(summary ?? "Nothing said")
    .accessibilityHint("Changes which conditions hold now")
    .accessibilityIdentifier("today.conditions")
    .sheet(isPresented: $showsSheet) {
      if let focus = conditions.focus { NowConditionsSheet(focus: focus).macSheet() }
    }
  }
}

/// Say what holds now: every condition an open task needs, under its
/// category, one tap each. Location is one place at a time, so its rows are
/// radios. Search reaches every condition, and a name nothing is called yet
/// makes one and turns it on.
struct NowConditionsSheet: View {
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.theme) private var theme
  @Bindable var focus: FocusModel
  @State private var catalogue = StoreQuery(ConditionsCatalogue.empty)
  @State private var query = ""
  /// How each day starts, shown here and set on the Mac or with `takt ws
  /// day-start`.
  @State private var dayStart: DayStart?

  /// "Each day starts at 04:00 with Home", or nil while what holds carries over.
  static func dayStartSummary(_ day: DayStart?, conditions: [TaskCondition]) -> String? {
    guard let day, day.resets else { return nil }
    let names = conditions.filter { day.conditionIDs.contains($0.id) && !$0.isArchived }.map(\.name)
    let hour = String(format: "%02d:00", day.hour)
    return names.isEmpty
      ? "Each day starts at \(hour) with nothing on" : "Each day starts at \(hour) with \(names.joined(separator: " · "))"
  }

  /// The core's rule, as Focus's chips: what an open task needs, plus whatever is on.
  static func available(_ catalogue: ConditionsCatalogue, active: Set<String>) -> [TaskConditionSection] {
    TaskConditionSection.available(
      catalogue.conditions, required: Set(catalogue.usage.filter { $0.value > 0 }.keys), active: active)
  }

  var body: some View {
    let value = catalogue.value
    // Said, provided by a setting said, or switched off for now: offered,
    // so it can be turned the other way.
    let holdings = focus.holdings
    let held = focus.heldIDs
    let offered = Self.available(value, active: Set(holdings.keys))
    let list = ConditionPicker.list(
      offered: offered,
      searchable: value.conditions.filter { !$0.isArchived || holdings[$0.id] != nil },
      all: value.conditions, query: query, isOn: { held.contains($0) }, exclusiveRadio: true)
    ConditionPickerSheet(
      title: "Where you are",
      identifier: "nowConditions",
      query: $query,
      offeredCount: offered.reduce(0) { $0 + $1.conditions.count },
      list: list,
      emptyText: value.active.isEmpty
        ? "No conditions yet. Type one — Home, Campus, Laptop — to make it."
        : "No open task needs a condition. Search to turn one on anyway.",
      detail: { focus.holdingNote($0.condition.id) ?? "" },
      count: { value.usage[$0.condition.id] },
      onToggle: { focus.toggleCondition($0.condition, among: value.conditions) },
      onCreate: create,
      leading: {
        if !focus.conditionIDs.isEmpty || !focus.offIDs.isEmpty {
          Button("Clear") { focus.clearConditions() }
            .accessibilityIdentifier("nowConditions.clear")
        }
      },
      notice: {
        if focus.isContextStale {
          Label("Set on an earlier day — still true?", systemImage: "clock.badge.exclamationmark")
            .font(theme.type.caption)
            .foregroundStyle(theme.muted)
        }
        if let summary = Self.dayStartSummary(dayStart, conditions: value.conditions) {
          Label(summary, systemImage: "sunrise")
            .font(theme.type.caption)
            .foregroundStyle(theme.muted)
            .accessibilityIdentifier("nowConditions.dayStart")
        }
      })
    .task(id: model.revision) {
      focus.restoreContext()
      dayStart = try? model.store.dayStart()
      let workspaceID = model.workspace.id
      await catalogue.load(model.store) { store in try ConditionsCatalogue.load(store: store, workspaceID: workspaceID) }
    }
  }

  private func create(_ name: String) {
    query = ""
    guard let made = model.createCondition(named: name, category: nil) else { return }
    focus.toggleCondition(made, among: catalogue.value.conditions + [made])
  }
}

/// The conditions a task needs, ticked: the Mac's `cc` picker. Every live
/// condition under its category — and any archived one the task still
/// needs, so it can be ticked off. A tick is "and needs"; a second place
/// joins the first as "or". Swiping a row right ticks it as *not* holding
/// ("not Home"), or a "not" off. A name nothing is called yet makes the
/// condition and requires it.
struct TaskConditionsSheet: View {
  @Environment(WorkspaceModel.self) private var model
  let inspector: InspectorModel
  @State private var catalogue = StoreQuery(ConditionsCatalogue.empty)
  @State private var query = ""

  var body: some View {
    let value = catalogue.value
    let groups = inspector.values?.requirementGroups
    let offered = value.conditions.filter { !$0.isArchived || TaskRequirementToggle.requires($0.id, in: groups) }
    let name: (String) -> String = { id in value.conditions.first { $0.id == id }?.name ?? "Missing condition" }
    let negated = { (id: String) in TaskRequirementToggle.requirement(of: id, in: groups)?.isNegated == true }
    let list = ConditionPicker.list(
      offered: TaskConditionSection.sections(offered), searchable: offered, all: value.conditions,
      query: query, isOn: { TaskRequirementToggle.requires($0, in: groups) }, exclusiveRadio: false)
    ConditionPickerSheet(
      title: "Needs",
      subtitle: ConditionPicker.requirementSummary(groups, name: name),
      identifier: "taskConditions",
      query: $query,
      offeredCount: offered.count,
      list: list,
      emptyText: "No conditions yet. Type one — Home, Campus, Floor space — to make it and require it.",
      detail: { row in
        [
          ConditionPicker.alternatives(to: row.condition.id, in: groups, name: name).map { "or \($0)" },
          row.condition.isArchived ? "Archived" : nil,
        ].compactMap { $0 }.joined(separator: " · ")
      },
      count: { value.usage[$0.condition.id] },
      onToggle: { toggle($0.condition.id) },
      onCreate: { name in
        query = ""
        if let made = model.createCondition(named: name, category: nil) { toggle(made.id) }
      },
      label: { row in negated(row.condition.id) ? "not \(row.condition.name)" : row.condition.name },
      onNegate: { row in inspector.negateRequirement(row.condition.id, model: model) })
    .task(id: model.revision) {
      let workspaceID = model.workspace.id
      await catalogue.load(model.store) { store in try ConditionsCatalogue.load(store: store, workspaceID: workspaceID) }
    }
  }

  private func toggle(_ id: String) {
    let places = Set(catalogue.value.conditions.filter(\.isLocation).map(\.id))
    inspector.toggleRequirement(id, places: places, model: model)
  }
}

/// The sheet both pickers are: a title, a search field that is also how a
/// condition is made, and the rows under their headings — each a
/// whole-width toggle at the touch target.
private struct ConditionPickerSheet<Leading: View, Notice: View>: View {
  @Environment(\.theme) private var theme
  @Environment(\.dismiss) private var dismiss
  let title: String
  var subtitle: String?
  let identifier: String
  @Binding var query: String
  let offeredCount: Int
  let list: ConditionPickerList
  let emptyText: String
  var detail: (ConditionPickerRow) -> String = { _ in "" }
  var count: (ConditionPickerRow) -> Int? = { _ in nil }
  let onToggle: (ConditionPickerRow) -> Void
  let onCreate: (String) -> Void
  /// What a row is called: its name, or "not Home" for a negated requirement.
  var label: (ConditionPickerRow) -> String = { $0.condition.name }
  /// Ticks a row as not holding, from a swipe; nil where that means nothing.
  var onNegate: ((ConditionPickerRow) -> Void)?
  @ViewBuilder var leading: () -> Leading
  @ViewBuilder var notice: () -> Notice

  private var isLong: Bool { offeredCount >= ConditionPicker.searchThreshold }

  var body: some View {
    NavigationStack {
      List {
        Section {
          VStack(alignment: .leading, spacing: theme.space.sm) {
            if let subtitle {
              Text(subtitle)
                .font(theme.type.caption)
                .foregroundStyle(theme.muted)
                .lineLimit(2)
                .accessibilityIdentifier("\(identifier).summary")
            }
            notice()
            searchField
          }
          .listRowBackground(theme.paper)
          .listRowSeparator(.hidden)
          .listRowInsets(EdgeInsets(top: theme.space.xs, leading: theme.space.lg, bottom: theme.space.sm, trailing: theme.space.lg))
        }
        if list.isEmpty {
          Text(list.isSearching ? "No condition matches." : emptyText)
            .font(theme.type.body)
            .foregroundStyle(theme.muted)
            .listRowBackground(theme.paper)
            .listRowSeparator(.hidden)
        }
        ForEach(list.sections) { section in
          Section {
            ForEach(section.rows) { row in rowView(row) }
          } header: {
            if let title = section.title {
              Text(title).font(theme.type.caption).foregroundStyle(theme.muted).textCase(nil)
            }
          }
        }
        if let name = list.newName {
          Button { onCreate(name) } label: {
            Label("New condition “\(name)”", systemImage: "plus")
              .font(theme.type.body)
              .foregroundStyle(theme.primary)
              .lineLimit(1)
              .truncationMode(.middle)
              .frame(maxWidth: .infinity, minHeight: theme.touchTarget, alignment: .leading)
              .contentShape(Rectangle())
          }
          .buttonStyle(.plain)
          .listRowBackground(theme.paper)
          .accessibilityIdentifier("\(identifier).new")
        }
      }
      .listStyle(.plain)
      .scrollContentBackground(.hidden)
      .scrollDismissesKeyboard(.immediately)
      .background(theme.paper)
      .navigationTitle(title)
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .topBarLeading) { leading() }
        ToolbarItem(placement: .confirmationAction) {
          Button("Done") { dismiss() }
            .accessibilityIdentifier("\(identifier).done")
        }
      }
    }
    .presentationDetents(isLong ? [.large] : [.medium, .large])
    .accessibilityIdentifier(identifier)
  }

  /// The field that narrows the rows, and makes the name typed when nothing
  /// is called that: Return takes the best match, or makes it.
  private var searchField: some View {
    HStack(spacing: theme.space.sm) {
      Image(systemName: "magnifyingglass").foregroundStyle(theme.muted)
      TextField(isLong ? "Search or add a condition" : "Add a condition", text: $query)
        .font(theme.type.body)
        .submitLabel(.done)
        .autocorrectionDisabled()
        .onSubmit(submit)
        .accessibilityIdentifier("\(identifier).search")
      if !query.isEmpty {
        Button { query = "" } label: {
          Image(systemName: "xmark.circle.fill").foregroundStyle(theme.dim)
        }
        .buttonStyle(.plain)
        .hitTarget()
        .accessibilityLabel("Clear search")
      }
    }
    .padding(.horizontal, theme.space.md)
    .frame(minHeight: theme.touchTarget)
    .background(theme.raised, in: RoundedRectangle(cornerRadius: theme.radius.control))
    .overlay(RoundedRectangle(cornerRadius: theme.radius.control).strokeBorder(theme.inputBorder, lineWidth: theme.stroke))
  }

  private func submit() {
    if list.isSearching, let first = list.sections.first?.rows.first {
      onToggle(first)
      query = ""
    } else if let name = list.newName {
      onCreate(name)
    }
  }

  private func rowView(_ row: ConditionPickerRow) -> some View {
    let line = [row.category, detail(row)].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
    let tasks = count(row) ?? 0
    return Button { onToggle(row) } label: {
      HStack(spacing: theme.space.md) {
        indicator(isOn: row.isOn, round: row.isExclusive)
        VStack(alignment: .leading, spacing: 1) {
          Text(label(row))
            .font(row.isOn ? theme.type.body.weight(.semibold) : theme.type.body)
            .foregroundStyle(row.condition.isArchived ? theme.muted : theme.ink)
            .lineLimit(1)
          if !line.isEmpty {
            Text(line).font(theme.type.footnote).foregroundStyle(theme.muted).lineLimit(1)
          }
        }
        Spacer(minLength: theme.space.sm)
        if tasks > 0 {
          Text("\(tasks)")
            .font(theme.type.numeral)
            .foregroundStyle(theme.dim)
            .accessibilityLabel(tasks == 1 ? "1 open task needs it" : "\(tasks) open tasks need it")
        }
      }
      .frame(minHeight: theme.touchTarget)
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .listRowBackground(theme.paper)
    .listRowSeparatorTint(theme.borderMuted)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(row.isOn ? .isSelected : [])
    .accessibilityIdentifier("conditionRow.\(row.condition.name)")
    .swipeActions(edge: .leading) {
      if let onNegate {
        Button("Not") { onNegate(row) }.tint(theme.primary)
      }
    }
    .accessibilityActions {
      if let onNegate { Button("Not") { onNegate(row) } }
    }
  }

  /// A box, or a round radio for a place: filled azure with a tick when on, a hairline when not.
  private func indicator(isOn: Bool, round: Bool) -> some View {
    let shape = RoundedRectangle(cornerRadius: round ? 11 : theme.radius.control, style: .continuous)
    return ZStack {
      shape.fill(isOn ? theme.primary : Color.clear)
      shape.strokeBorder(isOn ? theme.primary : theme.inputBorder, lineWidth: 1.25)
      if isOn {
        if round {
          Circle().fill(theme.onAccent).frame(width: 8, height: 8)
        } else {
          Image(systemName: "checkmark").font(.system(size: 12, weight: .bold)).foregroundStyle(theme.onAccent)
        }
      }
    }
    .frame(width: 22, height: 22)
    .accessibilityHidden(true)
  }
}

extension ConditionPickerSheet where Leading == EmptyView, Notice == EmptyView {
  init(
    title: String, subtitle: String?, identifier: String, query: Binding<String>, offeredCount: Int,
    list: ConditionPickerList, emptyText: String, detail: @escaping (ConditionPickerRow) -> String,
    count: @escaping (ConditionPickerRow) -> Int?, onToggle: @escaping (ConditionPickerRow) -> Void,
    onCreate: @escaping (String) -> Void, label: @escaping (ConditionPickerRow) -> String = { $0.condition.name },
    onNegate: ((ConditionPickerRow) -> Void)? = nil
  ) {
    self.init(
      title: title, subtitle: subtitle, identifier: identifier, query: query, offeredCount: offeredCount, list: list,
      emptyText: emptyText, detail: detail, count: count, onToggle: onToggle, onCreate: onCreate,
      label: label, onNegate: onNegate, leading: { EmptyView() }, notice: { EmptyView() })
  }
}
