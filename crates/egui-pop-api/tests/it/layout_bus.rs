//! The layout changed, on the bus (ADR 0029, "Panels and layout").
//!
//! Every real change of the workbench's layout (a panel docked, detached,
//! floated, moved, resized, opened or closed) is said once as
//! `pop.layout.changed`, once the gesture that made it ended; the layout is
//! saved by its listener (`pop.layout.save`), not by a comparison each pass.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::app::STEP_LAYOUT_SAVE;
use egui_pop_api::{App, AppBuilder, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};

const LOG: PanelId = PanelId::from_static("demo.log");
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
            PanelSpec::new(TREE, "Tree", Area::StartSide).default_size(220.0),
            |ui, _| {
                ui.label("tree body");
            },
        )?;
        Ok(())
    }
}

fn builder(dir: &std::path::Path) -> AppBuilder {
    App::builder().plugin(Demo).persist(dir)
}

fn harness(builder: AppBuilder) -> Harness<'static, App> {
    let app = builder.build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn dir(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("egui-pop-layout-bus-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// How many times the layout was saved (its `layout.save` step ran).
fn saves(h: &Harness<'_, App>) -> usize {
    h.state()
        .handle()
        .step_traces()
        .iter()
        .filter(|t| t.step == STEP_LAYOUT_SAVE)
        .count()
}

fn area_of(h: &Harness<'_, App>, p: &PanelId) -> Option<Area> {
    h.state().handle().layout().area_of(p)
}

#[test]
fn a_dock_and_an_undock_are_each_saved_once_when_they_happen() {
    let dir = dir("dock");
    let mut h = harness(builder(&dir));
    assert_eq!(saves(&h), 0, "nothing changed since it started");
    // Docked in another area.
    let out = h.state().handle().move_panel(&TREE, Area::EndSide).unwrap();
    assert!(out.ok(), "{out:?}");
    h.run();
    assert_eq!(saves(&h), 1, "saved once, in the passes that follow it");
    assert!(dir.join("layout.json").exists());
    for _ in 0..5 {
        h.run();
    }
    assert_eq!(saves(&h), 1, "nothing changed since");
    // Undocked: detached to its own window.
    let out = h.state().handle().detach_panel(&LOG).unwrap();
    assert!(out.ok(), "{out:?}");
    h.run();
    assert_eq!(saves(&h), 2, "the detach saved once");
    let text = std::fs::read_to_string(dir.join("layout.json")).unwrap();
    assert!(text.contains("demo.log"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_tab_dragged_saves_nothing_until_it_is_let_go_then_once() {
    let dir = dir("drag");
    let mut h = harness(builder(&dir));
    let app = h.state().handle().clone();
    // The tab, not its body: the active tab's body is a panel named by it.
    let from = h
        .get_by_role_and_label(egui::accesskit::Role::Tab, "Tree")
        .rect()
        .center();
    let to = app.area_rect(Area::Bottom).unwrap().center();
    let pointer = |h: &mut Harness<'_, App>, e: egui::Event| {
        h.input_mut().events.push(e);
        h.step();
    };
    pointer(&mut h, egui::Event::PointerMoved(from));
    pointer(
        &mut h,
        egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    for i in 1..=8 {
        let at = from + (to - from) * (i as f32 / 8.0);
        pointer(&mut h, egui::Event::PointerMoved(at));
    }
    for _ in 0..5 {
        h.step();
    }
    assert!(app.drop_hint().is_some(), "still dragging");
    assert_eq!(saves(&h), 0, "a drag in progress saves nothing");
    pointer(
        &mut h,
        egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    h.run();
    assert_eq!(area_of(&h, &TREE), Some(Area::Bottom), "dropped there");
    assert_eq!(saves(&h), 1, "let go: saved once");
    for _ in 0..5 {
        h.run();
    }
    assert_eq!(saves(&h), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// What `pop.layout.changed` said, by a test listener after egui-pop's save.
fn said(h: &Harness<'_, App>) -> std::sync::Arc<std::sync::Mutex<Vec<Vec<String>>>> {
    use egui_pop_api::bus::{LAYOUT_SAVE, LayoutChanged, bus_contract::ListenOpts};
    let said: std::sync::Arc<std::sync::Mutex<Vec<Vec<String>>>> = std::sync::Arc::default();
    let into = said.clone();
    h.state()
        .handle()
        .bus()
        .listen::<LayoutChanged>(
            "test.said",
            ListenOpts::new().owner("test").after(LAYOUT_SAVE),
            move |c, ()| {
                into.lock().unwrap().push(c.causes.clone());
                Ok(())
            },
        )
        .unwrap();
    said
}

#[test]
fn each_real_change_is_said_once_after_its_save() {
    use egui_pop_api::workbench::Change;
    let dir = dir("said");
    let mut h = harness(builder(&dir));
    let said = said(&h);
    let app = h.state().handle().clone();
    app.move_panel(&TREE, Area::EndSide).unwrap();
    h.run();
    assert_eq!(*said.lock().unwrap(), [vec!["workbench.move".to_string()]]);
    assert_eq!(saves(&h), 1, "its listener saved it, before the test's");
    // A step that changes nothing is not a change.
    app.move_panel(&TREE, Area::EndSide).unwrap();
    h.run();
    assert_eq!(said.lock().unwrap().len(), 1, "nothing changed");
    // Closed, then opened again: two changes, two saves.
    app.change_workbench(Change::Hide(LOG)).unwrap();
    h.run();
    app.change_workbench(Change::Show(LOG)).unwrap();
    h.run();
    assert_eq!(said.lock().unwrap().len(), 3, "{:?}", said.lock().unwrap());
    assert_eq!(saves(&h), 3);
    // Two changes in one pass: said once.
    app.move_panel(&TREE, Area::StartSide).unwrap();
    app.change_workbench(Change::Hide(LOG)).unwrap();
    h.run();
    assert_eq!(said.lock().unwrap().len(), 4, "{:?}", said.lock().unwrap());
    assert_eq!(saves(&h), 4);
    // A layout loaded is what is on disk: not said, not saved again.
    let out = h.state_mut().load_layout().unwrap();
    assert!(out.ok(), "{out:?}");
    h.run();
    assert_eq!(said.lock().unwrap().len(), 4);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The reviewed order of the layout point's listeners.
#[test]
fn the_layout_point_runs_in_its_reviewed_order() {
    use egui_pop_api::bus::{self, LAYOUT_POINTS};
    use std::path::Path;
    let dir = dir("order");
    let h = harness(builder(&dir));
    let app = h.state().handle();
    let dump = app.bus_order_of(&LAYOUT_POINTS);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/layout-points.txt");
    if std::env::var_os("UPDATE_BUS_ORDER").is_some() {
        std::fs::write(&path, &dump).unwrap();
    }
    let reviewed = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        dump,
        reviewed,
        "review it, then UPDATE_BUS_ORDER=1 writes {}",
        path.display()
    );
    assert!(dump.contains(bus::LAYOUT_SAVE), "{dump}");
    assert_eq!(
        bus::missing_targets(&app.bus(), &LAYOUT_POINTS),
        Vec::<String>::new()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
