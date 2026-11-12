import Foundation

/// How numbers and dates are written across the app, in one place so the
/// outline, the day and the inspector agree.
enum Format {
  /// `25m`, `1h`, `1h 30m`.
  static func duration(_ seconds: Int) -> String {
    let minutes = max(0, Int((Double(seconds) / 60).rounded()))
    if minutes < 60 { return "\(minutes)m" }
    let hours = minutes / 60
    let rest = minutes % 60
    return rest == 0 ? "\(hours)h" : "\(hours)h \(rest)m"
  }

  /// `12:04` or `1:02:04`, for a running clock.
  static func clock(_ seconds: Int) -> String {
    let seconds = max(0, seconds)
    let hours = seconds / 3600
    let minutes = (seconds % 3600) / 60
    let rest = seconds % 60
    if hours > 0 { return String(format: "%d:%02d:%02d", hours, minutes, rest) }
    return String(format: "%d:%02d", minutes, rest)
  }

  /// A due date, relative where that reads better: Today, Tomorrow,
  /// Yesterday, a weekday within the week, otherwise `3 Oct`.
  static func due(_ date: Date, now: Date = .now, calendar: Calendar = .current) -> String {
    let start = calendar.startOfDay(for: now)
    let day = calendar.startOfDay(for: date)
    let days = calendar.dateComponents([.day], from: start, to: day).day ?? 0
    let hasTime = !calendar.isDate(date, equalTo: day, toGranularity: .minute)
    let time = hasTime ? " " + date.formatted(date: .omitted, time: .shortened) : ""
    switch days {
    case 0: return "Today" + time
    case 1: return "Tomorrow" + time
    case -1: return "Yesterday" + time
    case 2..<7: return date.formatted(.dateTime.weekday(.abbreviated)) + time
    default:
      let sameYear = calendar.component(.year, from: date) == calendar.component(.year, from: now)
      let base = sameYear
        ? date.formatted(.dateTime.day().month(.abbreviated))
        : date.formatted(.dateTime.day().month(.abbreviated).year())
      return base + time
    }
  }

  /// What a buried task's row says: "starts 14:00" today, "starts tomorrow",
  /// "starts Mon" within the week, "starts 3 Nov" beyond it. The Mac's
  /// wording (`WorkspaceTaskPlanningBadges.startsText`).
  static func starts(_ start: Date, now: Date = .now, calendar: Calendar = .current) -> String {
    if calendar.isDate(start, inSameDayAs: now) {
      return "starts \(start.formatted(date: .omitted, time: .shortened))"
    }
    let days = calendar.dateComponents(
      [.day], from: calendar.startOfDay(for: now), to: calendar.startOfDay(for: start)
    ).day ?? 0
    if days == 1 { return "starts tomorrow" }
    if days < 7 { return "starts \(start.formatted(.dateTime.weekday(.abbreviated)))" }
    return "starts \(start.formatted(.dateTime.day().month(.abbreviated)))"
  }

  /// A day as the Mac's chips write one: "Today", "Tomorrow", "Yesterday",
  /// else "Mon 3 Nov" (`widgets::format_day`).
  static func day(_ date: Date, now: Date = .now, calendar: Calendar = .current) -> String {
    let days = calendar.dateComponents(
      [.day], from: calendar.startOfDay(for: now), to: calendar.startOfDay(for: date)
    ).day ?? 0
    switch days {
    case 0: return "Today"
    case 1: return "Tomorrow"
    case -1: return "Yesterday"
    default: return date.formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated))
    }
  }

  static func isOverdue(_ date: Date, now: Date = .now, calendar: Calendar = .current) -> Bool {
    let hasTime = !calendar.isDate(date, equalTo: calendar.startOfDay(for: date), toGranularity: .minute)
    return hasTime ? date < now : calendar.startOfDay(for: date) < calendar.startOfDay(for: now)
  }

  static func isToday(_ date: Date, calendar: Calendar = .current) -> Bool {
    calendar.isDateInToday(date)
  }

  /// `9:41`, the clock time a day finishes by.
  static func time(_ date: Date) -> String {
    date.formatted(date: .omitted, time: .shortened)
  }

  static func dayTitle(_ date: Date, calendar: Calendar = .current) -> String {
    if calendar.isDateInToday(date) { return "Today" }
    if calendar.isDateInYesterday(date) { return "Yesterday" }
    return date.formatted(.dateTime.weekday(.wide).day().month(.wide))
  }
}
