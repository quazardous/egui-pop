//! Detached panels' windows as windows of the application in full: keys
//! pressed there run in that window's context (its focus chain, `window`
//! naming its panel), a dialog opened from there is shown over it and
//! blocks the main window, and comes back to the main window when its
//! window closes. The command palette, popup menus and toasts open in the
//! window in use as dialogs do.
//!
//! kittest draws a detached panel inside the main window (one window
//! only), so these tests run an egui context with real viewports: an
//! immediate viewport renderer of their own runs each window's pass with
//! that window's input, as eframe does, without a GPU.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use egui::{Context, Event, Key, Modifiers, Pos2, RawInput, Rect, ViewportId, vec2};
use egui_pop_api::app::MENU_VIEW;
use egui_pop_api::dialogs::{self, Answer, Confirm, Dialogs};
use egui_pop_api::toasts::{TOASTS, Toast};
use egui_pop_api::workbench::{self, Change};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_api::{menus, palette};
use egui_pop_core::overlay;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
use serde_json::Value as Json;

const LOG: PanelId = PanelId::from_static("demo.log");
/// Bound in the log's window only.
const FIND: CommandId = CommandId::from_static("demo.find");
/// Bound while the log has the keyboard (its focus scope).
const CLEAR: CommandId = CommandId::from_static("demo.clear");
/// Bound everywhere.
const GO: CommandId = CommandId::from_static("demo.go");
/// Opens a dialog.
const ASK: CommandId = CommandId::from_static("demo.ask");

/// What ran, with the `window` context key it ran under.
type Runs = Rc<RefCell<Vec<(CommandId, String)>>>;

struct Demo(Runs);

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
            // Room to click into, filling the window.
            ui.allocate_space(ui.available_size());
        })?;
        for (id, keys, when) in [
            (FIND, "Ctrl+F", Some("window == 'demo.log'")),
            (CLEAR, "Ctrl+L", Some("demo.logFocus")),
            (GO, "Ctrl+G", None),
        ] {
            let runs = self.0.clone();
            let mut spec = CommandSpec::new(id.clone(), id.as_str(), "Demo").key(keys);
            if let Some(w) = when {
                spec = spec.when(w);
            }
            ctx.command(spec, move |call| {
                let window = call.app.when_context().get("window").to_string();
                runs.borrow_mut().push((id.clone(), window));
                Ok(Json::Null)
            })?;
        }
        ctx.command(CommandSpec::new(ASK, "Ask", "Demo").key("Ctrl+D"), |call| {
            let d = call.app.service(&dialogs::DIALOGS).ok_or("no dialogs")?;
            d.open(Confirm::new("Clear the log?", "Clear", GO));
            Ok(Json::Null)
        })?;
        Ok(())
    }
}

thread_local! {
    /// Each window's events for its next pass.
    static QUEUED: RefCell<HashMap<ViewportId, Vec<Event>>> = RefCell::default();
}

const WINDOW: Rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(480.0, 360.0));

/// One pass's input for a window.
fn input(viewport: ViewportId, rect: Rect) -> RawInput {
    let events = QUEUED.with(|q| q.borrow_mut().remove(&viewport).unwrap_or_default());
    RawInput {
        viewport_id: viewport,
        // egui wants the window it runs among the viewports it is told of.
        viewports: std::iter::once((viewport, Default::default())).collect(),
        screen_rect: Some(rect),
        events,
        ..Default::default()
    }
}

/// A context drawing detached panels in windows of their own, each
/// window's pass run with its own input.
fn context() -> Context {
    let ctx = Context::default();
    ctx.set_embed_viewports(false);
    Context::set_immediate_viewport_renderer(|ctx, viewport| {
        let mut draw = viewport.viewport_ui_cb;
        let mut out = ctx.run_ui(input(viewport.ids.this, WINDOW), |ui| draw(ui));
        // Nothing renders: the textures are dropped on purpose.
        out.textures_delta.clear();
    });
    ctx
}

struct Run {
    ctx: Context,
    app: App,
    runs: Runs,
    dialogs: Dialogs,
}

impl Run {
    fn new() -> Run {
        let ctx = context();
        let runs = Runs::default();
        let app = App::builder()
            .plugins(DefaultPlugins)
            .plugin(Demo(runs.clone()))
            .build(&ctx)
            .unwrap();
        let dialogs = (*app.handle().service(&dialogs::DIALOGS).unwrap()).clone();
        let mut run = Run {
            ctx,
            app,
            runs,
            dialogs,
        };
        run.frame();
        run
    }

    /// One frame: the main window's pass, the detached windows' inside it.
    fn frame(&mut self) {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0));
        let app = &mut self.app;
        let mut out = self
            .ctx
            .clone()
            .run_ui(input(ViewportId::ROOT, screen), |ui| app.show(ui));
        // Nothing renders: the textures are dropped on purpose.
        out.textures_delta.clear();
    }

    fn ran(&self) -> Vec<(CommandId, String)> {
        self.runs.borrow().clone()
    }
}

/// Queues a key press and its release (with Ctrl) for a window's next
/// pass: a key left down would make its next press a repeat.
fn press(viewport: ViewportId, key: Key) {
    press_with(viewport, key, Modifiers::COMMAND);
}

/// As [`press`], with given modifiers.
fn press_with(viewport: ViewportId, key: Key, modifiers: Modifiers) {
    QUEUED.with(|q| {
        let mut q = q.borrow_mut();
        let events = q.entry(viewport).or_default();
        for pressed in [true, false] {
            events.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
        }
    });
}

/// Queues a click for a window's next pass.
fn click(viewport: ViewportId, at: Pos2) {
    QUEUED.with(|q| {
        let mut q = q.borrow_mut();
        let events = q.entry(viewport).or_default();
        events.push(Event::PointerMoved(at));
        for pressed in [true, false] {
            events.push(Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    });
}

#[test]
fn keys_pressed_in_a_detached_window_run_in_its_context() {
    let mut r = Run::new();
    let h = r.app.handle().clone();
    assert!(h.detach_panel(&LOG).unwrap().ok());
    r.frame();
    let log = workbench::window_id(&LOG);
    assert_eq!(h.window_panel(log), Some(LOG));
    // A binding of the log's window fires there, under its context.
    press(log, Key::F);
    r.frame();
    assert_eq!(r.ran(), [(FIND, "demo.log".to_string())]);
    // Not in the main window, whose `window` key is empty.
    press(ViewportId::ROOT, Key::F);
    r.frame();
    assert_eq!(r.ran().len(), 1);
    // A global binding fires in either window.
    press(log, Key::G);
    r.frame();
    press(ViewportId::ROOT, Key::G);
    r.frame();
    assert_eq!(
        r.ran()[1..],
        [(GO, "demo.log".to_string()), (GO, String::new())]
    );
    // Clicked into, the log's focus scope heads that window's chain.
    click(log, WINDOW.center());
    r.frame();
    r.frame();
    press(log, Key::L);
    r.frame();
    assert_eq!(r.ran().last(), Some(&(CLEAR, "demo.log".to_string())));
}

#[test]
fn a_dialog_opened_from_a_detached_window_shows_there_and_blocks_the_others() {
    let mut r = Run::new();
    let h = r.app.handle().clone();
    assert!(h.detach_panel(&LOG).unwrap().ok());
    r.frame();
    let log = workbench::window_id(&LOG);
    // Opened by a key in the log's window: shown over it.
    press(log, Key::D);
    r.frame();
    assert_eq!(h.active_window(), log);
    let (id, _) = r.dialogs.current().expect("a dialog is open");
    let overlay_id = egui::Id::new(("egui_pop.dialog", id));
    r.frame();
    assert_eq!(overlay::window_of(&r.ctx, overlay_id), Some(log));
    assert_eq!(overlay::modal_windows(&r.ctx), [log]);
    // The main window is blocked: its keys reach nothing.
    press(ViewportId::ROOT, Key::G);
    r.frame();
    assert!(r.ran().is_empty(), "{:?}", r.ran());
    // Answered, every window is free again.
    r.dialogs.answer(Answer::Cancel, Origin::User).unwrap();
    for _ in 0..3 {
        r.frame();
    }
    assert!(r.dialogs.is_empty());
    assert!(!overlay::modal_open(&r.ctx));
    press(ViewportId::ROOT, Key::G);
    r.frame();
    assert_eq!(r.ran(), [(GO, String::new())]);
}

#[test]
fn a_dialog_whose_window_closes_comes_to_the_main_window() {
    let mut r = Run::new();
    let h = r.app.handle().clone();
    assert!(h.detach_panel(&LOG).unwrap().ok());
    r.frame();
    let log = workbench::window_id(&LOG);
    press(log, Key::D);
    r.frame();
    let (id, _) = r.dialogs.current().expect("a dialog is open");
    let overlay_id = egui::Id::new(("egui_pop.dialog", id));
    assert_eq!(overlay::window_of(&r.ctx, overlay_id), Some(log));
    // The window closes (docked back): the dialog is still to answer.
    assert!(h.change_workbench(Change::CloseWindow(LOG)).unwrap().ok());
    r.frame();
    r.frame();
    assert_eq!(h.active_window(), ViewportId::ROOT);
    assert_eq!(
        overlay::window_of(&r.ctx, overlay_id),
        Some(ViewportId::ROOT)
    );
    assert_eq!(overlay::modal_windows(&r.ctx), [ViewportId::ROOT]);
}

#[test]
fn a_dialog_opened_from_the_main_window_blocks_a_detached_one() {
    let mut r = Run::new();
    let h = r.app.handle().clone();
    assert!(h.detach_panel(&LOG).unwrap().ok());
    r.frame();
    let log = workbench::window_id(&LOG);
    press(ViewportId::ROOT, Key::D);
    r.frame();
    r.frame();
    let (id, _) = r.dialogs.current().expect("a dialog is open");
    let overlay_id = egui::Id::new(("egui_pop.dialog", id));
    assert_eq!(
        overlay::window_of(&r.ctx, overlay_id),
        Some(ViewportId::ROOT)
    );
    // The log's window waits: its bindings do not fire.
    press(log, Key::F);
    r.frame();
    assert!(r.ran().is_empty(), "{:?}", r.ran());
}

/// The palette's overlay id.
fn palette_overlay() -> egui::Id {
    egui::Id::new("egui_pop.palette")
}

/// The toasts' area id.
fn toasts_area() -> egui::Id {
    egui::Id::new("egui_pop.toasts")
}

#[test]
fn the_palette_menus_and_toasts_open_in_the_detached_window_in_use() {
    let mut r = Run::new();
    let h = r.app.handle().clone();
    assert!(h.detach_panel(&LOG).unwrap().ok());
    r.frame();
    let log = workbench::window_id(&LOG);
    // The palette, opened by its key in the log's window, shows there and
    // blocks the main window (a modal overlay).
    press_with(log, Key::F1, Modifiers::NONE);
    r.frame();
    r.frame();
    assert_eq!(h.active_window(), log);
    assert!(palette::is_open(&h));
    assert_eq!(h.service(&palette::PALETTE).unwrap().window(), Some(log));
    assert_eq!(overlay::window_of(&r.ctx, palette_overlay()), Some(log));
    assert_eq!(overlay::modal_windows(&r.ctx), [log]);
    // Agents see it in the `windows` collector.
    let seen = h.describe_windows();
    assert_eq!(seen["active"], "demo.log");
    assert_eq!(seen["palette"], "demo.log");
    assert_eq!(seen["windows"][1]["window"], "demo.log");
    assert_eq!(seen["windows"][1]["overlays"][0], "Command palette");
    assert_eq!(seen["windows"][1]["modal"], true);
    press(ViewportId::ROOT, Key::G);
    r.frame();
    assert!(r.ran().is_empty(), "{:?}", r.ran());
    // Its key closes it, in its window.
    press_with(log, Key::F1, Modifiers::NONE);
    r.frame();
    r.frame();
    assert!(!palette::is_open(&h));
    assert!(!overlay::modal_open(&r.ctx));
    // A popup menu opened while the log's window is in use shows there.
    assert!(menus::popup_at(&h, &MENU_VIEW, egui::pos2(40.0, 40.0)));
    r.frame();
    r.frame();
    assert_eq!(menus::popup_window(&h), Some(log));
    assert_eq!(menus::popup_open(&h), Some(MENU_VIEW));
    menus::close_popup(&h);
    // A toast shows in the window in use: drawn (its clock started), not
    // in the main window.
    h.service(&TOASTS).unwrap().notify(Toast::info("Saved"));
    r.frame();
    r.frame();
    let shown = h.service(&TOASTS).unwrap().shown();
    assert!(shown[0].until.is_some(), "{shown:?}");
    assert!(r.ctx.memory(|m| m.area_rect(toasts_area())).is_none());
    // Back in the main window, the toast follows.
    click(ViewportId::ROOT, egui::pos2(450.0, 300.0));
    r.frame();
    r.frame();
    assert_eq!(h.active_window(), ViewportId::ROOT);
    assert!(r.ctx.memory(|m| m.area_rect(toasts_area())).is_some());
}

#[test]
fn the_palette_and_a_menu_whose_window_closes_come_to_the_main_window() {
    let mut r = Run::new();
    let h = r.app.handle().clone();
    let log = workbench::window_id(&LOG);
    // A popup menu opened in the log's window.
    assert!(h.detach_panel(&LOG).unwrap().ok());
    r.frame();
    click(log, WINDOW.center());
    r.frame();
    assert_eq!(h.active_window(), log);
    assert!(menus::popup_at(&h, &MENU_VIEW, egui::pos2(40.0, 40.0)));
    r.frame();
    assert_eq!(menus::popup_window(&h), Some(log));
    // The window closes (docked back): the menu goes to the main window.
    assert!(h.change_workbench(Change::CloseWindow(LOG)).unwrap().ok());
    r.frame();
    r.frame();
    assert_eq!(menus::popup_window(&h), Some(ViewportId::ROOT));
    menus::close_popup(&h);
    // The palette, opened in the log's window detached again, too.
    assert!(h.detach_panel(&LOG).unwrap().ok());
    r.frame();
    press_with(log, Key::F1, Modifiers::NONE);
    r.frame();
    r.frame();
    assert_eq!(overlay::window_of(&r.ctx, palette_overlay()), Some(log));
    assert!(h.change_workbench(Change::CloseWindow(LOG)).unwrap().ok());
    r.frame();
    r.frame();
    assert!(palette::is_open(&h));
    assert_eq!(
        overlay::window_of(&r.ctx, palette_overlay()),
        Some(ViewportId::ROOT)
    );
}
