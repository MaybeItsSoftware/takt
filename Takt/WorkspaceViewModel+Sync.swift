import AppKit
import TaktSync

/// The Mac's end of multi-device sync (`docs/sync.md`). The session watches
/// the outbox for local writes and pulls in the background; all this side has
/// to do is start it and reload when a pull changes something.
extension WorkspaceViewModel {
  /// Called from `init` after `watchForExternalWrites()`, and it has to stay
  /// there: a pull's rows are applied from a background connection, and the
  /// watcher recognises them as someone else's write only once it holds its
  /// first `data_version` token. Started first, a pull that landed before the
  /// token was read would move the token without the reload it is for.
  func startSync() {
    guard syncSession == nil, let store else { return }
    let session = SyncSession(
      store: store, deviceName: Host.current().localizedName ?? "Mac", platform: "macos")
    session.onRemoteChanges = { [weak self] in self?.reloadAfterExternalWrite() }
    syncSession = session
    session.setInFront(NSApp?.isActive ?? true)
    session.activate()
    watchActivationForSync()
  }

  /// Long-polls only while Takt is the active app. Behind other windows, or
  /// with the Mac left on overnight, it syncs every couple of minutes
  /// instead: the hosted server is billed for every second a long-poll is
  /// held, and a Mac app that never quits held one all day. Local edits
  /// (the quick-capture panel's included) still go up at once.
  private func watchActivationForSync() {
    guard syncActivationObservers.isEmpty else { return }
    let center = NotificationCenter.default
    for (name, inFront) in [
      (NSApplication.didBecomeActiveNotification, true),
      (NSApplication.didResignActiveNotification, false),
    ] {
      let observer = center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
        MainActor.assumeIsolated { self?.syncSession?.setInFront(inFront) }
      }
      syncActivationObservers.append(observer)
    }
  }
}
