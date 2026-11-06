import Foundation

/// What runs once the workspace is open, outside any screen: the widget
/// bridge, the Live Activity. One line per feature.
@MainActor
let featureInstallers: [(WorkspaceModel) -> Void] = [
  SyncController.install(on:),
  { WidgetBridge.shared.install(on: $0) },
  { WatchBridge.shared.install(on: $0) },
  { QuickAddRouting.install(on: $0) },
  GoogleCalendarController.install(on:),
  { ReminderService.shared.install(on: $0) },
]
