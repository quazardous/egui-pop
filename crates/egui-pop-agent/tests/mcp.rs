//! The MCP side (feature `mcp`): tools made from a `describe` reply, and
//! an application in a kittest harness driven through its socket by the
//! MCP bridge, the client speaking MCP over an in-memory pipe.
#![cfg(all(unix, feature = "mcp"))]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_pop_agent::grants::{AgentGrants, AgentGrantsExt as _, Grant};
use egui_pop_agent::mcp::{McpBridge, SocketClient, Target, tool_name, tools};
use egui_pop_agent::{AgentSocket, AgentSocketExt as _};
use egui_pop_api::about::{AboutPlugin, AppInfo};
use egui_pop_api::model::commands::CommandSpec;
use egui_pop_api::model::ids::{CommandId, PluginId};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use rmcp::ServiceExt as _;
use serde_json::{Value as Json, json};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

#[test]
fn names_are_sanitised() {
    assert_eq!(tool_name("about.read"), "about_read");
    assert_eq!(tool_name("a b/c-d_e"), "a_b_c-d_e");
    assert_eq!(tool_name(""), "tool");
    assert_eq!(tool_name(&"x".repeat(100)).len(), 64);
}

#[test]
fn a_command_destructive_by_its_arguments_keeps_the_hint_and_names_its_rules() {
    let describe = json!({
        "protocol": 1,
        "capabilities": [{
            "kind": "command", "name": "library.export", "title": "Export",
            "description": "Exports photos.", "mutates": true, "agent": true,
            "destructive": false,
            "destructive_if": [{ "arg": "clash", "equals": "overwrite" }],
            "args": null, "example": null, "result": null,
            "call": { "collector": "commands", "action": "run", "id": "library.export" },
        }],
    });
    let generated = tools(&describe);
    let export = generated
        .iter()
        .find(|t| t.tool.name == "library_export")
        .unwrap();
    let a = export.tool.annotations.as_ref().unwrap();
    assert_eq!(a.destructive_hint, Some(true));
    let d = export.tool.description.as_deref().unwrap();
    assert!(d.contains("`clash` = \"overwrite\""), "{d}");
}

#[test]
fn tools_from_a_describe_fixture() {
    let describe = json!({
        "protocol": 1,
        "capabilities": [
            {
                "kind": "action", "name": "viewer.zoom", "description": "Zooms the viewer.",
                "mutates": true, "agent": true,
                "args": { "type": "object", "properties": { "factor": { "type": "number" } } },
                "example": null,
                "result": { "type": "object", "properties": { "zoom": { "type": "number" } } },
                "call": { "collector": "viewer", "action": "zoom" },
            },
            {
                "kind": "action", "name": "log.tail", "description": "The log's last lines.",
                "mutates": false, "agent": true, "args": null,
                "example": { "lines": 10, "target": "app" }, "result": null,
                "call": { "collector": "log", "action": "tail" },
            },
            {
                "kind": "action", "name": "secret.peek", "description": "User only.",
                "mutates": false, "agent": false, "args": null, "example": null, "result": null,
                "call": { "collector": "secret", "action": "peek" },
            },
            {
                "kind": "action", "name": "bridge.wait", "description": "Non-blocking wait.",
                "mutates": false, "agent": true, "args": null, "example": null, "result": null,
                "call": { "collector": "bridge", "action": "wait" },
            },
            {
                "kind": "command", "name": "about.read", "title": "About",
                "description": "", "mutates": false, "agent": true, "private": true,
                "args": null, "example": null, "result": null,
                "call": { "collector": "commands", "action": "run", "id": "about.read" },
            },
            {
                "kind": "command", "name": "file.open", "title": "Open",
                "description": "Opens a file.", "mutates": true, "agent": true,
                "args": { "type": "string" }, "example": null, "result": null,
                "call": { "collector": "commands", "action": "run", "id": "file.open" },
            },
            {
                "kind": "command", "name": "about_read", "title": "Clash",
                "description": "A name that clashes once sanitised.", "mutates": false,
                "agent": true, "args": null, "example": null, "result": null,
                "call": { "collector": "commands", "action": "run", "id": "about_read" },
            },
        ],
    });
    let generated = tools(&describe);
    let names: Vec<&str> = generated.iter().map(|t| &*t.tool.name).collect();
    assert_eq!(
        names,
        [
            "snapshot",
            "wait",
            "ui",
            "viewer_zoom",
            "log_tail",
            "about_read",
            "file_open",
            "about_read_2"
        ]
    );
    let by = |n: &str| generated.iter().find(|t| t.tool.name == n).unwrap();

    // A schema of an object is the input schema; its result schema is the
    // output schema.
    let zoom = by("viewer_zoom");
    assert_eq!(
        zoom.target,
        Target::Action {
            collector: "viewer".into(),
            action: "zoom".into()
        }
    );
    assert_eq!(
        zoom.tool.input_schema["properties"]["factor"]["type"],
        "number"
    );
    assert_eq!(
        zoom.tool.output_schema.as_ref().unwrap()["properties"]["zoom"]["type"],
        "number"
    );
    assert!(!zoom.wrapped);
    assert_eq!(
        zoom.tool.annotations.as_ref().unwrap().read_only_hint,
        Some(false)
    );

    // Without a schema, an object schema from the example.
    let tail = by("log_tail");
    assert_eq!(tail.tool.input_schema["type"], "object");
    assert_eq!(
        tail.tool.input_schema["properties"]["lines"]["type"],
        "integer"
    );
    assert_eq!(
        tail.tool.input_schema["properties"]["target"]["type"],
        "string"
    );
    assert!(tail.tool.output_schema.is_none());
    assert_eq!(
        tail.tool.annotations.as_ref().unwrap().read_only_hint,
        Some(true)
    );

    // A command: its title, a description, run through commands.run.
    let about = by("about_read");
    assert_eq!(
        about.target,
        Target::Command {
            id: "about.read".into()
        }
    );
    assert_eq!(about.tool.title.as_deref(), Some("About"));
    assert!(
        about
            .tool
            .description
            .as_ref()
            .unwrap()
            .contains("about.read")
    );
    // Private: the description says so; another command's does not.
    assert!(
        about
            .tool
            .description
            .as_ref()
            .unwrap()
            .contains("private data")
    );
    assert!(
        !by("file_open")
            .tool
            .description
            .as_ref()
            .unwrap()
            .contains("private data")
    );

    // A schema that is not an object's is taken as `args`.
    let open = by("file_open");
    assert!(open.wrapped);
    assert_eq!(
        open.tool.input_schema["properties"]["args"]["type"],
        "string"
    );
    assert_eq!(open.tool.input_schema["required"], json!(["args"]));

    // The built-ins.
    assert_eq!(by("wait").target, Target::Wait);
    assert_eq!(by("snapshot").target, Target::Snapshot);
}

/// A command that writes, offered to agents: what read-only refuses.
struct Writes;

impl Plugin for Writes {
    fn id(&self) -> PluginId {
        PluginId::from_static("test.writes")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(
                CommandId::from_static("test.write"),
                "Write something",
                "Test",
            )
            .description("Changes the application (a test's)."),
            |_| Ok(json!("written")),
        )?;
        // Read-only, so a read-only bridge runs it: its schema is what
        // refuses.
        ctx.command(
            CommandSpec::new(CommandId::from_static("test.zoom"), "Zoom", "Test")
                .description("Reads at a factor (a test's).")
                .read_only()
                .schema(zoom_schema()),
            |c| Ok(c.args["factor"].clone()),
        )?;
        // Private: a file path, read or failed.
        ctx.command(
            CommandSpec::new(HOME, "Home folder", "Test")
                .description("The library's folder (a test's).")
                .read_only()
                .private(),
            |c| match c.args.get("fail") {
                Some(_) => Err(format!("no such folder: {SECRET}")),
                None => Ok(json!(SECRET)),
            },
        )?;
        Ok(())
    }
}

/// A private command: its result is a file path.
const HOME: CommandId = CommandId::from_static("test.home");
/// The private data `test.home` answers.
const SECRET: &str = "/home/someone/Pictures";

fn zoom_schema() -> Json {
    json!({
        "type": "object",
        "required": ["factor"],
        "properties": { "factor": { "type": "number", "maximum": 8 } }
    })
}

fn private_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("egui-pop-mcp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}

fn app(socket: &Path) -> Harness<'static, App> {
    app_with(socket, AgentGrants::new())
}

fn app_with(socket: &Path, grants: AgentGrants) -> Harness<'static, App> {
    let builder = App::builder()
        .agent_grants(grants)
        .plugins(DefaultPlugins)
        .plugin(AboutPlugin::new(AppInfo::new("MCP test", "1.0.0")))
        .plugin(Writes)
        .app_id("org.example.McpTest")
        .agent_socket(AgentSocket::at(socket));
    let mut h = egui_pop_test::app(builder, egui::vec2(800.0, 600.0));
    h.step();
    h
}

/// An MCP client over an in-memory pipe, raw JSON-RPC lines.
struct Client {
    read: BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
    write: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    next: u64,
}

impl Client {
    async fn send(&mut self, message: Json) {
        let line = format!("{message}\n");
        self.write.write_all(line.as_bytes()).await.unwrap();
        self.write.flush().await.unwrap();
    }

    async fn request(&mut self, method: &str, params: Json) -> Json {
        let id = self.next;
        self.next += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        loop {
            let mut line = String::new();
            let n = tokio::time::timeout(Duration::from_secs(20), self.read.read_line(&mut line))
                .await
                .expect("an answer within 20 s")
                .unwrap();
            assert!(n > 0, "the server closed");
            let message: Json = serde_json::from_str(&line).expect("JSON");
            if message["id"] == id {
                assert!(message.get("error").is_none(), "{method}: {message}");
                return message["result"].clone();
            }
        }
    }
}

#[test]
fn mcp_lists_and_calls_through_the_socket() {
    let dir = private_dir("e2e");
    let path = dir.join("app.sock");
    let mut h = app(&path);
    let p = path.clone();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let (client, server) = tokio::io::duplex(1 << 20);
            let serving = tokio::spawn(async move {
                let running = McpBridge::new(SocketClient::new(p))
                    .serve(server)
                    .await
                    .expect("the handshake");
                let _ = running.waiting().await;
            });
            let (read, write) = tokio::io::split(client);
            let mut c = Client {
                read: BufReader::new(read),
                write,
                next: 1,
            };
            let init = c
                .request(
                    "initialize",
                    json!({
                        "protocolVersion": "2025-06-18",
                        "capabilities": {},
                        "clientInfo": { "name": "test", "version": "0" },
                    }),
                )
                .await;
            assert!(init["capabilities"].get("tools").is_some(), "{init}");
            c.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                .await;

            let listed = c.request("tools/list", json!({})).await;
            let names: Vec<&str> = listed["tools"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|t| t["name"].as_str())
                .collect();
            for n in ["snapshot", "wait", "ui", "about_read", "test_write"] {
                assert!(names.contains(&n), "{n} in {names:?}");
            }

            // A command's schema is its tool's input schema, and it is
            // enforced: a call it refuses is an error the model reads.
            let zoom = listed["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == "test_zoom")
                .cloned()
                .unwrap();
            assert_eq!(zoom["inputSchema"], zoom_schema(), "{zoom}");
            let bad = c
                .request("tools/call", json!({ "name": "test_zoom", "arguments": { "factor": 9 } }))
                .await;
            assert_eq!(bad["isError"], true, "{bad}");
            assert!(bad.to_string().contains("pop.command.args"), "{bad}");
            let good = c
                .request("tools/call", json!({ "name": "test_zoom", "arguments": { "factor": 2 } }))
                .await;
            assert_ne!(good["isError"], true, "{good}");

            // A read-only command: run, and its result answered.
            let about = c
                .request("tools/call", json!({ "name": "about_read", "arguments": {} }))
                .await;
            assert_ne!(about["isError"], true, "{about}");
            let text = about["content"][0]["text"].as_str().unwrap();
            assert!(text.contains("MCP test"), "{about}");

            // A write, refused in read-only mode: an error the model reads.
            let write = c
                .request("tools/call", json!({ "name": "test_write", "arguments": {} }))
                .await;
            assert_eq!(write["isError"], true, "{write}");
            assert!(write.to_string().contains("read-only"), "{write}");

            let snap = c
                .request(
                    "tools/call",
                    json!({ "name": "snapshot", "arguments": { "collector": "commands" } }),
                )
                .await;
            assert_ne!(snap["isError"], true, "{snap}");

            let settled = c
                .request(
                    "tools/call",
                    json!({ "name": "wait", "arguments": { "settled": true, "deadline_ms": 10_000 } }),
                )
                .await;
            assert_eq!(settled["structuredContent"]["met"], true, "{settled}");
            serving.abort();
        });
    });
    let start = Instant::now();
    while !thread.is_finished() {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the client did not finish"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    thread.join().expect("the client passes");
}

/// Runs `client` against the MCP bridge on the socket at `path`, on its
/// own thread, while stepping the harness's frames.
fn with_mcp<F>(
    h: &mut Harness<'static, App>,
    path: &Path,
    client: impl FnOnce(Client) -> F + Send + 'static,
) where
    F: std::future::Future<Output = ()> + 'static,
{
    let p = path.to_path_buf();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let (client_end, server) = tokio::io::duplex(1 << 20);
            let serving = tokio::spawn(async move {
                let running = McpBridge::new(SocketClient::new(p))
                    .serve(server)
                    .await
                    .expect("the handshake");
                let _ = running.waiting().await;
            });
            let (read, write) = tokio::io::split(client_end);
            let mut c = Client {
                read: BufReader::new(read),
                write,
                next: 1,
            };
            c.request(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "test", "version": "0" },
                }),
            )
            .await;
            c.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
                .await;
            client(c).await;
            serving.abort();
        });
    });
    let start = Instant::now();
    while !thread.is_finished() {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the client did not finish"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    thread.join().expect("the client passes");
}

/// The user runs `test.home` twice, once failing: its result and its
/// error are on the journal and in the run history.
fn user_runs_home(h: &mut Harness<'static, App>) {
    use egui_pop_api::model::commands::Origin;
    let ok = h
        .state_mut()
        .run(&HOME, Json::Null, Origin::User)
        .now()
        .unwrap();
    assert_eq!(ok, Ok(json!(SECRET)));
    let failed = h
        .state_mut()
        .run(&HOME, json!({ "fail": SECRET }), Origin::User)
        .now()
        .unwrap();
    assert!(failed.is_err());
    h.step();
}

#[test]
fn a_granted_agent_reads_a_private_commands_result_over_mcp() {
    let dir = private_dir("private-granted");
    let path = dir.join("app.sock");
    // The default grants show private data.
    let mut h = app(&path);
    user_runs_home(&mut h);
    with_mcp(&mut h, &path, |mut c| async move {
        // Queued, then read back from the journal's `command.done`.
        let home = c
            .request(
                "tools/call",
                json!({ "name": "test_home", "arguments": {} }),
            )
            .await;
        assert_ne!(home["isError"], true, "{home}");
        assert_eq!(
            home["content"][0]["text"],
            json!(SECRET).to_string(),
            "{home}"
        );
        // The journal and the run history, in clear.
        let journal = c
            .request(
                "tools/call",
                json!({ "name": "snapshot", "arguments": { "collector": "journal" } }),
            )
            .await;
        assert!(journal.to_string().contains(SECRET), "{journal}");
        let commands = c
            .request(
                "tools/call",
                json!({ "name": "snapshot", "arguments": { "collector": "commands" } }),
            )
            .await;
        assert!(commands.to_string().contains(SECRET), "{commands}");
    });
}

#[test]
fn an_agent_without_private_data_is_refused_a_private_command_and_reads_it_redacted() {
    let dir = private_dir("private-withheld");
    let path = dir.join("app.sock");
    let mut h = app_with(
        &path,
        AgentGrants::new().unpaired(Grant::read_only().private(false)),
    );
    user_runs_home(&mut h);
    with_mcp(&mut h, &path, |mut c| async move {
        // Refused before it runs: no result to read back.
        let home = c
            .request(
                "tools/call",
                json!({ "name": "test_home", "arguments": {} }),
            )
            .await;
        assert_eq!(home["isError"], true, "{home}");
        assert!(home.to_string().contains("private data"), "{home}");
        assert!(!home.to_string().contains(SECRET), "{home}");
        // What the user ran is on the journal, redacted for this agent.
        for collector in ["journal", "commands"] {
            let snap = c
                .request(
                    "tools/call",
                    json!({ "name": "snapshot", "arguments": { "collector": collector } }),
                )
                .await;
            let text = snap.to_string();
            assert!(!text.contains(SECRET), "{collector}: {snap}");
            assert!(text.contains("<private>"), "{collector}: {snap}");
        }
        let failed = c
            .request(
                "tools/call",
                json!({ "name": "wait", "arguments": { "event": "command.failed", "since": 0, "deadline_ms": 10_000 } }),
            )
            .await;
        let event = &failed["structuredContent"]["event"];
        assert_eq!(event["data"]["command"], "test.home", "{failed}");
        assert_eq!(event["data"]["error"], "<private>", "{failed}");
        assert_eq!(event["data"]["private"], true, "{failed}");
    });
}
