//! Strips: the activity bar shows the start side's panels and folds the
//! side bar; indicators follow the journal's events.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::strips::{Indicator, StripsPlugin, Tone};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::json;

const FILES: PanelId = PanelId::from_static("demo.files");
const SEARCH: PanelId = PanelId::from_static("demo.search");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(
            PanelSpec::new(FILES, "Files", Area::StartSide).icon("folder"),
            |ui, _| {
                ui.label("files body");
            },
        )?
        .panel(
            PanelSpec::new(SEARCH, "Search", Area::StartSide).icon("search"),
            |ui, _| {
                ui.label("search body");
            },
        )?;
        ctx.indicator(
            Indicator::new("demo.jobs", "Idle", "job.").reduce(|state, e| {
                let n = state.count.unwrap_or(0);
                match e.name.as_str() {
                    "job.started" => state.count = Some(n + 1),
                    "job.done" => state.count = Some(n.saturating_sub(1)),
                    _ => {}
                }
                let n = state.count.unwrap_or(0);
                state.text = if n == 0 { "Idle".into() } else { "Jobs".into() };
                state.tone = if n == 0 { Tone::Quiet } else { Tone::Active };
            }),
        );
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(StripsPlugin::default())
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

#[test]
fn the_activity_bar_shows_panels_and_folds_the_side_bar() {
    let mut h = harness();
    // Files and Search share the side bar: Files, the first, shows.
    assert!(h.query_by_label("files body").is_some());
    // The strip's Search button shows Search.
    let search: Vec<_> = h.query_all_by_label("Search").collect();
    assert!(search.len() >= 2, "the tab and the strip's button");
    search[0].click();
    h.run();
    assert!(h.query_by_label("search body").is_some());
    let files: Vec<_> = h.query_all_by_label("Files").collect();
    files[0].click();
    h.run();
    assert!(h.query_by_label("files body").is_some());
    // Clicking the shown one folds the side bar.
    let files: Vec<_> = h.query_all_by_label("Files").collect();
    files[0].click();
    h.run();
    assert!(
        h.state()
            .handle()
            .layout()
            .folded
            .contains(&Area::StartSide)
    );
    assert!(h.query_by_label("files body").is_none());
    // Ctrl+B unfolds it.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::B);
    h.run();
    assert!(
        !h.state()
            .handle()
            .layout()
            .folded
            .contains(&Area::StartSide)
    );
    assert!(h.query_by_label("files body").is_some());
}

#[test]
fn indicators_follow_the_hosts_events() {
    let mut h = harness();
    assert!(h.query_by_label("Idle").is_some());
    let app = h.state().handle().clone();
    app.publish("host", None, "job.started", json!({}));
    app.publish("host", None, "job.started", json!({}));
    h.run();
    assert!(h.query_by_label("Jobs (2)").is_some());
    let (_, state) = app
        .indicators()
        .into_iter()
        .find(|(i, _)| i.id == "demo.jobs")
        .unwrap();
    assert_eq!(state.tone, Tone::Active);
    app.publish("host", None, "job.done", json!({}));
    app.publish("host", None, "job.done", json!({}));
    h.run();
    assert!(h.query_by_label("Idle (0)").is_some());
    // Events of other names leave it alone.
    app.publish("host", None, "other.thing", json!({}));
    h.run();
    assert!(h.query_by_label("Idle (0)").is_some());
}
