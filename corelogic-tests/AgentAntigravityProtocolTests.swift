import XCTest

@testable import TaktCore

/// `agy`'s stream-json and the typed command. The fixtures are lines from a
/// real `agy` 1.4.9 session run with `AgentAntigravityInvocation`, trimmed.
final class AgentAntigravityProtocolTests: XCTestCase {

  func testInitStartsTheSession() {
    var parser = AgentAntigravityStreamParser()
    let events = parser.events(
      fromLine: #"{"event":"init","conversation_id":"c1","init":{"cwd":"/a","agent":"takt-panel","tools":["call_mcp_tool","finish"],"permission_mode":"request-review"}}"#
    )
    XCTAssertEqual(events, [.started(sessionID: "c1", model: nil, tools: ["call_mcp_tool", "finish"])])
  }

  func testAResponsesDeltasArriveAsOneTextWhenItsStepIsDone() {
    var parser = AgentAntigravityStreamParser()
    XCTAssertEqual(
      parser.events(
        fromLine: #"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":3,"state":"ACTIVE","step_type":"agent_response","text_delta":"You have "}}"#
      ), [])
    XCTAssertEqual(
      parser.events(
        fromLine: #"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":3,"state":"ACTIVE","step_type":"agent_response","text_delta":"**13** lists."}}"#
      ), [])
    XCTAssertEqual(
      parser.events(
        fromLine: #"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":3,"state":"DONE","step_type":"agent_response","text_delta":"\n","duration_seconds":2.7}}"#
      ), [.text("You have **13** lists.")])
    // Thinking alone, with no text, says nothing.
    XCTAssertEqual(
      parser.events(
        fromLine: #"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":1,"state":"DONE","step_type":"agent_response","duration_seconds":3.7}}"#
      ), [])
  }

  func testATaktToolCallIsAToolUseAndThenItsResult() {
    var parser = AgentAntigravityStreamParser()
    let use = AgentToolUse(id: "step-2", name: "mcp__takt__workspace_tree", input: .object([:]))
    XCTAssertEqual(
      parser.events(
        fromLine: #"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":2,"state":"ACTIVE","step_type":"tool","tool_name":"call_mcp_tool","tool_info":{"name":"call_mcp_tool","parameters":{"Arguments":{},"ServerName":"takt","ToolName":"workspace_tree"}}}}"#
      ), [.toolUse(use)])
    XCTAssertEqual(
      parser.events(
        fromLine: #"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":2,"state":"DONE","step_type":"tool","tool_name":"call_mcp_tool","duration_seconds":0.37,"tool_info":{"name":"call_mcp_tool","parameters":{"Arguments":{},"ServerName":"takt","ToolName":"workspace_tree"},"output":"Workspace\n\n{\"undo_step\":\"7\"}"}}}"#
      ),
      [.toolUse(use), .toolResult(toolUseID: "step-2", isError: false, text: "Workspace\n\n{\"undo_step\":\"7\"}")])
  }

  func testARefusedCallIsAFailedResult() {
    var parser = AgentAntigravityStreamParser()
    let events = parser.events(
      fromLine: #"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":6,"state":"ERROR","step_type":"tool","tool_name":"call_mcp_tool","duration_seconds":0.03,"tool_info":{"name":"call_mcp_tool","parameters":{"Arguments":{"title":"probe"},"ServerName":"takt","ToolName":"workspace_task_add"},"error":{"type":"TOOL_ERROR","message":"permission check failed for mcp \"takt/workspace_task_add\""}}}}"#
    )
    XCTAssertEqual(
      events,
      [
        .toolUse(
          AgentToolUse(
            id: "step-6", name: "mcp__takt__workspace_task_add", input: .object(["title": .string("probe")]))),
        .toolResult(
          toolUseID: "step-6", isError: true, text: "permission check failed for mcp \"takt/workspace_task_add\""),
      ])
  }

  func testABuiltInToolIsNotDrawn() {
    var parser = AgentAntigravityStreamParser()
    XCTAssertEqual(
      parser.events(
        fromLine: #"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":4,"state":"DONE","step_type":"tool","tool_name":"run_command","tool_info":{"name":"run_command","parameters":{"CommandLine":"ls"},"output":""}}}"#
      ), [])
  }

  func testTheResultEndsTheTurn() {
    var parser = AgentAntigravityStreamParser()
    XCTAssertEqual(
      parser.events(
        fromLine: #"{"event":"result","result":{"conversation_id":"c1","status":"SUCCESS","response":"hi\n","num_turns":1}}"#
      ), [.turnFinished(AgentTurnResult(isError: false, subtype: "success", text: "hi\n", costUSD: nil))])
    XCTAssertEqual(
      parser.events(
        fromLine: #"{"event":"result","result":{"conversation_id":"c1","status":"ERROR","response":"","error":"failed to construct executor","finished_reason":"error"}}"#
      ),
      [.turnFinished(AgentTurnResult(isError: true, subtype: "error", text: "failed to construct executor", costUSD: nil))])
  }

  func testAUserMessageIsOneLine() {
    XCTAssertEqual(
      AgentAntigravityEncoder.userMessage("hi\nthere"), #"{"event":"user","message":{"content":"hi\nthere"}}"# + "\n")
  }

  func testTheSettingsAllowOnlyTaktsToolsAndDenyTheBuiltIns() throws {
    let both = try XCTUnwrap(JSONValue.parse(AgentAntigravityInvocation.settings(asksBeforeChanges: false)))
    let allowed = both["permissions"]?["allow"]?.arrayValue?.compactMap(\.stringValue) ?? []
    XCTAssertTrue(allowed.contains("mcp(takt/workspace_tree)"))
    XCTAssertTrue(allowed.contains("mcp(takt/workspace_task_add)"))
    XCTAssertTrue(allowed.allSatisfy { $0.hasPrefix("mcp(takt/") })
    XCTAssertEqual(
      both["permissions"]?["deny"]?.arrayValue?.compactMap(\.stringValue),
      ["command(*)", "read_file(*)", "write_file(*)", "read_url(*)", "execute_url(*)"])

    let reads = try XCTUnwrap(JSONValue.parse(AgentAntigravityInvocation.settings(asksBeforeChanges: true)))
    let readOnly = reads["permissions"]?["allow"]?.arrayValue?.compactMap(\.stringValue) ?? []
    XCTAssertTrue(readOnly.contains("mcp(takt/workspace_tree)"))
    XCTAssertFalse(readOnly.contains("mcp(takt/workspace_task_add)"))
  }

  func testTheArgumentsNameThePanelsAgent() {
    let arguments = AgentAntigravityInvocation.arguments()
    XCTAssertEqual(Array(arguments.prefix(2)), ["--agent", "takt-panel"])
    XCTAssertFalse(arguments.contains("-p"))
    XCTAssertTrue(arguments.contains("--disable-slash-commands"))
  }

  func testTheAgentFileHasNoToolsAndThePromptAsItsBody() {
    let file = AgentAntigravityInvocation.agentFile(systemPrompt: "Be brief.")
    XCTAssertTrue(file.hasPrefix("---\nname: takt-panel\n"))
    XCTAssertTrue(file.contains("\ntools: []\n"))
    XCTAssertTrue(file.hasSuffix("---\nBe brief.\n"))
  }

  func testTheMCPConfigIsTaktsServerAlone() throws {
    let config = try XCTUnwrap(JSONValue.parse(AgentAntigravityInvocation.mcpConfig(helperPath: "/h/takt")))
    XCTAssertEqual(config["mcpServers"]?.objectValue?.keys.sorted(), ["takt"])
    XCTAssertEqual(config["mcpServers"]?["takt"]?["command"]?.stringValue, "/h/takt")
  }

  // MARK: - The typed command

  func testATypedCommandSplitsIntoWordsTheWayAShellWould() {
    XCTAssertEqual(AgentClient.words(of: "dense claude"), ["dense", "claude"])
    XCTAssertEqual(AgentClient.words(of: "  ~/bin/my agent  --flag "), ["~/bin/my", "agent", "--flag"])
    XCTAssertEqual(AgentClient.words(of: #""/Apps/My Agent/run" 'a b' c\ d"#), ["/Apps/My Agent/run", "a b", "c d"])
    XCTAssertEqual(AgentClient.words(of: #"x """#), ["x", ""])
    XCTAssertEqual(AgentClient.words(of: ""), [])
  }

  func testATypedCommandsBinaryIsLookedUpAndItsRestLeadsTheArguments() {
    let bare = AgentClient.custom.candidates(userPath: "dense claude", homeDirectory: "/Users/a")
    XCTAssertEqual(bare.first, "/Users/a/.local/bin/dense")
    XCTAssertTrue(bare.contains("/opt/homebrew/bin/dense"))
    XCTAssertEqual(
      AgentClient.custom.candidates(userPath: "~/bin/dense claude", homeDirectory: "/Users/a"),
      [("~/bin/dense" as NSString).expandingTildeInPath])
    XCTAssertEqual(AgentClient.custom.candidates(userPath: "  ", homeDirectory: "/Users/a"), [])
    XCTAssertEqual(AgentClient.custom.leadingArguments(userPath: "dense claude --x"), ["claude", "--x"])
    XCTAssertEqual(AgentClient.claudeCode.leadingArguments(userPath: "/bin/claude --x"), [])
  }

  func testATypedCommandSpeaksClaudeCodesOrAntigravitysFormat() {
    XCTAssertEqual(AgentWire.storedCustom(nil), .claudeCode)
    XCTAssertEqual(AgentWire.storedCustom("agy"), .antigravity)
    XCTAssertEqual(AgentWire.storedCustom("codex"), .claudeCode)
    XCTAssertEqual(AgentClient.custom.wire(custom: .antigravity), .antigravity)
    XCTAssertEqual(AgentClient.antigravity.wire(custom: .claudeCode), .antigravity)
    XCTAssertFalse(AgentWire.antigravity.canAskBeforeChanges)
  }

  func testAntigravityIsLookedForWhereItsInstallerPutsIt() {
    let candidates = AgentClient.antigravity.candidates(userPath: nil, homeDirectory: "/Users/a")
    XCTAssertEqual(candidates.first, "/Users/a/.local/bin/agy")
  }

  func testAdviceForAntigravity() {
    XCTAssertTrue(AgentFailureAdvice.advice(for: "Not logged in", client: .antigravity)?.contains("`agy`") == true)
    XCTAssertTrue(AgentFailureAdvice.advice(for: "fetch failed", client: .antigravity)?.contains("Google") == true)
    XCTAssertTrue(
      AgentFailureAdvice.advice(for: #"Error: --settings: unknown setting "x""#, client: .antigravity)?
        .contains("agy update") == true)
  }
}
