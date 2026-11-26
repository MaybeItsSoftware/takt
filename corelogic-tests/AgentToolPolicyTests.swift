import XCTest

@testable import TaktCore

final class AgentToolPolicyTests: XCTestCase {

  /// The two lists together are the CLI's whole tool table (57 tools, per
  /// `docs/mcp-server.md`), with nothing in both.
  func testReadAndWriteToolsPartitionTheServer() {
    let read = Set(AgentToolPolicy.readOnlyTools)
    let write = Set(AgentToolPolicy.writeTools)
    XCTAssertTrue(read.isDisjoint(with: write))
    XCTAssertEqual(read.count + write.count, 57)
    XCTAssertTrue(read.contains("agent_instructions_get"))
    XCTAssertTrue(write.contains("agent_instructions_set"), "saving instructions is a change")
  }

  func testWritesArePreAllowedUnlessTheUserAsksFirst() {
    for tool in AgentToolPolicy.writeTools {
      let qualified = AgentToolPolicy.qualifiedName(tool)
      XCTAssertFalse(AgentToolPolicy.isReadOnly(qualified), tool)
      XCTAssertTrue(AgentToolPolicy.isPreAllowed(qualified, asksBeforeChanges: false), tool)
      XCTAssertFalse(AgentToolPolicy.isPreAllowed(qualified, asksBeforeChanges: true), tool)
    }
    XCTAssertTrue(AgentToolPolicy.isReadOnly("mcp__takt__workspace_tasks"))
    XCTAssertTrue(AgentToolPolicy.isPreAllowed("mcp__takt__workspace_tasks", asksBeforeChanges: true))
    // Another server's tool of the same name is not ours to allow, and a tool
    // nobody classified yet asks either way.
    XCTAssertFalse(AgentToolPolicy.isReadOnly("mcp__other__workspace_tasks"))
    XCTAssertFalse(AgentToolPolicy.isPreAllowed("mcp__other__workspace_task_add", asksBeforeChanges: false))
    XCTAssertFalse(AgentToolPolicy.isPreAllowed("mcp__takt__brand_new_tool", asksBeforeChanges: false))
    XCTAssertFalse(AgentToolPolicy.isReadOnly("workspace_tasks"))
  }

  func testAPermissionRequestAsksOrIsRefused() {
    XCTAssertEqual(
      AgentToolPolicy.decision(forPermissionRequestOn: "mcp__takt__workspace_task_delete"), .askUser)
    XCTAssertEqual(AgentToolPolicy.decision(forPermissionRequestOn: "mcp__takt__task_search"), .askUser)
    XCTAssertEqual(AgentToolPolicy.decision(forPermissionRequestOn: "Bash"), .refuse)
    XCTAssertEqual(AgentToolPolicy.decision(forPermissionRequestOn: "mcp__filesystem__write"), .refuse)
    XCTAssertEqual(AgentToolPolicy.decision(forPermissionRequestOn: "mcp__takt__"), .refuse)
  }

  func testInvocationLoadsOnlyTaktAndPreAllowsItsClassifiedTools() throws {
    let arguments = AgentInvocation.arguments(
      helperPath: "/Applications/Takt.app/Contents/Helpers/takt", systemPrompt: "Hi")
    func value(after flag: String) -> String? {
      arguments.firstIndex(of: flag).map { arguments[$0 + 1] }
    }
    XCTAssertEqual(arguments.first, "-p")
    XCTAssertEqual(value(after: "--input-format"), "stream-json")
    XCTAssertEqual(value(after: "--output-format"), "stream-json")
    XCTAssertTrue(arguments.contains("--strict-mcp-config"))
    XCTAssertEqual(value(after: "--tools"), "", "every built-in tool is off")
    XCTAssertEqual(value(after: "--permission-mode"), "manual")
    XCTAssertEqual(value(after: "--permission-prompt-tool"), "stdio")
    XCTAssertEqual(value(after: "--setting-sources"), "")
    XCTAssertEqual(value(after: "--system-prompt"), "Hi")
    XCTAssertFalse(arguments.contains("--model"))
    XCTAssertFalse(arguments.contains("--dangerously-skip-permissions"))

    let allowed = try XCTUnwrap(value(after: "--allowedTools")).split(separator: ",").map(String.init)
    XCTAssertEqual(
      Set(allowed),
      Set((AgentToolPolicy.readOnlyTools + AgentToolPolicy.writeTools).map(AgentToolPolicy.qualifiedName)))

    // Asking first, only the reads are pre-allowed and the writes ask.
    let asking = AgentInvocation.arguments(helperPath: "/p", systemPrompt: "", asksBeforeChanges: true)
    let asked = try XCTUnwrap(asking.firstIndex(of: "--allowedTools").map { asking[$0 + 1] })
    XCTAssertEqual(
      Set(asked.split(separator: ",").map(String.init)),
      Set(AgentToolPolicy.readOnlyTools.map(AgentToolPolicy.qualifiedName)))

    let config = try XCTUnwrap(JSONValue.parse(try XCTUnwrap(value(after: "--mcp-config"))))
    let servers = try XCTUnwrap(config["mcpServers"]?.objectValue)
    XCTAssertEqual(Array(servers.keys), ["takt"])
    XCTAssertEqual(servers["takt"]?["command"], .string("/Applications/Takt.app/Contents/Helpers/takt"))
    XCTAssertEqual(servers["takt"]?["args"], .array([.string("--mcp-server")]))
  }

  func testAModelIsPassedOnlyWhenSet() {
    let arguments = AgentInvocation.arguments(helperPath: "/p", systemPrompt: "", model: " sonnet ")
    XCTAssertEqual(arguments.suffix(2), ["--model", "sonnet"])
    XCTAssertFalse(AgentInvocation.arguments(helperPath: "/p", systemPrompt: "", model: " ").contains("--model"))
  }

  func testLocatorTriesTheUsersPathFirstThenTheInstallerLocations() {
    let candidates = AgentCLILocator.candidates(userPath: " /opt/claude ", homeDirectory: "/Users/a")
    XCTAssertEqual(
      candidates,
      [
        "/opt/claude", "/Users/a/.local/bin/claude", "/Users/a/.claude/local/claude",
        "/opt/homebrew/bin/claude", "/usr/local/bin/claude",
      ])
    XCTAssertEqual(AgentCLILocator.candidates(userPath: "", homeDirectory: "/Users/a").count, 4)
    XCTAssertEqual(
      AgentCLILocator.resolve(candidates: candidates, isExecutable: { $0.hasPrefix("/opt/homebrew") }),
      "/opt/homebrew/bin/claude")
    XCTAssertNil(AgentCLILocator.resolve(candidates: candidates, isExecutable: { _ in false }))
  }

  func testSystemPromptCarriesTheDateAndTheRuleForChanges() {
    let date = ISO8601DateFormatter().date(from: "2026-09-28T09:00:00Z")!
    let text = AgentSystemPrompt.text(today: date, timeZone: TimeZone(identifier: "Europe/London")!)
    XCTAssertTrue(text.contains("2026-09-28"))
    XCTAssertTrue(text.contains("Monday 28 September 2026"))
    XCTAssertTrue(text.contains("turn a phrase like \"friday\""))
    XCTAssertTrue(text.contains("apply straight away"))
    XCTAssertTrue(text.contains("If they undo one, do not make it again"))
    let asking = AgentSystemPrompt.text(today: date, asksBeforeChanges: true)
    XCTAssertTrue(asking.contains("only runs if they approve it"))
    XCTAssertFalse(asking.contains("apply straight away"))
  }

  func testTheAssistantIsToldWhatWasUndone() {
    XCTAssertNil(AgentSystemPrompt.undone([]))
    XCTAssertEqual(
      AgentSystemPrompt.undone(["MCP: New Task"]), "[The user undid this change of yours: \"MCP: New Task\".]")
    XCTAssertEqual(
      AgentSystemPrompt.undone(["MCP: Rename", "MCP: New Task"]),
      "[The user undid these changes of yours: \"MCP: Rename\", \"MCP: New Task\".]")
  }

  func testAWriteResultNamesItsUndoStep() {
    let text = "Task added\n\n{\n  \"id\": \"T1\",\n  \"undo_step\": \"G-1\"\n}"
    XCTAssertEqual(AgentToolResult.undoStep(fromText: text), "G-1")
    XCTAssertNil(AgentToolResult.undoStep(fromText: "Workspace\n\n{\"lists\": []}"))
    XCTAssertNil(AgentToolResult.undoStep(fromText: "not json at all"))
    XCTAssertNil(AgentToolResult.undoStep(fromText: "Title\n\n[1, 2]"))
  }

  func testSystemPromptTellsTheAgentToKeepTheStandingInstructions() {
    let text = AgentSystemPrompt.text(today: Date())
    XCTAssertTrue(text.contains("agent_instructions_set"))
    XCTAssertTrue(text.contains("narrowest scope"))
    XCTAssertTrue(text.contains("rather than piling up duplicates"))
    XCTAssertTrue(text.contains("Never save a one-off request"))
  }

  func testContextNamesWhatIsOnScreen() {
    XCTAssertNil(AgentSystemPrompt.context(listName: nil, listID: nil, taskTitle: nil, taskID: nil))
    XCTAssertEqual(
      AgentSystemPrompt.context(listName: "Groceries", listID: "L1", taskTitle: "Milk", taskID: "T1"),
      #"[The user is looking at list "Groceries" (id L1), with selected task "Milk" (id T1)]"#)
  }

  // MARK: - Summaries

  func testWriteSummaryResolvesIdsAndReadsInOrder() {
    let summary = AgentToolSummary.describe(
      tool: "mcp__takt__workspace_task_add",
      input: .object([
        "list_id": .string("L1"), "title": .string("Milk"), "notes": .string(""), "at_top": .bool(true),
      ]),
      name: { $0 == "L1" ? "Groceries" : nil })
    XCTAssertEqual(summary.title, "Add a task")
    XCTAssertFalse(summary.isDestructive)
    XCTAssertEqual(
      summary.fields,
      [
        .init(label: "Title", value: "Milk"),
        .init(label: "List", value: "Groceries"),
        .init(label: "At the top", value: "Yes"),
      ])
  }

  func testAnUnresolvedIdIsShownRatherThanHidden() {
    let summary = AgentToolSummary.describe(
      tool: "mcp__takt__workspace_task_delete", input: .object(["task_id": .string("T9")]))
    XCTAssertTrue(summary.isDestructive)
    XCTAssertEqual(summary.fields, [.init(label: "Task", value: "T9")])
  }

  func testClearingAValueSaysSo() {
    let summary = AgentToolSummary.describe(
      tool: "mcp__takt__workspace_task_update",
      input: .object(["task_id": .string("T1"), "kanban_column": .null]),
      name: { _ in "Milk" })
    XCTAssertEqual(
      summary.fields, [.init(label: "Task", value: "Milk"), .init(label: "Column", value: "None")])
  }

  func testDatesPrioritiesAndEstimatesReadAsWords() {
    let london = TimeZone(identifier: "Europe/London")!
    let now = ISO8601DateFormatter().date(from: "2026-10-14T12:00:00Z")!
    let before = ["due": "None", "priority": "3 (Medium)", "estimate_minutes": "45 min"]
    let summary = AgentToolSummary.describe(
      tool: "mcp__takt__workspace_task_update",
      input: .object([
        "task_id": .string("T1"), "due": .string("2026-10-16"), "priority": .int(1),
        "start": .string("2026-10-15 09:30"), "estimate_minutes": .int(90),
      ]),
      name: { _ in "Report" }, current: { before[$0] }, now: now, timeZone: london)
    XCTAssertEqual(
      summary.fields,
      [
        .init(label: "Task", value: "Report"),
        .init(label: "Due", value: "Fri 16 Oct", previous: "None"),
        .init(label: "Start", value: "Thu 15 Oct, 09:30"),
        .init(label: "Priority", value: "1 (Urgent)", previous: "3 (Medium)"),
        .init(label: "Estimate", value: "1 h 30 min", previous: "45 min"),
      ])
  }

  func testClearingAPriorityOrADateSaysNone() {
    let now = ISO8601DateFormatter().date(from: "2026-10-14T12:00:00Z")!
    let summary = AgentToolSummary.describe(
      tool: "mcp__takt__workspace_task_update",
      input: .object([
        "task_id": .string("T1"), "due": .null, "priority": .int(0), "estimate_minutes": .null,
      ]),
      current: { ["due": "Fri 16 Oct", "priority": "2 (High)"][$0] }, now: now)
    XCTAssertEqual(
      summary.fields,
      [
        .init(label: "Task", value: "T1"),
        .init(label: "Due", value: "None", previous: "Fri 16 Oct"),
        .init(label: "Priority", value: "None", previous: "2 (High)"),
        .init(label: "Estimate", value: "None"),
      ])
    // Another year says which; a phrase Checkvist takes as it is stays as it is.
    XCTAssertEqual(AgentToolSummary.words(for: "due", value: .string("2027-01-04"), now: now), "Mon 4 Jan 2027")
    XCTAssertEqual(AgentToolSummary.words(for: "due", value: .string("friday"), now: now), "friday")
    XCTAssertEqual(AgentToolSummary.words(for: "estimate_minutes", value: .int(120)), "2 h")
  }

  func testAnUnknownToolStillGetsWords() {
    let summary = AgentToolSummary.describe(
      tool: "mcp__takt__brand_new_tool", input: .object(["some_flag": .bool(false)]))
    XCTAssertEqual(summary.title, "Brand new tool")
    XCTAssertEqual(summary.fields, [.init(label: "Some flag", value: "No")])
  }

  func testReadLineNamesTheToolAndWhatItLookedAt() {
    XCTAssertEqual(
      AgentToolSummary.readLine(tool: "mcp__takt__task_search", input: .object(["query": .string("milk")])),
      "task_search · milk")
    XCTAssertEqual(
      AgentToolSummary.readLine(
        tool: "mcp__takt__workspace_tasks", input: .object(["list_id": .string("L1")]),
        name: { _ in "Groceries" }),
      "workspace_tasks · Groceries")
    XCTAssertEqual(
      AgentToolSummary.readLine(tool: "mcp__takt__workspace_tree", input: .object([:])), "workspace_tree")
  }

  func testAnInstructionsEditShowsTheWholeTextBeforeAndAfter() {
    let replace = AgentToolSummary.describe(
      tool: "mcp__takt__agent_instructions_set",
      input: .object(["scope": .string("list"), "id": .string("L1"), "text": .string("Verbs first.")]),
      name: { $0 == "L1" ? "Groceries" : nil },
      current: { $0 == "text" ? "Keep it short." : nil })
    XCTAssertEqual(replace.title, "Save standing instructions for the agent")
    XCTAssertEqual(
      replace.fields,
      [
        .init(label: "Scope", value: "list"),
        .init(label: "For", value: "Groceries"),
        .init(label: "Instructions", value: "Verbs first.", previous: "Keep it short."),
      ])

    let append = AgentToolSummary.describe(
      tool: "mcp__takt__agent_instructions_set",
      input: .object(["scope": .string("account"), "text": .string("Weekdays only."), "mode": .string("append")]),
      current: { $0 == "text" ? "Keep it short." : nil })
    XCTAssertEqual(append.fields.first { $0.label == "Instructions" }?.value, "Keep it short.\n\nWeekdays only.")
    XCTAssertEqual(append.fields.first { $0.label == "Instructions" }?.previous, "Keep it short.")

    let clear = AgentToolSummary.describe(
      tool: "mcp__takt__agent_instructions_set",
      input: .object(["scope": .string("account"), "text": .string("  ")]),
      current: { $0 == "text" ? "Keep it short." : nil })
    XCTAssertEqual(
      clear.fields.first { $0.label == "Instructions" }, .init(label: "Instructions", value: "None", previous: "Keep it short."))
  }
}
