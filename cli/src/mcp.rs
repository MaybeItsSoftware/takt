//! Takt's MCP stdio server.
//!
//! This is the only one. There used to be three — a Swift server embedded in
//! the app and a Python fallback script alongside this — and holding them
//! equal from the outside cost more than it bought, since none could import
//! another.
//! The app ships this binary instead (`Takt.app/Contents/Helpers/takt`)
//! and `Takt --mcp-server` hands the process over to it, which is why the
//! bare flag is accepted in `main.rs` and why the environment outranks the
//! config file in `config.rs`: configurations written for the old server keep
//! working untouched.
//!
//! Exposing the tools here is a dispatch table rather than an implementation —
//! this and `cli.rs` both go through [`crate::tools::Tools::call`].
//! `scripts/mcp_smoke_check.py` checks the handover; `cargo test` checks the
//! behaviour.

use crate::tools::Tools;
use serde_json::{Map, Value, json};
use std::io::{BufRead, BufReader, Read, Stdin, Write};

const JSONRPC_VERSION: &str = "2.0";
const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "takt";
const SERVER_VERSION: &str = "0.3.0";

/// The most a header-framed message may claim to be. The body is allocated up
/// front to the declared size, so without a ceiling one line of input could
/// ask for any amount of memory. 64 MiB is far beyond any real request.
const MAX_CONTENT_LENGTH: usize = 64 * 1024 * 1024;

const JSONRPC_PARSE_ERROR: i64 = -32700;
const JSONRPC_INVALID_REQUEST: i64 = -32600;
const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
const JSONRPC_INVALID_PARAMS: i64 = -32602;

#[derive(Debug)]
struct JsonRpcError {
    code: i64,
    message: String,
}

impl JsonRpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        JsonRpcError {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, PartialEq)]
enum Framing {
    /// The MCP stdio transport: one JSON object per line, no headers.
    Newline,
    /// LSP-style, still accepted for anything already wired up that way.
    ContentLength,
}

pub struct Server {
    tools: Tools,
    reader: BufReader<Stdin>,
    protocol_version: String,
    framing: Framing,
}

impl Server {
    pub fn new(tools: Tools) -> Self {
        Server {
            tools,
            reader: BufReader::new(std::io::stdin()),
            protocol_version: DEFAULT_PROTOCOL_VERSION.into(),
            // The right default before any request has been read; switched if a
            // peer uses headers, and replies mirror whichever arrived.
            framing: Framing::Newline,
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.read_message() {
                Ok(None) => return,
                Ok(Some(message)) => {
                    let id = message.get("id").cloned();
                    if let Err(error) = self.handle(&message)
                        && let Some(id) = id.filter(|id| !id.is_null())
                    {
                        self.send(json!({
                            "jsonrpc": JSONRPC_VERSION,
                            "id": id,
                            "error": { "code": error.code, "message": error.message },
                        }));
                    }
                }
                Err(error) => {
                    // A frame we could not parse has no id to answer against,
                    // so it is reported without one rather than dropped.
                    self.send(json!({
                        "jsonrpc": JSONRPC_VERSION,
                        "id": Value::Null,
                        "error": { "code": error.code, "message": error.message },
                    }));
                }
            }
        }
    }

    fn handle(&mut self, message: &Value) -> Result<(), JsonRpcError> {
        let Some(object) = message.as_object() else {
            return Err(JsonRpcError::new(
                JSONRPC_INVALID_REQUEST,
                "Request must be an object.",
            ));
        };
        if object.get("jsonrpc").and_then(Value::as_str) != Some(JSONRPC_VERSION) {
            return Err(JsonRpcError::new(
                JSONRPC_INVALID_REQUEST,
                "Unsupported JSON-RPC version.",
            ));
        }
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            return Err(JsonRpcError::new(
                JSONRPC_INVALID_REQUEST,
                "Missing method.",
            ));
        };

        let params = object.get("params");
        // A message without an id is a notification: it is acted on, but never
        // answered.
        let id = object.get("id").filter(|id| !id.is_null()).cloned();

        let result = match method {
            "notifications/initialized" => return Ok(()),
            "initialize" => {
                if let Some(requested) = params
                    .and_then(|params| params.get("protocolVersion"))
                    .and_then(Value::as_str)
                    .filter(|version| !version.is_empty())
                {
                    self.protocol_version = requested.to_string();
                }
                json!({
                    "protocolVersion": self.protocol_version,
                    "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
                    // `logging` because `logging/setLevel` is answered below;
                    // a client only sends it to a server that advertises it.
                    "capabilities": { "tools": {}, "logging": {} },
                })
            }
            "ping" | "logging/setLevel" => json!({}),
            "tools/list" => json!({ "tools": tool_definitions() }),
            "resources/list" => json!({ "resources": [] }),
            "prompts/list" => json!({ "prompts": [] }),
            "tools/call" => {
                let Some(params) = params.and_then(Value::as_object) else {
                    return Err(JsonRpcError::new(
                        JSONRPC_INVALID_PARAMS,
                        "tools/call params must be an object.",
                    ));
                };
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| JsonRpcError::new(JSONRPC_INVALID_PARAMS, "Missing tool name."))?
                    .to_string();
                let arguments = match params.get("arguments") {
                    None | Some(Value::Null) => Map::new(),
                    Some(Value::Object(arguments)) => arguments.clone(),
                    Some(_) => {
                        return Err(JsonRpcError::new(
                            JSONRPC_INVALID_PARAMS,
                            "Tool arguments must be an object.",
                        ));
                    }
                };
                self.call_tool(&name, &arguments)
            }
            other => {
                return Err(JsonRpcError::new(
                    JSONRPC_METHOD_NOT_FOUND,
                    format!("Method not found: {other}"),
                ));
            }
        };

        if let Some(id) = id {
            self.send(json!({ "jsonrpc": JSONRPC_VERSION, "id": id, "result": result }));
        }
        Ok(())
    }

    /// A tool failure is a *result* with `isError`, not a JSON-RPC error: the
    /// client is supposed to show the assistant what went wrong so it can try
    /// something else, which a protocol-level error does not do.
    fn call_tool(&self, name: &str, arguments: &Map<String, Value>) -> Value {
        match self.tools.call(name, arguments) {
            Ok(outcome) => json!({
                "content": [{ "type": "text", "text": tool_result_text(&outcome.title, &outcome.payload) }],
            }),
            Err(error) => json!({
                "content": [{
                    "type": "text",
                    "text": tool_result_text(&format!("Error: {error}"), &error.detail()),
                }],
                "isError": true,
            }),
        }
    }

    // -- framing -------------------------------------------------------------

    fn read_message(&mut self) -> Result<Option<Value>, JsonRpcError> {
        let (message, framing) = match read_framed(&mut self.reader)? {
            None => return Ok(None),
            Some(read) => read,
        };
        self.framing = framing;
        Ok(Some(message))
    }

    fn send(&self, payload: Value) {
        let raw = serde_json::to_vec(&payload).unwrap_or_default();
        let mut stdout = std::io::stdout().lock();
        if self.framing == Framing::ContentLength {
            let _ = write!(stdout, "Content-Length: {}\r\n\r\n", raw.len());
            let _ = stdout.write_all(&raw);
        } else {
            let _ = stdout.write_all(&raw);
            let _ = stdout.write_all(b"\n");
        }
        let _ = stdout.flush();
    }
}

/// One message off the stream, and the framing it arrived in. `Ok(None)` is a
/// clean end of input. Free of `Server` so the framing can be tested against a
/// buffer rather than stdin.
fn read_framed(reader: &mut impl BufRead) -> Result<Option<(Value, Framing)>, JsonRpcError> {
    loop {
        let Some(line) = read_line(reader)? else {
            return Ok(None);
        };
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.to_lowercase().starts_with("content-length:") {
            // Decided before the body is read, so a reply to a bad header
            // goes back in the framing the peer is speaking.
            return read_header_framed(reader, &trimmed)
                .map(|message| Some((message, Framing::ContentLength)));
        }
        return parse_body(trimmed.as_bytes()).map(|message| Some((message, Framing::Newline)));
    }
}

fn read_line(reader: &mut impl BufRead) -> Result<Option<String>, JsonRpcError> {
    let mut buffer = Vec::new();
    match reader.read_until(b'\n', &mut buffer) {
        Ok(0) => Ok(None),
        Ok(_) => Ok(Some(String::from_utf8_lossy(&buffer).into_owned())),
        Err(err) => Err(JsonRpcError::new(
            JSONRPC_PARSE_ERROR,
            format!("Could not read input: {err}"),
        )),
    }
}

fn read_header_framed(reader: &mut impl BufRead, first_line: &str) -> Result<Value, JsonRpcError> {
    let mut content_length: Option<usize> = None;
    let mut line = first_line.to_string();
    loop {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            let Some((name, value)) = trimmed.split_once(':') else {
                return Err(JsonRpcError::new(
                    JSONRPC_PARSE_ERROR,
                    "Malformed header line.",
                ));
            };
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = Some(value.trim().parse().map_err(|_| {
                    JsonRpcError::new(JSONRPC_PARSE_ERROR, "Invalid Content-Length header.")
                })?);
            }
        }
        match read_line(reader)? {
            None => break,
            Some(next) if next == "\r\n" || next == "\n" => break,
            Some(next) => line = next,
        }
    }

    let content_length = content_length
        .ok_or_else(|| JsonRpcError::new(JSONRPC_PARSE_ERROR, "Missing Content-Length header."))?;
    if content_length > MAX_CONTENT_LENGTH {
        // Skipped rather than allocated, so the stream stays aligned on the
        // next message and the peer gets an error instead of a dead server.
        let _ = std::io::copy(
            &mut reader.take(content_length as u64),
            &mut std::io::sink(),
        );
        return Err(JsonRpcError::new(
            JSONRPC_PARSE_ERROR,
            format!(
                "Content-Length {content_length} exceeds the {} MiB limit.",
                MAX_CONTENT_LENGTH / (1024 * 1024)
            ),
        ));
    }

    let mut body = vec![0_u8; content_length];
    reader.read_exact(&mut body).map_err(|_| {
        JsonRpcError::new(
            JSONRPC_PARSE_ERROR,
            "Unexpected EOF while reading message body.",
        )
    })?;
    parse_body(&body)
}

fn parse_body(body: &[u8]) -> Result<Value, JsonRpcError> {
    serde_json::from_slice(body)
        .map_err(|_| JsonRpcError::new(JSONRPC_PARSE_ERROR, "Invalid JSON payload."))
}

/// A title line, a blank line, then the payload as sorted-key, two-space JSON.
/// `serde_json::Map` is a `BTreeMap`, so the sorting is inherent rather than a
/// flag that could be forgotten on one call site.
pub fn tool_result_text(title: &str, payload: &Value) -> String {
    let body = serde_json::to_string_pretty(payload).unwrap_or_else(|_| "null".into());
    format!("{title}\n\n{body}")
}

/// The tool surface. The descriptions are what an assistant reads to decide
/// whether to call one, so they carry as much weight as the schemas.
///
/// `scripts/mcp_smoke_check.py` pins the count, which is the cheap half of
/// noticing an accidental removal; `docs/mcp-server.md` lists them.
pub fn tool_definitions() -> Vec<Value> {
    let list_id = || json!({ "type": "string" });
    let with_task_id = json!({
        "type": "object",
        "properties": { "list_id": list_id(), "task_id": { "type": "integer" } },
        "required": ["task_id"],
        "additionalProperties": false,
    });

    vec![
        json!({
            "name": "task_lists",
            "description": "List available task lists (non-archived).",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
        }),
        json!({
            "name": "task_fetch",
            "description": "Fetch tasks for a list. Defaults to open tasks only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "list_id": list_id(),
                    "include_closed": { "type": "boolean", "default": false },
                    "with_notes": { "type": "boolean", "default": true },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "task_add",
            "description": "Quick-add a task to list root or to a specific parent task ID.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "list_id": list_id(),
                    "content": { "type": "string", "minLength": 1 },
                    "location": { "type": "string", "enum": ["default", "specific"], "default": "default" },
                    "parent_task_id": { "type": "integer" },
                    "position": { "type": "integer", "default": 1 },
                    "due": { "type": "string" },
                },
                "required": ["content"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "task_update",
            "description": "Update task content, due field, and/or tags. An empty tags string removes every tag.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "list_id": list_id(),
                    "task_id": { "type": "integer" },
                    "content": { "type": "string" },
                    "due": { "type": "string" },
                    "tags": { "type": "string" },
                },
                "required": ["task_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "task_complete",
            "description": "Mark a task as complete (close).",
            "inputSchema": with_task_id,
        }),
        json!({
            "name": "task_reopen",
            "description": "Reopen a task.",
            "inputSchema": with_task_id,
        }),
        json!({
            "name": "task_invalidate",
            "description": "Invalidate a task.",
            "inputSchema": with_task_id,
        }),
        json!({
            "name": "task_delete",
            "description": "Delete a task.",
            "inputSchema": with_task_id,
        }),
        json!({
            "name": "task_move",
            "description": "Reorder a task among its siblings. Position is 1-based.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "list_id": list_id(),
                    "task_id": { "type": "integer" },
                    "position": { "type": "integer", "minimum": 1 },
                },
                "required": ["task_id", "position"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "task_reparent",
            "description": "Move a task under a different parent. Omit parent_task_id (or pass 0) to move it to the list root.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "list_id": list_id(),
                    "task_id": { "type": "integer" },
                    "parent_task_id": { "type": "integer" },
                },
                "required": ["task_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "project_move",
            "description": "Move a root task and its complete subtree to another list. The destination copy is verified before the source project is deleted; the result includes an old-to-new task ID map.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source_list_id": { "type": "string", "minLength": 1 },
                    "target_list_id": { "type": "string", "minLength": 1 },
                    "task_id": { "type": "integer" },
                },
                "required": ["source_list_id", "target_list_id", "task_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "task_note_add",
            "description": "Append a note (Checkvist comment) to a task. Notes are read back via task_fetch with with_notes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "list_id": list_id(),
                    "task_id": { "type": "integer" },
                    "note": { "type": "string", "minLength": 1 },
                },
                "required": ["task_id", "note"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "list_create",
            "description": "Create a new checklist.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": { "type": "string", "minLength": 1 } },
                "required": ["name"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "task_search",
            "description": "Search tasks in a list by content substring, tag, and/or due date. Cheaper than fetching the whole list.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "list_id": list_id(),
                    "query": { "type": "string" },
                    "tag": { "type": "string" },
                    "due_before": { "type": "string", "description": "YYYY-MM-DD, exclusive." },
                    "include_closed": { "type": "boolean", "default": false },
                    "limit": { "type": "integer", "default": 50, "minimum": 1 },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "daily_log_fetch",
            "description": "What actually happened on recent days: completions, focus time, unfinished and deferred tasks, and daily ticks. Local to Takt; read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "days": {
                        "type": "integer",
                        "default": 1,
                        "minimum": 1,
                        "maximum": 90,
                        "description": "How many logical days back to include, ending today.",
                    },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "dailies_list",
            "description": "The configured dailies (habits) with today's schedule and tick state. Local to Takt; read-only.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
        }),
        json!({
            "name": "task_metadata",
            "description": "Takt-only per-task state that Checkvist does not store: priority ranks (scoped and absolute), recurrence rules, start dates, Eisenhower matrix placements, and the kanban board's columns. Read-only.",
            "inputSchema": {
                "type": "object",
                "properties": { "list_id": list_id() },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "task_matrix_set",
            "description": "Place tasks on the Eisenhower matrix (urgency and importance, each -9 to 9; 0,0 removes a placement). Local to Takt. Requires Takt to be closed - a running app overwrites these on its next save.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "list_id": list_id(),
                    "placements": {
                        "type": "array",
                        "description": "One entry per task.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "task_id": { "type": "integer" },
                                "urgency": { "type": "number", "minimum": -9, "maximum": 9 },
                                "importance": { "type": "number", "minimum": -9, "maximum": 9 },
                            },
                            "required": ["task_id", "urgency", "importance"],
                            "additionalProperties": false,
                        },
                    },
                },
                "required": ["placements"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "daily_add",
            "description": "Create a daily (a habit that resets each day, not a task). Local to Takt.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "minLength": 1 },
                    "active_weekdays": {
                        "type": "array",
                        "items": { "type": "integer", "minimum": 1, "maximum": 7 },
                        "description": "1 = Sunday. Omit for every day.",
                    },
                    "interval_days": {
                        "type": "integer", "minimum": 1, "maximum": 366,
                        "description": "Repeat every N days from today instead of on fixed weekdays. Not combinable with active_weekdays.",
                    },
                },
                "required": ["title"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "daily_update",
            "description": "Rename a daily, reschedule it (fixed weekdays or an every-N-days cycle), or archive/unarchive it. Archiving keeps history readable rather than deleting.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "daily_id": { "type": "string" },
                    "title": { "type": "string" },
                    "active_weekdays": {
                        "type": "array",
                        "items": { "type": "integer", "minimum": 1, "maximum": 7 },
                        "description": "1 = Sunday. Switches a cycling daily back to fixed weekdays.",
                    },
                    "interval_days": {
                        "type": "integer", "minimum": 1, "maximum": 366,
                        "description": "Repeat every N days instead of on fixed weekdays. Not combinable with active_weekdays.",
                    },
                    "archived": { "type": "boolean" },
                },
                "required": ["daily_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "daily_tick",
            "description": "Tick or un-tick a daily for today. Recorded against the current logical day, honouring the configured rollover hour.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "daily_id": { "type": "string" },
                    "done": { "type": "boolean", "default": true },
                },
                "required": ["daily_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "focus_status",
            "description": "What the focus timer is doing right now: the task, whether it is paused, elapsed and planned seconds, and the queue behind it. Read directly from the app's workspace database, read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "focus_history",
            "description": "Focused time already recorded, newest first, over the last N logical days. Read directly from the app's workspace database, read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "days": { "type": "integer", "minimum": 1, "maximum": 90, "default": 1 },
                },
                "additionalProperties": false,
            },
        }),
    ]
    .into_iter()
    .chain(workspace_tool_definitions())
    .collect()
}

/// The tools that edit the app's own local workspace rather than Checkvist.
/// Ids here are the workspace's UUID strings, not Checkvist's integers, and
/// every write lands in the app's Undo menu as "MCP: …".
fn workspace_tool_definitions() -> Vec<Value> {
    let id = |description: &str| json!({ "type": "string", "description": description });
    let board_help = "A list id from workspace_tree, everything, or default (the columns a board without its own uses). Default: everything.";
    let links = json!({
        "type": "array",
        "items": { "type": "string" },
        "description": "URLs the task links to. Takt opens the first http, https or obsidian:// link from the task with its open-link command.",
    });
    let column = json!({
        "type": ["string", "null"],
        "description": "Kanban column id, e.g. backlog, in-progress, this-week, waiting-on, today (the defaults). Null or \"\" clears it, which puts the card in the board's first column.",
    });
    // Filter lists. The rule vocabulary is the Rust core's catalog, written
    // out here so an assistant can build rules without a lookup first.
    let rule_help = format!(
        "Each rule is {{field, op, value}}; value is left out where the op takes none. \
         Fields and their ops: {}. Value kinds: tag without #; days, minutes and number \
         are whole numbers (due within 0 means due by today, overdue included); date is \
         yyyy-mm-dd; lists is comma-separated list ids and folder a folder id, both from \
         workspace_tree; column a kanban column id; text matches title or notes. \
         Priority runs 1 (the most urgent) to 4, then no priority, then -4 to -1 \
         (behind the rest, -1 last), so priority at-least 2 is 1 or 2 and at-most 3 is \
         3, 4 or a negative one. \
         Example: tagged work and due within a week is \
         [{{\"field\":\"tag\",\"op\":\"has\",\"value\":\"work\"}},\
         {{\"field\":\"due\",\"op\":\"within\",\"value\":\"7\"}}].",
        crate::workspace_tasks::rule_catalog_text()
    );
    let icon = json!({
        "type": ["string", "null"],
        "description": "An SF Symbol name, e.g. line.3.horizontal.decrease. Null or \"\" clears it.",
    });
    let match_mode = json!({
        "type": "string", "enum": ["all", "any"],
        "description": "all: every rule must hold (the default). any: at least one.",
    });
    let show = json!({
        "type": "string", "enum": ["open", "done", "all"],
        "description": "Which tasks by status: open (the default), done (completed or cancelled) or all.",
    });
    let rules = json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "field": { "type": "string", "enum": crate::workspace_tasks::rule_fields() },
                "op": { "type": "string" },
                "value": { "type": ["string", "integer"] },
            },
            "required": ["field", "op"],
            "additionalProperties": false,
        },
    });

    vec![
        json!({
            "name": "workspace_tree",
            "description": "Takt's local workspace, the app's source of truth: folders, lists (with folder_id and open task counts) and nested lists, each with its id. Start here to find a list_id or folder_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "include_archived": { "type": "boolean", "default": false },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_shared_lists",
            "description": "The local lists shared with other Takt accounts: each list's id and name, your role (owner or member) and its members' emails. A shared list's tasks are everyone's to edit; its sidebar place, board columns, matrix placement, conditions and agent context stay each person's own. Sharing, inviting and leaving happen in the app, not here. workspace_tree marks the same lists with shared.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
        }),
        json!({
            "name": "workspace_tasks",
            "description": "One local list's tasks as a tree: ids, titles, notes, status, kind (task or nested list), kanban column and external links, with blocked: true on a task waiting on an open task (workspace_task_dependencies says which). Open tasks only unless include_closed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "list_id": id("From workspace_tree."),
                    "parent_task_id": id("Only this task's subtree."),
                    "include_closed": { "type": "boolean", "default": false },
                },
                "required": ["list_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_task_add",
            "description": "Create a task in a local list, at the end of its siblings (or first, with at_top). Pass parent_task_id to create a subtask; list_id may then be omitted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "list_id": id("From workspace_tree."),
                    "title": { "type": "string", "minLength": 1 },
                    "parent_task_id": id("Create it as a subtask of this task."),
                    "notes": { "type": "string" },
                    "external_links": links,
                    "kanban_column": column,
                    "kind": { "type": "string", "enum": ["task", "list"], "default": "task", "description": "list makes a nested list." },
                    "at_top": { "type": "boolean", "default": false },
                },
                "required": ["title"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_task_update",
            "description": "Change a local task's title, notes, external links (replaces the whole set), status, kanban column, kind, sidebar pin, what it is waiting on and when to follow up, its due date, start, priority or estimate. Fields left out are unchanged; null clears one. One call is one undo step. Completing a repeating task writes its next occurrence, as Takt does, and reopening it takes that occurrence back while it is untouched. Moving a repeating task's dates onto its next turn is saved as asked: unlike the apps, this does not ask whether to stack the two turns or keep both. Dates are ISO only: turn a phrase like \"friday\" or \"next week\" into a date yourself, from today's date, before calling.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": id("From workspace_tasks."),
                    "title": { "type": "string", "minLength": 1 },
                    "notes": { "type": "string" },
                    "external_links": links,
                    "status": { "type": "string", "enum": ["open", "completed", "cancelled"] },
                    "kanban_column": column,
                    "kind": {
                        "type": "string", "enum": ["task", "list"],
                        "description": "'list' makes it a nested list in place (it stays where it is; its subtasks become the nested list's contents). To make a standalone sidebar list instead, use workspace_task_to_list.",
                    },
                    "pinned": {
                        "type": "boolean",
                        "description": "Pin a nested list to the sidebar (the app's 'Promote to sidebar'), or unpin it. Only for kind 'list'.",
                    },
                    "waiting_on": {
                        "type": ["string", "null"],
                        "description": "Who or what the task waits on (\"Sam\", \"Legal\"). Files it in the waiting-on column. Null or empty clears the tag; clearing it and follow_up_at both takes the task out of the waiting-on column. A kanban_column in the same call wins.",
                    },
                    "follow_up_at": {
                        "type": ["string", "null"],
                        "description": "When to chase it: 2026-10-08 14:00 (local) or RFC 3339. Files it in the waiting-on column; if it is still waiting then, Takt adds a 'Follow up' task to Today. Null or empty clears it; clearing it and waiting_on both takes the task out of the waiting-on column.",
                    },
        "assignee": {
                        "type": ["string", "null"],
                        "description": "Who in a shared list does it: a member's email, account id or \"me\" (workspace_shared_lists names them). Null assigns it to nobody. A task assigned to someone else leaves your Today and focus. Refused for a task in a list that isn't shared.",
                    },
                                        "follow_up_every_days": {
                        "type": ["integer", "null"],
                        "description": "Chase again every this many days (1 to 365): when the follow-up task is completed while this task is still waiting, its next follow-up is set that many days on, at the same time of day, and the next follow-up is numbered ('Follow up #2 with Sam: ...'). One never completed makes no more. Null or 0 makes it once. Needs a follow-up time: refused on a task with no follow_up_at unless one is passed in the same call.",
                    },
                    "due": {
                        "type": ["string", "null"],
                        "description": "When it is due. A date (2026-10-17) means any time that day; a date and time (2026-10-17 17:00, local, or RFC 3339) means by then. Replaces whichever it had. Null or empty clears it.",
                    },
                    "start": {
                        "type": ["string", "null"],
                        "description": "When it may start: a date (the start of that day, local) or a date and time (2026-10-20 09:00, local, or RFC 3339). Must come before the due date. Null or empty clears it.",
                    },
                    "priority": {
                        "type": ["integer", "null"], "minimum": -4, "maximum": 4,
                        "description": "1 urgent, 2 high, 3 medium, 4 low; -4 to -1 send it behind every task with none, -1 the very back, and move it to the bottom of its siblings as a positive one moves it to the top. Null or 0 clears it.",
                    },
                    "estimate_minutes": {
                        "type": ["integer", "null"], "minimum": 0,
                        "description": "How long it will take, in minutes. Null or 0 clears it.",
                    },
                },
                "required": ["task_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_task_move",
            "description": "Reparent and/or reorder a local task, with its whole subtree. parent_task_id moves it under that task (in that task's list). list_id alone moves it to that list's top level. position (1-based) places it among its siblings afterwards; alone, it just reorders.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": id("From workspace_tasks."),
                    "parent_task_id": id("New parent."),
                    "list_id": id("Destination list's top level, when no parent is given."),
                    "position": { "type": "integer", "minimum": 1 },
                },
                "required": ["task_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_task_to_list",
            "description": "Promote a task to a standalone list of its own, as dropping it on a folder or the sidebar's Lists heading does in the app. The list is named after the task, the task stays as the list's hidden root, and its subtasks become the list's contents with their ids unchanged.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": id("From workspace_tasks."),
                    "folder_id": id("Folder to put the new list in. Omit for the top level."),
                },
                "required": ["task_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_task_delete",
            "description": "Delete a local task and its whole subtree. Undoable from Takt's Undo menu.",
            "inputSchema": {
                "type": "object",
                "properties": { "task_id": id("From workspace_tasks.") },
                "required": ["task_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_task_dependencies",
            "description": "What a local task waits on (blocked_by) and what waits on it (blocking), each with id, title, status and list, and whether it is blocked now: open and waiting on at least one open task. A blocked task sorts below its siblings and is skipped by next up.",
            "inputSchema": {
                "type": "object",
                "properties": { "task_id": id("From workspace_tasks.") },
                "required": ["task_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_task_blocker_add",
            "description": "Make a local task wait on another until that one is completed or cancelled. Blockers may be in any list. A task cannot wait on itself or on a task that already waits on it. Adding one that is already there changes nothing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": id("The task that waits. From workspace_tasks."),
                    "blocked_by_task_id": id("The task it waits on."),
                },
                "required": ["task_id", "blocked_by_task_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_task_blocker_remove",
            "description": "Stop a local task waiting on another. Removing one that is not there changes nothing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": id("The task that waits. From workspace_task_dependencies."),
                    "blocked_by_task_id": id("The task it waits on."),
                },
                "required": ["task_id", "blocked_by_task_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_task_commit",
            "description": "Commit to a local task periodically: adds a subtask carrying a habit, which lands in Today on its schedule (and rests on other days) until the task is done. Use it for steady work towards a large goal — \"5 minutes of sonnets a day\" on \"Memorise the sonnets\". Set the subtask's estimate afterwards with workspace_task_update.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": id("The task it advances. From workspace_tasks."),
                    "title": { "type": "string", "description": "The subtask's title. Defaults to \"Work on <task>\"." },
                    "rule": { "type": "string", "description": "When it appears: \"daily\" (the default), \"every 3 days\", \"weekly\", \"weekdays\", \"friday\", or days like \"mon, wed, fri\"." },
                    "stacks_missed": { "type": "boolean", "description": "Keep a missed appearance owed until done, rather than letting it vanish. Default false." },
                },
                "required": ["task_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_filter_lists",
            "description": "Takt's filter lists (smart lists): saved rule sets asked of every task rather than places tasks live. The built-in Today (open tasks planned for today or due by today) and Everything (every open task) come first, then the user's own in sidebar order, each with id, name, icon, a one-line summary, match, show and rules.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_filter_list_tasks",
            "description": "The tasks a filter list shows right now, evaluated by the Rust core as the app does in this machine's time zone: open lists only, list by list in outline order, each task with its list_name and blocked: true where it waits on an open task.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "filter_list": id("An id from workspace_filter_lists (builtin:today and builtin:everything included), or a filter list's name."),
                },
                "required": ["filter_list"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_filter_list_create",
            "description": format!("Create a filter list at the end of the sidebar's. It shows the tasks whose status `show` admits and whose rules hold: every rule with match all, at least one with any; no rules shows every task `show` admits. {rule_help}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "icon": icon.clone(),
                    "match": match_mode.clone(),
                    "show": show.clone(),
                    "rules": rules.clone(),
                },
                "required": ["name"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_filter_list_update",
            "description": format!("Change a filter list's name, icon, match, show or rules (rules replaces the whole set). Fields left out are unchanged; one call is one undo step. Today and Everything are built in and cannot be edited. {rule_help}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "filter_list_id": id("From workspace_filter_lists."),
                    "name": { "type": "string", "minLength": 1 },
                    "icon": icon,
                    "match": match_mode.clone(),
                    "show": show,
                    "rules": rules.clone(),
                },
                "required": ["filter_list_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_filter_list_delete",
            "description": "Delete a filter list. No task changes: a filter list never held any. Today and Everything cannot be deleted. Undoable from Takt's Undo menu.",
            "inputSchema": {
                "type": "object",
                "properties": { "filter_list_id": id("From workspace_filter_lists.") },
                "required": ["filter_list_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_folder_create",
            "description": "Create a sidebar folder, at the end of the top level or inside parent_folder_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "parent_folder_id": id("Nest it inside this folder."),
                },
                "required": ["name"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_list_create",
            "description": "Create an empty local list, at the top level or inside folder_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "folder_id": id("From workspace_tree."),
                },
                "required": ["name"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_list_move",
            "description": "Move a local list into a folder, or to the top level when folder_id is omitted. It goes to the end of its new siblings.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "list_id": id("From workspace_tree."),
                    "folder_id": id("Destination folder. Omit for the top level."),
                },
                "required": ["list_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_list_delete",
            "description": "Delete a local list and every task in it. The Inbox cannot be deleted. Undoable from Takt's Undo menu.",
            "inputSchema": {
                "type": "object",
                "properties": { "list_id": id("From workspace_tree.") },
                "required": ["list_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_board_columns",
            "description": "A board's columns, left to right: a list's (its own, else the workspace default, else the stock five), Everything's, or the default itself. `source` says which. A column with rules holds the tasks its rules match, the first such column from the left winning; the others hold the tasks filed in them.",
            "inputSchema": {
                "type": "object",
                "properties": { "board": id(board_help) },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_board_column_set",
            "description": format!("Add a board column, or rename one or set its rules (pass column_id). Rules are a filter list's (match all or any); an empty rules array makes it an ordinary column. A board following the default gets columns of its own. One undo step. {rule_help}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "board": id(board_help),
                    "column_id": id("From workspace_board_columns. Omit to add a column."),
                    "title": { "type": "string", "minLength": 1 },
                    "match": match_mode,
                    "rules": rules,
                    "position": { "type": "integer", "minimum": 0, "description": "Where a new column goes among the board's, from 0. Default: last." },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_board_column_delete",
            "description": "Delete a board column. The tasks filed in it move to move_to (default: the first remaining ordinary column) in the same undo step. Deleting a rule column, Today or Waiting on moves nothing: their tasks stay planned or waiting.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "board": id(board_help),
                    "column_id": id("From workspace_board_columns."),
                    "move_to": id("A remaining column's id."),
                },
                "required": ["column_id"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "agent_instructions_get",
            "description": "The user's standing instructions for the agent: what they want kept in mind for the whole account, a folder or a list. With no scope, every account, folder and list scope that has any, each with its scope, id and name. With a scope, that one (empty text when none is written). The task scope (with a task's id) is that task's agent context: what to know about it, kept apart from its notes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string", "enum": ["account", "folder", "list", "task"] },
                    "id": id("A folder or list id from workspace_tree. Not for account."),
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_conditions",
            "description": "Conditions: the places and circumstances a task can require before it is offered (Home, Campus, Floor space), under their categories in the order the app shows them (settings first: Location, and any category whose conditions provide others), each with how many open tasks need it and what it provides. Location is one at a time. A condition can provide others: while Home holds, so do the Floor space, Desk and Quiet it provides. Also the context: which conditions were said to hold (on), everything that holds with where it comes from (holds: said, or from the settings that provide it — this is what tasks are checked against, negated requirements included), provided conditions switched off for now (off_for_now), when they were said (set_at; stale when that was an earlier day, so probably no longer true), the available-time window, goal and expiry, the last context an expiry set aside, and how each day starts (day_start: whether it resets, the hour the day changes, the conditions it starts with, the last day it reset). Reading starts the day first if it has changed since the last reset, as opening the app does. A task that needs a condition that is not on is held back from Today, next up and the focus panel: this is how to explain why. With condition (a name or id), also the open tasks that need it. workspace_tasks shows each task's requires.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "condition": { "type": "string", "description": "A condition's name or id: also list the open tasks that need it." },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_task_conditions_set",
            "description": "Set what a local task requires before it is offered. requires replaces the whole set: each item is one requirement that must hold, either a condition or an array of conditions any one of which will do (places a task can be done in go in one array: [[\"Home\", \"Campus\"]]). Conditions by name or id; an empty array requires nothing. To require that a condition does NOT hold, write it \"not Home\" or \"!Home\" (or \"!\" before its id): [[\"not Home\"]] is only away from Home, and [[\"Campus\", \"not Quiet\"]] is on campus or anywhere not quiet. workspace_tasks writes such a requirement back as \"not Home\". With create_missing, a name no condition has yet becomes a new condition first. Archived conditions are refused unless the task already needed them. One undo step.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": id("From workspace_tasks."),
                    "requires": {
                        "type": "array",
                        "items": { "anyOf": [
                            { "type": "string" },
                            { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                        ] },
                    },
                    "create_missing": { "type": "boolean", "description": "Default: false." },
                },
                "required": ["task_id", "requires"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_condition_provides_set",
            "description": "Set what a condition provides, replacing what it did: while it holds, they hold too, so a task needing Floor space is offered at Home or the Gym without anyone turning Floor space on. Location = Home might provide Floor space, Desk and Quiet; a tool on hand like Laptop, Typing and Big screen. Links chain (Home provides Desk, Desk provides Typing) and a loop is harmless. condition and provides by name or id; an empty array provides nothing. A condition cannot provide itself, nor an archived condition it did not already. With create_missing, a name no condition has yet becomes a new condition first. One undo step. Returns what it provides as saved.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "condition": { "type": "string", "description": "The condition that provides, by name or id." },
                    "provides": { "type": "array", "items": { "type": "string" } },
                    "create_missing": { "type": "boolean", "description": "Default: false." },
                },
                "required": ["condition", "provides"],
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_context_set",
            "description": "Say what holds now, as the app's Conditions tab does: which conditions are on (the user says where they are, or what they have), the time available, the goal and when the answer lapses. set replaces the conditions on; clear turns them all off; then off and on adjust. Turning a place on turns the other place off. What a condition said to hold provides holds with it (workspace_condition_provides_set): off on a provided condition switches it off for now (housemates in: off Quiet), until whatever provides it is turned off or replaced, and on brings it back. Conditions by name or id. available_minutes and expires_in_minutes count from now; 0 removes the limit. The app picks the change up at once. Not an undo step. A day that has changed since the last reset starts first (workspace_day_start_set), so what you set is today's. Returns the context as workspace_conditions reports it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "set": { "type": "array", "items": { "type": "string" } },
                    "on": { "type": "array", "items": { "type": "string" } },
                    "off": { "type": "array", "items": { "type": "string" } },
                    "clear": { "type": "boolean" },
                    "mode": { "type": "string", "enum": ["progress", "finish"] },
                    "available_minutes": { "type": "integer", "minimum": 0 },
                    "expires_in_minutes": { "type": "integer", "minimum": 0 },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "workspace_day_start_set",
            "description": "Set how each day starts, for every device: at the hour the day changes (local time, 0 to 23, 4 by default — the daily log's days change then too), what holds goes back to the conditions each day starts with (Home), with nothing switched off for now, no time window and no expiry. It happens once a day, on whichever device looks first after the change, including one that was closed for days; something said since the day changed is kept. resets turns it on or off (off: what holds carries over); conditions replaces the starting set, by name or id, one place at most, and naming any turns the reset on; an empty array starts each day with nothing on. Fields left out keep what they were; with none, it only reports. Today counts as started, so the first reset is at the next change of day. Not an undo step. Returns the settings as saved.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "resets": { "type": "boolean" },
                    "hour": { "type": "integer", "minimum": 0, "maximum": 23 },
                    "conditions": { "type": "array", "items": { "type": "string" } },
                },
                "additionalProperties": false,
            },
        }),
        json!({
            "name": "agent_instructions_set",
            "description": "Save the user's standing instructions for the account, a folder or a list, or the agent context of one task (scope task, with its id): durable preferences, facts and conventions they want kept, never a one-off request. Pick the narrowest scope that fits. A task's agent context is not its notes, which are the user's own. mode replace (the default) writes the whole text, so read the scope first and merge rather than duplicate; append adds a paragraph. Blank text with replace clears the scope. Keep it short. Returns the text before and after. One undo step.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string", "enum": ["account", "folder", "list", "task"] },
                    "id": id("A folder or list id from workspace_tree. Not for account."),
                    "text": { "type": "string" },
                    "mode": { "type": "string", "enum": ["replace", "append"], "description": "Default: replace." },
                },
                "required": ["scope", "text"],
                "additionalProperties": false,
            },
        }),
    ]
}

#[cfg(test)]
mod framing_tests {
    use super::*;
    use std::io::Cursor;

    fn read(input: &[u8]) -> Result<Option<(Value, Framing)>, JsonRpcError> {
        read_framed(&mut Cursor::new(input.to_vec()))
    }

    #[test]
    fn a_bare_line_is_a_newline_framed_message() {
        let (message, framing) = read(b"{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"id\":1}\n")
            .expect("read")
            .expect("a message");
        assert_eq!(message["method"], json!("ping"));
        assert!(framing == Framing::Newline);
    }

    #[test]
    fn a_content_length_header_frames_exactly_that_many_bytes() {
        let body = br#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let mut input = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        input.extend_from_slice(body);
        let (message, framing) = read(&input).expect("read").expect("a message");
        assert_eq!(message["id"], json!(1));
        assert!(framing == Framing::ContentLength);
    }

    #[test]
    fn an_oversized_or_unparsable_content_length_is_a_parse_error_not_a_crash() {
        // The size is a claim made by the peer; it must not be allocated on
        // trust. Both failures answer -32700, the code the run loop already
        // reports without an id, rather than ending the server.
        let huge = format!("Content-Length: {}\r\n\r\n{{}}", usize::MAX);
        let error = read(huge.as_bytes()).expect_err("refused");
        assert_eq!(error.code, JSONRPC_PARSE_ERROR);
        assert!(error.message.contains("exceeds"), "{}", error.message);

        let error = read(b"Content-Length: lots\r\n\r\n{}").expect_err("refused");
        assert_eq!(error.code, JSONRPC_PARSE_ERROR);
    }

    #[test]
    fn an_oversized_body_is_skipped_so_the_next_message_still_reads() {
        let body = vec![b' '; 16];
        let mut input = format!("Content-Length: {}\r\n\r\n", MAX_CONTENT_LENGTH + 1).into_bytes();
        input.extend_from_slice(&body);
        // Shorter than claimed: skipping consumes what there is and stops at
        // EOF, after which the stream is simply finished.
        let mut cursor = Cursor::new(input);
        assert!(read_framed(&mut cursor).is_err());
        assert!(read_framed(&mut cursor).expect("eof").is_none());
    }
}

#[cfg(test)]
mod tool_classification_tests {
    use super::tool_definitions;
    use std::collections::BTreeSet;
    use takt_core::agent_tools::{READ_ONLY_TOOLS, WRITE_TOOLS};

    /// The agent panel decides what to pre-allow from the core's two lists,
    /// so they must be this server's whole table, exactly.
    #[test]
    fn every_declared_tool_is_classified_as_a_read_or_a_write() {
        let declared: BTreeSet<String> = tool_definitions()
            .iter()
            .map(|tool| {
                tool["name"]
                    .as_str()
                    .expect("a tool has a name")
                    .to_string()
            })
            .collect();
        let classified: BTreeSet<String> = READ_ONLY_TOOLS
            .iter()
            .chain(WRITE_TOOLS)
            .map(|name| name.to_string())
            .collect();
        assert_eq!(declared, classified);
    }
}
