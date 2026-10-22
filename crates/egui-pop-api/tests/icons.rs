//! The icon registry seen from an application: commands, panels and
//! places declare icons by name, every name is checked (an unknown one is
//! a diagnostic, never silence) and agents read the names.

use egui_kittest::Harness;
use egui_pop_api::contexts::{self, ContextsPlugin, FrameSpec};
use egui_pop_api::strips::StripsPlugin;
use egui_pop_api::{App, AppBuilder, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, FrameId, PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::{Value as Json, json};

const GRID: CommandId = CommandId::from_static("demo.grid");
const BROKEN: CommandId = CommandId::from_static("demo.broken");
const LOG: PanelId = PanelId::from_static("demo.log");
const ALBUM: FrameId = FrameId::from_static("demo.album");
const PHOTO: FrameId = FrameId::from_static("demo.photo");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![contexts::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(GRID, "Show the grid", "View")
                .icon("visible")
                .icon_on("hidden"),
            |_| Ok(Json::Null),
        )?
        .command(
            CommandSpec::new(BROKEN, "Broken", "View").icon("test-no-such-command-icon"),
            |_| Ok(Json::Null),
        )?
        .panel(
            PanelSpec::new(LOG, "Log", Area::Bottom).icon("test-no-such-panel-icon"),
            |_, _| {},
        )?
        .frame(FrameSpec::place(ALBUM, "Album").icon("folder"))?
        .frame(FrameSpec::place(PHOTO, "Photo"))?;
        Ok(())
    }
}

fn builder() -> AppBuilder {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(ContextsPlugin::default())
        .plugin(StripsPlugin::status_only())
        .plugin(Demo)
}

fn harness(builder: AppBuilder) -> Harness<'static, App> {
    let app = builder.build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 480.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

#[test]
fn a_command_declares_its_icons_once_and_agents_read_them() {
    let h = harness(builder());
    let app = h.state().handle().clone();
    let snapshot = app.debug_snapshot();
    let commands = snapshot["commands"]["commands"].as_array().unwrap();
    let grid = commands.iter().find(|c| c["id"] == "demo.grid").unwrap();
    assert_eq!(grid["icon"], "visible");
    assert_eq!(grid["icon_on"], "hidden");
    assert_eq!(grid["unknown_icons"], json!([]));
    let broken = commands.iter().find(|c| c["id"] == "demo.broken").unwrap();
    assert_eq!(
        broken["unknown_icons"],
        json!(["test-no-such-command-icon"])
    );
}

#[test]
fn an_unknown_icon_name_is_a_diagnostic_naming_its_declarer() {
    let h = harness(builder());
    let app = h.state().handle().clone();
    let unknown: Vec<_> = app
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "unknown-icon")
        .collect();
    let subjects: Vec<&str> = unknown.iter().map(|d| d.subject.as_str()).collect();
    assert!(subjects.contains(&"demo.broken"), "{unknown:?}");
    assert!(subjects.contains(&"demo.log"), "{unknown:?}");
    assert!(!subjects.contains(&"demo.grid"), "{unknown:?}");
    let broken = unknown.iter().find(|d| d.subject == "demo.broken").unwrap();
    assert_eq!(
        broken.owner.as_ref().map(|o| o.to_string()),
        Some("demo".into())
    );
    // Said once per rebuild, not piled up.
    let n = unknown.len();
    let mut h = h;
    h.state_mut().rebuild();
    h.run();
    let again = app
        .diagnostics()
        .iter()
        .filter(|d| d.code == "unknown-icon")
        .count();
    assert_eq!(again, n);
}

#[test]
fn the_icons_collector_lists_the_vocabulary_the_declarations_and_the_unknown_names() {
    let h = harness(builder());
    let app = h.state().handle().clone();
    let icons = app.debug_snapshot()["icons"].clone();
    let builtin: Vec<&str> = icons["builtin"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Json::as_str)
        .collect();
    assert!(builtin.contains(&"settings"));
    assert_eq!(icons["phosphor"]["prefix"], "phosphor:");
    let declared = icons["declared"].as_array().unwrap();
    let find = |surface: &str, subject: &str| {
        declared
            .iter()
            .find(|d| d["surface"] == surface && d["subject"] == subject)
            .cloned()
            .unwrap_or_else(|| panic!("{surface} {subject} in {declared:?}"))
    };
    assert_eq!(find("command", "demo.grid")["resolves"], true);
    assert_eq!(find("command (on)", "demo.grid")["name"], "hidden");
    assert_eq!(find("panel", "demo.log")["resolves"], false);
    assert_eq!(find("frame", "demo.album")["name"], "folder");
    let unknown: Vec<&str> = icons["unknown"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|u| u["name"].as_str())
        .collect();
    assert!(unknown.contains(&"test-no-such-panel-icon"), "{unknown:?}");
    let found = app.debug_call("icons", "search", &json!({ "query": "chevron" }));
    assert!(found.ok, "{}", found.message);
    assert!(
        found
            .data
            .as_array()
            .unwrap()
            .contains(&json!("chevron-right"))
    );
}
