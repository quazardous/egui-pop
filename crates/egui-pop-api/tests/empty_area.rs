//! An area every tab was moved out of stays a drop target: the centre,
//! emptied by dragging its tabs away (or by `workbench.move`), takes a tab
//! dragged back onto it, and so does a side or bottom area emptied the
//! same way; a layout saved with an empty centre restores to one that
//! still takes a drop.

use egui::{Pos2, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::layout::{Layout, Node};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_stack::DropZone;

const DOC_A: PanelId = PanelId::from_static("demo.doc_a");
const DOC_B: PanelId = PanelId::from_static("demo.doc_b");
const FILES: PanelId = PanelId::from_static("demo.files");
const CONSOLE: PanelId = PanelId::from_static("demo.console");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let body = |text: &'static str| {
            move |ui: &mut egui::Ui, _: &egui_pop_api::AppHandle| {
                ui.label(text);
            }
        };
        ctx.panel(
            PanelSpec::new(DOC_A, "Doc A", Area::Center).floatable(true),
            body("doc a body"),
        )?
        .panel(
            PanelSpec::new(DOC_B, "Doc B", Area::Center),
            body("doc b body"),
        )?
        .panel(
            PanelSpec::new(FILES, "Files", Area::StartSide),
            body("files body"),
        )?
        .panel(
            PanelSpec::new(CONSOLE, "Console", Area::Bottom),
            body("console body"),
        )?;
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn tab(h: &Harness<'_, App>, title: &str) -> Rect {
    h.get_by_role_and_label(egui::accesskit::Role::Button, title)
        .rect()
}

fn area(h: &Harness<'_, App>, area: Area) -> Rect {
    h.state()
        .handle()
        .area_rect(area)
        .expect("the area is drawn")
}

fn tree(h: &Harness<'_, App>, area: Area) -> Option<Node> {
    h.state()
        .handle()
        .layout()
        .areas
        .into_iter()
        .find(|(a, _)| *a == area)
        .map(|(_, n)| n)
}

fn group(node: Option<Node>) -> Option<Vec<PanelId>> {
    match node? {
        Node::Tabs { panels, .. } => Some(panels),
        _ => None,
    }
}

/// Presses on `from` and drags to `to` a frame at a time, held there.
fn drag(h: &mut Harness<'_, App>, from: Pos2, to: Pos2) {
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

fn release(h: &mut Harness<'_, App>, at: Pos2) {
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
}

/// Drags the tab titled `title` into the middle of `to`'s group and lets
/// it go there.
fn drag_tab_into(h: &mut Harness<'_, App>, title: &str, to: Area) {
    let from = tab(h, title).center();
    let at = area(h, to).center();
    drag(h, from, at);
    release(h, at);
}

/// Empties the centre the way a user does: each of its tabs dragged into
/// the start side bar's group.
fn empty_the_centre_by_drag(h: &mut Harness<'_, App>) {
    drag_tab_into(h, "Doc A", Area::StartSide);
    drag_tab_into(h, "Doc B", Area::StartSide);
    assert_eq!(tree(h, Area::Center), None, "the centre is empty");
    assert_eq!(
        group(tree(h, Area::StartSide)),
        Some(vec![FILES, DOC_A, DOC_B])
    );
}

/// Drags `title` onto the middle of the (empty) centre and checks it docks
/// there.
fn drag_back_to_the_centre(h: &mut Harness<'_, App>, title: &str, panel: &PanelId) {
    let app = h.state().handle().clone();
    let centre = app
        .area_rect(Area::Center)
        .expect("an empty centre keeps its rect");
    let from = tab(h, title).center();
    let to = centre.center();
    drag(h, from, to);
    let hint = app
        .drop_hint()
        .expect("the empty centre is a drop target while a tab drags");
    assert_eq!((hint.area, hint.zone), ("center", DropZone::Center));
    assert!(hint.refusal.is_none());
    release(h, to);
    assert_eq!(group(tree(h, Area::Center)), Some(vec![panel.clone()]));
}

#[test]
fn the_centre_emptied_by_drag_takes_a_tab_dragged_back() {
    let mut h = harness();
    empty_the_centre_by_drag(&mut h);
    // The empty centre says what it is for, to the eye and to a reader.
    assert!(
        h.query_by_label("Drop a panel here").is_some(),
        "the empty centre's placeholder"
    );
    drag_back_to_the_centre(&mut h, "Doc A", &DOC_A);
    assert!(h.query_by_label("doc a body").is_some(), "shown there");
    assert!(h.query_by_label("Drop a panel here").is_none());
}

#[test]
fn the_centre_emptied_through_moves_takes_a_tab_dragged_back() {
    let mut h = harness();
    let app = h.state().handle().clone();
    for panel in [&DOC_A, &DOC_B] {
        app.move_panel(panel, Area::Bottom).expect("moved");
        h.run();
    }
    assert_eq!(tree(&h, Area::Center), None, "the centre is empty");
    drag_back_to_the_centre(&mut h, "Doc B", &DOC_B);
}

#[test]
fn a_layout_saved_with_an_empty_centre_restores_and_takes_a_drop() {
    let mut h = harness();
    empty_the_centre_by_drag(&mut h);
    let app = h.state().handle().clone();
    let saved = app.layout().to_json();
    let read = Layout::from_json(&saved).unwrap();
    assert!(read.areas.iter().all(|(a, _)| *a != Area::Center));
    // A fresh application, the layout applied to it.
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(app.set_layout(read).is_empty(), "nothing to repair");
    h.run();
    assert_eq!(tree(&h, Area::Center), None, "restored empty");
    drag_back_to_the_centre(&mut h, "Doc B", &DOC_B);
}

#[test]
fn a_side_and_the_bottom_area_emptied_by_drag_take_a_tab_back() {
    let mut h = harness();
    let app = h.state().handle().clone();
    // The start side bar's only tab into the centre: it closes.
    drag_tab_into(&mut h, "Files", Area::Center);
    assert_eq!(tree(&h, Area::StartSide), None);
    // Back along the areas' left edge, where a closed side area is offered.
    let centre = area(&h, Area::Center);
    let from = tab(&h, "Files").center();
    let to = egui::pos2(centre.left() + 10.0, centre.center().y);
    drag(&mut h, from, to);
    let hint = app.drop_hint().expect("the closed side area offered");
    assert_eq!(hint.area, "start_side");
    release(&mut h, to);
    assert_eq!(group(tree(&h, Area::StartSide)), Some(vec![FILES]));
    // The bottom area's only tab into the centre, then back along the
    // bottom edge.
    drag_tab_into(&mut h, "Console", Area::Center);
    assert_eq!(tree(&h, Area::Bottom), None);
    let centre = area(&h, Area::Center);
    let from = tab(&h, "Console").center();
    let to = egui::pos2(centre.center().x, centre.bottom() - 10.0);
    drag(&mut h, from, to);
    let hint = app.drop_hint().expect("the closed bottom area offered");
    assert_eq!(hint.area, "bottom");
    release(&mut h, to);
    assert_eq!(group(tree(&h, Area::Bottom)), Some(vec![CONSOLE]));
}

#[test]
fn a_palette_dropped_on_the_empty_centre_with_the_dock_keys_docks_there() {
    let mut h = harness();
    let app = h.state().handle().clone();
    for panel in [&DOC_A, &DOC_B] {
        app.move_panel(panel, Area::Bottom).expect("moved");
        h.run();
    }
    assert_eq!(tree(&h, Area::Center), None, "the centre is empty");
    // Doc A floats over the interface (a docked floatable panel does).
    app.change_workbench(egui_pop_api::workbench::Change::Float(
        DOC_A.clone(),
        [300.0, 120.0],
    ));
    h.run();
    assert!(app.layout().floating_of(&DOC_A).is_some());
    let band = h.get_by_label("Doc A").rect().center();
    let to = area(&h, Area::Center).center();
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
    h.run();
    h.drag_at(band);
    h.run();
    for i in 1..=4 {
        h.hover_at(band + (to - band) * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(to);
    h.run();
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run();
    assert!(app.layout().floating_of(&DOC_A).is_none(), "docked");
    assert_eq!(group(tree(&h, Area::Center)), Some(vec![DOC_A.clone()]));
}
