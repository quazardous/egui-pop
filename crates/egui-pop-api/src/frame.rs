//! The window's own frame, as a plugin ([`FramePlugin`], opt-in): the
//! application draws its title bar (in the [`zones::TITLE`] zone, above
//! the menu bar) and its resize edges, instead of the system, as VS Code
//! does. A setting (`window.frame`) switches between the application's
//! frame and the system's; the system's comes back at once.
//!
//! The bar holds, from the start edge, the blocks of the
//! [`zones::CAPTION_START`] zone (by default [`BLOCK_MENU`]: a button
//! opening the system menu, also `Alt+Space` and a right click on the
//! caption, and the application's name), the window's title, the blocks
//! of the [`zones::CAPTION`] zone (the application's buttons) and the
//! minimize, maximize and close buttons. Its hit regions are published
//! per window ([`egui_pop_core::frame::HitTable`]) and shown by the debug
//! bridge's `frame` collector.
//!
//! **In full screen** the bar stays, with its zones (the menus and the
//! application's items live there): close stays, minimize goes, and a
//! button leaving full screen (the `frame.exit_fullscreen` step) takes
//! maximize's place. The bar no longer moves the window and a double
//! click on it does nothing (the way out is that button, or `F11`); the
//! resize edges go. A setting (`window.bar_in_fullscreen`, on by default)
//! turns this off: then in full screen the bar is not drawn and publishes
//! no caption, and the window plugin's strip at the top edge is the way
//! out.
//!
//! An application puts its logo at the start of the bar by replacing the
//! menu block; the bar itself ([`BLOCK_TITLE`]) still follows the
//! setting:
//!
//! ```no_run
//! # use egui_pop_api::{Ctx, zones::{self, Block}, frame};
//! # use egui_pop_model::ids::BlockId;
//! # fn connect(ctx: &mut Ctx<'_>) {
//! ctx.block(
//!     zones::CAPTION_START,
//!     Block::replace(frame::BLOCK_MENU, BlockId::from_static("app.logo"), |ui, app| {
//!         frame::menu_button(ui, app);
//!         ui.strong("My app");
//!     }),
//! );
//! # }
//! ```
//!
//! Steps (ADR 0011), each with a default a plugin may veto or replace:
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `frame.minimize` | `{}` | minimizes the window |
//! | `frame.maximize` | `{ "maximized": bool }` (the state asked for) | maximizes or restores |
//! | `frame.close` | `{ "viewport": "root" }` | closes the window |
//! | `frame.system_menu` | `{ "x", "y" }` (points) | opens the system menu there |
//! | `frame.exit_fullscreen` | `{ "viewport": "root" }` | leaves full screen |
//! | `frame.hit_test` | `{ "viewport": "root", "install": bool }` | on Windows, takes over the window's hit testing (or gives it back) |
//!
//! ## Windows: the system's hit test
//!
//! winit cannot answer `WM_NCHITTEST` (see `docs/upstream.md`), so on
//! Windows, when the application draws the frame, the plugin subclasses
//! the main window through `egui-pop-win32` and answers it from the
//! published regions: the caption moves the window natively (Aero Snap),
//! the maximize button shows Snap Layouts, the edges resize. Clicks on
//! the buttons, a double click and a right click on the caption come back
//! as the usual requests, so the steps above still run. In full screen the
//! caption is answered as client area (the system must not move the
//! window), and the button leaving full screen is client area too: Snap
//! Layouts never show in full screen. The handle comes
//! from eframe when the application is started with
//! `AppBuilder::run_native` (or `run_native_with`); an application
//! starting eframe itself gives it with [`set_native_window`]. Only the
//! main window is subclassed for now.
//!
//! ## Corners
//!
//! A window without the system's decorations has square corners on
//! Windows 11; the plugin asks the system for the kit's `window.corners`
//! (`round`, `small` or `square`) when it draws the frame, again when
//! the theme changes it, and gives the system's default back with the
//! system's frame. Windows 10 refuses, harmlessly. The outcome is in the
//! `frame` collector (`corners`), never silent. Elsewhere nothing is
//! asked: no other system has a portable request for a window's
//! corners (Wayland and X11 leave them to the compositor or the window
//! manager, macOS rounds its own windows), and drawing them would need
//! a transparent window.

use egui::{Context, Id, Rect, Ui, Vec2, ViewportCommand};
use egui_pop_core::frame::{FrameRequest, HitTable, TitleBar};
use egui_pop_core::icon::{Icon, Size};
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Anchor, Kind, OverlaySpec};
use egui_pop_core::placement::Side;
use egui_pop_core::strings;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, MenuId, PluginId, SettingId, StepId};
use egui_pop_model::values::Descriptor;
use serde_json::{Value as Json, json};

use crate::app::{AppHandle, MENU_VIEW};
use crate::steps::StepSpec;
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// The setting: `own` (the application draws the frame) or `system`.
pub const SETTING_FRAME: SettingId = SettingId::from_static("window.frame");

/// The setting: whether the application's title bar stays in full screen
/// (on by default).
pub const SETTING_BAR_IN_FULLSCREEN: SettingId = SettingId::from_static("window.bar_in_fullscreen");

/// The system menu (restore, minimize, maximize, close); plugins may add
/// to it.
pub const MENU_SYSTEM: MenuId = MenuId::from_static("app.menu.system");

pub const CMD_MINIMIZE: CommandId = CommandId::from_static("app.window.minimize");
pub const CMD_MAXIMIZE: CommandId = CommandId::from_static("app.window.maximize");
pub const CMD_CLOSE: CommandId = CommandId::from_static("app.window.close");
pub const CMD_SYSTEM_MENU: CommandId = CommandId::from_static("app.window.menu");

pub const STEP_MINIMIZE: StepId = StepId::from_static("frame.minimize");
pub const STEP_MAXIMIZE: StepId = StepId::from_static("frame.maximize");
pub const STEP_CLOSE: StepId = StepId::from_static("frame.close");
pub const STEP_SYSTEM_MENU: StepId = StepId::from_static("frame.system_menu");
pub const STEP_HIT_TEST: StepId = StepId::from_static("frame.hit_test");
pub const STEP_EXIT_FULLSCREEN: StepId = StepId::from_static("frame.exit_fullscreen");

/// The title bar, in the [`zones::TITLE`] zone: it keeps the window's
/// decorations in line with the setting, so it is better wrapped or
/// extended than replaced.
pub const BLOCK_TITLE: BlockId = BlockId::from_static("egui_pop.frame");
/// The start of the title bar, in the [`zones::CAPTION_START`] zone: the
/// system menu button and the application's name.
pub const BLOCK_MENU: BlockId = BlockId::from_static("egui_pop.frame.menu");

/// The plugin: `FramePlugin::own()` draws the frame by default,
/// `FramePlugin::system()` leaves it to the system until the setting
/// says otherwise.
#[derive(Clone, Debug, Default)]
pub struct FramePlugin {
    own: bool,
    name: String,
}

impl FramePlugin {
    /// The application draws its frame by default.
    pub fn own() -> Self {
        Self {
            own: true,
            name: String::new(),
        }
    }

    /// The system draws the frame by default.
    pub fn system() -> Self {
        Self::default()
    }

    /// The name shown after the system menu button; without one, the
    /// application's (`AppBuilder::name`).
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.into();
        self
    }
}

/// Whether the application draws the window's frame now.
pub fn own_frame(app: &AppHandle) -> bool {
    lock(&app.store())
        .get(&format!("settings/{SETTING_FRAME}"))
        .and_then(|e| e.value.as_choice().map(|c| c == "own"))
        .unwrap_or(false)
}

/// Whether the application's title bar stays in full screen
/// (`window.bar_in_fullscreen`).
pub fn bar_in_fullscreen(app: &AppHandle) -> bool {
    lock(&app.store())
        .get(&format!("settings/{SETTING_BAR_IN_FULLSCREEN}"))
        .and_then(|e| e.value.as_bool())
        .unwrap_or(true)
}

fn viewport_name(ctx: &Context) -> String {
    if ctx.viewport_id() == egui::ViewportId::ROOT {
        "root".into()
    } else {
        format!("{:?}", ctx.viewport_id())
    }
}

/// Asks the frame's step for a request, with its default.
pub fn request(app: &AppHandle, ctx: &Context, r: FrameRequest) {
    let c = ctx.clone();
    match r {
        FrameRequest::Minimize => {
            app.step(&STEP_MINIMIZE, json!({}), move |_, _| {
                c.send_viewport_cmd(ViewportCommand::Minimized(true));
                Ok(json!({ "minimized": true }))
            });
        }
        FrameRequest::ToggleMaximize => {
            let to = !ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            app.step(
                &STEP_MAXIMIZE,
                json!({ "maximized": to }),
                move |_, args| {
                    let to = args["maximized"].as_bool().unwrap_or(true);
                    c.send_viewport_cmd(ViewportCommand::Maximized(to));
                    Ok(json!({ "maximized": to }))
                },
            );
        }
        FrameRequest::Close => {
            app.step(
                &STEP_CLOSE,
                json!({ "viewport": viewport_name(ctx) }),
                move |_, _| {
                    c.send_viewport_cmd(ViewportCommand::Close);
                    Ok(json!({ "closed": true }))
                },
            );
        }
        FrameRequest::ExitFullscreen => {
            app.step(
                &STEP_EXIT_FULLSCREEN,
                json!({ "viewport": viewport_name(ctx) }),
                move |_, _| {
                    c.send_viewport_cmd(ViewportCommand::Fullscreen(false));
                    Ok(json!({ "fullscreen": false }))
                },
            );
        }
        FrameRequest::SystemMenu(at) => {
            app.step(
                &STEP_SYSTEM_MENU,
                json!({ "x": at.x, "y": at.y }),
                move |_, args| {
                    let x = args["x"].as_f64().unwrap_or(0.0) as f32;
                    let y = args["y"].as_f64().unwrap_or(0.0) as f32;
                    let pass = c.cumulative_pass_nr();
                    c.data_mut(|d| d.insert_temp(menu_id(), Some((egui::pos2(x, y), pass))));
                    Ok(json!({ "open": true }))
                },
            );
        }
    }
}

fn menu_id() -> Id {
    Id::new("egui_pop.frame.system_menu")
}

impl Plugin for FramePlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.frame")
    }

    fn title(&self) -> String {
        "Window frame".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.setting(
            SETTING_FRAME,
            Descriptor::choice(
                "Window frame",
                &["own", "system"],
                if self.own { "own" } else { "system" },
            )
            .description(
                "Who draws the window's title bar and edges: the application (own), or the desktop (system).",
            ),
        )?;
        ctx.setting(
            SETTING_BAR_IN_FULLSCREEN,
            Descriptor::bool("Title bar in full screen", true).description(
                "Keeps the application's title bar (its buttons and items) in full screen; off, full screen hides it.",
            ),
        )?;
        for spec in [
            StepSpec::new(STEP_MINIMIZE, "The frame minimizes the window.").args(json!({})),
            StepSpec::new(
                STEP_MAXIMIZE,
                "The frame maximizes or restores the window (`maximized`: the state asked for).",
            )
            .args(json!({ "maximized": true })),
            StepSpec::new(
                STEP_CLOSE,
                "The frame closes a window (`viewport`: `root` for the main one).",
            )
            .args(json!({ "viewport": "root" })),
            StepSpec::new(
                STEP_SYSTEM_MENU,
                "The frame opens the system menu at a point (`x`, `y`).",
            )
            .args(json!({ "x": 8.0, "y": 32.0 })),
            StepSpec::new(
                STEP_EXIT_FULLSCREEN,
                "The title bar's button leaves full screen (`viewport`: `root` for the main window).",
            )
            .args(json!({ "viewport": "root" })),
            StepSpec::new(
                STEP_HIT_TEST,
                "On Windows, the frame takes over the window's hit testing (`install`: true) or gives it back (false); a veto keeps the system's.",
            )
            .args(json!({ "viewport": "root", "install": true })),
        ] {
            ctx.step(spec)?;
        }
        ctx.zone(zones::CAPTION, "Title bar items")?;
        ctx.zone(zones::CAPTION_START, "Title bar start")?;
        ctx.command(
            CommandSpec::new(CMD_MINIMIZE, "Minimize", "Window")
                .description("Minimizes the window.")
                .menu(MENU_SYSTEM, "1_size", 1)
                .no_agent(),
            |call| {
                request(call.app, &call.app.egui(), FrameRequest::Minimize);
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(CMD_MAXIMIZE, "Maximize", "Window")
                .description("Maximizes the window, or restores it.")
                .checked("windowMaximized")
                .menu(MENU_SYSTEM, "1_size", 2)
                .no_agent(),
            |call| {
                request(call.app, &call.app.egui(), FrameRequest::ToggleMaximize);
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(CMD_CLOSE, "Close window", "Window")
                .description("Closes the window, as its close button does.")
                .menu(MENU_SYSTEM, "9_close", 0)
                .no_agent(),
            |call| {
                request(call.app, &call.app.egui(), FrameRequest::Close);
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(CMD_SYSTEM_MENU, "Window menu", "Window")
                .description("Opens the window's menu (restore, minimize, maximize, close).")
                .key("Alt+Space")
                .when("windowOwnFrame")
                .menu(MENU_VIEW, "9_window", 1)
                .no_agent(),
            |call| {
                let ctx = call.app.egui();
                let at = HitTable::of(&ctx)
                    .get(egui::ViewportId::ROOT)
                    .and_then(|r| r.caption)
                    .map_or(egui::pos2(0.0, 0.0), |c| {
                        let ppp = ctx.pixels_per_point();
                        egui::pos2(c.x as f32 / ppp, (c.y + c.h) as f32 / ppp)
                    });
                request(call.app, &ctx, FrameRequest::SystemMenu(at));
                Ok(Json::Null)
            },
        )?;
        let name = self.name.clone();
        ctx.block(zones::TITLE, Block::add(BLOCK_TITLE, 0, title_block));
        ctx.block(
            zones::CAPTION_START,
            Block::add(BLOCK_MENU, 0, move |ui, app| {
                menu_button(ui, app);
                let name = if name.is_empty() {
                    app.name()
                } else {
                    name.clone()
                };
                if !name.is_empty() {
                    ui.add(
                        egui::Label::new(egui::RichText::new(&name).strong())
                            .selectable(false)
                            .sense(egui::Sense::hover()),
                    );
                }
            }),
        );
        ctx.block(
            zones::OVERLAYS,
            Block::add(
                BlockId::from_static("egui_pop.frame.edges"),
                40,
                |ui, app| {
                    edges(ui, app);
                },
            ),
        );
        let (w1, w2) = (ctx.app().downgrade(), ctx.app().downgrade());
        ctx.collector(
            egui_pop_core::debug::FnCollector::new(
                "frame",
                "The window frame: drawn by the application or the system; each window's hit regions in physical pixels (caption, buttons, edges).",
            )
            .snapshot(move |ctx| {
                let Ok(app) = w1.get() else { return Json::Null };
                let table = HitTable::of(ctx);
                json!({
                    "own": own_frame(&app),
                    "hit_test": native::describe(ctx),
                    "corners": native::describe_corners(ctx),
                    "windows": table.viewports().into_iter().map(|v| json!({
                        "viewport": if v == egui::ViewportId::ROOT { "root".to_string() } else { format!("{v:?}") },
                        "regions": table.get(v).map(|r| (*r).clone()),
                    })).collect::<Vec<_>>(),
                })
            })
            .action(
                "hit",
                "What a point of the main window is (`x`, `y` in physical pixels): caption, a button, an edge or client.",
                false,
                move |ctx, args| {
                    w2.get()?;
                    let x = args.get("x").and_then(Json::as_i64).ok_or("`x` is missing")? as i32;
                    let y = args.get("y").and_then(Json::as_i64).ok_or("`y` is missing")? as i32;
                    let r = HitTable::of(ctx)
                        .get(egui::ViewportId::ROOT)
                        .ok_or("the main window publishes no regions (the system draws its frame)")?;
                    Ok(json!(r.hit(x, y)))
                },
            )
            .args(json!({ "x": 10, "y": 10 })),
        )?;
        Ok(())
    }
}

/// Keeps the window's decorations in line with the setting; returns
/// whether the application draws the frame.
fn follow_setting(ui: &Ui, app: &AppHandle) -> bool {
    let ctx = ui.ctx();
    let own = own_frame(app);
    app.set_context("windowOwnFrame", if own { "true" } else { "false" });
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    app.set_context("windowMaximized", if maximized { "true" } else { "false" });
    native::follow(ctx, app, own);
    native::corners(ctx, own);
    let key = Id::new(("egui_pop.frame.decorated", ctx.viewport_id()));
    let applied: Option<bool> = ctx.data(|d| d.get_temp(key));
    if applied != Some(own) {
        ctx.send_viewport_cmd(ViewportCommand::Decorations(!own));
        ctx.data_mut(|d| d.insert_temp(key, own));
        if !own {
            HitTable::of(ctx).forget(ctx.viewport_id());
        }
    }
    own
}

fn bar_in_full_screen_id(ctx: &Context) -> Id {
    Id::new(("egui_pop.frame.bar_in_full_screen", ctx.viewport_id()))
}

/// Whether the application drew the current window's title bar in full
/// screen this pass or the one before (the window plugin's strip at the
/// top edge is not needed then). The previous pass counts because the
/// window plugin may draw before the title bar within a pass.
pub(crate) fn bar_shown_in_full_screen(ctx: &Context) -> bool {
    // The id reads the context, so it is built before its lock is taken.
    let id = bar_in_full_screen_id(ctx);
    ctx.data(|d| d.get_temp::<u64>(id))
        .is_some_and(|pass| pass + 1 >= ctx.cumulative_pass_nr())
}

/// The button opening the system menu (the [`BLOCK_MENU`] block draws
/// it); an application replacing that block calls it to keep the menu.
/// A click asks for the menu through the `frame.system_menu` step.
pub fn menu_button(ui: &mut Ui, app: &AppHandle) -> egui::Response {
    let r = ui.add(
        Icon::Menu
            .size(Size::Toolbar)
            .label(&strings::tr(ui.ctx(), &txt::FRAME_WINDOW_MENU))
            .sense_click(),
    );
    if r.clicked() {
        request(
            app,
            ui.ctx(),
            FrameRequest::SystemMenu(r.rect.left_bottom()),
        );
    }
    r
}

/// The title bar, in the title zone; in full screen, as the
/// `window.bar_in_fullscreen` setting says.
fn title_block(ui: &mut Ui, app: &AppHandle) {
    if !follow_setting(ui, app) {
        return;
    }
    let ctx = ui.ctx().clone();
    for r in native::requests(&ctx) {
        request(app, &ctx, r);
    }
    if egui_pop_core::frame::full_screen(&ctx) {
        if !bar_in_fullscreen(app) {
            egui_pop_core::frame::hide_title_bar(&ctx);
            return;
        }
        let pass = ctx.cumulative_pass_nr();
        let id = bar_in_full_screen_id(&ctx);
        ctx.data_mut(|d| d.insert_temp(id, pass));
    }
    let title = ctx
        .input(|i| i.viewport().title.clone())
        .unwrap_or_default();
    let out = TitleBar::new(&title).show(
        ui,
        |ui| {
            if app.zone_has_blocks(&zones::CAPTION_START) {
                app.show_zone(ui, &zones::CAPTION_START);
            }
        },
        |ui| {
            if app.zone_has_blocks(&zones::CAPTION) {
                app.show_zone(ui, &zones::CAPTION);
            }
        },
    );
    if let Some(r) = out.request {
        request(app, &ctx, r);
    }
}

/// The resize edges, and the system menu when open.
fn edges(ui: &mut Ui, app: &AppHandle) {
    let ctx = ui.ctx().clone();
    if !own_frame(app) {
        return;
    }
    egui_pop_core::frame::resize_edges(&ctx);
    egui_pop_core::frame::publish(&ctx);
    let at: Option<(egui::Pos2, u64)> = ctx.data(|d| d.get_temp(menu_id())).flatten();
    let Some((at, opened)) = at else { return };
    let spec = OverlaySpec::new("egui_pop.frame.system_menu", "Window menu", Kind::Menu);
    let mut open = true;
    let anchor = Anchor {
        rect: Rect::from_min_size(at, Vec2::ZERO),
        side: Side::Bottom,
    };
    let mut clicked = false;
    overlay::show(&ctx, &spec, &mut open, Some(anchor), |ui| {
        crate::menus::show(ui, app, &MENU_SYSTEM);
        // A click after the one that opened it: an item was chosen (a
        // click outside closes the overlay by itself).
        clicked =
            ui.ctx().cumulative_pass_nr() > opened && ui.ctx().input(|i| i.pointer.any_click());
    });
    if !open || clicked {
        overlay::close(&ctx, spec.id);
        ctx.data_mut(|d| d.insert_temp::<Option<(egui::Pos2, u64)>>(menu_id(), None));
    }
}

/// Records a window's native handle (an `HWND` on Windows), for the
/// frame's hit testing. `AppBuilder::run_native` does it for the main
/// window; an application starting eframe itself calls it from its
/// creation context.
pub fn set_native_window(ctx: &Context, viewport: egui::ViewportId, handle: isize) {
    ctx.data_mut(|d| d.insert_temp(native::handle_id(viewport), handle));
}

/// Records the main window's handle from eframe's creation context.
#[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
pub(crate) fn record_native_window(cc: &eframe::CreationContext<'_>) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(h) = cc.window_handle()
            && let RawWindowHandle::Win32(w) = h.as_raw()
        {
            set_native_window(&cc.egui_ctx, egui::ViewportId::ROOT, w.hwnd.get());
        }
    }
    #[cfg(not(windows))]
    let _ = cc;
}

/// The frame's Windows layer: the hit-test handler over the published
/// regions and the requests it hands back.
mod native {
    use std::sync::{Arc, Mutex};

    use egui::{Context, Id, ViewportId};
    use egui_pop_core::frame::{self as core_frame, Button, FrameRequest, Hit, HitTable};
    use egui_pop_core::kit::{Kit, WindowCorners};
    use egui_pop_core::lock;
    use egui_pop_win32::{Area, Corners, Handler, NcEvent, Subclassed};
    use serde_json::{Value as Json, json};

    use super::STEP_HIT_TEST;
    use crate::app::AppHandle;

    pub(super) fn handle_id(viewport: ViewportId) -> Id {
        Id::new(("egui_pop.frame.native_window", viewport))
    }

    /// What the layer holds, shared with the window procedure.
    #[derive(Clone, Debug, Default)]
    struct Native {
        installed: Arc<Mutex<Option<Subclassed>>>,
        events: Arc<Mutex<Vec<NcEvent>>>,
        error: Arc<Mutex<Option<String>>>,
        /// The corners last asked for, and what the system said.
        corners: Arc<Mutex<Option<Asked>>>,
    }

    /// Corners asked for, and the system's answer.
    type Asked = (Corners, Result<(), String>);

    fn native(ctx: &Context) -> Native {
        let id = Id::new("egui_pop.frame.native");
        if let Some(n) = ctx.data(|d| d.get_temp::<Native>(id)) {
            return n;
        }
        let n = Native::default();
        ctx.data_mut(|d| d.insert_temp(id, n.clone()));
        n
    }

    /// Maps the frame's regions to the system's areas.
    pub(super) fn area(hit: Hit) -> Area {
        match hit {
            Hit::Client => Area::Client,
            Hit::Caption => Area::Caption,
            Hit::Minimize => Area::Minimize,
            Hit::Maximize => Area::Maximize,
            Hit::Close => Area::Close,
            Hit::Left => Area::Left,
            Hit::Right => Area::Right,
            Hit::Top => Area::Top,
            Hit::Bottom => Area::Bottom,
            Hit::TopLeft => Area::TopLeft,
            Hit::TopRight => Area::TopRight,
            Hit::BottomLeft => Area::BottomLeft,
            Hit::BottomRight => Area::BottomRight,
        }
    }

    /// Answers the window's hit tests from the published regions; hands
    /// what the system saw back to the next frame.
    #[derive(Debug)]
    pub(super) struct Bridge {
        pub(super) table: HitTable,
        pub(super) viewport: ViewportId,
        pub(super) ctx: Context,
        pub(super) events: Arc<Mutex<Vec<NcEvent>>>,
    }

    impl Handler for Bridge {
        fn hit(&self, x: i32, y: i32) -> Area {
            self.table.get(self.viewport).map_or(Area::Client, |r| {
                match r.hit(x, y) {
                    // In full screen the bar never moves the window: its
                    // drag, double and right clicks are egui's.
                    Hit::Caption if r.fullscreen => Area::Client,
                    hit => area(hit),
                }
            })
        }

        fn event(&self, event: NcEvent) {
            lock(&self.events).push(event);
            self.ctx.request_repaint_of(self.viewport);
        }
    }

    /// Installs or removes the layer as the setting says, through the
    /// `frame.hit_test` step (main window only).
    pub(super) fn follow(ctx: &Context, app: &AppHandle, own: bool) {
        if ctx.viewport_id() != ViewportId::ROOT {
            return;
        }
        let Some(hwnd) = ctx.data(|d| d.get_temp::<isize>(handle_id(ViewportId::ROOT))) else {
            return;
        };
        let n = native(ctx);
        if lock(&n.installed).is_some() == own || (own && lock(&n.error).is_some()) {
            return;
        }
        let (c, n2) = (ctx.clone(), n.clone());
        app.step(
            &STEP_HIT_TEST,
            json!({ "viewport": "root", "install": own }),
            move |_, args| {
                let install = args["install"].as_bool().unwrap_or(own);
                let mut slot = lock(&n2.installed);
                if !install {
                    if let Some(s) = slot.take() {
                        s.remove();
                    }
                    core_frame::set_native_hover(&c, ViewportId::ROOT, None);
                    return Ok(json!({ "installed": false }));
                }
                let bridge = Bridge {
                    table: HitTable::of(&c),
                    viewport: ViewportId::ROOT,
                    ctx: c.clone(),
                    events: n2.events.clone(),
                };
                match egui_pop_win32::install(hwnd, Arc::new(bridge)) {
                    Ok(s) => {
                        *slot = Some(s);
                        Ok(json!({ "installed": true }))
                    }
                    Err(e) => {
                        *lock(&n2.error) = Some(e.to_string());
                        Err(e.to_string())
                    }
                }
            },
        );
        if !own {
            *lock(&n.error) = None;
        }
    }

    /// The system's corners for the kit's `window.corners`.
    pub(super) fn corners_of(c: WindowCorners) -> Corners {
        match c {
            WindowCorners::Round => Corners::Round,
            WindowCorners::Small => Corners::Small,
            WindowCorners::Square => Corners::Square,
        }
    }

    fn corners_name(c: Corners) -> &'static str {
        match c {
            Corners::System => "system",
            Corners::Square => "square",
            Corners::Round => "round",
            Corners::Small => "small",
        }
    }

    /// Asks the system for the kit's corners while the application draws
    /// the frame, and for its own default when it no longer does; again
    /// only when that changes (a theme naming other corners). Main window
    /// only. What is asked and answered is kept for the debug bridge and
    /// logged.
    pub(super) fn corners(ctx: &Context, own: bool) {
        if ctx.viewport_id() != ViewportId::ROOT {
            return;
        }
        let wanted = if own {
            corners_of(Kit::of(ctx).window.corners)
        } else {
            Corners::System
        };
        let n = native(ctx);
        let mut last = lock(&n.corners);
        match last.as_ref() {
            Some((asked, _)) if *asked == wanted => return,
            // Never asked, and the system's frame: nothing to give back.
            None if !own => return,
            _ => {}
        }
        let hwnd = ctx.data(|d| d.get_temp::<isize>(handle_id(ViewportId::ROOT)));
        let outcome = match hwnd {
            _ if !egui_pop_win32::SUPPORTED => Err(
                "not asked: only Windows rounds the corners of a window drawn by the application on request"
                    .to_string(),
            ),
            None => Err("not asked: the window's handle is unknown (see `set_native_window`)".into()),
            Some(h) => egui_pop_win32::set_corners(h, wanted).map_err(|e| e.to_string()),
        };
        match &outcome {
            Ok(()) => {
                log::info!(target: "egui_pop", "frame: corners `{}` applied", corners_name(wanted))
            }
            Err(e) => {
                log::info!(target: "egui_pop", "frame: corners `{}`: {e}", corners_name(wanted))
            }
        }
        *last = Some((wanted, outcome));
    }

    /// The corners asked for and the answer, for the debug bridge.
    pub(super) fn describe_corners(ctx: &Context) -> Json {
        match lock(&native(ctx).corners).as_ref() {
            None => json!({ "asked": Json::Null }),
            Some((asked, outcome)) => json!({
                "asked": corners_name(*asked),
                "applied": outcome.is_ok(),
                "error": outcome.as_ref().err(),
            }),
        }
    }

    /// The requests the system handed back since the last frame.
    pub(super) fn requests(ctx: &Context) -> Vec<FrameRequest> {
        let n = native(ctx);
        let events = std::mem::take(&mut *lock(&n.events));
        let ppp = ctx.pixels_per_point();
        let mut out = Vec::new();
        for e in events {
            match e {
                NcEvent::Hover(b) => {
                    let b = b.and_then(|a| match a {
                        Area::Minimize => Some(Button::Minimize),
                        Area::Maximize => Some(Button::Maximize),
                        Area::Close => Some(Button::Close),
                        _ => None,
                    });
                    core_frame::set_native_hover(ctx, ViewportId::ROOT, b);
                }
                NcEvent::Press(_) => {}
                NcEvent::Click(Area::Minimize) => out.push(FrameRequest::Minimize),
                NcEvent::Click(Area::Maximize) | NcEvent::DoubleClick(_) => {
                    out.push(FrameRequest::ToggleMaximize);
                }
                NcEvent::Click(Area::Close) => out.push(FrameRequest::Close),
                NcEvent::Click(_) => {}
                NcEvent::ContextMenu { x, y } => out.push(FrameRequest::SystemMenu(egui::pos2(
                    x as f32 / ppp,
                    y as f32 / ppp,
                ))),
            }
        }
        out
    }

    /// What the debug bridge shows.
    pub(super) fn describe(ctx: &Context) -> Json {
        let n = native(ctx);
        json!({
            "supported": egui_pop_win32::SUPPORTED,
            "window": ctx.data(|d| d.get_temp::<isize>(handle_id(ViewportId::ROOT))).is_some(),
            "installed": lock(&n.installed).is_some(),
            "error": lock(&n.error).clone(),
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_kit_corners_are_asked_once_and_reported() {
            let ctx = Context::default();
            assert_eq!(describe_corners(&ctx), json!({ "asked": null }));
            // The system's frame, never the application's: nothing asked.
            corners(&ctx, false);
            assert_eq!(describe_corners(&ctx)["asked"], Json::Null);
            corners(&ctx, true);
            let d = describe_corners(&ctx);
            assert_eq!(d["asked"], "round");
            if !egui_pop_win32::SUPPORTED {
                assert_eq!(d["applied"], false);
                assert!(d["error"].as_str().unwrap().starts_with("not asked"));
            }
            // A theme asking for square corners is asked again.
            let mut themes = egui_pop_core::theme::Themes::default();
            themes
                .add_text(
                    "square",
                    r#"
                    $extends: "pop-dark"
                    window:
                      corners: {$type: "string", $value: "square"}
                    "#,
                )
                .unwrap();
            egui_pop_core::theme::install_tokens(
                &ctx,
                egui_pop_core::theme::Tokens::new(
                    themes.resolve(&egui_pop_core::theme::ThemeSpec::new("square")),
                ),
            );
            corners(&ctx, true);
            assert_eq!(describe_corners(&ctx)["asked"], "square");
            // The system's frame again: its default is given back.
            corners(&ctx, false);
            assert_eq!(describe_corners(&ctx)["asked"], "system");
            assert_eq!(corners_of(WindowCorners::Small), Corners::Small);
        }

        #[test]
        fn the_bridge_answers_from_the_table_and_queues_requests() {
            let ctx = Context::default();
            let table = HitTable::of(&ctx);
            let layout = core_frame::FrameLayout {
                window: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
                caption: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 32.0),
                )),
                maximize: Some(egui::Rect::from_min_size(
                    egui::pos2(712.0, 0.0),
                    egui::vec2(44.0, 32.0),
                )),
                grip: 4.0,
                resizable: true,
                ..Default::default()
            };
            table.publish(
                ViewportId::ROOT,
                core_frame::HitRegions::measure(&layout, 1.5),
            );
            let events = native(&ctx).events;
            let b = Bridge {
                table,
                viewport: ViewportId::ROOT,
                ctx: ctx.clone(),
                events: events.clone(),
            };
            assert_eq!(b.hit(600, 24), Area::Caption);
            assert_eq!(b.hit((734.0 * 1.5) as i32, 24), Area::Maximize);
            assert_eq!(b.hit(2, 400), Area::Left);
            assert_eq!(b.hit(600, 400), Area::Client);
            for e in [
                NcEvent::Hover(Some(Area::Maximize)),
                NcEvent::Click(Area::Maximize),
                NcEvent::DoubleClick(Area::Caption),
                NcEvent::ContextMenu { x: 30, y: 15 },
                NcEvent::Click(Area::Close),
            ] {
                b.event(e);
            }
            let r = requests(&ctx);
            assert_eq!(
                r,
                vec![
                    FrameRequest::ToggleMaximize,
                    FrameRequest::ToggleMaximize,
                    FrameRequest::SystemMenu(egui::pos2(30.0, 15.0)),
                    FrameRequest::Close,
                ]
            );
            assert_eq!(
                core_frame::native_hover(&ctx),
                Some(Button::Maximize)
            );
            assert!(requests(&ctx).is_empty());
            // In full screen, the caption is client area: the system
            // must not move the window.
            b.table.publish(
                ViewportId::ROOT,
                core_frame::HitRegions::measure(
                    &core_frame::FrameLayout {
                        fullscreen: true,
                        resizable: false,
                        ..layout
                    },
                    1.5,
                ),
            );
            assert_eq!(b.hit(600, 24), Area::Client);
            assert_eq!(b.hit(2, 400), Area::Client);
        }
    }
}
