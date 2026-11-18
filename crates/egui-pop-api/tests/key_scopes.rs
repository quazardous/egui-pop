//! Each binding listens in its scope, and each window dispatches its own
//! keys bubbling through them: the focused surface, the window's focused
//! panel, the window, then the whole application. A panel's binding
//! never fires from another application window, though that panel was
//! the one focused last in the main window; a global one fires in every
//! window. Driven with real viewports (`egui_pop_test::windows::MultiWindow`).

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Key, Modifiers, ViewportId};
use egui_pop_api::windows::WindowSpec;
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
/// window's next (where the commands a window's keys asked run).
fn press(m: &mut MultiWindow, v: ViewportId, key: Key, mods: Modifiers) {
    m.focus(v);
    m.press(v, key, mods);
    if v == ViewportId::ROOT {
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
