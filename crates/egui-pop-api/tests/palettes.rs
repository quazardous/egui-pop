//! Palettes: floating panels over the workbench, opened floating, docked
//! and floated again, dragged (snapped, clamped), docked by a drop over
//! an area, hidden and shown by Tab, rolled up, pinned, giving the
//! keyboard back on Escape, every change through its step.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::palettes::{self, CMD_RESET, STEP_DOCK, STEP_MOVE};
use egui_pop_api::steps::Before;
use egui_pop_api::workbench::Change;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::layout::Floating;
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::{Value as Json, json};

const TOOLS: PanelId = PanelId::from_static("demo.tools");
const NAV: PanelId = PanelId::from_static("demo.nav");
const LOG: PanelId = PanelId::from_static("demo.log");

struct Demo {
    hooks: bool,
}

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let mut size = String::from("12");
        ctx.panel(
            PanelSpec::new(TOOLS, "Tools", Area::StartSide)
                .palette()
                .min_size(160.0),
            move |ui, _| {
                ui.label("Brush");
                ui.text_edit_singleline(&mut size);
            },
        )?
        .panel(
            PanelSpec::new(NAV, "Navigator", Area::EndSide)
                .palette()
                .min_size(160.0)
                .closed(),
            |ui, _| {
                ui.label("Overview");
            },
        )?
        .panel(
            PanelSpec::new(LOG, "Log", Area::Bottom).floatable(true),
            |ui, _| {
                ui.label("log body");
            },
        )?;
        if self.hooks {
            ctx.before(STEP_DOCK, |_, _| Before::Veto("palettes float here".into()));
            ctx.before(STEP_MOVE, |_, args| {
                let mut a = args.clone();
                a["pos"] = json!([100.0, 100.0]);
                Before::Redirect(a)
            });
        }
        Ok(())
    }
}

fn harness(hooks: bool) -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Demo { hooks })
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn floating(h: &Harness<'_, App>, p: &PanelId) -> Option<Floating> {
    h.state().handle().layout().floating_of(p).cloned()
}

fn events(h: &Harness<'_, App>, since: u64) -> Vec<String> {
    h.state()
        .handle()
        .events_since(since)
        .into_iter()
        .map(|e| e.name)
        .collect()
}

/// Drags from a point by an amount, in four moves.
fn drag_by(h: &mut Harness<'static, App>, from: egui::Pos2, by: egui::Vec2) {
    h.drag_at(from);
    h.run();
    for i in 1..=4 {
        h.hover_at(from + by * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(from + by);
    h.run();
    h.run();
}

#[test]
fn a_palette_opens_floating_docks_and_floats_again() {
    let mut h = harness(false);
    let tools = floating(&h, &TOOLS).expect("a palette floats");
    assert_eq!(tools.pos, Floating::cascade(0));
    assert!(h.query_by_label("Brush").is_some());
    assert_eq!(
        h.state().handle().layout().area_of(&LOG),
        Some(Area::Bottom)
    );
    // Docked: one model, two placements.
    let before = h.state().handle().last_event();
    h.state()
        .handle()
        .change_workbench(Change::Dock(TOOLS, Area::StartSide));
    h.run();
    assert!(floating(&h, &TOOLS).is_none());
    assert_eq!(
        h.state().handle().layout().area_of(&TOOLS),
        Some(Area::StartSide)
    );
    assert!(h.query_by_label("Brush").is_some(), "drawn in its area");
    let names = events(&h, before);
    assert!(names.contains(&"palette.dock".to_string()), "{names:?}");
    assert!(names.contains(&"palette.docked".to_string()), "{names:?}");
    // Floated again; a docked panel that is floatable floats too.
    h.state()
        .handle()
        .change_workbench(Change::Float(TOOLS, [300.0, 120.0]));
    h.state()
        .handle()
        .change_workbench(Change::Float(LOG, [40.0, 300.0]));
    h.run();
    assert_eq!(floating(&h, &TOOLS).unwrap().pos, [300.0, 120.0]);
    assert!(floating(&h, &LOG).is_some());
    assert!(h.query_by_label("log body").is_some());
    // A closed palette opens floating, through its own step.
    let before = h.state().handle().last_event();
    h.state().handle().set_panel_visible(&NAV, true);
    h.run();
    assert!(floating(&h, &NAV).is_some());
    assert!(events(&h, before).contains(&"palette.opened".to_string()));
}

#[test]
fn a_palette_is_dragged_by_its_band_snaps_and_docks_on_an_area() {
    let mut h = harness(false);
    let start = floating(&h, &TOOLS).unwrap().pos;
    let title = h.get_by_label("Tools").rect().center();
    let before = h.state().handle().last_event();
    drag_by(&mut h, title, egui::vec2(200.0, 120.0));
    let moved = floating(&h, &TOOLS).unwrap().pos;
    assert!(
        moved[0] > start[0] + 150.0 && moved[1] > start[1] + 80.0,
        "{start:?} {moved:?}"
    );
    let names = events(&h, before);
    assert!(names.contains(&"palette.moved".to_string()), "{names:?}");
    // Near the workbench's start edge: snapped onto it.
    let title = h.get_by_label("Tools").rect().center();
    drag_by(&mut h, title, egui::vec2(-moved[0] + 5.0, 0.0));
    assert_eq!(floating(&h, &TOOLS).unwrap().pos[0], 0.0, "snapped");
    // Far out: kept inside.
    let title = h.get_by_label("Tools").rect().center();
    drag_by(&mut h, title, egui::vec2(0.0, -400.0));
    let pos = floating(&h, &TOOLS).unwrap().pos;
    assert!(pos[0] >= 0.0 && pos[1] >= 0.0, "{pos:?}");
    // Dropped over the bottom panel: docked there.
    let title = h.get_by_label("Tools").rect().center();
    let target = h.get_by_label("log body").rect().center();
    drag_by(&mut h, title, target - title);
    assert!(floating(&h, &TOOLS).is_none());
    assert_eq!(
        h.state().handle().layout().area_of(&TOOLS),
        Some(Area::Bottom)
    );
}

#[test]
fn tab_hides_and_shows_the_palettes_but_not_a_pinned_one() {
    let mut h = harness(false);
    h.state().handle().set_panel_visible(&NAV, true);
    h.run();
    assert!(h.query_by_label("Overview").is_some());
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(palettes::hidden(h.state().handle()));
    assert!(h.query_by_label("Brush").is_none());
    assert!(h.query_by_label("Overview").is_none());
    assert!(h.query_by_label("log body").is_some(), "docked panels stay");
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.query_by_label("Brush").is_some());
    // Pinned: it stays.
    h.get_by_role_and_label(Role::Button, "Pin Tools").click();
    h.run();
    assert!(floating(&h, &TOOLS).unwrap().pinned);
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.query_by_label("Brush").is_some(), "pinned");
    assert!(h.query_by_label("Overview").is_none());
}

#[test]
fn a_palette_takes_the_keyboard_only_when_clicked_and_escape_gives_it_back() {
    let mut h = harness(false);
    let field = || Role::TextInput;
    assert!(!h.get_by_role(field()).is_focused(), "not when it opens");
    h.get_by_role(field()).click();
    h.run();
    assert!(h.get_by_role(field()).is_focused());
    // Tab is focus navigation while a widget has the keyboard.
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!h.get_by_role(field()).is_focused());
    assert!(h.query_by_label("Brush").is_some(), "still open");
    // Back with the canvas: Tab is the palettes' toggle again.
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.query_by_label("Brush").is_none());
}

#[test]
fn a_palette_rolls_up_to_its_band_and_closes_from_it() {
    let mut h = harness(false);
    h.get_by_role_and_label(Role::Button, "Roll up Tools")
        .click();
    h.run();
    assert!(floating(&h, &TOOLS).unwrap().rolled);
    assert!(h.query_by_label("Brush").is_none());
    assert!(h.query_by_label("Tools").is_some(), "its band stays");
    h.get_by_role_and_label(Role::Button, "Roll down Tools")
        .click();
    h.run();
    assert!(h.query_by_label("Brush").is_some());
    h.get_by_role_and_label(Role::Button, "Close Tools").click();
    h.run();
    assert!(floating(&h, &TOOLS).is_none());
    assert!(!h.state().handle().panel_visible(&TOOLS));
    // Its toggle opens it floating again.
    h.state().handle().set_panel_visible(&TOOLS, true);
    h.run();
    assert!(floating(&h, &TOOLS).is_some());
}

#[test]
fn hooks_veto_a_dock_and_redirect_a_move_and_reset_puts_palettes_back() {
    let mut h = harness(true);
    let title = h.get_by_label("Tools").rect().center();
    drag_by(&mut h, title, egui::vec2(220.0, 140.0));
    assert_eq!(
        floating(&h, &TOOLS).unwrap().pos,
        [100.0, 100.0],
        "redirected"
    );
    let title = h.get_by_label("Tools").rect().center();
    let target = h.get_by_label("log body").rect().center();
    drag_by(&mut h, title, target - title);
    assert_eq!(
        floating(&h, &TOOLS).unwrap().pos,
        [100.0, 100.0],
        "the dock was vetoed: back where it started"
    );
    h.state().handle().run(CMD_RESET, Json::Null);
    h.run();
    // The move step still redirects: reset goes through it.
    assert_eq!(floating(&h, &TOOLS).unwrap().pos, [100.0, 100.0]);
    let mut h = harness(false);
    let title = h.get_by_label("Tools").rect().center();
    drag_by(&mut h, title, egui::vec2(220.0, 140.0));
    h.state().handle().run(CMD_RESET, Json::Null);
    h.run();
    assert_eq!(floating(&h, &TOOLS).unwrap().pos, Floating::cascade(0));
}
