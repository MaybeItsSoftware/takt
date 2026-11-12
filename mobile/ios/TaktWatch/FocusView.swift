import SwiftUI

/// The running block: what it is on, how long it has run and how long is
/// left, and the three ways out of it. The time is drawn here from the
/// session's start and plan; the phone sends nothing per second.
struct FocusView: View {
  @Environment(WatchStore.self) private var store

  var body: some View {
    if let session = store.session {
      ScrollView {
        VStack(alignment: .leading, spacing: 8) {
          Text(session.isPaused ? "PAUSED" : "FOCUS")
            .font(.system(size: 11, weight: .bold, design: .monospaced))
            .tracking(1.5)
            .foregroundStyle(session.isPaused ? WatchPalette.amber : WatchPalette.azure)
          Text(session.title)
            .font(.system(.title3).weight(.semibold))
            .foregroundStyle(WatchPalette.text)
            .lineLimit(3)
            .minimumScaleFactor(0.8)
          FocusClock(session: session)
          HStack(spacing: 6) {
            ControlButton(
              symbol: session.isPaused ? "play.fill" : "pause.fill", label: session.isPaused ? "Resume" : "Pause",
              hue: WatchPalette.amber, action: { store.togglePause() })
            ControlButton(symbol: "checkmark", label: "Done", hue: WatchPalette.emerald, action: { store.done() })
            ControlButton(symbol: "stop.fill", label: "Stop", hue: WatchPalette.raspberry, action: { store.stop() })
          }
          .padding(.top, 4)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 4)
      }
      .background(WatchPalette.page)
    }
  }
}

/// Remaining time large, elapsed beneath it, and a bar of the plan used.
/// Past the plan the large figure counts the overrun.
private struct FocusClock: View {
  let session: WatchSnapshot.Session

  var body: some View {
    TimelineView(.periodic(from: .now, by: 1)) { context in
      let elapsed = session.elapsedSeconds(at: context.date)
      let planned = session.plannedSeconds
      let over = planned.map { elapsed > $0 } ?? false
      VStack(alignment: .leading, spacing: 4) {
        Group {
          if let planned, !over {
            Text(WatchFormat.clock(planned - elapsed))
          } else if let planned {
            Text("+" + WatchFormat.clock(elapsed - planned))
          } else {
            Text(WatchFormat.clock(elapsed))
          }
        }
        .font(.system(size: 40, weight: .semibold, design: .monospaced))
        .monospacedDigit()
        .minimumScaleFactor(0.6)
        .lineLimit(1)
        .foregroundStyle(over ? WatchPalette.amber : WatchPalette.text)
        .accessibilityLabel(over ? "Over by \(WatchFormat.duration(elapsed - (planned ?? 0)))" : "Time left")

        if let planned, planned > 0 {
          GeometryReader { proxy in
            ZStack(alignment: .leading) {
              RoundedRectangle(cornerRadius: 2).fill(WatchPalette.subtle)
              RoundedRectangle(cornerRadius: 2)
                .fill(over ? WatchPalette.amber : WatchPalette.azure)
                .frame(width: proxy.size.width * min(1, Double(elapsed) / Double(planned)))
            }
          }
          .frame(height: 4)
          Text("\(WatchFormat.clock(elapsed)) of \(WatchFormat.duration(planned))")
            .font(.system(.footnote, design: .monospaced))
            .foregroundStyle(WatchPalette.muted)
        }
      }
    }
  }
}

private struct ControlButton: View {
  let symbol: String
  let label: String
  let hue: Color
  let action: () -> Void

  var body: some View {
    Button(action: action) {
      VStack(spacing: 2) {
        Image(systemName: symbol).font(.system(size: 16, weight: .bold))
        Text(label).font(.system(size: 11, weight: .semibold))
      }
      .foregroundStyle(hue)
      .frame(maxWidth: .infinity, minHeight: 48)
      .background(RoundedRectangle(cornerRadius: 4).fill(hue.opacity(0.12)))
      .overlay(RoundedRectangle(cornerRadius: 4).stroke(hue.opacity(0.4), lineWidth: 1))
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .accessibilityLabel(label)
  }
}
