//! The agent panel: the user's own coding agent, headless, with Takt's MCP
//! server as its only tool. `WorkspaceAgentSession.swift`, `AgentInvocation`
//! and `AgentCodexInvocation`, with the same command lines.
//!
//! Claude Code runs as one `claude -p` process per thread in stream-json
//! both ways, so a follow-up is one more line on its stdin. Codex runs a
//! process per turn (`codex exec --json`), each resuming the thread by id.
//! A reader thread turns the process's stdout into [`Event`]s on a channel
//! the window drains. Writes are pre-allowed and apply at once; each is the
//! MCP server's own undo step, so ⌘Z (or a card's Undo) takes it back.

use super::App;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

pub const SERVER: &str = "takt";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    ClaudeCode,
    Codex,
}

impl Client {
    pub fn from_pref(raw: &str) -> Self {
        if raw == "codex" {
            Client::Codex
        } else {
            Client::ClaudeCode
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Client::ClaudeCode => "Claude Code",
            Client::Codex => "Codex",
        }
    }

    fn candidates(self, home: &str) -> Vec<String> {
        match self {
            Client::ClaudeCode => vec![
                format!("{home}/.local/bin/claude"),
                format!("{home}/.claude/local/claude"),
                "/opt/homebrew/bin/claude".into(),
                "/usr/local/bin/claude".into(),
            ],
            Client::Codex => vec![
                "/opt/homebrew/bin/codex".into(),
                "/usr/local/bin/codex".into(),
                format!("{home}/.local/bin/codex"),
            ],
        }
    }

    pub fn executable(self) -> Option<String> {
        let home = std::env::var("HOME").unwrap_or_default();
        self.candidates(&home)
            .into_iter()
            .find(|path| std::path::Path::new(path).is_file())
    }
}

/// What the panel draws.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    User(String),
    Assistant(String),
    /// A call of one of Takt's tools: its name, a line of its input, and
    /// whether it failed once its result came.
    Tool {
        id: String,
        name: String,
        summary: String,
        write: bool,
        failed: Option<bool>,
    },
    Notice {
        text: String,
        error: bool,
    },
}

/// A line of the agent's output, read.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Started(String),
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        id: String,
        failed: bool,
    },
    TurnFinished {
        error: Option<String>,
    },
    Exited,
}

#[derive(Default)]
pub struct Agent {
    pub entries: Vec<Entry>,
    pub working: bool,
    /// The client the thread started on.
    pub client: Option<Client>,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    pub events: Option<Receiver<Event>>,
    /// Codex's thread, resumed by the next turn.
    pub codex_thread: Option<String>,
    /// Instructions were sent on this thread already.
    pub sent_instructions: bool,
}

impl Drop for Agent {
    fn drop(&mut self) {
        self.end();
    }
}

impl Agent {
    /// Stops the process; the transcript stays.
    pub fn end(&mut self) {
        self.stdin = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.events = None;
        self.working = false;
    }

    /// A new thread: the old one's process ends and its transcript goes.
    pub fn reset(&mut self) {
        self.end();
        self.entries.clear();
        self.codex_thread = None;
        self.client = None;
        self.sent_instructions = false;
    }

    /// Takes what the process has said since the last drain. True when
    /// anything changed.
    pub fn drain(&mut self) -> bool {
        let Some(events) = &self.events else {
            return false;
        };
        let mut changed = false;
        let mut exited = false;
        while let Ok(event) = events.try_recv() {
            changed = true;
            match event {
                Event::Started(thread) => {
                    if self.client == Some(Client::Codex) {
                        self.codex_thread = Some(thread);
                    }
                }
                Event::Text(text) => self.entries.push(Entry::Assistant(text)),
                Event::ToolUse { id, name, input } => {
                    if self
                        .entries
                        .iter()
                        .any(|e| matches!(e, Entry::Tool { id: other, .. } if *other == id))
                    {
                        continue;
                    }
                    let bare = name
                        .strip_prefix(&format!("mcp__{SERVER}__"))
                        .unwrap_or(&name)
                        .to_string();
                    let write = takt_core::agent_tools::agent_write_tools().contains(&bare);
                    self.entries.push(Entry::Tool {
                        id,
                        name: bare,
                        summary: summarise(&input),
                        write,
                        failed: None,
                    });
                }
                Event::ToolResult { id, failed } => {
                    for entry in &mut self.entries {
                        if let Entry::Tool {
                            id: other,
                            failed: slot,
                            ..
                        } = entry
                            && *other == id
                        {
                            *slot = Some(failed);
                        }
                    }
                }
                Event::TurnFinished { error } => {
                    self.working = false;
                    if let Some(text) = error {
                        self.entries.push(Entry::Notice { text, error: true });
                    }
                }
                Event::Exited => exited = true,
            }
        }
        if exited {
            self.working = false;
            self.stdin = None;
            if let Some(mut child) = self.child.take() {
                let _ = child.wait();
            }
            self.events = None;
        }
        changed
    }
}

/// One line of a tool call's input, for its card.
fn summarise(input: &Value) -> String {
    let Some(map) = input.as_object() else {
        return String::new();
    };
    map.iter()
        .filter(|(key, _)| !key.ends_with("_id") && *key != "id")
        .map(|(key, value)| {
            let text = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            format!("{key}: {}", text.chars().take(60).collect::<String>())
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Claude Code's stream-json lines, read.
pub fn claude_events(line: &str) -> Vec<Event> {
    let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    match value["type"].as_str() {
        Some("system") if value["subtype"] == "init" => {
            vec![Event::Started(
                value["session_id"].as_str().unwrap_or("").into(),
            )]
        }
        Some("assistant") => value["message"]["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|block| match block["type"].as_str() {
                        Some("text") => block["text"]
                            .as_str()
                            .filter(|t| !t.trim().is_empty())
                            .map(|t| Event::Text(t.into())),
                        Some("tool_use") => Some(Event::ToolUse {
                            id: block["id"].as_str().unwrap_or("").into(),
                            name: block["name"].as_str().unwrap_or("").into(),
                            input: block["input"].clone(),
                        }),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Some("user") => value["message"]["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b["type"] == "tool_result")
                    .map(|b| Event::ToolResult {
                        id: b["tool_use_id"].as_str().unwrap_or("").into(),
                        failed: b["is_error"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Some("result") => vec![Event::TurnFinished {
            error: value["is_error"].as_bool().unwrap_or(false).then(|| {
                value["result"]
                    .as_str()
                    .unwrap_or("The turn failed.")
                    .to_string()
            }),
        }],
        _ => Vec::new(),
    }
}

/// Codex's `exec --json` lines, read.
pub fn codex_events(line: &str) -> Vec<Event> {
    let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    match value["type"].as_str() {
        Some("thread.started") => value["thread_id"]
            .as_str()
            .map(|t| vec![Event::Started(t.into())])
            .unwrap_or_default(),
        Some(kind @ ("item.started" | "item.completed")) => {
            let item = &value["item"];
            let id = item["id"].as_str().unwrap_or("").to_string();
            match item["type"].as_str() {
                Some("agent_message") if kind == "item.completed" => item["text"]
                    .as_str()
                    .filter(|t| !t.trim().is_empty())
                    .map(|t| vec![Event::Text(t.into())])
                    .unwrap_or_default(),
                Some("mcp_tool_call") => {
                    let name = format!(
                        "mcp__{}__{}",
                        item["server"].as_str().unwrap_or(""),
                        item["tool"].as_str().unwrap_or("")
                    );
                    let mut events = vec![Event::ToolUse {
                        id: id.clone(),
                        name,
                        input: item["arguments"].clone(),
                    }];
                    if kind == "item.completed" {
                        let failed = !item["error"]["message"].is_null()
                            || item["status"] == "failed"
                            || item["result"]["is_error"] == true;
                        events.push(Event::ToolResult { id, failed });
                    }
                    events
                }
                _ => Vec::new(),
            }
        }
        Some("turn.completed") => vec![Event::TurnFinished { error: None }],
        Some("turn.failed") => vec![Event::TurnFinished {
            error: Some(
                value["error"]["message"]
                    .as_str()
                    .unwrap_or("The turn failed.")
                    .into(),
            ),
        }],
        _ => Vec::new(),
    }
}

/// The bundled `takt`, or the installed one: the MCP server.
pub fn helper_path() -> Option<String> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(contents) = exe.parent().and_then(|p| p.parent())
    {
        candidates.push(contents.join("Helpers/takt"));
    }
    let home = std::env::var("HOME").unwrap_or_default();
    candidates.push(format!("{home}/.local/bin/takt").into());
    candidates.push("/Applications/Takt.app/Contents/Helpers/takt".into());
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

fn pre_allowed() -> Vec<String> {
    takt_core::agent_tools::agent_read_only_tools()
        .into_iter()
        .chain(takt_core::agent_tools::agent_write_tools())
        .collect()
}

pub fn system_prompt() -> String {
    let today = chrono::Local::now();
    format!(
        "You are the assistant inside Takt, a keyboard-first macOS task app. You help the user \
understand, plan and organise their tasks. Today is {} ({}). The tools take dates only as ISO \
(2026-10-17, or 2026-10-17 17:00 for a time), so turn a phrase like \"friday\" or \"next week\" \
into one from today yourself: \"friday\" is the next Friday after today.\n\n\
You have Takt's own tools and nothing else: no shell, no files, no web. The workspace_* tools \
read and edit the app's own lists and tasks, which are the source of truth; ids there are \
uppercase UUIDs. The task_* tools reach Checkvist, an optional integration the user may not \
have set up. Use the read tools freely to answer questions.\n\n\
Your changes — adding, editing, completing, moving or deleting anything — apply straight away, \
and each is shown to the user as a card with an Undo button. So before calling a tool that \
writes, say in one sentence what you are about to change, then call it with exactly that \
change. Make one change per call, so each can be undone on its own, and only make the changes \
the user asked for or agreed to. If they undo one, do not make it again; ask what they would \
like instead.\n\n\
Each change card also shows the keys that would have made the same change in the app, worked \
out by the app from your call and the user's own key bindings. So do not spell out keyboard \
shortcuts for your changes yourself.\n\n\
A message may begin with bracketed notes from the app rather than the user: what is on \
screen, and the user's standing instructions for their account, a folder or a list. \
Instructions are sent again only when they change; the latest replace any before them.\n\n\
You keep those standing instructions. When the user states a durable preference, fact or \
convention — about a list, a folder, or how they work — save it with agent_instructions_set, \
as a change they see like any other, in the narrowest scope that fits. Read the scope with \
agent_instructions_get first, then edit or merge into what is there. Never save a one-off \
request as a standing instruction.\n\n\
Reply in short, plain Markdown. Refer to tasks and lists by their titles, not their ids.",
        today.format("%A %-d %B %Y"),
        today.format("%Y-%m-%d"),
    )
}

fn claude_arguments(helper: &str) -> Vec<String> {
    let config = json!({
        "mcpServers": { SERVER: { "type": "stdio", "command": helper, "args": ["--mcp-server"] } }
    })
    .to_string();
    let allowed = pre_allowed()
        .into_iter()
        .map(|t| format!("mcp__{SERVER}__{t}"))
        .collect::<Vec<_>>()
        .join(",");
    [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--mcp-config",
        &config,
        "--strict-mcp-config",
        "--tools",
        "",
        "--allowedTools",
        &allowed,
        "--permission-mode",
        "manual",
        "--permission-prompt-tool",
        "stdio",
        "--setting-sources",
        "",
        "--no-session-persistence",
        "--system-prompt",
        &system_prompt(),
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn toml(text: &str) -> String {
    Value::String(text.into()).to_string()
}

fn codex_arguments(helper: &str, resume: Option<&str>) -> Vec<String> {
    let mut arguments: Vec<String> = vec!["exec".into()];
    if let Some(thread) = resume {
        arguments.extend(["resume".into(), thread.into()]);
    }
    arguments.extend(
        [
            "--json",
            "--skip-git-repo-check",
            "--ignore-user-config",
            "--ignore-rules",
        ]
        .into_iter()
        .map(String::from),
    );
    let server = format!("mcp_servers.{SERVER}");
    let tools = pre_allowed()
        .iter()
        .map(|t| toml(t))
        .collect::<Vec<_>>()
        .join(",");
    let mut config = vec![
        "sandbox_mode=\"read-only\"".to_string(),
        "web_search=\"disabled\"".into(),
        format!("developer_instructions={}", toml(&system_prompt())),
        format!("{server}.command={}", toml(helper)),
        format!("{server}.args=[{}]", toml("--mcp-server")),
        format!("{server}.enabled_tools=[{tools}]"),
        format!("{server}.default_tools_approval_mode=\"approve\""),
    ];
    for feature in [
        "shell_tool",
        "unified_exec",
        "view_image",
        "browser_use",
        "computer_use",
        "apps",
        "goals",
    ] {
        config.push(format!("features.{feature}=false"));
    }
    for entry in config {
        arguments.extend(["-c".into(), entry]);
    }
    arguments.push("-".into());
    arguments
}

fn spawn_reader(child: &mut Child, parse: fn(&str) -> Vec<Event>) -> Receiver<Event> {
    let (sender, receiver): (Sender<Event>, Receiver<Event>) = channel();
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                for event in parse(&line) {
                    if sender.send(event).is_err() {
                        return;
                    }
                }
            }
            let _ = sender.send(Event::Exited);
        });
    }
    receiver
}

impl App {
    /// What is on screen, as the bracketed note ahead of a message, and the
    /// standing instructions the first time a thread speaks.
    fn agent_context(&mut self) -> String {
        let mut notes = vec![format!(
            "[On screen: {} in {}",
            self.scope_title(),
            self.view_mode.title()
        )];
        if let Some(task) = self.selected().cloned() {
            notes[0].push_str(&format!(
                ", selected task \u{201c}{}\u{201d} ({})",
                task.title, task.id
            ));
        }
        notes[0].push(']');
        if !self.agent.sent_instructions {
            self.agent.sent_instructions = true;
            for instruction in self.store.core.agent_instructions().unwrap_or_default() {
                if !instruction.text.trim().is_empty() {
                    let scope = instruction
                        .name
                        .clone()
                        .unwrap_or_else(|| "the account".into());
                    notes.push(format!(
                        "[Standing instructions for {scope}: {}]",
                        instruction.text
                    ));
                }
            }
        }
        notes.join("\n")
    }

    /// Sends a message on the thread, starting its process when it has none.
    pub fn send_to_agent(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let client = self
            .agent
            .client
            .unwrap_or_else(|| Client::from_pref(&self.prefs.agent_client));
        let Some(executable) = client.executable() else {
            self.agent.entries.push(Entry::Notice {
                text: format!("{} isn't installed where Takt looks for it.", client.name()),
                error: true,
            });
            return;
        };
        let Some(helper) = helper_path() else {
            self.agent.entries.push(Entry::Notice {
                text:
                    "Takt's MCP server (the takt helper) wasn't found. Run scripts/install_cli.sh."
                        .into(),
                error: true,
            });
            return;
        };
        let context = self.agent_context();
        let message = format!("{context}\n\n{text}");
        self.agent.entries.push(Entry::User(text.into()));
        self.agent.client = Some(client);
        self.agent.working = true;
        let started = match client {
            Client::ClaudeCode => if self.agent.child.is_none() {
                self.start_claude(&executable, &helper)
            } else {
                Ok(())
            }
            .and_then(|()| {
                let line = json!({
                    "type": "user",
                    "message": { "role": "user", "content": [{ "type": "text", "text": message }] }
                })
                .to_string();
                let stdin = self
                    .agent
                    .stdin
                    .as_mut()
                    .ok_or("The agent isn't running.")?;
                writeln!(stdin, "{line}").map_err(|e| e.to_string())?;
                stdin.flush().map_err(|e| e.to_string())
            }),
            Client::Codex => self.start_codex_turn(&executable, &helper, &message),
        };
        if let Err(error) = started {
            self.agent.working = false;
            self.agent.end();
            self.agent.entries.push(Entry::Notice {
                text: error,
                error: true,
            });
        }
    }

    fn start_claude(&mut self, executable: &str, helper: &str) -> Result<(), String> {
        let mut child = Command::new(executable)
            .args(claude_arguments(helper))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Couldn't start Claude Code: {e}"))?;
        self.agent.events = Some(spawn_reader(&mut child, claude_events));
        self.agent.stdin = child.stdin.take();
        self.agent.child = Some(child);
        Ok(())
    }

    fn start_codex_turn(
        &mut self,
        executable: &str,
        helper: &str,
        message: &str,
    ) -> Result<(), String> {
        let mut child = Command::new(executable)
            .args(codex_arguments(helper, self.agent.codex_thread.as_deref()))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Couldn't start Codex: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(message.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        self.agent.events = Some(spawn_reader(&mut child, codex_events));
        self.agent.child = Some(child);
        Ok(())
    }

    /// After the agent wrote: what is held is read again, as for any other
    /// process's write.
    pub fn drain_agent(&mut self) -> bool {
        let had_writes = |agent: &super::agent::Agent| {
            agent
                .entries
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        Entry::Tool {
                            write: true,
                            failed: Some(false),
                            ..
                        }
                    )
                })
                .count()
        };
        let before = had_writes(&self.agent) + had_writes(&self.quick_agent);
        let mut changed = self.agent.drain();
        if self.quick_agent.working || self.quick_agent.events.is_some() {
            changed |= self.quick_agent.drain();
            // A quick ask is one turn: its process ends with it.
            if !self.quick_agent.working {
                self.quick_agent.end();
            }
        }
        if had_writes(&self.agent) + had_writes(&self.quick_agent) != before {
            self.check_external_writes();
        }
        changed
    }

    /// Whether either thread is still at work.
    pub fn agents_working(&self) -> bool {
        self.agent.working || self.quick_agent.working
    }

    /// `aa`: a one-line ask about the selected task, on a thread of its own
    /// so it neither waits behind the panel's turn nor joins its memory.
    /// While one is under way it reopens on that task.
    pub fn open_quick_ask(&mut self) {
        if !self.quick_agent.working {
            let Some(task) = self.selected_task.clone() else {
                return;
            };
            if self.quick_task.as_ref() != Some(&task) {
                self.quick_agent.reset();
            }
            self.quick_task = Some(task);
        }
        self.ask(super::ask::Ask::Agent);
    }

    /// Sends `text` about the quick-ask task, in a fresh process.
    pub fn send_quick_ask(&mut self, text: &str) {
        let Some(task_id) = self.quick_task.clone() else {
            return;
        };
        if text.trim().is_empty() || self.quick_agent.working {
            return;
        }
        let title = self
            .task(&task_id)
            .map(|t| t.title.clone())
            .unwrap_or_default();
        self.selected_task = Some(task_id);
        std::mem::swap(&mut self.agent, &mut self.quick_agent);
        self.agent.end();
        self.agent.entries.clear();
        self.agent.codex_thread = None;
        self.agent.sent_instructions = false;
        self.send_to_agent(&format!("About the task \u{201c}{title}\u{201d}: {text}"));
        std::mem::swap(&mut self.agent, &mut self.quick_agent);
    }

    /// The last quick ask, as the sheet shows it under the field.
    pub fn quick_ask_transcript(&self) -> String {
        let mut lines = Vec::new();
        for entry in &self.quick_agent.entries {
            match entry {
                Entry::User(text) => lines.push(format!("› {text}")),
                Entry::Assistant(text) => lines.push(text.clone()),
                Entry::Tool {
                    name,
                    summary,
                    failed,
                    ..
                } => lines.push(format!(
                    "{} {name} {summary}",
                    match failed {
                        Some(true) => "✕",
                        Some(false) => "✓",
                        None => "…",
                    }
                )),
                Entry::Notice { text, .. } => lines.push(text.clone()),
            }
        }
        if self.quick_agent.working {
            lines.push("Working…".into());
        }
        if lines.is_empty() {
            return "Ask the agent to change or explain this task. It runs apart from the panel; ⌘Z undoes what it did.".into();
        }
        lines.join("\n")
    }

    pub fn toggle_agent_panel(&mut self) {
        let showing = self.prefs.sidebar_visible && self.prefs.left_dock_tab == "agent";
        if showing && self.region == super::Region::Agent {
            self.prefs.left_dock_tab = "lists".into();
            self.region = super::Region::Tasks;
        } else {
            self.prefs.sidebar_visible = true;
            self.prefs.left_dock_tab = "agent".into();
            self.region = super::Region::Agent;
            self.editing = Some(super::Editing::AgentMessage);
        }
        self.prefs.save();
    }

    pub fn use_agent(&mut self, client: Client) {
        self.prefs.agent_client = match client {
            Client::ClaudeCode => "claude".into(),
            Client::Codex => "codex".into(),
        };
        self.prefs.save();
        self.message = Some(format!(
            "New threads use {}{}.",
            client.name(),
            if client.executable().is_none() {
                " (not found)"
            } else {
                ""
            }
        ));
    }

    /// `taskAskAgent`: the panel, with the task named in the draft.
    /// The panel, with the selected task named in the draft.
    #[allow(dead_code)]
    pub fn ask_agent_about_selected(&mut self) {
        let Some(task) = self.selected().cloned() else {
            return;
        };
        if !(self.prefs.sidebar_visible && self.prefs.left_dock_tab == "agent") {
            self.toggle_agent_panel();
        }
        self.region = super::Region::Agent;
        self.editing = Some(super::Editing::AgentMessage);
        self.agent_draft = Some(format!("About \u{201c}{}\u{201d}: ", task.title));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_lines_become_events() {
        let text = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done."},{"type":"tool_use","id":"t1","name":"mcp__takt__workspace_task_create","input":{"title":"Milk"}}]}}"#;
        let events = claude_events(text);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], Event::Text("Done.".into()));
        let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"ok"}"#;
        assert_eq!(
            claude_events(result),
            vec![Event::TurnFinished { error: None }]
        );
    }

    #[test]
    fn codex_lines_become_events() {
        let line = r#"{"type":"item.completed","item":{"id":"i1","type":"mcp_tool_call","server":"takt","tool":"workspace_lists","arguments":{},"status":"completed"}}"#;
        let events = codex_events(line);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[1], Event::ToolResult { failed: false, .. }));
        assert_eq!(
            codex_events(r#"{"type":"thread.started","thread_id":"abc"}"#),
            vec![Event::Started("abc".into())]
        );
    }

    #[test]
    fn codex_turns_resume_their_thread() {
        let args = codex_arguments("/x/takt", Some("abc"));
        assert_eq!(&args[..3], &["exec", "resume", "abc"]);
        assert_eq!(args.last().map(String::as_str), Some("-"));
    }
}
