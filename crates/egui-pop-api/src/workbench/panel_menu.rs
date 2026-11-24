//! The panel menu: one context menu for a panel wherever it is, docked
//! (its tab, which `egui_stack` opens), floating (its title and its tabs)
//! or detached (its window's tabs).
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! It is a declared menu
//! ([`MENU_PANEL`]), its entries made when it opens by list providers that
//! read where the panel is, each running a workbench command with the
//! panel in its arguments:
//!
//! - docked: Move to each area its rules allow, and Float when it may;
//!   floating or detached: Dock back (where it floated from); floating:
//!   Dock in each side bar and the bottom panel (`palette.dock`);
//! - Detach into a window; Move to window ▸ (the detached windows, by
//!   their panels' titles); Move group to window ▸ (its tab group whole,
//!   to a window or a new one);
//! - Move to group ▸ (the docked tab groups, by their panels' titles);
//! - Close.
//!
//! A floating panel's menu opens through [`crate::menus::context_menu`],
//! so it is a `pop.popup.open` on the bus like every context menu.
//!
//! A tab group has two more ways to open a panel in it, both asking
//! `pop.panel.open` with `here` (the `workbench.panel.open` command):
//!
//! - the group menu ([`MENU_GROUP`], a right click on the empty part of
//!   its tab bar): Open panel ▸, every panel its place allows that is not
//!   one of its tabs, an open one moving there;
//! - the "+" at the end of its tab bar
//!   ([`DockControls::add_buttons`](super::DockControls::add_buttons)):
//!   the closed panels its place allows ([`MENU_GROUP_ADD`]).
//!
//! Both list first "New `<panel>`" for each panel declared with several
//! instances its place allows (`pop.panel.open` with `instance: "new"`).
//! The same entries, opening where the panel's area is, are the list
//! [`LIST_NEW_INSTANCES`], in the View menu and for an application's own
//! Window menu (`MenuItem::list`).

use egui::Response;
use egui_pop_core::strings;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, MenuId, PanelId};
use egui_pop_model::menus::MenuItem;
use egui_pop_model::panels::{Area, Place};
use egui_pop_model::when::WhenContext;
use serde_json::{Value as Json, json};

use super::{Change, area_name, area_text};
use crate::app::AppHandle;
use crate::menus::{self, ListItem};
use crate::steps::Outcome;
use crate::strings as txt;
use crate::{Ctx, CtxError};

/// The panel menu (`args.panel`: the panel it acts on).
pub const MENU_PANEL: MenuId = MenuId::from_static("workbench.panel");
/// Its submenu listing the detached windows the panel may join.
pub const MENU_PANEL_WINDOWS: MenuId = MenuId::from_static("workbench.panel.windows");
/// Its submenu moving the panel's tab group into a window.
pub const MENU_PANEL_GROUP_WINDOWS: MenuId = MenuId::from_static("workbench.panel.group_windows");
/// Its submenu listing the docked tab groups the panel may join.
pub const MENU_PANEL_GROUPS: MenuId = MenuId::from_static("workbench.panel.groups");

/// A tab group's menu (`args.group`: a panel of the group): Open panel ▸.
pub const MENU_GROUP: MenuId = MenuId::from_static("workbench.group");
/// Its submenu listing the panels that may open in the group.
pub const MENU_GROUP_OPEN: MenuId = MenuId::from_static("workbench.group.open");
/// The menu under a tab bar's "+" (`args.group`): the closed panels that
/// may open in the group.
pub const MENU_GROUP_ADD: MenuId = MenuId::from_static("workbench.group.add");

/// Opens a panel through `pop.panel.open` (`panel`; `place`: `last`, the
/// default, `here` or an area's name; `group`: a panel of the tab group
/// `here` means, else the focused panel's).
pub const CMD_PANEL_OPEN: CommandId = CommandId::from_static("workbench.panel.open");
/// "Open panel…": a quick pick in the command palette of every declared
/// panel (and "New `<panel>`" for those declared with several), the
/// chosen one opened where it was (`workbench.panel.open`, `place:
/// "last"`). `Ctrl+Shift+O`.
pub const CMD_PANEL_PICK: CommandId = CommandId::from_static("workbench.panel.pick");
/// Docks a panel in an area (`panel`, `area`).
pub const CMD_PANEL_MOVE: CommandId = CommandId::from_static("workbench.panel.move");
/// Floats a docked panel, where its area is (`panel`).
pub const CMD_PANEL_FLOAT: CommandId = CommandId::from_static("workbench.panel.float");
/// Moves a panel into a window of its own (`panel`; `group`: its tab
/// group whole).
pub const CMD_PANEL_DETACH: CommandId = CommandId::from_static("workbench.panel.detach");
/// Docks a panel in the tab group holding `group` (`panel`, `group`).
pub const CMD_PANEL_MOVE_TO_GROUP: CommandId =
    CommandId::from_static("workbench.panel.move_to_group");
/// Docks a floating or detached panel back where it floated from
/// (`panel`).
pub const CMD_PANEL_DOCK_BACK: CommandId = CommandId::from_static("workbench.panel.dock_back");
/// Closes a panel (`panel`).
pub const CMD_PANEL_CLOSE: CommandId = CommandId::from_static("workbench.panel.close");

const LIST_PLACE: &str = "workbench.panel.place";
const LIST_DETACH: &str = "workbench.panel.detach";
const LIST_WINDOWS: &str = "workbench.panel.windows";
const LIST_GROUP_WINDOWS: &str = "workbench.panel.group_windows";
const LIST_GROUPS: &str = "workbench.panel.groups";
const LIST_CLOSE: &str = "workbench.panel.close";
const LIST_OPEN_HERE: &str = "workbench.group.open";
const LIST_ADD_HERE: &str = "workbench.group.add";
/// "New `<panel>`" for each panel declared with several instances, opening
/// a new one in the panel's area: in the View menu, and for an
/// application's Window menu.
pub const LIST_NEW_INSTANCES: &str = "workbench.panel.new";

/// Declares the panel menu: its commands, its items and their lists.
pub(crate) fn declare(ctx: &mut Ctx<'_>) -> Result<(), CtxError> {
    let example = json!({ "panel": "app.inspector" });
    ctx.command(
        CommandSpec::new(CMD_PANEL_OPEN, "Open panel", "View")
            .description("Opens `panel` through `pop.panel.open`: where it was when it closed (`place`: `last`, the default; that place gone, in the focused panel's tab group when its areas allow it, else its own area), `here` (the tab group holding `group`, else the focused panel's) or in an area (`center`, `start_side`, `end_side`, `bottom`). For a panel declared with several, `instance`: `new` opens a new instance (its id in `instance`), an instance's id opens that one.")
            .when("!modalOpen")
            .hidden()
            .args(json!({ "panel": "app.outline", "place": "here", "group": "app.inspector" })),
        |call| {
            let panel = call
                .args
                .get("panel")
                .and_then(Json::as_str)
                .ok_or("`panel` is missing")?;
            let place = match call.args.get("place").and_then(Json::as_str) {
                Some(name) => crate::bus::OpenPlace::parse(name).ok_or_else(|| {
                    format!("no place `{name}` (`last`, `here` or an area's name)")
                })?,
                None => crate::bus::OpenPlace::Last,
            };
            let mut ask = crate::bus::PanelOpenAsk::new(panel, place, CMD_PANEL_OPEN.as_str());
            if let Some(g) = call.args.get("group").and_then(Json::as_str) {
                ask = ask.group(g);
            }
            let instance = call.args.get("instance").and_then(Json::as_str);
            if let Some(i) = instance {
                ask = ask.instance(i);
            }
            let did = call.app.open_panel(ask)?;
            let mut out = json!({ "panel": panel, "place": place.as_str(), "did": did });
            // The new instance, by its id (the last one of its kind).
            if instance == Some("new") {
                let kind = PanelId::new(panel).map_err(|e| e.to_string())?;
                if let Some(id) = call.app.panel_instances(&kind).last() {
                    out["instance"] = json!(id);
                }
            }
            Ok(out)
        },
    )?
    .command(
        CommandSpec::new(CMD_PANEL_PICK, "Open panel…", "View")
            .description("Lists every panel in the command palette; the one chosen opens where it was when it closed (`workbench.panel.open`, `place: last`).")
            .icon("layout")
            .when("!modalOpen")
            .key("Ctrl+Shift+O"),
        |call| {
            let p = call
                .app
                .service(&crate::palette::PALETTE)
                .ok_or("no palette")?;
            let pick = panel_pick(call.app);
            let count = pick.items.len();
            let group = call.app.current_window_group();
            p.pick_in(group, call.app.when_context(), pick);
            Ok(json!({ "open": true, "items": count }))
        },
    )?
    .command(
        CommandSpec::new(CMD_PANEL_MOVE, "Move panel to area", "View")
            .description("Docks the focused panel, or `panel`, in an area (`area`: `center`, `start_side`, `end_side` or `bottom`).")
            .when("!modalOpen")
            .hidden()
            .args(json!({ "panel": "app.inspector", "area": "bottom" })),
        |call| {
            let panel = panel_of(call.app, call.args)?;
            let name = call
                .args
                .get("area")
                .and_then(Json::as_str)
                .ok_or("`area` is missing")?;
            let area = super::area_from(name).ok_or_else(|| format!("no area `{name}`"))?;
            let floating = call.app.layout().floating_of(&panel).is_some();
            run(
                call.app,
                if floating {
                    Change::Dock(panel, area)
                } else {
                    Change::Move(panel, area)
                },
            )
        },
    )?
    .command(
        CommandSpec::new(CMD_PANEL_FLOAT, "Float panel", "View")
            .description("Floats the focused panel, or `panel`, over the interface, where its area is.")
            .when("!modalOpen")
            .hidden()
            .args(example.clone()),
        |call| {
            let panel = panel_of(call.app, call.args)?;
            // Where its area is, a little in.
            let pos = {
                let wb = call.app.workbench_mut();
                let at = wb
                    .layout
                    .area_of(&panel)
                    .and_then(|a| wb.rects.get(&a))
                    .map_or(wb.content.min, |r| r.min);
                let off = at - wb.content.min;
                [off.x + 24.0, off.y + 24.0]
            };
            run(call.app, Change::Float(panel, pos))
        },
    )?
    .command(
        CommandSpec::new(CMD_PANEL_DETACH, "Detach panel into a window", "View")
            .description("Moves the focused panel, or `panel`, into a window of its own; with `group`, its tab group whole.")
            .when("!modalOpen")
            .hidden()
            .args(example.clone()),
        |call| {
            let panel = panel_of(call.app, call.args)?;
            let group = call.args.get("group").and_then(Json::as_bool) == Some(true);
            run(
                call.app,
                if group {
                    Change::DetachGroup(panel)
                } else {
                    Change::Detach(panel)
                },
            )
        },
    )?
    .command(
        CommandSpec::new(CMD_PANEL_MOVE_TO_GROUP, "Move panel to group", "View")
            .description("Docks the focused panel, or `panel`, in the tab group holding `group`, after its tabs: out of where it was, floating, detached or docked.")
            .when("!modalOpen")
            .hidden()
            .args(json!({ "panel": "app.outline", "group": "app.inspector" })),
        |call| {
            let panel = panel_of(call.app, call.args)?;
            let group = call
                .args
                .get("group")
                .and_then(Json::as_str)
                .ok_or("`group` is missing: a docked panel of the group")?;
            let group = PanelId::new(group).map_err(|e| e.to_string())?;
            run(call.app, Change::JoinGroup(panel, group))
        },
    )?
    .command(
        CommandSpec::new(CMD_PANEL_DOCK_BACK, "Dock panel back", "View")
            .description("Docks the focused panel, or `panel`, floating or detached, back where it floated from (else in its own area).")
            .when("!modalOpen")
            .key("Ctrl+Shift+D")
            .hidden()
            .args(example.clone()),
        |call| {
            let panel = panel_of(call.app, call.args)?;
            run(call.app, Change::DockBack(panel))
        },
    )?
    .command(
        CommandSpec::new(CMD_PANEL_CLOSE, "Close panel", "View")
            .description("Closes the focused panel, or `panel`.")
            .when("!modalOpen")
            .hidden()
            .args(example),
        |call| {
            let panel = panel_of(call.app, call.args)?;
            run(call.app, Change::Hide(panel))
        },
    )?;
    ctx.menu_item(MenuItem::list(MENU_PANEL, "1_place", 0, LIST_PLACE))?
        .menu_item(MenuItem::list(MENU_PANEL, "2_window", 0, LIST_DETACH))?
        .menu_item(
            MenuItem::submenu(
                MENU_PANEL,
                "2_window",
                1,
                MENU_PANEL_WINDOWS,
                "Move to window",
            )
            .when("args.has_windows", false),
        )?
        .menu_item(
            MenuItem::submenu(
                MENU_PANEL,
                "2_window",
                2,
                MENU_PANEL_GROUP_WINDOWS,
                "Move group to window",
            )
            .when("args.group_moves", false),
        )?
        .menu_item(
            MenuItem::submenu(MENU_PANEL, "3_group", 0, MENU_PANEL_GROUPS, "Move to group")
                .when("args.has_groups", false),
        )?
        .menu_item(MenuItem::list(MENU_PANEL, "9_close", 0, LIST_CLOSE))?
        .menu_item(MenuItem::list(MENU_PANEL_WINDOWS, "1", 0, LIST_WINDOWS))?
        .menu_item(MenuItem::list(
            MENU_PANEL_GROUP_WINDOWS,
            "1",
            0,
            LIST_GROUP_WINDOWS,
        ))?
        .menu_item(MenuItem::list(MENU_PANEL_GROUPS, "1", 0, LIST_GROUPS))?
        .menu_item(
            MenuItem::submenu(MENU_GROUP, "1_open", 0, MENU_GROUP_OPEN, "Open panel")
                .when("args.can_open", false),
        )?
        .menu_item(MenuItem::list(MENU_GROUP_OPEN, "1", 0, LIST_OPEN_HERE))?
        .menu_item(MenuItem::list(MENU_GROUP_ADD, "1", 0, LIST_ADD_HERE))?
        .menu_item(MenuItem::list(
            crate::app::MENU_VIEW,
            "4_new",
            0,
            LIST_NEW_INSTANCES,
        ))?;
    ctx.list_provider(LIST_PLACE, |app, cx| with_panel(app, cx, place_entries))
        .list_provider(LIST_DETACH, |app, cx| with_panel(app, cx, detach_entries))
        .list_provider(LIST_WINDOWS, |app, cx| {
            with_panel(app, cx, |app, panel| window_entries(app, panel, false))
        })
        .list_provider(LIST_GROUP_WINDOWS, |app, cx| {
            with_panel(app, cx, |app, panel| window_entries(app, panel, true))
        })
        .list_provider(LIST_GROUPS, |app, cx| with_panel(app, cx, group_entries))
        .list_provider(LIST_OPEN_HERE, |app, cx| {
            with_group(app, cx, |app, group| open_entries(app, group, false))
        })
        .list_provider(LIST_ADD_HERE, |app, cx| {
            with_group(app, cx, |app, group| {
                let entries = open_entries(app, group, true);
                if !entries.is_empty() {
                    return entries;
                }
                // Said, rather than an empty menu.
                vec![
                    ListItem::new(
                        &strings::tr(&app.egui(), &txt::WORKBENCH_NOTHING_TO_OPEN),
                        CMD_PANEL_OPEN,
                        Json::Null,
                    )
                    .disabled(),
                ]
            })
        })
        .list_provider(LIST_NEW_INSTANCES, |app, _| new_entries(app, None))
        .list_provider(LIST_CLOSE, |app, cx| {
            with_panel(app, cx, |app, panel| {
                let ctx = app.egui();
                let title = app.panel_title(&ctx, panel);
                vec![ListItem::new(
                    &strings::tr_args(&ctx, &txt::WORKBENCH_CLOSE_PANEL, &[("panel", &title)]),
                    CMD_PANEL_CLOSE,
                    json!({ "panel": panel }),
                )]
            })
        });
    Ok(())
}

/// Opens the panel menu on a widget's secondary click (or Shift+F10 and
/// the `Menu` key while it has the focus), as a context menu: a floating
/// panel's title, a tab of a floating group.
pub fn context_menu(response: &Response, app: &AppHandle, panel: &PanelId) {
    let by_key =
        response.has_focus() && egui_pop_core::pointer::claim::menu_key(&response.ctx, true);
    // Where the panel is, read only as the menu opens.
    let opening = by_key || egui_pop_core::pointer::claim::opens_context_menu(response);
    let args = if opening {
        menu_args(app, panel)
    } else {
        Json::Null
    };
    menus::context_menu_opened(response, app, &MENU_PANEL, args, by_key);
}

/// Draws the panel menu's items in an open menu: a docked tab's, which
/// `egui_stack` opens.
pub(crate) fn show(ui: &mut egui::Ui, app: &AppHandle, panel: &PanelId) {
    menus::show_args(ui, app, &MENU_PANEL, &menu_args(app, panel));
}

/// Draws a tab group's menu items (Open panel ▸) in an open menu, the
/// group named by one of its panels: the group menu `egui_stack` opens on
/// its bar.
pub(crate) fn show_group(ui: &mut egui::Ui, app: &AppHandle, group: &PanelId) {
    let args = json!({
        "group": group,
        "can_open": !open_entries(app, group, false).is_empty(),
    });
    menus::show_args(ui, app, &MENU_GROUP, &args);
}

/// The menu's arguments for a panel: the panel, and which submenus have
/// entries (`has_windows`, `group_moves`, `has_groups`).
pub fn menu_args(app: &AppHandle, panel: &PanelId) -> Json {
    json!({
        "panel": panel,
        "has_windows": !window_entries(app, panel, false).is_empty(),
        "group_moves": !window_entries(app, panel, true).is_empty(),
        "has_groups": !group_entries(app, panel).is_empty(),
    })
}

/// The panel a command names (`panel`), else the focused one.
fn panel_of(app: &AppHandle, args: &Json) -> Result<PanelId, String> {
    match args.get("panel").and_then(Json::as_str) {
        Some(p) => PanelId::new(p).map_err(|e| e.to_string()),
        None => app
            .try_workbench()
            .and_then(|wb| wb.focused.clone())
            .ok_or_else(|| "no panel is focused: name one (`panel`)".into()),
    }
}

/// Runs a workbench change as a command's result.
fn run(app: &AppHandle, change: Change) -> Result<Json, String> {
    match app.change_workbench(change) {
        Some(Outcome::Done(r)) => r,
        Some(Outcome::Vetoed { by, reason }) => Err(format!("stopped by `{by}`: {reason}")),
        None => Ok(json!({ "queued": true })),
    }
}

/// A list's entries for the menu's panel (`args.panel`), none without one.
fn with_panel(
    app: &AppHandle,
    cx: &WhenContext,
    f: impl FnOnce(&AppHandle, &PanelId) -> Vec<ListItem>,
) -> Vec<ListItem> {
    match PanelId::new(cx.get("args.panel")) {
        Ok(panel) => f(app, &panel),
        Err(_) => Vec::new(),
    }
}

/// A list's entries for the menu's tab group (`args.group`, one of its
/// panels), none without one.
fn with_group(
    app: &AppHandle,
    cx: &WhenContext,
    f: impl FnOnce(&AppHandle, &PanelId) -> Vec<ListItem>,
) -> Vec<ListItem> {
    match PanelId::new(cx.get("args.group")) {
        Ok(group) => f(app, &group),
        Err(_) => Vec::new(),
    }
}

/// The panels that may open in the tab group holding `group`, by title:
/// those its place (an area, a float, a window) allows that are not
/// among its tabs; `closed`: only the closed ones (the "+"). Each opens
/// there through `pop.panel.open` with `here`.
fn open_entries(app: &AppHandle, group: &PanelId, closed: bool) -> Vec<ListItem> {
    let ctx = app.egui();
    let panels: Vec<PanelId> = {
        let wb = app.workbench_mut();
        let place = if let Some(area) = wb.layout.area_of(group) {
            Place::Area(area)
        } else if wb.layout.window_of(group).is_some() {
            Place::Window
        } else if wb.layout.floating_of(group).is_some() {
            Place::Floating
        } else {
            return Vec::new();
        };
        let host = crate::mounts::panel_host(place);
        let mates: Vec<PanelId> = match wb.layout.floating_of(group) {
            Some(g) => g.panels().cloned().collect(),
            None => wb
                .layout
                .group_of(group)
                .map(|(panels, _)| panels)
                .unwrap_or_default(),
        };
        wb.specs
            .iter()
            .filter(|s| !s.bare && !mates.contains(&s.id) && s.refuses(&host).is_none())
            .filter(|s| !closed || !wb.layout.shows(&s.id))
            .map(|s| s.id.clone())
            .collect()
    };
    let mut out: Vec<(String, ListItem)> = panels
        .into_iter()
        .map(|p| {
            let title = app.panel_title(&ctx, &p);
            let item = ListItem::new(
                &title,
                CMD_PANEL_OPEN,
                json!({ "panel": p, "place": "here", "group": group }),
            );
            (title, item)
        })
        .collect();
    out.sort_by_key(|(title, _)| title.to_lowercase());
    let mut entries = new_entries(app, Some(group));
    entries.extend(out.into_iter().map(|(_, item)| item));
    entries
}

/// "Open panel…"'s quick pick: "New `<panel>`" for each panel declared
/// with several instances, then every declared panel by title, each
/// opening where it was.
pub fn panel_pick(app: &AppHandle) -> crate::palette::QuickPick {
    let ctx = app.egui();
    let panels: Vec<PanelId> = app
        .workbench_mut()
        .specs
        .iter()
        .filter(|s| !s.bare && !s.id.is_instance())
        .map(|s| s.id.clone())
        .collect();
    let mut named: Vec<(String, PanelId)> = panels
        .into_iter()
        .map(|p| (app.panel_title(&ctx, &p), p))
        .collect();
    named.sort_by_key(|(title, _)| title.to_lowercase());
    let mut pick = crate::palette::QuickPick::new(
        strings::tr(&ctx, &txt::WORKBENCH_PICK_TITLE),
        strings::tr(&ctx, &txt::WORKBENCH_PICK_HINT),
    );
    for item in new_entries(app, None) {
        pick = pick.item(crate::palette::PickItem::new(
            item.label,
            item.command,
            item.args,
        ));
    }
    for (title, p) in named {
        pick = pick.item(crate::palette::PickItem::new(
            title,
            CMD_PANEL_OPEN,
            json!({ "panel": p, "place": "last" }),
        ));
    }
    pick
}

/// "New `<panel>`" for each panel declared with several instances, by
/// title: in the tab group holding `group` (those its place allows), else
/// in the panel's own area.
fn new_entries(app: &AppHandle, group: Option<&PanelId>) -> Vec<ListItem> {
    let ctx = app.egui();
    let kinds: Vec<PanelId> = {
        let wb = app.workbench_mut();
        let host = match group {
            None => None,
            Some(g) => {
                let place = if let Some(area) = wb.layout.area_of(g) {
                    Place::Area(area)
                } else if wb.layout.window_of(g).is_some() {
                    Place::Window
                } else if wb.layout.floating_of(g).is_some() {
                    Place::Floating
                } else {
                    return Vec::new();
                };
                Some(crate::mounts::panel_host(place))
            }
        };
        wb.specs
            .iter()
            .filter(|s| {
                !s.id.is_instance()
                    && !s.bare
                    && s.instances == egui_pop_model::panels::Instances::Multiple
            })
            .filter(|s| host.as_ref().is_none_or(|h| s.refuses(h).is_none()))
            .map(|s| s.id.clone())
            .collect()
    };
    let mut out: Vec<(String, ListItem)> = kinds
        .into_iter()
        .map(|kind| {
            let title = app.panel_title(&ctx, &kind);
            let args = match group {
                Some(g) => json!({ "panel": kind, "instance": "new", "place": "here", "group": g }),
                None => json!({ "panel": kind, "instance": "new" }),
            };
            let label = strings::tr_args(&ctx, &txt::WORKBENCH_NEW_INSTANCE, &[("panel", &title)]);
            (title, ListItem::new(&label, CMD_PANEL_OPEN, args))
        })
        .collect();
    out.sort_by_key(|(title, _)| title.to_lowercase());
    out.into_iter().map(|(_, item)| item).collect()
}

/// Docked: Move to each area (greyed where its rules keep it out), and
/// Float when it may float; floating or detached: Dock back; floating:
/// Dock in each side and the bottom (greyed where its rules keep it out).
fn place_entries(app: &AppHandle, panel: &PanelId) -> Vec<ListItem> {
    let Some(spec) = app.workbench_mut().spec(panel).cloned() else {
        return Vec::new();
    };
    let ctx = app.egui();
    let layout = app.workbench_mut().layout.clone();
    if layout.area_of(panel).is_none() {
        if layout.floating_of(panel).is_none() && layout.window_of(panel).is_none() {
            return Vec::new();
        }
        let mut out = vec![ListItem::new(
            &strings::tr(&ctx, &txt::WORKBENCH_DOCK_BACK),
            CMD_PANEL_DOCK_BACK,
            json!({ "panel": panel }),
        )];
        // Floating: Dock in each side and the bottom, the way to dock
        // for a pointer whose drag cannot hold the dock keys.
        if layout.floating_of(panel).is_some() {
            out.extend(
                [Area::StartSide, Area::EndSide, Area::Bottom]
                    .into_iter()
                    .map(|area| {
                        let item = ListItem::new(
                            &strings::tr_args(
                                &ctx,
                                &txt::WORKBENCH_DOCK_IN,
                                &[("area", &area_text(&ctx, area))],
                            ),
                            CMD_PANEL_MOVE,
                            json!({ "panel": panel, "area": area_name(area) }),
                        );
                        if spec.allows(area) {
                            item
                        } else {
                            item.disabled()
                        }
                    }),
            );
        }
        return out;
    }
    let mut out: Vec<ListItem> = Area::ALL
        .into_iter()
        .map(|area| {
            let item = ListItem::new(
                &strings::tr_args(
                    &ctx,
                    &txt::WORKBENCH_MOVE_TO,
                    &[("area", &area_text(&ctx, area))],
                ),
                CMD_PANEL_MOVE,
                json!({ "panel": panel, "area": area_name(area) }),
            );
            if spec.allows(area) {
                item
            } else {
                item.disabled()
            }
        })
        .collect();
    if spec.floatable {
        out.push(ListItem::new(
            &strings::tr(&ctx, &txt::WORKBENCH_FLOAT),
            CMD_PANEL_FLOAT,
            json!({ "panel": panel }),
        ));
    }
    out
}

/// Detach into a window, for a panel that may go into one and is not
/// alone in a window already.
fn detach_entries(app: &AppHandle, panel: &PanelId) -> Vec<ListItem> {
    let Some(spec) = app.workbench_mut().spec(panel).cloned() else {
        return Vec::new();
    };
    let alone = app
        .workbench_mut()
        .layout
        .window_of(panel)
        .is_some_and(|a| a.tabs.is_empty());
    if !spec.detachable || alone {
        return Vec::new();
    }
    vec![ListItem::new(
        &strings::tr(&app.egui(), &txt::WORKBENCH_DETACH),
        CMD_PANEL_DETACH,
        json!({ "panel": panel }),
    )]
}

/// The detached windows a panel may join, by their panels' titles (the
/// way into a window where a drag cannot reach it, `window_pointer`); for
/// `group`, the windows its tab group may join whole, then a new window.
fn window_entries(app: &AppHandle, panel: &PanelId, group: bool) -> Vec<ListItem> {
    let ctx = app.egui();
    let (windows, in_window) = {
        let wb = app.workbench_mut();
        let host = crate::mounts::panel_host(Place::Window);
        let movable = if group {
            wb.layout.floating_of(panel).is_none()
                && wb.layout.group_of(panel).is_some_and(|(panels, _)| {
                    panels.len() > 1
                        && panels
                            .iter()
                            .all(|p| wb.spec(p).is_some_and(|s| s.refuses(&host).is_none()))
                })
        } else {
            wb.spec(panel).is_some_and(|s| s.refuses(&host).is_none())
        };
        if !movable {
            return Vec::new();
        }
        let windows: Vec<(PanelId, Vec<PanelId>)> = (wb.layout.auxiliary.iter())
            .filter(|a| !a.holds(panel))
            .map(|a| (a.panel.clone(), a.panels().cloned().collect()))
            .collect();
        (windows, wb.layout.window_of(panel).is_some())
    };
    let mut out: Vec<ListItem> = windows
        .into_iter()
        .map(|(window, panels)| {
            let mut args = json!({ "panel": panel, "window": window });
            if group {
                args["group"] = json!(true);
            }
            ListItem::new(&titles(app, &ctx, &panels), super::CMD_MOVE_TO_WINDOW, args)
        })
        .collect();
    // A group in a window is one already.
    if group && !in_window {
        out.push(ListItem::new(
            &strings::tr(&ctx, &txt::WORKBENCH_NEW_WINDOW),
            CMD_PANEL_DETACH,
            json!({ "panel": panel, "group": true }),
        ));
    }
    out
}

/// The docked tab groups a panel may join, by their panels' titles, in
/// the areas' order (not its own group; greyed where its rules keep it
/// out of the group's area).
fn group_entries(app: &AppHandle, panel: &PanelId) -> Vec<ListItem> {
    let ctx = app.egui();
    let (groups, spec) = {
        let wb = app.workbench_mut();
        let Some(spec) = wb.spec(panel).cloned() else {
            return Vec::new();
        };
        let mut groups: Vec<(Area, Vec<PanelId>)> = Vec::new();
        for (area, node) in &wb.layout.areas {
            for p in node.panels() {
                if groups.iter().any(|(_, g)| g.contains(p)) {
                    continue;
                }
                if let Some((panels, _)) = wb.layout.group_of(p)
                    && !panels.contains(panel)
                {
                    groups.push((*area, panels));
                }
            }
        }
        (groups, spec)
    };
    groups
        .into_iter()
        .map(|(area, panels)| {
            let host = crate::mounts::panel_host(Place::Area(area));
            let item = ListItem::new(
                &titles(app, &ctx, &panels),
                CMD_PANEL_MOVE_TO_GROUP,
                json!({ "panel": panel, "group": panels[0] }),
            );
            if spec.refuses(&host).is_none() {
                item
            } else {
                item.disabled()
            }
        })
        .collect()
}

/// Panels' titles, joined.
fn titles(app: &AppHandle, ctx: &egui::Context, panels: &[PanelId]) -> String {
    panels
        .iter()
        .map(|p| app.panel_title(ctx, p))
        .collect::<Vec<_>>()
        .join(", ")
}
