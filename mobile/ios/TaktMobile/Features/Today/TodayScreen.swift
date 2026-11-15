import TaktCore
import TaktWorkspace
import SwiftUI
import UIKit

/// The day as a list of cards: what is on today, what it should cost, what it
/// has cost, and the one you are on carrying its own controls. The Mac's
/// `DayView`, for a phone.
struct TodayScreen: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.isPadLayout) private var isPad
  @Environment(\.scenePhase) private var scenePhase
  @State private var today = TodayModel()
  @State private var conditions = NowConditionsModel()
  /// Rows just finished, held where they stood and crossed out for a moment
  /// before they go, so a tap reads as done rather than as vanished.
  @State private var leavingCards: [String: Leaving<DayCard>] = [:]
  @State private var leavingDailies: [String: Leaving<DayDaily>] = [:]

  var body: some View {
    @Bindable var navigation = model.navigation
    List(selection: isPad ? $navigation.selectedTaskID : .constant(nil)) {
      Section {
        DayHeader(day: today.day)
          .listRowInsets(EdgeInsets())
          .listRowBackground(theme.paper)
          .listRowSeparator(.hidden)
          .selectionDisabled()
        if !conditions.available.isEmpty {
          NowConditionsBar(conditions: conditions)
            .listRowInsets(EdgeInsets())
            .listRowBackground(theme.paper)
            .listRowSeparator(.hidden)
            .selectionDisabled()
        }
      }
      CalendarAgendaSection()
      Section {
        ForEach(Array(cards.enumerated()), id: \.element.id) { index, card in
          cardRow(card, index: index)
        }
        .onMove { source, destination in
          // The offsets are into the rows drawn, leaving ones included.
          today.move(from: source, to: destination, drawn: cards, model: model)
        }
      } header: {
        if !today.day.cards.isEmpty {
          // The Mac's day: the plan, else the head of the ranking.
          Group {
            if today.day.isPlanned {
              SectionHeading("Today", count: today.day.cards.count)
            } else {
              SectionHeading("Up next · \(today.day.cards.count) ranked")
            }
          }
          .listRowInsets(EdgeInsets())
        }
      }
      if !dailies.isEmpty {
        Section {
          ForEach(Array(dailies.enumerated()), id: \.element.id) { index, daily in
            DailyRow(
              daily: daily, isLeaving: leavingDailies[daily.id] != nil,
              onToggle: { finish(daily, index: index) },
              onOpen: { model.navigation.inspect(daily.taskID, isPad: isPad) })
              .listRowInsets(EdgeInsets(top: 0, leading: theme.mac(4), bottom: 0, trailing: theme.mac(4)))
              .listRowBackground(theme.paper)
              .listRowSeparator(.hidden)
              .moveDisabled(true)
              .selectionDisabled()
          }
        } header: {
          SectionHeading("Dailies", count: dailies.count).listRowInsets(EdgeInsets())
        }
      }
    }
    .listStyle(.plain)
    .scrollContentBackground(.hidden)
    .environment(\.defaultMinListRowHeight, theme.touchTarget)
    .background(theme.paper)
    .overlay {
      if today.isLoaded && today.day.cards.isEmpty && today.day.dailies.isEmpty {
        EmptyState(
          title: "Nothing planned for today",
          message: "Add one with +, or plan a task from its menu or with a swipe left in a list.",
          systemImage: "sun.max")
      }
    }
    .overlay(alignment: .bottomTrailing) {
      if !isPad { AddButton(plansToday: true) }
    }
    .navigationTitle("Today")
    .navigationBarTitleDisplayMode(.inline)
    .toolbar { WorkspaceToolbar() }
    .task(id: model.revision) { await today.load(model) }
    .task(id: model.revision) { await conditions.load(model) }
    .task(id: today.day.session?.id) { await refreshPeriodically() }
    // The day is a function of the clock too: back from the background, past
    // midnight or across a time-zone change, it is read again even though
    // nothing was written.
    .onChange(of: scenePhase) { _, phase in
      if phase == .active { Task { await today.load(model) } }
    }
    .onReceive(NotificationCenter.default.publisher(for: UIApplication.significantTimeChangeNotification)) { _ in
      Task { await today.load(model) }
    }
    .accessibilityIdentifier("today.list")
  }

  private var cards: [DayCard] { Leaving.merging(today.day.cards, leavingCards.values) }
  private var dailies: [DayDaily] { Leaving.merging(today.day.dailies, leavingDailies.values) }

  /// A tap on a row: crossed out where it stands, then gone once the day
  /// has dropped it. The write lands at once, as Android's does.
  private func finish(_ card: DayCard, index: Int) {
    // Already crossed out: a second tap or swipe would log it twice.
    guard leavingCards[card.id] == nil else { return }
    if !card.isRunning && !card.isDailyDoneToday {
      leavingCards[card.id] = Leaving(item: card, index: index)
      Task { try? await Task.sleep(for: Leaving<DayCard>.hold); leavingCards[card.id] = nil }
    }
    today.tickOff(card, model: model)
  }

  private func finish(_ daily: DayDaily, index: Int) {
    guard leavingDailies[daily.id] == nil else { return }
    leavingDailies[daily.id] = Leaving(item: daily, index: index)
    Task { try? await Task.sleep(for: Leaving<DayDaily>.hold); leavingDailies[daily.id] = nil }
    today.toggleDaily(daily, model: model)
  }

  @ViewBuilder
  private func cardRow(_ card: DayCard, index: Int) -> some View {
    let isSelected = model.navigation.selectedTaskID == card.id
    Group {
      if card.isRunning, let session = today.day.session {
        RunningDayCard(
          card: card, index: index + 1, session: session, canSkip: today.day.hasQueuedSuccessor,
          onPause: { today.togglePause(model: model) },
          onSkip: { today.requestCompletion(completeTask: false, model: model) },
          onLog: { today.requestCompletion(completeTask: false, model: model) },
          onDone: { today.requestCompletion(completeTask: true, model: model) },
          onStop: { today.endSession(model: model) })
      } else {
        DayCardRow(
          card: card, isSelected: isSelected, isBlocked: model.isBlocked(card.id),
          isLeaving: leavingCards[card.id] != nil, waiting: model.waitingBadge(card.id),
          isSessionRunning: today.day.session?.phase == .running,
          onTick: { finish(card, index: index) },
          onOpen: { model.navigation.inspect(card.id, isPad: isPad) },
          onStart: { today.start(card, model: model) },
          menu: { cardMenu(card) })
          .equatable()
      }
    }
    .tag(card.id)
    .listRowInsets(EdgeInsets(
      top: 0, leading: card.isRunning ? theme.mac(12) : theme.mac(4), bottom: 0,
      trailing: card.isRunning ? theme.mac(12) : theme.mac(4)))
    .listRowBackground(card.isRunning ? theme.selected : theme.paper)
    .listRowSeparator(.hidden)
    .moveDisabled(!card.isPlanned)
    .swipeActions(edge: .leading, allowsFullSwipe: true) {
      if leavingCards[card.id] == nil {
        Button { finish(card, index: index) } label: {
          Label(card.dailyID != nil ? "Log today" : "Done", systemImage: "checkmark")
        }
        .tint(theme.success)
      }
    }
    .swipeActions(edge: .trailing, allowsFullSwipe: false) {
      Button { today.deferToTomorrow(card, model: model) } label: { Label("Tomorrow", systemImage: "sunrise") }
        .tint(theme.warning)
      if card.isPlanned {
        Button { model.togglePlannedToday(card.id) } label: { Label("Take off", systemImage: "sun.max") }
          .tint(theme.muted)
      }
    }
    .contextMenu { cardMenu(card) }
    .accessibilityIdentifier("today.card.\(card.title)")
  }

  /// The options button's menu, and a long press's.
  @ViewBuilder
  private func cardMenu(_ card: DayCard) -> some View {
    Section {
      Button { model.navigation.inspect(card.id, isPad: isPad) } label: {
        Label("Details", systemImage: "info.circle")
      }
    }
    if card.isPlanned {
      Section {
        Button { today.movePlanned(card.id, by: -1, model: model) } label: {
          Label("Move up in the day", systemImage: "arrow.up")
        }
        Button { today.movePlanned(card.id, by: 1, model: model) } label: {
          Label("Move down in the day", systemImage: "arrow.down")
        }
      }
    }
    Section {
      Button { today.start(card, model: model) } label: {
        Label(card.isRunning ? "Done" : (today.day.session?.phase == .running ? "Queue next" : "Start"),
          systemImage: card.isRunning ? "checkmark" : "play")
      }
      Button { today.deferToTomorrow(card, model: model) } label: { Label("Not today", systemImage: "sunrise") }
    }
    TaskContextMenu(context: card.menuContext)
  }

  /// Re-reads the day every minute, as Android's `minuteTicks` does, so a
  /// start or a due time that arrives with the clock shows without a write.
  /// While a block runs, every half minute: a write elsewhere (the Live
  /// Activity, an intent) can end it, and the day should not show a stale
  /// clock. Cheap: one read, off the main actor.
  private func refreshPeriodically() async {
    let interval: Duration = today.day.session != nil ? .seconds(30) : .seconds(60)
    while !Task.isCancelled {
      try? await Task.sleep(for: interval)
      guard !Task.isCancelled else { return }
      await today.load(model)
    }
  }
}

// MARK: - Header

/// What the day costs against what it has cost, and when it ends: the Mac
/// pane's tally, with the panel's bar and week line under it.
struct DayHeader: View {
  @Environment(\.theme) private var theme
  let day: DaySnapshot

  var body: some View {
    let isTicking = day.session?.phase == .running && day.session?.pausedAt == nil
    TimelineView(.periodic(from: .now, by: isTicking ? 30 : 60)) { context in
      let forecast = day.forecast(now: context.date)
      VStack(alignment: .leading, spacing: theme.space.sm) {
        HStack(alignment: .firstTextBaseline, spacing: theme.space.sm) {
          Text(Self.spent(forecast))
            .font(theme.type.numeralBody)
            .foregroundStyle(theme.ink)
            .accessibilityIdentifier("today.tally")
          Spacer(minLength: theme.space.sm)
          if let finish = forecast.finishAt {
            Text("done by \(finish.formatted(date: .omitted, time: .shortened))")
              .font(theme.type.secondaryMono)
              .foregroundStyle(theme.primary)
          }
        }
        bar(forecast)
        HStack(spacing: theme.space.sm) {
          Text(Self.remaining(forecast))
          Spacer(minLength: theme.space.sm)
          Text(day.loggedToday > 0 ? "\(Format.duration(day.loggedToday)) logged today" : "Nothing logged yet")
        }
        .font(theme.type.secondary)
        .foregroundStyle(theme.muted)
        if day.workProgress.week.seconds > 0 || day.workProgress.week.completed > 0 {
          HStack(spacing: theme.space.sm) {
            Text(day.workProgress.today.completed == 1 ? "1 done today" : "\(day.workProgress.today.completed) done today")
            Spacer(minLength: theme.space.sm)
            Text("\(Format.duration(day.workProgress.week.seconds)) this week · \(Format.duration(day.workProgress.averageSecondsPerDay))/day")
          }
          .font(theme.type.secondary)
          .foregroundStyle(theme.dim)
        }
      }
      .padding(.horizontal, theme.mac(12))
      .padding(.vertical, theme.mac(12))
    }
    .overlay(alignment: .bottom) { Hairline() }
  }

  /// Square-ended: a bar is a length, not decoration.
  private func bar(_ forecast: DayForecast) -> some View {
    let fraction = forecast.estimatedSeconds > 0
      ? min(1, Double(forecast.loggedSeconds) / Double(forecast.estimatedSeconds)) : 0
    return GeometryReader { proxy in
      ZStack(alignment: .leading) {
        Rectangle().fill(theme.well)
        Rectangle().fill(theme.primary).frame(width: proxy.size.width * fraction)
      }
    }
    .frame(height: 4)
    .accessibilityHidden(true)
  }

  static func spent(_ forecast: DayForecast) -> String {
    let logged = DayForecastText.hoursAndMinutes(forecast.loggedSeconds)
    guard forecast.estimatedSeconds > 0 else { return "\(logged) logged" }
    return "\(logged) of \(DayForecastText.hoursAndMinutes(forecast.estimatedSeconds))"
  }

  static func remaining(_ forecast: DayForecast) -> String {
    guard forecast.estimatedSeconds > 0 else { return "No estimates yet" }
    var text = forecast.finishAt == nil
      ? "Every estimate used up" : "\(DayForecastText.hoursAndMinutes(forecast.remainingSeconds)) left"
    if forecast.unestimatedCount > 0 { text += " · \(forecast.unestimatedCount) unestimated" }
    return text
  }
}

/// The Mac's `DayForecast.hoursAndMinutes`, which lives in the app target.
enum DayForecastText {
  static func hoursAndMinutes(_ seconds: Int) -> String {
    let minutes = max(0, seconds) / 60
    if minutes < 60 { return "\(minutes)m" }
    let remainder = minutes % 60
    return remainder == 0 ? "\(minutes / 60)h" : "\(minutes / 60)h \(remainder)m"
  }
}

// MARK: - Cards

/// A card that is not running. No check circle: tapping the row finishes it,
/// crossed out and then gone; play on the far right starts a block on it, the
/// options sit just before play, and a planned card has a drag handle on its
/// leading edge. Android's `DayCardRow`, for the iPhone.
struct DayCardRow<Menu: View>: View, Equatable {
  @Environment(\.theme) private var theme
  let card: DayCard
  let isSelected: Bool
  /// Waiting on an open task: drawn greyed.
  var isBlocked = false
  /// Just finished, and about to leave the day.
  var isLeaving = false
  /// Filed in Waiting on: who it waits on and when it is chased.
  var waiting: WaitingBadge?
  /// A block is running, so the menu's play queues this card next rather
  /// than starting it. Part of equality, or the menu keeps its old label.
  var isSessionRunning = false
  let onTick: () -> Void
  let onOpen: () -> Void
  let onStart: () -> Void
  @ViewBuilder let menu: () -> Menu

  static func == (lhs: DayCardRow, rhs: DayCardRow) -> Bool {
    lhs.card == rhs.card && lhs.isSelected == rhs.isSelected && lhs.isBlocked == rhs.isBlocked
      && lhs.isLeaving == rhs.isLeaving && lhs.waiting == rhs.waiting
      && lhs.isSessionRunning == rhs.isSessionRunning
  }

  /// What VoiceOver reads for the row: everything its lines show, not just
  /// the title.
  var accessibilityText: String {
    var parts = [card.title]
    if let reason = card.reason, reason != .planned {
      parts.append(card.detail ?? reason.label)
    } else if let detail = card.detail {
      parts.append(detail)
    }
    if card.dailyID != nil { parts.append("Repeats daily") }
    if let list = card.listName { parts.append(list) }
    if let waiting {
      if let who = waiting.waitingOn { parts.append("Waiting on \(who)") }
      if let followUp = waiting.followUp { parts.append("Follow up \(followUp)") }
    }
    if isBlocked { parts.append("Blocked") }
    return parts.joined(separator: ", ")
  }

  /// The Mac's day row (`views::day_row`): the status mark outside a box
  /// that starts at the title — title, chips, then "list · reason" in dim
  /// secondary text — 30 Mac pixels high. A tap on the box opens the
  /// task's inspector, and a swipe right (or the options' Done) finishes it;
  /// the options and play sit after it as icons at rest.
  var body: some View {
    HStack(spacing: 0) {
      Color.clear.frame(width: theme.mac(2 + 10 + 4), height: 1)
      TaskCheckbox(status: isLeaving ? .completed : .open, isList: card.isList, showsBox: false)
      Button(action: onOpen) {
        HStack(spacing: theme.mac(8)) {
          Text(card.title)
            .font(theme.type.body)
            .foregroundStyle(isLeaving || isBlocked ? theme.dim : theme.ink)
            .strikethrough(isLeaving, color: theme.dim)
            .lineLimit(1)
            .celebrationStrike(card.id)
            .frame(maxWidth: .infinity, alignment: .leading)
          TaskChips(isBlocked: isBlocked, waitingOn: waiting?.waitingOn)
          if card.dailyID != nil { Chip(text: "↻", tint: theme.dim) }
          cost
          Text(context)
            .font(theme.type.secondary)
            .foregroundStyle(reasonTint)
            .lineLimit(1)
        }
        .padding(.leading, theme.mac(6))
        .padding(.trailing, theme.mac(8))
        .frame(height: theme.mac(30))
        .rowSelection(isSelected)
        .frame(minHeight: theme.touchTarget)
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .disabled(isLeaving)
      .accessibilityLabel(accessibilityText)
      .accessibilityHint("Opens its details")
      .accessibilityAction(named: card.dailyID != nil ? "Log today" : "Complete", onTick)
      .accessibilityIdentifier("today.check.\(card.title)")
      SwiftUI.Menu(content: menu) {
        Image(systemName: "ellipsis")
          .font(theme.type.glyph(13))
          .foregroundStyle(theme.icon)
          .frame(width: 32, height: theme.touchTarget)
          .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .accessibilityLabel("Options for \(card.title)")
      .accessibilityIdentifier("today.options.\(card.title)")
      Button(action: onStart) {
        Image(systemName: "play.fill")
          .font(theme.type.glyph(11.5))
          .foregroundStyle(theme.icon)
          .frame(width: 32, height: theme.touchTarget)
          .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .accessibilityLabel("Start \(card.title)")
      .accessibilityIdentifier("today.start.\(card.title)")
    }
    .celebrationRow(card.id)
  }

  /// "list · reason", as the Mac's day row ends.
  private var context: String {
    let reason: String =
      switch card.reason {
      case nil: card.detail ?? "ranked"
      case .planned: card.detail ?? "planned"
      case let reason?: card.detail ?? reason.label
      }
    return [card.listName, reason].compactMap { $0 }.joined(separator: " · ")
  }

  private var reasonTint: Color {
    switch card.reason {
    case .overdue: theme.danger
    default: theme.dim
    }
  }

  @ViewBuilder
  private var cost: some View {
    if let estimate = card.estimateSeconds, estimate > 0 {
      Text(card.loggedSeconds > 0
        ? "\(Format.duration(card.loggedSeconds))/\(Format.duration(estimate))" : Format.duration(estimate))
        .font(theme.type.secondaryMono)
        .foregroundStyle(card.loggedSeconds > estimate ? theme.warning : theme.dim)
        .fixedSize()
    } else if card.loggedSeconds > 0 {
      Text(Format.duration(card.loggedSeconds)).font(theme.type.secondaryMono).foregroundStyle(theme.dim).fixedSize()
    }
  }
}

/// The card you are on: the same card, grown — a live clock, and the
/// controls that only apply to the task actually running.
struct RunningDayCard: View {
  @Environment(\.theme) private var theme
  let card: DayCard
  let index: Int
  let session: FocusSession
  let canSkip: Bool
  let onPause: () -> Void
  let onSkip: () -> Void
  let onLog: () -> Void
  let onDone: () -> Void
  let onStop: () -> Void

  private var isPaused: Bool { session.pausedAt != nil }

  var body: some View {
    VStack(alignment: .leading, spacing: theme.space.sm) {
      HStack(spacing: theme.mac(8)) {
        Text("\(index)").font(theme.type.secondaryMono).foregroundStyle(theme.primary)
        Text(card.title).font(theme.type.bodyMedium).foregroundStyle(theme.ink).lineLimit(2)
        Spacer(minLength: theme.space.sm)
        if let list = card.listName {
          Text(list).font(theme.type.secondary).foregroundStyle(theme.dim).lineLimit(1)
        }
      }
      HStack(alignment: .firstTextBaseline, spacing: theme.space.sm) {
        TimelineView(.periodic(from: .now, by: 1)) { context in
          let elapsed = session.elapsedSeconds(now: context.date)
          // A timer counts up from nothing; only a block of a length counts
          // down to its end.
          let reading = session.openEnded
            ? FocusTimerDisplay.Reading(text: Format.clock(elapsed), isOverrun: false)
            : FocusTimerDisplay.reading(
              elapsed: TimeInterval(elapsed), planned: TimeInterval(session.workDurationSeconds))
          Text(reading.text)
            .font(theme.type.display)
            .monospacedDigit()
            .foregroundStyle(isPaused ? theme.muted : (reading.isOverrun ? theme.warning : theme.primary))
            .contentTransition(.numericText())
            .accessibilityIdentifier("today.clock")
        }
        Text(isPaused ? "paused" : (session.openEnded ? "so far" : "of \(Format.duration(session.workDurationSeconds))"))
          .font(theme.type.secondary)
          .foregroundStyle(isPaused ? theme.warning : theme.muted)
        Spacer(minLength: 0)
      }
      HStack(spacing: theme.space.sm) {
        control(isPaused ? "play.fill" : "pause.fill", isPaused ? "Resume" : "Pause", action: onPause)
        control("forward.end.fill", "Skip to the next queued task", action: onSkip)
          .disabled(!canSkip)
          .opacity(canSkip ? 1 : 0.4)
        control("clock.arrow.circlepath", "Log the time and keep the task open", action: onLog)
        control("stop.fill", "End the session", action: onStop)
          .accessibilityIdentifier("today.endSession")
        Spacer(minLength: 0)
        Button(action: onDone) {
          Label("Done", systemImage: "checkmark")
        }
        .buttonStyle(ThemedButtonStyle(kind: .primary, compact: true))
        .accessibilityIdentifier("today.done")
      }
    }
    .padding(.vertical, theme.space.md)
  }

  private func control(_ symbol: String, _ label: String, action: @escaping () -> Void) -> some View {
    Button(action: action) {
      Image(systemName: symbol)
        .font(theme.type.glyph(11.5))
        .foregroundStyle(theme.ink)
        .frame(width: theme.mac(34), height: theme.mac.control)
        .background(RoundedRectangle(cornerRadius: theme.mac.radiusMenu).fill(theme.altRow))
        .overlay(RoundedRectangle(cornerRadius: theme.mac.radiusMenu).strokeBorder(theme.border, lineWidth: 1))
        .hitTarget()
    }
    .buttonStyle(.plain)
    .accessibilityLabel(label)
  }
}

/// A daily still owed today. Tapping it logs today's contribution, crossed
/// out and then gone; the options open it.
struct DailyRow: View {
  @Environment(\.theme) private var theme
  let daily: DayDaily
  var isLeaving = false
  let onToggle: () -> Void
  let onOpen: () -> Void

  var body: some View {
    HStack(spacing: 0) {
      Color.clear.frame(width: theme.mac(2 + 10 + 4) + theme.mac.mark, height: 1)
      Button(action: onToggle) {
        HStack(spacing: theme.mac(8)) {
          Text(daily.title)
            .font(theme.type.body)
            .foregroundStyle(isLeaving ? theme.dim : theme.ink)
            .strikethrough(isLeaving, color: theme.dim)
            .lineLimit(1)
            .frame(maxWidth: .infinity, alignment: .leading)
          Chip(text: "↻", tint: theme.dim)
          if daily.secondsToday > 0 || daily.targetSeconds != nil {
            Chip(text: progress, tint: theme.dim, mono: true)
          }
        }
        .padding(.leading, theme.mac(6))
        .padding(.trailing, theme.mac(8))
        .frame(minHeight: theme.touchTarget)
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .disabled(isLeaving)
      .accessibilityLabel("Log \(daily.title) for today")
      .accessibilityIdentifier("today.daily.\(daily.title)")
      Button(action: onOpen) {
        Image(systemName: "ellipsis")
          .font(theme.type.glyph(13))
          .foregroundStyle(theme.icon)
          .frame(width: 32, height: theme.touchTarget)
          .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .accessibilityLabel("Options for \(daily.title)")
    }
  }

  private var progress: String {
    let logged = Format.duration(daily.secondsToday)
    guard let target = daily.targetSeconds else { return logged }
    return "\(daily.secondsToday > 0 ? logged : "0m")/\(Format.duration(target))"
  }
}

/// A row just finished, and where it stood, so the list can keep drawing it
/// crossed out for a moment after the day has dropped it.
struct Leaving<Item: Identifiable> {
  /// How long a finished row stays, crossed out — Android's `LEAVE_MILLIS`.
  static var hold: Duration { .milliseconds(900) }

  let item: Item
  let index: Int

  /// `items` with the rows still leaving put back where they stood.
  static func merging(_ items: [Item], _ leaving: some Collection<Leaving>) -> [Item] {
    guard !leaving.isEmpty else { return items }
    let present = Set(items.map(\.id))
    var rows = items
    for row in leaving.sorted(by: { $0.index < $1.index }) where !present.contains(row.item.id) {
      rows.insert(row.item, at: min(row.index, rows.count))
    }
    return rows
  }
}
