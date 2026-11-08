import Foundation
import SwiftUI
import TaktWorkspace

/// A file chosen in Google's Picker, as a task links it.
struct PickedDriveFile: Equatable {
  let url: String
  let name: String
  let mimeType: String?
}

/// Linking Drive files on the iPhone (docs/links.md "Phones"). Takt asks for
/// `drive.file`, which reaches only the files picked, so the file list is
/// Google's own Picker for mobile apps: a consent with `trigger_onepick` in
/// the sign-in sheet that comes back with the ids picked. Each is then read
/// with Drive's `files.get` for the name, type and the `https://` address
/// the link is stored by. No API key is involved; the iOS OAuth client is
/// `GoogleIOSClientID`, the one Calendar uses.
enum GoogleDriveFiles {
  private static let folder = "application/vnd.google-apps.folder"

  /// Picks in Google's Picker and describes what was picked; empty when
  /// nothing was.
  @MainActor
  static func pick() async throws -> [PickedDriveFile] {
    let auth = GoogleCalendarAuth.drive()
    let ids = try await auth.pickDriveFiles()
    guard !ids.isEmpty else { return [] }
    let token = try await auth.accessToken()
    var files: [PickedDriveFile] = []
    for id in ids where isID(id) {
      if let file = try await describe(id, token: token) { files.append(file) }
    }
    return files
  }

  /// A Drive id as Google makes them; anything else is not asked about.
  static func isID(_ id: String) -> Bool {
    !id.isEmpty && id.unicodeScalars.allSatisfy { CharacterSet.alphanumerics.contains($0) || $0 == "-" || $0 == "_" }
      && id.allSatisfy(\.isASCII)
  }

  /// Drive's own viewing address when it is a Drive or Docs one, else the
  /// one made from the id.
  static func linkURL(id: String, mimeType: String?, webViewLink: String?) -> String? {
    if let link = webViewLink?.trimmingCharacters(in: .whitespaces),
      link.hasPrefix("https://docs.google.com/") || link.hasPrefix("https://drive.google.com/")
    {
      return link
    }
    guard isID(id) else { return nil }
    return mimeType == folder
      ? "https://drive.google.com/drive/folders/\(id)" : "https://drive.google.com/file/d/\(id)/view"
  }

  /// One `files.get` answer as a link.
  static func read(id: String, json: Data) -> PickedDriveFile? {
    guard let file = (try? JSONSerialization.jsonObject(with: json)) as? [String: Any] else { return nil }
    let mimeType = file["mimeType"] as? String
    guard let url = linkURL(id: id, mimeType: mimeType, webViewLink: file["webViewLink"] as? String) else {
      return nil
    }
    let name = (file["name"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return PickedDriveFile(url: url, name: name.isEmpty ? "Untitled" : name, mimeType: mimeType)
  }

  private static func describe(_ id: String, token: String) async throws -> PickedDriveFile? {
    var components = URLComponents(string: "https://www.googleapis.com/drive/v3/files/\(id)")!
    components.queryItems = [
      .init(name: "fields", value: "id,name,mimeType,webViewLink"),
      .init(name: "supportsAllDrives", value: "true"),
    ]
    var request = URLRequest(url: components.url!)
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    let (data, response) = try await URLSession.shared.data(for: request)
    let status = (response as? HTTPURLResponse)?.statusCode ?? 0
    if (200..<300).contains(status), let file = read(id: id, json: data) { return file }
    // Picked but not describable: still linked, by its id.
    return linkURL(id: id, mimeType: nil, webViewLink: nil).map {
      PickedDriveFile(url: $0, name: "Google Drive file", mimeType: nil)
    }
  }
}

/// "Pick from Google Drive" under the inspector's links: Google's Picker,
/// then each file linked through the store's `addLink` with its name and
/// type. Says it isn't set up when this build has no Google OAuth client.
struct DriveLinkButton: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  let taskID: String
  /// Saves the inspector's draft first, so a pending edit to the address
  /// list isn't lost under the new links.
  let beforeWriting: () -> Void
  let afterWriting: () -> Void
  @State private var picking = false
  @State private var note: String?

  var body: some View {
    VStack(alignment: .leading, spacing: theme.space.xs) {
      Button {
        Task { await pick() }
      } label: {
        Label(picking ? "Choosing in Google Drive…" : "Pick from Google Drive", systemImage: "folder.badge.plus")
      }
      .buttonStyle(ThemedButtonStyle(kind: .quiet, compact: true))
      .disabled(picking)
      .accessibilityIdentifier("inspector.links.drive")
      if let note {
        Text(note).font(theme.type.footnote).foregroundStyle(theme.muted)
      }
    }
  }

  private func pick() async {
    guard GoogleCalendarAuth.isConfigured else {
      note = "Google Drive linking isn't set up in this build: it has no Google OAuth client."
      return
    }
    picking = true
    defer { picking = false }
    do {
      let files = try await GoogleDriveFiles.pick()
      guard !files.isEmpty else {
        note = "Nothing linked from Drive."
        return
      }
      beforeWriting()
      let linked = model.perform { store in
        for file in files {
          try store.addLink(file.url, title: file.name, mimeType: file.mimeType, to: taskID)
        }
      }
      afterWriting()
      note = linked ? (files.count == 1 ? "Linked from Drive." : "Linked \(files.count) files from Drive.") : nil
    } catch GoogleCalendarAuthError.cancelled {
      note = nil
    } catch {
      note = error.localizedDescription
    }
  }
}
