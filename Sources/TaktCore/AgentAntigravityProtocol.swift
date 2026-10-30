import Foundation

/// The agent panel's third wire format: the Antigravity CLI's (`agy`)
/// stream-json, one JSON object per line each way.
///
/// Like Claude Code's, one process serves a whole thread: each user message
/// is a line on stdin, and the turn streams back as `step_update` events — a
/// step per user input, response and tool call, numbered across the whole
/// conversation, each going `ACTIVE` then `DONE` (or `ERROR`) — closed by a
/// `result`. A response's text arrives as `text_delta`s, so the parser keeps
/// them per step until the step is done; that is the only state it holds.
///
/// There are no control requests: a call headless `agy` would have to ask
/// about is refused on the spot, which is why an Antigravity thread cannot
/// wait for Approve (`AgentWire.canAskBeforeChanges`) and why the tools it
/// may use are an allow-list in its settings (`AgentAntigravityInvocation`).
///
/// The lines become the same `AgentStreamEvent`s as Claude Code's, so the
/// session, its cards and their Undo are the same. See `docs/agent-panel.md`;
/// the fixtures in `AgentAntigravityProtocolTests` are lines from a real
/// session (`agy` 1.4.9).
public struct AgentAntigravityStreamParser {

  /// Each response step's text so far, by step index.
  private var responses: [Int: String] = [:]

  public init() {}

  public mutating func events(fromLine line: String) -> [AgentStreamEvent] {
    let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, let value = JSONValue.parse(trimmed),
      let event = value["event"]?.stringValue
    else { return [] }

    switch event {
    case "init":
      guard let conversation = value["conversation_id"]?.stringValue else { return [] }
      let tools = value["init"]?["tools"]?.arrayValue?.compactMap(\.stringValue) ?? []
      return [.started(sessionID: conversation, model: value["init"]?["model"]?.stringValue, tools: tools)]

    case "step_update":
      guard let step = value["step_update"], let index = step["step_index"]?.intValue else { return [] }
      let state = step["state"]?.stringValue
      let finished = state == "DONE" || state == "ERROR"
      switch step["step_type"]?.stringValue {
      case "agent_response":
        if let delta = step["text_delta"]?.stringValue { responses[index, default: ""] += delta }
        guard finished, let text = responses.removeValue(forKey: index) else { return [] }
        let said = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return said.isEmpty ? [] : [.text(said)]
      case "tool":
        // Takt's tools all arrive through `call_mcp_tool`; the built-in ones
        // are switched off, and one that ran anyway is nothing the panel
        // draws, as with Codex.
        guard let info = step["tool_info"], info["name"]?.stringValue == "call_mcp_tool",
          let parameters = info["parameters"], let server = parameters["ServerName"]?.stringValue,
          let tool = parameters["ToolName"]?.stringValue
        else { return [] }
        let use = AgentToolUse(
          id: "step-\(index)", name: "mcp__\(server)__\(tool)", input: parameters["Arguments"] ?? .object([:]))
        guard finished else { return [.toolUse(use)] }
        // The call again with its result: the session draws a call once.
        let failed = state == "ERROR" || info["error"] != nil
        let text = info["error"]?["message"]?.stringValue ?? info["output"]?.stringValue ?? ""
        return [.toolUse(use), .toolResult(toolUseID: use.id, isError: failed, text: text)]
      default:
        // The user's own input echoed back, checkpoints, and `error_message`
        // steps, whose error the `result` repeats.
        return []
      }

    case "result":
      guard let result = value["result"] else { return [] }
      let status = result["status"]?.stringValue ?? ""
      let failed = status != "SUCCESS"
      return [
        .turnFinished(
          AgentTurnResult(
            isError: failed, subtype: status.lowercased(),
            text: failed ? result["error"]?.stringValue : result["response"]?.stringValue, costUSD: nil))
      ]

    default:
      return []
    }
  }
}

/// What the panel writes to `agy`'s stdin.
public enum AgentAntigravityEncoder {

  /// A user turn, one line.
  public static func userMessage(_ text: String) -> String {
    JSONValue.object([
      "event": .string("user"),
      "message": .object(["content": .string(text)]),
    ]).jsonString() + "\n"
  }
}

/// The command line, and the two files beside it, for an Antigravity thread,
/// built here so they can be tested and quoted in the docs exactly.
///
/// `agy` takes no MCP configuration, tool list or system prompt on the
/// command line. What it does read is the working directory's `.agents/`:
/// `mcp_config.json` for MCP servers, and `agents/<name>.md` for an agent
/// whose front matter lists its tools and whose body is its system prompt.
/// So the panel runs it in a directory of its own holding exactly those two
/// files, rewritten at each thread's start, and names the agent with
/// `--agent`.
public enum AgentAntigravityInvocation {

  /// The agent's name in `--agent` and its file's. Prefixed so it cannot
  /// clash with one the user wrote.
  public static let agentName = "takt-panel"

  /// Built-in tools' actions, denied outright: commands, files and the web.
  /// The agent's empty `tools` list already leaves it none; these hold even
  /// if a release stops honouring that.
  static let deniedActions = ["command(*)", "read_file(*)", "write_file(*)", "read_url(*)", "execute_url(*)"]

  /// Every argument after the executable.
  ///
  /// - stream-json both ways (and no `-p`) keeps one process per thread, so a
  ///   follow-up is one more line on stdin.
  /// - `--agent` is the agent written to `.agents/agents/` (`agentFile`):
  ///   no built-in tools, the panel's system prompt.
  /// - `--settings` applies to this session only and is never saved. Its
  ///   `permissions` deny the built-in actions and allow Takt's reads and
  ///   writes — only the reads when the user asks before changes, which `agy`
  ///   cannot do. Headless `agy` refuses any call no rule allows, so that
  ///   allow-list is every tool the thread has: one of the user's own MCP
  ///   servers is there to call, and refused.
  /// - `--disable-slash-commands`: a message starting `/` is text, not a
  ///   command that would end the session.
  public static func arguments(asksBeforeChanges: Bool = false) -> [String] {
    [
      "--agent", agentName,
      "--input-format", "stream-json",
      "--output-format", "stream-json",
      "--disable-slash-commands",
      "--settings", settings(asksBeforeChanges: asksBeforeChanges),
    ]
  }

  /// The `--settings` document.
  public static func settings(asksBeforeChanges: Bool = false) -> String {
    let allowed = AgentToolPolicy.preAllowedTools(asksBeforeChanges: asksBeforeChanges)
      .map { "mcp(\(AgentToolPolicy.serverName)/\($0))" }
    return JSONValue.object([
      "permissions": .object([
        "allow": .array(allowed.map(JSONValue.string)),
        "deny": .array(deniedActions.map(JSONValue.string)),
      ])
    ]).jsonString()
  }

  /// `.agents/mcp_config.json`: Takt's server and nothing else. Its
  /// environment is inherited, so the helper finds the same database and
  /// credentials the app's own MCP clients do.
  public static func mcpConfig(helperPath: String) -> String {
    JSONValue.object([
      "mcpServers": .object([
        AgentToolPolicy.serverName: .object([
          "command": .string(helperPath),
          "args": .array([.string("--mcp-server")]),
        ])
      ])
    ]).jsonString(pretty: true) + "\n"
  }

  /// `.agents/agents/<agentName>.md`: no built-in tools, not offered as a
  /// subagent, and the system prompt as its body.
  public static func agentFile(systemPrompt: String) -> String {
    """
    ---
    name: \(agentName)
    description: "The assistant in Takt's agent panel: Takt's own tools and nothing else."
    tools: []
    subagent: false
    ---
    \(systemPrompt)

    """
  }
}
