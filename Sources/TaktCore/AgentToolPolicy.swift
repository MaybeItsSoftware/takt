import Foundation
import TaktRustCore

/// What the agent panel's assistant may do on its own, and what it has to ask.
///
/// The assistant gets exactly one MCP server, Takt's own (the bundled `takt`
/// CLI), and no built-in tools at all — no shell, no files, no web. Of that
/// server's tools, the ones that only read are always pre-allowed on the
/// command line. The writes are pre-allowed too unless the user has turned on
/// "Ask before changing tasks" (Settings ▸ Agent, `asksBeforeChangesKey`, off
/// by default): then they are left to Claude Code's permission check, which
/// with `--permission-prompt-tool stdio` asks the app, which asks the user.
/// Either way each write is shown as a card, and one that made an undo step
/// can be taken back from it. See `docs/agent-panel.md`.
///
/// Both lists are allow-lists. A tool added to `cli/src/tools.rs` tomorrow is
/// in neither until it is classified, so it asks even when changes do not.
public enum AgentToolPolicy {

  /// The MCP server's name in `--mcp-config`, and so the middle of every
  /// tool's qualified name: `mcp__takt__task_search`.
  public static let serverName = "takt"
  public static let toolPrefix = "mcp__\(serverName)__"

  /// The tools that change nothing. The classification is the Rust core's
  /// (`core/src/agent_tools.rs`), which the CLI's tests hold to its own tool
  /// table, so a new tool cannot go unclassified.
  public static let readOnlyTools: [String] = agentReadOnlyTools()

  /// The tools that write. Pre-allowed unless the user asks to approve
  /// changes; a tool in neither list always asks.
  public static let writeTools: [String] = agentWriteTools()

  /// `UserDefaults`: whether every write waits for Approve. Off by default:
  /// changes apply at once and each card has an Undo.
  public static let asksBeforeChangesKey = "agentAsksBeforeChangesV1"

  /// The tools a thread may call without asking.
  public static func preAllowedTools(asksBeforeChanges: Bool) -> [String] {
    asksBeforeChanges ? readOnlyTools : readOnlyTools + writeTools
  }

  /// A write that runs without asking in a thread started with
  /// `asksBeforeChanges`.
  public static func isPreAllowed(_ qualified: String, asksBeforeChanges: Bool) -> Bool {
    priorityToolName(qualified).map(preAllowedTools(asksBeforeChanges: asksBeforeChanges).contains) ?? false
  }

  public static func qualifiedName(_ tool: String) -> String { toolPrefix + tool }

  /// `task_search` from `mcp__priority__task_search`; `nil` for anything that
  /// is not one of Priority's tools.
  public static func priorityToolName(_ qualified: String) -> String? {
    guard qualified.hasPrefix(toolPrefix) else { return nil }
    let bare = String(qualified.dropFirst(toolPrefix.count))
    return bare.isEmpty ? nil : bare
  }

  public static func isReadOnly(_ qualified: String) -> Bool {
    priorityToolName(qualified).map(readOnlyTools.contains) ?? false
  }

  /// What the panel does with a permission request for `qualified`: one for
  /// a write when the user asks before changes, or for a tool not yet
  /// classified either way.
  public enum Decision: Equatable, Sendable {
    /// Show the approval card and wait for a click.
    case askUser
    /// Refuse without asking: not one of Priority's tools, so nothing the
    /// user agreed to let the assistant have. It should never arrive —
    /// `--tools ""` and `--strict-mcp-config` leave nothing else loaded —
    /// and if it does the answer is no.
    case refuse
  }

  /// Never `allow`: whatever is allowed is allowed by the command line, and
  /// anything that reaches here asks.
  public static func decision(forPermissionRequestOn qualified: String) -> Decision {
    priorityToolName(qualified) == nil ? .refuse : .askUser
  }
}

/// The command line the panel launches, built here so it can be tested and
/// quoted in the docs exactly.
public enum AgentInvocation {

  /// The `--mcp-config` document: Priority's server and nothing else. Its
  /// environment is inherited, so the helper finds the same database and
  /// credentials the app's own MCP clients do.
  public static func mcpConfig(helperPath: String) -> String {
    JSONValue.object([
      "mcpServers": .object([
        AgentToolPolicy.serverName: .object([
          "type": .string("stdio"),
          "command": .string(helperPath),
          "args": .array([.string("--mcp-server")]),
        ])
      ])
    ]).jsonString()
  }

  /// Every argument after the executable.
  ///
  /// - `-p` with stream-json both ways keeps one process per thread, so a
  ///   follow-up is just another line on stdin.
  /// - `--tools ""` removes every built-in tool. `--strict-mcp-config` stops
  ///   the user's own MCP servers loading beside Priority's.
  /// - `--setting-sources ""` skips the user's, project's and local settings,
  ///   so no hook, plugin or permission rule written for their coding sessions
  ///   can run here or pre-approve a write.
  /// - `--allowedTools` is the reads, and the writes unless
  ///   `asksBeforeChanges`; `AgentToolPolicy.preAllowedTools`.
  /// - `--permission-mode manual` is the mode that asks for anything else;
  ///   `auto` and `acceptEdits` would let a classifier or a rule answer.
  /// - `--permission-prompt-tool stdio` routes each question to the app.
  /// - `--no-session-persistence`: a thread lives as long as its process;
  ///   nothing is written to `~/.claude/projects`.
  public static func arguments(
    helperPath: String,
    systemPrompt: String,
    model: String? = nil,
    asksBeforeChanges: Bool = false
  ) -> [String] {
    var arguments = [
      "-p",
      "--input-format", "stream-json",
      "--output-format", "stream-json",
      "--verbose",
      "--mcp-config", mcpConfig(helperPath: helperPath),
      "--strict-mcp-config",
      "--tools", "",
      "--allowedTools",
      AgentToolPolicy.preAllowedTools(asksBeforeChanges: asksBeforeChanges)
        .map(AgentToolPolicy.qualifiedName).joined(separator: ","),
      "--permission-mode", "manual",
      "--permission-prompt-tool", "stdio",
      "--setting-sources", "",
      "--no-session-persistence",
      "--system-prompt", systemPrompt,
    ]
    if let model = model?.trimmingCharacters(in: .whitespacesAndNewlines), !model.isEmpty {
      arguments += ["--model", model]
    }
    return arguments
  }
}

/// Where the user's agent is.
///
/// A GUI app does not see the shell's `PATH` — or its aliases, which is how
/// `claude` is often spelled — so the binary is looked for where the installer
/// puts it, after a path the user set by hand (`AgentClient.candidates`).
public enum AgentCLILocator {

  /// Claude Code's hand-set path, from before there was a choice of agent.
  public static let userPathDefaultsKey = "agentClaudeExecutablePathV1"

  public static func candidates(
    userPath: String?, homeDirectory: String, client: AgentClient = .claudeCode
  ) -> [String] {
    client.candidates(userPath: userPath, homeDirectory: homeDirectory)
  }

  public static func resolve(candidates: [String], isExecutable: (String) -> Bool) -> String? {
    candidates.first(where: isExecutable)
  }
}

/// What the assistant is told about where it is.
public enum AgentSystemPrompt {

  /// The system prompt for a thread. Replaces Claude Code's own, which is
  /// about writing software with tools this process does not have. With
  /// `canChange` false the thread has only the reads: the user asks before
  /// changes and the agent has no way to ask (`AgentWire.canAskBeforeChanges`).
  public static func text(
    today: Date, timeZone: TimeZone = .current, asksBeforeChanges: Bool = false, canChange: Bool = true
  ) -> String {
    let formatter = DateFormatter()
    formatter.locale = Locale(identifier: "en_GB")
    formatter.timeZone = timeZone
    formatter.dateFormat = "EEEE d MMMM yyyy"
    let iso = DateFormatter()
    iso.locale = Locale(identifier: "en_US_POSIX")
    iso.timeZone = timeZone
    iso.dateFormat = "yyyy-MM-dd"
    return """
      You are the assistant inside Takt, a keyboard-first macOS task app. You help the \
      user understand, plan and organise their tasks. Today is \(formatter.string(from: today)) \
      (\(iso.string(from: today))). The tools take dates only as ISO (2026-10-17, or \
      2026-10-17 17:00 for a time), so turn a phrase like "friday" or "next week" into one \
      from today yourself: "friday" is the next Friday after today.

      You have Takt's own tools and nothing else: no shell, no files, no web. The \
      workspace_* tools read and edit the app's own lists and tasks, which are the source of \
      truth; ids there are uppercase UUIDs. The task_* tools reach Checkvist, an optional \
      integration the user may not have set up. Use the read tools freely to answer questions.

      \(!canChange ? readOnlyRule : asksBeforeChanges ? approvalRule : undoRule)

      \(keysRule)

      A message may begin with bracketed notes from the app rather than the user: what is on \
      screen, and the user's standing instructions for their account, a folder or a list. \
      Instructions are sent again only when they change; the latest replace any before them.

      You keep those standing instructions. When the user states a durable preference, fact or \
      convention — about a list, a folder, or how they work — save it with \
      agent_instructions_set, as a change they see like any other, in the narrowest scope that \
      fits: a list's for \
      that list alone, a folder's for every list in it, the account's only for how they work \
      everywhere. Keep them short and current: read the scope with agent_instructions_get \
      first, then edit or merge into what is there rather than piling up duplicates, and drop \
      what the new instruction replaces. Never save a one-off request — what to do with this \
      task, today, this once — as a standing instruction.

      Reply in short, plain Markdown. Refer to tasks and lists by their titles, not their ids.
      """
  }

  /// The rule for writes when each one waits for Approve.
  static let approvalRule = """
    Every change — adding, editing, completing, moving or deleting anything — is shown to the \
    user as a card and only runs if they approve it. So before calling a tool that writes, say \
    in one sentence what you are about to change, then call it with exactly that change. Make \
    one change per call, so each can be approved or declined on its own. If a change is \
    declined, do not retry it; ask what they would like instead.
    """

  /// The rule when the thread has no writes at all.
  static let readOnlyRule = """
    In this thread you can only read. The user wants to approve every change before it runs, \
    and this agent has no way to ask them, so the tools that change things are switched off. \
    If they ask for a change, say exactly what you would change, and that they can make it \
    themselves or switch the agent to Claude Code (Settings ▸ Agent) to approve changes as you \
    go.
    """

  /// The rule for writes when they apply at once.
  static let undoRule = """
    Your changes — adding, editing, completing, moving or deleting anything — apply straight \
    away, and each is shown to the user as a card with an Undo button. So before calling a \
    tool that writes, say in one sentence what you are about to change, then call it with \
    exactly that change. Make one change per call, so each can be undone on its own, and only \
    make the changes the user asked for or agreed to. If they undo one, do not make it again; \
    ask what they would like instead.
    """

  /// The app draws the keys for each change on its card (`AgentKeyHintLine`),
  /// so the assistant need not, and would only get them wrong after a rebind.
  static let keysRule = """
    Each change card also shows the keys that would have made the same change in the app, \
    worked out by the app from your call and the user's own key bindings. So do not spell out \
    keyboard shortcuts for your changes yourself.
    """

  /// Ahead of the next message when the user has taken back changes from
  /// their cards since the last one, so the assistant does not assume they
  /// still stand.
  public static func undone(_ labels: [String]) -> String? {
    guard !labels.isEmpty else { return nil }
    let named = labels.map { "\"\($0)\"" }.joined(separator: ", ")
    return "[The user undid \(labels.count == 1 ? "this change" : "these changes") of yours: \(named).]"
  }

  /// Ahead of a quick ask (`aa`): the task the request is about, by title and
  /// id, and its list, so "make it due friday" needs no lookup — and how the
  /// popup wants the answer, since it has room for a line or two.
  public static func quickAskSubject(
    taskTitle: String, taskID: String, listName: String?, listID: String?
  ) -> String {
    var subject = "task \"\(taskTitle)\" (id \(taskID))"
    if let listName, !listName.isEmpty {
      subject += " in list \"\(listName)\"" + (listID.map { " (id \($0))" } ?? "")
    }
    return """
      [Quick ask, from a small popup over one task. The request is about \(subject): "it", "this" \
      and "this task" mean that task, so act on it without searching for it first. Reply in one or \
      two short sentences; the popup shows each change as a card.]
      """
  }

  /// A line prefixed to each message, saying what the user is looking at, so
  /// "this task" and "this list" mean something. Sent to the assistant, not
  /// shown in the transcript.
  public static func context(
    listName: String?, listID: String?, taskTitle: String?, taskID: String?
  ) -> String? {
    var parts: [String] = []
    if let listName, !listName.isEmpty {
      parts.append("list \"\(listName)\"" + (listID.map { " (id \($0))" } ?? ""))
    }
    if let taskTitle, !taskTitle.isEmpty {
      parts.append("selected task \"\(taskTitle)\"" + (taskID.map { " (id \($0))" } ?? ""))
    }
    guard !parts.isEmpty else { return nil }
    return "[The user is looking at \(parts.joined(separator: ", with "))]"
  }
}
