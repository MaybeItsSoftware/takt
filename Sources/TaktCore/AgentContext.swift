import Foundation

/// Standing instructions for the agent, written by the user at one of three
/// scopes: the whole account, a folder, or a list.
///
/// They reach the assistant as a block at the head of a message — the account's
/// first, then each folder enclosing the current list from the outermost in,
/// then the list's own — so the narrower scope is read last and wins where
/// they disagree. See `docs/agent-panel.md`.
public struct AgentInstructionScope: Equatable, Sendable {
  public enum Kind: String, Equatable, Sendable {
    case account
    case folder
    case list
  }

  public let kind: Kind
  /// The folder's or list's id; nil for the account.
  public let id: String?
  /// What the scope is called: the folder's or list's name. Ignored for the account.
  public let name: String
  public let text: String

  public init(kind: Kind, id: String?, name: String, text: String) {
    self.kind = kind
    self.id = id
    self.name = name
    self.text = text
  }

  /// "Account", "Folder "Work"", "List "Groceries"".
  public var label: String {
    switch kind {
    case .account: "Account"
    case .folder: "Folder \"\(name)\""
    case .list: "List \"\(name)\""
    }
  }
}

public enum AgentStandingInstructions {

  /// The folder ids from the outermost folder in to `folderID`, following
  /// `parent`. A cycle in the data stops the walk rather than looping.
  public static func folderChain(from folderID: String?, parent: (String) -> String?) -> [String] {
    var chain: [String] = []
    var seen = Set<String>()
    var current = folderID
    while let id = current, seen.insert(id).inserted {
      chain.append(id)
      current = parent(id)
    }
    return chain.reversed()
  }

  /// The block for the scopes in effect, in the order given, leaving out any
  /// whose text is blank. Nil when none has any.
  public static func block(_ scopes: [AgentInstructionScope]) -> String? {
    let written = scopes.compactMap { scope -> String? in
      let text = scope.text.trimmingCharacters(in: .whitespacesAndNewlines)
      return text.isEmpty ? nil : "\(scope.label):\n\(text)"
    }
    guard !written.isEmpty else { return nil }
    return "[The user's standing instructions for where they are now. They replace any sent earlier; "
      + "follow them unless a message says otherwise. Later, narrower scopes take precedence.]\n\n"
      + written.joined(separator: "\n\n")
  }

  /// Sent in place of a block when the user moves somewhere with none, so
  /// the previous list's instructions do not carry over.
  public static let cleared = "[No standing instructions apply where the user is now; earlier ones no longer do.]"
}

/// A piece of context sent ahead of a message, only when it differs from what
/// the assistant was last told. The process keeps the conversation, so a block
/// it has already read once stays read; sending it again costs tokens and says
/// nothing.
public struct AgentContextBlock: Equatable, Sendable {
  /// Which block this is, so a later one replaces it: "instructions", "screen".
  public let key: String
  /// The block now, or nil when there is nothing to say.
  public let text: String?
  /// What to send when a block that was sent has gone, or nil to say nothing.
  public let cleared: String?

  public init(key: String, text: String?, cleared: String? = nil) {
    self.key = key
    self.text = text
    self.cleared = cleared
  }
}

/// What each block was last sent as, for one process. A new process has no
/// memory of the old one, so it starts with a new ledger.
public struct AgentContextLedger: Equatable, Sendable {
  private var sent: [String: String] = [:]

  public init() {}

  /// The blocks to send with the next message, in the order given: each one
  /// that changed since it was last sent, or its `cleared` line when it has
  /// gone. Records them as sent.
  public mutating func changes(_ blocks: [AgentContextBlock]) -> [String] {
    var out: [String] = []
    for block in blocks {
      let previous = sent[block.key]
      if let text = block.text {
        guard text != previous else { continue }
        out.append(text)
        sent[block.key] = text
      } else if previous != nil {
        sent[block.key] = nil
        if let cleared = block.cleared { out.append(cleared) }
      }
    }
    return out
  }
}

extension AgentSystemPrompt {

  /// The tasks on screen, in the order they are drawn, with their ids, so
  /// "these", "the top three" and "everything here" mean something without a
  /// read first. Capped at `limit`, and saying so when it is. Sent as a block,
  /// so only when what is on screen has changed since the last message.
  public static func visibleTasks(_ tasks: [(id: String, title: String)], limit: Int = 30) -> String? {
    guard !tasks.isEmpty else { return nil }
    let shown = tasks.prefix(limit).map { task in
      var title = task.title.replacingOccurrences(of: "\n", with: " ").trimmingCharacters(in: .whitespaces)
      if title.count > 80 { title = String(title.prefix(79)) + "…" }
      return "- \(title) (id \(task.id))"
    }
    let count = tasks.count > limit ? "the first \(limit) of \(tasks.count)" : "\(tasks.count)"
    return "[Tasks on the user's screen, top to bottom (\(count)):\n\(shown.joined(separator: "\n"))]"
  }

  public static let noVisibleTasks = "[No tasks are on the user's screen now.]"
}

/// What to do about a failure the agent reported, in words the user can act
/// on. The CLIs' own messages are written for a terminal ("Please run
/// /login"), and in the panel there is no terminal to run it in.
public enum AgentFailureAdvice {

  /// The fix for `message` — a turn's error, or the stderr of a process that
  /// died — or nil when it is not one this knows.
  public static func advice(for message: String, client: AgentClient = .claudeCode) -> String? {
    let text = message.lowercased()
    func has(_ needles: String...) -> Bool { needles.contains(where: text.contains) }
    let name = client.displayName
    if has("/login", "not logged in", "invalid api key", "please log in", "authentication", "oauth token", "codex login")
    {
      let command =
        switch client {
        case .claudeCode: "`claude`, type `/login`"
        case .codex: "`codex login`"
        case .antigravity: "`agy`"
        case .custom: "the command"
        }
      return "\(name) is not signed in. Open Terminal, run \(command) and finish signing in, "
        + "then send your message again here."
    }
    if has("unknown option", "unknown argument", "unrecognized option", "unexpected argument", "unknown configuration field",
      "unknown setting", "flag provided but not defined")
    {
      let update =
        switch client {
        case .claudeCode: "Run `claude update` in Terminal"
        case .codex: "Update it (`brew upgrade codex` or `npm install -g @openai/codex`)"
        case .antigravity: "Run `agy update` in Terminal"
        case .custom: "Update it, or check it speaks the format chosen for it in Settings ▸ Agent"
        }
      return "This \(name) is too old for the agent panel. \(update), then start a new thread."
    }
    if has("usage limit", "rate limit", "limit reached", "credit balance", "quota") {
      let check =
        switch client {
        case .claudeCode: "`claude` in Terminal says when"
        case .codex: "`codex` in Terminal says when"
        case .antigravity: "`agy` in Terminal says when"
        case .custom: "the command, run in Terminal, says when"
        }
      return "\(name) has hit a usage limit. Wait for it to reset (\(check)), then try again."
    }
    if has("enotfound", "econnrefused", "econnreset", "connection error", "fetch failed", "network error") {
      return "\(name) could not reach \(client.provider). Check the connection, then send the message again."
    }
    return nil
  }
}
