//! The screens on the bus (ADR 0029).
//!
//! Whatever runs the event loop tells the screens
//! ([`crate::window::set_screens`]: egui-pop's native runner, or a host
//! running eframe on its own winit loop), at start and again when they
//! may have changed. Each application follows the change from its main
//! window's next pass: a screen come is said as `pop.screen.added`
//! ([`ScreenAdded`]), a screen gone as `pop.screen.removed`
//! ([`ScreenRemoved`]), each a [`ScreenChange`] naming the screen as
//! winit names it.
//!
//! egui-pop's listeners of `pop.screen.removed`, local (they touch the
//! interface's state), in this order:
//!
//! 1. [`WORKBENCH_SCREENS`]: a detached panel's window kept on the screen
//!    gone docks back (the layout's rule: "a window on a screen that is
//!    gone comes back docked"), by its window's `pop.window.reparent`;
//! 2. [`super::PLACE`]: every other window (the main window, the
//!    application windows, a detached panel's window on a screen it
//!    cannot name) that no longer meets a screen moves onto one that is
//!    left, and keeps that place.
//!
//! Where the system tells no window's position (Wayland), nothing moves:
//! the system places the windows. The screens first told to an
//! application are not said (nothing changed), but a window kept on a
//! screen gone since the last run is repaired the same way. A window
//! opening is repaired against the screens as it opens (its builder).
//! Invariant 6 ([`crate::window_invariants::ON_SCREEN`]) checks that every
//! positioned window meets a screen.

use bus_contract::{Bus, ListenOpts, Point, PointId, PointKind};
use egui::{Context, Pos2, Rect, ViewportCommand, ViewportId};
use serde::Serialize;

use super::{OWNER, PLACE, WindowKind, asking, emit_with};
use crate::app::AppHandle;
use crate::window::{Screen, WindowState};

/// A screen come or gone: its name as the system gives it (winit's
/// `MonitorHandle::name`; none when it gives none), its rectangle in
/// points (`[x, y, width, height]`) and its scale factor.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct ScreenChange {
    pub name: Option<String>,
    pub rect: [f32; 4],
    pub scale: f32,
}

impl From<&Screen> for ScreenChange {
    fn from(s: &Screen) -> Self {
        Self {
            name: s.name.clone(),
            rect: [s.rect.min.x, s.rect.min.y, s.rect.width(), s.rect.height()],
            scale: s.scale,
        }
    }
}

/// A screen was plugged in (`pop.screen.added`).
#[derive(Debug)]
pub struct ScreenAdded;
impl Point for ScreenAdded {
    const KIND: PointKind = PointKind::Event;
    const ID: PointId = PointId::new("pop.screen.added");
    type Input = ScreenChange;
    type Value = ();
}

/// A screen was unplugged (`pop.screen.removed`): the windows on it move onto
/// a screen left, or dock back.
#[derive(Debug)]
pub struct ScreenRemoved;
impl Point for ScreenRemoved {
    const KIND: PointKind = PointKind::Event;
    const ID: PointId = PointId::new("pop.screen.removed");
    type Input = ScreenChange;
    type Value = ();
}

/// The screen points, in the order [`super::order_dump`] prints them.
pub const SCREEN_POINTS: [&str; 2] = [ScreenAdded::ID.as_str(), ScreenRemoved::ID.as_str()];

/// The workbench's listener of `pop.screen.removed`: a detached panel's
/// window kept on the screen gone docks back.
pub const WORKBENCH_SCREENS: &str = "pop.workbench.screens";

/// What an application knows of the screens it follows.
#[derive(Debug, Default)]
pub(crate) struct Seen {
    /// [`crate::window::screens_told`] when last followed; none before
    /// the first pass.
    told: Option<u64>,
    screens: Vec<Screen>,
}

/// Whether two screens told are the same screen: by name, or by their
/// rectangle when the system names neither.
fn same(a: &Screen, b: &Screen) -> bool {
    match (&a.name, &b.name) {
        (Some(x), Some(y)) => x == y,
        (None, None) => a.rect == b.rect,
        _ => false,
    }
}

/// Registers egui-pop's listeners of `pop.screen.removed`.
pub(super) fn install(kernel: &dyn Bus) {
    let mine = || ListenOpts::new().owner(OWNER);
    let listened = [
        kernel.listen_local::<ScreenRemoved>(WORKBENCH_SCREENS, mine().before(PLACE), |_, _| {
            if let Some(app) = asking() {
                app.dock_windows_off_screens();
            }
            Ok(())
        }),
        kernel.listen_local::<ScreenRemoved>(PLACE, mine().after(WORKBENCH_SCREENS), |_, _| {
            if let Some(app) = asking() {
                app.move_windows_onto_screens(&app.egui());
            }
            Ok(())
        }),
    ];
    for l in listened {
        debug_assert!(l.is_ok(), "{l:?}");
    }
}

impl AppHandle {
    /// From the main window's pass: the screens told since the last pass
    /// followed. Each screen gone is said as `pop.screen.removed`, each come
    /// as `pop.screen.added`; the screens first told (to an application that
    /// knew none) are not said, the windows kept on a screen gone since
    /// are repaired as on `pop.screen.removed`.
    pub(crate) fn follow_screens(&self, ctx: &Context) {
        if ctx.viewport_id() != ViewportId::ROOT {
            return;
        }
        let told = crate::window::screens_told();
        let (before, now) = {
            let mut seen = self.0.bus.screens.borrow_mut();
            if seen.told == Some(told) {
                return;
            }
            seen.told = Some(told);
            let now = crate::window::screen_list();
            let before = std::mem::replace(&mut seen.screens, now.clone());
            (before, now)
        };
        if now.is_empty() {
            // None known any more: nothing to repair against.
            return;
        }
        if before.is_empty() {
            self.dock_windows_off_screens();
            self.move_windows_onto_screens(ctx);
            return;
        }
        for s in now.iter().filter(|s| !before.iter().any(|b| same(b, s))) {
            log::info!(target: "egui_pop", "screen added: {s:?}");
            emit_with::<ScreenAdded>(self, s.into(), ());
        }
        for s in before.iter().filter(|b| !now.iter().any(|s| same(b, s))) {
            log::info!(target: "egui_pop", "screen removed: {s:?}");
            emit_with::<ScreenRemoved>(self, s.into(), ());
        }
    }

    /// The names of the screens a layout's detached windows may be kept
    /// on: those told, and [`crate::window::UNNAMED_SCREEN`]. While no
    /// screen is told, or one is told without a name, every screen the
    /// layout names is kept too: nothing docks back for a screen that
    /// cannot be told gone.
    pub(crate) fn layout_screens(&self) -> Vec<String> {
        let told = crate::window::screen_list();
        let mut names = vec![crate::window::UNNAMED_SCREEN.to_string()];
        names.extend(told.iter().filter_map(|s| s.name.clone()));
        if told.is_empty() || told.iter().any(|s| s.name.is_none()) {
            let wb = self.workbench_mut();
            names.extend(wb.layout.auxiliary.iter().map(|a| a.screen.clone()));
            names.extend(wb.dormant.iter().filter_map(|d| match &d.place {
                egui_pop_model::layout::DormantPlace::Window { screen, .. } => Some(screen.clone()),
                _ => None,
            }));
        }
        names.sort();
        names.dedup();
        names
    }

    /// [`WORKBENCH_SCREENS`]: the detached panels' windows kept on a
    /// screen no longer told dock back, each by its window's
    /// `pop.window.reparent` into the main window. How many windows docked.
    pub(crate) fn dock_windows_off_screens(&self) -> usize {
        let names = self.layout_screens();
        let screens: Vec<&str> = names.iter().map(String::as_str).collect();
        let before = self.workbench_mut().layout.auxiliary.clone();
        let gone = before
            .iter()
            .filter(|a| !screens.contains(&a.screen.as_str()))
            .count();
        if gone == 0 {
            return 0;
        }
        let notes = {
            let mut wb = self.workbench_mut();
            let layout = wb.layout();
            wb.set_layout(layout, &screens)
        };
        for note in notes {
            log::info!(target: "egui_pop", "layout: {note}");
        }
        self.sync_panel_windows(&before);
        self.egui().request_repaint();
        gone
    }

    /// `pop.window.place` on `pop.screen.removed`: every window whose position the
    /// system tells and that meets no screen told (the main window, the
    /// application windows, the detached panels' windows) moves onto the
    /// nearest screen left, and keeps that place. Where positions are
    /// hidden (Wayland), nothing moves. The windows moved, by id.
    pub(crate) fn move_windows_onto_screens(&self, ctx: &Context) -> Vec<String> {
        let screens = crate::window::screens();
        if screens.is_empty() {
            return Vec::new();
        }
        let mut moved = Vec::new();
        for r in self.windows_list() {
            if !matches!(
                r.kind,
                WindowKind::Main | WindowKind::App | WindowKind::Panel
            ) {
                continue;
            }
            let Some(v) = crate::window_invariants::viewport_of(r.kind, &r.id) else {
                continue;
            };
            let Some(rect) = positioned(ctx, v) else {
                continue;
            };
            if screens.iter().any(|s| s.intersects(rect)) {
                continue;
            }
            let state = WindowState {
                position: Some(rect.min),
                size: rect.size(),
                maximized: false,
                fullscreen: false,
            }
            .repair(&screens);
            let Some(at) = state.position else { continue };
            log::info!(
                target: "egui_pop",
                "window `{}` was on no screen left ({rect:?}): moved to {at:?}",
                r.id
            );
            ctx.send_viewport_cmd_to(v, ViewportCommand::OuterPosition(at));
            match r.kind {
                WindowKind::App => self.app_window_moved(v, at),
                WindowKind::Panel => self.panel_window_moved(&r.id, at),
                _ => {}
            }
            ctx.request_repaint_of(v);
            moved.push(r.id);
        }
        moved
    }

    /// A detached panel's window moved to `at`: its place in the layout,
    /// on the screen it is now on.
    fn panel_window_moved(&self, id: &str, at: Pos2) {
        let panel = id
            .strip_prefix(crate::window_invariants::PANEL_PREFIX)
            .unwrap_or(id);
        let mut wb = self.workbench_mut();
        if let Some(a) = wb
            .layout
            .auxiliary
            .iter_mut()
            .find(|a| a.panel.as_str() == panel)
        {
            let [_, _, w, h] = a.rect;
            a.rect = [at.x, at.y, w, h];
            if let Some(name) =
                crate::window::screen_name_at(Rect::from_min_size(at, egui::vec2(w, h)))
            {
                a.screen = name;
            }
            // Asked by the command just sent: not asked again.
            let panel = a.panel.clone();
            wb.window_place_asked(&panel);
        }
    }
}

/// A window's rectangle on the screens, where the system tells it: none
/// where positions are hidden (Wayland), for a window not shown yet, or
/// minimized (Windows puts those far off every screen).
pub(crate) fn positioned(ctx: &Context, v: ViewportId) -> Option<Rect> {
    ctx.input(|i| {
        let info = i.raw.viewports.get(&v)?;
        if info.minimized == Some(true) {
            return None;
        }
        info.outer_rect.filter(|r| r.is_positive())
    })
}
