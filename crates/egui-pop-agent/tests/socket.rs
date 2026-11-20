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
use egui_kittest::kittest::Queryable as _;
use egui_pop_agent::{AgentGrants, AgentGrantsExt as _, AgentSocket, AgentSocketExt as _, Grant};
use egui_pop_api::about::{AboutPlugin, AppInfo};
use egui_pop_api::model::commands::CommandSpec;
use egui_pop_api::model::ids::{CommandId, PluginId};
use egui_pop_api::{App, AppBuilder, Ctx, DefaultPlugins, Plugin, PluginError};
use serde_json::{Value as Json, json};

/// A command that writes, offered to agents: what read-only refuses.
struct Writes;

const WRITE: CommandId = CommandId::from_static("test.write");
/// A destructive command offered to agents: what the user confirms.
const PURGE: CommandId = CommandId::from_static("test.purge");
/// A command whose arguments are a JSON Schema: what the validator
/// checks.
const ZOOM: CommandId = CommandId::from_static("test.zoom");
/// A destructive command with a schema: refused at once when its
/// arguments do not match, never asked about.
const WIPE: CommandId = CommandId::from_static("test.wipe");

/// A command destructive by its arguments only: an export that
/// overwrites with `clash: "overwrite"`.
const EXPORT: CommandId = CommandId::from_static("test.export");

/// A read whose result is private data (places): refused to an agent
/// whose grant withholds it.
const PLACES: CommandId = CommandId::from_static("test.places");
/// A read whose result is only partly private: it leaves the places out
/// for an agent whose grant withholds them.
const PHOTOS: CommandId = CommandId::from_static("test.photos");

/// A private command taking a path: it answers the path, or fails
/// naming it when `fail` is set.
const LOCATE: CommandId = CommandId::from_static("test.locate");
/// The private data `test.locate` is given.
const SECRET: &str = "/home/someone/Pictures";

/// The schema `test.zoom` and `test.wipe` declare.
fn factor_schema() -> Json {
    json!({
        "type": "object",
        "required": ["factor"],
        "additionalProperties": false,
        "properties": { "factor": { "type": "number", "minimum": 0.1, "maximum": 8 } }
    })
}

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
        ctx.command(
            CommandSpec::new(PURGE, "Purge something", "Test")
                .description("Discards what cannot be had back (a test's).")
                .destructive(),
            |_| Ok(json!("purged")),
        )?;
        ctx.command(
            CommandSpec::new(EXPORT, "Export", "Test")
                .description(
                    "Writes files; overwrites existing ones with `clash: overwrite` (a test's).",
                )
                .destructive_if("clash", json!("overwrite")),
            |c| Ok(c.args["clash"].clone()),
        )?;
        ctx.command(
            CommandSpec::new(ZOOM, "Zoom", "Test")
                .description("Zooms by a factor (a test's).")
                .schema(factor_schema()),
            |c| Ok(c.args["factor"].clone()),
        )?;
        ctx.command(
            CommandSpec::new(WIPE, "Wipe", "Test")
                .description("Discards by a factor (a test's).")
                .destructive()
                .schema(factor_schema()),
            |_| Ok(json!("wiped")),
        )?;
        ctx.command(
            CommandSpec::new(PLACES, "Places", "Test")
                .description("Where the photos were taken (a test's).")
                .read_only()
                .private(),
            |_| Ok(json!({ "places": ["48.85,2.35"] })),
        )?;
        ctx.command(
            CommandSpec::new(LOCATE, "Locate", "Test")
                .description("Finds a folder by its path (a test's).")
                .read_only()
                .private(),
            |c| {
                let path = c.args["path"].clone();
                match c.args.get("fail") {
                    Some(_) => Err(format!("no such folder: {}", path.as_str().unwrap_or(""))),
                    None => Ok(json!({ "found": path })),
                }
            },
        )?;
        ctx.command(
            CommandSpec::new(PHOTOS, "Photos", "Test")
                .description(
                    "The photos, with their places when the caller reads private data (a test's).",
                )
                .read_only(),
            |c| {
                let mut photo = json!({ "id": 1 });
                if egui_pop_agent::grants::reads_private(c.app, c.origin) {
                    photo["gps"] = json!("48.85,2.35");
                }
                Ok(json!([photo]))
            },
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
    // The escape hatch: an unpaired client writes as before grants.
    let builder = App::builder()
        .debug_bridge(egui_pop_api::core::debug::BridgeMode::On)
        .agent_grants(AgentGrants::new().allow_unpaired_writes());
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
            // Refused while the closed one still held its place: the
            // refusal, or a write or read on the connection it closed
            // already (a broken pipe, a reset).
            let mut line = String::new();
            let answered = writeln!(c.write, r#"{{"jsonrpc":"2.0","id":1,"method":"describe"}}"#)
                .and_then(|()| c.read.read_line(&mut line))
                .is_ok_and(|n| n > 0);
            if answered {
                let r: Json = serde_json::from_str(&line).unwrap();
                if r.get("error").is_none() {
                    break;
                }
                assert_eq!(r["error"]["code"], egui_pop_agent::RpcError::LIMIT, "{r}");
            }
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
    assert!(!f.ok, "{f:?}");
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

// Destructive calls: the user confirms an agent's.

/// The `command.done` events of `test.purge`.
fn purged(h: &Harness<'static, App>) -> Vec<egui_pop_api::host::Event> {
    events(h, "command.done")
        .into_iter()
        .filter(|e| e.data["command"] == "test.purge")
        .collect()
}

/// An application whose bridge lets agents write.
fn writable(name: &str) -> (Harness<'static, App>, PathBuf) {
    let dir = private_dir(name);
    let path = dir.join("app.sock");
    let h = app(
        App::builder()
            .debug_bridge(egui_pop_api::core::debug::BridgeMode::On)
            .agent_grants(AgentGrants::new().allow_unpaired_writes()),
        &path,
    );
    (h, path)
}

fn purge(deadline_ms: u64) -> Json {
    json!({
        "collector": "commands",
        "action": "run",
        "args": { "id": "test.purge", "args": { "what": "everything" } },
        "deadline_ms": deadline_ms,
    })
}

#[test]
fn an_agents_destructive_call_waits_for_the_user_and_runs_once_accepted() {
    let (mut h, path) = writable("destructive-accept");
    let p = path.clone();
    let client = std::thread::spawn(move || {
        let mut c = Client::connect(&p);
        let described = c.result("describe", json!({}));
        let since = c.result("wait", json!({ "settled": true, "deadline_ms": 10_000 }))["last"]
            .as_u64()
            .unwrap();
        let f = c.result("call", purge(20_000));
        let done = c.result(
            "wait",
            json!({ "event": "command.done", "since": since, "deadline_ms": 10_000 }),
        );
        (described, f, done)
    });
    // The dialog shows; the command has not run.
    answer_pairing(&mut h, true, egui_pop_api::model::commands::Origin::User).unwrap();
    assert!(purged(&h).is_empty(), "it ran before the user answered");
    let (described, f, done) = finish(&mut h, client);
    let cap = described["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "test.purge")
        .cloned()
        .unwrap();
    assert_eq!(cap["destructive"], true, "{cap}");
    assert_eq!(f["ok"], true, "{f}");
    assert_eq!(done["met"], true, "{done}");
    let ran = purged(&h);
    assert_eq!(ran.len(), 1);
    // It ran as the agent, not as the user who confirmed it.
    assert_eq!(ran[0].source, "agent:agent-socket");
    assert!(events(&h, "command.refused").is_empty());
}

#[test]
fn an_agents_destructive_call_the_user_declines_is_refused_and_an_agent_may_not_accept_it() {
    let (mut h, path) = writable("destructive-decline");
    let p = path.clone();
    let client = std::thread::spawn(move || Client::connect(&p).request("call", purge(20_000)));
    // An agent's accept is vetoed: only the user confirms.
    let agent = egui_pop_api::model::commands::Origin::Agent("intruder".into());
    answer_pairing(&mut h, true, agent).unwrap();
    for _ in 0..5 {
        h.step();
    }
    assert!(purged(&h).is_empty());
    // The user declines.
    answer_pairing(&mut h, false, egui_pop_api::model::commands::Origin::User).unwrap();
    let r = finish(&mut h, client);
    assert_eq!(r["error"]["code"], -32001, "{r}");
    assert!(
        r["error"]["message"].as_str().unwrap().contains("declined"),
        "{r}"
    );
    for _ in 0..3 {
        h.step();
    }
    assert!(purged(&h).is_empty());
    let refused = events(&h, "command.refused");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].data["command"], "test.purge");
    assert_eq!(refused[0].data["by"], "pop.agent.confirm");
    assert_eq!(refused[0].source, "agent:agent-socket");
}

#[test]
fn an_agents_destructive_call_unconfirmed_ends_at_its_deadline() {
    let (mut h, path) = writable("destructive-deadline");
    let p = path.clone();
    let r = with_client(&mut h, move || {
        Client::connect(&p).request("call", purge(100))
    });
    assert_eq!(r["error"]["code"], -32002, "{r}");
    for _ in 0..3 {
        h.step();
    }
    assert!(purged(&h).is_empty());
    let refused = events(&h, "command.refused");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].data["by"], "pop.agent.confirm");
}

#[test]
fn a_call_that_is_not_destructive_is_not_held_nor_is_the_users_destructive_one() {
    use egui_pop_api::dialogs::DIALOGS;
    let (mut h, path) = writable("destructive-not-asked");
    let p = path.clone();
    // An agent's write that destroys nothing runs at once, no dialog.
    let f = with_client(&mut h, move || {
        Client::connect(&p).result(
            "call",
            json!({ "collector": "commands", "action": "run", "args": { "id": "test.write" } }),
        )
    });
    assert_eq!(f["ok"], true, "{f}");
    assert!(
        h.state()
            .handle()
            .service(&DIALOGS)
            .unwrap()
            .current()
            .is_none(),
        "nothing was asked"
    );
    // The user's own destructive run is never asked.
    let f = h.state_mut().debug_call_as(
        "commands",
        "run",
        &json!({ "id": "test.purge" }),
        egui_pop_api::model::commands::Origin::User,
    );
    assert!(f.ok, "{f:?}");
    h.step();
    assert!(
        h.state()
            .handle()
            .service(&DIALOGS)
            .unwrap()
            .current()
            .is_none(),
        "nothing was asked"
    );
    let ran = purged(&h);
    assert_eq!(ran.len(), 1);
    assert_eq!(ran[0].source, "user");
}

/// A `test.export` call, with `clash`.
fn export(clash: &str) -> Json {
    json!({
        "collector": "commands",
        "action": "run",
        "args": { "id": "test.export", "args": { "clash": clash } },
        "deadline_ms": 20_000,
    })
}

#[test]
fn a_call_destructive_by_its_arguments_alone_waits_for_the_user() {
    use egui_pop_api::dialogs::DIALOGS;
    let (mut h, path) = writable("destructive-by-args");
    // Without overwriting, the export runs at once: nothing is asked.
    let p = path.clone();
    let (described, f) = with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        (
            c.result("describe", json!({})),
            c.result("call", export("rename")),
        )
    });
    assert_eq!(f["ok"], true, "{f}");
    for _ in 0..3 {
        h.step();
    }
    let dialogs = h.state().handle().service(&DIALOGS).unwrap();
    assert!(dialogs.current().is_none(), "nothing was asked");
    let ran = done_runs(&h, "test.export");
    assert_eq!(ran.len(), 1, "{ran:?}");
    let cap = described["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "test.export")
        .cloned()
        .unwrap();
    assert_eq!(cap["destructive"], false, "{cap}");
    assert_eq!(
        cap["destructive_if"],
        json!([{ "arg": "clash", "equals": "overwrite" }]),
        "{cap}"
    );

    // Overwriting, it waits for the user's confirmation.
    let p = path.clone();
    let client =
        std::thread::spawn(move || Client::connect(&p).result("call", export("overwrite")));
    answer_pairing(&mut h, true, egui_pop_api::model::commands::Origin::User).unwrap();
    assert_eq!(
        done_runs(&h, "test.export").len(),
        1,
        "it ran before the user answered"
    );
    let f = finish(&mut h, client);
    assert_eq!(f["ok"], true, "{f}");
    for _ in 0..3 {
        h.step();
    }
    let ran = done_runs(&h, "test.export");
    assert_eq!(ran.len(), 2, "{ran:?}");
    assert_eq!(ran[1].source, "agent:agent-socket");

    // Declined, the overwriting export is refused and nothing runs.
    let p = path.clone();
    let client =
        std::thread::spawn(move || Client::connect(&p).request("call", export("overwrite")));
    answer_pairing(&mut h, false, egui_pop_api::model::commands::Origin::User).unwrap();
    let r = finish(&mut h, client);
    assert_eq!(r["error"]["code"], -32001, "{r}");
    for _ in 0..3 {
        h.step();
    }
    assert_eq!(done_runs(&h, "test.export").len(), 2);
    let refused = refused_runs(&h, "test.export");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].data["by"], "pop.agent.confirm");
}

// Grants: what each agent may do.

/// The `command.refused` events of `command`.
fn refused_runs(h: &Harness<'static, App>, command: &str) -> Vec<egui_pop_api::host::Event> {
    events(h, "command.refused")
        .into_iter()
        .filter(|e| e.data["command"] == command)
        .collect()
}

/// The `command.done` events of `command`.
fn done_runs(h: &Harness<'static, App>, command: &str) -> Vec<egui_pop_api::host::Event> {
    events(h, "command.done")
        .into_iter()
        .filter(|e| e.data["command"] == command)
        .collect()
}

fn run_call(id: &str, args: Json) -> Json {
    json!({ "collector": "commands", "action": "run", "args": { "id": id, "args": args } })
}

#[test]
fn an_unpaired_clients_write_is_refused_by_its_grant_and_journaled() {
    use egui_pop_api::dialogs::DIALOGS;
    let dir = private_dir("grants-unpaired");
    let path = dir.join("app.sock");
    // The bridge on, no escape hatch: unpaired clients read only.
    let mut h = app(
        App::builder().debug_bridge(egui_pop_api::core::debug::BridgeMode::On),
        &path,
    );
    let p = path.clone();
    let (write, purge, ui) = with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        run_about_read(&mut c);
        (
            c.request("call", run_call("test.write", Json::Null)),
            c.request("call", run_call("test.purge", Json::Null)),
            c.request("ui", json!({ "action": "click", "label": "anything" })),
        )
    });
    for r in [&write, &purge, &ui] {
        assert_eq!(r["error"]["code"], -32001, "{r}");
        assert!(
            r["error"]["message"]
                .as_str()
                .unwrap()
                .starts_with("refused by `pop.agent.grants`"),
            "{r}"
        );
    }
    h.step();
    assert!(done_runs(&h, "test.write").is_empty(), "never ran");
    // Refused before the user is asked to confirm a destructive one.
    assert!(
        h.state()
            .handle()
            .service(&DIALOGS)
            .unwrap()
            .current()
            .is_none()
    );
    let refused = refused_runs(&h, "test.write");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].data["by"], "pop.agent.grants");
    assert_eq!(refused[0].source, "agent:agent-socket");
    assert_eq!(
        refused_runs(&h, "test.purge")[0].data["by"],
        "pop.agent.grants"
    );
    let ui_refused = events(&h, "debug.call.refused");
    assert!(
        ui_refused
            .iter()
            .any(|e| e.data["collector"] == "ui" && e.data["by"] == "pop.agent.grants"),
        "{ui_refused:?}"
    );
    // Reading still works.
    assert_eq!(about_read_origins(&h), ["agent:agent-socket"]);
}

#[test]
fn the_grants_listener_runs_before_the_applications_and_holds_any_path() {
    use egui_pop_api::bus::CommandRun;
    use egui_pop_api::bus::bus_contract::{ListenOpts, Point as _};
    use egui_pop_api::model::commands::Origin;
    use std::sync::{Arc, Mutex};
    let dir = private_dir("grants-order");
    let path = dir.join("app.sock");
    let mut h = app(
        App::builder().debug_bridge(egui_pop_api::core::debug::BridgeMode::On),
        &path,
    );
    // An application policy, registered as Helios's is (no order given).
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let mine = seen.clone();
    h.state()
        .handle()
        .bus()
        .listen::<CommandRun>(
            "test.policy",
            ListenOpts::new().owner("test"),
            move |ask, _| {
                mine.lock().unwrap().push(format!("{}", ask.origin));
                Ok(())
            },
        )
        .unwrap();
    let order =
        egui_pop_api::bus::order_dump(&*h.state().handle().bus(), &[CommandRun::ID.as_str()]);
    let (g, p) = (
        order.find("pop.agent.grants").expect("listed"),
        order.find("test.policy").expect("listed"),
    );
    assert!(g < p, "{order}");
    // A run that did not come through the socket's `call` (a queued
    // command, a link): the listener refuses it, the policy never sees it.
    let out = h
        .state_mut()
        .run(&WRITE, Json::Null, Origin::Agent("agent-socket".into()))
        .now()
        .unwrap();
    assert_eq!(
        out.unwrap_err(),
        "refused by `pop.agent.grants`: the agent is read-only: `test.write` changes the application"
    );
    assert!(seen.lock().unwrap().is_empty(), "{seen:?}");
    let refused = refused_runs(&h, "test.write");
    assert_eq!(refused[0].data["by"], "pop.agent.grants");
    // The user, and agents that are not the socket's, are not its to judge.
    assert!(
        h.state_mut()
            .run(&WRITE, Json::Null, Origin::User)
            .now()
            .unwrap()
            .is_ok()
    );
    assert!(
        h.state_mut()
            .run(&WRITE, Json::Null, Origin::Agent("debug-bridge".into()))
            .now()
            .unwrap()
            .is_ok()
    );
    assert_eq!(*seen.lock().unwrap(), ["user", "agent:debug-bridge"]);
}

#[test]
fn a_paired_agent_is_held_to_its_grant_and_grants_change_live() {
    use egui_pop_api::model::commands::Origin;
    let dir = private_dir("grants-paired");
    let path = dir.join("app.sock");
    let mut h = app(
        App::builder().debug_bridge(egui_pop_api::core::debug::BridgeMode::On),
        &path,
    );
    // Paired: the editor preset by default.
    let p = path.clone();
    let client = std::thread::spawn(move || {
        let mut c = Client::connect(&p);
        c.result("hello", json!({ "name": "bot", "deadline_ms": 20_000 }));
        let write = c.result("call", run_call("test.write", Json::Null));
        let value = c.request(
            "call",
            run_call("value.set", json!({ "path": "settings/x", "value": 1 })),
        );
        let ui = c.request("ui", json!({ "action": "key", "key": "Escape" }));
        (write, value, ui)
    });
    answer_pairing(&mut h, true, Origin::User).unwrap();
    let (write, value, ui) = finish(&mut h, client);
    assert_eq!(write["ok"], true, "{write}");
    assert_eq!(value["error"]["code"], -32001, "{value}");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("may not write the value `settings/x`"),
        "{value}"
    );
    assert_eq!(ui["error"]["code"], -32001, "{ui}");

    // Its own grant, set live: only `about.*`.
    let app_handle = h.state().handle().clone();
    egui_pop_agent::grants::set_grants(
        &app_handle,
        AgentGrants::new().agent("bot", Grant::editor().commands(["about.*"])),
    );
    assert_eq!(
        egui_pop_agent::grants::grant_of(&app_handle, &Origin::Agent("bot".into())),
        Some(Grant::editor().commands(["about.*"]))
    );
    let out = h
        .state_mut()
        .run(&WRITE, Json::Null, Origin::Agent("bot".into()))
        .now()
        .unwrap();
    assert_eq!(
        out.unwrap_err(),
        "refused by `pop.agent.grants`: the agent may not run `test.write`"
    );
    // The full preset: everything the mode allows.
    egui_pop_agent::grants::set_grants(&app_handle, AgentGrants::new().agent("bot", Grant::full()));
    assert!(
        h.state_mut()
            .run(&WRITE, Json::Null, Origin::Agent("bot".into()))
            .now()
            .unwrap()
            .is_ok()
    );
}

// Input lock and expiry: while the user answers an agent's request,
// agents do not act on the interface; an unanswered request closes its
// dialog.

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

/// The `debug.call.refused` events of `ui` actions by `by`.
fn ui_refused(h: &Harness<'static, App>, by: &str) -> Vec<egui_pop_api::host::Event> {
    events(h, "debug.call.refused")
        .into_iter()
        .filter(|e| e.data["collector"] == "ui" && e.data["by"] == by)
        .collect()
}

#[test]
fn an_agents_ui_click_on_an_open_confirmation_is_refused_and_the_users_click_runs_it() {
    let (mut h, path) = writable("confirm-input-lock");
    let p = path.clone();
    let asking = std::thread::spawn(move || Client::connect(&p).request("call", purge(20_000)));
    for _ in 0..5 {
        h.step();
    }
    let dialog = confirmation_shown(&mut h);
    for _ in 0..3 {
        h.step();
    }
    // Another connection tries to click the confirmation's button, then
    // to press Enter: both refused. Reading the tree still works.
    let p = path.clone();
    let (click, key, tree) = with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        (
            c.request("ui", json!({ "action": "click", "label": "Run" })),
            c.request("ui", json!({ "action": "key", "key": "Enter" })),
            c.request("ui", json!({ "action": "tree" })),
        )
    });
    for r in [&click, &key] {
        assert_eq!(r["error"]["code"], -32001, "{r}");
        assert!(
            r["error"]["message"]
                .as_str()
                .unwrap()
                .contains("input is locked"),
            "{r}"
        );
    }
    assert!(tree.get("error").is_none(), "{tree}");
    for _ in 0..3 {
        h.step();
    }
    assert!(purged(&h).is_empty(), "an agent's input ran the purge");
    let d = h
        .state()
        .handle()
        .service(&egui_pop_api::dialogs::DIALOGS)
        .unwrap();
    assert!(d.is_open(dialog), "the confirmation is still open");
    let refused = ui_refused(&h, "pop.agent.confirm");
    assert_eq!(refused.len(), 2, "{refused:?}");
    assert_eq!(refused[0].source, "agent:agent-socket");
    assert_eq!(refused[0].data["action"], "click");
    // The user's own click runs it.
    h.get_by_label("Run").click();
    let r = finish(&mut h, asking);
    assert_eq!(r["result"]["ok"], true, "{r}");
    for _ in 0..3 {
        h.step();
    }
    let ran = purged(&h);
    assert_eq!(ran.len(), 1);
    assert_eq!(ran[0].source, "agent:agent-socket");
    // Answered: agents act again.
    let p = path.clone();
    let after = with_client(&mut h, move || {
        Client::connect(&p).request("ui", json!({ "action": "key", "key": "Escape" }))
    });
    assert!(after.get("error").is_none(), "{after}");
}

#[test]
fn an_agents_ui_action_is_refused_while_a_pairing_request_is_open() {
    let (mut h, path) = writable("pair-input-lock");
    let p = path.clone();
    let asking = std::thread::spawn(move || {
        Client::connect(&p).request("hello", json!({ "name": "bot", "deadline_ms": 20_000 }))
    });
    confirmation_shown(&mut h);
    let p = path.clone();
    let click = with_client(&mut h, move || {
        Client::connect(&p).request("ui", json!({ "action": "click", "label": "Pair" }))
    });
    assert_eq!(click["error"]["code"], -32001, "{click}");
    assert!(events(&h, "agent.paired").is_empty());
    assert_eq!(ui_refused(&h, "pop.agent.confirm").len(), 1);
    answer_pairing(&mut h, false, egui_pop_api::model::commands::Origin::User).unwrap();
    let r = finish(&mut h, asking);
    assert_eq!(r["error"]["code"], -32001, "{r}");
}

#[test]
fn an_unconfirmed_call_closes_its_dialog_at_its_deadline_and_a_late_accept_runs_nothing() {
    use egui_pop_api::dialogs::DIALOGS;
    use egui_pop_api::model::commands::Origin;
    let (mut h, path) = writable("confirm-expiry");
    let p = path.clone();
    let asking = std::thread::spawn(move || Client::connect(&p).request("call", purge(500)));
    let dialog = confirmation_shown(&mut h);
    let r = finish(&mut h, asking);
    assert_eq!(r["error"]["code"], -32002, "{r}");
    for _ in 0..5 {
        h.step();
    }
    let d = h.state().handle().service(&DIALOGS).unwrap();
    assert!(!d.is_open(dialog), "the expired confirmation closed");
    assert!(d.current().is_none());
    // The user's answer comes late, through the answer command: nothing
    // waits under it, nothing runs.
    let out = h
        .state_mut()
        .run(
            &egui_pop_agent::CMD_CONFIRM,
            json!({ "run": 1 }),
            Origin::User,
        )
        .now()
        .unwrap();
    assert!(
        out.as_ref().unwrap_err().contains("deadline passed"),
        "{out:?}"
    );
    for _ in 0..3 {
        h.step();
    }
    assert!(purged(&h).is_empty(), "a late accept ran the purge");
    let refused = refused_runs(&h, "test.purge");
    assert_eq!(refused.len(), 1, "{refused:?}");
    // Its dialog's cancel was not one more refusal.
    assert_eq!(refused[0].data["by"], "pop.agent.confirm");
}

#[test]
fn an_unanswered_pairing_closes_its_dialog_at_its_deadline() {
    use egui_pop_api::dialogs::DIALOGS;
    let dir = private_dir("pair-expiry");
    let path = dir.join("app.sock");
    let mut h = app(App::builder(), &path);
    let p = path.clone();
    let asking = std::thread::spawn(move || {
        Client::connect(&p).request("hello", json!({ "name": "bot", "deadline_ms": 500 }))
    });
    let dialog = confirmation_shown(&mut h);
    let r = finish(&mut h, asking);
    assert_eq!(r["error"]["code"], -32002, "{r}");
    for _ in 0..5 {
        h.step();
    }
    let d = h.state().handle().service(&DIALOGS).unwrap();
    assert!(!d.is_open(dialog), "the expired pairing request closed");
    assert!(events(&h, "agent.paired").is_empty());
}

// Argument schemas: a command's JSON Schema enforced before its handler.

#[test]
fn arguments_a_commands_schema_refuses_never_reach_its_handler_and_are_journaled() {
    use egui_pop_api::dialogs::DIALOGS;
    let (mut h, path) = writable("schema-agent");
    let p = path.clone();
    let (described, bad, wipe, good, free, done) = with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        let described = c.result("describe", json!({}));
        let since = c.result("wait", json!({ "settled": true, "deadline_ms": 10_000 }))["last"]
            .as_u64()
            .unwrap();
        let bad = c.result("call", run_call("test.zoom", json!({ "factor": "big" })));
        // Destructive, but refused at once: the user is not asked.
        let wipe = c.result("call", run_call("test.wipe", json!({ "factor": 99 })));
        let good = c.result("call", run_call("test.zoom", json!({ "factor": 2 })));
        // A command without a schema takes anything, as before.
        let free = c.result(
            "call",
            run_call("test.write", json!({ "anything": [1, 2] })),
        );
        let done = c.result(
            "wait",
            json!({ "event": "command.done", "since": since, "deadline_ms": 10_000 }),
        );
        (described, bad, wipe, good, free, done)
    });
    for _ in 0..3 {
        h.step();
    }
    // `describe` publishes the schema as the command's `args`.
    let cap = described["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "test.zoom")
        .cloned()
        .unwrap();
    assert_eq!(cap["args"], factor_schema(), "{cap}");
    // Refused with where and why, before the handler.
    assert_eq!(bad["ok"], false, "{bad}");
    let message = bad["message"].as_str().unwrap();
    assert!(message.contains("refused by `pop.command.args`"), "{bad}");
    assert!(
        message.contains("/factor: expected a number, got a string"),
        "{bad}"
    );
    assert_eq!(wipe["ok"], false, "{wipe}");
    assert!(
        wipe["message"]
            .as_str()
            .unwrap()
            .contains("above the maximum"),
        "{wipe}"
    );
    assert!(
        h.state()
            .handle()
            .service(&DIALOGS)
            .unwrap()
            .current()
            .is_none(),
        "nothing was asked"
    );
    assert!(done_runs(&h, "test.wipe").is_empty(), "never ran");
    // Journaled, naming the validator, from the agent.
    let refused = refused_runs(&h, "test.zoom");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].data["by"], "pop.command.args");
    assert_eq!(refused[0].source, "agent:agent-socket");
    assert_eq!(
        refused_runs(&h, "test.wipe")[0].data["by"],
        "pop.command.args"
    );
    // The matching run ran, once; the free one too.
    assert_eq!(good["ok"], true, "{good}");
    assert_eq!(done["met"], true, "{done}");
    assert_eq!(done_runs(&h, "test.zoom").len(), 1);
    assert_eq!(free["ok"], true, "{free}");
    assert_eq!(done_runs(&h, "test.write").len(), 1);
    assert!(refused_runs(&h, "test.write").is_empty());
}

#[test]
fn a_commands_schema_holds_every_origin_the_users_runs_included() {
    use egui_pop_api::model::commands::Origin;
    let (mut h, _path) = writable("schema-user");
    // The user's run (a key, a menu, the palette) with arguments its
    // schema refuses: refused before the handler, on the journal.
    let r = h
        .state_mut()
        .run(&ZOOM, json!({ "factor": 0 }), Origin::User)
        .now()
        .expect("answered at once");
    let e = r.unwrap_err();
    assert!(e.starts_with("refused by `pop.command.args`"), "{e}");
    assert!(e.contains("below the minimum"), "{e}");
    // No arguments are checked as `{}`: `factor` is required.
    let r = h
        .state_mut()
        .run(&ZOOM, Json::Null, Origin::User)
        .now()
        .expect("answered at once");
    assert!(r.unwrap_err().contains("`factor` is required"));
    let r = h
        .state_mut()
        .run(&ZOOM, json!({ "factor": 0.5 }), Origin::User)
        .now()
        .expect("answered at once");
    assert_eq!(r, Ok(json!(0.5)));
    h.step();
    let refused = refused_runs(&h, "test.zoom");
    assert_eq!(refused.len(), 2, "{refused:?}");
    assert!(
        refused
            .iter()
            .all(|e| e.data["by"] == "pop.command.args" && e.source == "user")
    );
    assert_eq!(done_runs(&h, "test.zoom").len(), 1);
    // In-process, the check answers before a run.
    assert!(
        h.state()
            .handle()
            .command_args_refusal(&ZOOM, &json!({ "factor": "x" }))
            .is_some()
    );
    assert!(
        h.state()
            .handle()
            .command_args_refusal(&WRITE, &json!(42))
            .is_none()
    );
}

// The Agents page (`developer/agents`): the paired agents listed, their
// access set live, a pairing revoked; all of it the user's only.

#[test]
fn the_agents_page_lists_a_paired_agent_sets_its_access_live_and_revokes_it() {
    use egui_pop_agent::{CMD_GRANT, CMD_LIST, CMD_REVOKE};
    use egui_pop_api::model::commands::Origin;
    use std::sync::mpsc;
    let dir = private_dir("agents-page");
    let path = dir.join("app.sock");
    let persist = dir.join("persist");
    let mut h = app(
        App::builder()
            .persist(&persist)
            .debug_bridge(egui_pop_api::core::debug::BridgeMode::On),
        &path,
    );

    // An agent pairs, then tries to revoke itself and change its access
    // through commands: kept from agents. Its connection stays open until
    // told to go on.
    let (told, token_rx) = mpsc::channel();
    let (go, go_rx) = mpsc::channel::<()>();
    let p = path.clone();
    let client = std::thread::spawn(move || {
        let mut c = Client::connect(&p);
        let hello = c.result(
            "hello",
            json!({ "name": "Test bot", "deadline_ms": 20_000 }),
        );
        let revoke = c.request(
            "call",
            run_call("pop.agent.revoke", json!({ "agent": "test-bot" })),
        );
        let grant = c.request(
            "call",
            run_call(
                "pop.agent.grant",
                json!({ "agent": "test-bot", "preset": "full" }),
            ),
        );
        told.send((hello["token"].as_str().unwrap().to_string(), revoke, grant))
            .unwrap();
        go_rx.recv().unwrap();
        // Revoked meanwhile: the connection is closed, nothing answers.
        let _ = writeln!(
            c.write,
            "{}",
            json!({ "jsonrpc": "2.0", "id": 99, "method": "describe" })
        );
        let mut line = String::new();
        let n = c.read.read_line(&mut line).unwrap_or(0);
        (n, line)
    });
    answer_pairing(&mut h, true, Origin::User).unwrap();
    let start = Instant::now();
    let (token, revoke, grant) = loop {
        if let Ok(t) = token_rx.try_recv() {
            break t;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "no pairing");
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    };
    let refused = |r: &Json| r.get("error").is_some() || r["result"]["ok"] == false;
    assert!(refused(&revoke), "an agent may not revoke: {revoke}");
    assert!(refused(&grant), "an agent may not change access: {grant}");
    // Nor through any other origin than the user's.
    let bot = Origin::Agent("test-bot".into());
    let run = |h: &mut Harness<'static, App>, id: &CommandId, args: Json, origin: Origin| {
        h.state_mut().run(id, args, origin).now().unwrap()
    };
    assert!(
        run(
            &mut h,
            &CMD_REVOKE,
            json!({ "agent": "test-bot" }),
            bot.clone()
        )
        .is_err()
    );
    assert!(
        run(
            &mut h,
            &CMD_GRANT,
            json!({ "agent": "test-bot", "preset": "full" }),
            bot.clone()
        )
        .is_err()
    );
    assert_eq!(egui_pop_agent::pairings(h.state().handle()).len(), 1);

    // Listed: connected now, the editor preset by default.
    let list = run(&mut h, &CMD_LIST, Json::Null, Origin::User).unwrap();
    let row = &list["agents"][0];
    assert_eq!(row["id"], "test-bot", "{list}");
    assert_eq!(row["name"], "Test bot", "{list}");
    assert_eq!(row["connections"], 1, "{list}");
    assert_eq!(row["preset"], "editor", "{list}");
    assert_eq!(list["defaults"][0]["preset"], "read-only", "{list}");

    // The page shows it.
    run(
        &mut h,
        &CommandId::from_static("admin.open"),
        json!({ "path": "developer/agents" }),
        Origin::User,
    )
    .unwrap();
    let start = Instant::now();
    while h.query_by_label("Test bot").is_none() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the Agents page does not list the agent"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(h.query_by_label("Paired agents").is_some());
    assert!(h.query_by_label("Revoke").is_some());

    // Its access set to read-only by the user: applies at once.
    assert!(
        run(&mut h, &WRITE, Json::Null, bot.clone()).is_ok(),
        "the editor runs a write"
    );
    run(
        &mut h,
        &CMD_GRANT,
        json!({ "agent": "test-bot", "preset": "read-only" }),
        Origin::User,
    )
    .unwrap();
    assert_eq!(
        egui_pop_agent::grants::grant_of(h.state().handle(), &bot),
        Some(Grant::read_only())
    );
    let refused_write = run(&mut h, &WRITE, Json::Null, bot.clone()).unwrap_err();
    assert!(refused_write.contains("read-only"), "{refused_write}");
    // A default too.
    run(
        &mut h,
        &CMD_GRANT,
        json!({ "default": "unpaired", "preset": "full" }),
        Origin::User,
    )
    .unwrap();
    assert_eq!(
        egui_pop_agent::grants::grants(h.state().handle()).unpaired,
        Grant::full()
    );

    // Revoked by the user: its connection closes, its token no longer
    // pairs, the journal says so.
    let out = run(
        &mut h,
        &CMD_REVOKE,
        json!({ "agent": "test-bot" }),
        Origin::User,
    )
    .unwrap();
    assert_eq!(out["closed"], 1, "{out}");
    go.send(()).unwrap();
    let (n, line) = finish(&mut h, client);
    assert!(n == 0, "the revoked connection still answered: {line}");
    let revoked = events(&h, "agent.revoked");
    assert_eq!(revoked.len(), 1);
    assert_eq!(revoked[0].data["agent"], "test-bot");
    assert_eq!(revoked[0].source, "user");
    assert!(egui_pop_agent::pairings(h.state().handle()).is_empty());
    assert!(
        !egui_pop_agent::grants::grants(h.state().handle())
            .agents
            .contains_key("test-bot")
    );
    let kept = std::fs::read_to_string(persist.join("agent-pairings.json")).unwrap();
    assert!(!kept.contains("test-bot"), "{kept}");
    let p = path.clone();
    let again = with_client(&mut h, move || {
        Client::connect(&p).request("hello", json!({ "name": "Test bot", "token": token }))
    });
    assert_eq!(again["error"]["code"], -32001, "{again}");
}

#[test]
fn private_data_is_refused_or_left_out_for_an_agent_whose_grant_withholds_it() {
    use egui_pop_agent::{CMD_GRANT, CMD_LIST};
    use egui_pop_api::model::commands::Origin;
    let dir = private_dir("grants-private");
    let path = dir.join("app.sock");
    let mut h = app(
        App::builder().debug_bridge(egui_pop_api::core::debug::BridgeMode::On),
        &path,
    );
    let unpaired = Origin::Agent("agent-socket".into());
    let run = |h: &mut Harness<'static, App>, id: &CommandId, args: Json, origin: Origin| {
        h.state_mut().run(id, args, origin).now().unwrap()
    };

    // By default every preset reads private data.
    assert_eq!(
        run(&mut h, &PLACES, Json::Null, unpaired.clone()).unwrap(),
        json!({ "places": ["48.85,2.35"] })
    );
    assert_eq!(
        run(&mut h, &PHOTOS, Json::Null, unpaired.clone()).unwrap()[0]["gps"],
        "48.85,2.35"
    );

    // The user withholds it from unpaired clients, on the Agents page:
    // the preset stays read-only.
    let set = run(
        &mut h,
        &CMD_GRANT,
        json!({ "default": "unpaired", "private": false }),
        Origin::User,
    )
    .unwrap();
    assert_eq!(set["preset"], "read-only", "{set}");
    assert_eq!(set["private"], false, "{set}");
    let app_handle = h.state().handle().clone();
    assert_eq!(
        egui_pop_agent::grants::grants(&app_handle).unpaired,
        Grant::read_only().private(false)
    );
    let list = run(&mut h, &CMD_LIST, Json::Null, Origin::User).unwrap();
    let row = &list["defaults"][0];
    assert_eq!(
        (row["id"].as_str(), row["preset"].as_str()),
        (Some("unpaired"), Some("read-only"))
    );
    assert_eq!(row["private"], false, "{list}");
    assert!(!egui_pop_agent::grants::reads_private(
        &app_handle,
        &unpaired
    ));
    assert!(egui_pop_agent::grants::reads_private(
        &app_handle,
        &Origin::User
    ));

    // Through the socket: the private read is refused at once, the mixed
    // one answers without its places; `describe` says which is private.
    let ran = done_runs(&h, "test.places").len();
    let p = path.clone();
    let (describe, places) = with_client(&mut h, move || {
        let mut c = Client::connect(&p);
        (
            c.result("describe", Json::Null),
            c.request("call", run_call("test.places", Json::Null)),
        )
    });
    let caps = describe["capabilities"].as_array().unwrap();
    let cap = |name: &str| caps.iter().find(|c| c["name"] == name).unwrap().clone();
    assert_eq!(cap("test.places")["private"], true);
    assert_eq!(cap("test.photos")["private"], false);
    assert_eq!(places["error"]["code"], -32001, "{places}");
    assert!(
        places["error"]["message"]
            .as_str()
            .unwrap()
            .contains("does not read private data: `test.places`"),
        "{places}"
    );
    let refused = refused_runs(&h, "test.places");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].data["by"], "pop.agent.grants");
    assert_eq!(refused[0].source, "agent:agent-socket");
    h.step();
    assert_eq!(
        done_runs(&h, "test.places").len(),
        ran,
        "never ran for the socket"
    );

    // Any other path holds too (the grants listener), and the mixed read
    // leaves the places out.
    assert_eq!(
        run(&mut h, &PLACES, Json::Null, unpaired.clone()).unwrap_err(),
        "refused by `pop.agent.grants`: the agent does not read private data: `test.places` gives places, people or file paths"
    );
    assert_eq!(
        run(&mut h, &PHOTOS, Json::Null, unpaired.clone()).unwrap(),
        json!([{ "id": 1 }])
    );
    // The user is never held.
    assert!(run(&mut h, &PLACES, Json::Null, Origin::User).is_ok());

    // Shown again: the switch alone, the preset untouched.
    run(
        &mut h,
        &CMD_GRANT,
        json!({ "default": "unpaired", "private": true }),
        Origin::User,
    )
    .unwrap();
    assert_eq!(
        egui_pop_agent::grants::grants(&app_handle).unpaired,
        Grant::read_only()
    );
    assert!(run(&mut h, &PLACES, Json::Null, unpaired).is_ok());
}

/// What an agent reads of `test.locate` and `test.photos` through the
/// socket: the journal (snapshot, `journal.since`), the `commands`
/// collector's run history, and a `wait` on the failure.
fn read_private_runs(path: &Path, h: &mut Harness<'static, App>) -> (Json, Json, Json, Json) {
    let p = path.to_path_buf();
    with_client(h, move || {
        let mut c = Client::connect(&p);
        let journal = c.result("snapshot", json!({ "collector": "journal" }));
        let since = c.result(
            "call",
            json!({ "collector": "journal", "action": "since", "args": { "seq": 0 } }),
        );
        let commands = c.result("snapshot", json!({ "collector": "commands" }));
        let failed = c.result(
            "wait",
            json!({ "event": "command.failed", "since": 0, "deadline_ms": 10_000 }),
        );
        (journal, since, commands, failed)
    })
}

#[test]
fn a_private_commands_args_result_and_error_are_redacted_for_an_agent_without_private_data() {
    use egui_pop_api::model::commands::Origin;
    let dir = private_dir("redacted");
    let path = dir.join("app.sock");
    let mut h = app(
        App::builder().debug_bridge(egui_pop_api::core::debug::BridgeMode::On),
        &path,
    );
    // The user runs the private command twice (once failing) and a
    // public one.
    let args = json!({ "path": SECRET });
    let ok = h
        .state_mut()
        .run(&LOCATE, args.clone(), Origin::User)
        .now()
        .unwrap();
    assert_eq!(ok, Ok(json!({ "found": SECRET })));
    let failing = json!({ "path": SECRET, "fail": true });
    assert!(
        h.state_mut()
            .run(&LOCATE, failing, Origin::User)
            .now()
            .unwrap()
            .is_err()
    );
    h.state_mut()
        .run(&PHOTOS, Json::Null, Origin::User)
        .now()
        .unwrap()
        .unwrap();
    h.step();

    // The journal keeps them whole, marked private; in process (the
    // application) they are in clear.
    let done = done_runs(&h, "test.locate");
    assert_eq!(done[0].data["result"]["found"], SECRET);
    assert_eq!(done[0].data["private"], true);
    assert_eq!(done_runs(&h, "test.photos")[0].data.get("private"), None);

    // An agent shown private data (every preset by default) reads them in
    // clear.
    let (journal, since, commands, failed) = read_private_runs(&path, &mut h);
    for read in [&journal, &since, &commands, &failed] {
        assert!(read.to_string().contains(SECRET), "{read}");
    }
    assert_eq!(
        failed["event"]["data"]["error"],
        format!("no such folder: {SECRET}")
    );

    // Withheld: every read says `<private>` instead, and nothing else.
    let app_handle = h.state().handle().clone();
    egui_pop_agent::grants::set_grants(
        &app_handle,
        egui_pop_agent::grants::AgentGrants::new().unpaired(Grant::read_only().private(false)),
    );
    let (journal, since, commands, failed) = read_private_runs(&path, &mut h);
    for read in [&journal, &since, &commands, &failed] {
        assert!(!read.to_string().contains(SECRET), "{read}");
    }
    let event = |events: &Json, name: &str, command: &str| {
        events
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == name && e["data"]["command"] == command)
            .cloned()
            .unwrap_or_else(|| panic!("{name} of {command} in {events}"))
    };
    for events in [&journal["events"], &since["data"]] {
        let dispatched = event(events, "command.dispatched", "test.locate");
        assert_eq!(dispatched["data"]["args"], "<private>", "{dispatched}");
        assert_eq!(dispatched["data"]["private"], true, "{dispatched}");
        let done = event(events, "command.done", "test.locate");
        assert_eq!(done["data"]["result"], "<private>", "{done}");
        let failed = event(events, "command.failed", "test.locate");
        assert_eq!(failed["data"]["error"], "<private>", "{failed}");
        // A public command's are unchanged.
        let photos = event(events, "command.done", "test.photos");
        assert_eq!(
            photos["data"]["result"],
            json!([{ "id": 1, "gps": "48.85,2.35" }])
        );
    }
    assert_eq!(failed["event"]["data"]["error"], "<private>", "{failed}");
    let runs = commands["history"].as_array().unwrap();
    let locate: Vec<&Json> = runs.iter().filter(|r| r["id"] == "test.locate").collect();
    assert_eq!(locate.len(), 2, "{commands}");
    assert_eq!(locate[0]["args"], "<private>");
    assert_eq!(locate[0]["result"], json!({ "Ok": "<private>" }));
    assert_eq!(locate[1]["result"], json!({ "Err": "<private>" }));
    let photos = runs.iter().find(|r| r["id"] == "test.photos").unwrap();
    assert_eq!(photos["args"], Json::Null);
    assert_eq!(photos.get("private"), None, "{photos}");
    // A declaration is not data: its `private` flag stays as it is.
    let spec = commands["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "test.locate")
        .unwrap();
    assert_eq!(spec["private"], true);
}
