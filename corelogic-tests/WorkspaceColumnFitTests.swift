import XCTest

@testable import TaktCore

final class WorkspaceColumnFitTests: XCTestCase {
  private typealias Fit = WorkspaceColumnFit

  private func fit(_ available: Double, sidebar: Double?, right: Double?) -> Fit.Widths {
    Fit.fit(
      available: available, chrome: 2,
      sidebar: sidebar.map { Fit.Dock(width: $0, minimum: 140) },
      rightDock: right.map { Fit.Dock(width: $0, minimum: 210) },
      mainMinimum: 420)
  }

  func testAWideWindowKeepsTheRememberedWidths() {
    XCTAssertEqual(fit(1400, sidebar: 300, right: 280), Fit.Widths(sidebar: 300, rightDock: 280))
  }

  /// 900 wide leaves 478 for the docks, 102 short of 580; the sidebar can spare
  /// 160 and the dock 70, so they give 102 in that proportion.
  func testANarrowWindowShrinksTheDocksBySlack() {
    let widths = fit(900, sidebar: 300, right: 280)
    XCTAssertEqual(widths.sidebar + widths.rightDock, 478, accuracy: 0.001)
    XCTAssertEqual(300 - widths.sidebar, 102 * 160 / 230, accuracy: 0.001)
    XCTAssertGreaterThanOrEqual(widths.rightDock, 210)
  }

  func testAHiddenDockLeavesItsRoomToTheOther() {
    XCTAssertEqual(fit(760, sidebar: 420, right: nil), Fit.Widths(sidebar: 338, rightDock: 0))
  }

  func testAtTheirMinimumsTheMainPaneGivesWay() {
    XCTAssertEqual(fit(760, sidebar: 300, right: 280), Fit.Widths(sidebar: 140, rightDock: 210))
  }

  func testNothingEverOverflowsTheRow() {
    for available in stride(from: 0.0, through: 1600, by: 37) {
      let widths = fit(available, sidebar: 420, right: 480)
      XCTAssertLessThanOrEqual(widths.sidebar + widths.rightDock, max(available - 2, 0) + 0.001)
    }
  }

  func testADragStopsWhereTheMainPaneWouldGoUnderItsMinimum() {
    XCTAssertEqual(
      Fit.largest(available: 1000, chrome: 2, other: 280, mainMinimum: 420, minimum: 140, maximum: 420), 298)
    XCTAssertEqual(
      Fit.largest(available: 760, chrome: 2, other: 280, mainMinimum: 420, minimum: 140, maximum: 420), 140)
  }
}
