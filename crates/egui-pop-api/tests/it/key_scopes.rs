//! Each binding listens in its scope, and each window dispatches its own
//! keys bubbling through them: the focused surface, the window's focused
//! panel, the window, then the whole application. A panel's binding
//! never fires from another application window, though that panel was
//! the one focused last in the main window; a global one fires in every
//! window. Driven with real viewports (`egui_pop_test::windows::MultiWindow`).
//!
//! Every kind of window keeps its arrows: the main window, a detached
//! panel's window (which belongs to the main window), and each
//! application window, with the same key bound in each.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Key, Modifiers, ViewportId};
use egui_pop_api::windows::WindowSpec;
use egui_pop_api::workbench;
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PanelId, PluginId};
use egui_pop_model::keymap::{BindOptions, KeyScope};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_test::windows::MultiWindow;
use serde_json::Value as Json;

const GRID: PanelId = PanelId::from_static("demo.grid");

/// The commands that ran, with the window each ran for.
type Ran = Rc<RefCell<Vec<(String, String)>>>;

struct Demo(Ran);

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(GRID, "Grid", Area::Bottom), |ui, _| {
            ui.label("grid body");
        })?;
        let cmds: [(&'static str, &str, BindOptions, &str); 4] = [
            // A clause requiring the panel: its scope is inferred.
            (
                "demo.next",
                "Right",
                BindOptions::global(),
                "focusedPanel == 'demo.grid'",
            ),
            ("demo.remove", "Delete", BindOptions::panel("demo.grid"), ""),
            ("demo.main_only", "F2", BindOptions::window("main"), ""),
            ("demo.anywhere", "Ctrl+G", BindOptions::global(), ""),
        ];
        for (id, keys, options, when) in cmds {
            let ran = self.0.clone();
            let name = id.to_string();
            let mut spec =
                CommandSpec::new(CommandId::from_static(id), id, "Demo").key_with(keys, options);
            if !when.is_empty() {
                spec = spec.when(when);
            }
            ctx.command(spec, move |c| {
                ran.borrow_mut()
                    .push((name.clone(), c.app.active_app_window().to_string()));
                Ok(Json::Null)
            })?;
        }
        Ok(())
    }
}

/// The main window with the grid focused, and an application window
/// `import` open beside it.
fn setup() -> (MultiWindow, ViewportId, Ran) {
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
    m.app().focus_panel(&GRID);
    m.root_pass();
    m.root_pass();
    assert_eq!(m.app().focused_panel(), Some(GRID));
    m.app()
        .open_window(WindowSpec::new("import", "Import"), |ui, _| {
            ui.label("import body");
        });
    m.root_pass();
    let import = m.app().window_viewport(&"import".into()).unwrap();
    m.window_pass(import);
    (m, import, ran)
}

/// Presses a key in a window with the focus, runs its pass and the main
/// window's next (where the commands a window's keys asked run). A
/// detached panel's window is drawn in the main window's pass.
fn press(m: &mut MultiWindow, v: ViewportId, key: Key, mods: Modifiers) {
    m.focus(v);
    m.press(v, key, mods);
    if v == ViewportId::ROOT || m.shown_immediate().contains(&v) {
        m.root_pass();
    } else {
        m.window_pass(v);
    }
    m.root_pass();
    m.root_pass();
}

fn take(ran: &Ran) -> Vec<String> {
    ran.borrow_mut().drain(..).map(|(c, _)| c).collect()
}

#[test]
fn a_panel_s_keys_do_not_fire_from_another_application_window() {
    let (mut m, import, ran) = setup();
    for key in [Key::ArrowRight, Key::Delete] {
        press(&mut m, import, key, Modifiers::NONE);
        assert!(
            take(&ran).is_empty(),
            "{key:?} in the import window ran a grid command"
        );
    }
    // The control: in the main window, the grid focused, they fire.
    press(&mut m, ViewportId::ROOT, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(take(&ran), ["demo.next"]);
    press(&mut m, ViewportId::ROOT, Key::Delete, Modifiers::NONE);
    assert_eq!(take(&ran), ["demo.remove"]);
}

#[test]
fn a_window_s_keys_fire_in_it_alone_and_global_keys_everywhere() {
    let (mut m, import, ran) = setup();
    press(&mut m, import, Key::F2, Modifiers::NONE);
    assert!(take(&ran).is_empty());
    press(&mut m, ViewportId::ROOT, Key::F2, Modifiers::NONE);
    assert_eq!(take(&ran), ["demo.main_only"]);
    press(&mut m, import, Key::G, Modifiers::COMMAND);
    assert_eq!(
        *ran.borrow(),
        [("demo.anywhere".to_string(), "import".to_string())]
    );
    take(&ran);
    press(&mut m, ViewportId::ROOT, Key::G, Modifiers::COMMAND);
    assert_eq!(take(&ran), ["demo.anywhere"]);
}

#[test]
fn bindings_declared_without_a_scope_take_it_from_their_clause() {
    let (m, _, _) = setup();
    let scope = |cmd: &'static str| {
        m.app()
            .key_sources(&CommandId::from_static(cmd))
            .first()
            .map(|b| b.options.scope.clone())
    };
    assert_eq!(scope("demo.next"), Some(KeyScope::panel("demo.grid")));
    assert_eq!(scope("demo.anywhere"), Some(KeyScope::Global));
}

/// The same arrows and digit bound once per window (`window:<id>`), and
/// Delete bound to the grid's panel.
struct Kinds(Ran);

impl Plugin for Kinds {
    fn id(&self) -> PluginId {
        PluginId::from_static("kinds")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(GRID, "Grid", Area::Bottom), |ui, _| {
            ui.label("grid body");
        })?;
        let mut cmds: Vec<(&'static str, &'static str, BindOptions)> = Vec::new();
        for (window, ids) in [
            ("main", ["main.right", "main.left", "main.rate"]),
            ("a", ["a.right", "a.left", "a.rate"]),
            ("b", ["b.right", "b.left", "b.rate"]),
        ] {
            for (id, keys) in ids.into_iter().zip(["Right", "Left", "3"]) {
                cmds.push((id, keys, BindOptions::window(window)));
            }
        }
        cmds.push(("grid.remove", "Delete", BindOptions::panel("demo.grid")));
        for (id, keys, options) in cmds {
            let ran = self.0.clone();
            ctx.command(
                CommandSpec::new(CommandId::from_static(id), id, "Kinds").key_with(keys, options),
                move |c| {
                    ran.borrow_mut()
                        .push((id.to_string(), c.app.active_app_window().to_string()));
                    Ok(Json::Null)
                },
            )?;
        }
        Ok(())
    }
}

/// The main window with the grid focused then detached into its own
/// window, and two application windows `a` and `b`: their viewports.
fn kinds() -> (MultiWindow, [ViewportId; 3], Ran) {
    let ran = Ran::default();
    let r = ran.clone();
    let mut m = MultiWindow::new(move |ctx| {
        App::builder()
            .name("Kinds")
            .plugins(DefaultPlugins)
            .plugin(Kinds(r))
            .build(ctx)
            .unwrap()
    });
    m.root_pass();
    m.app().focus_panel(&GRID);
    m.root_pass();
    m.root_pass();
    assert!(m.app().detach_panel(&GRID).unwrap().ok());
    m.root_pass();
    m.root_pass();
    let grid = workbench::window_id(&GRID);
    assert!(m.shown_immediate().contains(&grid));
    for id in ["a", "b"] {
        m.app().open_window(WindowSpec::new(id, id), |ui, _| {
            ui.label("body");
        });
    }
    m.root_pass();
    let a = m.app().window_viewport(&"a".into()).unwrap();
    let b = m.app().window_viewport(&"b".into()).unwrap();
    m.window_pass(a);
    m.window_pass(b);
    (m, [grid, a, b], ran)
}

/// Presses a key in a window with the focus and returns what ran, with
/// the window each ran for.
fn ran_by(m: &mut MultiWindow, ran: &Ran, v: ViewportId, key: Key) -> Vec<(String, String)> {
    press(m, v, key, Modifiers::NONE);
    ran.borrow_mut().drain(..).collect()
}

#[test]
fn each_kind_of_window_keeps_its_arrows_and_digits() {
    let (mut m, [grid, a, b], ran) = kinds();
    let pair = |c: &str, w: &str| vec![(c.to_string(), w.to_string())];
    for (key, name) in [
        (Key::ArrowRight, "right"),
        (Key::ArrowLeft, "left"),
        (Key::Num3, "rate"),
    ] {
        for (v, window) in [
            (ViewportId::ROOT, "main"),
            // A detached panel's window belongs to the main window.
            (grid, "main"),
            (a, "a"),
            (b, "b"),
            (a, "a"),
            (ViewportId::ROOT, "main"),
        ] {
            assert_eq!(
                ran_by(&mut m, &ran, v, key),
                pair(&format!("{window}.{name}"), window),
                "{key:?} pressed in the window of {window} ({v:?})"
            );
        }
    }
}

#[test]
fn a_detached_panel_s_keys_fire_in_its_window_and_not_in_application_windows() {
    let (mut m, [grid, a, b], ran) = kinds();
    for v in [a, b] {
        assert!(
            ran_by(&mut m, &ran, v, Key::Delete).is_empty(),
            "Delete in an application window ran the grid's command"
        );
    }
    assert_eq!(
        ran_by(&mut m, &ran, grid, Key::Delete),
        [("grid.remove".to_string(), "main".to_string())]
    );
}
