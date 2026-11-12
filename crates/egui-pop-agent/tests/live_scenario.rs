//! Scenarios run live: an application in a kittest harness serves its
//! socket, and `egui_pop_test::scenario::live` replays a scenario over it
//! on another thread while the test steps frames, as a window would.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_pop_agent::{AgentSocket, AgentSocketExt as _};
use egui_pop_api::about::{AboutPlugin, AppInfo};
use egui_pop_api::core::debug::BridgeMode;
use egui_pop_api::windows::WindowSpec;
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, AppBuilder, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, PluginId};
use egui_pop_test::scenario::live::LiveRunner;
use egui_pop_test::scenario::parse;
use egui_pop_test::windows::MultiWindow;

fn private_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("egui-pop-live-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}

fn app(socket: &Path) -> Harness<'static, App> {
    app_with(App::builder(), socket)
}

fn app_with(builder: AppBuilder, socket: &Path) -> Harness<'static, App> {
    let builder = builder
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

/// A button, a text field and a key, each saying on screen what it got.
#[derive(Default)]
struct Widgets {
    state: Arc<Mutex<(u32, String, bool)>>,
}

impl Plugin for Widgets {
    fn id(&self) -> PluginId {
        PluginId::from_static("test.widgets")
    }

    fn title(&self) -> String {
        "Widgets".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let state = self.state.clone();
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("test.widgets"), 0, move |ui, _| {
                let mut s = state.lock().unwrap();
                if ui.button("Press me").clicked() {
                    s.0 += 1;
                }
                ui.label(format!("pressed {}", s.0));
                ui.add(egui::TextEdit::singleline(&mut s.1).hint_text("Name"));
                ui.label(format!("typed {}", s.1));
                if ui.input(|i| i.key_pressed(egui::Key::F2)) {
                    s.2 = true;
                }
                ui.label(if s.2 { "F2 pressed" } else { "F2 waiting" });
            }),
        );
        Ok(())
    }
}

#[test]
fn ui_steps_click_type_and_press_keys_live() {
    let dir = private_dir("ui");
    let path = dir.join("app.sock");
    let builder = App::builder()
        .debug_bridge(BridgeMode::On)
        .plugin(Widgets::default());
    let mut h = app_with(builder, &path);
    let scenario = parse(
        r#"Scenario(
            name: "ui-live",
            steps: [
                Wait(Settled),
                Expect(Tree(has: (role: "Button", label: "Press me"), lacks: (label: "pressed 1"))),
                Ui(Click(role: "Button", label: "Press me")),
                Wait(Settled),
                Expect(Tree(has: (label: "pressed 1"))),
                Ui(Type(role: "TextInput", text: "$who")),
                Wait(Settled),
                Expect(Tree(has: (label: "typed Ada"))),
                Ui(Key("F2")),
                Wait(Settled),
                Expect(Tree(has: (label: "F2 pressed"))),
                Expect(NoError),
            ],
        )"#,
    )
    .unwrap();
    let p = path.clone();
    let passed = with_client(&mut h, move || {
        LiveRunner::new(p)
            .var("who", "Ada")
            .connect_within(Duration::from_secs(10))
            .run(&scenario)
    })
    .unwrap_or_else(|f| panic!("{f}"));
    assert_eq!(passed.steps, 12);

    // A node named twice, or not at all, is refused with the reason.
    let p = path.clone();
    let missing = parse(r#"Scenario(name: "m", steps: [Ui(Click(label: "No such"))])"#).unwrap();
    let f = with_client(&mut h, move || {
        LiveRunner::new(p)
            .connect_within(Duration::from_secs(10))
            .run(&missing)
    })
    .expect_err("no such node");
    assert_eq!(f.broken, "ui", "{f}");
    assert!(f.message.contains("no \"No such\""), "{f}");
}

#[test]
fn live_runs_break_unknown_commands_and_refuse_ui_actions_when_read_only() {
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
    let b = b.expect_err("the bridge is read-only");
    assert_eq!(b.broken, "ui", "{b}");
    assert!(b.message.contains("read-only"), "{b}");
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

/// A "Press" button in the main window, and `test.second`, which opens a
/// second application window with a "Press" button of its own; each
/// says on screen how often it was pressed.
#[derive(Default)]
struct TwoWindows {
    pressed: Arc<Mutex<[u32; 2]>>,
}

impl Plugin for TwoWindows {
    fn id(&self) -> PluginId {
        PluginId::from_static("test.windows")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let main = self.pressed.clone();
        let second = self.pressed.clone();
        ctx.block(
            zones::MAIN,
            Block::add(
                BlockId::from_static("test.windows.main"),
                0,
                move |ui, _| {
                    // Below where egui embeds the second window (top left).
                    ui.add_space(320.0);
                    let mut p = main.lock().unwrap();
                    if ui.button("Press").clicked() {
                        p[0] += 1;
                    }
                    ui.label(format!("main pressed {}", p[0]));
                },
            ),
        );
        ctx.command(
            CommandSpec::new(
                CommandId::from_static("test.second"),
                "Second window",
                "Test",
            ),
            move |c| {
                let pressed = second.clone();
                c.app.open_window(
                    WindowSpec::new("second", "Second").size([260.0, 140.0]),
                    move |ui, _| {
                        let mut p = pressed.lock().unwrap();
                        if ui.button("Press").clicked() {
                            p[1] += 1;
                        }
                        ui.label(format!("second pressed {}", p[1]));
                    },
                );
                Ok(serde_json::Value::Null)
            },
        )?;
        Ok(())
    }
}

fn two_windows() -> (AppBuilder, Arc<Mutex<[u32; 2]>>) {
    let plugin = TwoWindows::default();
    let pressed = plugin.pressed.clone();
    let builder = App::builder().debug_bridge(BridgeMode::On).plugin(plugin);
    (builder, pressed)
}

/// Clicks the second window's "Press", then the main window's: the same
/// name in each, told apart by the `Window` step; each window's tree has
/// a `Window` node named by its title.
const TWO_WINDOWS: &str = r#"Scenario(
    name: "two-windows-live",
    fixtures: [Fixture("test.second", ())],
    steps: [
        Wait(Settled),
        Window("second"),
        Expect(Tree(has: (role: "Window", label: "Second"))),
        Ui(Click(role: "Button", label: "Press")),
        Wait(Settled),
        Expect(Tree(has: (label: "second pressed 1"), lacks: (label: "main pressed 0"))),
        Window("main"),
        Expect(Tree(has: (label: "main pressed 0"), lacks: (role: "Window", label: "Second"))),
        Expect(Tree(lacks: (label: "second pressed 1"))),
        Ui(Click(role: "Button", label: "Press")),
        Wait(Settled),
        Expect(Tree(has: (label: "main pressed 1"))),
        Window("third"),
        Expect(Tree(has: (label: "main pressed 1"))),
    ],
)"#;

/// The scenario's last step names a window that is not open.
fn assert_two_windows(
    r: Result<egui_pop_test::scenario::live::LivePassed, Box<egui_pop_test::scenario::Failure>>,
    pressed: &Mutex<[u32; 2]>,
) {
    let f = r.expect_err("no window `third`");
    assert_eq!(f.step, Some(14), "{f}");
    assert!(f.message.contains("no window `third`"), "{f}");
    assert_eq!(*pressed.lock().unwrap(), [1, 1]);
}

#[test]
fn a_window_step_acts_in_an_embedded_window_live() {
    let dir = private_dir("embedded");
    let path = dir.join("app.sock");
    let (builder, pressed) = two_windows();
    let mut h = app_with(builder, &path);
    let scenario = parse(TWO_WINDOWS).unwrap();
    let p = path.clone();
    let r = with_client(&mut h, move || {
        LiveRunner::new(p)
            .connect_within(Duration::from_secs(10))
            .run(&scenario)
    });
    assert_two_windows(r, &pressed);
}

#[test]
fn a_window_step_acts_in_a_window_of_its_own_live() {
    let dir = private_dir("own");
    let path = dir.join("app.sock");
    let (builder, pressed) = two_windows();
    let builder = builder
        .plugins(DefaultPlugins)
        .app_id("org.example.LiveTest")
        .agent_socket(AgentSocket::at(&path));
    // Real viewports, each window's pass on its own, as eframe runs them.
    let mut m = MultiWindow::new(|ctx| builder.build(ctx).unwrap());
    let scenario = parse(TWO_WINDOWS).unwrap();
    let p = path.clone();
    let thread = std::thread::spawn(move || {
        LiveRunner::new(p)
            .connect_within(Duration::from_secs(10))
            .run(&scenario)
    });
    let start = Instant::now();
    while !thread.is_finished() {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the runner did not finish"
        );
        m.root_pass();
        for v in m.declared() {
            m.window_pass(v);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let r = thread.join().expect("the runner thread ends");
    assert!(
        m.declared().len() == 1,
        "the second window is its own viewport"
    );
    assert_two_windows(r, &pressed);
}
