//! The agent socket as an MCP server (feature `mcp`): the tools are made
//! at run time from the application's `describe` reply, so a new
//! capability becomes a tool with no code. Every tool call goes to the
//! socket (`call`, `snapshot`, `wait`); the bridge's mode and the `agent`
//! flags apply there, as to any agent.
//!
//! The `widget_*` tools drive the interface by pointer and pixels: Rerun's
//! agent tools (`egui_inspection_tools`, by Lucas Meurer and Emil
//! Ernerfeldt, [rerun-io/egui_crates](https://github.com/rerun-io/egui_crates)),
//! served over the socket's `inspect` method, so they are held like it (the
//! bridge on, the `ui` and private grants, the input lock). See
//! [`widget_tools`].
//!
//! [`McpBridge`] implements rmcp's [`ServerHandler`] over a [`Backend`]:
//! [`SocketClient`] for a running application (the
//! `egui-pop-agent-mcp <socket>` binary serves it over stdio), or any
//! other in a test.

use std::borrow::Cow;
use std::collections::HashSet;
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use egui_inspection_tools::{BoxFuture, Content, Effect, ToolDef, ToolOutput, Transport};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value as Json, json};

use crate::RpcError;
use crate::inspect::{Request, Response, request_to_json, response_from_json};

/// How long a command's tool waits for the command to be done: a command
/// is queued for the next frame and its result comes on the journal.
pub const COMMAND_DEADLINE_MS: u64 = 10_000;

/// Where the bridge sends its requests: the socket's JSON-RPC methods
/// (`describe`, `snapshot`, `call`, `wait`) and their params; the result,
/// or the error. Called on a blocking thread.
pub trait Backend: Send + Sync + 'static {
    fn request(&self, method: &str, params: Json) -> Result<Json, RpcError>;
}

/// The application's socket (on Windows, its pipe), one connection per
/// request: a long `wait` holds no other call.
#[derive(Clone, Debug)]
pub struct SocketClient {
    path: PathBuf,
}

impl SocketClient {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn transport_error(&self, e: impl std::fmt::Display) -> RpcError {
        RpcError::new(
            RpcError::NO_ANSWER,
            format!("the socket {} did not answer: {e}", self.path.display()),
        )
    }
}

impl Backend for SocketClient {
    fn request(&self, method: &str, params: Json) -> Result<Json, RpcError> {
        #[cfg(unix)]
        let stream = std::os::unix::net::UnixStream::connect(&self.path)
            .map_err(|e| self.transport_error(e))?;
        #[cfg(not(unix))]
        let stream = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)
            .map_err(|e| self.transport_error(e))?;
        let mut write = stream.try_clone().map_err(|e| self.transport_error(e))?;
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        writeln!(write, "{req}")
            .and_then(|()| write.flush())
            .map_err(|e| self.transport_error(e))?;
        let mut line = String::new();
        BufReader::new(stream)
            .read_line(&mut line)
            .map_err(|e| self.transport_error(e))?;
        let response: Json = serde_json::from_str(&line)
            .map_err(|e| self.transport_error(format!("not JSON: {e}")))?;
        if let Some(e) = response.get("error") {
            return Err(RpcError {
                code: e["code"].as_i64().unwrap_or(RpcError::NO_ANSWER),
                message: e["message"].as_str().unwrap_or("error").into(),
                data: e.get("data").cloned(),
            });
        }
        Ok(response.get("result").cloned().unwrap_or(Json::Null))
    }
}

/// What a tool calls on the socket.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A collector's action: `call { collector, action, args }`.
    Action { collector: String, action: String },
    /// A command: `call commands.run { id, args }`, then a wait for its
    /// result.
    Command { id: String },
    /// `snapshot { collector? }`.
    Snapshot,
    /// `wait`, held by the socket.
    Wait,
    /// `ui`: the accessibility tree, and input on its nodes.
    Ui,
}

/// A tool made from a capability.
#[derive(Clone, Debug)]
pub struct GeneratedTool {
    pub tool: Tool,
    pub target: Target,
    /// The arguments are not an object: the tool takes them as `args`.
    pub wrapped: bool,
}

/// An MCP tool name from a capability's: `[A-Za-z0-9_-]`, 64 characters
/// at most (what the strictest clients take); `.` and anything else
/// become `_`.
pub fn tool_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if s.is_empty() { "tool".into() } else { s }
}

fn object(v: Json) -> Arc<JsonObject> {
    match v {
        Json::Object(o) => Arc::new(o),
        _ => Arc::new(JsonObject::new()),
    }
}

fn is_object_schema(s: &Json) -> bool {
    s.get("type").and_then(Json::as_str) == Some("object")
}

/// A JSON type's name, for a schema made from an example.
fn type_of(v: &Json) -> &'static str {
    match v {
        Json::Null => "null",
        Json::Bool(_) => "boolean",
        Json::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Json::Number(_) => "number",
        Json::String(_) => "string",
        Json::Array(_) => "array",
        Json::Object(_) => "object",
    }
}

/// The input schema and whether the arguments are wrapped as `args`:
/// the capability's schema when it is an object's; a schema that is not
/// is wrapped; without a schema, an object schema made from the example
/// (its keys and their types, or its text as the description).
fn input_schema(args: &Json, example: &Json) -> (Json, bool) {
    if args.is_object() {
        if is_object_schema(args) {
            return (args.clone(), false);
        }
        return (
            json!({ "type": "object", "properties": { "args": args }, "required": ["args"] }),
            true,
        );
    }
    match example {
        Json::Object(o) => {
            let props: serde_json::Map<String, Json> = o
                .iter()
                .map(|(k, v)| (k.clone(), json!({ "type": type_of(v) })))
                .collect();
            (
                json!({
                    "type": "object",
                    "properties": props,
                    "description": format!("Example: {example}"),
                }),
                false,
            )
        }
        Json::Null => (json!({ "type": "object" }), false),
        Json::String(text) => (
            json!({
                "type": "object",
                "properties": { "args": { "description": text } },
            }),
            true,
        ),
        other => (
            json!({
                "type": "object",
                "properties": { "args": { "type": type_of(other) } },
                "description": format!("Example: {{\"args\": {other}}}"),
            }),
            true,
        ),
    }
}

/// The tools always offered: `snapshot`, `wait` and `ui`.
fn builtins() -> Vec<GeneratedTool> {
    let snapshot = Tool::new(
        "snapshot",
        "Every collector's snapshot by name, or one collector's: the application's state as the \
         debug bridge reads it. Refused when the bridge is off.",
        object(json!({
            "type": "object",
            "properties": {
                "collector": { "type": "string", "description": "One collector's name; all when absent." }
            },
        })),
    )
    .with_annotations(ToolAnnotations::new().read_only(true));
    let wait = Tool::new(
        "wait",
        "Waits until a journal event happens (`event`, optionally caused by `cause`), the \
         application is settled (`settled: true`), or the first of several (`any`), after the \
         event numbered `since`; held up to `deadline_ms` (5 s by default, 120 s at most). \
         Answers `met` and, when met, `by` and the `event`; `last` is the newest event's number; \
         `orphaned` counts the jobs left going on by a disconnected plugin, which `settled` \
         does not wait on.",
        object(json!({
            "type": "object",
            "properties": {
                "event": { "type": "string" },
                "cause": { "type": "integer" },
                "settled": { "type": "boolean" },
                "any": { "type": "array", "items": { "type": "object" } },
                "since": { "type": "integer" },
                "deadline_ms": { "type": "integer" },
            },
        })),
    )
    .with_raw_output_schema(object(json!({
        "type": "object",
        "properties": {
            "met": { "type": "boolean" },
            "by": { "type": ["string", "null"] },
            "event": {},
            "last": { "type": "integer" },
            "orphaned": { "type": "integer" },
        },
        "required": ["met"],
    })))
    .with_annotations(ToolAnnotations::new().read_only(true));
    let ui = Tool::new(
        "ui",
        "The interface, as a person sees and drives it. `action: \"tree\"` lists the \
         accessibility tree's nodes (role, label, value, rect), or those matching `label` \
         (exactly) and `role` (`Button`, `TextInput`, any case). `click`, `hover`, \
         `secondary_click` (a right-click), `type` (with `text`) and `scroll` (`up`, optional) act \
         on the one node `label` and `role` name, `key` presses \
         `key` (`Escape`, `Ctrl+K`); they answer once the input went in, and need the bridge on.",
        object(json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["tree", "click", "hover", "secondary_click", "type", "key", "scroll"] },
                "label": { "type": "string" },
                "role": { "type": "string" },
                "nth": { "type": "integer", "description": "Which of several matching nodes, in reading order, from 0." },
                "text": { "type": "string" },
                "key": { "type": "string" },
                "up": { "type": "boolean" },
                "window": { "type": "string", "description": "The application window to read and act in: its id, `main` for the main window (the default)." },
            },
            "required": ["action"],
        })),
    );
    vec![
        GeneratedTool {
            tool: snapshot,
            target: Target::Snapshot,
            wrapped: false,
        },
        GeneratedTool {
            tool: wait,
            target: Target::Wait,
            wrapped: false,
        },
        GeneratedTool {
            tool: ui,
            target: Target::Ui,
            wrapped: false,
        },
    ]
}

/// The tools for a `describe` reply (`{ "protocol", "capabilities" }`):
/// `snapshot`, `wait` and `ui`, then one per capability offered to agents
/// (`bridge.wait` is the `wait` tool). A name taken already gets a
/// number.
pub fn tools(describe: &Json) -> Vec<GeneratedTool> {
    let mut out = builtins();
    let mut taken: HashSet<String> = out.iter().map(|t| t.tool.name.to_string()).collect();
    // The `widget_*` tools' names are taken too.
    taken.extend(
        egui_inspection_tools::tools()
            .iter()
            .map(|t| widget_tool_name(t.name)),
    );
    let caps = describe["capabilities"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for cap in &caps {
        let Some(name) = cap["name"].as_str() else {
            continue;
        };
        if cap["agent"] == false || name == "bridge.wait" {
            continue;
        }
        let target = match cap["kind"].as_str() {
            Some("command") => Target::Command { id: name.into() },
            _ => match (
                cap["call"]["collector"].as_str(),
                cap["call"]["action"].as_str(),
            ) {
                (Some(c), Some(a)) => Target::Action {
                    collector: c.into(),
                    action: a.into(),
                },
                _ => continue,
            },
        };
        let base = tool_name(name);
        let mut tname = base.clone();
        let mut n = 2;
        while !taken.insert(tname.clone()) {
            tname = format!("{}_{n}", &base[..base.len().min(60)]);
            n += 1;
        }
        let mutates = cap["mutates"] != false;
        let mut description = cap["description"]
            .as_str()
            .filter(|d| !d.is_empty())
            .or_else(|| cap["title"].as_str())
            .unwrap_or(name)
            .to_string();
        description.push_str(&format!(" (`{name}`"));
        description.push_str(if mutates {
            ", changes the application)"
        } else {
            ", read-only)"
        });
        if matches!(target, Target::Command { .. }) {
            description.push_str(" Runs the command and answers its result.");
        }
        let (schema, wrapped) = input_schema(&cap["args"], &cap["example"]);
        // Destructive: the user confirms each run (`destructiveHint`).
        let destructive = mutates && cap["destructive"] == true;
        if destructive {
            description.push_str(" Destructive: the user confirms each run.");
        }
        // Destructive by its arguments: the user confirms those runs only;
        // the hint stays (some calls destroy).
        let rules = cap["destructive_if"].as_array().filter(|r| !r.is_empty());
        let destructive = destructive || (mutates && rules.is_some());
        if let Some(rules) = rules.filter(|_| mutates && cap["destructive"] != true) {
            let said: Vec<String> = rules
                .iter()
                .map(|r| format!("`{}` = {}", r["arg"].as_str().unwrap_or("?"), r["equals"]))
                .collect();
            description.push_str(&format!(
                " Destructive with {}: the user confirms those runs.",
                said.join(" or ")
            ));
        }
        // Only said when declared: an undeclared command is not known to
        // be harmless, and MCP's default (destructive) stays.
        // Private: its result carries places, people or file paths; an
        // agent the user withholds them from is refused it.
        if cap["private"] == true {
            description.push_str(
                " Gives private data (places, people, file paths): refused when the user withholds it.",
            );
        }
        let mut annotations = ToolAnnotations::new().read_only(!mutates);
        if destructive {
            annotations = annotations.destructive(true);
        }
        let mut tool = Tool::new(tname, description, object(schema)).with_annotations(annotations);
        if let Some(t) = cap["title"].as_str() {
            tool = tool.with_title(t);
        }
        if is_object_schema(&cap["result"]) {
            tool = tool.with_raw_output_schema(object(cap["result"].clone()));
        }
        out.push(GeneratedTool {
            tool,
            target,
            wrapped,
        });
    }
    out
}

/// The `widget_*` tools' name for one of `egui_inspection_tools`' tools:
/// `widget_` before it (`click` is `widget_click`), `widget_tree` as it
/// is.
pub fn widget_tool_name(name: &str) -> String {
    if name.starts_with("widget_") {
        name.to_string()
    } else {
        format!("widget_{name}")
    }
}

/// The `egui_inspection_tools` tool a `widget_*` tool serves.
fn widget_tool(name: &str) -> Option<&'static ToolDef> {
    egui_inspection_tools::tools()
        .iter()
        .find(|t| widget_tool_name(t.name) == name)
}

/// What every `widget_*` tool's description ends with.
const WIDGET_NOTE: &str = "\nServed by egui-pop over the agent socket's `inspect` (the main \
    window): needs the bridge on and the agent's `ui` and private grants, and is refused while \
    the user answers a confirmation or a pairing. For testing by pointer and pixels; the \
    application's own tools (its commands) are the way to act on it. Tools named here without \
    `widget_` (`click`, `get_widget`) are this server's `widget_*` ones; `batch` steps take \
    either name.";

/// The `widget_*` tools: Rerun's agent tools (`egui_inspection_tools`, by
/// Lucas Meurer and Emil Ernerfeldt,
/// [rerun-io/egui_crates](https://github.com/rerun-io/egui_crates)) over
/// the socket's `inspect`: `widget_tree`, `widget_get_widget`,
/// `widget_screenshot`, `widget_wait_for`, `widget_click`,
/// `widget_hover`, `widget_scroll`, `widget_drag`, `widget_type_text`,
/// `widget_press_key`, `widget_resize` and `widget_batch`. They act on the
/// accessibility tree, the pointer and the keyboard, as a person would.
pub fn widget_tools() -> Vec<Tool> {
    egui_inspection_tools::tools()
        .iter()
        .map(|def| {
            let annotations = ToolAnnotations::new()
                .read_only(def.effect.is_read_only())
                .destructive(def.effect == Effect::Destructive)
                .idempotent(matches!(def.effect, Effect::ReadOnly | Effect::Idempotent))
                .open_world(false);
            let mut tool = Tool::new(
                widget_tool_name(def.name),
                format!("{}{WIDGET_NOTE}", def.description),
                Arc::new(def.input_schema.clone()),
            )
            .with_title(def.title)
            .with_annotations(annotations);
            if let Some(out) = &def.output_schema {
                tool = tool.with_raw_output_schema(Arc::new(out.clone()));
            }
            tool
        })
        .collect()
}

/// `egui_inspection_tools`' [`Transport`] over the socket's `inspect`:
/// one request, one connection, on a blocking thread.
struct Inspect<B: Backend>(Arc<B>);

impl<B: Backend> Transport for Inspect<B> {
    fn request(&self, request: Request) -> BoxFuture<'_, Result<Response, String>> {
        let backend = self.0.clone();
        Box::pin(async move {
            let params = request_to_json(&request);
            let result = tokio::task::spawn_blocking(move || backend.request("inspect", params))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| format!("{} ({})", e.message, e.code))?;
            response_from_json(&result)
        })
    }
}

/// The timer `widget_wait_for` polls with.
fn sleep(duration: std::time::Duration) -> BoxFuture<'static, ()> {
    Box::pin(tokio::time::sleep(duration))
}

/// A `batch`'s steps named as `widget_*` tools, by the names
/// `egui_inspection_tools` knows them by.
fn batch_names(arguments: &mut JsonObject) {
    let Some(Json::Array(actions)) = arguments.get_mut("actions") else {
        return;
    };
    for action in actions {
        if let Some(name) = action.get("name").and_then(Json::as_str)
            && let Some(def) = widget_tool(name)
        {
            action["name"] = json!(def.name);
        }
    }
}

/// A `widget_*` tool's output as an MCP result: its text, its images
/// (base64), its structured value.
fn widget_result(output: ToolOutput) -> CallToolResult {
    let content = output
        .content
        .into_iter()
        .map(|c| match c {
            Content::Text(text) => ContentBlock::text(text),
            Content::Image { bytes, mime_type } => {
                ContentBlock::image(BASE64.encode(bytes), mime_type)
            }
        })
        .collect();
    if output.is_error {
        return CallToolResult::error(content);
    }
    let mut result = CallToolResult::success(content);
    result.structured_content = output.structured;
    result
}

/// The MCP server: tools from the application's `describe`, calls to its
/// socket.
pub struct McpBridge<B: Backend> {
    backend: Arc<B>,
    tools: Mutex<Vec<GeneratedTool>>,
}

impl<B: Backend> std::fmt::Debug for McpBridge<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpBridge").finish_non_exhaustive()
    }
}

impl<B: Backend> McpBridge<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend: Arc::new(backend),
            tools: Mutex::new(Vec::new()),
        }
    }

    async fn request(&self, method: &'static str, params: Json) -> Result<Json, RpcError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || backend.request(method, params))
            .await
            .unwrap_or_else(|e| Err(RpcError::new(RpcError::NO_ANSWER, e.to_string())))
    }

    /// The tools now: `describe` asked again, so a capability added since
    /// is there. When `describe` is refused (the bridge is off) or the
    /// application does not answer, only `snapshot`, `wait` and `ui`.
    async fn refresh(&self) -> Vec<GeneratedTool> {
        let tools = match self.request("describe", json!({})).await {
            Ok(d) => tools(&d),
            Err(e) => {
                log::warn!(target: "egui_pop_agent", "describe: {}", e.message);
                builtins()
            }
        };
        if let Ok(mut t) = self.tools.lock() {
            *t = tools.clone();
        }
        tools
    }

    async fn find(&self, name: &str) -> Option<GeneratedTool> {
        let known = self
            .tools
            .lock()
            .ok()
            .and_then(|t| t.iter().find(|t| t.tool.name == name).cloned());
        match known {
            Some(t) => Some(t),
            None => self
                .refresh()
                .await
                .into_iter()
                .find(|t| t.tool.name == name),
        }
    }

    /// Runs a tool: the socket's answer as the tool's result, a refusal
    /// or an error as an error result the model reads.
    pub async fn run(&self, name: &str, arguments: JsonObject) -> CallToolResult {
        if let Some(def) = widget_tool(name) {
            let mut arguments = arguments;
            if def.name == "batch" {
                batch_names(&mut arguments);
            }
            let transport = Inspect(self.backend.clone());
            let output = egui_inspection_tools::call(&transport, &sleep, def.name, arguments).await;
            return widget_result(output);
        }
        let Some(t) = self.find(name).await else {
            return error_result(format!("no tool `{name}`"), None);
        };
        let mut args = Json::Object(arguments);
        if t.wrapped {
            args = args.get("args").cloned().unwrap_or(Json::Null);
        }
        let structured = t.tool.output_schema.is_some();
        match t.target {
            Target::Snapshot => match self.request("snapshot", args).await {
                Ok(v) => CallToolResult::success(vec![ContentBlock::text(v.to_string())]),
                Err(e) => rpc_error(e),
            },
            Target::Wait => match self.request("wait", args).await {
                Ok(v) => CallToolResult::structured(v),
                Err(e) => rpc_error(e),
            },
            Target::Ui => match self.request("ui", args).await {
                Ok(v) => CallToolResult::success(vec![ContentBlock::text(v.to_string())]),
                Err(e) => rpc_error(e),
            },
            Target::Action { collector, action } => {
                let params = json!({ "collector": collector, "action": action, "args": args });
                match self.request("call", params).await {
                    Ok(f) => feedback(f, structured),
                    Err(e) => rpc_error(e),
                }
            }
            Target::Command { id } => self.run_command(&id, args).await,
        }
    }

    /// A command is queued for the next frame: run it, then wait for its
    /// `command.done` (or `command.failed`) on the journal.
    async fn run_command(&self, id: &str, args: Json) -> CallToolResult {
        // The newest event now: the command's result comes after it.
        let mark = json!({ "event": "egui_pop_agent.mcp.mark", "deadline_ms": 0 });
        let mut since = match self.request("wait", mark).await {
            Ok(w) => w["last"].as_u64().unwrap_or(0),
            Err(e) => return rpc_error(e),
        };
        let params = json!({
            "collector": "commands",
            "action": "run",
            "args": { "id": id, "args": args },
        });
        let f = match self.request("call", params).await {
            Ok(f) => f,
            Err(e) => return rpc_error(e),
        };
        if f["ok"] != true {
            return feedback(f, false);
        }
        loop {
            let until = json!({
                "any": [{ "event": "command.done" }, { "event": "command.failed" }],
                "since": since,
                "deadline_ms": COMMAND_DEADLINE_MS,
            });
            let w = match self.request("wait", until).await {
                Ok(w) => w,
                Err(e) => return rpc_error(e),
            };
            if w["met"] != true {
                return error_result(
                    format!("`{id}` was queued but not done within {COMMAND_DEADLINE_MS} ms"),
                    Some(f),
                );
            }
            let event = &w["event"];
            if event["data"]["command"] == id {
                let data = &event["data"];
                return if event["name"] == "command.done" {
                    CallToolResult::success(vec![ContentBlock::text(
                        data.get("result").unwrap_or(&Json::Null).to_string(),
                    )])
                } else {
                    error_result(format!("`{id}` failed"), Some(data.clone()))
                };
            }
            // Another command's: wait on, after it.
            since = event["seq"].as_u64().unwrap_or(since + 1);
        }
    }
}

/// The bridge's feedback block as a tool result: `data` when it is `ok`
/// (structured when the tool declares an output schema), the block
/// otherwise.
fn feedback(f: Json, structured: bool) -> CallToolResult {
    if f["ok"] != true {
        let message = f["message"].as_str().unwrap_or("refused").to_string();
        return error_result(message, Some(f));
    }
    let data = f.get("data").cloned().unwrap_or(Json::Null);
    if structured && data.is_object() {
        CallToolResult::structured(data)
    } else {
        CallToolResult::success(vec![ContentBlock::text(data.to_string())])
    }
}

fn error_result(message: String, detail: Option<Json>) -> CallToolResult {
    let mut content = vec![ContentBlock::text(message)];
    if let Some(d) = detail {
        content.push(ContentBlock::text(d.to_string()));
    }
    CallToolResult::error(content)
}

fn rpc_error(e: RpcError) -> CallToolResult {
    error_result(format!("{} ({})", e.message, e.code), e.data)
}

impl<B: Backend> ServerHandler for McpBridge<B> {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "egui-pop-agent",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Drives a running egui-pop application through its agent socket. The tools are \
                 its capabilities; `snapshot` reads its state, `wait` waits for an event or for \
                 it to settle, `ui` reads the interface's tree and clicks, hovers, right-clicks, types and scrolls. Calls that change the application are refused unless it offers \
                 them. The `widget_*` tools drive the main window by pointer and pixels \
                 (screenshots, drags, key presses, batches, waits), for testing it as a person \
                 would.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = self.refresh().await;
        Ok(ListToolsResult::with_all_items(
            tools
                .into_iter()
                .map(|t| t.tool)
                .chain(widget_tools())
                .collect(),
        ))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name: Cow<'_, str> = request.name;
        Ok(self
            .run(&name, request.arguments.unwrap_or_default())
            .await
            .into())
    }
}
