//! The workbench's layout, as VS Code arranges it.
//!
//! - **Areas shown or hidden**: the commands
//!   `workbench.toggle.primarySideBar` (`Ctrl+B`), `.panel` (`Ctrl+J`)
//!   and `.secondarySideBar` (`Ctrl+Alt+B`) fold a shown area to its
//!   strip, or show it again, through the `workbench.fold` step. An area
//!   with nothing left to show gets back its first closed panel (the
//!   `workbench.show` step). The primary side bar is the start side
//!   area, the panel the bottom one, the secondary side bar the end side
//!   one. The context keys `sideBarVisible`, `panelVisible` and
//!   `auxiliaryBarVisible` say which show, as VS Code's.
//! - **Where the side bars lie**: the `workbench.side_bar_position`
//!   setting, on the left by default, the secondary one across from it
//!   (`workbench.layout.sideBarPosition`); **where the panel lies**: the
//!   `workbench.panel_alignment` setting, under the editors by default
//!   or across the full width (`workbench.layout.panelAlignment`). Each
//!   change is a write of the setting, one undo step.
//! - **The layout menu** ([`MENU_LAYOUT`], "Customize layout", also a
//!   submenu of View): the area toggles, the arrangements (the built-in
//!   templates and the saved ones, the current one ticked), "Save the
//!   current arrangement as…", and the options above.
//! - **The title bar's controls** ([`show`]): VS Code's layout controls,
//!   at the end of the title bar when an application asks
//!   (`FramePlugin::layout_controls`): the "Customize layout" button
//!   opening that menu, then a toggle per area, its icon filled while the
//!   area shows and outlined while it is folded or hidden, its tooltip
//!   giving its key. Drawn by the kit's `icon-button`; the side bars'
//!   icons follow their side, and so does their order: with the primary
//!   side bar on the right, the secondary side bar's toggle comes first.
//!   Menus and the palette show the toggle commands' icons mirrored the
//!   same way ([`command_icon`]).

use egui::{Rect, Response, Sense, Ui, WidgetInfo, WidgetType, vec2};
use egui_pop_core::icon::{Icon, Size};
use egui_pop_core::kit::{Kit, KitCx, Part, State};
use egui_pop_core::lock;
use egui_pop_core::state::Slot;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{BlockId, CommandId, MenuId, OwnerId, SettingId};
use egui_pop_model::menus::MenuItem;
use egui_pop_model::panels::Area;
use egui_pop_model::values::{Change, Set, Value};
use serde_json::{Value as Json, json};

use crate::app::{AppHandle, MENU_VIEW};
use crate::arrangements;
use crate::menus::ListItem;
use crate::steps::Outcome;
use crate::strings as txt;
use crate::workbench::{PanelAlignment, SETTING_PANEL_ALIGNMENT, area_name};

/// The layout menu: "Customize layout".
pub const MENU_LAYOUT: MenuId = MenuId::from_static("app.menu.layout");
/// The title bar's controls, in the `zones::CAPTION` zone
/// (`FramePlugin::layout_controls`).
pub const BLOCK_LAYOUT_CONTROLS: BlockId = BlockId::from_static("egui_pop.frame.layout");

/// Opens the layout menu, under the title bar's button when it is drawn.
pub const CMD_CUSTOMIZE: CommandId = CommandId::from_static("workbench.layout.customize");
/// Puts the primary side bar on the left or the right (`position`;
/// without it, on the other side).
pub const CMD_SIDE_BAR_POSITION: CommandId =
    CommandId::from_static("workbench.layout.sideBarPosition");
/// Lines the bottom panel up (`alignment`; without it, switches between
/// under the editors and the full width).
pub const CMD_PANEL_ALIGNMENT: CommandId =
    CommandId::from_static("workbench.layout.panelAlignment");

/// The lists the layout menu is made of, filled when it opens.
const LIST_ARRANGEMENTS: &str = "workbench.layout.arrangements";
const LIST_SAVE: &str = "workbench.layout.save";
const LIST_SIDE_BAR: &str = "workbench.layout.side_bar";
const LIST_PANEL: &str = "workbench.layout.panel";

/// Where the "Customize layout" button was drawn last, per window.
const BUTTON: Slot<Rect> = Slot::new("api.layout_controls.button");

/// Shows or hides the primary side bar (the start side area): `Ctrl+B`.
pub const CMD_TOGGLE_PRIMARY_SIDE_BAR: CommandId =
    CommandId::from_static("workbench.toggle.primarySideBar");
/// Shows or hides the panel (the bottom area): `Ctrl+J`.
pub const CMD_TOGGLE_PANEL: CommandId = CommandId::from_static("workbench.toggle.panel");
/// Shows or hides the secondary side bar (the end side area):
/// `Ctrl+Alt+B`.
pub const CMD_TOGGLE_SECONDARY_SIDE_BAR: CommandId =
    CommandId::from_static("workbench.toggle.secondarySideBar");

/// The areas the toggles show or hide, with their command, their key and
/// their context key, in VS Code's order.
pub const TOGGLES: [(Area, CommandId, &str); 3] = [
    (
        Area::StartSide,
        CMD_TOGGLE_PRIMARY_SIDE_BAR,
        "sideBarVisible",
    ),
    (Area::Bottom, CMD_TOGGLE_PANEL, "panelVisible"),
    (
        Area::EndSide,
        CMD_TOGGLE_SECONDARY_SIDE_BAR,
        "auxiliaryBarVisible",
    ),
];

/// The command showing or hiding an area (`None` for the centre).
pub fn toggle_command(area: Area) -> Option<CommandId> {
    TOGGLES.iter().find(|t| t.0 == area).map(|t| t.1.clone())
}

/// The setting: where the primary side bar (the start side area) lies,
/// `left` (the default) or `right`; the secondary one lies across from
/// it, and the activity bar beside it.
pub const SETTING_SIDE_BAR_POSITION: SettingId =
    SettingId::from_static("workbench.side_bar_position");

/// Where the primary side bar lies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SideBarPosition {
    /// On the left, the secondary side bar on the right (the default).
    #[default]
    Left,
    /// On the right, the secondary side bar on the left.
    Right,
}

impl SideBarPosition {
    /// The setting's choices, in order.
    pub const CHOICES: [&'static str; 2] = ["left", "right"];

    pub fn as_str(self) -> &'static str {
        match self {
            SideBarPosition::Left => "left",
            SideBarPosition::Right => "right",
        }
    }

    pub fn parse(s: &str) -> Option<SideBarPosition> {
        match s {
            "left" => Some(SideBarPosition::Left),
            "right" => Some(SideBarPosition::Right),
            _ => None,
        }
    }

    /// The other side.
    pub fn other(self) -> SideBarPosition {
        match self {
            SideBarPosition::Left => SideBarPosition::Right,
            SideBarPosition::Right => SideBarPosition::Left,
        }
    }

    /// Whether the side areas are drawn the other way round from their
    /// names (the start one on the right).
    pub(crate) fn flipped(self) -> bool {
        self == SideBarPosition::Right
    }
}

/// The primary side bar's position, as the setting says now.
pub fn side_bar_position(app: &AppHandle) -> SideBarPosition {
    lock(&app.store())
        .get(&format!("settings/{SETTING_SIDE_BAR_POSITION}"))
        .and_then(|e| e.value.as_choice().and_then(SideBarPosition::parse))
        .unwrap_or_default()
}

/// Whether an area shows now: not folded, with a panel to draw (a side
/// area lying over the centre counts while open). `false` while the
/// workbench is busy (asked from inside its own change).
pub fn area_shown(app: &AppHandle, area: Area) -> bool {
    let panels: Vec<_> = {
        let Some(mut wb) = app.try_workbench() else {
            return false;
        };
        let layout = wb.layout();
        if layout.folded.contains(&area) {
            return false;
        }
        layout
            .areas
            .iter()
            .filter(|(a, _)| *a == area)
            .flat_map(|(_, n)| n.panels().into_iter().cloned())
            .collect()
    };
    panels.iter().any(|p| crate::workbench::drawable(app, p))
}

/// The context keys saying which areas show (`sideBarVisible`,
/// `panelVisible`, `auxiliaryBarVisible`).
pub(crate) fn keys(app: &AppHandle) -> Vec<(&'static str, bool)> {
    TOGGLES
        .iter()
        .map(|(area, _, key)| (*key, area_shown(app, *area)))
        .collect()
}

/// A workbench change's outcome as a result: a change asked while the
/// areas are drawn runs after them (`None`), and is no failure.
fn done(outcome: Option<Outcome>) -> Result<(), String> {
    match outcome {
        None | Some(Outcome::Done(Ok(_))) => Ok(()),
        Some(Outcome::Done(Err(e))) => Err(e),
        Some(Outcome::Vetoed { by, reason }) => Err(format!("stopped by `{by}`: {reason}")),
    }
}

/// Shows an area or hides it, as its toggle command does: a shown area
/// folds to its strip (`workbench.fold`); a folded one unfolds; one with
/// nothing to draw gets back its first closed panel (`workbench.show`).
pub fn toggle_area(app: &AppHandle, area: Area) -> Result<Json, String> {
    if area_shown(app, area) {
        done(app.fold_area(area, true))?;
        return Ok(json!({ "area": area_name(area), "shown": false }));
    }
    if app.layout().folded.contains(&area) {
        done(app.fold_area(area, false))?;
    }
    if !area_shown(app, area) {
        let layout = app.layout();
        let closed = app.panels().into_iter().find(|s| {
            s.area == area
                && !s.palette
                && !layout.shows(&s.id)
                && crate::workbench::drawable(app, &s.id)
        });
        let Some(spec) = closed else {
            return Err(format!("nothing to show in {}", area.name()));
        };
        done(app.change_workbench(crate::workbench::Change::Show(spec.id)))?;
    }
    Ok(json!({ "area": area_name(area), "shown": area_shown(app, area) }))
}

/// The icon an area's toggle shows now: filled while the area shows,
/// outlined (`-off`) while it is folded or hidden; a side bar's icon
/// follows the side it lies on.
pub fn toggle_icon(app: &AppHandle, area: Area) -> Icon {
    let on = area_shown(app, area);
    let left = crate::workbench::on_left(area, side_bar_position(app).flipped());
    match (area, left, on) {
        (Area::Bottom | Area::Center, _, true) => Icon::LayoutBottom,
        (Area::Bottom | Area::Center, _, false) => Icon::LayoutBottomOff,
        (_, true, true) => Icon::LayoutLeft,
        (_, true, false) => Icon::LayoutLeftOff,
        (_, false, true) => Icon::LayoutRight,
        (_, false, false) => Icon::LayoutRightOff,
    }
}

/// A command's icon as menus and the palette show it: the side bars'
/// toggles declare their icons for the default position (the primary
/// side bar on the left), and mirror them while it is on the right, as
/// the title bar's toggles do ([`toggle_icon`]). Any other command's
/// icon, or a name that is not a side's, is shown as declared.
pub fn command_icon<'a>(app: &AppHandle, command: &CommandId, name: &'a str) -> &'a str {
    let side_bar =
        *command == CMD_TOGGLE_PRIMARY_SIDE_BAR || *command == CMD_TOGGLE_SECONDARY_SIDE_BAR;
    if !side_bar || !side_bar_position(app).flipped() {
        return name;
    }
    match name {
        "layout-left" => "layout-right",
        "layout-left-off" => "layout-right-off",
        "layout-right" => "layout-left",
        "layout-right-off" => "layout-left-off",
        other => other,
    }
}

/// Writes a layout setting as the user's change: one undo step.
fn write_choice(
    app: &AppHandle,
    setting: &SettingId,
    value: &str,
    origin: &Origin,
) -> Result<Json, String> {
    lock(&app.store())
        .set(
            Set::new(
                &format!("settings/{setting}"),
                Change::To(Value::Choice(value.into())),
                origin.clone(),
            )
            .by(&OwnerId::FRAMEWORK),
        )
        .map_err(|e| e.to_string())?;
    app.egui().request_repaint();
    Ok(json!({ "setting": setting, "value": value }))
}

/// Puts the primary side bar on a side (`workbench.side_bar_position`).
pub fn set_side_bar_position(app: &AppHandle, position: SideBarPosition) -> Result<Json, String> {
    write_choice(
        app,
        &SETTING_SIDE_BAR_POSITION,
        position.as_str(),
        &Origin::User,
    )
}

/// Lines the bottom panel up (`workbench.panel_alignment`).
pub fn set_panel_alignment(app: &AppHandle, alignment: PanelAlignment) -> Result<Json, String> {
    write_choice(
        app,
        &SETTING_PANEL_ALIGNMENT,
        alignment.as_str(),
        &Origin::User,
    )
}

/// Opens the layout menu under a rectangle (the title bar's button), or
/// under the button where it was last drawn, else at the window's top
/// end.
pub fn open_menu(app: &AppHandle, under: Option<Rect>) -> bool {
    let ctx = app.egui();
    let rect = under
        .or_else(|| BUTTON.keyed(ctx.viewport_id()).get(&ctx))
        .unwrap_or_else(|| {
            let screen = ctx.content_rect();
            Rect::from_min_size(
                egui::pos2(screen.right() - 240.0, screen.top()),
                egui::Vec2::ZERO,
            )
        });
    crate::menus::popup_at(app, &MENU_LAYOUT, rect)
}

/// The title bar's layout controls (the [`BLOCK_LAYOUT_CONTROLS`]
/// block): the "Customize layout" button, then the toggles of the left
/// side bar, the panel and the right side bar, from the start, whichever
/// side the primary side bar lies on. Each button's icon is recorded
/// (the `layout controls` surface of the icons the debug bridge lists),
/// so an agent reads which one shows. An application drawing a bar of
/// its own may call it there.
pub fn show(ui: &mut Ui, app: &AppHandle) {
    let ctx = ui.ctx().clone();
    let (left, right) = if side_bar_position(app).flipped() {
        (Area::EndSide, Area::StartSide)
    } else {
        (Area::StartSide, Area::EndSide)
    };
    let mut items = [None, Some(left), Some(Area::Bottom), Some(right)];
    // Laid out from the end (the title bar's items are): the same order
    // on screen.
    if ui.layout().prefer_right_to_left() {
        items.reverse();
    }
    // Its own spacing, not the bar's.
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = Tokens::of(&ctx).space.xs;
        for item in items {
            match item {
                None => {
                    let name = strings::tr(&ctx, &txt::LAYOUT_CUSTOMIZE);
                    let r = button(ui, Icon::Layout, &name, &name, None);
                    egui_pop_core::icon::registry::record(
                        &ctx,
                        "layout controls",
                        &name,
                        Icon::Layout.name(),
                    );
                    BUTTON.keyed(ctx.viewport_id()).set(&ctx, r.rect);
                    if r.clicked() {
                        open_menu(app, Some(r.rect));
                    }
                }
                Some(area) => {
                    let Some(command) = toggle_command(area) else {
                        continue;
                    };
                    let english = app.command(&command).map(|c| c.title).unwrap_or_default();
                    let name = strings::command_title(&ctx, command.as_str(), &english);
                    let tip = match app.keys_for(&command).first() {
                        Some(keys) => strings::tr_args(
                            &ctx,
                            &txt::LAYOUT_WITH_KEY,
                            &[("action", &name), ("keys", keys)],
                        ),
                        None => name.clone(),
                    };
                    let on = area_shown(app, area);
                    let icon = toggle_icon(app, area);
                    let r = button(ui, icon, &name, &tip, Some(on));
                    egui_pop_core::icon::registry::record(
                        &ctx,
                        "layout controls",
                        &name,
                        icon.name(),
                    );
                    if r.clicked() {
                        app.run(command, Json::Null);
                    }
                }
            }
        }
    });
}

/// An icon button of the title bar, drawn by the kit's `icon-button`:
/// the icon says the state (filled or outlined), so a toggle is not drawn
/// selected; it is announced as toggled (`on`).
fn button(ui: &mut Ui, icon: Icon, name: &str, tip: &str, on: Option<bool>) -> Response {
    let t = Tokens::of(ui.ctx());
    let side = Size::Toolbar.points(&t);
    let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    if ui.is_rect_visible(rect) {
        let kit = Kit::of(ui.ctx());
        let b = &kit.icon_button;
        let state = State::from_response(&response);
        let cx = KitCx {
            painter: ui.painter(),
            ppp: ui.ctx().pixels_per_point(),
        };
        b.paint(
            &cx,
            &Part {
                rect,
                room: rect,
                state,
                label: None,
            },
        );
        icon.paint(ui.painter(), rect, b.style(state).text);
    }
    let enabled = ui.is_enabled();
    let name = name.to_string();
    match on {
        Some(on) => {
            response.widget_info(|| WidgetInfo::selected(WidgetType::Button, enabled, on, &name));
        }
        None => response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, &name)),
    }
    response.on_hover_text(tip)
}

/// The arrangements to switch to, the current one ticked; empty without
/// the arrangements plugin.
fn arrangement_items(app: &AppHandle) -> Vec<ListItem> {
    if app.command(&arrangements::CMD_SWITCH).is_none() {
        return Vec::new();
    }
    let current = arrangements::current(app);
    arrangements::names(app)
        .into_iter()
        .map(|name| {
            let on = name == current;
            ListItem::new(&name, arrangements::CMD_SWITCH, json!({ "name": name })).checked(on)
        })
        .collect()
}

/// Declares the commands, the setting and the layout menu (the
/// framework's own plugin does). Each command's icon is its area's
/// layout icon, outlined while the area is hidden and filled while it
/// shows (`icon_on`), on the side the default position gives it; menus
/// and the palette mirror it while the primary side bar is on the right
/// ([`command_icon`]), and the title bar's toggles follow the side the
/// area lies on ([`toggle_icon`]).
pub(crate) fn declare(ctx: &mut crate::Ctx<'_>) -> Result<(), crate::CtxError> {
    let specs = [
        CommandSpec::new(CMD_TOGGLE_PRIMARY_SIDE_BAR, "Toggle primary side bar", "View")
            .icon("layout-left-off")
            .icon_on("layout-left")
            .description("Shows or hides the primary side bar: hidden, it folds to its strip and keeps its panels.")
            .key("Ctrl+B"),
        CommandSpec::new(CMD_TOGGLE_PANEL, "Toggle panel", "View")
            .icon("layout-bottom-off")
            .icon_on("layout-bottom")
            .description("Shows or hides the bottom panel: hidden, it keeps its panels.")
            .key("Ctrl+J"),
        CommandSpec::new(CMD_TOGGLE_SECONDARY_SIDE_BAR, "Toggle secondary side bar", "View")
            .icon("layout-right-off")
            .icon_on("layout-right")
            .description("Shows or hides the secondary side bar: hidden, it folds to its strip and keeps its panels.")
            .key("Ctrl+Alt+B"),
    ];
    for (order, (spec, (area, _, key))) in specs.into_iter().zip(TOGGLES).enumerate() {
        ctx.command(
            spec.checked(key).menu(MENU_VIEW, "3_layout", order as i32),
            move |call| toggle_area(call.app, area),
        )?;
    }
    ctx.setting(
        SETTING_SIDE_BAR_POSITION,
        egui_pop_model::values::Descriptor::choice(
            "Primary side bar position",
            &SideBarPosition::CHOICES,
            SideBarPosition::Left.as_str(),
        )
        .description(
            "Where the primary side bar lies: on the left, the secondary one on the right (left), or the other way round (right).",
        ),
    )?;
    ctx.command(
        CommandSpec::new(CMD_CUSTOMIZE, "Customize layout…", "View")
            .icon("layout")
            .description(
            "Opens the layout menu: the areas, the arrangements, saving the current one, where the side bars and the panel lie.",
        ),
        |call| Ok(json!({ "open": open_menu(call.app, None) })),
    )?
    .command(
        CommandSpec::new(CMD_SIDE_BAR_POSITION, "Toggle primary side bar position", "View")
            .description("Puts the primary side bar on the left or the right (`position`: `left` or `right`; without it, on the other side).")
            .args(json!({ "position": "right" })),
        |call| {
            let position = match call.args.get("position").and_then(Json::as_str) {
                Some(p) => SideBarPosition::parse(p)
                    .ok_or_else(|| format!("no position `{p}` (`left` or `right`)"))?,
                None => side_bar_position(call.app).other(),
            };
            write_choice(call.app, &SETTING_SIDE_BAR_POSITION, position.as_str(), call.origin)
        },
    )?
    .command(
        CommandSpec::new(CMD_PANEL_ALIGNMENT, "Toggle full-width panel", "View")
            .description("Lines the bottom panel up under the editors or across the full width (`alignment`: `center`, `justify`, `start` or `end`; without it, switches between `center` and `justify`).")
            .args(json!({ "alignment": "justify" })),
        |call| {
            let alignment = match call.args.get("alignment").and_then(Json::as_str) {
                Some(a) => PanelAlignment::parse(a).ok_or_else(|| {
                    format!("no alignment `{a}` (`center`, `justify`, `start` or `end`)")
                })?,
                None if crate::workbench::panel_alignment(call.app) == PanelAlignment::Center => {
                    PanelAlignment::Justify
                }
                None => PanelAlignment::Center,
            };
            write_choice(call.app, &SETTING_PANEL_ALIGNMENT, alignment.as_str(), call.origin)
        },
    )?;
    // The layout menu: the areas, the arrangements, saving, the options.
    for (order, (_, command, key)) in TOGGLES.into_iter().enumerate() {
        ctx.menu_item(
            MenuItem::command(MENU_LAYOUT, "1_areas", order as i32, command).checked(key),
        )?;
    }
    for (group, list) in [
        ("2_arrangements", LIST_ARRANGEMENTS),
        ("3_save", LIST_SAVE),
        ("4_side_bar", LIST_SIDE_BAR),
        ("5_panel", LIST_PANEL),
    ] {
        ctx.menu_item(MenuItem::list(MENU_LAYOUT, group, 0, list))?;
    }
    ctx.submenu(MENU_VIEW, "3_layout", 9, MENU_LAYOUT, "Customize layout")?;
    ctx.list_provider(LIST_ARRANGEMENTS, |app, _| arrangement_items(app))
        .list_provider(LIST_SAVE, |app, _| {
            if app.command(&arrangements::CMD_SAVE).is_none() {
                return Vec::new();
            }
            let label = strings::tr(&app.egui(), &txt::LAYOUT_SAVE_AS);
            vec![ListItem::new(
                &label,
                arrangements::CMD_SAVE,
                json!({ "name": "" }),
            )]
        })
        .list_provider(LIST_SIDE_BAR, |app, _| {
            let ctx = app.egui();
            let now = side_bar_position(app);
            [
                (SideBarPosition::Left, &txt::LAYOUT_SIDE_BAR_LEFT),
                (SideBarPosition::Right, &txt::LAYOUT_SIDE_BAR_RIGHT),
            ]
            .into_iter()
            .map(|(p, key)| {
                ListItem::new(
                    &strings::tr(&ctx, key),
                    CMD_SIDE_BAR_POSITION,
                    json!({ "position": p.as_str() }),
                )
                .checked(now == p)
            })
            .collect()
        })
        .list_provider(LIST_PANEL, |app, _| {
            let ctx = app.egui();
            let now = crate::workbench::panel_alignment(app);
            [
                (PanelAlignment::Center, &txt::LAYOUT_PANEL_CENTER),
                (PanelAlignment::Justify, &txt::LAYOUT_PANEL_JUSTIFY),
            ]
            .into_iter()
            .map(|(a, key)| {
                ListItem::new(
                    &strings::tr(&ctx, key),
                    CMD_PANEL_ALIGNMENT,
                    json!({ "alignment": a.as_str() }),
                )
                .checked(now == a)
            })
            .collect()
        });
    Ok(())
}
