//! The window's place, as a plugin ([`WindowPlugin`], in
//! `DefaultPlugins`): its size, position, maximized and full-screen
//! states are kept in the store (`settings/window.*`, saved with the
//! settings, no undo step) and given back at the next start by
//! `AppBuilder::run_native`, after a repair: a window whose title bar
//! would be on no screen comes back onto the first one, one larger than
//! its screen (or than what the GPU can draw) is shrunk.
//!
//! **Full screen** (`F11`, View > Full screen) keeps the way out
//! reachable: the application's title bar keeps a button leaving it
//! (`frame::FramePlugin`); without that bar (the system draws the frame,
//! or `window.bar_in_fullscreen` is off), moving the pointer to the top
//! edge shows a strip with the title and an "Exit full screen" button.
//!
//! Where the window manager does not tell the window's position
//! (Wayland), only the size and the states are kept.

use egui::{Align2, Context, Id, Order, Pos2, Rect, Ui, Vec2, ViewportCommand};
use egui_pop_core::kit::{Kit, State};
use egui_pop_core::lock;
use egui_pop_core::state::Slot;
use egui_pop_core::strings;

/// The way out of full screen was shown on the last pass.
const WAY_OUT: Slot<bool> = Slot::new("api.window.way_out");
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{BlockId, CommandId, OwnerId, PluginId};
use egui_pop_model::values::{Change, Descriptor, Set, SettingsDoc, Value};
use serde_json::json;

use crate::app::{AppHandle, MENU_VIEW};
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// Toggles full screen.
pub const CMD_FULLSCREEN: CommandId = CommandId::from_static("app.fullscreen");

/// Where the window's place is kept.
pub const RECT: &str = "settings/window.rect";
pub const MAXIMIZED: &str = "settings/window.maximized";
pub const FULLSCREEN: &str = "settings/window.fullscreen";
/// Whether the kept position is known (the window manager told it).
pub const PLACED: &str = "settings/window.placed";

/// The largest side kept (a window past the GPU's texture limit cannot
/// be drawn).
pub const MAX_SIDE: f32 = 8192.0;
/// The smallest side kept.
pub const MIN_SIDE: f32 = 200.0;
/// How much of a title bar must be on a screen to be reachable.
const REACHABLE: Vec2 = Vec2::new(64.0, 16.0);

/// The window's place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowState {
    /// The outer position, when the window manager tells it.
    pub position: Option<Pos2>,
    /// The inner size, in points.
    pub size: Vec2,
    pub maximized: bool,
    pub fullscreen: bool,
}

impl WindowState {
    /// As the store keeps it: `[x, y, width, height]`, whether x and y
    /// are known; a zero size when nothing was kept.
    pub fn from_store(
        rect: &[f64],
        placed: bool,
        maximized: bool,
        fullscreen: bool,
    ) -> Option<WindowState> {
        let [x, y, w, h] = rect else { return None };
        if *w <= 0.0 || *h <= 0.0 {
            return None;
        }
        let position = placed.then(|| Pos2::new(*x as f32, *y as f32));
        Some(WindowState {
            position,
            size: Vec2::new(*w as f32, *h as f32),
            maximized,
            fullscreen,
        })
    }

    fn to_store(self) -> Vec<f64> {
        let (x, y) = self
            .position
            .map_or((0.0, 0.0), |p| (f64::from(p.x), f64::from(p.y)));
        vec![x, y, f64::from(self.size.x), f64::from(self.size.y)]
    }

    /// Keeps the window reachable on these screens (none known: only the
    /// size is bounded, and a position far away is dropped).
    pub fn repair(mut self, screens: &[Rect]) -> WindowState {
        self.size = self
            .size
            .clamp(Vec2::splat(MIN_SIDE), Vec2::splat(MAX_SIDE));
        let Some(pos) = self.position else {
            return self;
        };
        if screens.is_empty() {
            if pos.x.abs() > 65_536.0 || pos.y.abs() > 65_536.0 {
                self.position = None;
            }
            return self;
        }
        let title = Rect::from_min_size(pos, Vec2::new(self.size.x, 32.0));
        let on = screens.iter().find(|s| {
            let i = s.intersect(title);
            i.is_positive() && i.width() >= REACHABLE.x && i.height() >= REACHABLE.y
        });
        let screen = match on {
            Some(s) => *s,
            None => {
                let s = screens[0];
                self.size = self.size.min(s.size());
                self.position = Some(s.center() - self.size / 2.0);
                s
            }
        };
        self.size = self.size.min(screen.size());
        self
    }
}

/// Reads the window's place from a saved settings document.
pub fn from_settings(doc: &SettingsDoc) -> Option<WindowState> {
    let rect: Vec<f64> = doc
        .values
        .get(RECT)?
        .as_array()?
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0))
        .collect();
    let flag = |p: &str| doc.values.get(p).and_then(|v| v.as_bool()).unwrap_or(false);
    WindowState::from_store(&rect, flag(PLACED), flag(MAXIMIZED), flag(FULLSCREEN))
}

/// Gives a native window the place kept in a settings directory (the
/// one given to `AppBuilder::persist`), repaired: the screens are not
/// known before the window exists, so only its size is bounded and a
/// position far away dropped. Returns the place given, if one was kept.
/// `AppBuilder::run_native` does it; an application running eframe
/// itself calls it.
#[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
pub fn restore(
    dir: impl Into<std::path::PathBuf>,
    options: &mut eframe::NativeOptions,
) -> Option<WindowState> {
    let p = egui_pop_core::persist::Persist::new(dir);
    let read = p
        .load::<SettingsDoc>("settings", &egui_pop_model::values::settings_schema())
        .ok()
        .flatten()?;
    let state = from_settings(&read.value)?.repair(&[]);
    apply(options, state);
    Some(state)
}

/// Puts a kept window place into a native window's options.
#[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
pub fn apply(options: &mut eframe::NativeOptions, state: WindowState) {
    let mut v = std::mem::take(&mut options.viewport).with_inner_size(state.size);
    if let Some(p) = state.position {
        v = v.with_position(p);
    }
    if state.maximized {
        v = v.with_maximized(true);
    }
    if state.fullscreen {
        v = v.with_fullscreen(true);
    }
    options.viewport = v;
}

/// The plugin.
#[derive(Debug, Default)]
pub struct WindowPlugin;

impl Plugin for WindowPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.window")
    }

    fn title(&self) -> String {
        "Window".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let owner = ctx.owner().clone();
        {
            let store = ctx.app().store();
            let mut s = lock(&store);
            let declare = |s: &mut egui_pop_model::values::Store, p: &str, d: Descriptor| {
                s.declare(&owner, p, d)
                    .map_err(|e| PluginError::Failed(e.to_string()))
            };
            declare(
                &mut s,
                RECT,
                Descriptor::floats("Window", 4, -1.0e6, 1.0e6, &[0.0; 4]),
            )?;
            declare(&mut s, PLACED, Descriptor::bool("Window placed", false))?;
            declare(&mut s, MAXIMIZED, Descriptor::bool("Maximized", false))?;
            declare(&mut s, FULLSCREEN, Descriptor::bool("Full screen", false))?;
        }
        ctx.command(
            CommandSpec::new(CMD_FULLSCREEN, "Full screen", "View")
                .description("Fills the screen; the top edge brings the way out back.")
                .key("F11")
                .checked("windowFullscreen")
                .menu(MENU_VIEW, "9_window", 0),
            |call| {
                let on = call.app.context().get("windowFullscreen") != "true";
                call.app
                    .egui()
                    .send_viewport_cmd(ViewportCommand::Fullscreen(on));
                Ok(json!({ "fullscreen": on }))
            },
        )?;
        ctx.block(
            zones::OVERLAYS,
            Block::add(BlockId::from_static("egui_pop.window"), 30, show),
        );
        Ok(())
    }
}

/// Follows the window's place into the store, and draws the way out of
/// full screen.
fn show(ui: &mut Ui, app: &AppHandle) {
    let ctx = ui.ctx().clone();
    let info = ctx.input(|i| i.viewport().clone());
    let fullscreen = info.fullscreen.unwrap_or(false);
    let maximized = info.maximized.unwrap_or(false);
    app.set_context(
        "windowFullscreen",
        if fullscreen { "true" } else { "false" },
    );
    track(app, &info, maximized, fullscreen);
    // The application's title bar, drawn this pass, holds the way out.
    if fullscreen && !crate::frame::bar_shown_in_full_screen(&ctx) {
        way_out(&ctx, app);
    }
}

fn track(app: &AppHandle, info: &egui::ViewportInfo, maximized: bool, fullscreen: bool) {
    let store = app.store();
    let mut s = lock(&store);
    let mut write = |path: &str, v: Value| {
        if s.get(path).is_some_and(|e| e.value != v) {
            let _ = s.set(
                Set::new(path, Change::To(v), Origin::User)
                    .by(&OwnerId::FRAMEWORK)
                    .untracked()
                    .force(),
            );
        }
    };
    write(MAXIMIZED, Value::Bool(maximized));
    write(FULLSCREEN, Value::Bool(fullscreen));
    // The normal place only: not while maximized, full screen or
    // minimized.
    if maximized || fullscreen || info.minimized == Some(true) {
        return;
    }
    let Some(inner) = info.inner_rect else { return };
    let state = WindowState {
        position: info.outer_rect.map(|r| r.min),
        size: inner.size(),
        maximized,
        fullscreen,
    };
    write(PLACED, Value::Bool(state.position.is_some()));
    write(RECT, Value::Floats(state.to_store()));
}

/// The strip at the top of a full screen window.
fn way_out(ctx: &Context, app: &AppHandle) {
    let screen = ctx.content_rect();
    let near_top = ctx
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| p.y <= screen.top() + 4.0);
    let id = Id::unique("egui_pop.window.way_out");
    let shown_before = WAY_OUT.get(ctx).unwrap_or(false);
    let mut keep = near_top;
    if near_top || shown_before {
        let r = egui::Area::new(id)
            .order(Order::Foreground)
            .anchor(Align2::CENTER_TOP, Vec2::ZERO)
            .show(ctx, |ui| {
                let shadow = ui.visuals().popup_shadow;
                Kit::of(ctx)
                    .fullscreen_exit
                    .show(ui, State::REST, shadow, |ui| {
                        ui.horizontal(|ui| {
                            let title = ctx
                                .input(|i| i.viewport().title.clone())
                                .unwrap_or_default();
                            if !title.is_empty() {
                                ui.strong(title);
                            }
                            let exit = strings::tr(ctx, &crate::strings::WINDOW_EXIT_FULLSCREEN);
                            if ui.button(exit).clicked() {
                                app.run(CMD_FULLSCREEN, json!(null));
                            }
                        });
                    })
            });
        keep |= r.response.contains_pointer();
    }
    WAY_OUT.set(ctx, keep);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(x: f32, y: f32, w: f32, h: f32) -> WindowState {
        WindowState {
            position: Some(Pos2::new(x, y)),
            size: Vec2::new(w, h),
            maximized: false,
            fullscreen: false,
        }
    }

    #[test]
    fn a_window_off_every_screen_comes_back_onto_the_first() {
        let left = Rect::from_min_size(Pos2::new(-1920.0, 0.0), Vec2::new(1920.0, 1080.0));
        let main = Rect::from_min_size(Pos2::ZERO, Vec2::new(2560.0, 1440.0));
        // On the left screen: kept as is.
        let s = state(-1500.0, 100.0, 800.0, 600.0).repair(&[main, left]);
        assert_eq!(s.position, Some(Pos2::new(-1500.0, 100.0)));
        // That screen is gone: centred on the first.
        let s = state(-1500.0, 100.0, 800.0, 600.0).repair(&[main]);
        assert_eq!(s.position, Some(Pos2::new(880.0, 420.0)));
        // Only a sliver of the title bar shows: moved too.
        let s = state(2540.0, 100.0, 800.0, 600.0).repair(&[main]);
        assert_eq!(s.position, Some(main.center() - Vec2::new(400.0, 300.0)));
        // Larger than its screen: shrunk to it.
        let s = state(0.0, 0.0, 4000.0, 3000.0).repair(&[main]);
        assert_eq!(s.size, Vec2::new(2560.0, 1440.0));
    }

    #[test]
    fn without_screens_only_the_size_is_bounded() {
        let s = state(10.0, 10.0, 20_000.0, 50.0).repair(&[]);
        assert_eq!(s.size, Vec2::new(MAX_SIDE, MIN_SIDE));
        assert_eq!(s.position, Some(Pos2::new(10.0, 10.0)));
        let s = state(1.0e7, 0.0, 800.0, 600.0).repair(&[]);
        assert_eq!(s.position, None);
    }

    #[test]
    fn the_store_keeps_whether_the_position_is_known() {
        let s = WindowState {
            position: None,
            size: Vec2::new(800.0, 600.0),
            maximized: true,
            fullscreen: false,
        };
        let back = WindowState::from_store(&s.to_store(), false, true, false).unwrap();
        assert_eq!(back, s);
        assert!(WindowState::from_store(&[0.0, 0.0, 0.0, 0.0], true, false, false).is_none());
    }
}
