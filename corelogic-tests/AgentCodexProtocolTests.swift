import XCTest

@testable import TaktCore

final class AgentCodexProtocolTests: XCTestCase {

  // Lines from a real `codex exec --json` turn (Codex 0.156), the result's
  // text shortened.
  private let started = #"{"type":"thread.started","thread_id":"01a16849-25a5-7470-8d5d-8ab1f1d18502"}"#
  private let message =
    #"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"I’ll check the available takt tools."}}"#
  private let callStarted =
    #"{"type":"item.started","item":{"id":"item_1","type":"mcp_tool_call","server":"takt","tool":"task_lists","arguments":{},"result":null,"error":null,"status":"in_progress"}}"#
  private let callCompleted =
    #"{"type":"item.completed","item":{"id":"item_1","type":"mcp_tool_call","server":"takt","tool":"task_lists","arguments":{},"result":{"content":[{"type":"text","text":"Checklists\n\n[]"}]},"error":null,"status":"completed"}}"#
  private let callRefused =
    #"{"type":"item.completed","item":{"id":"item_1","type":"mcp_tool_call","server":"takt","tool":"task_lists","arguments":{},"result":null,"error":{"message":"MCP tool call requires approval, but approval policy is never"},"status":"failed"}}"#
  private let completed =
    #"{"type":"turn.completed","usage":{"input_tokens":38139,"cached_input_tokens":23168,"output_tokens":86}}"#

  func testATurnReadsAsTheSameEventsAsClaudeCodes() {
    XCTAssertEqual(
      AgentCodexStreamParser.events(fromLine: started),
      [.started(sessionID: "01a16849-25a5-7470-8d5d-8ab1f1d18502", model: nil, tools: [])])
    XCTAssertEqual(AgentCodexStreamParser.events(fromLine: message), [.text("I’ll check the available takt tools.")])
    let use = AgentToolUse(id: "item_1", name: "mcp__takt__task_lists", input: .object([:]))
    XCTAssertEqual(AgentCodexStreamParser.events(fromLine: callStarted), [.toolUse(use)])
    XCTAssertEqual(
      AgentCodexStreamParser.events(fromLine: callCompleted),
      [.toolUse(use), .toolResult(toolUseID: "item_1", isError: false, text: "Checklists\n\n[]")])
    XCTAssertEqual(
      AgentCodexStreamParser.events(fromLine: completed),
      [.turnFinished(AgentTurnResult(isError: false, subtype: "success", text: nil, costUSD: nil))])
  }

  func testARefusedCallIsAFailedResultWithCodexsReason() {
    XCTAssertEqual(
      AgentCodexStreamParser.events(fromLine: callRefused).last,
      .toolResult(
        toolUseID: "item_1", isError: true, text: "MCP tool call requires approval, but approval policy is never"))
  }

  func testAFailedTurnCarriesItsMessage() {
    let line = #"{"type":"turn.failed","error":{"message":"You've hit your usage limit."}}"#
    XCTAssertEqual(
      AgentCodexStreamParser.events(fromLine: line),
      [.turnFinished(AgentTurnResult(isError: true, subtype: "error", text: "You've hit your usage limit.", costUSD: nil))])
  }

  func testReasoningRetriesAndNoiseAreNoEvents() {
    XCTAssertEqual(AgentCodexStreamParser.events(fromLine: #"{"type":"turn.started"}"#), [])
    XCTAssertEqual(
      AgentCodexStreamParser.events(
        fromLine: #"{"type":"item.completed","item":{"id":"item_2","type":"reasoning","text":"Thinking"}}"#), [])
    XCTAssertEqual(AgentCodexStreamParser.events(fromLine: #"{"type":"error","message":"Reconnecting... 1/5"}"#), [])
    XCTAssertEqual(AgentCodexStreamParser.events(fromLine: "not json"), [])
  }

  func testTheInvocationGivesTaktsServerAndNothingThatWrites() {
    let arguments = AgentCodexInvocation.arguments(helperPath: "/A/Takt.app/Contents/Helpers/takt", systemPrompt: "Be \"brief\".\nOK")
    XCTAssertEqual(Array(arguments.prefix(2)), ["exec", "--json"])
    XCTAssertEqual(arguments.last, "-", "the message goes on stdin, never as an argument")
    let config = zip(arguments, arguments.dropFirst()).filter { $0.0 == "-c" }.map(\.1)
    XCTAssertTrue(config.contains("sandbox_mode=\"read-only\""))
    XCTAssertTrue(config.contains("mcp_servers.takt.command=\"/A/Takt.app/Contents/Helpers/takt\""))
    XCTAssertTrue(config.contains("developer_instructions=\"Be \\\"brief\\\".\\nOK\""))
    XCTAssertTrue(config.contains("features.shell_tool=false"))
    XCTAssertTrue(config.contains("mcp_servers.takt.default_tools_approval_mode=\"approve\""))
    XCTAssertTrue(arguments.contains("--ignore-user-config"))
  }

  func testAskingFirstLeavesCodexOnlyTheReads() {
    func enabledTools(_ asks: Bool) -> String? {
      AgentCodexInvocation.arguments(helperPath: "/h", systemPrompt: "", asksBeforeChanges: asks)
        .first { $0.hasPrefix("mcp_servers.takt.enabled_tools=") }
    }
    XCTAssertTrue(enabledTools(false)?.contains("\"task_add\"") == true)
    XCTAssertFalse(enabledTools(true)?.contains("\"task_add\"") == true)
    XCTAssertTrue(enabledTools(true)?.contains("\"task_lists\"") == true)
  }

  func testAFollowUpResumesTheThread() {
    let arguments = AgentCodexInvocation.arguments(helperPath: "/h", systemPrompt: "", resumeThreadID: "T1")
    XCTAssertEqual(Array(arguments.prefix(3)), ["exec", "resume", "T1"])
  }

  func testTheStoredChoiceFallsBackToClaudeCode() {
    XCTAssertEqual(AgentClient.stored(nil), .claudeCode)
    XCTAssertEqual(AgentClient.stored("gemini"), .claudeCode)
    XCTAssertEqual(AgentClient.stored("codex"), .codex)
    XCTAssertEqual(AgentClient.claudeCode.userPathDefaultsKey, AgentCLILocator.userPathDefaultsKey)
    XCTAssertEqual(AgentClient.stored("agy"), .antigravity)
    XCTAssertTrue(AgentClient.claudeCode.wire(custom: .antigravity).canAskBeforeChanges)
    XCTAssertFalse(AgentClient.codex.wire(custom: .claudeCode).canAskBeforeChanges)
  }

  func testCodexIsLookedForWhereItsInstallersPutIt() {
    let candidates = AgentCLILocator.candidates(userPath: "~/bin/codex ", homeDirectory: "/Users/a", client: .codex)
    XCTAssertEqual(candidates.first, ("~/bin/codex" as NSString).expandingTildeInPath)
    XCTAssertTrue(candidates.contains("/opt/homebrew/bin/codex"))
    XCTAssertFalse(candidates.contains { $0.hasSuffix("/claude") })
  }

  func testAdviceNamesTheAgentThatFailed() {
    XCTAssertTrue(AgentFailureAdvice.advice(for: "Not logged in", client: .codex)?.contains("codex login") == true)
    XCTAssertTrue(AgentFailureAdvice.advice(for: "fetch failed", client: .codex)?.contains("OpenAI") == true)
  }

  func testAReadOnlyThreadIsToldItCannotChangeAnything() {
    let text = AgentSystemPrompt.text(today: Date(timeIntervalSince1970: 0), asksBeforeChanges: true, canChange: false)
    XCTAssertTrue(text.contains("you can only read"))
    XCTAssertFalse(text.contains("only runs if they approve it"))
  }
}
