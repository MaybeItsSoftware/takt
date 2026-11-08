import SwiftUI
import TaktRustCore
import TaktSync
import TaktWorkspace
import UIKit

/// The join sheet's invite: what a followed link or the clipboard held.
struct JoinInvite: Identifiable, Equatable {
  let id = UUID()
  let text: String
}

/// Sharing one list with other Takt accounts (docs/shared-lists-plan.md):
/// who is in it, an invite for one more person, and leaving or stopping.
struct SharedListSheet: View {
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.theme) private var theme
  @Environment(\.dismiss) private var dismiss
  let listID: String

  @State private var shared: SharedList?
  @State private var invite: SharedListInvite?
  @State private var isWorking = false
  @State private var problem: String?
  @State private var confirmingUnshare = false
  @State private var removing: SharedListMember?

  private var session: SyncSession? { SyncController.shared?.session }
  private var listName: String { shared?.name ?? model.structure.list(listID)?.name ?? "This list" }

  var body: some View {
    NavigationStack {
      Form {
        if session?.isSignedIn != true {
          Section {
            Text("Sharing needs sync. Sign in under Settings → Sync first.")
              .foregroundStyle(theme.muted)
          }
        } else {
          if let shared {
            membersSection(shared)
          } else {
            Section {
              Text(
                "Invite another Takt account to this list. Its tasks are everyone's to edit; where it sits in each person's lists, their board columns and their conditions stay their own."
              )
              .font(theme.type.footnote)
              .foregroundStyle(theme.muted)
            }
          }
          if shared?.isOwner ?? true {
            inviteSection
          }
          if let problem {
            Section {
              Text(problem).font(theme.type.caption).foregroundStyle(theme.danger)
            }
            .listRowBackground(theme.danger.opacity(0.08))
          }
          if let shared {
            Section {
              Button(shared.isOwner ? "Stop sharing" : "Leave list", role: .destructive) {
                confirmingUnshare = true
              }
              .frame(minHeight: theme.touchTarget)
            } footer: {
              Text(
                shared.isOwner
                  ? "The list stays yours and goes from everyone else's devices."
                  : "The list goes from your devices. The others keep it."
              )
            }
          }
        }
      }
      .scrollContentBackground(.hidden)
      .background(theme.paper)
      .font(theme.type.body)
      .navigationTitle(listName)
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
      }
      .onAppear { shared = model.store.sharedList(listId: listID) }
      .confirmationDialog(
        shared?.isOwner == true ? "Stop sharing \(listName)?" : "Leave \(listName)?",
        isPresented: $confirmingUnshare, titleVisibility: .visible
      ) {
        Button(shared?.isOwner == true ? "Stop sharing" : "Leave", role: .destructive) { unshare() }
      }
      .confirmationDialog(
        "Remove \(removing?.email ?? "them")?",
        isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }),
        titleVisibility: .visible, presenting: removing
      ) { member in
        Button("Remove", role: .destructive) { remove(member) }
      }
    }
    .presentationDetents([.medium, .large])
  }

  private func membersSection(_ shared: SharedList) -> some View {
    Section("People") {
      ForEach(shared.members, id: \.accountId) { member in
        HStack {
          VStack(alignment: .leading, spacing: theme.space.xs) {
            Text(member.you ? "You" : (member.email ?? "Someone"))
            if member.role == "owner" {
              Text("Owner").font(theme.type.caption).foregroundStyle(theme.muted)
            }
          }
          Spacer()
          if shared.isOwner, !member.you {
            Button("Remove", role: .destructive) { removing = member }
              .buttonStyle(.borderless)
              .frame(minHeight: theme.touchTarget)
          }
        }
      }
    }
  }

  private var inviteSection: some View {
    Section {
      if let invite {
        ShareLink(
          item: invite.link,
          subject: Text("Join \(invite.listName) in Takt"),
          message: Text("Join my list \(invite.listName) in Takt. Or enter the code \(invite.code).")
        ) {
          Label("Send the invite", systemImage: "square.and.arrow.up")
        }
        .frame(minHeight: theme.touchTarget)
        LabeledContent("Code") {
          Text(invite.code).font(theme.type.numeral).textSelection(.enabled)
        }
        Button("Copy link") { UIPasteboard.general.url = invite.link }
          .frame(minHeight: theme.touchTarget)
      } else {
        Button {
          makeInvite()
        } label: {
          if isWorking {
            ProgressView()
          } else {
            Label(shared == nil ? "Share and invite someone" : "Invite someone", systemImage: "person.badge.plus")
          }
        }
        .disabled(isWorking)
        .frame(minHeight: theme.touchTarget)
      }
    } footer: {
      Text("An invite works once, for seven days. Make one for each person.")
    }
  }

  private func makeInvite() {
    guard let session else { return }
    isWorking = true
    problem = nil
    Task {
      defer { isWorking = false }
      do {
        invite = try await session.shareList(listId: listID)
        shared = model.store.sharedList(listId: listID)
        model.didChange()
      } catch {
        problem = error.localizedDescription
      }
    }
  }

  private func unshare() {
    guard let session, let shared else { return }
    Task {
      do {
        if shared.isOwner {
          try await session.stopSharing(listId: listID)
          model.showToast("\(listName) isn't shared any more")
        } else {
          try await session.leaveList(listId: listID)
          model.showToast("Left \(listName)")
          model.navigation.go(to: .lists, isPad: UIDevice.current.userInterfaceIdiom == .pad)
        }
        model.didChange()
        dismiss()
      } catch {
        problem = error.localizedDescription
      }
    }
  }

  private func remove(_ member: SharedListMember) {
    guard let session else { return }
    Task {
      do {
        try await session.removeMember(listId: listID, accountId: member.accountId)
        self.shared = model.store.sharedList(listId: listID)
        model.didChange()
      } catch {
        problem = error.localizedDescription
      }
    }
  }
}

/// Joining a list from an invite: a followed link, the clipboard, or typed.
struct JoinListSheet: View {
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.theme) private var theme
  @Environment(\.dismiss) private var dismiss
  let initialText: String

  @State private var text = ""
  @State private var preview: SyncInvitePreview?
  @State private var isWorking = false
  @State private var problem: String?

  private var session: SyncSession? { SyncController.shared?.session }

  var body: some View {
    NavigationStack {
      Form {
        if session?.isSignedIn != true {
          Section {
            Text("Joining a shared list needs sync. Sign in under Settings → Sync first.")
              .foregroundStyle(theme.muted)
          }
        } else {
          Section {
            TextField("Invite link or code", text: $text)
              .textInputAutocapitalization(.characters)
              .autocorrectionDisabled()
              .font(theme.type.body)
              .onSubmit(lookUp)
          } footer: {
            Text("Its tasks are everyone's to edit. Where it sits in your lists and your board columns stay yours.")
          }
          if let preview {
            Section {
              LabeledContent("List", value: preview.name.isEmpty ? "A shared list" : preview.name)
              if let owner = preview.ownerEmail { LabeledContent("From", value: owner) }
            }
          }
          if let problem {
            Section {
              Text(problem).font(theme.type.caption).foregroundStyle(theme.danger)
            }
            .listRowBackground(theme.danger.opacity(0.08))
          }
        }
      }
      .scrollContentBackground(.hidden)
      .background(theme.paper)
      .navigationTitle("Join a shared list")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
        ToolbarItem(placement: .confirmationAction) {
          Button(preview?.joined == true ? "Open" : "Join") { join() }
            .disabled(isWorking || syncInviteCode(text: text) == nil || session?.isSignedIn != true)
        }
      }
      .onAppear {
        text = initialText
        if text.isEmpty, let pasted = UIPasteboard.general.string, syncInviteCode(text: pasted) != nil {
          text = pasted
        }
        lookUp()
      }
      .onChange(of: text) { _, _ in preview = nil }
    }
    .presentationDetents([.medium])
  }

  private func lookUp() {
    guard let session, session.isSignedIn, syncInviteCode(text: text) != nil else { return }
    problem = nil
    Task {
      do {
        preview = try await session.previewInvite(text)
      } catch {
        problem = error.localizedDescription
      }
    }
  }

  private func join() {
    guard let session else { return }
    isWorking = true
    problem = nil
    Task {
      defer { isWorking = false }
      do {
        let shared = try await session.joinList(text)
        model.didChange()
        model.showToast("Joined \(shared.name)")
        dismiss()
      } catch {
        problem = error.localizedDescription
      }
    }
  }
}
