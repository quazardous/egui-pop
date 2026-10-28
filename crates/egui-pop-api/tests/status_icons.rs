//! Status items with icons (`docs/status-bar.md`): an icon of its own
//! before the text, changed by a reducer; the icon of the command an item
//! runs (its "on" icon while the command is checked); an item shown by its
//! icon alone, named by its label; agents read the names, and an unknown
//! name is reported.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::strips::{self, Indicator, IndicatorState, StripsPlugin};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::Value as Json;

const SYNC: CommandId = CommandId::from_static("demo.sync");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.app().set_context("demoLocked", "false");
        ctx.command(
            CommandSpec::new(SYNC, "Lock the catalogue", "Demo")
                .icon("unlock")
                .icon_on("lock")
                .checked("demoLocked"),
            |call| {
                call.app.set_context("demoLocked", "true");
                Ok(Json::Null)
            },
        )?;
        ctx.indicator(
            Indicator::new("demo.saved", "Saved", "doc.")
                .icon("save")
                .reduce(|state, e| {
                    if e.name == "doc.changed" {
                        state.text = "Changed".into();
                        state.icon = Some("edit".into());
                    }
                }),
        )
        .indicator(Indicator::new("demo.lock", "Catalogue", "lock.").command(SYNC))
        .indicator(
            Indicator::new("demo.home", "Home folder", "home.").initial(IndicatorState {
                icon: Some("home".into()),
                ..Default::default()
            }),
        )
        .indicator(
            Indicator::new("demo.broken", "Broken", "broken.").icon("test-no-such-status-icon"),
        );
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugin(StripsPlugin::status_only())
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn icon_of(h: &Harness<'_, App>, id: &str) -> Option<String> {
    strips::status(h.state().handle())
        .into_iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no item {id}"))
        .icon
}

#[test]
fn an_item_shows_its_icon_or_its_command_s() {
    let mut h = harness();
    assert_eq!(icon_of(&h, "demo.saved").as_deref(), Some("save"));
    // No icon of its own: its command's, the "on" one once checked.
    assert_eq!(icon_of(&h, "demo.lock").as_deref(), Some("unlock"));
    h.state().handle().set_context("demoLocked", "true");
    h.run();
    assert_eq!(icon_of(&h, "demo.lock").as_deref(), Some("lock"));
    // A reducer changes it.
    h.state()
        .handle()
        .publish("demo", None, "doc.changed", Json::Null);
    h.run();
    h.run();
    assert_eq!(icon_of(&h, "demo.saved").as_deref(), Some("edit"));
    let drawn = egui_pop_core::icon::registry::drawn(&h.ctx);
    assert!(
        drawn
            .iter()
            .any(|d| d.surface == "status" && d.icon == "edit"),
        "{drawn:?}"
    );
}

#[test]
fn an_item_with_an_icon_alone_is_named_by_its_label() {
    let h = harness();
    let home = strips::status(h.state().handle())
        .into_iter()
        .find(|e| e.id == "demo.home")
        .unwrap();
    assert!(home.text.is_empty());
    assert_eq!(home.icon.as_deref(), Some("home"));
    assert!(h.query_by_label("Home folder").is_some());
}

#[test]
fn an_unknown_status_icon_is_reported() {
    let h = harness();
    let unknown: Vec<_> = h
        .state()
        .handle()
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "unknown-icon")
        .collect();
    assert!(
        unknown.iter().any(|d| d.subject == "demo.broken"),
        "{unknown:?}"
    );
}
