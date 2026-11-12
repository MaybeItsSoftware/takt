import SwiftUI

/// The day's open tasks, in the order the iPhone's Today shows them. Tapping
/// a row ticks it off; the play button starts a block on it.
struct TodayView: View {
  @Environment(WatchStore.self) private var store
  var onStart: () -> Void = {}

  var body: some View {
    NavigationStack {
      List {
        if !store.hasSnapshot {
          EmptyDay(
            symbol: "iphone", title: "Open Takt on your iPhone",
            message: "The day appears here once the phone has sent it.")
        } else if store.openTasks.isEmpty {
          EmptyDay(symbol: "checkmark", title: "Nothing left today", message: nil)
        } else {
          ForEach(store.openTasks) { task in
            TaskRow(
              task: task, session: store.session?.taskId == task.id ? store.session : nil,
              isFinishing: store.finishing.contains(task.id),
              onTick: { store.tickOff(task) },
              onStart: {
                if store.session?.taskId == task.id {
                  onStart()
                } else {
                  store.start(task)
                  onStart()
                }
              })
          }
        }
        StatusFooter()
      }
      .navigationTitle("Today")
      .toolbar {
        ToolbarItem(placement: .topBarTrailing) {
          if store.hasSnapshot, !store.openTasks.isEmpty {
            Text("\(store.openTasks.count)")
              .font(.system(.footnote, design: .monospaced).weight(.bold))
              .foregroundStyle(WatchPalette.muted)
              .padding(.horizontal, 6)
              .padding(.vertical, 2)
              .overlay(RoundedRectangle(cornerRadius: 4).stroke(WatchPalette.border, lineWidth: 1))
              .accessibilityLabel("\(store.openTasks.count) left today")
          }
        }
      }
    }
  }
}

/// One task: tap to tick it off, play to start a block on it. The running
/// task carries its clock instead of an estimate.
private struct TaskRow: View {
  let task: WatchSnapshot.Task
  let session: WatchSnapshot.Session?
  let isFinishing: Bool
  let onTick: () -> Void
  let onStart: () -> Void

  var body: some View {
    HStack(spacing: 8) {
      Button(action: onTick) {
        // No check circle, as on the phone's Today and the Wear watch: a tap
        // crosses the title out and the row leaves once the tick is sent.
        HStack(alignment: .top, spacing: 8) {
          VStack(alignment: .leading, spacing: 2) {
            Text(task.title)
              .font(.system(.body).weight(.semibold))
              .foregroundStyle(isFinishing ? WatchPalette.muted : WatchPalette.text)
              .strikethrough(isFinishing, color: WatchPalette.emerald)
              .lineLimit(3)
            if let session {
              RunningClock(session: session)
                .font(.system(.footnote, design: .monospaced).weight(.medium))
                .foregroundStyle(session.isPaused ? WatchPalette.amber : WatchPalette.azure)
            } else if let detail = WatchFormat.detail(task) {
              Text(detail)
                .font(.footnote)
                .foregroundStyle(WatchPalette.muted)
                .lineLimit(1)
            }
          }
          Spacer(minLength: 0)
        }
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .accessibilityLabel(task.isDaily ? "Log \(task.title) for today" : "Complete \(task.title)")

      Button(action: onStart) {
        Image(systemName: session == nil ? "play.fill" : "timer")
          .font(.system(size: 16, weight: .bold))
          .foregroundStyle(session == nil ? WatchPalette.azure : WatchPalette.text)
          .frame(width: 40, height: 40)
          .background(
            RoundedRectangle(cornerRadius: 4)
              .fill(session == nil ? WatchPalette.azure.opacity(0.12) : WatchPalette.azure))
          .overlay(RoundedRectangle(cornerRadius: 4).stroke(WatchPalette.azure.opacity(0.4), lineWidth: 1))
          .frame(width: 44, height: 44)
          .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .disabled(isFinishing)
      .accessibilityLabel(session == nil ? "Focus on \(task.title)" : "Show the running block")
    }
    .padding(.vertical, 6)
    .padding(.horizontal, 8)
    .listRowBackground(
      RoundedRectangle(cornerRadius: 6)
        .fill(WatchPalette.raised)
        .overlay(
          RoundedRectangle(cornerRadius: 6)
            .stroke(session == nil ? WatchPalette.border : WatchPalette.azure.opacity(0.6), lineWidth: 1)))
    .opacity(isFinishing ? 0.6 : 1)
    .animation(.easeOut(duration: 0.2), value: isFinishing)
  }
}

/// The block's time, counted on the watch from the session's own numbers.
struct RunningClock: View {
  let session: WatchSnapshot.Session

  var body: some View {
    if session.isPaused {
      Text("Paused · " + WatchFormat.clock(session.accumulatedSeconds))
    } else {
      Text(timerInterval: session.timerStart...Date.distantFuture, countsDown: false)
    }
  }
}

private struct EmptyDay: View {
  let symbol: String
  let title: String
  let message: String?

  var body: some View {
    VStack(spacing: 6) {
      Image(systemName: symbol).font(.title2).foregroundStyle(WatchPalette.emerald)
      Text(title).font(.headline).foregroundStyle(WatchPalette.text).multilineTextAlignment(.center)
      if let message {
        Text(message).font(.footnote).foregroundStyle(WatchPalette.muted).multilineTextAlignment(.center)
      }
    }
    .frame(maxWidth: .infinity)
    .padding(.vertical, 12)
    .listRowBackground(Color.clear)
  }
}

/// How fresh the day is: its age once it is worth saying, and the taps still
/// waiting for the phone.
private struct StatusFooter: View {
  @Environment(WatchStore.self) private var store

  var body: some View {
    TimelineView(.everyMinute) { context in
      let age = WatchFormat.age(of: store.snapshot, now: context.date)
      let queued = store.queuedCount
      if age != nil || queued > 0 || (store.hasSnapshot && !store.isReachable) {
        VStack(alignment: .leading, spacing: 2) {
          if let age { Text(age) }
          if queued > 0 {
            Text(queued == 1 ? "1 change waiting for your iPhone" : "\(queued) changes waiting for your iPhone")
              .foregroundStyle(WatchPalette.amber)
          } else if !store.isReachable, store.hasSnapshot {
            Text("iPhone not reachable")
          }
        }
        .font(.footnote)
        .foregroundStyle(WatchPalette.dim)
        .frame(maxWidth: .infinity, alignment: .leading)
        .listRowBackground(Color.clear)
      }
    }
  }
}
