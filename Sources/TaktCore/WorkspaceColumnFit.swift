import Foundation

/// Fits the desktop window's side docks into the width the window actually has.
///
/// The sidebar and the right dock keep the widths you dragged them to, and the
/// main pane wants a minimum of its own. Laid out as asked, a narrow window got
/// a row wider than itself: SwiftUI centred the overflow and clipped it at both
/// edges, so the sidebar lost its leading edge and the list its trailing one.
///
/// Fitting never rewrites the remembered widths — widen the window again and
/// the docks come back to them. The docks give way first, each in proportion
/// to how far it sits above its own minimum; only once both are at their
/// minimums does the main pane go below its own.
public enum WorkspaceColumnFit {

  public struct Dock: Equatable {
    public var width: Double
    public var minimum: Double

    public init(width: Double, minimum: Double) {
      self.width = width
      self.minimum = minimum
    }
  }

  public struct Widths: Equatable {
    public var sidebar: Double
    public var rightDock: Double

    public init(sidebar: Double, rightDock: Double) {
      self.sidebar = sidebar
      self.rightDock = rightDock
    }
  }

  /// - Parameters:
  ///   - available: the row's whole width, handles included.
  ///   - chrome: what the resize handles take, which no pane can have.
  ///   - sidebar, rightDock: `nil` for a dock that is hidden.
  ///   - mainMinimum: what the main pane wants before the docks are its minimums.
  public static func fit(
    available: Double, chrome: Double, sidebar: Dock?, rightDock: Dock?, mainMinimum: Double
  ) -> Widths {
    let wanted = Widths(sidebar: sidebar?.width ?? 0, rightDock: rightDock?.width ?? 0)
    let floor = Widths(sidebar: sidebar.map { min($0.minimum, $0.width) } ?? 0,
                       rightDock: rightDock.map { min($0.minimum, $0.width) } ?? 0)
    let forDocks = max(available - chrome, 0)
    let room = max(forDocks - mainMinimum, 0)

    let total = wanted.sidebar + wanted.rightDock
    if total <= room { return wanted }

    // Down towards the minimums, sharing the cut by how much each can spare.
    let slack = (wanted.sidebar - floor.sidebar) + (wanted.rightDock - floor.rightDock)
    let excess = total - room
    if slack >= excess, slack > 0 {
      return Widths(
        sidebar: wanted.sidebar - excess * (wanted.sidebar - floor.sidebar) / slack,
        rightDock: wanted.rightDock - excess * (wanted.rightDock - floor.rightDock) / slack)
    }

    // Both at their minimums: the main pane gives up its own, and only if even
    // the minimums don't fit the window do they scale down together.
    let floorTotal = floor.sidebar + floor.rightDock
    if floorTotal <= forDocks || floorTotal == 0 { return floor }
    let scale = forDocks / floorTotal
    return Widths(sidebar: floor.sidebar * scale, rightDock: floor.rightDock * scale)
  }

  /// The widest a dock may be dragged with the other one where it is: past
  /// that the main pane would be under its minimum, and the drag would move
  /// nothing on screen while it went on changing the remembered width.
  public static func largest(
    available: Double, chrome: Double, other: Double, mainMinimum: Double, minimum: Double, maximum: Double
  ) -> Double {
    min(max(available - chrome - other - mainMinimum, minimum), maximum)
  }
}
