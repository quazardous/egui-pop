//! Global shortcuts: system-scope bindings registered with the desktop,
//! their state for the editor, presses run through `keys.system`.

#![cfg(feature = "keys")]

use std::time::{Duration, Instant};

use egui_pop_api::system_keys::{KeyStatus, SYSTEM_KEYS, Support};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use egui_pop_model::keymap::{BindOptions, Chord};
use egui_pop_platform::keys::{self, STEP_PRESSED, Simulated, SystemKeysPlugin};
use serde_json::json;

const TOGGLE: CommandId = CommandId::from_static("player.toggle");
const NEXT: CommandId = CommandId::from_static("player.next");
const SHOW: CommandId = CommandId::from_static("player.show");

struct Player;

impl Plugin for Player {
    fn id(&self) -> PluginId {
        PluginId::from_static("player")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for (id, keys) in [
            (TOGGLE, "Ctrl+Alt+P"),
            (NEXT, "Ctrl+Alt+N"),
            (SHOW, "Ctrl+K Ctrl+P"),
        ] {
            ctx.command(
                CommandSpec::new(id, "Player command", "Player")
                    .key_with(keys, BindOptions::system()),
                |_| Ok(serde_json::Value::Null),
            )?;
        }
        Ok(())
    }
}

fn app(desktop: Simulated) -> (egui::Context, App) {
    let ctx = egui::Context::default();
    let app = App::builder()
        .plugin(Player)
        .plugin(SystemKeysPlugin::new("org.egui_pop.test").backend(desktop))
        .build(&ctx)
        .unwrap();
    (ctx, app)
}

#[test]
fn system_bindings_are_registered_and_their_state_said() {
    let desktop = Simulated::default().taken("Ctrl+Alt+N");
    let (ctx, mut app) = app(desktop.clone());
    app.logic(&ctx);
    let status = app.handle().service(&SYSTEM_KEYS).unwrap();
    assert_eq!(
        status.support(),
        Support::Available {
            backend: "simulated".into()
        }
    );
    assert_eq!(status.status("Ctrl+Alt+P"), Some(KeyStatus::Registered));
    assert_eq!(
        status.status("Ctrl+Alt+N"),
        Some(KeyStatus::Failed("taken by another application".into()))
    );
    assert_eq!(
        status.status("Ctrl+K Ctrl+P"),
        Some(KeyStatus::Failed(
            "a global shortcut is a single chord".into()
        ))
    );
    let registered: Vec<String> = desktop.registered().into_iter().map(|s| s.keys).collect();
    assert_eq!(registered, ["Ctrl+Alt+P"]);
}

#[test]
fn a_press_runs_the_step_and_its_command() {
    let desktop = Simulated::default();
    let (ctx, mut app) = app(desktop.clone());
    app.logic(&ctx);
    assert!(desktop.press("Ctrl+Alt+P"));
    app.logic(&ctx);
    let trace = app
        .handle()
        .step_traces()
        .into_iter()
        .find(|t| t.step == STEP_PRESSED)
        .unwrap();
    assert_eq!(trace.args["command"], "player.toggle");
    assert_eq!(trace.args["backend"], "simulated");
    let ran: Vec<CommandId> = app.handle().history().into_iter().map(|r| r.id).collect();
    assert_eq!(ran, [TOGGLE]);
}

#[test]
fn rebinding_registers_the_new_keys() {
    let desktop = Simulated::default();
    let (ctx, mut app) = app(desktop.clone());
    app.logic(&ctx);
    app.handle().rebind(TOGGLE, Some("Ctrl+Shift+F9".into()));
    app.logic(&ctx);
    app.logic(&ctx);
    let registered: Vec<String> = desktop.registered().into_iter().map(|s| s.keys).collect();
    assert!(
        registered.contains(&"Ctrl+Shift+F9".to_string()),
        "{registered:?}"
    );
    assert!(!registered.contains(&"Ctrl+Alt+P".to_string()));
    // Inside the window the keys still run the command.
    let _ = app.run(&TOGGLE, json!(null), Origin::User);
}

#[test]
fn a_desktop_without_global_shortcuts_says_why_and_registers_nothing() {
    let desktop = Simulated::unavailable("the compositor offers no global shortcuts");
    let (ctx, mut app) = app(desktop.clone());
    app.logic(&ctx);
    let status = app.handle().service(&SYSTEM_KEYS).unwrap();
    assert_eq!(
        status.support(),
        Support::Unavailable {
            reason: "the compositor offers no global shortcuts".into()
        }
    );
    assert!(desktop.registered().is_empty());
    assert!(!desktop.press("Ctrl+Alt+P"));
}

#[test]
fn chords_become_the_desktops_names() {
    let chord = |k: &str| Chord::parse(k).unwrap();
    assert_eq!(
        keys::xdg_trigger(&chord("Ctrl+Alt+P")).as_deref(),
        Some("CTRL+ALT+p")
    );
    assert_eq!(
        keys::xdg_trigger(&chord("Shift+Meta+F12")).as_deref(),
        Some("SHIFT+LOGO+F12")
    );
    assert_eq!(
        keys::xdg_trigger(&chord("Ctrl+PageUp")).as_deref(),
        Some("CTRL+Page_Up")
    );
    assert_eq!(
        keys::xdg_trigger(&chord("Ctrl+Space")).as_deref(),
        Some("CTRL+space")
    );
    let hk = keys::hotkey_of(&chord("Ctrl+Alt+P")).unwrap();
    assert_eq!(hk, keys::hotkey_of(&chord("Alt+Ctrl+P")).unwrap());
    assert!(keys::hotkey_of(&chord("Ctrl+Nothing")).is_err());
}

/// The portal on a session bus of its own, where no compositor serves
/// global shortcuts: every shortcut ends refused, with a reason, or the
/// desktop is said to take none; nothing stays silent. Run with
/// `dbus-run-session -- cargo test -p egui-pop-platform --test keys --
/// --ignored`.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs a private D-Bus session (dbus-run-session)"]
fn without_a_compositor_behind_the_portal_the_plugin_says_why() {
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(Player)
        .plugin(
            SystemKeysPlugin::new("org.egui_pop.test")
                .backend(keys::Portal::new("org.egui_pop.test")),
        )
        .build(&ctx)
        .unwrap();
    let status = app.handle().service(&SYSTEM_KEYS).unwrap();
    let settled = || {
        matches!(status.support(), Support::Unavailable { .. })
            || matches!(status.status("Ctrl+Alt+P"), Some(KeyStatus::Failed(_)))
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    while !settled() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        app.logic(&ctx);
    }
    assert!(settled(), "{:?} {:?}", status.support(), status.statuses());
}

/// global-hotkey on an X server of its own, a key pressed through XTest:
/// run with `xvfb-run cargo test -p egui-pop-platform --test keys --
/// --ignored` (needs `xdotool`).
#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs a virtual X server (xvfb-run) and xdotool"]
fn on_x11_a_grabbed_key_runs_its_command() {
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(Player)
        .plugin(SystemKeysPlugin::new("org.egui_pop.test").backend(keys::HotKeys::default()))
        .build(&ctx)
        .unwrap();
    app.logic(&ctx);
    let status = app.handle().service(&SYSTEM_KEYS).unwrap();
    assert_eq!(status.status("Ctrl+Alt+P"), Some(KeyStatus::Registered));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut pressed = false;
    while app.handle().history().is_empty() && Instant::now() < deadline {
        if !pressed {
            std::thread::sleep(Duration::from_millis(200));
            let ok = std::process::Command::new("xdotool")
                .args(["key", "ctrl+alt+p"])
                .status()
                .unwrap();
            assert!(ok.success());
            pressed = true;
        }
        std::thread::sleep(Duration::from_millis(20));
        app.logic(&ctx);
    }
    let ran: Vec<CommandId> = app.handle().history().into_iter().map(|r| r.id).collect();
    assert_eq!(ran, [TOGGLE]);
}
