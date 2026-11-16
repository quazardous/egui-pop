//! A shortcut in progress belongs to one window: each window keys with
//! its own dispatcher (the main window, each detached panel's window,
//! each application window), and a shortcut in progress is dropped when
//! another window becomes the active one (`window.activated`). The first
//! key of a chord pressed in one window and its second in another runs
//! nothing; a chord pressed whole in a detached panel's window runs, in
//! that window's context (`appWindow` the main window's, `window` its
//! panel). Driven headless (`egui_pop_test::windows::MultiWindow`).

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Key, Modifiers, ViewportId};
use egui_pop_api::workbench;
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_test::windows::MultiWindow;
use serde_json::Value as Json;

const LOG: PanelId = PanelId::from_static("demo.log");
const GO: CommandId = CommandId::from_static("demo.go");
const IN_LOG: CommandId = CommandId::from_static("demo.in_log");

/// The commands that ran, by id.
type Ran = Rc<RefCell<Vec<String>>>;

struct Demo(Ran);

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let (r1, r2) = (self.0.clone(), self.0.clone());
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?
        .command(
            CommandSpec::new(GO, "Go", "Demo").key("Ctrl+K Ctrl+G"),
            move |_| {
                r1.borrow_mut().push("go".into());
                Ok(Json::Null)
            },
        )?
        // Only in the log's window, which belongs to the main window.
        .command(
            CommandSpec::new(IN_LOG, "In the log", "Demo")
                .key("Ctrl+K Ctrl+L")
                .when("appWindow == 'main' && window == 'demo.log'"),
            move |_| {
                r2.borrow_mut().push("in_log".into());
                Ok(Json::Null)
            },
        )?;
        Ok(())
    }
}

/// The main window and the log detached into its own window: its
/// viewport.
fn detached() -> (MultiWindow, ViewportId, Ran) {
    let ran = Ran::default();
    let r = ran.clone();
    let mut m = MultiWindow::new(move |ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .plugin(Demo(r))
            .build(ctx)
            .unwrap()
    });
    m.root_pass();
    assert!(m.app().detach_panel(&LOG).unwrap().ok());
    m.root_pass();
    m.root_pass();
    let panel = workbench::window_id(&LOG);
    assert!(m.shown_immediate().contains(&panel));
    (m, panel, ran)
}

/// Presses a key in a window with the focus, and runs the passes that
/// deliver it (a detached panel's window is drawn in the main window's
/// pass), then one more for the commands asked.
fn key_in(m: &mut MultiWindow, v: ViewportId, key: Key) {
    m.focus(v);
    m.press(v, key, Modifiers::COMMAND);
    m.root_pass();
    m.root_pass();
}

/// The acceptance test: a chord begun in the main window is not finished
/// by a key in the detached window, and the other way round.
#[test]
fn a_chord_begun_in_one_window_is_not_finished_in_another() {
    let (mut m, panel, ran) = detached();

    key_in(&mut m, ViewportId::ROOT, Key::K);
    key_in(&mut m, panel, Key::G);
    assert!(ran.borrow().is_empty(), "ran across windows: {ran:?}");

    key_in(&mut m, panel, Key::K);
    key_in(&mut m, ViewportId::ROOT, Key::G);
    assert!(ran.borrow().is_empty(), "ran across windows: {ran:?}");
}

/// A shortcut in progress is dropped when another window becomes active:
/// back in the window it began in, its second key runs nothing.
#[test]
fn a_chord_in_progress_is_dropped_when_another_window_becomes_active() {
    let (mut m, panel, ran) = detached();

    key_in(&mut m, ViewportId::ROOT, Key::K);
    assert_eq!(m.app().pending_keys().as_deref(), Some("Ctrl+K"));
    m.focus(panel);
    m.click(panel, egui::pos2(320.0, 300.0));
    assert_eq!(m.app().active_window_id(), "panel:demo.log");
    key_in(&mut m, ViewportId::ROOT, Key::G);
    assert!(ran.borrow().is_empty(), "{ran:?}");
}

/// A chord pressed whole in the detached window runs there, in its
/// context; one pressed whole in the main window runs too.
#[test]
fn a_chord_pressed_whole_in_one_window_runs() {
    let (mut m, panel, ran) = detached();

    key_in(&mut m, panel, Key::K);
    key_in(&mut m, panel, Key::G);
    assert_eq!(*ran.borrow(), ["go"]);

    key_in(&mut m, panel, Key::K);
    key_in(&mut m, panel, Key::L);
    assert_eq!(*ran.borrow(), ["go", "in_log"]);

    key_in(&mut m, ViewportId::ROOT, Key::K);
    key_in(&mut m, ViewportId::ROOT, Key::G);
    assert_eq!(*ran.borrow(), ["go", "in_log", "go"]);

    // The log's command is not the main window's.
    key_in(&mut m, ViewportId::ROOT, Key::K);
    key_in(&mut m, ViewportId::ROOT, Key::L);
    assert_eq!(*ran.borrow(), ["go", "in_log", "go"]);
}
