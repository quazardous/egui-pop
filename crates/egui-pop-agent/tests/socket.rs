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

        let all = c.result("snapshot", json!({}));
        assert!(all.get("commands").is_some(), "{all}");
        let one = c.result("snapshot", json!({ "collector": "commands" }));
        assert!(!one.is_null(), "{one}");
        let r = c.request("snapshot", json!({ "collector": "no.such" }));
        assert_eq!(r["error"]["code"], -32602, "{r}");

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
fn off_refuses_snapshots() {
    let dir = private_dir("off");
    let path = dir.join("app.sock");
    let builder = App::builder().debug_bridge(egui_pop_api::core::debug::BridgeMode::Off);
    let mut h = app(builder, &path);
    let p = path.clone();
    let (all, one, described) = with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        (
            c.request("snapshot", json!({})),
            c.request("snapshot", json!({ "collector": "commands" })),
            c.request("describe", json!({})),
        )
    });
    for r in [&all, &one, &described] {
        assert_eq!(r["error"]["code"], -32001, "{r}");
        assert!(r.get("result").is_none(), "{r}");
    }
    assert!(
        all["error"]["message"].as_str().unwrap().contains("off"),
        "{all}"
    );
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

/// The line the socket sends a client it closes, and that it closed.
fn refused_and_closed(read: &mut BufReader<UnixStream>) -> Json {
    let mut line = String::new();
    read.read_line(&mut line).expect("a response line");
    let r: Json = serde_json::from_str(&line).expect("JSON");
    let mut rest = String::new();
    assert_eq!(
        read.read_line(&mut rest).unwrap_or(0),
        0,
        "closed after it: {rest}"
    );
    r
}

#[test]
fn one_client_over_the_cap_is_refused_and_a_freed_place_is_given_again() {
    let dir = private_dir("cap");
    let path = dir.join("app.sock");
    let mut h = app(App::builder(), &path);
    let p = path.clone();
    with_client(&mut h, move || {
        // Each one answered: each one holds its place.
        let mut held: Vec<Client> = (0..egui_pop_agent::MAX_CONNECTIONS)
            .map(|_| {
                let mut c = Client::connect(&p);
                c.result("describe", json!({}));
                c
            })
            .collect();

        let over = Client::connect(&p);
        let mut read = over.read;
        let r = refused_and_closed(&mut read);
        assert_eq!(r["error"]["code"], egui_pop_agent::RpcError::LIMIT, "{r}");
        assert!(r["id"].is_null(), "{r}");

        // One closes: its place is given to the next client, once the
        // socket saw it close.
        drop(held.pop());
        let start = Instant::now();
        loop {
            let mut c = Client::connect(&p);
            writeln!(c.write, r#"{{"jsonrpc":"2.0","id":1,"method":"describe"}}"#).unwrap();
            let mut line = String::new();
            c.read.read_line(&mut line).unwrap();
            let r: Json = serde_json::from_str(&line).unwrap();
            if r.get("error").is_none() {
                break;
            }
            // Refused while the closed one still held its place.
            assert_eq!(r["error"]["code"], egui_pop_agent::RpcError::LIMIT, "{r}");
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "the place was never freed"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        // The ones held still work.
        held[0].result("describe", json!({}));
    });
}

#[test]
fn a_request_too_long_is_refused_and_its_connection_closed() {
    let dir = private_dir("long");
    let path = dir.join("app.sock");
    let mut h = app(App::builder(), &path);
    let p = path.clone();
    with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        // Exactly the cap: still a request (not JSON, so a parse error).
        let line = "x".repeat(egui_pop_agent::MAX_REQUEST_BYTES);
        writeln!(c.write, "{line}").unwrap();
        let mut answer = String::new();
        c.read.read_line(&mut answer).unwrap();
        let r: Json = serde_json::from_str(&answer).unwrap();
        assert_eq!(r["error"]["code"], -32700, "{r}");
        c.result("describe", json!({}));

        // One byte more: refused, and closed.
        let line = "x".repeat(egui_pop_agent::MAX_REQUEST_BYTES + 1);
        // The socket may close before reading it all.
        let _ = writeln!(c.write, "{line}");
        let r = refused_and_closed(&mut c.read);
        assert_eq!(r["error"]["code"], egui_pop_agent::RpcError::LIMIT, "{r}");
    });
}

// Identity: `hello`, pairing through the user's approval, tokens.

/// The application's journal events named `name`, oldest first.
fn events(h: &Harness<'static, App>, name: &str) -> Vec<egui_pop_api::host::Event> {
    h.state()
        .handle()
        .events_since(0)
        .into_iter()
        .filter(|e| e.name == name)
        .collect()
}

/// Steps frames until the pairing dialog shows, then answers it as
/// `origin`; the client thread runs meanwhile.
fn answer_pairing(
    h: &mut Harness<'static, App>,
    accept: bool,
    origin: egui_pop_api::model::commands::Origin,
) -> Result<(), String> {
    use egui_pop_api::dialogs::{Answer, DIALOGS, Dialog};
    let start = Instant::now();
    loop {
        h.step();
        let d = h.state().handle().service(&DIALOGS).unwrap();
        if let Some((_, Dialog::Confirm(c))) = d.current()
            && c.command == egui_pop_agent::CMD_CONFIRM
        {
            let answer = if accept {
                Answer::Accept {
                    values: Default::default(),
                }
            } else {
                Answer::Cancel
            };
            return d.answer(answer, origin);
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "no pairing dialog showed"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Steps frames until `thread` ends.
fn finish<T>(h: &mut Harness<'static, App>, thread: std::thread::JoinHandle<T>) -> T {
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

/// The journal's `command.done` origins of `about.read` runs.
fn about_read_origins(h: &Harness<'static, App>) -> Vec<String> {
    events(h, "command.done")
        .into_iter()
        .filter(|e| e.data["command"] == "about.read")
        .map(|e| e.source.clone())
        .collect()
}

/// Runs `about.read` and waits for it, on `c`.
fn run_about_read(c: &mut Client) {
    let since = c.result("wait", json!({ "settled": true, "deadline_ms": 10_000 }))["last"]
        .as_u64()
        .unwrap();
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
}

#[test]
fn a_hello_paired_by_the_user_names_the_connections_calls_and_its_token_pairs_again() {
    let dir = private_dir("hello");
    let path = dir.join("app.sock");
    let persist = dir.join("persist");
    let mut h = app(App::builder().persist(&persist), &path);

    // Without `hello`: the socket's agent, as before.
    let p = path.clone();
    with_client(&mut h, move || run_about_read(&mut Client::connect(&p)));
    assert_eq!(about_read_origins(&h), ["agent:agent-socket"]);

    // `hello` is held until the user accepts.
    let p = path.clone();
    let client = std::thread::spawn(move || {
        let mut c = Client::connect(&p);
        let hello = c.result(
            "hello",
            json!({ "name": "Claude Code", "deadline_ms": 20_000 }),
        );
        // A second one on the same connection is refused.
        let again = c.request("hello", json!({ "name": "other" }));
        run_about_read(&mut c);
        (hello, again)
    });
    answer_pairing(&mut h, true, egui_pop_api::model::commands::Origin::User).unwrap();
    let (hello, again) = finish(&mut h, client);
    assert_eq!(hello["agent"], "claude-code", "{hello}");
    assert_eq!(hello["origin"], "agent:claude-code", "{hello}");
    let token = hello["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("egp_"), "{hello}");
    assert_eq!(again["error"]["code"], -32602, "{again}");
    assert_eq!(
        about_read_origins(&h),
        ["agent:agent-socket", "agent:claude-code"]
    );

    // The journal: paired, connected, then disconnected once it closed.
    let paired = events(&h, "agent.paired");
    assert_eq!(paired.len(), 1);
    assert_eq!(paired[0].data["agent"], "claude-code");
    assert_eq!(paired[0].source, "agent:claude-code");
    // The token is nowhere on the journal.
    let all = serde_json::to_string(&h.state().handle().events_since(0)).unwrap();
    assert!(!all.contains(&token), "the token leaked to the journal");
    let start = Instant::now();
    while events(&h, "agent.disconnected").is_empty() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "no disconnection"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }

    // Kept on disk as a hash only.
    let pairings = std::fs::read_to_string(persist.join("agent-pairings.json")).unwrap();
    assert!(pairings.contains("claude-code"), "{pairings}");
    assert!(!pairings.contains(&token), "the token is never written");
    drop(h);

    // A new run of the application: the token pairs at once, no dialog.
    let mut h = app(App::builder().persist(&persist), &path);
    assert_eq!(
        egui_pop_agent::pairings(h.state().handle())
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        ["claude-code"]
    );
    let p = path.clone();
    let (known, unknown) = with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        let known = c.result("hello", json!({ "name": "Claude Code", "token": token }));
        run_about_read(&mut c);
        let unknown =
            Client::connect(&p).request("hello", json!({ "name": "x", "token": "egp_nope" }));
        (known, unknown)
    });
    assert_eq!(known["agent"], "claude-code", "{known}");
    assert!(known.get("token").is_none(), "{known}");
    assert_eq!(unknown["error"]["code"], -32001, "{unknown}");
    assert_eq!(about_read_origins(&h), ["agent:claude-code"]);
    assert_eq!(events(&h, "agent.connected").len(), 1);
}

#[test]
fn a_pairing_the_user_declines_is_refused_and_an_agent_may_not_accept_it() {
    let dir = private_dir("decline");
    let path = dir.join("app.sock");
    let mut h = app(
        App::builder().debug_bridge(egui_pop_api::core::debug::BridgeMode::On),
        &path,
    );
    let p = path.clone();
    let client = std::thread::spawn(move || {
        Client::connect(&p).request("hello", json!({ "name": "bot", "deadline_ms": 20_000 }))
    });
    // An agent's accept is vetoed: only the user pairs.
    let agent = egui_pop_api::model::commands::Origin::Agent("intruder".into());
    answer_pairing(&mut h, true, agent).unwrap();
    for _ in 0..5 {
        h.step();
    }
    assert!(events(&h, "agent.paired").is_empty());
    // The command itself is kept from agents.
    let f = h.state().handle().debug_call(
        "commands",
        "run",
        &json!({ "id": "pop.agent.confirm", "args": { "id": 1 } }),
    );
    assert_eq!(f.ok, false, "{f:?}");
    // The user declines.
    answer_pairing(&mut h, false, egui_pop_api::model::commands::Origin::User).unwrap();
    let r = finish(&mut h, client);
    assert_eq!(r["error"]["code"], -32001, "{r}");
    assert!(
        r["error"]["message"].as_str().unwrap().contains("declined"),
        "{r}"
    );
    assert_eq!(events(&h, "agent.declined").len(), 1);
    assert!(events(&h, "agent.paired").is_empty());
    assert!(egui_pop_agent::pairings(h.state().handle()).is_empty());
}

#[test]
fn a_pairing_unanswered_ends_at_its_deadline() {
    let dir = private_dir("unanswered");
    let path = dir.join("app.sock");
    let mut h = app(App::builder(), &path);
    let p = path.clone();
    let r = with_client(&mut h, move || {
        Client::connect(&p).request("hello", json!({ "name": "bot", "deadline_ms": 100 }))
    });
    assert_eq!(r["error"]["code"], -32002, "{r}");
    assert!(events(&h, "agent.paired").is_empty());
}

#[test]
fn hello_checks_its_params() {
    let dir = private_dir("hello-params");
    let path = dir.join("app.sock");
    let mut h = app(App::builder(), &path);
    let p = path.clone();
    let (none, empty, long) = with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        (
            c.request("hello", json!({})),
            c.request("hello", json!({ "name": " " })),
            c.request("hello", json!({ "name": "x".repeat(65) })),
        )
    });
    for r in [&none, &empty, &long] {
        assert_eq!(r["error"]["code"], -32602, "{r}");
    }
}
