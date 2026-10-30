import Foundation

/// Which coding agent the agent panel runs: the user's own CLI, headless,
/// with Takt's MCP server as its tools. `custom` is a command the user types
/// (`dense claude`, say), run with the arguments of whichever wire format it
/// speaks (`AgentWire`).
///
/// Distinct from `MCPClientCatalog`, which is every app Takt can add its MCP
/// server to. Most of those — Claude Desktop, Cursor, VS Code, Zed — are
/// editors with an agent inside, not a command the panel can start and talk
/// to over a pipe. These are the ones it can: each has a headless mode that
/// takes Takt's server on the command line and streams its turn back as
/// JSON lines.
///
/// The choice is kept in `UserDefaults` (`defaultsKey`) and read when a
/// thread starts, so changing it leaves a running thread on the agent it
/// started with. See `docs/agent-panel.md`.
public enum AgentClient: String, CaseIterable, Identifiable, Sendable {
  case claudeCode = "claude-code"
  case codex
  case antigravity = "agy"
  case custom

  public var id: String { rawValue }

  /// `UserDefaults`: which wire format the typed command speaks.
  public static let customWireDefaultsKey = "agentCustomWireV1"

  /// `UserDefaults`: the agent new threads use.
  public static let defaultsKey = "agentClientV1"

  /// The stored choice, or Claude Code — the panel's only agent before there
  /// was a choice — when there is none or it names one this build lacks.
  public static func stored(_ raw: String?) -> AgentClient {
    raw.flatMap(AgentClient.init(rawValue:)) ?? .claudeCode
  }

  public var displayName: String {
    switch self {
    case .claudeCode: "Claude Code"
    case .codex: "Codex"
    case .antigravity: "Antigravity"
    case .custom: "Typed command"
    }
  }

  /// The wire format a thread on this agent speaks; `custom` is whichever
  /// the user picked for it.
  public func wire(custom: AgentWire) -> AgentWire {
    switch self {
    case .claudeCode: .claudeCode
    case .codex: .codex
    case .antigravity: .antigravity
    case .custom: custom
    }
  }

  /// The binary's name, as the installers leave it.
  public var executableName: String {
    switch self {
    case .claudeCode: "claude"
    case .codex: "codex"
    case .antigravity: "agy"
    case .custom: "command"
    }
  }

  /// `UserDefaults`: a path to the binary set by hand. Claude Code's is the
  /// key it had before there was a choice, so a path set then still holds.
  /// The typed command's is the whole command, arguments and all.
  public var userPathDefaultsKey: String {
    switch self {
    case .claudeCode: AgentCLILocator.userPathDefaultsKey
    case .codex: "agentCodexExecutablePathV1"
    case .antigravity: "agentAntigravityExecutablePathV1"
    case .custom: "agentCustomCommandV1"
    }
  }

  /// Who the agent belongs to, for "could not reach …".
  public var provider: String {
    switch self {
    case .claudeCode: "Anthropic"
    case .codex: "OpenAI"
    case .antigravity: "Google"
    case .custom: "its provider"
    }
  }

  /// Where to get it and how to sign in, for the panel's setup state.
  public var installHint: String {
    switch self {
    case .claudeCode:
      "Install it from claude.com/claude-code and sign in once in a terminal (`claude`), or give the path to "
        + "an existing `claude` binary below. No API key is needed; it uses your Claude Code login."
    case .codex:
      "Install it (`brew install codex` or `npm install -g @openai/codex`) and sign in once in a terminal "
        + "(`codex login`), or give the path to an existing `codex` binary below. It uses your Codex login."
    case .antigravity:
      "Install the Antigravity CLI from antigravity.google and sign in once in a terminal (`agy`), or give the "
        + "path to an existing `agy` binary below. It uses your Antigravity login."
    case .custom:
      "Type the command that starts your agent, as you would in a terminal — `dense claude`, say. Aliases "
        + "aren't seen, so name what the alias runs. Takt adds the headless arguments for the format it "
        + "speaks (Settings ▸ Agent)."
    }
  }

  /// What goes between the binary and the wire format's own arguments: the
  /// rest of a typed command. Nothing for the others.
  public func leadingArguments(userPath: String?) -> [String] {
    self == .custom ? Array(Self.words(of: userPath ?? "").dropFirst()) : []
  }

  /// A typed command split the way a shell would split it for words: on
  /// whitespace, with single or double quotes keeping a word together and a
  /// backslash escaping the next character. No expansion beyond a leading `~`
  /// on the binary, which `candidates` does.
  public static func words(of command: String) -> [String] {
    var words: [String] = []
    var current = ""
    var inWord = false
    var quote: Character?
    var escaped = false
    for character in command {
      if escaped {
        current.append(character)
        escaped = false
      } else if character == "\\", quote != "'" {
        escaped = true
        inWord = true
      } else if let open = quote {
        if character == open { quote = nil } else { current.append(character) }
      } else if character == "\"" || character == "'" {
        quote = character
        inWord = true
      } else if character.isWhitespace {
        if inWord { words.append(current) }
        current = ""
        inWord = false
      } else {
        current.append(character)
        inWord = true
      }
    }
    if inWord { words.append(current) }
    return words
  }

  /// Where the binary is looked for, first match wins: a path set by hand,
  /// then where each installer puts it.
  public func candidates(userPath: String?, homeDirectory: String) -> [String] {
    if self == .custom {
      // The command's first word: a path as it stands, or a bare name looked
      // up where installers put things, since there is no shell `PATH`.
      guard let binary = Self.words(of: userPath ?? "").first else { return [] }
      if binary.contains("/") { return [(binary as NSString).expandingTildeInPath] }
      return ["\(homeDirectory)/.local/bin", "/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"]
        .map { "\($0)/\(binary)" }
    }
    var candidates: [String] = []
    if let trimmed = userPath?.trimmingCharacters(in: .whitespacesAndNewlines), !trimmed.isEmpty {
      candidates.append((trimmed as NSString).expandingTildeInPath)
    }
    switch self {
    case .claudeCode:
      candidates += [
        "\(homeDirectory)/.local/bin/claude",
        "\(homeDirectory)/.claude/local/claude",
        "/opt/homebrew/bin/claude",
        "/usr/local/bin/claude",
      ]
    case .codex:
      candidates += [
        "/opt/homebrew/bin/codex",
        "/usr/local/bin/codex",
        "\(homeDirectory)/.local/bin/codex",
        "\(homeDirectory)/.npm-global/bin/codex",
      ]
    case .antigravity:
      candidates += [
        "\(homeDirectory)/.local/bin/agy",
        "/opt/homebrew/bin/agy",
        "/usr/local/bin/agy",
      ]
    case .custom:
      break
    }
    var seen = Set<String>()
    return candidates.filter { seen.insert($0).inserted }
  }
}

/// The line format a thread speaks, which decides its arguments, how its
/// output is read and what goes in on stdin. Each fixed agent speaks its own;
/// a typed command speaks the one the user picked for it.
public enum AgentWire: String, CaseIterable, Identifiable, Sendable {
  /// Claude Code's `-p` stream-json: one process per thread.
  case claudeCode = "claude-code"
  /// `codex exec --json`: one process per turn.
  case codex
  /// `agy`'s stream-json: one process per thread.
  case antigravity = "agy"

  public var id: String { rawValue }

  /// The formats a typed command may speak.
  public static let forCustomCommands: [AgentWire] = [.claudeCode, .antigravity]

  /// The stored choice for the typed command, or Claude Code's.
  public static func storedCustom(_ raw: String?) -> AgentWire {
    raw.flatMap(AgentWire.init(rawValue:)).flatMap { forCustomCommands.contains($0) ? $0 : nil } ?? .claudeCode
  }

  public var displayName: String {
    switch self {
    case .claudeCode: "Claude Code"
    case .codex: "Codex"
    case .antigravity: "Antigravity"
    }
  }

  /// Whether a write can stop and wait for Approve. Claude Code asks its host
  /// over stdio; neither Codex's headless mode nor `agy`'s has anyone to ask,
  /// so with "Ask before changing tasks" on, their threads get only the reads.
  public var canAskBeforeChanges: Bool { self == .claudeCode }
}
