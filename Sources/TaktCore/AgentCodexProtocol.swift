import Foundation

/// The agent panel's other wire format: Codex's `codex exec --json`, one
/// JSON object per line, out only.
///
/// Codex's headless mode is a turn per process rather than a thread per
/// process: the message goes in on stdin, the turn streams back as
/// `thread.started`, `turn.started`, `item.started` / `item.completed` and
/// `turn.completed` (or `turn.failed`), and the process exits. The next
/// message is `codex exec resume <thread id>`, which picks the conversation
/// up from Codex's own session store. There are no control requests: nothing
/// asks the host anything, which is why a Codex thread cannot wait for
/// Approve (`AgentWire.canAskBeforeChanges`).
///
/// The lines are turned into the same `AgentStreamEvent`s as Claude Code's,
/// so the session, its cards and their Undo are the same for both. See
/// `docs/agent-panel.md`; the fixtures in `AgentCodexProtocolTests` are lines
/// from a real session (Codex 0.156).
public enum AgentCodexStreamParser {

  public static func events(fromLine line: String) -> [AgentStreamEvent] {
    let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, let value = JSONValue.parse(trimmed),
      let type = value["type"]?.stringValue
    else { return [] }

    switch type {
    case "thread.started":
      guard let thread = value["thread_id"]?.stringValue else { return [] }
      return [.started(sessionID: thread, model: nil, tools: [])]

    case "item.started", "item.completed":
      guard let item = value["item"], let id = item["id"]?.stringValue else { return [] }
      switch item["type"]?.stringValue {
      case "agent_message":
        guard type == "item.completed", let text = item["text"]?.stringValue,
          !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        else { return [] }
        return [.text(text)]
      case "mcp_tool_call":
        guard let server = item["server"]?.stringValue, let tool = item["tool"]?.stringValue else { return [] }
        let use = AgentToolUse(id: id, name: "mcp__\(server)__\(tool)", input: item["arguments"] ?? .object([:]))
        guard type == "item.completed" else { return [.toolUse(use)] }
        // The call again with its result: a completed item can arrive without
        // its start, and the session draws a call it has already drawn once.
        let message = item["error"]?["message"]?.stringValue
        let failed = message != nil || item["status"]?.stringValue == "failed"
          || item["result"]?["is_error"]?.boolValue == true
        return [.toolUse(use), .toolResult(toolUseID: id, isError: failed, text: message ?? resultText(item["result"]))]
      default:
        // Reasoning, and the built-in tools the invocation switches off.
        return []
      }

    case "turn.completed":
      return [.turnFinished(AgentTurnResult(isError: false, subtype: "success", text: nil, costUSD: nil))]

    case "turn.failed":
      return [
        .turnFinished(
          AgentTurnResult(
            isError: true, subtype: "error", text: value["error"]?["message"]?.stringValue, costUSD: nil))
      ]

    default:
      // `error` lines are Codex retrying a dropped connection; a failure that
      // ends the turn comes as `turn.failed`.
      return []
    }
  }

  /// An MCP result's text blocks, as the assistant read them.
  private static func resultText(_ result: JSONValue?) -> String {
    (result?["content"]?.arrayValue ?? []).compactMap { $0["text"]?.stringValue }.joined(separator: "\n")
  }
}

/// The command line for one Codex turn, built here so it can be tested and
/// quoted in the docs exactly.
public enum AgentCodexInvocation {

  /// Built-in tools switched off, by feature name. Set through `-c` rather
  /// than `--disable`, which refuses a name the installed Codex does not
  /// know: a feature renamed or retired must not stop the panel starting.
  static let disabledFeatures = [
    "shell_tool", "unified_exec", "view_image", "browser_use", "computer_use", "apps", "goals",
  ]

  /// Every argument after the executable. The message itself goes on stdin
  /// (the trailing `-`), so nothing the user typed is ever an argument.
  ///
  /// - `resume <thread>` carries on a thread; without it a new one starts.
  /// - `--ignore-user-config` and `--ignore-rules` keep the user's
  ///   `config.toml`, profiles and exec rules out, as `--setting-sources ""`
  ///   does for Claude Code: nothing written for coding sessions applies here.
  ///   Sign-in is not config, so it still holds.
  /// - `sandbox_mode="read-only"` and the features above: no shell, no file
  ///   writes, no browser, no web search.
  /// - Takt's server is the only MCP server, with `enabled_tools` the reads
  ///   and writes — or only the reads when the user asks before changes,
  ///   which Codex cannot do — each approved in advance, since headless Codex
  ///   has no one to ask and would otherwise refuse every call.
  /// - `developer_instructions` is the panel's system prompt.
  public static func arguments(
    helperPath: String,
    systemPrompt: String,
    resumeThreadID: String? = nil,
    asksBeforeChanges: Bool = false
  ) -> [String] {
    var arguments = ["exec"]
    if let resumeThreadID { arguments += ["resume", resumeThreadID] }
    arguments += [
      "--json",
      "--skip-git-repo-check",
      "--ignore-user-config",
      "--ignore-rules",
    ]
    let tools = AgentToolPolicy.preAllowedTools(asksBeforeChanges: asksBeforeChanges)
    let server = "mcp_servers.\(AgentToolPolicy.serverName)"
    var config = [
      "sandbox_mode=\"read-only\"",
      "web_search=\"disabled\"",
      "developer_instructions=\(toml(systemPrompt))",
      "\(server).command=\(toml(helperPath))",
      "\(server).args=[\(toml("--mcp-server"))]",
      "\(server).enabled_tools=[\(tools.map(toml).joined(separator: ","))]",
      "\(server).default_tools_approval_mode=\"approve\"",
    ]
    config += disabledFeatures.map { "features.\($0)=false" }
    for entry in config { arguments += ["-c", entry] }
    arguments.append("-")
    return arguments
  }

  /// A TOML basic string. JSON's string escapes are a subset of TOML's, so a
  /// JSON string is one.
  static func toml(_ text: String) -> String {
    JSONValue.string(text).jsonString()
  }
}
