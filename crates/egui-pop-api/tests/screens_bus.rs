//! The screens on the bus: a screen unplugged is said as
//! `pop.screen.removed`, by its name, and every window on it moves onto a
//! screen left (an application window, the main window) or docks back (a
//! detached panel's window); invariant 6 ("a positioned window meets a
//! screen") holds. Where positions are hidden (Wayland), nothing moves.

use std::cell::RefCell;
use std::rc::Rc;

use bus_contract::{ListenOpts, Point as _};
use egui::{ViewportCommand, ViewportId, vec2};
use egui_pop_api::bus::{self, SCREEN_POINTS, ScreenAdded, ScreenChange, ScreenRemoved};
use egui_pop_api::window::{self, Screen};
use egui_pop_api::window_invariants::ON_SCREEN;
use egui_pop_api::windows::WindowSpec;
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError, workbench};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_test::windows::MultiWindow;

const LOG: PanelId = PanelId::from_static("demo.log");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?;
        Ok(())
    }
}

/// A laptop's panel and, at its right, a monitor, named as winit names
/// them.
fn panel() -> Screen {
    Screen::from_physical(Some("eDP-1".into()), (0, 0), (2560, 1600), 2.0)
}

fn monitor() -> Screen {
    Screen::from_physical(Some("DP-1".into()), (1280, 0), (1920, 1080), 1.0)
}

/// What `pop.screen.added` and `pop.screen.removed` said, in order.
type Heard = Rc<RefCell<Vec<(String, ScreenChange)>>>;

/// An application with both screens told, its main window on the panel;
/// what the screen points say is heard.
fn run() -> (MultiWindow, Heard) {
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .plugin(Demo)
            .build(ctx)
            .unwrap()
    });
    let heard: Heard = Rc::default();
    let bus = m.app().bus();
    for (point, said) in [
        (ScreenAdded::ID, heard.clone()),
        (ScreenRemoved::ID, heard.clone()),
    ] {
        let opts = ListenOpts::new().owner("test").last();
        let listened = if point == ScreenAdded::ID {
            bus.listen_local::<ScreenAdded>("test.added", opts, move |c, _| {
                said.borrow_mut().push((ScreenAdded::ID.into(), c.clone()));
                Ok(())
            })
        } else {
            bus.listen_local::<ScreenRemoved>("test.removed", opts, move |c, _| {
                said.borrow_mut()
                    .push((ScreenRemoved::ID.into(), c.clone()));
                Ok(())
            })
        };
        listened.unwrap();
    }
    m.set_screen_list(vec![panel(), monitor()]);
    m.set_outer_position(ViewportId::ROOT, [100.0, 100.0]);
    m.root_pass();
    m.root_pass();
    (m, heard)
}

fn on_the_monitor() -> [f32; 2] {
    (monitor().rect.min + vec2(200.0, 100.0)).into()
}

/// Unplugs the monitor: the host tells the panel alone, and the main
/// window's next passes follow.
fn unplug(m: &mut MultiWindow) {
    m.set_screen_list(vec![panel()]);
    m.root_pass();
}

fn names(heard: &Heard) -> Vec<(String, Option<String>)> {
    heard
        .borrow()
        .iter()
        .map(|(p, c)| (p.clone(), c.name.clone()))
        .collect()
}

fn moved_to(m: &MultiWindow, v: ViewportId) -> Option<egui::Pos2> {
    m.commands(v).into_iter().find_map(|c| match c {
        ViewportCommand::OuterPosition(at) => Some(at),
        _ => None,
    })
}

#[test]
fn the_screen_points_are_declared_with_their_listeners_in_order() {
    let (m, _) = run();
    let dump = m.app().bus_order_of(&SCREEN_POINTS);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/bus/screen-points.txt");
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
    assert!(dump.contains(bus::WORKBENCH_SCREENS), "{dump}");
    assert!(dump.contains(bus::PLACE), "{dump}");
    for p in SCREEN_POINTS {
        assert!(
            bus::CONTRACT.iter().any(|d| d.id == p),
            "{p} in the contract"
        );
        assert!(bus::POINTS.contains(&p), "{p} in the points");
    }
}

#[test]
fn the_screens_first_told_are_not_said_a_screen_plugged_in_is() {
    let (mut m, heard) = run();
    assert!(names(&heard).is_empty(), "{:?}", names(&heard));
    m.set_screen_list(vec![panel()]);
    m.root_pass();
    m.set_screen_list(vec![panel(), monitor()]);
    m.root_pass();
    assert_eq!(
        names(&heard),
        vec![
            ("pop.screen.removed".into(), Some("DP-1".into())),
            ("pop.screen.added".into(), Some("DP-1".into())),
        ]
    );
    let said = heard.borrow()[0].1.clone();
    assert_eq!(said.rect, [1280.0, 0.0, 1920.0, 1080.0]);
    assert_eq!(said.scale, 1.0);
}

#[test]
fn a_detached_window_on_a_screen_unplugged_docks_back() {
    let (mut m, heard) = run();
    assert!(m.app().detach_panel(&LOG).unwrap().ok());
    m.root_pass();
    m.root_pass();
    let v = workbench::window_id(&LOG);
    assert!(m.shown_immediate().contains(&v));
    m.set_outer_position(v, on_the_monitor());
    m.root_pass();
    let kept = m.app().layout().window_of(&LOG).cloned().expect("detached");
    assert_eq!(kept.screen, "DP-1", "kept on the monitor, by its name");

    unplug(&mut m);
    assert_eq!(
        names(&heard),
        vec![("pop.screen.removed".into(), Some("DP-1".into()))]
    );
    m.root_pass();
    m.root_pass();
    let layout = m.app().layout();
    assert!(layout.auxiliary.is_empty(), "{:?}", layout.auxiliary);
    assert!(layout.area_of(&LOG).is_some(), "the log docked back");
    assert!(!m.shown_immediate().contains(&v));
    assert!(
        m.app().invariants_broken_now().is_empty(),
        "{:?}",
        m.app().broken_invariants()
    );
}

#[test]
fn a_detached_window_it_cannot_name_a_screen_for_moves_onto_one_left() {
    let (mut m, _) = run();
    assert!(m.app().detach_panel(&LOG).unwrap().ok());
    m.root_pass();
    m.root_pass();
    let v = workbench::window_id(&LOG);
    // Kept on a screen it could not name (a run that told no names).
    let mut layout = m.app().layout();
    layout.auxiliary[0].screen = window::UNNAMED_SCREEN.into();
    m.app().set_layout(layout);
    m.root_pass();
    m.set_outer_position(v, on_the_monitor());
    let mut layout = m.app().layout();
    layout.auxiliary[0].screen = window::UNNAMED_SCREEN.into();
    m.app().set_layout(layout);

    unplug(&mut m);
    let at = moved_to(&m, v).expect("the window is moved");
    assert!(panel().rect.contains(at), "{at:?} is on the panel");
    for _ in 0..5 {
        m.root_pass();
    }
    assert!(m.app().layout().window_of(&LOG).is_some(), "still detached");
    let r = m.outer_rect(v).expect("a position");
    assert!(panel().rect.intersects(r), "{r:?}");
    assert!(m.app().invariants_broken_now().is_empty());
}

#[test]
fn an_app_window_on_a_screen_unplugged_moves_onto_one_left() {
    let (mut m, heard) = run();
    m.app()
        .open_window(WindowSpec::new("a", "A").size([300.0, 200.0]), |ui, _| {
            ui.label("window A");
        });
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).expect("a is open");
    m.set_outer_position(va, on_the_monitor());
    m.window_pass(va);
    m.root_pass();

    unplug(&mut m);
    assert_eq!(
        names(&heard),
        vec![("pop.screen.removed".into(), Some("DP-1".into()))]
    );
    let at = moved_to(&m, va).expect("the window is moved");
    assert!(panel().rect.contains(at), "{at:?} is on the panel");
    // Its builder asks that place from now on: nothing moves it back.
    m.root_pass();
    let b = m.builder(va).expect("a builder");
    if let Some(p) = b.position {
        assert!(panel().rect.contains(p), "{p:?}");
    }
    for _ in 0..5 {
        m.window_pass(va);
        m.root_pass();
    }
    let r = m.outer_rect(va).expect("a position");
    assert!(panel().rect.intersects(r), "{r:?}");
    assert!(m.app().invariants_broken_now().is_empty());
}

#[test]
fn the_main_window_on_a_screen_unplugged_moves_onto_one_left() {
    let (mut m, _) = run();
    m.set_outer_position(ViewportId::ROOT, on_the_monitor());
    m.root_pass();
    unplug(&mut m);
    let at = moved_to(&m, ViewportId::ROOT).expect("the main window is moved");
    assert!(panel().rect.contains(at), "{at:?} is on the panel");
}

#[test]
fn where_positions_are_hidden_nothing_moves() {
    let (mut m, heard) = run();
    m.app()
        .open_window(WindowSpec::new("a", "A").size([300.0, 200.0]), |ui, _| {
            ui.label("window A");
        });
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).expect("a is open");
    assert!(m.app().detach_panel(&LOG).unwrap().ok());
    m.root_pass();
    m.root_pass();
    let v = workbench::window_id(&LOG);
    m.hide_positions();
    m.root_pass();
    unplug(&mut m);
    // Said all the same: the system told the screens.
    assert_eq!(
        names(&heard),
        vec![("pop.screen.removed".into(), Some("DP-1".into()))]
    );
    assert_eq!(moved_to(&m, va), None);
    assert_eq!(moved_to(&m, v), None);
    assert_eq!(moved_to(&m, ViewportId::ROOT), None);
    let kept = m.app().layout().window_of(&LOG).cloned();
    assert_eq!(
        kept.map(|a| a.screen),
        Some(window::UNNAMED_SCREEN.to_string()),
        "the system places it: it stays detached"
    );
    for _ in 0..5 {
        m.root_pass();
    }
    assert!(m.app().invariants_broken_now().is_empty());
}

#[test]
fn a_positioned_window_on_no_screen_breaks_invariant_6() {
    let (mut m, _) = run();
    m.allow_broken(ON_SCREEN);
    m.app()
        .open_window(WindowSpec::new("a", "A").size([300.0, 200.0]), |ui, _| {
            ui.label("window A");
        });
    m.root_pass();
    let va = m.app().window_viewport(&"a".into()).expect("a is open");
    // Off every screen, no screen changing: nothing moves it.
    m.set_outer_position(va, [9000.0, 9000.0]);
    for _ in 0..5 {
        m.root_pass();
    }
    let broken = m.app().broken_invariants();
    assert!(
        broken
            .iter()
            .any(|b| b.rule == ON_SCREEN && b.window == "a"),
        "{broken:?}"
    );
    // Back on a screen: it holds again.
    m.set_outer_position(va, [100.0, 100.0]);
    m.root_pass();
    assert!(m.app().invariants_broken_now().is_empty());
}
