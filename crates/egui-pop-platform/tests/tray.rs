//! The tray: its menu is a declared egui-pop menu, kept current; picks
//! run commands, clicks run `tray.activate`.

#![cfg(feature = "tray")]

use std::sync::{Arc, Mutex};

use egui_pop_api::menus::ListItem;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, MenuId, PluginId};
use egui_pop_platform::tray::{Entry, MENU_TRAY, STEP_ACTIVATE, Simulated, TRAY, TrayPlugin};
use serde_json::{Value as Json, json};

const SYNC: CommandId = CommandId::from_static("player.sync");
const PAUSE: CommandId = CommandId::from_static("player.pause");
const OPEN: CommandId = CommandId::from_static("player.open");
const RECENT: MenuId = MenuId::from_static("player.recent");

/// A player filling the tray's menu.
struct Player(Seen);

impl Plugin for Player {
    fn id(&self) -> PluginId {
        PluginId::from_static("player")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for (id, title, group) in [(SYNC, "Sync now", "1_sync"), (PAUSE, "Pause", "2_play")] {
            let seen = self.0.clone();
            let mut spec = CommandSpec::new(id.clone(), title, "Player").menu(MENU_TRAY, group, 0);
            if id == PAUSE {
                spec = spec.checked("playerPaused");
            }
            ctx.command(spec, move |call| {
                seen.lock()
                    .unwrap()
                    .push((id.to_string(), call.args.clone()));
                Ok(Json::Null)
            })?;
        }
        let seen = self.0.clone();
        ctx.command(
            CommandSpec::new(OPEN, "Open", "Player").hidden(),
            move |call| {
                seen.lock()
                    .unwrap()
                    .push((OPEN.to_string(), call.args.clone()));
                Ok(Json::Null)
            },
        )?;
        ctx.submenu(MENU_TRAY, "3_recent", 0, RECENT, "Recent")?;
        ctx.menu_item(egui_pop_model::menus::MenuItem::list(
            RECENT,
            "1",
            0,
            "player.recent",
        ))?;
        ctx.list_provider("player.recent", |_, _| {
            vec![ListItem::new("Album A", OPEN, json!({ "album": "a" }))]
        });
        Ok(())
    }
}

/// What the player's commands ran: command and arguments.
type Seen = Arc<Mutex<Vec<(String, Json)>>>;

fn app() -> (egui::Context, App, Simulated, Seen) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let tray = Simulated::default();
    let ctx = egui::Context::default();
    let app = App::builder()
        .plugin(Player(seen.clone()))
        .plugin(TrayPlugin::new("Player").backend(tray.clone()))
        .build(&ctx)
        .unwrap();
    (ctx, app, tray, seen)
}

fn labels(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|e| match e {
            Entry::Item { label, checked, .. } => match checked {
                Some(true) => format!("[x] {label}"),
                Some(false) => format!("[ ] {label}"),
                None => label.clone(),
            },
            Entry::Separator => "--".into(),
            Entry::Submenu { label, entries, .. } => {
                format!("{label} > {}", labels(entries).join(", "))
            }
        })
        .collect()
}

#[test]
fn the_menu_follows_the_declared_menu_and_its_context() {
    let (ctx, mut app, tray, _) = app();
    assert_eq!(tray.spec().unwrap().title, "Player");
    app.logic(&ctx);
    assert_eq!(
        labels(&tray.menu()),
        ["Sync now", "--", "[ ] Pause", "--", "Recent > Album A"]
    );
    app.handle().set_context("playerPaused", "true");
    app.logic(&ctx);
    assert_eq!(labels(&tray.menu())[2], "[x] Pause");
    assert_eq!(app.handle().service(&TRAY).unwrap().menu(), tray.menu());
}

#[test]
fn picks_run_commands_and_clicks_run_the_step() {
    let (ctx, mut app, tray, seen) = app();
    app.logic(&ctx);
    assert!(tray.pick("Sync now"));
    assert!(tray.pick("Album A"));
    app.logic(&ctx);
    app.logic(&ctx);
    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            ("player.sync".to_string(), Json::Null),
            ("player.open".to_string(), json!({ "album": "a" })),
        ]
    );
    tray.click("left");
    app.logic(&ctx);
    let trace = app
        .handle()
        .step_traces()
        .into_iter()
        .find(|t| t.step == STEP_ACTIVATE)
        .unwrap();
    assert_eq!(trace.args, json!({ "button": "left" }));
}

#[test]
fn disconnecting_takes_the_icon_away() {
    let (ctx, mut app, tray, _) = app();
    app.logic(&ctx);
    assert!(tray.spec().is_some());
    app.disconnect(&egui_pop_platform::tray::PLUGIN);
    assert!(tray.spec().is_none());
    assert!(!tray.pick("Sync now"));
}

/// The system's tray, on a session bus of its own without a tray host:
/// run with `dbus-run-session -- cargo test -p egui-pop-platform --test
/// tray -- --ignored`.
#[test]
#[ignore = "needs a private D-Bus session (dbus-run-session)"]
fn the_system_tray_starts_and_takes_its_menu() {
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(Player(Arc::default()))
        .plugin(TrayPlugin::new("Player").app_id("org.egui_pop.test"))
        .build(&ctx)
        .unwrap();
    app.logic(&ctx);
    let tray = app.handle().service(&TRAY).unwrap();
    assert_eq!(tray.status(), Some(Ok(())), "{}", tray.backend());
    assert_eq!(labels(&tray.menu()).len(), 5);
}
