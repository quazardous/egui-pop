//! A failure stays in its frame, and a window never stays empty without
//! a word: a focus screen whose page is not drawn gives the shell back, a
//! panel that panics shows its fault while the other panels are drawn, a
//! window that painted nothing shows a fallback page and says why.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::admin::model::{FORMAT, Field, FieldKind, Manifest, Wizard};
use egui_pop_api::admin::{ADMIN, AdminPlugin, CMD_WIZARD_CANCEL, CMD_WIZARD_OPEN};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{BlockId, CommandId, PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::{Value as Json, json};

const BOOK: CommandId = CommandId::from_static("trip.book");
const GOOD: PanelId = PanelId::from_static("demo.good");
const BAD: PanelId = PanelId::from_static("demo.bad");

/// A wizard, a status bar block, and (when `hide_page`) the Settings
/// app's block replaced by one that draws nothing: the wizard is stored
/// and its page never drawn.
struct Trip {
    hide_page: bool,
}

impl Plugin for Trip {
    fn id(&self) -> PluginId {
        PluginId::from_static("trip")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![PluginId::from_static("egui_pop.admin")]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(CommandSpec::new(BOOK, "Book a trip", "Trip"), |_| {
            Ok(json!({ "booked": true }))
        })?;
        ctx.block(
            zones::STATUS_BAR,
            Block::add(BlockId::from_static("trip.status"), 0, |ui, _| {
                ui.label("Trip status");
            }),
        );
        if self.hide_page {
            ctx.block(
                zones::OVERLAYS,
                Block::replace(
                    BlockId::from_static("egui_pop.admin"),
                    BlockId::from_static("trip.no_page"),
                    |_, _| {},
                ),
            );
        }
        let admin = ctx.app().service(&ADMIN).expect("the admin service");
        admin.manifest(Manifest::new(FORMAT).wizard(
            Wizard::new("trip", "Plan a trip", "trip.book").step("where", "Where", |s| {
                s.field(Field::draft("trip", "city", "City", FieldKind::Text));
            }),
        ));
        Ok(())
    }
}

fn harness(app: App) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 640.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn run(h: &mut Harness<'_, App>, cmd: &CommandId, args: Json) -> Result<Json, String> {
    let out = h.state_mut().run(cmd, args, Origin::User).now().unwrap();
    h.run();
    h.run();
    out
}

fn trip(hide_page: bool) -> App {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(AdminPlugin)
        .plugin(Trip { hide_page })
        .build(&egui::Context::default())
        .unwrap()
}

fn events(app: &egui_pop_api::AppHandle, name: &str) -> Vec<Json> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == name)
        .map(|e| e.data)
        .collect()
}

/// The focus flag set while no page is drawn (here the Settings app's
/// block draws nothing): the shell is drawn, not the title bar alone, and
/// the hole is published once with what holds the screen.
#[test]
fn a_focus_screen_without_its_page_gives_the_shell_back() {
    let mut h = harness(trip(true));
    let app = h.state().handle().clone();
    let a = app.service(&ADMIN).unwrap();
    assert!(h.query_by_label("Trip status").is_some(), "the shell drawn");
    run(
        &mut h,
        &CMD_WIZARD_OPEN,
        json!({ "id": "trip", "focus": true }),
    )
    .unwrap();
    assert!(a.focus_screen(), "the wizard is stored as a focus screen");
    assert!(h.query_by_label("Where").is_none(), "no page of it drawn");
    assert!(app.focus_screen_hole());
    assert!(
        h.query_by_label("Trip status").is_some(),
        "the shell still paints under a focus screen without a page"
    );
    for _ in 0..5 {
        h.run();
    }
    let holes = events(&app, "window.focus_hole");
    assert_eq!(holes.len(), 1, "said once: {holes:?}");
    assert_eq!(holes[0]["settings"]["focus_screen"], true);
    assert_eq!(holes[0]["settings"]["wizard"], true);
    // The wizard ended: no hole left.
    run(&mut h, &CMD_WIZARD_CANCEL, json!({})).unwrap();
    assert!(!app.focus_screen_hole());
    assert!(h.query_by_label("Trip status").is_some());
}

/// The page drawn: the focus screen hides the shell as before, and no
/// hole is reported.
#[test]
fn a_focus_screen_with_its_page_still_hides_the_shell() {
    let mut h = harness(trip(false));
    let app = h.state().handle().clone();
    run(
        &mut h,
        &CMD_WIZARD_OPEN,
        json!({ "id": "trip", "focus": true }),
    )
    .unwrap();
    for _ in 0..5 {
        h.run();
    }
    assert!(h.query_by_label("Where").is_some(), "the wizard's step");
    assert!(
        h.query_by_label("Trip status").is_none(),
        "the shell hidden"
    );
    assert!(!app.focus_screen_hole());
    assert!(events(&app, "window.focus_hole").is_empty());
    assert!(!app.empty_window(), "a page is content");
}

/// Two panels, one panicking on purpose.
struct Panels(Arc<AtomicBool>);

impl Plugin for Panels {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let broken = self.0.clone();
        ctx.panel(PanelSpec::new(GOOD, "Good", Area::Center), |ui, _| {
            ui.label("Good content");
        })?
        .panel(PanelSpec::new(BAD, "Bad", Area::StartSide), move |ui, _| {
            assert!(!broken.load(Ordering::Relaxed), "the bad panel broke");
            ui.label("Bad content");
        })?;
        Ok(())
    }
}

/// A panel that panics while drawing is caught: its region shows the
/// fault with Retry, the other panel and the bars are drawn, and the
/// fault is published with the panel's id.
#[test]
fn a_panel_that_panics_is_contained_and_named() {
    let broken = Arc::new(AtomicBool::new(false));
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Panels(broken.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let mut h = harness(app);
    let app = h.state().handle().clone();
    assert!(h.query_by_label("Good content").is_some());
    assert!(h.query_by_label("Bad content").is_some());
    broken.store(true, Ordering::Relaxed);
    h.run();
    h.run();
    assert!(
        h.query_by_label("Good content").is_some(),
        "the other panel is drawn"
    );
    assert!(h.query_by_label("Bad content").is_none());
    assert!(
        h.query_by_label("Bad stopped working").is_some(),
        "the fault in the panel's region"
    );
    let faults = events(&app, "panel.fault");
    assert_eq!(faults.len(), 1, "published once: {faults:?}");
    assert_eq!(faults[0]["panel"], "demo.bad");
    assert_eq!(faults[0]["owner"], "demo");
    assert!(
        faults[0]["message"]
            .as_str()
            .unwrap()
            .contains("the bad panel broke")
    );
    assert!(!app.empty_window());
    // Repaired, then Retry: the panel is back.
    broken.store(false, Ordering::Relaxed);
    h.get_by_label("Retry").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Bad content").is_some());
}

/// No panel, no bar: one block, drawing only while `on`.
struct Blank(Arc<AtomicBool>);

impl Plugin for Blank {
    fn id(&self) -> PluginId {
        PluginId::from_static("blank")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let on = self.0.clone();
        ctx.block(
            zones::OVERLAYS,
            Block::add(BlockId::from_static("blank.content"), 0, move |ui, _| {
                if on.load(Ordering::Relaxed) {
                    ui.label("Good content");
                }
            }),
        );
        Ok(())
    }
}

/// A window that paints nothing under its title bar shows the fallback
/// page past the delay and publishes what holds the screen, once; the
/// page leaves when the window paints again.
#[test]
fn a_window_that_paints_nothing_shows_the_fallback_page() {
    let on = Arc::new(AtomicBool::new(true));
    let app = App::builder()
        .menu_bar(false)
        .plugin(Blank(on.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let mut h = harness(app);
    let app = h.state().handle().clone();
    app.set_empty_window_delay(Some(Duration::ZERO));
    for _ in 0..6 {
        h.run();
    }
    assert!(h.query_by_label("Good content").is_some());
    assert!(!app.empty_window(), "a window with content is left alone");
    on.store(false, Ordering::Relaxed);
    for _ in 0..6 {
        h.run();
    }
    assert!(app.empty_window());
    assert!(h.query_by_label("Nothing to show here").is_some());
    assert!(h.query_by_label("Reset the layout").is_some());
    let said = events(&app, "window.painted_nothing");
    assert_eq!(said.len(), 1, "said once: {said:?}");
    assert_eq!(said[0]["areas"], json!([]), "no panel anywhere");
    assert_eq!(said[0]["shell_drawn"], true);
    assert_eq!(said[0]["zen"], false);
    assert_eq!(said[0]["settings"]["open"], false);
    // Painting again: the page leaves.
    on.store(true, Ordering::Relaxed);
    for _ in 0..4 {
        h.run();
    }
    assert!(!app.empty_window());
    assert!(h.query_by_label("Nothing to show here").is_none());
    assert!(h.query_by_label("Good content").is_some());
    assert_eq!(events(&app, "window.painted_nothing").len(), 1);
}

/// The watch turned off: an empty window stays as it is.
#[test]
fn the_watch_can_be_turned_off() {
    let on = Arc::new(AtomicBool::new(false));
    let app = App::builder()
        .menu_bar(false)
        .plugin(Blank(on))
        .build(&egui::Context::default())
        .unwrap();
    let mut h = harness(app);
    let app = h.state().handle().clone();
    app.set_empty_window_delay(None);
    for _ in 0..8 {
        h.run();
    }
    assert!(!app.empty_window());
    assert!(h.query_by_label("Nothing to show here").is_none());
}

/// Zen mode hides the bars and the title bar: with nothing in the centre
/// the window has no decorations either, and is watched all the same.
#[test]
fn a_window_without_decorations_that_paints_nothing_is_watched_too() {
    let on = Arc::new(AtomicBool::new(false));
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Blank(on))
        .build(&egui::Context::default())
        .unwrap();
    let mut h = harness(app);
    let app = h.state().handle().clone();
    app.set_empty_window_delay(Some(Duration::ZERO));
    for _ in 0..6 {
        h.run();
    }
    assert!(!app.empty_window(), "the bars are content");
    run(&mut h, &egui_pop_api::workbench::CMD_ZEN, json!({})).unwrap();
    for _ in 0..6 {
        h.run();
    }
    assert!(app.zen());
    assert!(app.empty_window(), "no bar, no title bar, no content");
    assert!(h.query_by_label("Nothing to show here").is_some());
    let said = events(&app, "window.painted_nothing");
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0]["zen"], true);
    assert_eq!(said[0]["title_bar_painted"], false);
    // The way out leaves zen mode: the bars are back.
    h.get_by_label("Reset the layout").click();
    for _ in 0..6 {
        h.run();
    }
    assert!(!app.zen());
    assert!(!app.empty_window());
}
