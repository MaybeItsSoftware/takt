import Foundation

/// Every command the desktop workspace knows about, as a name rather than a
/// keystroke.
///
/// The point of the indirection is that a key, a palette row and a menu item
/// can all say the same word. Before it, the workspace's keys were a 600-line
/// `switch` over `NSEvent` and the reference sheet was a hand-written table of
/// seventy rows beside it — two lists of the same facts, with nothing keeping
/// them equal, which is precisely how the old settings pane came to tell people
/// that `u` was undo eight months after it stopped being.
///
/// `WorkspaceCommandCatalog.all` must carry one entry per case, and
/// `WorkspaceViewModel.run(_:)` must switch over all of them; the first is a
/// test and the second is the compiler.
public enum WorkspaceCommandID: String, CaseIterable, Sendable {
  // Go
  case goToday, goBoard, goOutline, goMatrix, goEverything, goInbox, goFocus, goTimeline
  case goPreviousList, goNextList, goBack, goForward
  case goListNavigator, goSearch, goCommandPalette, goKeyboardReference
  case goSidebarRegion, goTaskRegion, goInspectorRegion, goCycleRegion

  // Task
  case taskNew, taskNewAbove, taskNewChild, taskComplete, taskInvalidate, taskDelete
  case taskRename, taskRenameAppend, taskRenameInsert, taskEditDue, taskEditNotes, taskEditTags, taskEditRecurrence
  case taskEditStart, taskEditEstimate
  case taskDueToday, taskDueTomorrow, taskBury, taskClearDue, taskClearStart, taskClearNotes, taskClearTags
  case taskClearPriority, taskSendToBack, taskToggleDaily, taskHabit, taskWaiting, taskAddBlocker, taskConditions
  case taskAskAgent
  case taskTogglePlannedToday, taskStartFocus, taskMove
  case taskConvertToList, taskPromoteList, taskExtractBranch, taskOpenLink, taskAddLink
  case taskShowProgress, taskToggleInspector, taskGoToList
  case taskIndent, taskOutdent, taskMoveUp, taskMoveDown
  case taskMoveToPreviousList, taskMoveToNextList

  // Focus
  case focusFinish, focusStop, focusSetAvailable, focusSetPlace, focusManageConditions

  // Plan
  case planEnterTask, planLeaveTask, planHideCompleted, planHideLater
  case planBoardNewColumn, planBoardMoveCardLeft, planBoardMoveCardRight
  case planBoardRemoveColumn, planMatrixPlace
  case planBoardRenameColumn, planBoardMoveColumnLeft, planBoardMoveColumnRight
  case planBoardAddColumnLeft, planBoardAddColumnRight, planBoardEditColumnRules
  case planBoardMakeDefaultColumns, planBoardResetColumns
  case planToggleFold, planGrowTask, planShrinkTask, planFoldAll, planUnfoldAll

  // Lists and folders
  case listNew, listRename, listSettings, listArchive, listRestore, listComplete
  case listDelete, folderNew, folderMoveUp, folderMoveDown, folderSelectPrevious
  case listShare, listJoin, listMembers, listLeave, listStopSharing, taskAssign
  case folderSelectNext, listMoveUp, listMoveDown, listNewTaskDestination
  case folderCollapseAll, folderExpandAll
  case filterListNew, filterListEdit, listIcon

  // Window
  case windowUndo, windowRedo, windowToggleSidebar, windowToggleInspectorPane, windowToggleRightDock
  case windowToggleDoneRail, windowToggleAgentPanel, windowToggleProgressDock, windowOpenKeymap, windowReloadKeymap
  case windowShowDiagnostics, windowOpenThemesFolder, windowReloadThemes, windowExportTheme
  case windowUseClaudeCode, windowUseCodex
  case windowCloseAllDocks, windowDockNextTab, windowDockPreviousTab

  // Timeline surface
  case timelinePreviousDay, timelineNextDay, timelineToday, timelineClose
  case timelineReveal, motionTimelineSelect

  // Done rail
  case doneReveal, doneReopen, doneClose, motionDoneSelect

  // Conditions tab
  case conditionsToggle, conditionsAdjust, conditionsTypeTime, conditionsEdit, conditionsNew, conditionsCategory
  case conditionsProvides, conditionsArchive
  case conditionsClear, conditionsUseAgain, conditionsClose, motionConditionsSelect
  case conditionsMoveUp, conditionsMoveDown, conditionsMoveCategoryUp, conditionsMoveCategoryDown
  case conditionsRenameCategory, conditionsMerge, conditionsShowTasks, conditionsFold, conditionsFoldAll

  // Motions
  case motionSelectNext, motionSelectPrevious, motionSelectEnds, motionSelectPage
  case motionFindForward, motionFindBackward, motionFindRepeat, motionGoToNumber
  case motionSidebarSelect, motionSidebarExpand, motionSidebarCollapse
  case motionInspectorNext, motionInspectorPrevious, motionInspectorEnds, motionInspectorEdit
  case motionInspectorClear, motionInspectorLeave
  case motionBoardIntoCard, motionBoardOutOfCard
  case motionBoardColumn, motionOutlineUnfold, motionOutlineFold, motionDismiss, motionSetPriority
  case todayStart, motionTodayUnfold, motionTodayLeave
}
