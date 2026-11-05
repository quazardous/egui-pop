//! Auxiliary windows: a panel detached into a window of its own, docked
//! back, its window's closing a step a plugin may veto; panels that may
//! not leave say so; other panels joining a window as tabs, by a drag,
//! the tab menu or a command.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::{Before, Outcome};
use egui_pop_api::workbench::{self, Change, STEP_CLOSE_WINDOW, STEP_JOIN_WINDOW};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};

const LOG: PanelId = PanelId::from_static("demo.log");
const PINNED: PanelId = PanelId::from_static("demo.pinned");
const TREE: PanelId = PanelId::from_static("demo.tree");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?
        .panel(
            PanelSpec::new(PINNED, "Pinned", Area::EndSide).detachable(false),
            |ui, _| {
                ui.label("pinned body");
            },
        )?
        // Beside the pinned panel: kittest draws a detached window at the
        // top left, over the start side bar.
        .panel(PanelSpec::new(TREE, "Tree", Area::EndSide), |ui, _| {
            ui.label("tree body");
        })?;
        ctx.before(STEP_CLOSE_WINDOW, |app, _| {
            if app.context().get("busy") == "true" {
                Before::Veto("the log is busy".into())
            } else {
                Before::Go
            }
        });
        ctx.before(STEP_JOIN_WINDOW, |app, _| {
            let n: u32 = app.context().get("joins").parse().unwrap_or(0);
            app.set_context("joins", &(n + 1).to_string());
            if app.context().get("busy") == "true" {
                Before::Veto("the log is busy".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
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
fn a_panel_detaches_into_its_own_window_and_docks_back() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(app.detach_panel(&LOG).unwrap().ok());
    h.run();
    let layout = app.layout();
    assert_eq!(layout.area_of(&LOG), None);
    assert_eq!(layout.auxiliary.len(), 1);
    assert_eq!(layout.auxiliary[0].panel, LOG);
    // Drawn in its window (kittest embeds it in the main one).
    assert!(h.query_by_label("log body").is_some());
    // The window's closing is vetoed while busy, then docks it back.
    app.set_context("busy", "true");
    let out = app.change_workbench(Change::CloseWindow(LOG)).unwrap();
    assert_eq!(out.vetoed(), Some("the log is busy"));
    assert_eq!(app.layout().auxiliary.len(), 1);
    app.set_context("busy", "false");
    assert!(app.change_workbench(Change::CloseWindow(LOG)).unwrap().ok());
    h.run();
    assert_eq!(app.layout().area_of(&LOG), Some(Area::Bottom));
    assert!(app.layout().auxiliary.is_empty());
    assert!(h.query_by_label("log body").is_some());
}

#[test]
fn a_panel_that_may_not_leave_says_so() {
    let h = harness();
    let out = h.state().handle().detach_panel(&PINNED).unwrap();
    assert!(
        matches!(out, Outcome::Done(Err(ref e)) if e.contains("cannot be detached")),
        "{out:?}"
    );
    assert_eq!(
        h.state().handle().layout().area_of(&PINNED),
        Some(Area::EndSide)
    );
}

#[test]
fn a_saved_window_place_is_repaired_before_it_opens() {
    let mut h = harness();
    let app = h.state().handle().clone();
    app.detach_panel(&LOG);
    // A place far off every screen and a size past what can be drawn.
    let mut layout = app.layout();
    layout.auxiliary[0].rect = [1.0e7, 1.0e7, 20_000.0, 10.0];
    app.set_layout(layout);
    h.run();
    assert!(h.query_by_label("log body").is_some());
    assert_eq!(workbench::window_id(&LOG), workbench::window_id(&LOG));
}

#[test]
fn a_detached_window_is_titled_with_the_application_name_not_the_main_title() {
    let app = App::builder()
        .name("Demo")
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    assert_eq!(app.handle().name(), "Demo");
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    // The main window's title names a document as well.
    h.input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .title = Some("notes.txt — Demo".into());
    h.run();
    let app = h.state().handle().clone();
    assert!(app.detach_panel(&LOG).unwrap().ok());
    h.run();
    // kittest embeds the window in the main one, titled as its own.
    assert!(h.query_by_label("Log — Demo").is_some());
    assert!(h.query_by_label_contains("notes.txt").is_none());
}

/// Presses on `from`, drags to `to` a step at a time, and lets go
/// unless `hold`.
fn drag(h: &mut Harness<'_, App>, from: egui::Pos2, to: egui::Pos2, hold: bool) {
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
    h.step();
    if !hold {
        release(h, to);
    }
}

fn release(h: &mut Harness<'_, App>, at: egui::Pos2) {
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
}

fn joins(app: &egui_pop_api::AppHandle) -> u32 {
    app.context().get("joins").parse().unwrap_or(0)
}

#[test]
fn a_tab_dropped_on_a_detached_window_joins_its_tabs_in_one_step() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(app.detach_panel(&LOG).unwrap().ok());
    h.run();
    // kittest embeds the window in the main one: it sees the drag.
    let from = h.get_by_label("Tree").rect().center();
    let to = h.get_by_label("log body").rect().center();
    drag(&mut h, from, to, true);
    let over = app.window_drop().expect("the window shows its drop zone");
    assert_eq!((over.panel.clone(), over.window.clone()), (TREE, LOG));
    assert_eq!(over.zone, egui_stack::DropZone::Center, "tabs only");
    assert!(over.refusal.is_none());
    assert!(
        app.drop_hint().is_none(),
        "the window shows it, not an area"
    );
    let snap = app.debug_snapshot();
    assert_eq!(snap["workbench"]["window_drop"]["window"], "demo.log");
    assert_eq!(snap["workbench"]["window_drop"]["zone"], "center");
    release(&mut h, to);
    assert!(app.window_drop().is_none());
    assert_eq!(joins(&app), 1, "one workbench.join_window step");
    let layout = app.layout();
    assert_eq!(layout.area_of(&TREE), None);
    let window = layout.window_of(&TREE).expect("in the window");
    assert_eq!(window.panel, LOG);
    assert_eq!(window.tabs, vec![TREE]);
    assert_eq!(window.shown(), &TREE);
    assert_eq!(layout.auxiliary.len(), 1);
    // The window shows its tabs, the joined one in front.
    assert!(h.query_by_label("tree body").is_some());
    assert!(h.query_by_label("Log").is_some());
    // The layout round-trips with the window's tabs.
    let saved = egui_pop_model::layout::Layout::from_json(&layout.to_json()).unwrap();
    assert_eq!(saved, layout);
    // Closing the window docks both back.
    assert!(app.change_workbench(Change::CloseWindow(LOG)).unwrap().ok());
    h.run();
    let layout = app.layout();
    assert!(layout.auxiliary.is_empty());
    assert_eq!(layout.area_of(&LOG), Some(Area::Bottom));
    assert_eq!(layout.area_of(&TREE), Some(Area::EndSide));
}

#[test]
fn a_panel_that_may_not_leave_is_refused_by_the_window_it_is_dropped_on() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(app.detach_panel(&LOG).unwrap().ok());
    h.run();
    let from = h.get_by_label("Pinned").rect().center();
    let to = h.get_by_label("log body").rect().center();
    drag(&mut h, from, to, true);
    let over = app.window_drop().expect("the window shows its zone");
    assert_eq!(over.panel, PINNED);
    let why = over.refusal.expect("disabled, with its reason");
    assert!(why.contains("Pinned"), "{why}");
    release(&mut h, to);
    assert_eq!(app.layout().area_of(&PINNED), Some(Area::EndSide));
    assert_eq!(app.layout().auxiliary[0].tabs, Vec::<PanelId>::new());
}

#[test]
fn a_veto_keeps_a_dropped_tab_where_it_was() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(app.detach_panel(&LOG).unwrap().ok());
    h.run();
    app.set_context("busy", "true");
    let from = h.get_by_label("Tree").rect().center();
    let to = h.get_by_label("log body").rect().center();
    drag(&mut h, from, to, false);
    assert_eq!(joins(&app), 1);
    assert_eq!(app.layout().area_of(&TREE), Some(Area::EndSide));
    assert!(app.layout().window_of(&TREE).is_none());
}

#[test]
fn the_tab_menu_moves_a_panel_into_a_window_where_a_drag_cannot_reach() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(app.detach_panel(&LOG).unwrap().ok());
    h.run();
    h.get_by_label("Tree").click_secondary();
    h.run();
    let submenu = h.get_by_label("Move to window").rect();
    h.get_by_label("Move to window").hover();
    h.run();
    // The windows, by their panels' titles: the Log's, in the submenu
    // (beside the menu; the window's title says Log too).
    let entry = h
        .get_all_by_label("Log")
        .map(|n| n.rect())
        .find(|r| {
            (r.center().y - submenu.center().y).abs() < submenu.height() * 2.0
                && (r.left() >= submenu.right() - 8.0 || r.right() <= submenu.left() + 8.0)
        })
        .expect("the submenu lists the Log's window");
    h.input_mut()
        .events
        .push(egui::Event::PointerMoved(entry.center()));
    h.run();
    h.get_all_by_label("Log")
        .find(|n| n.rect() == entry)
        .unwrap()
        .click();
    h.run();
    assert_eq!(joins(&app), 1);
    let layout = app.layout();
    assert_eq!(layout.window_of(&TREE).map(|w| w.panel.clone()), Some(LOG));
    // Out again from the window's own tab menu: Detach gives it a
    // window of its own.
    assert!(app.detach_panel(&TREE).unwrap().ok());
    let layout = app.layout();
    assert_eq!(layout.auxiliary.len(), 2);
    assert!(layout.auxiliary.iter().all(|a| a.tabs.is_empty()));
}

#[test]
fn an_agent_moves_a_panel_into_a_window_by_command() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(app.detach_panel(&LOG).unwrap().ok());
    h.run();
    let agent = egui_pop_model::commands::Origin::Agent("t".into());
    let out = h.state_mut().run(
        &workbench::CMD_MOVE_TO_WINDOW,
        serde_json::json!({ "panel": "demo.tree", "window": "demo.log" }),
        agent.clone(),
    );
    let out = out.now();
    assert!(matches!(out, Some(Ok(_))), "{out:?}");
    h.run();
    assert_eq!(app.layout().auxiliary[0].tabs, vec![TREE]);
    // A panel that is in no window names none.
    let out = h.state_mut().run(
        &workbench::CMD_MOVE_TO_WINDOW,
        serde_json::json!({ "panel": "demo.log", "window": "demo.pinned" }),
        agent,
    );
    let out = out.now();
    assert!(matches!(out, Some(Err(_))), "{out:?}");
    // Attach docks one tab back, the window keeps the other.
    assert!(app.attach_panel(&LOG).unwrap().ok());
    let layout = app.layout();
    assert_eq!(layout.area_of(&LOG), Some(Area::Bottom));
    assert_eq!(layout.auxiliary[0].panel, TREE, "the window goes on");
}
