import TaktCore
import TaktWorkspace
import SwiftUI

/// Focus: say where you are and how long you have, pick a rung of the
/// ranked ladder, decide the estimate, begin — then the running block, and
/// the question of how it went.
struct FocusScreen: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @State private var focus: FocusModel?

  var body: some View {
    Group {
      if let focus {
        FocusContent(focus: focus)
      } else {
        Color.clear
      }
    }
    .background(theme.paper)
    .navigationTitle("Focus")
    .navigationBarTitleDisplayMode(.inline)
    .toolbar { WorkspaceToolbar() }
    .onAppear {
      if focus == nil { focus = FocusModel(model: model) }
    }
  }
}

private struct FocusContent: View {
  @Environment(WorkspaceModel.self) private var model
  @Bindable var focus: FocusModel

  var body: some View {
    Group {
      if focus.session != nil {
        FocusRunningView(focus: focus)
      } else {
        FocusPlanningView(focus: focus)
      }
    }
    .task(id: QueryKey(revision: model.revision, scope: focus.contextVersion)) {
      await focus.load()
    }
    .onAppear { focus.takeRequest() }
    .onChange(of: model.focusRequestTaskID) { _, _ in focus.takeRequest() }
    .task {
      // The screen's heartbeat: context expiry, the clock-jump rebase, the
      // ranking's own boundaries, checkpoints and the end of the block.
      while !Task.isCancelled {
        focus.tick()
        try? await Task.sleep(for: .seconds(1))
      }
    }
    .confirmationDialog(
      "Start anyway?", isPresented: overrideBinding, titleVisibility: .visible, presenting: focus.startOverride
    ) { _ in
      Button("Start anyway") { focus.confirmOverride() }
      Button("Cancel", role: .cancel) { focus.startOverride = nil }
    } message: { pending in
      Text(pending.explanation)
    }
  }

  private var overrideBinding: Binding<Bool> {
    Binding(get: { focus.startOverride != nil }, set: { if !$0 { focus.startOverride = nil } })
  }
}

// MARK: - Planning

private struct FocusPlanningView: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Bindable var focus: FocusModel
  @State private var showsBlocked = false

  var body: some View {
    List {
      Section {
        FocusPointsStrip(points: focus.snapshot.points, award: focus.lastAward, outcome: focus.lastOutcome) {
          focus.dismissAward()
        }
        FocusContextControls(focus: focus)
      }
      .listRowBackground(theme.paper)
      .listRowSeparator(.hidden)

      if focus.stagedTaskID != nil {
        Section {
          FocusStagedCard(focus: focus)
        } header: {
          header("Staged")
        }
        .listRowBackground(theme.paper)
        .listRowSeparator(.hidden)
      }

      Section {
        if focus.ladder.isEmpty, focus.isLoaded {
          EmptyState(
            title: "Nothing to do here", message: "Nothing fits this context and time. Change them, or add a task.",
            systemImage: "scope")
          .listRowBackground(theme.paper)
        }
        ForEach(Array(focus.ladder.enumerated()), id: \.element.id) { index, rung in
          FocusRungRow(focus: focus, rung: rung, index: index)
        }
        .onMove { focus.moveRungs(from: $0, to: $1) }
      } header: {
        HStack {
          header("Ladder")
          Spacer()
          if focus.snapshot.hasManualOrder {
            Button("Reset order") { focus.resetOrder() }
              .font(theme.type.caption)
              .accessibilityIdentifier("focus.resetOrder")
          }
        }
      }

      if !focus.snapshot.blocked.isEmpty {
        Section {
          DisclosureGroup(isExpanded: $showsBlocked) {
            ForEach(focus.snapshot.blocked) { blocked in
              VStack(alignment: .leading, spacing: theme.space.xxs) {
                Text(blocked.candidate.title).font(theme.type.body).foregroundStyle(theme.muted)
                Text(blocked.reasons.map(focus.unavailableDescription).joined(separator: " · "))
                  .font(theme.type.footnote).foregroundStyle(theme.dim)
              }
              .contextMenu {
                Button { focus.stage(blocked.id) } label: { Label("Stage anyway", systemImage: "target") }
              }
            }
          } label: {
            Text("Not available now · \(focus.snapshot.blocked.count)")
              .font(theme.type.callout).foregroundStyle(theme.muted)
          }
        }
        .listRowBackground(theme.paper)
      }
    }
    .listStyle(.plain)
    .scrollContentBackground(.hidden)
    .accessibilityIdentifier("focus.ladder")
  }

  private func header(_ text: String) -> some View {
    Text(text).font(theme.type.caption).foregroundStyle(theme.muted).textCase(nil)
  }
}

/// Points today, this week and ever, and what the last block earned.
private struct FocusPointsStrip: View {
  @Environment(\.theme) private var theme
  let points: FocusPointsSummary
  let award: FocusAward?
  let outcome: WorkspaceStore.FocusCompletionOutcome?
  let onDismiss: () -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: theme.space.sm) {
      if let award {
        HStack(spacing: theme.space.sm) {
          Image(systemName: "sparkles").foregroundStyle(theme.warning)
          Text("+\(FocusPoints.formatted(award.points)) points")
            .font(theme.type.numeralBody).foregroundStyle(theme.ink)
          Text(outcomeText).font(theme.type.caption).foregroundStyle(theme.muted)
          Spacer()
          Button(action: onDismiss) { Image(systemName: "xmark").foregroundStyle(theme.dim) }
            .buttonStyle(.plain)
            .accessibilityLabel("Dismiss")
        }
        .padding(theme.space.md)
        .background(theme.warning.opacity(0.10), in: RoundedRectangle(cornerRadius: theme.radius.panel))
        .overlay(RoundedRectangle(cornerRadius: theme.radius.panel).strokeBorder(theme.warning.opacity(0.4), lineWidth: theme.stroke))
      }
      HStack(spacing: theme.space.lg) {
        stat("Today", FocusPoints.formatted(points.today))
        stat("7 days", FocusPoints.formatted(points.last7Days))
        stat("All time", FocusPoints.formatted(points.allTime))
        stat("Blocks", "\(points.blocksToday)")
      }
    }
    .accessibilityIdentifier("focus.points")
  }

  private var outcomeText: String {
    switch outcome {
    case .taskCompleted: "Task done"
    case .progressLogged(let seconds): "Logged \(Format.duration(seconds))"
    case .contributionLogged(let seconds): "Daily +\(Format.duration(seconds))"
    case nil: ""
    }
  }

  private func stat(_ label: String, _ value: String) -> some View {
    VStack(alignment: .leading, spacing: 1) {
      Text(value).font(theme.type.numeralBody).foregroundStyle(theme.ink)
      Text(label).font(theme.type.footnote).foregroundStyle(theme.muted)
    }
  }
}

/// Where you are (condition chips), how long you have, and whether you want
/// to make progress or finish something.
private struct FocusContextControls: View {
  @Environment(\.theme) private var theme
  @Bindable var focus: FocusModel
  @State private var isAddingCondition = false
  @State private var newCondition = ""
  @State private var newCategory = ""
  @State private var showsConditions = false
  @State private var choosesConditions = false

  var body: some View {
    VStack(alignment: .leading, spacing: theme.space.md) {
      ScrollView(.horizontal, showsIndicators: false) {
        HStack(spacing: theme.space.sm) {
          if focus.isLoaded, focus.conditionSections.isEmpty {
            Text("No open tasks need a condition")
              .font(theme.type.caption)
              .foregroundStyle(theme.muted)
              .accessibilityIdentifier("focus.conditionsEmpty")
          }
          ForEach(focus.conditionSections) { section in
            // Only what an open task needs, under its category: a condition
            // nothing waits on changes nothing, so it is not asked about.
            Text(section.title)
              .font(theme.type.caption)
              .foregroundStyle(theme.muted)
              .accessibilityAddTraits(.isHeader)
            ForEach(section.conditions) { condition in
              chip(condition, isExclusive: section.isExclusive)
            }
          }
          // Every condition in one sheet, searched, for when the chips are many.
          Button {
            choosesConditions = true
          } label: {
            Image(systemName: "magnifyingglass")
              .foregroundStyle(theme.muted)
              .frame(width: theme.touchTarget, height: theme.touchTarget)
              .overlay(RoundedRectangle(cornerRadius: theme.radius.control).strokeBorder(theme.border, lineWidth: theme.stroke))
          }
          .buttonStyle(.plain)
          .accessibilityLabel("All conditions")
          .accessibilityIdentifier("focus.allConditions")
          Button {
            isAddingCondition = true
          } label: {
            Image(systemName: "plus")
              .foregroundStyle(theme.muted)
              .frame(width: theme.touchTarget, height: theme.touchTarget)
              .overlay(RoundedRectangle(cornerRadius: theme.radius.control).strokeBorder(theme.border, lineWidth: theme.stroke))
          }
          .buttonStyle(.plain)
          .accessibilityLabel("New condition")
          Button {
            showsConditions = true
          } label: {
            Image(systemName: "slider.horizontal.3")
              .foregroundStyle(theme.muted)
              .frame(width: theme.touchTarget, height: theme.touchTarget)
              .overlay(RoundedRectangle(cornerRadius: theme.radius.control).strokeBorder(theme.border, lineWidth: theme.stroke))
          }
          .buttonStyle(.plain)
          .accessibilityLabel("Manage conditions")
          .accessibilityIdentifier("focus.manageConditions")
        }
        .padding(.vertical, 1)
      }
      if focus.isContextStale {
        staleContext
      }
      if !focus.suggestedContextIDs.isEmpty {
        suggestedContext
      }
      HStack(spacing: theme.space.sm) {
        Menu {
          Button("No time limit") { focus.setAvailable(minutes: nil) }
          ForEach([15, 30, 60], id: \.self) { minutes in
            Button("\(minutes) minutes") { focus.setAvailable(minutes: minutes) }
          }
          Menu("Custom") {
            ForEach([10, 20, 45, 90, 120], id: \.self) { minutes in
              Button("\(minutes) minutes") { focus.setAvailable(minutes: minutes) }
            }
          }
          Button("Until…") {
            focus.availableUntil = focus.availableUntil ?? Date.now.addingTimeInterval(3_600)
          }
        } label: {
          Label(timeTitle, systemImage: "clock")
            .font(theme.type.callout)
            .controlFrame(expands: false)
        }
        .accessibilityIdentifier("focus.time")
        if let until = focus.availableUntil {
          DatePicker("Until", selection: Binding(get: { until }, set: { focus.availableUntil = $0 }),
                     in: Date.now..., displayedComponents: .hourAndMinute)
            .labelsHidden()
        }
        Spacer(minLength: 0)
      }
      contextExpiry
      Picker("Mode", selection: $focus.mode) {
        Text("Make progress").tag(FocusTimeMode.progress)
        Text("Finish something").tag(FocusTimeMode.finish)
      }
      .pickerStyle(.segmented)
      .accessibilityIdentifier("focus.mode")
    }
    .alert("New condition", isPresented: $isAddingCondition) {
      TextField("Name", text: $newCondition)
      TextField("Category (optional)", text: $newCategory)
      Button("Cancel", role: .cancel) { newCondition = ""; newCategory = "" }
      Button("Add") { add() }
    } message: {
      Text("A place, a tool or a state you need to be in for some tasks, filed under a category — "
        + "Location, Equipment… Location is one place at a time.")
    }
    .sheet(isPresented: $showsConditions) { ConditionsSheet() }
    .sheet(isPresented: $choosesConditions) { NowConditionsSheet(focus: focus) }
  }

  /// Said on an earlier day, here or on another device: probably not where
  /// you are now. The Mac's notice, in the same words.
  private var staleContext: some View {
    HStack(spacing: theme.space.sm) {
      Image(systemName: "clock.badge.exclamationmark")
        .foregroundStyle(theme.warning)
      Text(focus.conditionsSetAt.map {
        "Set \($0.formatted(.relative(presentation: .named))) — still true?"
      } ?? "Set on an earlier day — still true?")
        .font(theme.type.caption)
        .foregroundStyle(theme.muted)
        .lineLimit(2)
      Spacer(minLength: 0)
      Button("Clear") { focus.clearConditions() }
        .font(theme.type.caption)
        .foregroundStyle(theme.primary)
        .accessibilityIdentifier("focus.clearStaleContext")
    }
    .buttonStyle(.plain)
  }

  /// "Last context: Office, Laptop — Use again". What an expired context
  /// leaves behind: offered back, not assumed.
  private var suggestedContext: some View {
    HStack(spacing: theme.space.sm) {
      Text("Last context: " + focus.suggestedContextIDs.compactMap { id in
        focus.snapshot.conditions.first { $0.id == id }?.name
      }.sorted().joined(separator: ", "))
        .font(theme.type.caption)
        .foregroundStyle(theme.muted)
        .lineLimit(2)
      Spacer(minLength: 0)
      Button("Use again") { focus.confirmSuggestedContext() }
        .font(theme.type.caption)
        .foregroundStyle(theme.primary)
        .accessibilityIdentifier("focus.useContextAgain")
      Button("Dismiss") { focus.dismissSuggestedContext() }
        .font(theme.type.caption)
        .foregroundStyle(theme.muted)
    }
    .buttonStyle(.plain)
  }

  /// "These conditions hold until…": past it the context lapses and is
  /// offered back. The available time also stops there.
  private var contextExpiry: some View {
    HStack(spacing: theme.space.sm) {
      Toggle("Context expires", isOn: Binding(
        get: { focus.contextExpiresAt != nil }, set: { focus.setContextExpires($0) }))
        .toggleStyle(ThemedToggleStyle())
        .fixedSize()
        .accessibilityIdentifier("focus.contextExpires")
      Spacer(minLength: 0)
      if let expires = focus.contextExpiresAt {
        DatePicker("Expires", selection: Binding(get: { expires }, set: { focus.contextExpiresAt = $0 }),
                   in: Date.now..., displayedComponents: .hourAndMinute)
          .labelsHidden()
          .accessibilityIdentifier("focus.contextExpiresAt")
      }
    }
  }

  private var timeTitle: String {
    guard let until = focus.availableUntil else { return "Free time: any length" }
    let minutes = max(0, Int(until.timeIntervalSinceNow / 60))
    return "Free time: \(minutes)m · until \(Format.time(until))"
  }

  private func add() {
    let name = newCondition.trimmingCharacters(in: .whitespacesAndNewlines)
    let category = TaskConditionSection.normalizedCategory(newCategory)
    newCondition = ""
    newCategory = ""
    guard !name.isEmpty else { return }
    focus.createCondition(named: name, category: category)
  }

  /// A condition you can say holds; a pin under Location, where choosing one
  /// replaces the last. One a setting provides is on with where it comes
  /// from ("from Home"); tapping it switches it off for now.
  private func chip(_ condition: TaskCondition, isExclusive: Bool) -> some View {
    let isOn = focus.heldIDs.contains(condition.id)
    let note = focus.holdingNote(condition.id)
    return Button {
      focus.toggleCondition(condition)
    } label: {
      HStack(spacing: theme.space.xs) {
        if isExclusive { Image(systemName: "mappin").imageScale(.small) }
        Text(condition.name)
        if let note {
          Text(note).font(theme.type.caption).foregroundStyle(theme.muted)
        }
      }
      .font(theme.type.callout)
      .foregroundStyle(isOn ? theme.primary : theme.ink)
      .padding(.horizontal, theme.space.md)
      .frame(minHeight: theme.touchTarget)
      .background(isOn ? theme.primary.opacity(0.10) : theme.raised,
        in: RoundedRectangle(cornerRadius: theme.radius.control))
      .overlay(RoundedRectangle(cornerRadius: theme.radius.control)
        .strokeBorder(isOn ? theme.primary.opacity(0.5) : theme.border, lineWidth: theme.stroke))
    }
    .buttonStyle(.plain)
    .accessibilityAddTraits(isOn ? .isSelected : [])
    .accessibilityValue(note ?? "")
    .accessibilityIdentifier("focus.condition.\(condition.name)")
  }

}

/// The task about to be started and the estimate it is started with.
private struct FocusStagedCard: View {
  @Environment(\.theme) private var theme
  @Bindable var focus: FocusModel

  var body: some View {
    VStack(alignment: .leading, spacing: theme.space.md) {
      Text(focus.stagedTitle ?? "")
        .font(theme.type.title)
        .foregroundStyle(theme.ink)
      if let rung = focus.stagedRung {
        Text(focus.explanation(for: rung).capitalizedFirst)
          .font(theme.type.caption).foregroundStyle(theme.muted)
      }
      HStack(spacing: theme.space.md) {
        Text(Format.duration(Int(focus.estimateMinutes * 60)))
          .font(theme.type.numeralBody)
          .foregroundStyle(theme.ink)
          .frame(minWidth: 64, alignment: .leading)
        Stepper("Estimate", value: $focus.estimateMinutes, in: 1...480, step: 5)
          .labelsHidden()
          .accessibilityIdentifier("focus.estimate")
        Spacer(minLength: 0)
      }
      HStack(spacing: theme.space.sm) {
        ForEach([15, 25, 45, 60, 90], id: \.self) { minutes in
          Button("\(minutes)m") { focus.estimateMinutes = Double(minutes) }
            .buttonStyle(ThemedButtonStyle(kind: Int(focus.estimateMinutes) == minutes ? .primary : .quiet, compact: true))
            .font(theme.type.numeral)
        }
      }
      HStack(spacing: theme.space.sm) {
        Button {
          focus.beginStaged()
        } label: {
          Label("Begin", systemImage: "play.fill").frame(maxWidth: .infinity)
        }
        .buttonStyle(ThemedButtonStyle(kind: .primary))
        .accessibilityIdentifier("focus.begin")
        Button("Back") { focus.unstage() }
          .buttonStyle(ThemedButtonStyle(kind: .quiet))
      }
    }
    .padding(theme.space.lg)
    .cardSurface(selected: true)
  }
}

private struct FocusRungRow: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Bindable var focus: FocusModel
  let rung: ScoredNextUp
  let index: Int

  private func tickOff() {
    let id = rung.id
    model.celebrate(id) { [focus] in focus.completeWithoutSession(id) }
  }

  var body: some View {
    let isStaged = focus.stagedTaskID == rung.id
    Button {
      focus.stage(rung.id)
    } label: {
      HStack(alignment: .firstTextBaseline, spacing: theme.space.md) {
        Text("\(index + 1)")
          .font(theme.type.numeral)
          .foregroundStyle(index == 0 ? theme.primary : theme.dim)
          .frame(width: 22, alignment: .trailing)
        VStack(alignment: .leading, spacing: theme.space.xxs) {
          Text(rung.candidate.title)
            .font(index == 0 ? theme.type.bodyMedium : theme.type.body)
            .foregroundStyle(theme.ink)
            .lineLimit(2)
            .celebrationStrike(rung.id)
          Text(focus.explanation(for: rung).capitalizedFirst)
            .font(theme.type.footnote)
            .foregroundStyle(theme.muted)
            .lineLimit(2)
        }
        Spacer(minLength: theme.space.sm)
        if rung.candidate.isDailyDueToday {
          Image(systemName: "repeat").font(theme.type.glyph(11)).foregroundStyle(theme.purple)
            .accessibilityLabel("Daily")
        }
        if let remaining = rung.candidate.remainingSeconds, remaining > 0 {
          Text(Format.duration(remaining)).font(theme.type.numeral).foregroundStyle(theme.muted)
        }
      }
      .padding(.vertical, 6)
      .contentShape(Rectangle())
      .celebrationRow(rung.id)
    }
    .buttonStyle(.plain)
    .listRowBackground(isStaged ? theme.primary.opacity(0.10) : theme.paper)
    .listRowSeparatorTint(theme.borderMuted)
    .accessibilityIdentifier("focus.rung.\(rung.candidate.title)")
    .swipeActions(edge: .leading, allowsFullSwipe: true) {
      Button { tickOff() } label: { Label("Tick off", systemImage: "checkmark") }
        .tint(theme.success)
    }
    .swipeActions(edge: .trailing) {
      Button { focus.deferTask(rung.id, .tomorrow) } label: { Label("Tomorrow", systemImage: "sunrise") }
        .tint(theme.warning)
      Button { focus.deferTask(rung.id, .anHour) } label: { Label("An hour", systemImage: "clock") }
        .tint(theme.muted)
    }
    .contextMenu {
      Button { focus.stage(rung.id) } label: { Label("Stage this", systemImage: "target") }
      Button { tickOff() } label: { Label("Tick off", systemImage: "checkmark") }
      Menu {
        ForEach(FocusDeferral.allCases) { deferral in
          Button(deferral.title) { focus.deferTask(rung.id, deferral) }
        }
      } label: { Label("Later", systemImage: "clock") }
      Divider()
      Button { focus.moveRung(rung.id, by: -1) } label: { Label("Move up", systemImage: "arrow.up") }
        .disabled(index == 0)
      Button { focus.moveRung(rung.id, by: 1) } label: { Label("Move down", systemImage: "arrow.down") }
        .disabled(index == focus.ladder.count - 1)
    }
  }
}

// MARK: - Running

private struct FocusRunningView: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Bindable var focus: FocusModel

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: theme.space.xl) {
        if let session = focus.session, let task = focus.activeTask {
          VStack(alignment: .leading, spacing: theme.space.sm) {
            Text(session.pausedAt == nil ? "Focusing" : "Paused")
              .font(theme.type.caption)
              .foregroundStyle(session.pausedAt == nil ? theme.primary : theme.warning)
            Text(task.title)
              .font(theme.type.largeTitle)
              .foregroundStyle(theme.ink)
              .accessibilityIdentifier("focus.running.title")
          }
          TimelineView(.periodic(from: .now, by: 1)) { context in
            clock(session: session, now: context.date)
          }
          controls(session: session)
          if focus.snapshot.queue.contains(where: { $0.item.state == .queued && $0.task.id != task.id }) {
            queue(activeID: task.id)
          }
        } else {
          EmptyState(
            title: "Nothing in the queue can run now",
            message: "The rest of this session's queue doesn't fit the current context.", systemImage: "pause.circle")
          Button("End session") { focus.finish() }
            .buttonStyle(ThemedButtonStyle(kind: .quiet))
        }
      }
      .padding(theme.space.lg)
      .frame(maxWidth: 640, alignment: .leading)
      .frame(maxWidth: .infinity)
    }
  }

  private func clock(session: FocusSession, now: Date) -> some View {
    let elapsed = session.elapsedSeconds(now: now)
    let planned = max(60, session.workDurationSeconds)
    let fraction = min(1, Double(elapsed) / Double(planned))
    return VStack(alignment: .leading, spacing: theme.space.sm) {
      Text(Format.clock(elapsed))
        .font(theme.type.hero)
        .monospacedDigit()
        .lineLimit(1)
        .minimumScaleFactor(0.5)
        .foregroundStyle(theme.ink)
        .accessibilityIdentifier("focus.clock")
      // A timer has no length, so there is no bar to fill and nothing planned:
      // only the time so far, counting up.
      if session.openEnded {
        Text("so far")
          .font(theme.type.caption).foregroundStyle(theme.muted)
      } else {
        bar(elapsed: elapsed, planned: planned, fraction: fraction)
      }
    }
  }

  private func bar(elapsed: Int, planned: Int, fraction: Double) -> some View {
    VStack(alignment: .leading, spacing: theme.space.sm) {
      GeometryReader { proxy in
        ZStack(alignment: .leading) {
          Rectangle().fill(theme.well)
          Rectangle().fill(elapsed > planned ? theme.warning : theme.primary)
            .frame(width: proxy.size.width * fraction)
        }
      }
      .frame(height: 4)
      .clipShape(RoundedRectangle(cornerRadius: 2))
      Text("of \(Format.duration(planned)) planned")
        .font(theme.type.caption).foregroundStyle(theme.muted)
    }
  }

  private func controls(session: FocusSession) -> some View {
    VStack(spacing: theme.space.sm) {
      Button {
        focus.requestCompletion(completeTask: true)
      } label: {
        Label("Done", systemImage: "checkmark").frame(maxWidth: .infinity)
      }
      .buttonStyle(ThemedButtonStyle(kind: .primary))
      .accessibilityIdentifier("focus.done")
      HStack(spacing: theme.space.sm) {
        Button {
          focus.togglePause()
        } label: {
          Label(session.pausedAt == nil ? "Pause" : "Resume",
                systemImage: session.pausedAt == nil ? "pause.fill" : "play.fill")
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(ThemedButtonStyle(kind: .quiet))
        .accessibilityIdentifier("focus.pause")
        Button {
          focus.requestCompletion(completeTask: false)
        } label: {
          Label("Log and keep", systemImage: "tray.and.arrow.down").frame(maxWidth: .infinity)
        }
        .buttonStyle(ThemedButtonStyle(kind: .quiet))
        .accessibilityIdentifier("focus.logKeep")
      }
      Button("End session") { focus.finish() }
        .font(theme.type.callout)
        .foregroundStyle(theme.muted)
        .padding(.top, theme.space.xs)
        .accessibilityIdentifier("focus.end")
    }
  }

  private func queue(activeID: String) -> some View {
    VStack(alignment: .leading, spacing: 0) {
      Text("Up next in this session").font(theme.type.caption).foregroundStyle(theme.muted)
        .padding(.bottom, theme.space.sm)
      ForEach(focus.snapshot.queue.filter { $0.item.state == .queued && $0.task.id != activeID }) { entry in
        HStack {
          Text(entry.task.title).font(theme.type.body).foregroundStyle(theme.ink)
          Spacer()
          if entry.item.openEnded {
            Text("timer").font(theme.type.numeral).foregroundStyle(theme.muted)
          } else if let planned = entry.item.plannedSeconds {
            Text(Format.duration(planned)).font(theme.type.numeral).foregroundStyle(theme.muted)
          }
        }
        .padding(.vertical, theme.space.sm)
        Hairline(role: .borderMuted)
      }
    }
  }
}

extension String {
  /// "it is due today" → "It is due today".
  var capitalizedFirst: String { prefix(1).uppercased() + dropFirst() }
}
