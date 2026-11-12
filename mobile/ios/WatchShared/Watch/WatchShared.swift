import Foundation
import SwiftUI

// What the watch app and its complications share: where the last snapshot
// is kept, the palette and how numbers are written. Not compiled into the
// iPhone, which has its own theme.

/// The container the watch app and its complications share. The watch app
/// writes the last snapshot the phone sent; the complications only read it.
enum WatchStorage {
  static let appGroup = "group.uk.co.maybeitssoftware.takt"
  /// The complication's WidgetKit kind.
  static let widgetKind = "TaktWatchNow"

  static var containerURL: URL {
    FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroup)
      ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
  }

  static var snapshotURL: URL {
    containerURL.appending(path: "Takt/watch-snapshot.json", directoryHint: .notDirectory)
  }

  static func load(from url: URL = snapshotURL) -> WatchSnapshot? {
    guard let data = try? Data(contentsOf: url) else { return nil }
    return try? WatchSnapshot.decode(data)
  }

  static func save(_ snapshot: WatchSnapshot, to url: URL = snapshotURL) throws {
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try snapshot.encoded().write(to: url, options: .atomic)
  }
}

/// Zed One Dark on the watch, the Mac's and the phone's default theme
/// (`one_palette()` in core/src/theme/builtins.rs): its surfaces, its accent
/// (still called `azure`) for what you can act on, and its status hues for
/// what happened. The watch is always dark, so there is one set rather than
/// a pair.
enum WatchPalette {
  static let page = Color(hex: 0x282C33)
  static let raised = Color(hex: 0x2F343E)
  static let subtle = Color(hex: 0x363C46)
  static let border = Color(hex: 0x464B57)
  static let text = Color(hex: 0xDCE0E5)
  static let muted = Color(hex: 0xA9AFBC)
  static let dim = Color(hex: 0x878A98)
  static let azure = Color(hex: 0x74ADE8)
  static let emerald = Color(hex: 0xA1C181)
  static let raspberry = Color(hex: 0xD07277)
  static let amber = Color(hex: 0xDEC184)
}

extension Color {
  init(hex: UInt32) {
    self.init(
      red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255)
  }
}

/// How the watch writes numbers, matching the iPhone's `Format`.
enum WatchFormat {
  /// `25m`, `1h`, `1h 30m`.
  static func duration(_ seconds: Int) -> String {
    let minutes = max(0, Int((Double(seconds) / 60).rounded()))
    if minutes < 60 { return "\(minutes)m" }
    let hours = minutes / 60
    let rest = minutes % 60
    return rest == 0 ? "\(hours)h" : "\(hours)h \(rest)m"
  }

  /// `12:04` or `1:02:04`.
  static func clock(_ seconds: Int) -> String {
    let seconds = max(0, seconds)
    let hours = seconds / 3600
    let minutes = (seconds % 3600) / 60
    let rest = seconds % 60
    if hours > 0 { return String(format: "%d:%02d:%02d", hours, minutes, rest) }
    return String(format: "%d:%02d", minutes, rest)
  }

  /// A row's small line: its reason or list, then its estimate.
  static func detail(_ task: WatchSnapshot.Task) -> String? {
    let parts = [task.detail, task.estimateSeconds.map(duration)].compactMap { $0 }.filter { !$0.isEmpty }
    return parts.isEmpty ? nil : parts.joined(separator: " · ")
  }

  /// How old the snapshot is, when that is worth saying: "Updated 12m ago".
  static func age(of snapshot: WatchSnapshot, now: Date) -> String? {
    let seconds = now.timeIntervalSince(snapshot.generatedAt)
    guard snapshot.generatedAt != .distantPast, seconds >= 120 else { return nil }
    if seconds < 3_600 { return "Updated \(Int(seconds / 60))m ago" }
    if seconds < 86_400 { return "Updated \(Int(seconds / 3_600))h ago" }
    return "Updated " + snapshot.generatedAt.formatted(.dateTime.weekday(.abbreviated).hour().minute())
  }
}
