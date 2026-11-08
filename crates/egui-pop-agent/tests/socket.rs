//! The agent socket from outside: an application in a kittest harness,
//! its socket in a private temporary directory, and a client thread
//! speaking JSON-RPC while the test steps frames.
#![cfg(unix)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_pop_agent::{AgentSocket, AgentSocketExt as _};
use egui_pop_api::about::{AboutPlugin, AppInfo};
use egui_pop_api::model::commands::CommandSpec;
use egui_pop_api::model::ids::{CommandId, PluginId};
use egui_pop_api::{App, AppBuilder, Ctx, DefaultPlugins, Plugin, PluginError};
use serde_json::{Value as Json, json};

/// A command that writes, offered to agents: what read-only refuses.
struct Writes;

const WRITE: CommandId = CommandId::from_static("test.write");

impl Plugin for Writes {
    fn id(&self) -> PluginId {
        PluginId::from_static("test.writes")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(WRITE, "Write something", "Test")
                .description("Changes the application (a test's)."),
            |_| Ok(json!("written")),
        )?;
        Ok(())
    }
}

/// A fresh private directory (mode 0700) for one test.
fn private_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("egui-pop-agent-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}

/// A gallery-like application serving its bridge at `socket`.
fn app(builder: AppBuilder, socket: &Path) -> Harness<'static, App> {
    let builder = builder
        .plugins(DefaultPlugins)
        .plugin(AboutPlugin::new(AppInfo::new("Agent test", "1.0.0")))
        .plugin(Writes)
        .app_id("org.example.AgentTest")
        .agent_socket(AgentSocket::at(socket));
    let mut h = egui_pop_test::app(builder, egui::vec2(800.0, 600.0));
    h.step();
    h
}

/// A JSON-RPC client over the socket.
struct Client {
    read: BufReader<UnixStream>,
    write: UnixStream,
    next: u64,
}

impl Client {
    fn connect(path: &Path) -> Self {
        let write = UnixStream::connect(path).expect("the socket answers");
        write
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        Self {
            read: BufReader::new(write.try_clone().unwrap()),
            write,
            next: 1,
        }
    }

    /// The whole response.
    fn request(&mut self, method: &str, params: Json) -> Json {
        let id = self.next;
        self.next += 1;
        let req = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(self.write, "{req}").unwrap();
        let mut line = String::new();
        self.read.read_line(&mut line).expect("a response line");
        let response: Json = serde_json::from_str(&line).expect("JSON");
        assert_eq!(response["id"], id, "{response}");
        response
    }

    /// The result, failing on an error.
    fn result(&mut self, method: &str, params: Json) -> Json {
        let r = self.request(method, params);
        assert!(r.get("error").is_none(), "{method}: {r}");
        r["result"].clone()
    }
}

/// Runs `client` on its own thread while stepping the harness's frames,
/// as a window would draw them, until it ends.
fn with_client<T: Send + 'static>(
    h: &mut Harness<'static, App>,
    client: impl FnOnce() -> T + Send + 'static,
) -> T {
    let thread = std::thread::spawn(client);
    let start = Instant::now();
    while !thread.is_finished() {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the client did not finish"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    thread.join().expect("the client passes")
}

#[test]
fn describe_call_and_wait_from_outside() {
    let dir = private_dir("describe");
    let path = dir.join("app.sock");
    let mut h = app(App::builder(), &path);
    let p = path.clone();
    with_client(&mut h, move || {
        let mut c = Client::connect(&p);

        let described = c.result("describe", json!({}));
        assert_eq!(described["protocol"], 1);
        let caps = described["capabilities"].as_array().unwrap();
        assert!(caps.iter().any(|c| c["name"] == "about.read"));
        assert!(caps.iter().any(|c| c["name"] == "bridge.wait"));

        let snapshot = c.result("snapshot", json!({ "collector": "bridge" }));
        assert!(snapshot.is_object(), "{snapshot}");

        let settled = c.result("wait", json!({ "settled": true, "deadline_ms": 10_000 }));
        assert_eq!(settled["met"], true, "{settled}");
        let since = settled["last"].as_u64().unwrap();

        // A read-only command, allowed in read-only mode: queued for the
        // next frame, its result on the journal.
        let f = c.result(
            "call",
            json!({ "collector": "commands", "action": "run", "args": { "id": "about.read" } }),
        );
        assert_eq!(f["ok"], true, "{f}");
        let done = c.result(
            "wait",
            json!({ "event": "command.done", "since": since, "deadline_ms": 10_000 }),
        );
        assert_eq!(done["met"], true, "{done}");
        assert_eq!(done["by"], "event");
        assert_eq!(done["event"]["data"]["command"], "about.read", "{done}");

        let settled = c.result("wait", json!({ "settled": true, "deadline_ms": 10_000 }));
        assert_eq!(settled["met"], true, "{settled}");
        assert_eq!(settled["by"], "settled");

        // A wait that is never met ends at its deadline, unmet.
        let never = c.result(
            "wait",
            json!({ "event": "no.such.event", "deadline_ms": 50 }),
        );
        assert_eq!(never["met"], false, "{never}");

        // Protocol errors.
        let r = c.request("nope", json!({}));
        assert_eq!(r["error"]["code"], -32601, "{r}");
        let r = c.request("call", json!({ "collector": "commands" }));
        assert_eq!(r["error"]["code"], -32602, "{r}");
    });
}

#[test]
fn read_only_refuses_a_write() {
    let dir = private_dir("readonly");
    let path = dir.join("app.sock");
    let mut h = app(App::builder(), &path);
    let p = path.clone();
    let f = with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        c.result(
            "call",
            json!({ "collector": "commands", "action": "run", "args": { "id": "test.write" } }),
        )
    });
    assert_eq!(f["ok"], false, "{f}");
    assert!(f["message"].as_str().unwrap().contains("read-only"), "{f}");
    // Refused on the application's journal too.
    let refused = h
        .state()
        .handle()
        .events_since(0)
        .into_iter()
        .any(|e| e.name == "debug.call.refused");
    assert!(refused);
}

#[test]
fn an_application_may_offer_more() {
    let dir = private_dir("offer");
    let path = dir.join("app.sock");
    let builder = App::builder().debug_bridge(egui_pop_api::core::debug::BridgeMode::On);
    let mut h = app(builder, &path);
    let p = path.clone();
    let f = with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        c.result(
            "call",
            json!({ "collector": "commands", "action": "run", "args": { "id": "test.write" } }),
        )
    });
    assert_eq!(f["ok"], true, "{f}");
}

#[test]
fn the_socket_is_private() {
    let dir = private_dir("mode");
    let path = dir.join("app.sock");
    let h = app(App::builder(), &path);
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "mode {:o}", mode & 0o777);
    drop(h);
}

#[test]
fn a_shared_directory_is_refused() {
    let dir = private_dir("shared");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
    let path = dir.join("app.sock");
    // The application still starts; the socket does not.
    let _h = app(App::builder(), &path);
    assert!(!path.exists());
}
