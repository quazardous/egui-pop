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
                state.tone = if n == 0 { Tone::Neutral } else { Tone::Accent };
            }),
        );
        ctx.indicator(Indicator::new("demo.sync", "Synced", "sync."));
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
fn the_activity_bar_is_a_fixed_zone_of_the_layout_that_refuses_tabs() {
    let mut h = harness();
    let app = h.state().handle().clone();
    // A pane at the start of the window's content (within its margin),
    // beside the primary side bar.
    let bar = h.get_by_label("the activity bar").rect();
    let side = app.area_rect(Area::StartSide).unwrap();
    assert!(bar.left() <= 8.0, "{bar:?}");
    assert!(bar.width() > 0.0, "{bar:?}");
    assert!((side.left() - bar.right()).abs() < 1.0, "{bar:?} {side:?}");
    // Its breadth is fixed: no sash resizes it.
    assert!(h.query_by_label("Resize the activity bar").is_none());

    // The Files tab, dragged and released over it, stays where it was.
    let before = app.layout();
    let tab = h
        .query_all_by_label("Files")
        .map(|n| n.rect())
        .find(|r| side.contains(r.center()))
        .expect("the Files tab");
    let to = bar.center();
    drag(&mut h, tab.center(), to);
    assert!(
        app.drop_hint().is_none(),
        "no drop preview over the activity bar: {:?}",
        app.drop_hint()
    );
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    assert_eq!(app.layout().areas, before.areas, "back where it was");
    assert_eq!(h.get_by_label("the activity bar").rect(), bar);
}

/// Presses on `from` and drags to `to` a frame at a time, as egui_stack
/// follows a tab's drag; held there.
fn drag(h: &mut Harness<'_, App>, from: egui::Pos2, to: egui::Pos2) {
    let mut send = |e: egui::Event| {
        h.input_mut().events.push(e);
        h.step();
    };
    send(egui::Event::PointerMoved(from));
    send(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    for i in 1..=8 {
        send(egui::Event::PointerMoved(
            from + (to - from) * (i as f32 / 8.0),
        ));
    }
    for _ in 0..4 {
        h.step();
    }
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
    assert_eq!(state.tone, Tone::Accent);
    app.publish("host", None, "job.done", json!({}));
    app.publish("host", None, "job.done", json!({}));
    h.run();
    assert!(h.query_by_label("Idle (0)").is_some());
    // Events of other names leave it alone.
    app.publish("host", None, "other.thing", json!({}));
    h.run();
    assert!(h.query_by_label("Idle (0)").is_some());
}

#[test]
fn the_activity_bar_is_one_tab_stop_with_up_and_down_inside() {
    use egui::Key;
    use egui_pop_test::walk::{self, F6, Press, SHIFT_TAB, TAB};
    let mut h = harness();
    let name = |s: Option<walk::Stop>| s.expect("a focus");
    // F6 starts at the activity bar.
    let files = name(walk::press(&mut h, F6));
    assert_eq!(files.name, "Files");
    let down = Press::key(Key::ArrowDown);
    let search = name(walk::press(&mut h, down));
    assert_eq!(search.name, "Search");
    assert_eq!(name(walk::press(&mut h, down)).id, files.id, "round");
    let up = Press::key(Key::ArrowUp);
    assert_eq!(name(walk::press(&mut h, up)).id, search.id);
    // Tab leaves the bar (for the side bar's tabs, named alike); Shift+Tab
    // comes back to the item focused last.
    let out = name(walk::press(&mut h, TAB));
    assert!(out.id != files.id && out.id != search.id, "{out}");
    assert_eq!(name(walk::press(&mut h, SHIFT_TAB)).id, search.id);
}

#[test]
fn the_status_bar_is_one_tab_stop_with_left_and_right_inside() {
    use egui::Key;
    use egui_pop_test::walk::{self, End, Press, SHIFT_F6, TAB};
    let mut h = harness();
    // Shift+F6 from nothing: the last region, the status bar.
    walk::press(&mut h, SHIFT_F6).expect("the status bar");
    let along = walk::walk(&mut h, Press::key(Key::ArrowRight), 20);
    assert!(matches!(along.end, End::Looped { .. }), "{along}");
    assert!(along.stops.len() >= 2, "{along}");
    let items: Vec<egui::Id> = along.stops.iter().map(|s| s.id).collect();
    let round = walk::walk(&mut h, TAB, 100);
    let in_bar = round.stops.iter().filter(|s| items.contains(&s.id)).count();
    assert_eq!(in_bar, 1, "{round}");
}
