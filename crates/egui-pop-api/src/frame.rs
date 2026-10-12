//! The window's own frame, as a plugin ([`FramePlugin`], opt-in): the
//! application draws its title bar (in the [`zones::TITLE`] zone, above
//! the menu bar) and its resize edges, instead of the system, as VS Code
//! does. A setting (`window.frame`) switches between the application's
//! frame and the system's; the system's comes back at once.
//!
//! The bar holds a button opening the system menu (also `Alt+Space` and
//! a right click on the caption), the application's name, the window's
//! title, the blocks of the [`zones::CAPTION`] zone (the application's
//! buttons) and the minimize, maximize and close buttons. Its hit
//! regions are published per window ([`egui_pop_core::frame::HitTable`])
//! and shown by the debug bridge's `frame` collector.
//!
//! Steps (ADR 0011), each with a default a plugin may veto or replace:
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `frame.minimize` | `{}` | minimizes the window |
//! | `frame.maximize` | `{ "maximized": bool }` (the state asked for) | maximizes or restores |
//! | `frame.close` | `{ "viewport": "root" }` | closes the window |
//! | `frame.system_menu` | `{ "x", "y" }` (points) | opens the system menu there |

use egui::{Context, Id, Rect, Ui, Vec2, ViewportCommand};
use egui_pop_core::frame::{FrameRequest, HitTable, TitleBar};
use egui_pop_core::icon::{Icon, Size};
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Anchor, Kind, OverlaySpec};
use egui_pop_core::placement::Side;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, MenuId, PluginId, SettingId, StepId};
use egui_pop_model::values::Descriptor;
use serde_json::{Value as Json, json};

use crate::app::{AppHandle, MENU_VIEW};
use crate::steps::StepSpec;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// The setting: `own` (the application draws the frame) or `system`.
pub const SETTING_FRAME: SettingId = SettingId::from_static("window.frame");

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

    /// The application's name, shown after the system menu button.
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
        ] {
            ctx.step(spec)?;
        }
        ctx.zone(zones::CAPTION, "Title bar items")?;
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
        ctx.block(
            zones::TITLE,
            Block::add(BlockId::from_static("egui_pop.frame"), 0, move |ui, app| {
                title_block(ui, app, &name);
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

/// The title bar, in the title zone.
fn title_block(ui: &mut Ui, app: &AppHandle, name: &str) {
    if !follow_setting(ui, app) {
        return;
    }
    let ctx = ui.ctx().clone();
    let title = ctx
        .input(|i| i.viewport().title.clone())
        .unwrap_or_default();
    let mut menu_at = None;
    let out = TitleBar::new(&title).show(
        ui,
        |ui| {
            let r = ui.add(
                Icon::Menu
                    .size(Size::Toolbar)
                    .label("Window menu")
                    .sense_click(),
            );
            if r.clicked() {
                menu_at = Some(r.rect.left_bottom());
            }
            if !name.is_empty() {
                ui.add(
                    egui::Label::new(egui::RichText::new(name).strong())
                        .selectable(false)
                        .sense(egui::Sense::hover()),
                );
            }
        },
        |ui| {
            if app.zone_has_blocks(&zones::CAPTION) {
                app.show_zone(ui, &zones::CAPTION);
            }
        },
    );
    if let Some(at) = menu_at {
        request(app, &ctx, FrameRequest::SystemMenu(at));
    }
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
