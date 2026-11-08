//! Scenarios run live: an application in a kittest harness serves its
//! socket, and `egui_pop_test::scenario::live` replays a scenario over it
//! on another thread while the test steps frames, as a window would.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_pop_agent::{AgentSocket, AgentSocketExt as _};
use egui_pop_api::about::{AboutPlugin, AppInfo};
use egui_pop_api::{App, DefaultPlugins};
use egui_pop_test::scenario::live::LiveRunner;
use egui_pop_test::scenario::parse;

fn private_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("egui-pop-live-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}

fn app(socket: &Path) -> Harness<'static, App> {
    let builder = App::builder()
        .plugins(DefaultPlugins)
        .plugin(AboutPlugin::new(AppInfo::new("Live test", "1.0.0")))
        .app_id("org.example.LiveTest")
        .agent_socket(AgentSocket::at(socket));
    let mut h = egui_pop_test::app(builder, egui::vec2(800.0, 600.0));
    h.step();
    h
}

/// Runs `client` on its own thread while stepping frames, until it ends.
fn with_client<T: Send + 'static>(
    h: &mut Harness<'static, App>,
    client: impl FnOnce() -> T + Send + 'static,
) -> T {
    let thread = std::thread::spawn(client);
    let start = Instant::now();
    while !thread.is_finished() {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the runner did not finish"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(2));
    }
    thread.join().expect("the runner thread ends")
}

#[test]
fn a_scenario_runs_live_over_the_socket_with_its_shots_and_journal() {
    let dir = private_dir("pass");
    let path = dir.join("app.sock");
    let out = dir.join("out");
    let mut h = app(&path);
    let scenario = parse(
        r#"Scenario(
            name: "about-live",
            steps: [
                Wait(Settled),
                Call("about.read", ()),
                Wait(Event("command.done")),
                Expect(Journal(has: "command.done", data: { "command": "about.read" })),
                Expect(NoError),
                Expect(Value("journal.last", 0)),
            ],
        )"#,
    )
    .unwrap();
    // The last expectation is wrong on purpose: the failure names it.
    let (p, o) = (path.clone(), out.clone());
    let s = scenario.clone();
    let failure = with_client(&mut h, move || {
        LiveRunner::new(p)
            .out(o)
            .connect_within(Duration::from_secs(10))
            .run(&s)
    })
    .expect_err("journal.last is not 0");
    assert_eq!(failure.step, Some(6), "{failure}");
    assert_eq!(failure.broken, "expectation", "{failure}");
    assert!(
        failure.journal.iter().any(|e| e.contains("command.done")),
        "{failure}"
    );

    let mut ok = scenario;
    ok.steps.pop();
    ok.steps.push(parse_step(r#"Shot("after")"#));
    let (p, o) = (path.clone(), out.clone());
    let passed = with_client(&mut h, move || {
        LiveRunner::new(p)
            .out(o)
            .connect_within(Duration::from_secs(10))
            .run(&ok)
    })
    .unwrap_or_else(|f| panic!("{f}"));
    assert_eq!(passed.steps, 6);
    assert_eq!(passed.shots, [out.join("about-live-after.json")]);
    let shot: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&passed.shots[0]).unwrap()).unwrap();
    assert!(shot.get("journal").is_some(), "every collector's snapshot");
    assert!(passed.journal.iter().any(|e| e["name"] == "command.done"));
}

fn parse_step(step: &str) -> egui_pop_test::scenario::Step {
    let s = parse(&format!("Scenario(name: \"s\", steps: [{step}])")).unwrap();
    s.steps.into_iter().next().unwrap()
}

#[test]
fn live_runs_break_unknown_commands_and_refuse_ui_steps() {
    let dir = private_dir("broken");
    let path = dir.join("app.sock");
    let mut h = app(&path);
    let unknown = parse(
        r#"Scenario(name: "unknown", steps: [Call("commands.run", { "id": "no.such.command" })])"#,
    )
    .unwrap();
    let ui = parse(r#"Scenario(name: "ui", steps: [Ui(Click(label: "About"))])"#).unwrap();
    let p = path.clone();
    let (a, b) = with_client(&mut h, move || {
        let r = LiveRunner::new(p).connect_within(Duration::from_secs(10));
        (r.run(&unknown), r.run(&ui))
    });
    let a = a.expect_err("no such command");
    assert_eq!(a.broken, "invariant NoUnknownCommand", "{a}");
    assert!(a.message.contains("no.such.command"), "{a}");
    let b = b.expect_err("Ui is not live");
    assert_eq!(b.broken, "ui", "{b}");
}

#[test]
fn a_variable_fills_the_arguments_and_a_gone_application_breaks_no_panic() {
    let dir = private_dir("gone");
    let path = dir.join("app.sock");
    let h = app(&path);
    let p = path.clone();
    // No frame runs once the harness is dropped: the socket closes.
    let s = parse(r#"Scenario(name: "gone", steps: [Wait(Settled)])"#).unwrap();
    drop(h);
    let r = LiveRunner::new(p)
        .var("x", "y")
        .connect_within(Duration::from_millis(300))
        .run(&s);
    let f = r.expect_err("nobody answers");
    assert!(
        f.broken == "connect" || f.broken == "invariant NoPanic",
        "{f}"
    );
}
