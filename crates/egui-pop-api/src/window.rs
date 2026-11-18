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
//! (Wayland), only the size and the states are kept: the position an
//! earlier run kept (X11, Windows) stays for the next run where it is
//! told, never replaced by a made-up one, and the View menu says under
//! Full screen that the system places the windows ([`POSITIONS_HIDDEN`]).

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
/// The context key saying the system tells no window's position and
/// places windows itself (Wayland): `"true"` there.
pub const POSITIONS_HIDDEN: &str = "windowPositionsHidden";
/// The View menu's note, under Full screen, where the system places the
/// windows ([`POSITIONS_HIDDEN`]).
pub const LIST_PLACEMENT: &str = "app.window.placement";

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

    #[cfg(test)]
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

/// A screen (a monitor), as the system tells it: its name, its
/// rectangle in points and its scale factor. Given by whatever runs the
/// event loop ([`set_screens`]): egui-pop's native runner, or a host
/// application running eframe on its own loop.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    /// The monitor's name as the system gives it (winit's
    /// `MonitorHandle::name`), what recognises a screen plugged in
    /// again; none when the system gives none.
    pub name: Option<String>,
    /// Its rectangle, in points: the physical one over [`Self::scale`].
    pub rect: Rect,
    /// Physical pixels per point (1.0 when the system tells none).
    pub scale: f32,
}

impl Screen {
    /// A screen from what the system gives in physical pixels (winit's
    /// `MonitorHandle::{name, position, size, scale_factor}`): its
    /// rectangle in points is the physical one over its own scale factor
    /// (1.0 when that is not positive).
    pub fn from_physical(
        name: Option<String>,
        position: (i32, i32),
        size: (u32, u32),
        scale: f64,
    ) -> Screen {
        let s = if scale > 0.0 { scale as f32 } else { 1.0 };
        Screen {
            name,
            rect: Rect::from_min_size(
                egui::pos2(position.0 as f32 / s, position.1 as f32 / s),
                egui::vec2(size.0 as f32 / s, size.1 as f32 / s),
            ),
            scale: s,
        }
    }

    /// A screen known by its rectangle in points alone (no name, scale
    /// 1.0): what a test says.
    pub fn from_rect(rect: Rect) -> Screen {
        Screen {
            name: None,
            rect,
            scale: 1.0,
        }
    }
}

thread_local! {
    /// The screens told to egui-pop on this thread ([`set_screens`]).
    static SCREENS: std::cell::RefCell<Vec<Screen>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// How many times the screens were told on this thread: an
    /// application follows them from its main window's pass when it
    /// changed (`pop.screen.added`, `pop.screen.removed`).
    static TOLD: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The name a layout gives a screen it cannot name: a window detached
/// where no screen is told by name (Wayland tells no positions, a
/// system names no monitor). A window kept on it is never docked back
/// for its screen being gone; its position is still repaired.
pub const UNNAMED_SCREEN: &str = "primary";

/// The screens, in points, as far as egui-pop knows them: those told by
/// what runs the event loop ([`set_screens`]; egui-pop's native runner
/// tells them before the main window is created); none until told. A
/// window's kept place is repaired against them
/// ([`WindowState::repair`]) before it opens.
pub fn screens() -> Vec<Rect> {
    SCREENS.with_borrow(|s| s.iter().map(|s| s.rect).collect())
}

/// The screens as told ([`set_screens`]), with their names and scale
/// factors.
pub fn screen_list() -> Vec<Screen> {
    SCREENS.with_borrow(Clone::clone)
}

/// Tells egui-pop the screens, for the windows of this thread (the one
/// running the event loop and the application's frames). A host that
/// runs the event loop itself (eframe on its own winit loop, without the
/// `create-native` feature) reads the monitors and calls it: before the
/// main window is created (so its kept place is repaired against them,
/// [`repair_builder`]), and again when they change (a scale factor
/// changed, a window moved to another monitor). egui-pop's native runner
/// calls it the same way. Windows opened after the call (detached panels,
/// application windows) are repaired against the screens given; an empty
/// list says none are known (only sizes are bounded).
///
/// Each application of the thread follows the change from its main
/// window's next pass: a screen gone is said as `pop.screen.removed`, a new
/// one as `pop.screen.added` (by name; [`crate::bus::ScreenRemoved`]), and
/// every window on a screen gone moves onto one that is left, a detached
/// panel's window docking back (`pop.window.place`).
pub fn set_screens(screens: Vec<Screen>) {
    log::debug!(target: "egui_pop::window", "screens: {screens:?}");
    SCREENS.set(screens);
    TOLD.set(TOLD.get() + 1);
}

/// How many times the screens were told on this thread.
pub(crate) fn screens_told() -> u64 {
    TOLD.get()
}

/// The name of the screen `rect` is on (the one holding its centre, else
/// the one it overlaps most), as told; none when no screen is told by
/// name there.
pub fn screen_name_at(rect: Rect) -> Option<String> {
    SCREENS.with_borrow(|screens| {
        let centre = rect.center();
        screens
            .iter()
            .find(|s| s.rect.contains(centre))
            .or_else(|| {
                screens
                    .iter()
                    .filter(|s| s.rect.intersects(rect))
                    .max_by(|a, b| {
                        let area = |s: &Screen| s.rect.intersect(rect).area();
                        area(a).total_cmp(&area(b))
                    })
            })
            .and_then(|s| s.name.clone())
    })
}

/// Says the screens for this thread's windows by their rectangles, in
/// place of the system's (`None`: none known again). For headless tests
/// (`egui_pop_test::windows::MultiWindow::set_screens`).
#[doc(hidden)]
pub fn set_test_screens(screens: Option<Vec<Rect>>) {
    set_screens(
        screens
            .unwrap_or_default()
            .into_iter()
            .map(Screen::from_rect)
            .collect(),
    );
}

/// A native window's builder, its kept place repaired against the
/// screens told ([`set_screens`]): a position off every screen is pulled
/// back onto the first, a size larger than its screen shrunk; the
/// maximized and full-screen flags stay. Unchanged while no screen is
/// known. Called from eframe's `NativeOptions::window_builder` hook, which
/// runs as the main window is created (egui-pop's native runner does it).
pub fn repair_builder(vb: egui::ViewportBuilder) -> egui::ViewportBuilder {
    repair_place(vb, &screens())
}

pub(crate) fn repair_place(
    mut vb: egui::ViewportBuilder,
    screens: &[Rect],
) -> egui::ViewportBuilder {
    if screens.is_empty() {
        return vb;
    }
    let Some(size) = vb.inner_size else {
        return vb;
    };
    let state = WindowState {
        position: vb.position,
        size,
        maximized: false,
        fullscreen: false,
    }
    .repair(screens);
    vb.position = state.position;
    vb.inner_size = Some(state.size);
    vb
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
                .icon("fullscreen")
                .icon_on("exit-fullscreen")
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
        // Where the system places windows itself (Wayland), the View menu
        // says so under Full screen: their place is not lost by a bug.
        ctx.menu_item(egui_pop_model::menus::MenuItem::list(
            MENU_VIEW,
            "9_window",
            1,
            LIST_PLACEMENT,
        ))?;
        ctx.list_provider(LIST_PLACEMENT, |app, _| {
            if app.context().get(POSITIONS_HIDDEN) != "true" {
                return Vec::new();
            }
            let note = strings::tr(&app.egui(), &crate::strings::WINDOW_PLACED_BY_SYSTEM);
            vec![crate::menus::ListItem::new(&note, CMD_FULLSCREEN, json!(null)).disabled()]
        });
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
    // A window of its own (not embedded: kittest, the web) told no
    // rectangle while shown: the system hides positions (Wayland).
    let hidden =
        !ctx.embed_viewports() && info.inner_rect.is_none() && info.minimized != Some(true);
    app.set_context(POSITIONS_HIDDEN, if hidden { "true" } else { "false" });
    let size = ctx.input(|i| i.viewport_rect().size());
    track(app, &info, size, maximized, fullscreen);
    // The application's title bar, drawn this pass, holds the way out;
    // in zen mode, zen's own strip does (leaving it shows the bar).
    if fullscreen && !app.zen() && !crate::frame::bar_shown_in_full_screen(&ctx) {
        way_out(&ctx, app);
    }
}

fn track(
    app: &AppHandle,
    info: &egui::ViewportInfo,
    viewport_size: Vec2,
    maximized: bool,
    fullscreen: bool,
) {
    let store = app.store();
    let mut s = lock(&store);
    // The position an earlier run kept, where the system told it.
    let kept = match s.get(RECT).map(|e| &e.value) {
        Some(Value::Floats(r)) if r.len() == 4 => Some((r[0], r[1])),
        _ => None,
    };
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
    // Wayland tells no rectangle: the size is the window's own.
    let size = info.inner_rect.map_or(viewport_size, |r| r.size());
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let (x, y) = match info.outer_rect {
        Some(r) => {
            write(PLACED, Value::Bool(true));
            (f64::from(r.min.x), f64::from(r.min.y))
        }
        // Not told (Wayland): the position an earlier run kept stays as
        // it is, never replaced by a made-up one.
        None => kept.unwrap_or((0.0, 0.0)),
    };
    write(
        RECT,
        Value::Floats(vec![x, y, f64::from(size.x), f64::from(size.y)]),
    );
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

/// The windowing system the application's windows are on, as the event
/// loop's display says it (`runner::windowing`): egui-pop's runner reads
/// it when the loop resumes, a host running its own loop tells it
/// (`runner::host::resumed`).
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Windowing {
    /// Wayland: a window can have subsurfaces below it.
    Wayland,
    /// X11 (Xlib or XCB).
    X11,
    /// Windows.
    Windows,
    /// macOS.
    MacOs,
    /// Another system, or the web.
    Other,
    /// Not told: no event loop seen yet, or egui-pop does not run it and
    /// the host did not tell.
    Unknown,
}

/// How a window that may show a surface below egui (the image viewport's
/// image surface, a Wayland subsurface) is built on `windowing`:
/// transparent on Wayland, so that surface shows through; opaque
/// elsewhere, where there is no such surface to show and a transparent
/// window only costs the compositor, with the reason.
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn surface_below_transparency(
    windowing: Windowing,
) -> egui_pop_core::window_surface::Transparency {
    use egui_pop_core::window_surface::Transparency;
    let elsewhere = |name: &str| {
        Transparency::Opaque(format!(
            "{name}: the image surface is a Wayland subsurface, there is none here"
        ))
    };
    match windowing {
        Windowing::Wayland => Transparency::Transparent,
        Windowing::X11 => elsewhere("X11"),
        Windowing::Windows => elsewhere("Windows"),
        Windowing::MacOs => elsewhere("macOS"),
        Windowing::Other => elsewhere("this system"),
        Windowing::Unknown => Transparency::Opaque(
            "the windowing system is not known: egui-pop does not run the event loop and the \
             host did not tell it (runner::host::resumed)"
                .into(),
        ),
    }
}

/// Whether the windows the application decorates are built transparent
/// for rounded corners (`AppBuilder::rounded_windows`): `asked`, or by
/// default on Wayland only. X11 needs a compositing window manager (so
/// only when asked); Windows and macOS round their windows themselves;
/// the web, headless runs and an unknown system never.
pub(crate) fn rounded_windows(asked: Option<bool>, windowing: Windowing) -> bool {
    match (asked, windowing) {
        (Some(false), _) => false,
        (_, Windowing::Wayland) => true,
        (Some(true), Windowing::X11) => true,
        _ => false,
    }
}

/// Builds the main window transparent when its corners may be rounded
/// ([`rounded_windows`]), decided once the windowing system is known
/// (eframe's `window_builder` hook runs in `resumed`).
#[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
pub(crate) fn transparent_for_corners(options: &mut eframe::NativeOptions, asked: Option<bool>) {
    if asked == Some(false) {
        return;
    }
    let previous = options.window_builder.take();
    options.window_builder = Some(Box::new(move |vb| {
        let vb = match previous {
            Some(hook) => hook(vb),
            None => vb,
        };
        if rounded_windows(asked, windowing()) {
            vb.with_transparent(true)
        } else {
            vb
        }
    }));
}

/// The windowing system of this run ([`Windowing`]): the runner's, or
/// [`Windowing::Unknown`] without one (the web, headless, tests).
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn windowing() -> Windowing {
    #[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
    {
        crate::runner::windowing()
    }
    #[cfg(not(all(feature = "eframe", not(target_arch = "wasm32"))))]
    {
        Windowing::Unknown
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn rounded_windows_by_default_on_wayland_only() {
        use super::{Windowing, rounded_windows};
        assert!(rounded_windows(None, Windowing::Wayland));
        assert!(!rounded_windows(Some(false), Windowing::Wayland));
        assert!(!rounded_windows(None, Windowing::X11));
        assert!(rounded_windows(Some(true), Windowing::X11));
        for w in [
            Windowing::Windows,
            Windowing::MacOs,
            Windowing::Other,
            Windowing::Unknown,
        ] {
            assert!(!rounded_windows(Some(true), w), "{w:?}");
            assert!(!rounded_windows(None, w), "{w:?}");
        }
    }

    #[test]
    fn a_window_showing_a_surface_below_is_transparent_on_wayland_only() {
        use super::{Windowing, surface_below_transparency};
        assert!(surface_below_transparency(Windowing::Wayland).is_transparent());
        for (w, name) in [
            (Windowing::X11, "X11"),
            (Windowing::Windows, "Windows"),
            (Windowing::MacOs, "macOS"),
        ] {
            let t = surface_below_transparency(w);
            let why = t.why_opaque().expect("opaque");
            assert!(why.contains(name) && why.contains("Wayland"), "{why}");
        }
        let unknown = surface_below_transparency(Windowing::Unknown);
        assert!(unknown.why_opaque().unwrap().contains("runner::host"));
    }

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
