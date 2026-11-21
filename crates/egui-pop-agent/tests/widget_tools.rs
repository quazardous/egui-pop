//! The `widget_*` MCP tools (Rerun's `egui_inspection_tools` over the
//! socket's `inspect`): an application in a kittest harness drawn on the
//! process's wgpu device, the MCP bridge calling its socket from a thread
//! while the test steps frames.
#![cfg(all(unix, feature = "mcp"))]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_pop_agent::grants::{AgentGrants, AgentGrantsExt as _, Grant};
use egui_pop_agent::mcp::{Backend as _, McpBridge, SocketClient, widget_tools};
use egui_pop_agent::{AgentSocket, AgentSocketExt as _};
use egui_pop_api::about::{AboutPlugin, AppInfo};
use egui_pop_api::core::debug::BridgeMode;
use egui_pop_api::model::commands::CommandSpec;
use egui_pop_api::model::ids::{CommandId, PluginId};
use egui_pop_api::{App, AppBuilder, Ctx, DefaultPlugins, Plugin, PluginError};
use rmcp::model::{CallToolResult, ContentBlock, JsonObject};
use serde_json::{Value as Json, json};

/// A destructive command offered to agents: what the user confirms.
const PURGE: CommandId = CommandId::from_static("test.purge");

struct Purges;

impl Plugin for Purges {
    fn id(&self) -> PluginId {
        PluginId::from_static("test.purges")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(PURGE, "Purge something", "Test")
                .description("Discards what cannot be had back (a test's).")
                .destructive(),
            |_| Ok(json!("purged")),
        )?;
        Ok(())
    }
}

fn private_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("egui-pop-widget-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}

/// An application serving its socket at `socket`, drawn on the process's
/// wgpu device, so a screenshot has pixels.
fn rendered(builder: AppBuilder, socket: &Path) -> Harness<'static, App> {
    use egui_pop_test::SharedGpu as _;
    egui_pop_api::files::refuse_system_chooser();
    let app = builder
        .plugins(DefaultPlugins)
        .plugin(AboutPlugin::new(AppInfo::new("Widget tools test", "1.0.0")))
        .plugin(Purges)
        .app_id("org.example.WidgetToolsTest")
        .agent_socket(AgentSocket::at(socket))
        .build(&egui::Context::default())
        .expect("the application builds");
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .shared_wgpu()
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.step();
    h
}

fn args(v: Json) -> JsonObject {
    match v {
        Json::Object(o) => o,
        other => panic!("not an object: {other}"),
    }
}

/// Runs `client` with an MCP bridge on the socket at `path`, on its own
/// thread, while stepping the harness's frames; its result.
fn with_bridge<T, F>(
    h: &mut Harness<'static, App>,
    path: &Path,
    client: impl FnOnce(McpBridge<SocketClient>) -> F + Send + 'static,
) -> T
where
    T: Send + 'static,
    F: std::future::Future<Output = T> + 'static,
{
    let p = path.to_path_buf();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(client(McpBridge::new(SocketClient::new(p))))
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
    thread.join().expect("the client passes")
}

fn ok(name: &str, r: &CallToolResult) {
    assert_ne!(r.is_error, Some(true), "{name}: {r:?}");
}

/// The text of a result, all its text items joined.
fn text(r: &CallToolResult) -> String {
    serde_json::to_string(&r.content).unwrap()
}

fn images(r: &CallToolResult) -> Vec<(Vec<u8>, String)> {
    use base64::Engine as _;
    r.content
        .iter()
        .filter_map(|c| match c {
            ContentBlock::Image(i) => Some((
                base64::engine::general_purpose::STANDARD
                    .decode(&i.data)
                    .unwrap(),
                i.mime_type.clone(),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn the_widget_tools_are_named_widget_and_annotated() {
    let tools = widget_tools();
    let names: Vec<&str> = tools.iter().map(|t| &*t.name).collect();
    for n in [
        "widget_tree",
        "widget_get_widget",
        "widget_screenshot",
        "widget_wait_for",
        "widget_click",
        "widget_hover",
        "widget_scroll",
        "widget_drag",
        "widget_type_text",
        "widget_press_key",
        "widget_resize",
        "widget_batch",
    ] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
    let by = |n: &str| tools.iter().find(|t| t.name == n).unwrap();
    let shot = by("widget_screenshot").annotations.clone().unwrap();
    assert_eq!(shot.read_only_hint, Some(true));
    let click = by("widget_click").annotations.clone().unwrap();
    assert_eq!(click.read_only_hint, Some(false));
    assert_eq!(click.destructive_hint, Some(true));
    assert!(by("widget_tree").output_schema.is_some());
    let d = by("widget_click").description.as_deref().unwrap();
    assert!(d.contains("`ui` and private grants"), "{d}");
}

#[test]
fn a_capability_named_like_a_widget_tool_gets_a_number() {
    let describe = json!({
        "protocol": 1,
        "capabilities": [{
            "kind": "action", "name": "widget.click", "description": "Clashes.",
            "mutates": false, "agent": true, "args": null, "example": null, "result": null,
            "call": { "collector": "widget", "action": "click" },
        }],
    });
    let names: Vec<String> = egui_pop_agent::mcp::tools(&describe)
        .into_iter()
        .map(|t| t.tool.name.to_string())
        .collect();
    assert!(names.contains(&"widget_click_2".to_string()), "{names:?}");
}

#[test]
fn the_widget_tools_drive_the_window_under_full_grants() {
    let dir = private_dir("full");
    let path = dir.join("app.sock");
    let builder = App::builder()
        .debug_bridge(BridgeMode::On)
        .agent_grants(AgentGrants::new().unpaired(Grant::full()));
    let mut h = rendered(builder, &path);
    let results = with_bridge(&mut h, &path, |b| async move {
        let mut out = Vec::new();
        for (name, a) in [
            ("widget_tree", json!({ "role": "Button" })),
            ("widget_screenshot", json!({})),
            ("widget_hover", json!({ "pos": { "x": 400.0, "y": 300.0 } })),
            ("widget_click", json!({ "pos": { "x": 5.0, "y": 5.0 } })),
            (
                "widget_scroll",
                json!({ "pos": { "x": 400.0, "y": 300.0 }, "delta": { "x": 0.0, "y": 20.0 } }),
            ),
            (
                "widget_drag",
                json!({ "start": { "pos": { "x": 300.0, "y": 300.0 } }, "end": { "pos": { "x": 320.0, "y": 310.0 } } }),
            ),
            ("widget_type_text", json!({ "text": "abc" })),
            ("widget_press_key", json!({ "key": "Escape" })),
            (
                "widget_wait_for",
                json!({ "min_steps": 2, "timeout_secs": 10 }),
            ),
            ("widget_resize", json!({ "width": 800, "height": 600 })),
            (
                "widget_batch",
                json!({ "actions": [
                    { "name": "widget_click", "args": { "pos": { "x": 5.0, "y": 5.0 } } },
                    { "name": "press_key", "args": { "key": "Escape" } },
                    { "name": "widget_tree", "args": { "limit": 5 } },
                    { "name": "widget_screenshot", "args": {} },
                ] }),
            ),
        ] {
            out.push((name, b.run(name, args(a)).await));
        }
        out
    });
    for (name, r) in &results {
        ok(name, r);
    }
    let by = |n: &str| &results.iter().find(|(m, _)| *m == n).unwrap().1;
    // The tree: structured, the buttons the window shows.
    let tree = by("widget_tree");
    let nodes = &tree.structured_content.as_ref().expect("structured")["nodes"];
    assert!(
        nodes.as_array().is_some_and(|n| !n.is_empty()),
        "no button in {tree:?}"
    );
    // The screenshot: a PNG of the window, at 1 pixel per point.
    let shot = images(by("widget_screenshot"));
    assert_eq!(shot.len(), 1, "one image");
    assert_eq!(shot[0].1, "image/png");
    assert_eq!(&shot[0].0[1..4], b"PNG", "a PNG");
    // The batch: each step's text, then the screenshot's image.
    let batch = by("widget_batch");
    let said = text(batch);
    assert!(said.contains("nodes"), "the tree step in {said}");
    assert_eq!(images(batch).len(), 1, "the screenshot step's image");
}

#[test]
fn the_widget_tools_are_refused_read_only_and_without_the_ui_or_private_grant() {
    for (name, builder, says) in [
        (
            "readonly",
            App::builder()
                .debug_bridge(BridgeMode::ReadOnly)
                .agent_grants(AgentGrants::new().unpaired(Grant::full())),
            "read-only",
        ),
        (
            "no-ui",
            App::builder()
                .debug_bridge(BridgeMode::On)
                .agent_grants(AgentGrants::new().unpaired(Grant::full().ui(false))),
            "no `ui`",
        ),
        (
            "no-private",
            App::builder()
                .debug_bridge(BridgeMode::On)
                .agent_grants(AgentGrants::new().unpaired(Grant::full().private(false))),
            "private",
        ),
    ] {
        let dir = private_dir(name);
        let path = dir.join("app.sock");
        let mut h = rendered(builder, &path);
        let results = with_bridge(&mut h, &path, |b| async move {
            let mut out = Vec::new();
            for (tool, a) in [
                ("widget_tree", json!({})),
                ("widget_screenshot", json!({})),
                ("widget_click", json!({ "pos": { "x": 5.0, "y": 5.0 } })),
                ("widget_press_key", json!({ "key": "Enter" })),
            ] {
                out.push((tool, b.run(tool, args(a)).await));
            }
            out
        });
        for (tool, r) in &results {
            assert_eq!(r.is_error, Some(true), "{name} {tool}: {r:?}");
            let t = text(r);
            assert!(t.contains(says), "{name} {tool}: {t}");
        }
    }
}

/// Steps frames until a confirmation of the agent socket shows; its id.
fn confirmation_shown(h: &mut Harness<'static, App>) -> u64 {
    use egui_pop_api::dialogs::{DIALOGS, Dialog};
    let start = Instant::now();
    loop {
        h.step();
        let d = h.state().handle().service(&DIALOGS).unwrap();
        if let Some((id, Dialog::Confirm(c))) = d.current()
            && c.command == egui_pop_agent::CMD_CONFIRM
        {
            return id;
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "no confirmation showed"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn the_widget_tools_are_refused_while_the_user_answers_a_confirmation() {
    let dir = private_dir("input-lock");
    let path = dir.join("app.sock");
    let builder = App::builder()
        .debug_bridge(BridgeMode::On)
        .agent_grants(AgentGrants::new().allow_unpaired_writes());
    let mut h = rendered(builder, &path);
    // An agent asks for the purge: the user is asked to confirm.
    let p = path.clone();
    let asking = std::thread::spawn(move || {
        SocketClient::new(p).request(
            "call",
            json!({
                "collector": "commands",
                "action": "run",
                "args": { "id": "test.purge", "args": {} },
                "deadline_ms": 20_000,
            }),
        )
    });
    let dialog = confirmation_shown(&mut h);
    // Another agent tries to read the screen and to press Enter on the
    // confirmation, alone and in a batch: all refused.
    let results = with_bridge(&mut h, &path, |b| async move {
        let mut out = Vec::new();
        for (tool, a) in [
            ("widget_screenshot", json!({})),
            ("widget_press_key", json!({ "key": "Enter" })),
            (
                "widget_batch",
                json!({ "actions": [{ "name": "widget_press_key", "args": { "key": "Enter" } }] }),
            ),
        ] {
            out.push((tool, b.run(tool, args(a)).await));
        }
        out
    });
    for (tool, r) in &results {
        assert_eq!(r.is_error, Some(true), "{tool}: {r:?}");
        assert!(text(r).contains("input is locked"), "{tool}: {r:?}");
    }
    for _ in 0..3 {
        h.step();
    }
    let done = h
        .state()
        .handle()
        .events_since(0)
        .into_iter()
        .filter(|e| e.name == "command.done" && e.data["command"] == "test.purge")
        .count();
    assert_eq!(done, 0, "an agent's input ran the purge");
    let d = h
        .state()
        .handle()
        .service(&egui_pop_api::dialogs::DIALOGS)
        .unwrap();
    assert!(d.is_open(dialog), "the confirmation is still open");
    // The user declines; the tools work again.
    d.answer(
        egui_pop_api::dialogs::Answer::Cancel,
        egui_pop_api::model::commands::Origin::User,
    )
    .unwrap();
    let start = Instant::now();
    while !asking.is_finished() {
        assert!(start.elapsed() < Duration::from_secs(20), "no answer");
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(asking.join().unwrap().is_err(), "the purge was refused");
    let after = with_bridge(&mut h, &path, |b| async move {
        b.run("widget_tree", JsonObject::new()).await
    });
    ok("widget_tree after", &after);
}
