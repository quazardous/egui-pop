//! The workbench: panels in areas (the centre, the start and end side
//! bars, the bottom panel), as tabs and splits drawn by `egui_dock`. The
//! versioned layout document (`egui_pop_model::layout::Layout`) is the
//! saved form; each area's dock is how it is drawn and rearranged by
//! hand, and is read back into the layout after each frame.
//!
//! **Placement rules** come from each panel's declaration
//! (`PanelSpec`): the areas it may be in (`only_in`), its smallest size
//! across its area (`min_size`) and the size it asks for
//! (`default_size`). A tab dragged onto another area is moved there when
//! its rules allow it; otherwise it stays, the drop hint says why while
//! dragging, and the refusal is reported (ADR 0003).
//!
//! The shell's zones are placed in areas as panels: `app.side-bar`
//! (the `SIDE_BAR` zone, start side), `app.main` (`MAIN`, centre) and
//! `app.inspector` (`INSPECTOR`, end side); a zone panel with no blocks
//! is not drawn, and one alone in its area is drawn without a tab bar.
//!
//! The layout changes only through named steps (ADR 0011), each with
//! its default, a plugin may veto, redirect or replace them:
//!
//! | Step | Arguments |
//! |---|---|
//! | `workbench.move` | `{ "panel", "area" }` (`center`, `start_side`, `end_side`, `bottom`) |
//! | `workbench.show` | `{ "panel" }`: docks it back (its default area) and shows its tab |
//! | `workbench.hide` | `{ "panel" }`: closes it (kept closed by the layout) |
//! | `workbench.focus` | `{ "panel" }`: shows it and gives it the keyboard |
//! | `workbench.fold` | `{ "area", "folded" }`: an area folded to its strip, or unfolded |
//! | `workbench.place` | `{ "area", "placement" }`: a side area lies `beside` the centre or `over` it |
//! | `workbench.flyout` | `{ "panel", "open" }`: a folded side area's panel opens alone over the centre, or closes |
//! | `workbench.flyout_size` | `{ "panel", "width" }`: the width of a panel's flyout, saved with the layout |
//! | `workbench.detach` | `{ "panel" }`: into a window of its own (its own scale) |
//! | `workbench.attach` | `{ "panel" }`: a detached panel docked back |
//! | `workbench.close_window` | `{ "panel" }`: a detached panel's window closes; the default docks it back, a veto keeps the window |
//! | `workbench.zen` | `{ "zen" }`: zen mode on or off (see below) |
//!
//! Each declared panel gets the commands `<panel>.focus` and
//! `<panel>.toggle`.
//!
//! A side area lies **beside** the centre (an egui side panel, taking the
//! centre's room) or **over** it ([`Placement::Over`]): its folded strip
//! keeps the room in the layout, so the centre keeps its width, and the
//! area, unfolded, lies above the centre next to its strip with the kit's
//! `side-area` shadow; a click on the strip folds or unfolds it.
//! The centre never folds.
//!
//! **Strip icons.** A side area's strip shows one icon per panel, in tab
//! order: its [`PanelSpec::icon`] (a vector icon tinted by the kit's
//! `activity-bar-item`, crisp at any scale), else its title's first
//! letter; the tooltip gives the title and the panel's key, if it has
//! one. On a folded area a click opens that panel alone as a
//! **flyout** over the centre, next to the strip, without moving the
//! layout (`workbench.flyout`); Escape or a click away closes it (it is
//! an overlay of the stack, `egui_pop_core::overlay::hold`). A double
//! click on the icon, or the flyout's pin button, unfolds the area with
//! the panel shown. The flyout's free edge resizes it; its width is
//! kept per panel in the layout (`flyouts`, `workbench.flyout_size`).
//! On an area lying over the centre, open, a click shows the panel.
//! A panel's indicator ([`crate::strips::Indicator::panel`]) shows on
//! its icon while the panel is not shown; [`strip`] lists the icons
//! with their marks for agents, as the `workbench` collector does.
//!
//! **Zen mode** (the `workbench.zen` command, a toggle on `Shift+F11`
//! that an application may rebind) leaves only the centre: the title bar, the
//! menu and status bars, the activity bar, the side and bottom areas and
//! the floating panels are hidden, and Escape (`workbench.leave_zen`)
//! brings them back. The way out stays in sight: a strip at the top
//! centre says how to leave, with a "Leave zen mode" button, for a few
//! seconds on entering, and again whenever the pointer reaches the top
//! edge. It is a state of the session, not of the layout: never saved,
//! so the next start shows the whole workbench.
//!
//! A panel may also float over the interface: a palette
//! (`PanelSpec::palette`) does when it opens, any floatable panel when
//! asked (its tab menu's Float). Floating panels are drawn by
//! [`crate::palettes`], with steps of their own (`palette.open`,
//! `palette.float`, `palette.dock`, `palette.move`, `palette.roll`,
//! `palette.pin`).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use egui::{Color32, Id, Pos2, Rect, Ui, ViewportBuilder, ViewportId, WidgetInfo, WidgetType};
use egui_dock::{DockArea, DockState, NodeIndex, Split, TabViewer};
use egui_pop_core::kit::{
    BoxStyle, Corners, Glyph, Kit, KitCx, Part, Side, State, paint_box, paint_indicator,
};
use egui_pop_core::state::Slot;
use egui_pop_core::strings;
use egui_pop_model::ids::{PanelId, StepId, ZoneId};
use egui_pop_model::layout::{FloatingGroup, Join, Layout, Node, Placement};
use egui_pop_model::panels::{Area, PanelSpec, Panels, Place};
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::steps::Outcome;
use crate::strings as txt;
use crate::window::WindowState;

/// The shell's zones as panels.
pub const PANEL_SIDE_BAR: PanelId = PanelId::from_static("app.side-bar");
pub const PANEL_MAIN: PanelId = PanelId::from_static("app.main");
pub const PANEL_INSPECTOR: PanelId = PanelId::from_static("app.inspector");

pub const STEP_MOVE: StepId = StepId::from_static("workbench.move");
pub const STEP_SHOW: StepId = StepId::from_static("workbench.show");
pub const STEP_HIDE: StepId = StepId::from_static("workbench.hide");
pub const STEP_FOCUS: StepId = StepId::from_static("workbench.focus");
pub const STEP_FOLD: StepId = StepId::from_static("workbench.fold");
pub const STEP_PLACE: StepId = StepId::from_static("workbench.place");
pub const STEP_DETACH: StepId = StepId::from_static("workbench.detach");
pub const STEP_ATTACH: StepId = StepId::from_static("workbench.attach");
pub const STEP_CLOSE_WINDOW: StepId = StepId::from_static("workbench.close_window");
pub const STEP_ZEN: StepId = StepId::from_static("workbench.zen");
pub const STEP_FLYOUT: StepId = StepId::from_static("workbench.flyout");
pub const STEP_FLYOUT_SIZE: StepId = StepId::from_static("workbench.flyout_size");

/// Toggles zen mode: only the centre is shown.
pub const CMD_ZEN: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.zen");
/// Leaves zen mode (Escape while in it).
pub const CMD_LEAVE_ZEN: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.leave_zen");

/// The setting: how the bottom panel lines up with the side bars, as
/// VS Code's panel alignment (see [`PanelAlignment`]).
pub const SETTING_PANEL_ALIGNMENT: egui_pop_model::ids::SettingId =
    egui_pop_model::ids::SettingId::from_static("workbench.panel_alignment");

/// How the bottom panel lines up with the side bars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelAlignment {
    /// The window's whole width, under both side bars (the default).
    #[default]
    Justify,
    /// Between the side bars, which go down to the bottom.
    Center,
    /// From the start edge, under the start side bar, to the end side bar.
    Start,
    /// From the start side bar to the end edge, under the end side bar.
    End,
}

impl PanelAlignment {
    /// The setting's choices, in order.
    pub const CHOICES: [&'static str; 4] = ["justify", "center", "start", "end"];

    pub fn as_str(self) -> &'static str {
        match self {
            PanelAlignment::Justify => "justify",
            PanelAlignment::Center => "center",
            PanelAlignment::Start => "start",
            PanelAlignment::End => "end",
        }
    }

    pub fn parse(s: &str) -> Option<PanelAlignment> {
        [
            PanelAlignment::Justify,
            PanelAlignment::Center,
            PanelAlignment::Start,
            PanelAlignment::End,
        ]
        .into_iter()
        .find(|a| a.as_str() == s)
    }

    /// The order the side and bottom areas are drawn in: an area drawn
    /// earlier spans the whole height (a side bar) or width (the bottom).
    fn order(self) -> [Area; 3] {
        match self {
            PanelAlignment::Justify => [Area::Bottom, Area::StartSide, Area::EndSide],
            PanelAlignment::Center => [Area::StartSide, Area::EndSide, Area::Bottom],
            PanelAlignment::Start => [Area::EndSide, Area::Bottom, Area::StartSide],
            PanelAlignment::End => [Area::StartSide, Area::Bottom, Area::EndSide],
        }
    }
}

/// The bottom panel's alignment, as the setting says now.
pub fn panel_alignment(app: &AppHandle) -> PanelAlignment {
    egui_pop_core::lock(&app.store())
        .get(&format!("settings/{SETTING_PANEL_ALIGNMENT}"))
        .and_then(|e| e.value.as_choice().and_then(PanelAlignment::parse))
        .unwrap_or_default()
}

/// Where a panel detached without a known place opens, and its size.
pub const DETACHED: [f32; 4] = [80.0, 80.0, 480.0, 360.0];

/// The workbench's steps, as the framework declares them.
pub(crate) fn steps() -> Vec<crate::steps::StepSpec> {
    use crate::steps::StepSpec;
    vec![
        StepSpec::new(STEP_MOVE, "A panel moves to an area (`panel`, `area`).")
            .args(json!({ "panel": "app.inspector", "area": "start_side" })),
        StepSpec::new(
            STEP_SHOW,
            "A panel is shown, docked back in its area if it was closed (`panel`).",
        )
        .args(json!({ "panel": "app.inspector" })),
        StepSpec::new(STEP_HIDE, "A panel is closed (`panel`).")
            .args(json!({ "panel": "app.inspector" })),
        StepSpec::new(
            STEP_FOCUS,
            "A panel is shown and gets the keyboard (`panel`).",
        )
        .args(json!({ "panel": "app.inspector" })),
        StepSpec::new(
            STEP_FOLD,
            "An area folds to its strip, or unfolds (`area`, `folded`).",
        )
        .args(json!({ "area": "start_side", "folded": true })),
        StepSpec::new(
            STEP_PLACE,
            "A side area lies beside the centre or over it (`area`, `placement`: `beside` or `over`).",
        )
        .args(json!({ "area": "start_side", "placement": "over" })),
        StepSpec::new(
            STEP_FLYOUT,
            "A folded side area's panel opens alone over the centre (`panel`, `open`), or closes (`open`: false).",
        )
        .args(json!({ "panel": "app.side-bar", "open": true })),
        StepSpec::new(
            STEP_FLYOUT_SIZE,
            "A panel's flyout is given a width in points (`panel`, `width`), saved with the layout.",
        )
        .args(json!({ "panel": "app.side-bar", "width": 280.0 })),
        StepSpec::new(
            STEP_DETACH,
            "A panel moves to a window of its own (`panel`).",
        )
        .args(json!({ "panel": "app.inspector" })),
        StepSpec::new(
            STEP_ATTACH,
            "A detached panel docks back in its area (`panel`).",
        )
        .args(json!({ "panel": "app.inspector" })),
        StepSpec::new(
            STEP_CLOSE_WINDOW,
            "A detached panel's window closes; by default the panel docks back (`panel`).",
        )
        .args(json!({ "panel": "app.inspector" })),
        StepSpec::new(
            STEP_ZEN,
            "Zen mode starts (`zen`: true: only the centre is shown) or ends (false).",
        )
        .args(json!({ "zen": true })),
    ]
}

/// How close to the areas' edges a dragged tab offers a closed side or
/// bottom area.
const EDGE: f32 = 48.0;

/// An area's name in documents and arguments.
pub fn area_name(area: Area) -> &'static str {
    match area {
        Area::Center => "center",
        Area::StartSide => "start_side",
        Area::EndSide => "end_side",
        Area::Bottom => "bottom",
    }
}

/// An area from its name.
pub fn area_from(name: &str) -> Option<Area> {
    Area::ALL.into_iter().find(|a| area_name(*a) == name)
}

/// The drop hint shown while a tab is dragged over another area: what
/// [`AppHandle::drop_hint`], `overlay_rects` (`drop:<panel>:<area>`) and
/// the `workbench` collector report.
#[derive(Clone, Debug, PartialEq)]
pub struct DropHint {
    pub panel: PanelId,
    /// The area the tab would go to (`area_name`).
    pub area: &'static str,
    /// Where the hint is drawn, in points.
    pub rect: Rect,
    /// Why the panel may not go there, as the hint says it; `None` when
    /// it may.
    pub refusal: Option<String>,
    /// The ghost of the panel's tab where it would land; `None` when the
    /// drop is refused.
    pub preview: Option<TabPreview>,
}

/// Where a drop hint draws the dragged panel's tab (the kit's
/// `tab-preview`): in the target area's tab bar, after its tabs, where a
/// drop on the area puts it.
#[derive(Clone, Debug, PartialEq)]
pub struct TabPreview {
    /// The area (`area_name`).
    pub area: &'static str,
    /// The tab's index among the tabs it joins (their count: a drop on
    /// an area appends to its first tabs).
    pub index: usize,
    /// Where the ghost is drawn, in points.
    pub rect: Rect,
}

/// An area's first tab bar as drawn last frame, where a drop on the area
/// adds its tab: the bar, the right edge of its last tab, its tab count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabBar {
    pub rect: Rect,
    pub end: f32,
    pub tabs: usize,
}

impl DropHint {
    /// Its name among the overlays: `drop:<panel>:<area>`.
    pub fn name(&self) -> String {
        format!("drop:{}:{}", self.panel, self.area)
    }
}

/// A change of the workbench, as a step runs it.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Move(PanelId, Area),
    Show(PanelId),
    Hide(PanelId),
    Focus(PanelId),
    /// An area folded to its strip (`true`) or unfolded.
    Fold(Area, bool),
    /// A side area lies beside the centre or over it.
    Place(Area, Placement),
    /// A panel moved to a window of its own.
    Detach(PanelId),
    /// A detached panel docked back in its area.
    Attach(PanelId),
    /// A detached panel's window closed (by default: docked back).
    CloseWindow(PanelId),
    /// A palette opens: floating (where it was, or cascaded), or docked
    /// when it may not float.
    OpenPalette(PanelId),
    /// A panel floats over the interface, its top-left at a place (in
    /// points from the workbench's top-left).
    Float(PanelId, [f32; 2]),
    /// A floating panel docks in an area (taken out of its group).
    Dock(PanelId, Area),
    /// Every member of a floating panel's group docks in an area, as
    /// tabs.
    DockGroup(PanelId, Area),
    /// Floating panels join the group of another one: above it, below
    /// it, or as tabs.
    Group(Vec<PanelId>, PanelId, Join),
    /// A floating panel comes out of its group, alone at a place.
    Ungroup(PanelId, [f32; 2]),
    /// A floating panel moves.
    MovePalette(PanelId, [f32; 2]),
    /// A floating panel's group is resized (its frame, in points), or
    /// sized by its content again (`None`).
    ResizePalette(PanelId, Option<[f32; 2]>),
    /// A floating panel rolls up to its header band (`true`) or down.
    Roll(PanelId, bool),
    /// A floating panel is pinned (`true`) or unpinned.
    Pin(PanelId, bool),
    /// A folded side area's panel opens alone over the centre (`true`),
    /// or closes.
    Flyout(PanelId, bool),
    /// The width of a panel's flyout, in points.
    FlyoutSize(PanelId, f32),
    /// Zen mode on (`true`: only the centre is shown) or off.
    Zen(bool),
}

/// `[x, y]` as a place.
fn pos_of(args: &Json) -> Result<[f32; 2], String> {
    let a = args
        .get("pos")
        .and_then(Json::as_array)
        .ok_or("`pos` is missing")?;
    match a.as_slice() {
        [x, y] => Ok([
            x.as_f64().ok_or("`pos` holds numbers")? as f32,
            y.as_f64().ok_or("`pos` holds numbers")? as f32,
        ]),
        _ => Err("`pos` is `[x, y]`".into()),
    }
}

/// `[width, height]` as a size, or `null` (sized by the content).
fn size_of(args: &Json) -> Result<Option<[f32; 2]>, String> {
    let Some(a) = args.get("size").filter(|s| !s.is_null()) else {
        return Ok(None);
    };
    match a.as_array().map(Vec::as_slice) {
        Some([w, h]) => {
            let w = w.as_f64().ok_or("`size` holds numbers")? as f32;
            let h = h.as_f64().ok_or("`size` holds numbers")? as f32;
            if w > 0.0 && h > 0.0 {
                Ok(Some([w, h]))
            } else {
                Err("`size` is positive".into())
            }
        }
        _ => Err("`size` is `[width, height]` or `null`".into()),
    }
}

impl Change {
    pub(crate) fn step(&self) -> (StepId, Json) {
        match self {
            Change::Move(p, a) => (STEP_MOVE, json!({ "panel": p, "area": area_name(*a) })),
            Change::Show(p) => (STEP_SHOW, json!({ "panel": p })),
            Change::Hide(p) => (STEP_HIDE, json!({ "panel": p })),
            Change::Focus(p) => (STEP_FOCUS, json!({ "panel": p })),
            Change::Fold(a, f) => (STEP_FOLD, json!({ "area": area_name(*a), "folded": f })),
            Change::Place(a, p) => (
                STEP_PLACE,
                json!({ "area": area_name(*a), "placement": p.as_str() }),
            ),
            Change::Detach(p) => (STEP_DETACH, json!({ "panel": p })),
            Change::Attach(p) => (STEP_ATTACH, json!({ "panel": p })),
            Change::CloseWindow(p) => (STEP_CLOSE_WINDOW, json!({ "panel": p })),
            Change::OpenPalette(p) => (crate::palettes::STEP_OPEN, json!({ "panel": p })),
            Change::Float(p, pos) => (
                crate::palettes::STEP_FLOAT,
                json!({ "panel": p, "pos": pos }),
            ),
            Change::Dock(p, a) => (
                crate::palettes::STEP_DOCK,
                json!({ "panel": p, "area": area_name(*a) }),
            ),
            Change::DockGroup(p, a) => (
                crate::palettes::STEP_DOCK,
                json!({ "panel": p, "area": area_name(*a), "group": true }),
            ),
            Change::Group(panels, into, at) => (
                crate::palettes::STEP_GROUP,
                json!({ "panels": panels, "into": into, "at": at.as_str() }),
            ),
            Change::Ungroup(p, pos) => (
                crate::palettes::STEP_UNGROUP,
                json!({ "panel": p, "pos": pos }),
            ),
            Change::MovePalette(p, pos) => (
                crate::palettes::STEP_MOVE,
                json!({ "panel": p, "pos": pos }),
            ),
            Change::ResizePalette(p, size) => (
                crate::palettes::STEP_RESIZE,
                json!({ "panel": p, "size": size }),
            ),
            Change::Roll(p, on) => (
                crate::palettes::STEP_ROLL,
                json!({ "panel": p, "rolled": on }),
            ),
            Change::Pin(p, on) => (
                crate::palettes::STEP_PIN,
                json!({ "panel": p, "pinned": on }),
            ),
            Change::Zen(on) => (STEP_ZEN, json!({ "zen": on })),
            Change::Flyout(p, open) => (STEP_FLYOUT, json!({ "panel": p, "open": open })),
            Change::FlyoutSize(p, width) => {
                (STEP_FLYOUT_SIZE, json!({ "panel": p, "width": width }))
            }
        }
    }

    /// The event a palette's change publishes once done.
    pub(crate) fn event(&self) -> Option<&'static str> {
        Some(match self {
            Change::OpenPalette(_) => "palette.opened",
            Change::Float(..) => "palette.floated",
            Change::Dock(..) | Change::DockGroup(..) => "palette.docked",
            Change::Group(..) => "palette.grouped",
            Change::Ungroup(..) => "palette.ungrouped",
            Change::MovePalette(..) => "palette.moved",
            Change::ResizePalette(..) => "palette.resized",
            Change::Roll(..) => "palette.rolled",
            Change::Pin(..) => "palette.pinned",
            _ => return None,
        })
    }

    /// The change a step's (possibly redirected) arguments ask for.
    pub(crate) fn from_args(step: &StepId, args: &Json) -> Result<Change, String> {
        if *step == STEP_ZEN {
            return Ok(Change::Zen(
                args.get("zen").and_then(Json::as_bool).unwrap_or(true),
            ));
        }
        if *step == STEP_FOLD {
            let name = args
                .get("area")
                .and_then(Json::as_str)
                .ok_or("`area` is missing")?;
            let area = area_from(name).ok_or_else(|| format!("no area `{name}`"))?;
            let folded = args.get("folded").and_then(Json::as_bool).unwrap_or(true);
            return Ok(Change::Fold(area, folded));
        }
        if *step == STEP_PLACE {
            let name = args
                .get("area")
                .and_then(Json::as_str)
                .ok_or("`area` is missing")?;
            let area = area_from(name).ok_or_else(|| format!("no area `{name}`"))?;
            let name = args
                .get("placement")
                .and_then(Json::as_str)
                .ok_or("`placement` is missing")?;
            let placement = Placement::parse(name)
                .ok_or_else(|| format!("no placement `{name}` (`beside` or `over`)"))?;
            return Ok(Change::Place(area, placement));
        }
        if *step == crate::palettes::STEP_GROUP {
            let panels = args
                .get("panels")
                .and_then(Json::as_array)
                .ok_or("`panels` is missing")?
                .iter()
                .map(|p| {
                    let p = p.as_str().ok_or("`panels` holds panel ids")?;
                    PanelId::new(p).map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, String>>()?;
            let into = args
                .get("into")
                .and_then(Json::as_str)
                .ok_or("`into` is missing")?;
            let into = PanelId::new(into).map_err(|e| e.to_string())?;
            let name = args.get("at").and_then(Json::as_str).unwrap_or("below");
            let at = Join::parse(name)
                .ok_or_else(|| format!("no place `{name}` (`above`, `below` or `tab`)"))?;
            return Ok(Change::Group(panels, into, at));
        }
        let panel = args
            .get("panel")
            .and_then(Json::as_str)
            .ok_or("`panel` is missing")?;
        let panel = PanelId::new(panel).map_err(|e| e.to_string())?;
        let flag = |key: &str| args.get(key).and_then(Json::as_bool).unwrap_or(true);
        Ok(match step.as_str() {
            "palette.open" => Change::OpenPalette(panel),
            "palette.float" => Change::Float(panel, pos_of(args)?),
            "palette.move" => Change::MovePalette(panel, pos_of(args)?),
            "palette.resize" => Change::ResizePalette(panel, size_of(args)?),
            "palette.ungroup" => Change::Ungroup(panel, pos_of(args)?),
            "palette.roll" => Change::Roll(panel, flag("rolled")),
            "palette.pin" => Change::Pin(panel, flag("pinned")),
            "workbench.flyout" => Change::Flyout(panel, flag("open")),
            "workbench.flyout_size" => {
                let width = args
                    .get("width")
                    .and_then(Json::as_f64)
                    .ok_or("`width` is missing")? as f32;
                if !(width.is_finite() && width > 0.0) {
                    return Err("`width` is positive".into());
                }
                Change::FlyoutSize(panel, width)
            }
            "workbench.move" | "palette.dock" => {
                let name = args
                    .get("area")
                    .and_then(Json::as_str)
                    .ok_or("`area` is missing")?;
                let area = area_from(name).ok_or_else(|| format!("no area `{name}`"))?;
                if step.as_str() == "palette.dock" {
                    if args.get("group").and_then(Json::as_bool) == Some(true) {
                        Change::DockGroup(panel, area)
                    } else {
                        Change::Dock(panel, area)
                    }
                } else {
                    Change::Move(panel, area)
                }
            }
            "workbench.show" => Change::Show(panel),
            "workbench.detach" => Change::Detach(panel),
            "workbench.attach" => Change::Attach(panel),
            "workbench.close_window" => Change::CloseWindow(panel),
            "workbench.hide" => Change::Hide(panel),
            _ => Change::Focus(panel),
        })
    }
}

/// The workbench's state, held by the application.
pub(crate) struct Workbench {
    /// The declared panels, in declaration order (refreshed at rebuild).
    pub specs: Vec<PanelSpec>,
    /// Panels showing a zone.
    pub zone_of: BTreeMap<PanelId, ZoneId>,
    pub layout: Layout,
    /// Each docked area as drawn; taken out while drawn.
    docks: BTreeMap<Area, DockState<PanelId>>,
    /// Where each area was drawn last frame.
    pub rects: BTreeMap<Area, Rect>,
    /// Each area's first tab bar as drawn last frame (none for an area
    /// drawn bare).
    pub bars: BTreeMap<Area, TabBar>,
    /// The areas' rectangle.
    pub content: Rect,
    /// A tab being dragged: its panel, its area, the pointer.
    pub dragging: Option<(PanelId, Area, Pos2)>,
    /// The drop hint drawn last frame.
    pub hint: Option<DropHint>,
    pub focused: Option<PanelId>,
    /// The layout changed outside the docks: project it again.
    stale: bool,
    /// Docks taken out to be drawn: the layout is not read back then.
    taken: usize,
    /// The application's named layouts (`Ctx::arrangement`): owner,
    /// name, layout.
    pub templates: Vec<(egui_pop_model::ids::OwnerId, String, Layout)>,
    /// A palette's band being dragged.
    pub palette_drag: Option<crate::palettes::PaletteDrag>,
    /// A palette's edge being dragged.
    pub palette_resize: Option<crate::palettes::PaletteResize>,
    /// Where each floating panel's group was drawn last frame.
    pub palette_rects: BTreeMap<PanelId, Rect>,
    /// Where each floating panel was drawn last frame, itself (in a
    /// group, its band and content).
    pub member_rects: BTreeMap<PanelId, Rect>,
    /// Where the dragged palettes would join a group, while dragged over
    /// another palette.
    pub group_hint: Option<crate::palettes::GroupHint>,
    /// Zen mode: only the centre is drawn. Not part of the layout, so
    /// never saved.
    pub zen: bool,
    /// The panel shown as a flyout out of its folded side area's strip:
    /// the session's state, never saved.
    pub flyout: Option<PanelId>,
    /// Where the flyout was drawn last frame.
    pub flyout_rect: Option<Rect>,
}

impl Default for Workbench {
    fn default() -> Self {
        Self {
            specs: Vec::new(),
            zone_of: BTreeMap::new(),
            layout: Layout::default(),
            docks: BTreeMap::new(),
            rects: BTreeMap::new(),
            bars: BTreeMap::new(),
            content: Rect::NOTHING,
            dragging: None,
            hint: None,
            focused: None,
            stale: true,
            taken: 0,
            templates: Vec::new(),
            palette_drag: None,
            palette_resize: None,
            palette_rects: BTreeMap::new(),
            member_rects: BTreeMap::new(),
            group_hint: None,
            zen: false,
            flyout: None,
            flyout_rect: None,
        }
    }
}

impl Workbench {
    /// A template by name (the last offered wins).
    pub fn template(&self, name: &str) -> Option<Layout> {
        self.templates
            .iter()
            .rev()
            .find(|t| t.1 == name)
            .map(|t| t.2.clone())
    }

    pub fn spec(&self, id: &PanelId) -> Option<&PanelSpec> {
        self.specs.iter().find(|s| s.id == *id)
    }

    fn panels(&self) -> Panels {
        let mut p = Panels::new();
        for s in &self.specs {
            let _ = p.declare(&egui_pop_model::ids::OwnerId::FRAMEWORK, s.clone());
        }
        p
    }

    /// The declared panels changed (or a layout was loaded): the layout
    /// is repaired against them; says what it changed.
    pub fn refresh(&mut self, specs: Vec<PanelSpec>, screens: &[&str]) -> Vec<String> {
        self.read_back();
        self.specs = specs;
        let panels = self.panels();
        let (layout, notes) = std::mem::take(&mut self.layout).repair(&panels, screens);
        self.layout = layout;
        self.stale = true;
        notes
    }

    /// Puts in a layout (loaded, or an arrangement), repaired.
    pub fn set_layout(&mut self, layout: Layout, screens: &[&str]) -> Vec<String> {
        let (layout, notes) = layout.repair(&self.panels(), screens);
        self.layout = layout;
        self.stale = true;
        notes
    }

    /// The layout as it stands (the docks read back).
    pub fn layout(&mut self) -> Layout {
        self.read_back();
        self.layout.clone()
    }

    /// Reads the docks' tabs and splits back into the layout.
    fn read_back(&mut self) {
        if self.stale || self.taken > 0 || self.docks.is_empty() {
            return;
        }
        let mut areas: Vec<(Area, Node)> = self
            .docks
            .iter()
            .filter_map(|(a, d)| from_dock(d).map(|n| (*a, n)))
            .collect();
        areas.sort_by_key(|(a, _)| *a);
        self.layout.areas = areas;
    }

    /// Makes the docks show the layout, when it changed outside them.
    pub fn project(&mut self) {
        if !self.stale {
            return;
        }
        self.docks = self
            .layout
            .areas
            .iter()
            .map(|(a, n)| (*a, to_dock(n)))
            .collect();
        self.stale = false;
    }

    /// Whether a panel shows (docked or detached, not closed).
    pub fn shows(&mut self, panel: &PanelId) -> bool {
        self.read_back();
        self.layout.shows(panel)
    }

    /// Puts a closed panel back: a palette floating (cascaded), any other
    /// in its area.
    fn reopen(&mut self, spec: &PanelSpec) {
        if spec.palette {
            let n = self.layout.floating.len();
            self.layout.float(&spec.id, FloatingGroup::cascade(n));
        } else {
            self.layout.place(&spec.id, spec.area);
        }
    }

    /// Applies a change; refused when the panel's rules do not allow it.
    /// Anchors a floating panel to the workbench edges its new place lies
    /// on (`palettes::anchor_of`), or to none: dropped against an edge,
    /// it sticks to it; dropped away, it no longer does.
    /// `alone`: it just came out of its group, its size is its own. A
    /// group resized (`size`) is anchored with its new size.
    fn anchor_palette(&mut self, p: &PanelId, alone: bool) {
        let rects = if alone {
            &self.member_rects
        } else {
            &self.palette_rects
        };
        let resized = (!alone)
            .then(|| self.layout.floating_of(p).and_then(|g| g.size))
            .flatten();
        let size = resized
            .map(egui::Vec2::from)
            .unwrap_or_else(|| rects.get(p).map_or(egui::Vec2::ZERO, |r| r.size()));
        let content = self.content;
        if let Some(f) = self.layout.floating_mut(p) {
            f.anchor = if content.is_positive() {
                let rect = Rect::from_min_size(content.min + egui::Vec2::from(f.pos), size);
                crate::palettes::anchor_of(rect, content)
            } else {
                Default::default()
            };
        }
    }

    pub fn apply(&mut self, change: &Change) -> Result<Json, String> {
        self.read_back();
        let spec_of = |p: &PanelId| {
            self.spec(p)
                .cloned()
                .ok_or_else(|| format!("no panel `{p}`"))
        };
        let out = match change {
            Change::Move(p, area) | Change::Dock(p, area) => {
                let spec = spec_of(p)?;
                if let Some(why) = spec.refuses(&crate::mounts::panel_host(Place::Area(*area))) {
                    return Err(why);
                }
                self.layout.place(p, *area);
                json!({ "panel": p, "area": area_name(*area) })
            }
            Change::Show(p) => {
                let spec = spec_of(p)?;
                if !self.layout.shows(p) {
                    self.reopen(&spec);
                }
                self.layout.activate(p);
                json!({ "panel": p, "shown": true })
            }
            Change::Hide(p) => {
                spec_of(p)?;
                self.layout.hide(p);
                if self.focused.as_ref() == Some(p) {
                    self.focused = None;
                }
                json!({ "panel": p, "shown": false })
            }
            Change::Focus(p) => {
                let spec = spec_of(p)?;
                if !self.layout.shows(p) {
                    self.reopen(&spec);
                }
                self.layout.activate(p);
                self.focused = Some(p.clone());
                let area = self.layout.area_of(p);
                self.layout.folded.retain(|a| Some(*a) != area);
                json!({ "panel": p, "focused": true })
            }
            Change::Detach(p) => {
                let spec = spec_of(p)?;
                if let Some(why) = spec.refuses(&crate::mounts::panel_host(Place::Window)) {
                    return Err(why);
                }
                if self.layout.auxiliary.iter().any(|a| a.panel == *p) {
                    return Ok(json!({ "panel": p, "detached": true }));
                }
                // Where it was drawn, in the window's coordinates.
                let rect = self
                    .layout
                    .area_of(p)
                    .and_then(|a| self.rects.get(&a))
                    .map_or(DETACHED, |r| {
                        [
                            r.min.x + 40.0,
                            r.min.y + 40.0,
                            r.width().max(320.0),
                            r.height().max(240.0),
                        ]
                    });
                self.layout.remove(p);
                self.layout.hidden.retain(|h| h != p);
                self.layout
                    .auxiliary
                    .push(egui_pop_model::layout::Auxiliary {
                        panel: p.clone(),
                        screen: crate::app::SCREENS[0].into(),
                        rect,
                    });
                json!({ "panel": p, "detached": true })
            }
            Change::Attach(p) | Change::CloseWindow(p) => {
                let spec = spec_of(p)?;
                self.layout.auxiliary.retain(|a| a.panel != *p);
                self.layout.place(p, spec.area);
                json!({ "panel": p, "detached": false })
            }
            Change::OpenPalette(p) => {
                let spec = spec_of(p)?;
                if !self.layout.shows(p) {
                    if spec.floatable {
                        let n = self.layout.floating.len();
                        self.layout.float(p, FloatingGroup::cascade(n));
                    } else {
                        self.reopen(&spec);
                    }
                }
                self.layout.activate(p);
                let floating = self.layout.floating_of(p).is_some();
                json!({ "panel": p, "shown": true, "floating": floating })
            }
            Change::Float(p, pos) => {
                let spec = spec_of(p)?;
                if let Some(why) = spec.refuses(&crate::mounts::panel_host(Place::Floating)) {
                    return Err(why);
                }
                self.layout.float(p, *pos);
                self.anchor_palette(p, false);
                json!({ "panel": p, "pos": pos })
            }
            Change::DockGroup(p, area) => {
                let panels: Vec<PanelId> = match self.layout.floating_of(p) {
                    Some(g) => g.panels().cloned().collect(),
                    None => vec![p.clone()],
                };
                let host = crate::mounts::panel_host(Place::Area(*area));
                for q in &panels {
                    if let Some(why) = spec_of(q)?.refuses(&host) {
                        return Err(why);
                    }
                }
                let docked = self.layout.dock_group(p, *area);
                json!({ "panel": p, "area": area_name(*area), "panels": docked })
            }
            Change::Group(panels, into, at) => {
                self.layout.group(panels, into, *at)?;
                json!({ "panels": panels, "into": into, "at": at.as_str() })
            }
            Change::Ungroup(p, pos) => {
                self.layout.ungroup(p, *pos)?;
                self.anchor_palette(p, true);
                json!({ "panel": p, "pos": pos })
            }
            Change::MovePalette(p, pos) => {
                let f = self
                    .layout
                    .floating_mut(p)
                    .ok_or_else(|| format!("`{p}` does not float"))?;
                f.pos = *pos;
                self.anchor_palette(p, false);
                json!({ "panel": p, "pos": pos })
            }
            Change::ResizePalette(p, size) => {
                let f = self
                    .layout
                    .floating_mut(p)
                    .ok_or_else(|| format!("`{p}` does not float"))?;
                f.size = *size;
                self.anchor_palette(p, false);
                json!({ "panel": p, "size": size })
            }
            Change::Roll(p, on) => {
                let m = self
                    .layout
                    .floating_mut(p)
                    .and_then(|g| g.member_mut(p))
                    .ok_or_else(|| format!("`{p}` does not float"))?;
                m.rolled = *on;
                json!({ "panel": p, "rolled": on })
            }
            Change::Pin(p, on) => {
                let f = self
                    .layout
                    .floating_mut(p)
                    .ok_or_else(|| format!("`{p}` does not float"))?;
                f.pinned = *on;
                json!({ "panel": p, "pinned": on })
            }
            Change::Fold(area, folded) => {
                if *area == Area::Center && *folded {
                    return Err("the centre never folds".into());
                }
                if !folded
                    && self
                        .flyout
                        .as_ref()
                        .is_some_and(|p| self.layout.area_of(p) == Some(*area))
                {
                    self.flyout = None;
                }
                self.layout.folded.retain(|a| a != area);
                if *folded {
                    self.layout.folded.push(*area);
                }
                json!({ "area": area_name(*area), "folded": folded })
            }
            Change::Flyout(p, open) => {
                let spec = spec_of(p)?;
                if !*open {
                    if self.flyout.as_ref() == Some(p) {
                        self.flyout = None;
                    }
                    return Ok(json!({ "panel": p, "open": false }));
                }
                let area = self
                    .layout
                    .area_of(p)
                    .ok_or_else(|| format!("{} is not in an area", spec.title))?;
                if !matches!(area, Area::StartSide | Area::EndSide)
                    || !self.layout.folded.contains(&area)
                {
                    return Err(format!(
                        "{} is not in a folded side area: only those open as a flyout",
                        spec.title
                    ));
                }
                // The session's state: the layout is left as it is.
                self.flyout = Some(p.clone());
                return Ok(json!({ "panel": p, "open": true }));
            }
            Change::FlyoutSize(p, width) => {
                spec_of(p)?;
                self.layout.flyouts.insert(p.clone(), *width);
                return Ok(json!({ "panel": p, "width": width }));
            }
            Change::Zen(on) => {
                // The session's state: the layout is left as it is.
                self.zen = *on;
                return Ok(json!({ "zen": on }));
            }
            Change::Place(area, placement) => {
                if !matches!(area, Area::StartSide | Area::EndSide) {
                    return Err(format!(
                        "only a side area may lie over the centre, not {}",
                        area.name()
                    ));
                }
                self.layout.set_placement(*area, *placement);
                json!({ "area": area_name(*area), "placement": placement.as_str() })
            }
        };
        self.stale = true;
        Ok(out)
    }

    pub fn take_dock(&mut self, area: Area) -> Option<DockState<PanelId>> {
        let d = self.docks.remove(&area);
        if d.is_some() {
            self.taken += 1;
        }
        d
    }

    pub fn put_dock(&mut self, area: Area, dock: DockState<PanelId>) {
        self.taken = self.taken.saturating_sub(1);
        if !self.stale {
            self.docks.insert(area, dock);
        }
    }

    /// The docked areas, in drawing order.
    pub fn areas(&self) -> Vec<Area> {
        self.docks.keys().copied().collect()
    }

    /// The panels an area holds now.
    pub fn panels_in(&self, area: Area) -> Vec<PanelId> {
        self.docks
            .get(&area)
            .map(|d| d.iter_all_tabs().map(|(_, t)| t.clone()).collect())
            .unwrap_or_default()
    }

    /// The tab egui is dragging, if it is one of the docks': its panel
    /// and area.
    pub fn dragged_tab(&self, ctx: &egui::Context) -> Option<(PanelId, Area)> {
        let dragged = ctx.dragged_id()?;
        self.docks.iter().find_map(|(area, dock)| {
            let base = dock_id(*area);
            dock.iter_all_tabs().find_map(|(path, tab)| {
                // How egui_dock names a tab's drag (`show/leaf.rs`).
                let id = base
                    .with((path.surface, "surface"))
                    .with((path.node, "node"))
                    .with((path.tab.0, "tab"));
                (id == dragged).then(|| (tab.clone(), *area))
            })
        })
    }

    /// The area a drop at `at` goes to: the one drawn there, else a
    /// closed side or bottom area along the areas' edges.
    pub fn area_at(&self, at: Pos2) -> Option<Area> {
        let shown = |a: Area| self.docks.contains_key(&a);
        let c = self.content;
        if c.is_positive() {
            if !shown(Area::StartSide) && at.x < c.left() + EDGE {
                return Some(Area::StartSide);
            }
            if !shown(Area::EndSide) && at.x > c.right() - EDGE {
                return Some(Area::EndSide);
            }
            if !shown(Area::Bottom) && at.y > c.bottom() - EDGE {
                return Some(Area::Bottom);
            }
        }
        self.rects
            .iter()
            .filter(|(a, _)| **a != Area::Center)
            .find(|(_, r)| r.contains(at))
            .map(|(a, _)| *a)
            .or_else(|| {
                self.rects
                    .get(&Area::Center)
                    .filter(|r| r.contains(at))
                    .map(|_| Area::Center)
            })
    }

    /// The area a palette dropped at `at` docks in: a side or bottom area
    /// drawn there, or a closed one when the pointer is within `edge` of
    /// the areas' edges (narrower than for tabs: a palette snapped to an
    /// edge must not dock); `None` over the centre (it floats there).
    pub fn dock_at(&self, at: Pos2, edge: f32) -> Option<Area> {
        let shown = |a: Area| self.docks.contains_key(&a);
        let c = self.content;
        if c.is_positive() {
            if !shown(Area::StartSide) && at.x < c.left() + edge {
                return Some(Area::StartSide);
            }
            if !shown(Area::EndSide) && at.x > c.right() - edge {
                return Some(Area::EndSide);
            }
            if !shown(Area::Bottom) && at.y > c.bottom() - edge {
                return Some(Area::Bottom);
            }
        }
        self.rects
            .iter()
            .filter(|(a, _)| **a != Area::Center)
            .find(|(_, r)| r.contains(at))
            .map(|(a, _)| *a)
    }

    /// Where a drop hint for `area` is drawn.
    pub fn hint_rect(&self, area: Area) -> Rect {
        let c = self.content;
        self.rects.get(&area).copied().unwrap_or(match area {
            Area::StartSide => Rect::from_min_max(c.min, egui::pos2(c.left() + EDGE, c.bottom())),
            Area::EndSide => Rect::from_min_max(egui::pos2(c.right() - EDGE, c.top()), c.max),
            Area::Bottom => Rect::from_min_max(egui::pos2(c.left(), c.bottom() - EDGE), c.max),
            Area::Center => c,
        })
    }
}

/// Draws the detached panels, each in a window of its own (with its own
/// scale, and the application's frame when it draws one). Closing such a
/// window is the `workbench.close_window` step, whose default docks the
/// panel back.
///
/// Each window is a window of the application in full: `keys` dispatches
/// its keys in its own context (its focus chain, `window` naming its
/// panel), the `WINDOW_OVERLAYS` zone draws the dialogs opened from it
/// over it, and a dialog open in another window blocks it
/// (`overlay::block_window`).
pub(crate) fn windows(
    app: &AppHandle,
    ctx: &egui::Context,
    app_name: &str,
    keys: &mut dyn FnMut(&egui::Context),
) {
    let auxiliary = app.workbench_mut().layout.auxiliary.clone();
    for aux in auxiliary {
        let Some(spec) = app.workbench_mut().spec(&aux.panel).cloned() else {
            continue;
        };
        let [x, y, w, h] = aux.rect;
        let state = WindowState {
            position: Some(egui::pos2(x, y)),
            size: egui::vec2(w, h),
            maximized: false,
            fullscreen: false,
        }
        .repair(&[]);
        let own = crate::frame::own_frame(app);
        let panel_title = strings::panel_title(ctx, spec.id.as_str(), &spec.title);
        let title = if app_name.is_empty() {
            panel_title
        } else {
            strings::tr_args(
                ctx,
                &txt::WORKBENCH_WINDOW_TITLE,
                &[("panel", &panel_title), ("application", app_name)],
            )
        };
        let mut builder = ViewportBuilder::default()
            .with_title(title.clone())
            .with_inner_size(state.size)
            .with_decorations(!own);
        if let Some(p) = state.position {
            builder = builder.with_position(p);
        }
        let panel = aux.panel.clone();
        ctx.show_viewport_immediate(window_id(&panel), builder, |ui, class| {
            let vctx = ui.ctx().clone();
            // Embedded in the main window (a backend without windows of
            // its own, kittest): the main window's pass does all this.
            let own_window = class != egui::ViewportClass::EmbeddedWindow;
            if own_window {
                app.note_window(&vctx);
                egui_pop_core::overlay::block_window(&vctx);
                egui_pop_core::overlay::begin_pass(&vctx);
                keys(&vctx);
            }
            if let Some((from, to)) = egui_pop_core::scale::changed(&vctx) {
                app.publish(
                    "egui_pop",
                    None,
                    "scale.changed",
                    json!({ "viewport": panel, "from": from, "to": to }),
                );
            }
            // In full screen, the title bar as the main window's: kept
            // unless the setting says otherwise.
            let full_screen = egui_pop_core::frame::full_screen(&vctx);
            if own && (!full_screen || crate::frame::bar_in_fullscreen(app)) {
                egui_pop_core::frame::resize_edges(&vctx);
                let out = egui::Panel::top(Id::new(("egui_pop.aux.title", panel.as_str())))
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        egui_pop_core::frame::TitleBar::new(&title)
                            .button(egui_pop_core::frame::Button::Minimize, false)
                            .show(ui, |_| {}, |_| {})
                    })
                    .inner;
                match out.request {
                    Some(egui_pop_core::frame::FrameRequest::Close) => {
                        app.change_workbench(Change::CloseWindow(panel.clone()));
                    }
                    Some(r) => crate::frame::request(app, &vctx, r),
                    None => {}
                }
            }
            egui::CentralPanel::default().show(ui, |ui| {
                app.show_panel(ui, &panel);
            });
            if own_window {
                app.show_zone(ui, &crate::zones::WINDOW_OVERLAYS);
            }
            if vctx.input(|i| i.viewport().close_requested()) {
                // The step decides: the default docks the panel back.
                let out = app.change_workbench(Change::CloseWindow(panel.clone()));
                if matches!(out, Some(Outcome::Vetoed { .. })) {
                    vctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
            }
            // Its place, kept in the layout.
            let info = vctx.input(|i| i.viewport().clone());
            if let (Some(outer), Some(inner)) = (info.outer_rect, info.inner_rect) {
                let rect = [outer.min.x, outer.min.y, inner.width(), inner.height()];
                let mut wb = app.workbench_mut();
                if let Some(a) = wb.layout.auxiliary.iter_mut().find(|a| a.panel == panel)
                    && a.rect != rect
                {
                    a.rect = rect;
                }
            }
        });
    }
}

/// Forgets the overlays of the windows closed since the last pass: what
/// they showed is gone with them (a dialog shown there comes to the main
/// window).
pub(crate) fn forget_closed_windows(app: &AppHandle, ctx: &egui::Context) {
    let open: Vec<ViewportId> = app
        .workbench_mut()
        .layout
        .auxiliary
        .iter()
        .map(|a| window_id(&a.panel))
        .collect();
    egui_pop_core::overlay::retain_windows(ctx, &open);
}

/// A detached panel's window id.
pub fn window_id(panel: &PanelId) -> ViewportId {
    ViewportId::from_hash_of(("egui_pop.aux", panel.as_str()))
}

/// An area's dock id.
pub(crate) fn dock_id(area: Area) -> Id {
    Id::new(("egui_pop.area", area_name(area)))
}
/// An area as the interface names it ("the start side bar").
fn area_text(ctx: &egui::Context, area: Area) -> String {
    strings::tr(
        ctx,
        match area {
            Area::Center => &txt::AREA_CENTER,
            Area::StartSide => &txt::AREA_START_SIDE,
            Area::EndSide => &txt::AREA_END_SIDE,
            Area::Bottom => &txt::AREA_BOTTOM,
        },
    )
}

/// Why a panel may not go to an area, as the interface says it.
fn refusal(ctx: &egui::Context, spec: &PanelSpec, title: &str, area: Area) -> Option<String> {
    (!spec.allows(area)).then(|| {
        strings::tr_args(
            ctx,
            &txt::WORKBENCH_STAYS_OUT,
            &[("panel", title), ("area", &area_text(ctx, area))],
        )
    })
}

/// An area's tree of tabs and splits as an `egui_dock` state. The
/// layout document stays the saved form; the dock is how it is drawn
/// and moved by hand.
pub(crate) fn to_dock(node: &Node) -> DockState<PanelId> {
    let mut dock = DockState::new(leading_tabs(node));
    fill(dock.main_surface_mut(), NodeIndex::root(), node);
    dock
}

/// The tabs a node shows first (its own, or its first child's).
fn leading_tabs(node: &Node) -> Vec<PanelId> {
    match node {
        Node::Tabs { panels, .. } => panels.clone(),
        Node::Split { a, .. } => leading_tabs(a),
    }
}

fn fill(tree: &mut egui_dock::Tree<PanelId>, at: NodeIndex, node: &Node) {
    match node {
        Node::Tabs { panels, active } => {
            if let egui_dock::Node::Leaf(leaf) = &mut tree[at] {
                leaf.tabs.clone_from(panels);
                leaf.active = egui_dock::TabIndex((*active).min(panels.len().saturating_sub(1)));
            }
        }
        Node::Split {
            vertical,
            ratio,
            a,
            b,
        } => {
            let side = if *vertical {
                Split::Below
            } else {
                Split::Right
            };
            let [first, second] = tree.split(
                at,
                side,
                ratio.clamp(0.05, 0.95),
                egui_dock::Node::leaf_with(leading_tabs(b)),
            );
            fill(tree, first, a);
            fill(tree, second, b);
        }
    }
}

/// The tree a dock shows now; `None` when it holds no panel.
pub(crate) fn from_dock(dock: &DockState<PanelId>) -> Option<Node> {
    read(dock.main_surface(), NodeIndex::root())
}

fn read(tree: &egui_dock::Tree<PanelId>, at: NodeIndex) -> Option<Node> {
    if at.0 >= tree.len() {
        return None;
    }
    match &tree[at] {
        egui_dock::Node::Empty => None,
        egui_dock::Node::Leaf(leaf) => (!leaf.tabs.is_empty()).then(|| Node::Tabs {
            panels: leaf.tabs.clone(),
            active: leaf.active.0.min(leaf.tabs.len() - 1),
        }),
        egui_dock::Node::Vertical(s) | egui_dock::Node::Horizontal(s) => {
            let vertical = matches!(tree[at], egui_dock::Node::Vertical(_));
            match (read(tree, at.left()), read(tree, at.right())) {
                (Some(a), Some(b)) => Some(Node::Split {
                    vertical,
                    ratio: s.fraction,
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (Some(n), None) | (None, Some(n)) => Some(n),
                (None, None) => None,
            }
        }
    }
}

/// Draws the docked areas in `ui` (side and bottom areas, the centre
/// takes what is left), then follows a tab dragged to another area.
pub(crate) fn show(app: &AppHandle, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let (areas, sizes) = {
        let mut wb = app.workbench_mut();
        wb.project();
        wb.content = ui.max_rect();
        wb.rects.clear();
        wb.bars.clear();
        let sizes: BTreeMap<Area, (f32, f32)> = Area::ALL
            .into_iter()
            .map(|a| {
                let specs: Vec<&PanelSpec> =
                    wb.panels_in(a).iter().filter_map(|p| wb.spec(p)).collect();
                let min = specs.iter().map(|s| s.min_size).fold(0.0, f32::max);
                let default = specs
                    .iter()
                    .filter_map(|s| s.default_size)
                    .fold(0.0, f32::max);
                (a, (min, default))
            })
            .collect();
        (wb.areas(), sizes)
    };
    let zen = app.workbench_mut().zen;
    app.set_drawing_areas(true);
    if !zen && app.zone_has_blocks(&crate::zones::ACTIVITY_BAR) {
        let t = egui_pop_core::theme::Tokens::of(&ctx);
        egui::Panel::left(Id::new("egui_pop.activity_bar"))
            .resizable(false)
            .exact_size(t.size.icon_panel + 2.0 * t.space.s + 2.0 * t.space.xs)
            .show(ui, |ui| {
                egui_pop_core::focus::scope(ui, "activityBar", &[], |ui| {
                    app.show_zone(ui, &crate::zones::ACTIVITY_BAR);
                });
            });
    }
    let (folded, over) = {
        let wb = app.workbench_mut();
        (wb.layout.folded.clone(), wb.layout.over.clone())
    };
    let kit = Kit::of(&ctx);
    // Side areas lying over the centre, open, next to their strip; the
    // strips of folded side areas, where a flyout may open.
    let mut lying = Vec::new();
    let mut strips = Vec::new();
    for area in panel_alignment(app).order() {
        // Zen mode: the centre alone.
        if zen || !areas.contains(&area) || !visible_in(app, area) {
            continue;
        }
        let side = matches!(area, Area::StartSide | Area::EndSide);
        let is_over = side && over.contains(&area);
        if folded.contains(&area) || is_over {
            if side {
                let open = is_over && !folded.contains(&area);
                let rect = draw_strip(app, ui, &kit, area, open);
                if open {
                    lying.push((area, rect));
                } else {
                    strips.push((area, rect));
                }
            }
            continue;
        }
        let (min, default) = sizes[&area];
        let id = dock_id(area);
        let fallback: f32 = match area {
            Area::Bottom => 200.0,
            _ => 240.0,
        };
        let default = if default > 0.0 {
            default
        } else {
            fallback.max(min)
        };
        let panel = match area {
            Area::StartSide => egui::Panel::left(id),
            Area::EndSide => egui::Panel::right(id),
            _ => egui::Panel::bottom(id),
        };
        let pad = |p: f32| p.round().clamp(0.0, 127.0) as i8;
        let side = &kit.side_area.data;
        let shown =
            panel
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(
                    egui::Margin::symmetric(pad(side.padding_x), pad(side.padding_y)),
                ))
                .show_separator_line(kit.side_area.style(State::REST).border_width > 0.0)
                .resizable(true)
                .default_size(default)
                .min_size(min.max(48.0))
                .show(ui, |ui| {
                    // The panel keeps its width; content narrower than it does
                    // not shrink it.
                    ui.set_min_size(ui.available_size());
                    draw_area(app, ui, area);
                });
        app.workbench_mut().rects.insert(area, shown.response.rect);
        grip(&ctx, &kit, area, shown.response.rect);
    }
    let mut central = egui::CentralPanel::default();
    if app.edge_to_edge() {
        central = central.frame(egui::Frame::central_panel(ui.style()).inner_margin(0.0));
    }
    let shown = central.show(ui, |ui| {
        if areas.contains(&Area::Center) && visible_in(app, Area::Center) {
            draw_area(app, ui, Area::Center);
        }
    });
    app.workbench_mut()
        .rects
        .insert(Area::Center, shown.response.rect);
    for (area, strip) in lying {
        lie_over(app, &ctx, &kit, area, strip, sizes[&area]);
    }
    // The flyout, while its panel is still in a folded side area drawn
    // with its strip; otherwise it is gone.
    let flyout = app.workbench_mut().flyout.clone();
    let mut drawn = None;
    if let Some(panel) = flyout {
        let area = app.workbench_mut().layout.area_of(&panel);
        if let Some((area, strip)) = strips.iter().find(|(a, _)| Some(*a) == area) {
            drawn = show_flyout(app, &ctx, &kit, *area, *strip, &panel);
        }
        if drawn.is_none() {
            let mut wb = app.workbench_mut();
            if wb.flyout.as_ref() == Some(&panel) {
                wb.flyout = None;
            }
        }
    }
    app.workbench_mut().flyout_rect = drawn;
    app.set_drawing_areas(false);
    if !zen {
        crate::palettes::show(app, &ctx);
    }
    follow_drag(app, &ctx);
    drop_hint(app, &ctx);
}

/// A folded side area's strip (the kit's `area-strip`): its box, the line
/// on the centre's side, the drag-handle glyph; a click unfolds the area
/// through the `workbench.fold` step. A side area lying over the centre
/// keeps its strip while `open` (`selected` then), and a click folds it.
/// Below the glyph, an icon per panel ([`strip_icon`]), the strip
/// widened to hold them. Returns where the strip was drawn.
fn draw_strip(app: &AppHandle, ui: &mut Ui, kit: &Kit, area: Area, open: bool) -> Rect {
    let component = &kit.area_strip;
    let grip = component.data.width.max(1.0);
    let panels = strip_panels(app, area);
    let t = egui_pop_core::theme::Tokens::of(ui.ctx());
    let cell = t.size.icon_inline + 2.0 * t.space.s;
    let width = if panels.is_empty() {
        grip
    } else {
        grip.max(cell)
    };
    let id = dock_id(area).with("egui_pop.strip");
    let (panel, inner) = match area {
        Area::StartSide => (egui::Panel::left(id), Side::Right),
        _ => (egui::Panel::right(id), Side::Left),
    };
    panel
        .resizable(false)
        .exact_size(width)
        .show_separator_line(false)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let ctx = ui.ctx().clone();
            let name = strings::tr_args(
                &ctx,
                if open {
                    &txt::WORKBENCH_FOLD
                } else {
                    &txt::WORKBENCH_UNFOLD
                },
                &[("area", &area_text(&ctx, area))],
            );
            let r = ui.interact(rect, id.with("unfold"), egui::Sense::click());
            let label = name.clone();
            r.widget_info(move || WidgetInfo::labeled(WidgetType::Button, true, &label));
            let r = r.on_hover_text(name);
            let state = State::from_response(&r).with(State::SELECTED, open);
            let b = component.style(state);
            let cx = KitCx {
                painter: ui.painter(),
                ppp: ctx.pixels_per_point(),
            };
            paint_box(
                &cx,
                rect,
                &BoxStyle {
                    border_width: 0.0,
                    ..*b
                },
                Corners::None,
                inner,
            );
            let line = BoxStyle {
                indicator_color: b.border_color,
                indicator_size: b.border_width,
                ..BoxStyle::default()
            };
            paint_indicator(&cx, rect, &line, inner);
            let at = Rect::from_center_size(
                egui::pos2(rect.center().x, rect.top() + grip * 1.25),
                egui::vec2(grip, grip),
            );
            kit.glyph(Glyph::DragHandle).paint(ui.painter(), at, b.text);
            if r.clicked() {
                app.fold_area(area, open);
            }
            // The panels' icons, under the glyph, in tab order.
            let shown: Vec<PanelId> = {
                let wb = app.workbench_mut();
                wb.layout.active_in(area).into_iter().cloned().collect()
            };
            let mut top = rect.top() + grip * 2.5;
            for p in &panels {
                let cell_rect = Rect::from_min_size(
                    egui::pos2(rect.left(), top),
                    egui::vec2(rect.width(), cell),
                );
                if cell_rect.bottom() > rect.bottom() {
                    break;
                }
                strip_icon(
                    app,
                    ui,
                    kit,
                    area,
                    p,
                    cell_rect,
                    open,
                    open && shown.contains(p),
                );
                top += cell + t.space.xs;
            }
            rect
        })
        .inner
}

/// A panel's icon in a side area's strip, as agents read it
/// ([`strip`], the `workbench` collector's `strips`).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct StripIcon {
    /// The area whose strip it is in (`area_name`).
    pub area: &'static str,
    pub panel: PanelId,
    pub title: String,
    /// Its icon's name; `None`: its title's first letter is drawn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Its key (its focus command's, else its toggle command's).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The panel shows: in its area (open over the centre, the tab in
    /// front) or as the flyout.
    pub shown: bool,
    /// It is the flyout.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub flyout: bool,
    /// Its indicator's state, when a panel indicator speaks for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicator: Option<crate::strips::IndicatorState>,
    /// Whether the icon shows the indicator's mark now (the panel not
    /// shown, the state saying something).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub marked: bool,
}

/// Every icon of the side areas' strips (an area folded, or lying over
/// the centre), start side first, each in tab order, with its panel's
/// indicator. Empty in zen mode.
pub fn strip(app: &AppHandle) -> Vec<StripIcon> {
    let (layout, zen, flyout) = {
        let mut wb = app.workbench_mut();
        (wb.layout(), wb.zen, wb.flyout.clone())
    };
    if zen {
        return Vec::new();
    }
    let ctx = app.egui();
    let mut out = Vec::new();
    for area in [Area::StartSide, Area::EndSide] {
        let folded = layout.folded.contains(&area);
        if !folded && !layout.over.contains(&area) {
            continue;
        }
        let active = layout.active_in(area);
        for panel in strip_panels(app, area) {
            let flown = flyout.as_ref() == Some(&panel);
            let shown = flown || (!folded && active.contains(&&panel));
            let indicator = panel_indicator(app, &panel);
            let marked = !shown
                && indicator
                    .as_ref()
                    .is_some_and(|(i, s)| panel_mark(i, s).is_some());
            let icon = app
                .workbench_mut()
                .spec(&panel)
                .map(|s| s.icon.clone())
                .filter(|i| egui_pop_core::icon::Icon::by_name(i).is_some());
            out.push(StripIcon {
                area: area_name(area),
                title: app.panel_title(&ctx, &panel),
                icon,
                key: panel_key(app, &panel),
                shown,
                flyout: flown,
                indicator: indicator.map(|(_, s)| s),
                marked,
                panel,
            });
        }
    }
    out
}

/// The panels a side area's strip shows icons for: those it holds that
/// have something to draw, in tab order.
fn strip_panels(app: &AppHandle, area: Area) -> Vec<PanelId> {
    let panels = app.workbench_mut().panels_in(area);
    panels.into_iter().filter(|p| drawable(app, p)).collect()
}

/// A panel's key: the first binding of its focus command, else of its
/// toggle command.
fn panel_key(app: &AppHandle, panel: &PanelId) -> Option<String> {
    ["focus", "toggle"].into_iter().find_map(|suffix| {
        let id = egui_pop_model::ids::CommandId::new(format!("{panel}.{suffix}")).ok()?;
        app.keys_for(&id).into_iter().next()
    })
}

/// The indicator speaking for a panel ([`crate::strips::Indicator::panel`])
/// and its state.
fn panel_indicator(
    app: &AppHandle,
    panel: &PanelId,
) -> Option<(crate::strips::Indicator, crate::strips::IndicatorState)> {
    app.indicators()
        .into_iter()
        .find(|(i, _)| i.panel.as_ref() == Some(panel))
}

/// The mark a panel's indicator state shows on its strip icon: a count
/// as a badge (in the accent tone unless the state has one), else its
/// errors or warnings, else a dot in its tone; nothing in the neutral
/// tone with nothing to count.
fn panel_mark(
    ind: &crate::strips::Indicator,
    state: &crate::strips::IndicatorState,
) -> Option<egui_pop_core::indicator::Indicator> {
    use egui_pop_core::indicator::Indicator as Mark;
    use egui_pop_core::kit::Tone;
    let tone = state.tone;
    let mark = if let Some(n) = state.count.filter(|n| *n > 0) {
        Mark::count(n).tone(if tone == Tone::Neutral {
            Tone::Accent
        } else {
            tone
        })
    } else if state.errors > 0 {
        Mark::count(state.errors).tone(Tone::Danger)
    } else if state.warnings > 0 {
        Mark::count(state.warnings).tone(Tone::Warning)
    } else if tone != Tone::Neutral {
        Mark::dot(tone)
    } else {
        return None;
    };
    let name = if state.text.is_empty() {
        ind.label.clone()
    } else {
        state.text.clone()
    };
    Some(mark.name(name).id(format!("strip:{}", ind.id)))
}

/// One panel's icon in a side area's strip, drawn by the kit's
/// `activity-bar-item` (`selected` while its panel shows, in the area
/// or as the flyout): its [`PanelSpec::icon`], else its title's first
/// letter; its indicator's mark while the panel is not shown. A click
/// opens or closes its flyout (folded) or shows it (`open`); a double
/// click unfolds the area with it shown.
#[allow(clippy::too_many_arguments)]
fn strip_icon(
    app: &AppHandle,
    ui: &mut Ui,
    kit: &Kit,
    area: Area,
    panel: &PanelId,
    rect: Rect,
    open: bool,
    shown: bool,
) {
    let ctx = ui.ctx().clone();
    let t = egui_pop_core::theme::Tokens::of(&ctx);
    let title = app.panel_title(&ctx, panel);
    let icon = app
        .workbench_mut()
        .spec(panel)
        .and_then(|s| egui_pop_core::icon::Icon::by_name(&s.icon));
    let flown = app.workbench_mut().flyout.as_ref() == Some(panel);
    let id = dock_id(area).with(("egui_pop.strip_icon", panel.as_str()));
    let r = ui.interact(rect, id, egui::Sense::click());
    let label = title.clone();
    r.widget_info(move || WidgetInfo::labeled(WidgetType::Button, true, &label));
    let indicator = panel_indicator(app, panel);
    let mut tip = title.clone();
    if let Some(key) = panel_key(app, panel) {
        tip.push('\n');
        tip.push_str(&key);
    }
    let visible = shown || flown;
    let mark = indicator
        .as_ref()
        .filter(|_| !visible)
        .and_then(|(i, s)| panel_mark(i, s).map(|m| (m, s.detail.clone())));
    if let Some((_, detail)) = &mark
        && !detail.is_empty()
    {
        tip.push('\n');
        tip.push_str(detail);
    }
    let state = State::REST
        .with(State::HOVER, r.hovered() && !visible)
        .with(State::ACTIVE, r.is_pointer_button_down_on())
        .with(State::FOCUS, r.has_focus())
        .with(State::SELECTED, visible);
    let item = &kit.activity_bar_item;
    let cx = KitCx {
        painter: ui.painter(),
        ppp: ctx.pixels_per_point(),
    };
    item.paint(
        &cx,
        &Part {
            rect,
            room: rect,
            state,
            label: None,
        },
    );
    let color = item.style(state).text;
    let inner = Rect::from_center_size(
        rect.center(),
        egui::vec2(t.size.icon_inline, t.size.icon_inline),
    );
    match icon {
        Some(i) => egui_pop_core::icon::Source::from(i).paint(ui, inner, color),
        None => {
            let letter: String = title
                .chars()
                .next()
                .map(|c| c.to_uppercase().collect())
                .unwrap_or_else(|| "?".into());
            ui.painter().text(
                inner.center(),
                egui::Align2::CENTER_CENTER,
                letter,
                egui::FontId::proportional(t.size.icon_inline),
                color,
            );
        }
    }
    if let Some((m, _)) = mark {
        m.on(ui, &r, egui_pop_core::indicator::Anchor::TopEnd);
    }
    let r = r.on_hover_text(tip);
    if r.double_clicked() {
        // Unfolds the area with the panel shown.
        app.change_workbench(Change::Flyout(panel.clone(), false));
        app.focus_panel(panel);
    } else if r.clicked() {
        if open {
            app.focus_panel(panel);
        } else {
            app.change_workbench(Change::Flyout(panel.clone(), !flown));
        }
    }
}

/// A folded side area's panel shown alone over the centre, next to the
/// strip (as an area lying over it): a band with its title and a pin
/// button unfolding the area, then the panel. An overlay of the stack
/// (`overlay::hold`), so Escape or a click away closes it; its edge on
/// the centre's side resizes it, the width kept per panel in the layout
/// (`workbench.flyout_size`). Returns where it was drawn, if it is
/// still open.
fn show_flyout(
    app: &AppHandle,
    ctx: &egui::Context,
    kit: &Kit,
    area: Area,
    strip: Rect,
    panel: &PanelId,
) -> Option<Rect> {
    let spec = app.workbench_mut().spec(panel).cloned()?;
    let title = app.panel_title(ctx, panel);
    let id = dock_id(area).with(("egui_pop.flyout", panel.as_str()));
    let live_id = id.with("live");
    let content = app.workbench_mut().content;
    let min = spec.min_size.max(48.0);
    let max = (content.width() - 2.0 * strip.width()).max(min);
    let saved = app.workbench_mut().layout.flyouts.get(panel).copied();
    let width = ctx
        .data(|d| d.get_temp::<f32>(live_id))
        .or(saved)
        .or(spec.default_size)
        .or_else(|| {
            egui::containers::panel::PanelState::load(ctx, dock_id(area)).map(|s| s.size().x)
        })
        .unwrap_or(240.0)
        .clamp(min, max);
    let start = area == Area::StartSide;
    let rect = if start {
        Rect::from_min_size(
            egui::pos2(strip.right(), strip.top()),
            egui::vec2(width, strip.height()),
        )
    } else {
        Rect::from_min_size(
            egui::pos2(strip.left() - width, strip.top()),
            egui::vec2(width, strip.height()),
        )
    };
    // In the overlay stack: its rect takes in the resize edge, the strip
    // is its anchor (a click on another icon switches the flyout).
    let reach = ctx.global_style().interaction.resize_grab_radius_side;
    let spec_overlay = egui_pop_core::overlay::OverlaySpec::new(
        ("egui_pop.flyout", panel.as_str()),
        &title,
        egui_pop_core::overlay::Kind::Popover,
    );
    let mut open = true;
    if !egui_pop_core::overlay::hold(
        ctx,
        &spec_overlay,
        &mut open,
        Some(strip),
        rect.expand2(egui::vec2(reach, 0.0)),
    ) {
        app.change_workbench(Change::Flyout(panel.clone(), false));
        return None;
    }
    let side = &kit.side_area;
    let rest = *side.style(State::REST);
    let mut shadow = side.data.shadow;
    if !start {
        shadow.offset[0] = shadow.offset[0].saturating_neg();
    }
    let pad = egui::vec2(side.data.padding_x, side.data.padding_y);
    let t = egui_pop_core::theme::Tokens::of(ctx);
    egui::Area::new(id)
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(rect.size(), egui::Sense::hover());
            let painter = ui.painter();
            if shadow != egui::Shadow::NONE {
                painter.add(shadow.as_shape(rect, egui::CornerRadius::ZERO));
            }
            painter.rect_filled(rect, 0.0, ui.visuals().panel_fill);
            let body = rect.shrink2(pad);
            let mut inner = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("egui_pop.flyout.inner")
                    .max_rect(body),
            );
            inner.set_clip_rect(body);
            inner.set_min_size(inner.available_size());
            // The band: the title, then the pin button at the end.
            let pin = strings::tr_args(
                inner.ctx(),
                &txt::WORKBENCH_FLYOUT_PIN,
                &[("panel", &title)],
            );
            let button = t.size.control_height.max(24.0);
            let header = kit.header_band.style(State::REST).text;
            let pinned = inner
                .horizontal(|ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(&title).strong().color(header))
                            .truncate()
                            .selectable(false),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui_pop_core::band::button(
                            ui,
                            button,
                            egui_pop_core::icon::Icon::Unlock,
                            &pin,
                            false,
                        )
                        .clicked()
                    })
                    .inner
                })
                .inner;
            inner.separator();
            app.show_panel(&mut inner, panel);
            if pinned {
                app.change_workbench(Change::Flyout(panel.clone(), false));
                app.focus_panel(panel);
            }
            // The line on the centre's side, as a side panel's.
            let x = if start { rect.right() } else { rect.left() };
            if rest.border_width > 0.0 {
                ui.painter().vline(
                    x,
                    rect.y_range(),
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
            }
            // Resized from that edge; the width is saved once let go.
            let edge = Rect::from_x_y_ranges(x - reach..=x + reach, rect.y_range());
            let r = ui.interact(edge, id.with("resize"), egui::Sense::drag());
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if r.dragged() {
                let dx = r.drag_delta().x;
                let w = (width + if start { dx } else { -dx }).clamp(min, max);
                ui.ctx().data_mut(|d| d.insert_temp(live_id, w));
            }
            if r.drag_stopped() {
                ui.ctx().data_mut(|d| d.remove::<f32>(live_id));
                app.change_workbench(Change::FlyoutSize(panel.clone(), width));
            }
        });
    Some(rect)
}

/// A side area lying over the centre, open: next to its strip (`strip`),
/// as tall, above the centre (an egui area on the middle layer, as
/// palettes), in the kit's `side-area` box with its shadow cast towards
/// the centre. Its edge on the centre's side resizes it; its width is
/// kept per area (from the side panel's when it lay beside).
fn lie_over(
    app: &AppHandle,
    ctx: &egui::Context,
    kit: &Kit,
    area: Area,
    strip: Rect,
    (min, default): (f32, f32),
) {
    let id = dock_id(area).with("egui_pop.over");
    let width_id = id.with("width");
    let content = app.workbench_mut().content;
    let min = min.max(48.0);
    let max = (content.width() - 2.0 * strip.width()).max(min);
    let width = ctx
        .data_mut(|d| d.get_persisted::<f32>(width_id))
        .or_else(|| {
            egui::containers::panel::PanelState::load(ctx, dock_id(area)).map(|s| s.size().x)
        })
        .unwrap_or(if default > 0.0 { default } else { 240.0 })
        .clamp(min, max);
    let start = area == Area::StartSide;
    let rect = if start {
        Rect::from_min_size(
            egui::pos2(strip.right(), strip.top()),
            egui::vec2(width, strip.height()),
        )
    } else {
        Rect::from_min_size(
            egui::pos2(strip.left() - width, strip.top()),
            egui::vec2(width, strip.height()),
        )
    };
    let side = &kit.side_area;
    let rest = *side.style(State::REST);
    let mut shadow = side.data.shadow;
    if !start {
        shadow.offset[0] = shadow.offset[0].saturating_neg();
    }
    let pad = egui::vec2(side.data.padding_x, side.data.padding_y);
    egui::Area::new(id)
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(rect.size(), egui::Sense::hover());
            let painter = ui.painter();
            if shadow != egui::Shadow::NONE {
                painter.add(shadow.as_shape(rect, egui::CornerRadius::ZERO));
            }
            painter.rect_filled(rect, 0.0, ui.visuals().panel_fill);
            let mut inner = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("egui_pop.over.inner")
                    .max_rect(rect.shrink2(pad)),
            );
            inner.set_clip_rect(rect.shrink2(pad));
            inner.set_min_size(inner.available_size());
            draw_area(app, &mut inner, area);
            // The line on the centre's side, as a side panel's.
            let x = if start { rect.right() } else { rect.left() };
            if rest.border_width > 0.0 {
                ui.painter().vline(
                    x,
                    rect.y_range(),
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
            }
            // Resized from that edge.
            let reach = ui.style().interaction.resize_grab_radius_side;
            let edge = Rect::from_x_y_ranges(x - reach..=x + reach, rect.y_range());
            let r = ui.interact(edge, id.with("resize"), egui::Sense::drag());
            if r.hovered() || r.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if r.dragged() {
                let dx = r.drag_delta().x;
                let w = (width + if start { dx } else { -dx }).clamp(min, max);
                ui.ctx().data_mut(|d| d.insert_persisted(width_id, w));
            }
        });
    app.workbench_mut().rects.insert(area, rect);
    grip(ctx, kit, area, rect);
}

/// A side area's resize grip (the kit's `area-grip`) at the middle of its
/// edge, above the panels; egui takes the drag. Nothing is drawn while
/// the kit's grip is transparent in every state (the default kit).
fn grip(ctx: &egui::Context, kit: &Kit, area: Area, rect: Rect) {
    let g = &kit.area_grip;
    if [State::REST, State::HOVER, State::ACTIVE]
        .iter()
        .all(|s| g.style(*s).fill.a() == 0)
    {
        return;
    }
    let (w, h, off) = (g.data.width, g.data.height, g.data.offset);
    let (centre, size) = match area {
        Area::StartSide => (
            egui::pos2(rect.right() + off, rect.center().y),
            egui::vec2(w, h),
        ),
        Area::EndSide => (
            egui::pos2(rect.left() - off, rect.center().y),
            egui::vec2(w, h),
        ),
        _ => (
            egui::pos2(rect.center().x, rect.top() - off),
            egui::vec2(h, w),
        ),
    };
    let reach = ctx.global_style().interaction.resize_grab_radius_side;
    let near = ctx.pointer_hover_pos().is_some_and(|p| match area {
        Area::StartSide | Area::EndSide => {
            (p.x - centre.x).abs() <= reach && rect.y_range().contains(p.y)
        }
        _ => (p.y - centre.y).abs() <= reach && rect.x_range().contains(p.x),
    });
    let state = State::REST.with(State::HOVER, near).with(
        State::ACTIVE,
        near && ctx.input(|i| i.pointer.primary_down()),
    );
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        Id::new("egui_pop.area_grips"),
    ));
    let rect = Rect::from_center_size(centre, size);
    g.paint(
        &KitCx {
            painter: &painter,
            ppp: ctx.pixels_per_point(),
        },
        &Part {
            rect,
            room: rect,
            state,
            label: None,
        },
    );
}

/// Whether a panel has something to draw (a zone panel's zone has
/// blocks).
fn drawable(app: &AppHandle, panel: &PanelId) -> bool {
    let zone = app.workbench_mut().zone_of.get(panel).cloned();
    zone.is_none_or(|z| app.zone_has_blocks(&z))
}

/// Whether an area has a panel to draw.
fn visible_in(app: &AppHandle, area: Area) -> bool {
    let panels = app.workbench_mut().panels_in(area);
    panels.iter().any(|p| drawable(app, p))
}

/// Draws one area: a bare panel alone plainly, else its dock.
fn draw_area(app: &AppHandle, ui: &mut Ui, area: Area) {
    let Some(mut dock) = app.workbench_mut().take_dock(area) else {
        return;
    };
    let tabs: Vec<PanelId> = dock.iter_all_tabs().map(|(_, t)| t.clone()).collect();
    let drawable_tabs: Vec<&PanelId> = tabs.iter().filter(|p| drawable(app, p)).collect();
    let bare = drawable_tabs.len() == 1
        && app
            .workbench_mut()
            .spec(drawable_tabs[0])
            .is_some_and(|s| s.bare);
    if bare {
        let id = drawable_tabs[0].clone();
        app.show_panel(ui, &id);
    } else {
        let ctx = ui.ctx().clone();
        let kit = Kit::of(&ctx);
        let cache_id = TAB_CACHE.keyed(dock_id(area));
        let mut cache = cache_id.update(&ctx, std::mem::take);
        cache.prepare(app, &ctx, &kit, &tabs);
        let style = cache.dock_style(ui.style(), &kit);
        let mut viewer = Viewer {
            app,
            prepared: &cache.tabs,
            seen: Vec::new(),
        };
        DockArea::new(&mut dock)
            .id(dock_id(area))
            .style(style)
            .show_close_buttons(false)
            .show_add_buttons(false)
            .show_leaf_close_all_buttons(false)
            .show_leaf_collapse_buttons(false)
            .show_inside(ui, &mut viewer);
        let seen = std::mem::take(&mut viewer.seen);
        paint_tabs(ui, &dock, &kit, &cache.tabs, &seen);
        cache_id.set(&ctx, cache);
        if let Some(bar) = first_bar(ui, &dock, &kit, &seen) {
            app.workbench_mut().bars.insert(area, bar);
        }
    }
    app.workbench_mut().put_dock(area, dock);
}

/// The tab bar of an area's first tabs (where `Layout::place` adds a
/// panel), as drawn this pass: `None` when hidden.
fn first_bar(ui: &Ui, dock: &DockState<PanelId>, kit: &Kit, seen: &[SeenTab]) -> Option<TabBar> {
    let tree = dock.main_surface();
    let mut at = NodeIndex::root();
    while at.0 < tree.len() && tree[at].is_parent() {
        at = at.left();
    }
    if at.0 >= tree.len() {
        return None;
    }
    let leaf = tree[at].get_leaf()?;
    if leaf.tabs.is_empty() || leaf.tab_bar_hidden {
        return None;
    }
    let height = kit.tab_bar.data.height.max(kit.tab.data.height);
    let rect = Rect::from_min_size(leaf.rect.min, egui::vec2(leaf.rect.width(), height));
    // The right edge of its last tab in the bar (a dragged tab floats on
    // a layer of its own); tabs scrolled out end at the bar's right.
    let end = seen
        .iter()
        .filter(|t| t.layer == ui.layer_id() && leaf.tabs.contains(&t.panel))
        .map(|t| t.rect.right())
        .fold(rect.left(), f32::max)
        .min(rect.right());
    Some(TabBar {
        rect,
        end,
        tabs: leaf.tabs.len(),
    })
}

struct Viewer<'a> {
    app: &'a AppHandle,
    prepared: &'a HashMap<PanelId, PreparedTab>,
    /// The tabs egui_dock laid out this pass, for the kit to paint.
    seen: Vec<SeenTab>,
}

/// A tab egui_dock laid out and took the input of: where, on which layer
/// (a dragged tab floats), and its interaction.
struct SeenTab {
    panel: PanelId,
    rect: Rect,
    layer: egui::LayerId,
    hovered: bool,
    pressed: bool,
    focused: bool,
}

/// egui_dock lays the tabs out and takes their input; the kit paints them
/// (`egui_pop_core::kit`). Its own tab painting is made invisible, and
/// each tab is as wide as the kit measures it.
fn dock_style(style: &egui::Style, kit: &Kit) -> egui_dock::Style {
    let mut s = egui_dock::Style::from_egui(style);
    let clear = Color32::TRANSPARENT;
    s.tab_bar.bg_fill = clear;
    s.tab_bar.hline_color = clear;
    s.tab_bar.height = kit.tab_bar.data.height.max(kit.tab.data.height);
    for t in [
        &mut s.tab.active,
        &mut s.tab.inactive,
        &mut s.tab.focused,
        &mut s.tab.hovered,
        &mut s.tab.active_with_kb_focus,
        &mut s.tab.inactive_with_kb_focus,
        &mut s.tab.focused_with_kb_focus,
    ] {
        t.bg_fill = clear;
        t.outline_color = clear;
        t.text_color = clear;
    }
    s
}

/// A tab ready for the kit: its title, its label laid out with no colour
/// (the kit gives it its state's) and its width as the kit measures it.
/// The label is laid out again each pass through egui's own galley cache
/// (cheap): a galley kept across passes points into the font atlas of the
/// pass it was made in, and a theme that installs other fonts rebuilds
/// that atlas, which scrambled the tab labels.
#[derive(Clone)]
struct PreparedTab {
    /// The panel's English title, as declared: translated again only when
    /// it changes.
    english: String,
    title: String,
    label: Arc<egui::Galley>,
    width: f32,
}

/// An area's prepared tabs and egui_dock style, kept across passes: a tab
/// is translated again only when its title changes, and
/// everything when the kit (a theme install, a light or dark switch) or
/// the scale changes; the style again when egui's style does. Per dock
/// (and one for a dragged tab's ghost).
const TAB_CACHE: Slot<TabCache> = Slot::opaque("api.workbench.tab_cache");

#[derive(Clone, Default)]
struct TabCache {
    /// The kit the cache was made for, kept: compared by address, which
    /// a kit dropped could otherwise lend to the next one (of another
    /// theme, or of the other variant).
    kit: Option<Arc<Kit>>,
    /// The pixels per point and the strings provider the cache was made
    /// for.
    key: Option<(u32, Option<usize>)>,
    tabs: HashMap<PanelId, PreparedTab>,
    /// egui_dock's style and the egui style it was made from (kept, as
    /// the kit is).
    style: Option<(Arc<egui::Style>, Arc<egui_dock::Style>)>,
}

impl TabCache {
    fn prepare(&mut self, app: &AppHandle, ctx: &egui::Context, kit: &Arc<Kit>, tabs: &[PanelId]) {
        let key = (ctx.pixels_per_point().to_bits(), strings::provider_id(ctx));
        if self.key != Some(key) || !self.kit.as_ref().is_some_and(|k| Arc::ptr_eq(k, kit)) {
            *self = TabCache {
                kit: Some(kit.clone()),
                key: Some(key),
                ..TabCache::default()
            };
        }
        self.tabs.retain(|p, _| tabs.contains(p));
        let font = egui::TextStyle::Button.resolve(&ctx.global_style());
        let layout = |title: &str| {
            let label = ctx.fonts_mut(|f| {
                f.layout_no_wrap(title.to_owned(), font.clone(), Color32::PLACEHOLDER)
            });
            let width = kit.tab.measure(label.size()).x;
            (label, width)
        };
        for panel in tabs {
            let english = {
                let wb = app.workbench_mut();
                let english = wb.spec(panel).map_or(panel.as_str(), |s| s.title.as_str());
                if let Some(t) = self.tabs.get_mut(panel).filter(|t| t.english == english) {
                    (t.label, t.width) = layout(&t.title);
                    continue;
                }
                english.to_string()
            };
            let title = app.panel_title(ctx, panel);
            let (label, width) = layout(&title);
            self.tabs.insert(
                panel.clone(),
                PreparedTab {
                    english,
                    title,
                    label,
                    width,
                },
            );
        }
    }

    fn dock_style(&mut self, style: &Arc<egui::Style>, kit: &Kit) -> egui_dock::Style {
        match &self.style {
            Some((from, dock)) if Arc::ptr_eq(from, style) => (**dock).clone(),
            _ => {
                let dock = dock_style(style, kit);
                self.style = Some((style.clone(), Arc::new(dock.clone())));
                dock
            }
        }
    }
}

/// Paints the tab bars and the tabs of an area with the kit, over what
/// egui_dock laid out.
fn paint_tabs(
    ui: &Ui,
    dock: &DockState<PanelId>,
    kit: &Kit,
    prepared: &HashMap<PanelId, PreparedTab>,
    seen: &[SeenTab],
) {
    let ctx = ui.ctx();
    let ppp = ctx.pixels_per_point();
    let height = kit.tab_bar.data.height.max(kit.tab.data.height);
    let mut bars: Vec<(Rect, Option<&PanelId>, &[PanelId])> = Vec::new();
    for (_, leaf) in dock.iter_leaves() {
        if leaf.tabs.is_empty() || leaf.tab_bar_hidden {
            continue;
        }
        let bar = Rect::from_min_size(leaf.rect.min, egui::vec2(leaf.rect.width(), height));
        bars.push((bar, leaf.tabs.get(leaf.active.0), &leaf.tabs));
        let cx = KitCx {
            painter: ui.painter(),
            ppp,
        };
        kit.tab_bar.paint(
            &cx,
            &Part {
                rect: bar,
                room: bar,
                state: State::REST,
                label: None,
            },
        );
    }
    for tab in seen {
        let Some((bar, active, _)) = bars.iter().find(|(_, _, tabs)| tabs.contains(&tab.panel))
        else {
            continue;
        };
        let painter = ctx.layer_painter(tab.layer);
        // In the bar, tabs scrolled out of it are cut; a dragged tab
        // floats free.
        let painter = if tab.layer == ui.layer_id() {
            painter.with_clip_rect(*bar)
        } else {
            painter
        };
        let state = State::REST
            .with(State::SELECTED, *active == Some(&tab.panel))
            .with(State::HOVER, tab.hovered)
            .with(State::ACTIVE, tab.pressed)
            .with(State::FOCUS, tab.focused);
        let Some(prep) = prepared.get(&tab.panel) else {
            continue;
        };
        kit.tab.paint_part(
            &KitCx {
                painter: &painter,
                ppp,
            },
            &Part {
                rect: tab.rect,
                room: tab.rect,
                state,
                label: Some(&prep.label),
            },
        );
    }
}

impl TabViewer for Viewer<'_> {
    type Tab = PanelId;

    fn id(&mut self, tab: &mut PanelId) -> Id {
        Id::new(("egui_pop.panel", tab.as_str()))
    }

    fn title(&mut self, tab: &mut PanelId) -> egui::WidgetText {
        // The prepared label: egui_dock lays nothing out again.
        match self.prepared.get(tab) {
            Some(p) => p.label.clone().into(),
            None => self.app.panel_title(&self.app.egui(), tab).into(),
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut PanelId) {
        let rect = ui.max_rect();
        if ui.input(|i| i.pointer.any_pressed()) && ui.rect_contains_pointer(rect) {
            self.app.workbench_mut().focused = Some(tab.clone());
        }
        self.app.show_panel(ui, tab);
    }

    fn on_tab_button(&mut self, tab: &mut PanelId, response: &egui::Response) {
        // egui_dock's tab buttons carry no name: give them their panel's.
        match self.prepared.get(tab) {
            Some(p) => egui_pop_core::a11y::name(response, egui::WidgetType::Button, &p.title),
            None => {
                let title = self.app.panel_title(&response.ctx, tab);
                egui_pop_core::a11y::name(response, egui::WidgetType::Button, &title);
            }
        }
        self.seen.push(SeenTab {
            panel: tab.clone(),
            rect: response.rect,
            layer: response.layer_id,
            hovered: response.hovered(),
            pressed: response.is_pointer_button_down_on(),
            focused: response.has_focus(),
        });
    }

    fn tab_style_override(
        &self,
        tab: &PanelId,
        global: &egui_dock::TabStyle,
    ) -> Option<egui_dock::TabStyle> {
        let width = self.prepared.get(tab)?.width;
        Some(egui_dock::TabStyle {
            minimum_width: Some(width),
            ..global.clone()
        })
    }

    fn context_menu(&mut self, ui: &mut Ui, tab: &mut PanelId, _path: egui_dock::NodePath) {
        let spec = self.app.workbench_mut().spec(tab).cloned();
        let Some(spec) = spec else { return };
        let ctx = ui.ctx().clone();
        let title = strings::panel_title(&ctx, spec.id.as_str(), &spec.title);
        for area in Area::ALL {
            let allowed = spec.allows(area);
            let r = ui
                .add_enabled(
                    allowed,
                    egui::Button::new(strings::tr_args(
                        &ctx,
                        &txt::WORKBENCH_MOVE_TO,
                        &[("area", &area_text(&ctx, area))],
                    )),
                )
                .on_disabled_hover_text(refusal(&ctx, &spec, &title, area).unwrap_or_default());
            if r.clicked() {
                self.app.change_workbench(Change::Move(tab.clone(), area));
                ui.close();
            }
        }
        ui.separator();
        if spec.floatable
            && ui
                .button(strings::tr(&ctx, &txt::WORKBENCH_FLOAT))
                .clicked()
        {
            // Floats where its area is, a little in.
            let pos = {
                let wb = self.app.workbench_mut();
                let at = wb
                    .layout
                    .area_of(tab)
                    .and_then(|a| wb.rects.get(&a))
                    .map_or(wb.content.min, |r| r.min);
                let off = at - wb.content.min;
                [off.x + 24.0, off.y + 24.0]
            };
            self.app.change_workbench(Change::Float(tab.clone(), pos));
            ui.close();
        }
        if spec.detachable
            && ui
                .button(strings::tr(&ctx, &txt::WORKBENCH_DETACH))
                .clicked()
        {
            self.app.change_workbench(Change::Detach(tab.clone()));
            ui.close();
        }
        if ui
            .button(strings::tr_args(
                &ctx,
                &txt::WORKBENCH_CLOSE_PANEL,
                &[("panel", &title)],
            ))
            .clicked()
        {
            self.app.change_workbench(Change::Hide(tab.clone()));
            ui.close();
        }
    }

    fn allowed_in_windows(&self, _tab: &mut PanelId) -> bool {
        // Detached panels get windows of their own (a step); a tab
        // dropped over another area is moved there by the workbench.
        false
    }
}

/// Follows a tab drag; dropped over another area, `egui_dock` (which
/// only sees its own area) leaves it, and the workbench moves it there,
/// or says why it may not go.
// upstream: egui_dock has no per-tab, per-destination drop veto (see
// docs/upstream.md).
fn follow_drag(app: &AppHandle, ctx: &egui::Context) {
    let pos = ctx.input(|i| i.pointer.latest_pos());
    let dragged = app.workbench_mut().dragged_tab(ctx);
    if let Some((panel, from)) = dragged {
        let mut wb = app.workbench_mut();
        let at = pos.or(wb.dragging.as_ref().map(|d| d.2));
        if let Some(at) = at {
            wb.dragging = Some((panel, from, at));
        }
        return;
    }
    let taken = app.workbench_mut().dragging.take();
    let Some((panel, from, last)) = taken else {
        return;
    };
    let at = pos.unwrap_or(last);
    let to = app.workbench_mut().area_at(at).filter(|a| *a != from);
    if let Some(to) = to {
        let out = app.change_workbench(Change::Move(panel.clone(), to));
        if let Some(Outcome::Done(Err(e))) = &out {
            app.workbench_refused(&format!("move {panel}"), e);
        }
    }
}

/// While a tab is dragged over another area, shows whether it may go
/// there and, when it may, the ghost of its tab where it would land.
fn drop_hint(app: &AppHandle, ctx: &egui::Context) {
    let mut wb = app.workbench_mut();
    wb.hint = None;
    // A tab over another area, or a palette over an area it docks in.
    let target = if let Some((panel, from, at)) = wb.dragging.clone() {
        wb.area_at(at).filter(|a| *a != from).map(|to| (panel, to))
    } else if let Some(d) = wb.palette_drag.clone().filter(|_| wb.group_hint.is_none()) {
        let edge = crate::palettes::magnet(ctx);
        wb.dock_at(d.at, edge).map(|to| (d.panel, to))
    } else {
        None
    };
    let Some((panel, to)) = target else {
        return;
    };
    let Some(spec) = wb.spec(&panel).cloned() else {
        return;
    };
    let rect = wb.hint_rect(to);
    let bar = wb.bars.get(&to).copied();
    let joined = bar.map_or_else(|| wb.panels_in(to).len(), |b| b.tabs);
    let content = wb.content;
    drop(wb);
    let title = strings::panel_title(ctx, spec.id.as_str(), &spec.title);
    let why = refusal(ctx, &spec, &title, to);
    let kit = Kit::of(ctx);
    // The ghost: the panel's tab, laid out once per drag (a cache of its
    // own, as the areas' tabs are), where a drop puts it.
    let ghost = if why.is_none() {
        let cache_id = TAB_CACHE.keyed("ghost");
        let mut cache = cache_id.update(ctx, std::mem::take);
        cache.prepare(app, ctx, &kit, std::slice::from_ref(&panel));
        let prep = cache.tabs.get(&panel).cloned();
        cache_id.set(ctx, cache);
        prep.map(|p| {
            let at = ghost_rect(&kit, bar, rect, content, p.width);
            (p, at)
        })
    } else {
        None
    };
    app.workbench_mut().hint = Some(DropHint {
        panel,
        area: area_name(to),
        rect,
        refusal: why.clone(),
        preview: ghost.as_ref().map(|(_, at)| TabPreview {
            area: area_name(to),
            index: joined,
            rect: *at,
        }),
    });
    // The kit's `dock-preview`: at rest where the panel may go, disabled
    // with its label saying why where it may not.
    let t = egui_pop_core::theme::Tokens::of(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::new("egui_pop.workbench.drop"),
    ));
    let label = why.map(|why| {
        let font = egui::FontId::proportional(t.font.body);
        ctx.fonts_mut(|f| f.layout_no_wrap(why, font, Color32::PLACEHOLDER))
    });
    let part = Part {
        rect,
        room: rect,
        state: State::REST.with(State::DISABLED, label.is_some()),
        label: label.as_ref(),
    };
    let cx = KitCx {
        painter: &painter,
        ppp: ctx.pixels_per_point(),
    };
    kit.dock_preview.paint_part(&cx, &part);
    // Over it, the kit's `tab-preview`.
    if let Some((prep, at)) = &ghost {
        kit.tab_preview.paint_part(
            &cx,
            &Part {
                rect: *at,
                room: *at,
                state: State::REST,
                label: Some(&prep.label),
            },
        );
    }
}

/// Where a dragged tab's ghost goes: after the last tab of the area's
/// first tab bar (kept inside the bar), else, for an area drawn bare or
/// closed, at the top-left of where the hint is (kept inside the areas).
fn ghost_rect(kit: &Kit, bar: Option<TabBar>, hint: Rect, content: Rect, width: f32) -> Rect {
    let (row, start) = match bar {
        Some(b) => (b.rect, b.end),
        None => {
            let height = kit.tab_bar.data.height.max(kit.tab.data.height);
            let row = Rect::from_min_size(hint.min, egui::vec2(hint.width(), height));
            let room = if content.is_positive() { content } else { row };
            (
                Rect::from_x_y_ranges(room.x_range(), row.y_range()),
                hint.left(),
            )
        }
    };
    let x = start.min(row.right() - width).max(row.left());
    Rect::from_min_size(egui::pos2(x, row.top()), egui::vec2(width, row.height()))
}
