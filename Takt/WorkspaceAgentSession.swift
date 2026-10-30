import Foundation
import Observation
import TaktCore
import os

/// One agent thread: the user's own coding agent, headless, with Takt's MCP
/// server as its only tool. Which agent is `clientSetting` (Settings ▸ Agent,
/// or the panel's header), read when a thread starts: a thread stays on the
/// agent it started with, and the next one uses whatever is chosen then.
///
/// With Claude Code the process is `claude -p` in stream-json mode (see
/// `AgentInvocation` for the exact command line and why each flag is there).
/// It stays up for the thread, so a follow-up is one more line on its stdin,
/// and it goes when the thread does: Stop and New thread both end it. Codex
/// runs a process per turn instead (`AgentCodexInvocation`), each one resuming
/// the thread by its id, and Stop and New thread forget the id. Antigravity
/// (`agy`) is like Claude Code, a process per thread, but takes its tools and
/// system prompt from files in its working directory
/// (`AgentAntigravityInvocation`). A typed command speaks whichever of Claude
/// Code's and Antigravity's formats the user picked for it (`AgentWire`).
///
/// **Changes apply at once and each can be taken back.** The reads and the
/// writes are pre-allowed on the command line, and every write shows as a
/// card saying what it changed, with an Undo that reverts exactly that step
/// (`WorkspaceViewModel.undoAgentChange`). With "Ask before changing tasks"
/// on (Settings ▸ Agent, read when a thread starts) the writes are not
/// pre-allowed: each stops inside the CLI on a `can_use_tool` control request
/// that only `approve(_:)` answers yes to, from a button. A tool in neither
/// list asks either way. A request left alone just waits; stopping the
/// thread, starting a new one, quitting the app or the process dying all
/// withdraw it — the CLI that asked is gone, so the call can never run.
@MainActor
@Observable final class WorkspaceAgentSession {
  private static let log = Logger(subsystem: "uk.co.maybeitssoftware.takt", category: "WorkspaceAgentSession")

  enum ApprovalState: Equatable {
    case pending
    case approved
    /// Pre-allowed: it ran without asking, and the card is there to say what
    /// it did and to take it back.
    case automatic
    case denied
    /// The thread ended before anyone answered, so the change never ran.
    case withdrawn
  }

  /// What happened to an approved write, once the tool reports back.
  enum Outcome: Equatable {
    case applied
    case failed(String)
  }

  struct Approval: Equatable {
    let request: AgentPermissionRequest
    var state: ApprovalState
    var outcome: Outcome?
    /// What the change would replace, by input key, read when it was asked
    /// for: once approved, the task holds the new values. See
    /// `WorkspaceViewModel.agentCurrentValues`.
    var before: [String: String] = [:]
    /// The undo step the change made, from its result; nil until it has run,
    /// and for a change the journal does not cover.
    var undoStep: String?
    /// Taken back from the card. The labels are every step that went with it.
    var undoneLabels: [String]?
    /// The keys that would have made the same change, from the keymap in
    /// force when the card was drawn; nil when no key does.
    var keyHint: AgentKeyHintLine?

    init(request: AgentPermissionRequest, state: ApprovalState, before: [String: String] = [:]) {
      self.request = request
      self.state = state
      self.before = before
      keyHint = AgentKeyHintLine.forTool(request.toolName, input: request.input)
    }
  }

  enum Entry: Equatable {
    case user(String)
    case assistant(String)
    /// A pre-allowed read. `failed` is nil until its result arrives.
    case read(AgentToolUse, failed: Bool?)
    case write(Approval)
    case notice(String, isError: Bool)
  }

  struct Item: Identifiable, Equatable {
    let id = UUID()
    var entry: Entry
  }

  private(set) var items: [Item] = []
  /// A turn is under way: a message sent and its `result` not yet back.
  private(set) var isWorking = false
  /// Bumped at the end of every turn, so the workspace can look for the
  /// writes the turn made without waiting for its next poll.
  private(set) var finishedTurns = 0

  /// Reads what a write would replace, for its card. Set by the workspace.
  @ObservationIgnored var currentValues: ((String, JSONValue) -> [String: String])?

  /// Called at the end of every turn, for a thread no view is watching: the
  /// quick ask's, whose popup may have been put away while it worked.
  @ObservationIgnored var onTurnFinished: (() -> Void)?

  /// A one-shot thread ends its process when its turn does: nothing can
  /// follow up on it, and a `claude` left idle is memory for nothing. The
  /// transcript and its cards' Undo stay.
  @ObservationIgnored var endsAfterTurn = false

  /// Whether this thread's writes wait for Approve: the setting as it was
  /// when the process started.
  private(set) var asksBeforeChanges = false

  /// What the user took back from cards since the last message, to tell the
  /// assistant with the next one.
  @ObservationIgnored private var undoneSinceLastMessage: [String] = []

  /// The agent new threads run (`AgentClient.defaultsKey`). Read when a
  /// thread starts, so changing it leaves a running thread where it is — and
  /// says so in that thread's transcript.
  var clientSetting = AgentClient.stored(UserDefaults.standard.string(forKey: AgentClient.defaultsKey)) {
    didSet {
      guard clientSetting != oldValue else { return }
      UserDefaults.standard.set(clientSetting.rawValue, forKey: AgentClient.defaultsKey)
      if isRunning, client != clientSetting {
        items.append(
          Item(
            entry: .notice(
              "New threads will use \(clientSetting.displayName). This one stays on \(client.displayName) "
                + "until you start a new thread or stop it.",
              isError: false)))
      }
    }
  }

  /// The agent this thread runs: the setting as it was when it started.
  private(set) var client = AgentClient.stored(UserDefaults.standard.string(forKey: AgentClient.defaultsKey))

  /// The format the typed command speaks (Settings ▸ Agent). Read when a
  /// thread starts, like the agent itself.
  var customWireSetting = AgentWire.storedCustom(UserDefaults.standard.string(forKey: AgentClient.customWireDefaultsKey))
  {
    didSet { UserDefaults.standard.set(customWireSetting.rawValue, forKey: AgentClient.customWireDefaultsKey) }
  }

  /// The format this thread speaks, fixed when it started.
  private(set) var wire = AgentWire.claudeCode

  /// The format a thread on `client` would speak if it started now.
  func wire(for client: AgentClient) -> AgentWire { client.wire(custom: customWireSetting) }

  /// "Ask before changing tasks" (Settings ▸ Agent). Off by default. Read
  /// when a thread starts, so it applies from the next one.
  var asksBeforeChangesSetting = UserDefaults.standard.bool(forKey: AgentToolPolicy.asksBeforeChangesKey) {
    didSet { UserDefaults.standard.set(asksBeforeChangesSetting, forKey: AgentToolPolicy.asksBeforeChangesKey) }
  }

  /// A path to each agent's binary set by hand, tried before the usual
  /// install locations.
  var userExecutablePaths: [AgentClient: String] = Dictionary(
    uniqueKeysWithValues: AgentClient.allCases.map {
      ($0, UserDefaults.standard.string(forKey: $0.userPathDefaultsKey) ?? "")
    })
  {
    didSet {
      for (client, path) in userExecutablePaths where path != oldValue[client] {
        UserDefaults.standard.set(path, forKey: client.userPathDefaultsKey)
      }
    }
  }

  /// The chosen agent's hand-set path.
  var userExecutablePath: String {
    get { userExecutablePaths[clientSetting] ?? "" }
    set { userExecutablePaths[clientSetting] = newValue }
  }

  func executableCandidates(for client: AgentClient) -> [String] {
    AgentCLILocator.candidates(
      userPath: userExecutablePaths[client], homeDirectory: NSHomeDirectory(), client: client)
  }

  /// Where `client` is installed, or nil when it is not.
  func executablePath(for client: AgentClient) -> String? {
    AgentCLILocator.resolve(
      candidates: executableCandidates(for: client), isExecutable: FileManager.default.isExecutableFile)
  }

  var executableCandidates: [String] { executableCandidates(for: clientSetting) }

  /// `client` as the pickers list it, saying when it cannot run.
  func menuLabel(for client: AgentClient) -> String {
    guard executablePath(for: client) == nil else { return client.displayName }
    return "\(client.displayName) — \(client == .custom ? "not set" : "not installed")"
  }

  /// The binary a new thread would launch, or nil when there is none.
  var executablePath: String? { executablePath(for: clientSetting) }

  var pendingApproval: Item? {
    items.first { item in
      if case .write(let approval) = item.entry { return approval.state == .pending }
      return false
    }
  }

  /// A thread is under way: its process is up, or a Codex thread has an id
  /// to resume between its turns' processes.
  var isRunning: Bool { process != nil || codexThreadID != nil }

  @ObservationIgnored private var process: Process?
  /// Holds `agy`'s text between its deltas; a new one per process.
  @ObservationIgnored private var antigravityParser = AgentAntigravityStreamParser()
  /// The Codex thread the next turn resumes; nil before its first turn.
  private var codexThreadID: String?
  @ObservationIgnored private var input: FileHandle?
  @ObservationIgnored private var stderrTail = AgentOutputTail()
  /// Which process a line or an exit belongs to. A thread that was stopped can
  /// still have lines in flight; they are dropped rather than applied to the
  /// thread that replaced it.
  @ObservationIgnored private var generation = 0
  @ObservationIgnored private var stoppedOnPurpose = false
  /// Tool-use ids to the item that shows them, for matching results.
  @ObservationIgnored private var toolItems: [String: UUID] = [:]
  /// The context blocks this process has been sent. A new process starts
  /// with a new one: it remembers nothing of the last.
  @ObservationIgnored private var ledger = AgentContextLedger()

  // MARK: - The user's side

  /// Sends a message, starting the thread's process first if there is none.
  /// `blocks` and then `context` are sent ahead of the text but not shown in
  /// the transcript. `context` goes with every message; a block only when it
  /// differs from what this process was last sent, since it still has the
  /// last one.
  func send(_ text: String, context: String?, blocks: [AgentContextBlock] = []) {
    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, !isWorking, pendingApproval == nil else { return }
    if process == nil {
      guard start() else { return }
    }
    items.append(Item(entry: .user(trimmed)))
    let undone = AgentSystemPrompt.undone(undoneSinceLastMessage)
    undoneSinceLastMessage = []
    let message = (ledger.changes(blocks) + [undone, context, trimmed].compactMap { $0 }).joined(separator: "\n\n")
    let sent: Bool
    switch wire {
    case .claudeCode:
      sent = write(AgentStreamEncoder.userMessage(message))
    case .codex:
      // The whole message is the turn's stdin; closing it is what sends it.
      sent = write(message)
      try? input?.close()
      input = nil
    case .antigravity:
      sent = write(AgentAntigravityEncoder.userMessage(message))
    }
    if sent { isWorking = true }
  }

  /// Lets one write run, with exactly the input the card showed. Only ever
  /// called from the card's button or its Return key.
  func approve(_ itemID: UUID) {
    answer(itemID) { approval in
      write(AgentStreamEncoder.allow(requestID: approval.request.requestID, input: approval.request.input))
        ? .approved : nil
    }
  }

  func deny(_ itemID: UUID) {
    answer(itemID) { approval in
      write(
        AgentStreamEncoder.deny(
          requestID: approval.request.requestID,
          message: "The user declined this change. Do not retry it; ask what they would like instead."))
      // Denied whether or not the line got through: if the process is gone,
      // the call it asked about is gone with it.
      return .denied
    }
  }

  /// Records that a change was taken back (or, with nil, put back) from its
  /// card. The labels go to the assistant with the next message.
  func markUndone(_ itemID: UUID, labels: [String]?) {
    guard let index = items.firstIndex(where: { $0.id == itemID }),
      case .write(var approval) = items[index].entry
    else { return }
    approval.undoneLabels = labels
    items[index].entry = .write(approval)
    if let labels { undoneSinceLastMessage += labels } else { undoneSinceLastMessage = [] }
  }

  /// The newest card that can be taken back or put back, for Tab from the
  /// message field.
  var latestUndoableChange: Item? {
    items.last { item in
      if case .write(let approval) = item.entry { return approval.undoStep != nil }
      return false
    }
  }

  /// Ends the thread's process. The transcript stays, so what was said can
  /// still be read; the next message starts a fresh process with no memory
  /// of it.
  func stop() {
    guard isRunning else { return }
    let wasWorking = isWorking || pendingApproval != nil
    end()
    if wasWorking { items.append(Item(entry: .notice("Stopped.", isError: false))) }
  }

  /// Ends the thread and clears the transcript.
  func newThread() {
    end()
    items = []
  }

  /// For `applicationWillTerminate`: ends the process and returns only once
  /// it has gone, because nothing will be left running to escalate
  /// afterwards. Blocks the caller for at most the grace period.
  func shutdown() {
    guard let process else { return }
    stoppedOnPurpose = true
    try? input?.close()
    self.process = nil
    input = nil
    guard process.isRunning else { return }
    process.terminate()
    let deadline = Date().addingTimeInterval(Self.terminationGracePeriod)
    while process.isRunning, Date() < deadline {
      Thread.sleep(forTimeInterval: 0.05)
    }
    if process.isRunning {
      kill(process.processIdentifier, SIGKILL)
    }
  }

  // MARK: - The process

  /// Launches the agent: a new thread's, on the agent chosen now, or the next
  /// turn of a Codex thread, on the agent it started with. Returns false, with
  /// a notice saying why, when it cannot — except for a missing binary, which
  /// the panel's setup state already says in its place, with the paths it
  /// tried and a field for one.
  private func start() -> Bool {
    let continuing = wire == .codex && codexThreadID != nil
    let client = continuing ? self.client : clientSetting
    let wire = continuing ? self.wire : wire(for: client)
    guard let executable = executablePath(for: client) else { return false }
    guard let helper = Self.mcpHelperPath() else {
      items.append(
        Item(
          entry: .notice(
            "Takt's MCP server is missing from this build (Contents/Helpers/takt), so the "
              + "assistant would have no tools. Build without TAKT_SKIP_CLI_BUNDLE, or install the "
              + "CLI with scripts/install_cli.sh.",
            isError: true)))
      return false
    }

    // A write to a pipe whose reader has died raises SIGPIPE, which would
    // take the app down with the child. Ignored, the write fails instead and
    // `write(_:)` reports it.
    signal(SIGPIPE, SIG_IGN)

    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    let asks = continuing ? asksBeforeChanges : asksBeforeChangesSetting
    let systemPrompt = AgentSystemPrompt.text(
      today: .now, asksBeforeChanges: asks, canChange: wire.canAskBeforeChanges || !asks)
    var arguments = client.leadingArguments(userPath: userExecutablePaths[client])
    switch wire {
    case .claudeCode:
      arguments += AgentInvocation.arguments(
        helperPath: helper, systemPrompt: systemPrompt, asksBeforeChanges: asks)
      process.currentDirectoryURL = Self.workingDirectory()
    case .codex:
      arguments += AgentCodexInvocation.arguments(
        helperPath: helper, systemPrompt: systemPrompt, resumeThreadID: codexThreadID, asksBeforeChanges: asks)
      process.currentDirectoryURL = Self.workingDirectory()
    case .antigravity:
      do {
        process.currentDirectoryURL = try Self.antigravityDirectory(helperPath: helper, systemPrompt: systemPrompt)
      } catch {
        items.append(
          Item(entry: .notice("Could not set up \(client.displayName): \(error.localizedDescription)", isError: true)))
        return false
      }
      arguments += AgentAntigravityInvocation.arguments(asksBeforeChanges: asks)
      antigravityParser = AgentAntigravityStreamParser()
    }
    process.arguments = arguments
    process.environment = Self.environment()

    let stdin = Pipe()
    let stdout = Pipe()
    let stderr = Pipe()
    process.standardInput = stdin
    process.standardOutput = stdout
    process.standardError = stderr

    generation += 1
    let generation = generation
    let lines = AgentLineBuffer()
    stdout.fileHandleForReading.readabilityHandler = { [weak self] handle in
      let chunk = handle.availableData
      if chunk.isEmpty {
        handle.readabilityHandler = nil
        return
      }
      for line in lines.append(chunk) {
        // The main queue, not a `Task`: it is FIFO, and the order of these
        // lines is the order of the conversation.
        DispatchQueue.main.async {
          MainActor.assumeIsolated { self?.receive(line, generation: generation) }
        }
      }
    }
    let tail = AgentOutputTail()
    stderrTail = tail
    stderr.fileHandleForReading.readabilityHandler = { handle in
      let chunk = handle.availableData
      if chunk.isEmpty { handle.readabilityHandler = nil } else { tail.append(chunk) }
    }
    process.terminationHandler = { [weak self] finished in
      let status = finished.terminationStatus
      DispatchQueue.main.async {
        MainActor.assumeIsolated { self?.processExited(status: status, generation: generation) }
      }
    }

    do {
      try process.run()
    } catch {
      Self.log.error("Could not launch \(client.displayName, privacy: .public): \(error.localizedDescription, privacy: .public)")
      items.append(
        Item(entry: .notice("Could not start \(client.displayName): \(error.localizedDescription)", isError: true)))
      return false
    }
    Self.log.info("Started an agent \(continuing ? "turn" : "thread", privacy: .public) with \(executable, privacy: .public)")
    self.process = process
    input = stdin.fileHandleForWriting
    if !continuing {
      self.client = client
      self.wire = wire
      ledger = AgentContextLedger()
      asksBeforeChanges = asks
      undoneSinceLastMessage = []
    }
    // Codex numbers a turn's items from zero again, so the last turn's ids
    // would match this one's.
    if wire == .codex { toolItems = [:] }
    stoppedOnPurpose = false
    return true
  }

  /// Ends the process and withdraws anything still waiting on it.
  private func end() {
    stoppedOnPurpose = true
    try? input?.close()
    if let process { Self.terminate(process) }
    process = nil
    input = nil
    codexThreadID = nil
    isWorking = false
    toolItems = [:]
    withdrawPendingApprovals()
  }

  /// How long a process gets to leave on SIGTERM before it is killed.
  private static let terminationGracePeriod: TimeInterval = 2

  /// SIGTERM, then SIGKILL if the process is still there after the grace
  /// period. The wait is off the main thread: Stop is a click, and a CLI
  /// that ignores SIGTERM must not make the click hang.
  private static func terminate(_ process: Process) {
    guard process.isRunning else { return }
    process.terminate()
    DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + terminationGracePeriod) {
      guard process.isRunning else { return }
      log.warning("The agent ignored SIGTERM for \(terminationGracePeriod)s; sending SIGKILL.")
      kill(process.processIdentifier, SIGKILL)
    }
  }

  private func processExited(status: Int32, generation: Int) {
    guard generation == self.generation, process != nil else { return }
    process = nil
    input = nil
    isWorking = false
    withdrawPendingApprovals()
    guard !stoppedOnPurpose else { return }
    let detail = stderrTail.text.trimmingCharacters(in: .whitespacesAndNewlines)
    let name = client.displayName
    Self.log.error("\(name, privacy: .public) exited with \(status); stderr: \(detail, privacy: .public)")
    items.append(
      Item(
        entry: .notice(
          detail.isEmpty ? "\(name) stopped (exit \(status))." : "\(name) stopped: \(detail)",
          isError: true)))
    adviseOn(detail)
  }

  /// The fix, under a failure it is known for: the CLI says "Please run
  /// /login", and the panel has no terminal to run it in.
  private func adviseOn(_ failure: String) {
    guard let advice = AgentFailureAdvice.advice(for: failure, client: client) else { return }
    items.append(Item(entry: .notice(advice, isError: false)))
  }

  private func withdrawPendingApprovals() {
    for index in items.indices {
      if case .write(var approval) = items[index].entry, approval.state == .pending {
        approval.state = .withdrawn
        items[index].entry = .write(approval)
      }
    }
  }

  @discardableResult
  private func write(_ line: String) -> Bool {
    guard let input else { return false }
    do {
      try input.write(contentsOf: Data(line.utf8))
      return true
    } catch {
      Self.log.error("Could not write to the agent: \(error.localizedDescription, privacy: .public)")
      return false
    }
  }

  private func answer(_ itemID: UUID, _ reply: (Approval) -> ApprovalState?) {
    guard let index = items.firstIndex(where: { $0.id == itemID }),
      case .write(var approval) = items[index].entry, approval.state == .pending
    else { return }
    guard process != nil else {
      approval.state = .withdrawn
      items[index].entry = .write(approval)
      return
    }
    guard let state = reply(approval) else { return }
    approval.state = state
    items[index].entry = .write(approval)
  }

  // MARK: - What the CLI says

  private func receive(_ line: String, generation: Int) {
    guard generation == self.generation else { return }
    let events =
      switch wire {
      case .claudeCode: AgentStreamParser.events(fromLine: line)
      case .codex: AgentCodexStreamParser.events(fromLine: line)
      case .antigravity: antigravityParser.events(fromLine: line)
      }
    for event in events {
      handle(event)
    }
  }

  private func handle(_ event: AgentStreamEvent) {
    switch event {
    case .started(let session, let model, _):
      Self.log.info("Agent session \(session, privacy: .public) on \(model ?? "the default model", privacy: .public)")
      if wire == .codex { codexThreadID = session }

    case .text(let text):
      items.append(Item(entry: .assistant(text)))

    case .toolUse(let use):
      // Drawn once: Codex repeats a call with its result.
      guard toolItems[use.id] == nil else { return }
      // Reads get a line of their own. A pre-allowed write gets its card now,
      // with what it is about to replace read before it runs. Any other
      // write's card is drawn by its permission request, which follows.
      if AgentToolPolicy.isReadOnly(use.name) {
        let item = Item(entry: .read(use, failed: nil))
        toolItems[use.id] = item.id
        items.append(item)
      } else if AgentToolPolicy.isPreAllowed(use.name, asksBeforeChanges: asksBeforeChanges) {
        let request = AgentPermissionRequest(requestID: "", toolName: use.name, input: use.input, toolUseID: use.id)
        let before = currentValues?(use.name, use.input) ?? [:]
        let item = Item(entry: .write(Approval(request: request, state: .automatic, before: before)))
        toolItems[use.id] = item.id
        items.append(item)
      }

    case .toolResult(let toolUseID, let isError, let text):
      guard let itemID = toolItems[toolUseID], let index = items.firstIndex(where: { $0.id == itemID }) else {
        return
      }
      switch items[index].entry {
      case .read(let use, _):
        items[index].entry = .read(use, failed: isError)
      case .write(var approval) where approval.state == .approved || approval.state == .automatic:
        approval.outcome = isError ? .failed(text) : .applied
        if !isError { approval.undoStep = AgentToolResult.undoStep(fromText: text) }
        items[index].entry = .write(approval)
      default:
        break
      }

    case .permissionRequest(let request):
      switch AgentToolPolicy.decision(forPermissionRequestOn: request.toolName) {
      case .refuse:
        write(
          AgentStreamEncoder.deny(
            requestID: request.requestID, message: "Only Takt's own tools are available here."))
        items.append(Item(entry: .notice("Refused \(request.toolName): not one of Takt's tools.", isError: true)))
      case .askUser:
        // A write already drawn as pre-allowed that asks anyway waits on its
        // own card rather than gaining a second one.
        if let toolUseID = request.toolUseID, let itemID = toolItems[toolUseID],
          let index = items.firstIndex(where: { $0.id == itemID }),
          case .write(var approval) = items[index].entry, approval.state == .automatic
        {
          approval = Approval(request: request, state: .pending, before: approval.before)
          items[index].entry = .write(approval)
          return
        }
        let before = currentValues?(request.toolName, request.input) ?? [:]
        let item = Item(entry: .write(Approval(request: request, state: .pending, before: before)))
        if let toolUseID = request.toolUseID { toolItems[toolUseID] = item.id }
        items.append(item)
      }

    case .unsupportedControlRequest(let requestID, let subtype):
      write(AgentStreamEncoder.error(requestID: requestID, message: "Takt does not handle \(subtype)."))

    case .controlRequestCancelled(let requestID):
      for index in items.indices {
        if case .write(var approval) = items[index].entry, approval.state == .pending,
          approval.request.requestID == requestID
        {
          approval.state = .withdrawn
          items[index].entry = .write(approval)
        }
      }

    case .turnFinished(let result):
      isWorking = false
      finishedTurns += 1
      if result.isError {
        let detail = result.text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        items.append(
          Item(entry: .notice(detail.isEmpty ? "The turn ended with \(result.subtype)." : detail, isError: true)))
        adviseOn(detail)
      }
      // A Codex turn's process exits by itself now; the thread lives on in
      // its id, and the next message starts the next turn's.
      if wire == .codex {
        process = nil
        input = nil
      }
      onTurnFinished?()
      if endsAfterTurn { end() }
    }
  }

  // MARK: - Where things are

  /// The bundled MCP helper, found the way `Priority --mcp-server` finds it.
  private static func mcpHelperPath() -> String? {
    let candidates = MCPHelperLocator.candidates(
      environmentOverride: ProcessInfo.processInfo.environment["PRIORITY_MCP_EXECUTABLE_PATH"],
      bundlePath: Bundle.main.bundlePath,
      homeDirectory: NSHomeDirectory())
    return MCPHelperLocator.resolve(candidates: candidates, isExecutable: FileManager.default.isExecutableFile)
  }

  /// An empty directory of the app's own, so no project's `CLAUDE.md` or
  /// `.mcp.json` is picked up from wherever the app happened to launch.
  private static func workingDirectory() -> URL {
    let directory = AppIdentity.applicationSupportDirectory()
      .appendingPathComponent("Agent", isDirectory: true)
    try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory
  }

  /// `agy`'s working directory, with the two files that give it Takt's
  /// server and the panel's agent (`AgentAntigravityInvocation`), rewritten
  /// for this thread: the system prompt carries today's date and whether it
  /// may change things. A directory of its own, beside Claude Code's, so the
  /// `.agents` folder is never where another agent looks.
  private static func antigravityDirectory(helperPath: String, systemPrompt: String) throws -> URL {
    let directory = workingDirectory().appendingPathComponent("Antigravity", isDirectory: true)
    let agents = directory.appendingPathComponent(".agents/agents", isDirectory: true)
    try FileManager.default.createDirectory(at: agents, withIntermediateDirectories: true)
    try AgentAntigravityInvocation.mcpConfig(helperPath: helperPath)
      .write(to: directory.appendingPathComponent(".agents/mcp_config.json"), atomically: true, encoding: .utf8)
    try AgentAntigravityInvocation.agentFile(systemPrompt: systemPrompt)
      .write(
        to: agents.appendingPathComponent("\(AgentAntigravityInvocation.agentName).md"), atomically: true,
        encoding: .utf8)
    return directory
  }

  /// The app's environment with a usable `PATH` — a GUI app's is the bare
  /// system one — and without the markers that tell Claude Code it is
  /// running inside another Claude Code session, which it would be if the
  /// app was launched from one.
  private static func environment() -> [String: String] {
    var environment = ProcessInfo.processInfo.environment
    let home = NSHomeDirectory()
    environment["PATH"] = [
      "\(home)/.local/bin", "/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin",
    ].joined(separator: ":")
    for key in ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SSE_PORT"] {
      environment[key] = nil
    }
    return environment
  }
}

/// Splits a byte stream into lines. Touched only from the pipe's reader, one
/// chunk at a time, but locked so that is not something to take on trust.
private final class AgentLineBuffer: @unchecked Sendable {
  private let lock = NSLock()
  private var pending = Data()

  func append(_ chunk: Data) -> [String] {
    lock.lock()
    defer { lock.unlock() }
    pending.append(chunk)
    var lines: [String] = []
    while let newline = pending.firstIndex(of: 0x0A) {
      let line = pending[pending.startIndex..<newline]
      pending.removeSubrange(pending.startIndex...newline)
      if let text = String(bytes: line, encoding: .utf8) { lines.append(text) }
    }
    return lines
  }
}

/// The last few kilobytes of stderr, to say why a process died.
private final class AgentOutputTail: @unchecked Sendable {
  private let lock = NSLock()
  private var data = Data()

  func append(_ chunk: Data) {
    lock.lock()
    data.append(chunk)
    if data.count > 4096 { data.removeFirst(data.count - 4096) }
    lock.unlock()
  }

  var text: String {
    lock.lock()
    defer { lock.unlock() }
    return String(bytes: data, encoding: .utf8) ?? ""
  }
}
