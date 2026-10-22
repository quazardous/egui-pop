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
//!   setting, on the left by default, the secondary one across from it.

use egui_pop_core::lock;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, SettingId};
use egui_pop_model::panels::Area;
use serde_json::{Value as Json, json};

use crate::app::{AppHandle, MENU_VIEW};
use crate::steps::Outcome;
use crate::workbench::area_name;

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

/// Declares the commands and the setting (the framework's own plugin
/// does). Each command's icon is its area's layout icon, outlined while
/// the area is hidden and filled while it shows (`icon_on`), on the side
/// the default position gives it.
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
    Ok(())
}
