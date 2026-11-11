//! The workbench: panels in areas (the centre, the start and end side
//! bars, the bottom panel), as tabs and splits drawn by `egui_stack`. The
//! versioned layout document (`egui_pop_model::layout::Layout`) is the
//! saved form; the areas are the named docks of one `egui_stack::DockSpace`,
//! which is how they are drawn and rearranged by hand, and which is read
//! back into the layout as needed.
//!
//! The workbench lays the areas out (a side area's width, its fold and
//! flyout, the bottom panel's alignment) and draws each area's dock where
//! it put the area; the docks share one drag, so a tab dragged from one
//! area to another gets the same drop zones as inside one (the middle of a
//! group joins its tabs, its edges split it, a tab bar takes it between two
//! tabs), with the same preview.
//!
//! **Placement rules** come from each panel's declaration
//! (`PanelSpec`): the areas it may be in (`only_in`), its smallest size
//! across its area (`min_size`) and the size it asks for
//! (`default_size`). A tab dragged onto an area its rules keep it out of
//! is refused there: the drop zone shows it disabled, with the reason. A
//! tab dropped in another area is moved through the `workbench.move` step,
//! which a plugin may veto (the tab goes back where it was) or redirect
//! (ADR 0003).
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
//! | `workbench.move` | `{ "panel", "area" }` (`center`, `start_side`, `end_side`, `bottom`); also run once a tab is dropped in another area |
//! | `workbench.show` | `{ "panel" }`: docks it back (its default area) and shows its tab |
//! | `workbench.hide` | `{ "panel" }`: closes it (kept closed by the layout) |
//! | `workbench.focus` | `{ "panel" }`: shows it and gives it the keyboard: see [the focused panel](#the-focused-panel) |
//! | `workbench.fold` | `{ "area", "folded" }`: an area folded to its strip, or unfolded |
//! | `workbench.place` | `{ "area", "placement" }`: a side area lies `beside` the centre or `over` it |
//! | `workbench.flyout` | `{ "panel", "open" }`: a folded side area's panel opens alone over the centre, or closes |
//! | `workbench.flyout_size` | `{ "panel", "width" }`: the width of a panel's flyout, saved with the layout |
//! | `workbench.detach` | `{ "panel" }`: into a window of its own (its own scale) |
//! | `workbench.attach` | `{ "panel" }`: a detached panel docked back |
//! | `workbench.close_window` | `{ "panel" }`: a detached panel's window closes; the default docks it back, a veto keeps the window |
//! | `workbench.zen` | `{ "zen" }`: zen mode on or off (see below) |
//!
//! # The focused panel
//!
//! [`AppHandle::focused_panel`], the context key `focusedPanel`, is the panel
//! the keys obey. It follows the widget focus, however that arrived:
//!
//! - **Tab or any widget taking the focus:** its panel, the one it was drawn
//!   in, is the focused panel.
//! - **`workbench.focus` and `<panel>.focus`:** the panel is focused at once
//!   and takes the widget focus in the next pass: the widget named by
//!   `focus::prefer` in it (a canvas under a toolbar), else its first
//!   focusable widget, else the panel itself, a stop of its own, named after
//!   it and there only while it holds the focus, from which Tab goes on. It
//!   stays where it is if the focus already is in the panel. Behind a modal,
//!   or when an agent's command finds someone typing, only the logical focus
//!   moves.
//! - **A press** in a panel, whether or not a widget takes the focus.
//! - **The focus leaving for something no panel holds** (the menu, the status
//!   bar, a tab of a dock, nothing): the last panel stays focused until another
//!   takes it, so a menu's command acts on the panel it was opened from.
//! - **A floating palette or a flyout only borrows it:** a press in it moves
//!   nothing, a widget of it taking the focus does, and when that lets go
//!   (Escape, a click elsewhere, the palette closing) the keys go back to the
//!   panel of the layout that had them. So a click on a palette's background
//!   leaves the keys on the image, as in an image editor.
//! - **A detached window** counts when its widget takes the focus or the
//!   pointer presses in it, and the main window when it gets the system's focus
//!   back. Each window keeps its own widget focus (egui does).
//!
//! Following is a plain write, not a step, so it cannot loop back into
//! `workbench.focus`; each change is published once as `workbench.focused`.
//!
//! # Moving between regions
//!
//! `workbench.next_region` (F6) and `workbench.previous_region` (Shift+F6) move
//! the keyboard through the window's regions: the activity bar, the start side
//! area, the end side area, the bottom area, the centre, each palette drawn, an
//! open flyout, the status bar. Each landing follows
//! [`egui_pop_core::regions`]: an area without anything to focus in its panel
//! takes it as `workbench.focus` does, on the panel's own stop. A menu or a
//! popover open (not a flyout) keeps the focus, for Escape first, and so does a
//! modal; so does someone typing when an agent asks.
//!
//! Each declared panel gets the commands `<panel>.focus` and
//! `<panel>.toggle`.
//!
//! A side area lies **beside** the centre (an egui side panel, taking the
//! centre's room) or **over** it ([`Placement::Over`]): its folded strip
//! keeps the room in the layout, so the centre keeps its width, and the
//! area, unfolded, lies above the centre next to its strip with the kit's
//! `side-area` shadow (egui_stack's `ZoneState::over`). It is a lasting
//! placement: a click away or Escape leaves it, only a click on the strip
//! folds or unfolds it; its edge on the centre's side resizes it.
//! The centre never folds.
//!
//! **Strip icons.** A side area's strip shows one icon per panel, in tab
//! order: its [`PanelSpec::icon`] (a vector icon tinted by the kit's
//! `activity-bar-item`, crisp at any scale), else its title's first
//! letter; the tooltip gives the title and the panel's key, if it has
//! one. On a folded area a click opens that panel alone as a
//! **flyout** over the centre, next to the strip, without moving the
//! layout (egui_stack's `LayoutArea::flyout`, `workbench.flyout`);
//! Escape or a click away closes it (it is an overlay of the stack,
//! `egui_pop_core::overlay::hold`: Escape closes the top overlay first,
//! and a combo box or a context menu opened in it is part of it). A double
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
//! `palette.pin`, `palette.group`, `palette.ungroup`,
//! `palette.close_group`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use egui::{Color32, Id, Pos2, Rect, Role, Ui, ViewportBuilder, ViewportId, WidgetInfo};
use egui_pop_core::focus;
use egui_pop_core::kit::{Kit, KitCx, Part, Side, State};
use egui_pop_core::state::Slot;
use egui_pop_core::strings;
use egui_pop_model::ids::{PanelId, StepId, ZoneId};
use egui_pop_model::layout::{Auxiliary, FloatingGroup, Join, Layout, Node, Placement};
use egui_pop_model::panels::{Area, PanelSpec, Panels, Place};
use egui_stack::layout::{
    Content, Dir, Dismiss, Drops, Flyout, FlyoutPaint, Fold, Layout as ZoneLayout, LayoutArea,
    LayoutEvent, LayoutPainter, LayoutStyle, SashPaint, Shown, Size, StripPaint, Zone, ZoneName,
    ZoneViewer,
};
use egui_stack::tab_viewer::DropVerdict;
use egui_stack::{
    DockArea, DockDrop, DockRules, DockSpace, DockState, DropZone, NodeIndex, PartState, Split,
    TabViewer,
};
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
pub const STEP_JOIN_WINDOW: StepId = StepId::from_static("workbench.join_window");
pub const STEP_ZEN: StepId = StepId::from_static("workbench.zen");
pub const STEP_FLYOUT: StepId = StepId::from_static("workbench.flyout");
pub const STEP_FLYOUT_SIZE: StepId = StepId::from_static("workbench.flyout_size");
pub const STEP_ARRANGE: StepId = StepId::from_static("workbench.arrange");

/// Toggles zen mode: only the centre is shown.
pub const CMD_ZEN: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.zen");
/// Leaves zen mode (Escape while in it).
/// The keyboard to the next region of the window (F6): see
/// [Moving between regions](self#moving-between-regions).
/// The keyboard to the next tab of the focused panel's tab group
/// (Ctrl+Page Down).
pub const CMD_NEXT_TAB: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.tab.next");
/// The keyboard to the previous tab of the focused panel's tab group
/// (Ctrl+Page Up).
pub const CMD_PREVIOUS_TAB: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.tab.previous");
pub const CMD_NEXT_REGION: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.next_region");

/// The keyboard to the previous region of the window (Shift+F6).
pub const CMD_PREVIOUS_REGION: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.previous_region");

pub const CMD_LEAVE_ZEN: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.leave_zen");

/// Lays out the focused panel's group and its neighbours (the row,
/// column or grid holding it) as a grid.
/// Moves a panel into a detached window, as a tab (`panel`: the focused
/// one by default; `window`: a panel of that window).
pub const CMD_MOVE_TO_WINDOW: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.move_to_window");
pub const CMD_ARRANGE_GRID: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.arrange.grid");
/// Lays out the focused panel's group and its neighbours in a row.
pub const CMD_ARRANGE_ROW: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.arrange.row");
/// Lays out the focused panel's group and its neighbours in a column.
pub const CMD_ARRANGE_COLUMN: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("workbench.arrange.column");

/// The setting: how the bottom panel lines up with the side bars, as
/// VS Code's panel alignment (see [`PanelAlignment`]).
pub const SETTING_PANEL_ALIGNMENT: egui_pop_model::ids::SettingId =
    egui_pop_model::ids::SettingId::from_static("workbench.panel_alignment");

/// A size across an area set by the application
/// ([`AppBuilder::shell_panel_min_size`](crate::AppBuilder::shell_panel_min_size),
/// `shell_panel_max_size`, `centre_min_size`): points, or a share of the
/// window's content (0.6: 60 % of its width). A plain number is points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShellSize {
    Points(f32),
    Share(f32),
}

impl ShellSize {
    /// In points, in a window whose content is `across` points across.
    pub fn resolve(self, across: f32) -> f32 {
        let points = match self {
            ShellSize::Points(p) => p,
            ShellSize::Share(s) => s * across,
        };
        if points.is_finite() {
            points.max(0.0)
        } else {
            0.0
        }
    }
}

impl From<f32> for ShellSize {
    fn from(points: f32) -> Self {
        ShellSize::Points(points)
    }
}

/// The least and largest sizes the application set for a shell panel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ShellBounds {
    pub(crate) min: Option<ShellSize>,
    pub(crate) max: Option<ShellSize>,
}

/// How the bottom panel lines up with the side bars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelAlignment {
    /// The window's whole width, under both side bars.
    Justify,
    /// Between the side bars, which go down to the bottom: under the
    /// editors (the default, as VS Code's).
    #[default]
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
        StepSpec::new(
            STEP_MOVE,
            "A panel moves to an area (`panel`, `area`); a tab dropped in another area runs it after the drop, and a veto puts it back.",
        )
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
            STEP_ARRANGE,
            "The groups beside a panel's (the row, column or grid holding it) are laid out again (`panel`, `as`: `grid`, `row` or `column`; `columns` for a grid, none for as many as fit a square).",
        )
        .args(json!({ "panel": "app.inspector", "as": "grid" })),
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
            "A detached panel's window closes; by default its panels dock back (`panel`).",
        )
        .args(json!({ "panel": "app.inspector" })),
        StepSpec::new(
            STEP_JOIN_WINDOW,
            "A panel moves into a detached window as a tab (`panel`; `window`: a panel of that window).",
        )
        .args(json!({ "panel": "app.outline", "window": "app.inspector" })),
        StepSpec::new(
            STEP_ZEN,
            "Zen mode starts (`zen`: true: only the centre is shown) or ends (false).",
        )
        .args(json!({ "zen": true })),
    ]
}

/// How close to the areas' edges a dragged tab or palette is offered a
/// closed side or bottom area.
const EDGE: f32 = 48.0;

/// The id the areas' docks share: one drag between them.
pub(crate) fn space_id() -> Id {
    Id::unique("egui_pop.workbench.space")
}

/// The name of the space's drop target over an area drawn bare (a zone
/// alone, without a tab bar): a tab dropped there joins the area's tabs.
fn bare_target(area: Area) -> &'static str {
    match area {
        Area::Center => "center.bare",
        Area::StartSide => "start_side.bare",
        Area::EndSide => "end_side.bare",
        Area::Bottom => "bottom.bare",
    }
}

/// The area a dock of the space stands for (its own, or its bare target),
/// and whether it is the bare target.
fn dock_area(name: &str) -> Option<(Area, bool)> {
    match name.strip_suffix(".bare") {
        Some(area) => area_from(area).map(|a| (a, true)),
        None => area_from(name).map(|a| (a, false)),
    }
}

/// An area's dock rules: any split, as inside one area; no floating
/// window (a panel leaves the main window through `workbench.detach`);
/// kept when empty, a target for a tab dragged to a closed area.
fn area_rules() -> DockRules {
    DockRules::default().windows(false).keep_when_empty(true)
}

/// A bare target's rules: the tab joins the area's tabs, so no split.
fn bare_rules() -> DockRules {
    area_rules().splits(egui_stack::AllowedSplits::None)
}

/// An area's name in documents and arguments.
pub fn area_name(area: Area) -> &'static str {
    match area {
        Area::Center => "center",
        Area::StartSide => "start_side",
        Area::EndSide => "end_side",
        Area::Bottom => "bottom",
    }
}

/// Whether a side area lies on the window's left: the start one, unless
/// the primary side bar is on the right (`flipped`). `false` for the
/// centre and the bottom area.
pub(crate) fn on_left(area: Area, flipped: bool) -> bool {
    match area {
        Area::StartSide => !flipped,
        Area::EndSide => flipped,
        _ => false,
    }
}

/// An area from its name.
pub fn area_from(name: &str) -> Option<Area> {
    Area::ALL.into_iter().find(|a| area_name(*a) == name)
}

/// The drop hint shown while a tab is dragged (in its area or over
/// another), or a palette over an area it docks in: what
/// [`AppHandle::drop_hint`], `overlay_rects` (`drop:<panel>:<area>`) and
/// the `workbench` collector report.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DropHint {
    pub panel: PanelId,
    /// The area the panel would go to (`area_name`).
    pub area: &'static str,
    /// Where the hint is drawn, in points: the region the panel would
    /// take (a group of tabs, the half of it a split gives, a place in a
    /// tab bar, a closed area along the edge).
    pub rect: Rect,
    /// Why the panel may not go there, as the hint says it; `None` when
    /// it may.
    pub refusal: Option<String>,
    /// How it would go there: [`DropZone::Center`] joins a group's tabs,
    /// an edge splits the group on that side.
    pub zone: DropZone,
    /// Its index among the tabs it would join; `None` for a split.
    pub index: Option<usize>,
    /// A palette's: the ghost of its tab where it would land; `None` when
    /// the drop is refused, and for a tab (the drop zone shows it).
    pub preview: Option<TabPreview>,
}

/// Where a drop hint draws a dragged palette's tab (the kit's
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

/// A tab dragged over a detached panel's window: a drop there moves
/// it into that window, as a tab (`workbench.join_window`).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowDrop {
    /// The dragged panel.
    pub panel: PanelId,
    /// The window, by its first panel.
    pub window: PanelId,
    /// Where it would go: the window's body, the zone drawn there.
    pub zone: egui_stack::DropZone,
    /// Why it may not go there, as the zone's label says.
    pub refusal: Option<String>,
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
    /// A detached panel's window closed (by default: docked back, with
    /// the panels that joined it as tabs).
    CloseWindow(PanelId),
    /// A panel joins a detached window as a tab: the window holding the
    /// second panel.
    JoinWindow(PanelId, PanelId),
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
    /// The tab group a floating panel is a tab of, nested in a stack,
    /// comes out of the stack whole, at a place.
    UngroupTabs(PanelId, [f32; 2]),
    /// Every member of a floating panel's group closes.
    CloseGroup(PanelId),
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
    /// The row, column or grid holding a panel's group is laid out again.
    Arrange(PanelId, Arrangement),
}

/// How [`Change::Arrange`] lays out the groups beside a panel's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Arrangement {
    /// In rows and columns, row by row: `columns` of them, or as many as
    /// the square root of their number, rounded up (`None`).
    Grid(Option<usize>),
    /// Side by side.
    Row,
    /// One above the other.
    Column,
}

impl Arrangement {
    /// Its name in a step's arguments.
    pub fn as_str(self) -> &'static str {
        match self {
            Arrangement::Grid(_) => "grid",
            Arrangement::Row => "row",
            Arrangement::Column => "column",
        }
    }

    fn from_args(args: &Json) -> Result<Arrangement, String> {
        let name = args.get("as").and_then(Json::as_str).unwrap_or("grid");
        match name {
            "grid" => {
                let columns = match args.get("columns").filter(|c| !c.is_null()) {
                    None => None,
                    Some(c) => Some(
                        c.as_u64()
                            .filter(|n| *n > 0)
                            .ok_or("`columns` is a positive whole number")?
                            as usize,
                    ),
                };
                Ok(Arrangement::Grid(columns))
            }
            "row" => Ok(Arrangement::Row),
            "column" => Ok(Arrangement::Column),
            _ => Err(format!(
                "no arrangement `{name}` (`grid`, `row` or `column`)"
            )),
        }
    }
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
            Change::JoinWindow(p, w) => (STEP_JOIN_WINDOW, json!({ "panel": p, "window": w })),
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
            Change::UngroupTabs(p, pos) => (
                crate::palettes::STEP_UNGROUP,
                json!({ "panel": p, "pos": pos, "tabs": true }),
            ),
            Change::CloseGroup(p) => (crate::palettes::STEP_CLOSE_GROUP, json!({ "panel": p })),
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
            Change::Arrange(p, how) => {
                let mut args = json!({ "panel": p, "as": how.as_str() });
                if let Arrangement::Grid(Some(n)) = how {
                    args["columns"] = json!(n);
                }
                (STEP_ARRANGE, args)
            }
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
            Change::Ungroup(..) | Change::UngroupTabs(..) => "palette.ungrouped",
            Change::CloseGroup(..) => "palette.group_closed",
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
        // A flag that is off unless it is given.
        let flag_off = |key: &str| args.get(key).and_then(Json::as_bool).unwrap_or(false);
        Ok(match step.as_str() {
            "palette.open" => Change::OpenPalette(panel),
            "palette.float" => Change::Float(panel, pos_of(args)?),
            "palette.move" => Change::MovePalette(panel, pos_of(args)?),
            "palette.resize" => Change::ResizePalette(panel, size_of(args)?),
            "palette.ungroup" if flag_off("tabs") => Change::UngroupTabs(panel, pos_of(args)?),
            "palette.ungroup" => Change::Ungroup(panel, pos_of(args)?),
            "palette.close_group" => Change::CloseGroup(panel),
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
            "workbench.join_window" => {
                let w = args
                    .get("window")
                    .and_then(Json::as_str)
                    .ok_or("`window` is missing")?;
                Change::JoinWindow(panel, PanelId::new(w).map_err(|e| e.to_string())?)
            }
            "workbench.hide" => Change::Hide(panel),
            "workbench.arrange" => Change::Arrange(panel, Arrangement::from_args(args)?),
            _ => Change::Focus(panel),
        })
    }
}

/// Where the dragged tab would land, as the space asked the veto this
/// frame.
#[derive(Clone, Debug)]
struct Seen {
    panel: PanelId,
    /// The area (a drop on its bare target counts as on the area).
    area: Area,
    landing: Landing,
    refusal: Option<String>,
}

/// How a dragged tab lands in a dock.
#[derive(Clone, Copy, Debug)]
enum Landing {
    /// Among a group's tabs, at this index.
    At(usize),
    /// After a group's tabs.
    End(egui_stack::NodePath),
    /// Beside a group: the zone of the edge.
    Split(DropZone),
    /// In an empty dock (a closed area), or on an area drawn bare (after
    /// its first tabs).
    Whole { bare: bool },
}

impl Landing {
    fn of(destination: &egui_stack::TabDestination, bare: bool) -> Option<Landing> {
        use egui_stack::{TabDestination, TabInsert};
        Some(match destination {
            TabDestination::Node(_, TabInsert::Insert(index)) => Landing::At(index.0),
            TabDestination::Node(path, TabInsert::Append) => Landing::End(*path),
            TabDestination::Node(_, TabInsert::Split(split)) => Landing::Split(match split {
                Split::Left => DropZone::Left,
                Split::Right => DropZone::Right,
                Split::Above => DropZone::Top,
                Split::Below => DropZone::Bottom,
            }),
            TabDestination::EmptySurface(_) => Landing::Whole { bare },
            TabDestination::Window(_) => return None,
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
    /// The areas as drawn, each a dock of the space (and each its bare
    /// target); taken out while a dock is drawn.
    space: Option<DockSpace<PanelId>>,
    /// Where each area was drawn last frame.
    pub rects: BTreeMap<Area, Rect>,
    /// Each area's first tab bar as drawn last frame (none for an area
    /// drawn bare).
    pub bars: BTreeMap<Area, TabBar>,
    /// The areas drawn bare this frame.
    bare: BTreeSet<Area>,
    /// The areas' rectangle.
    pub content: Rect,
    /// A tab being dragged: its panel, its area, the pointer.
    pub dragging: Option<(PanelId, Area, Pos2)>,
    /// Where the dragged tab would land this frame, as the dock asked the
    /// veto, and where its preview was painted.
    landing: Option<Seen>,
    landing_rect: Option<Rect>,
    /// The drop hint drawn last frame.
    pub hint: Option<DropHint>,
    /// The detached window under a dragged tab last frame, where a drop
    /// moves it.
    pub window_drop: Option<WindowDrop>,
    /// The panel the keys obey: see [`Workbench::follow_focus`].
    pub focused: Option<PanelId>,
    /// The last panel to be `focused` that lives in the layout proper: where
    /// the keys go back to when a palette or a flyout lets go.
    settled: Option<PanelId>,
    /// A panel to give the widget focus to, and for how many passes it has
    /// waited.
    focus_request: Option<(PanelId, u8)>,
    /// The layout changed outside the space: project it again.
    stale: bool,
    /// The space taken out to draw a dock: the layout is not read back
    /// then.
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
    /// The panel whose flyout was held in the overlay stack last frame.
    flyout_held: Option<PanelId>,
    /// The shell's zones that hold no dock this frame (the title, top and
    /// status bars, the activity bar): a tab released there goes back
    /// where it was, with no drop preview.
    pub no_drop: Vec<Rect>,
    /// The primary (start) side bar lies on the right, the secondary on
    /// the left (`workbench.side_bar_position`), as of the last pass.
    pub flipped: bool,
    /// The sizes the user gave the side and bottom areas, by zone name
    /// (`shell_sizes`); `None` until loaded.
    shell_sizes: Option<BTreeMap<String, f32>>,
}

impl Default for Workbench {
    fn default() -> Self {
        Self {
            specs: Vec::new(),
            zone_of: BTreeMap::new(),
            layout: Layout::default(),
            space: None,
            rects: BTreeMap::new(),
            bars: BTreeMap::new(),
            bare: BTreeSet::new(),
            content: Rect::NOTHING,
            dragging: None,
            landing: None,
            landing_rect: None,
            window_drop: None,
            hint: None,
            focused: None,
            settled: None,
            focus_request: None,
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
            flyout_held: None,
            no_drop: Vec::new(),
            flipped: false,
            shell_sizes: None,
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

    /// The layout as it stands (the space read back).
    pub fn layout(&mut self) -> Layout {
        self.read_back();
        self.layout.clone()
    }

    /// Reads the docks' tabs and splits back into the layout.
    fn read_back(&mut self) {
        if self.stale || self.taken > 0 {
            return;
        }
        if let Some(space) = &self.space {
            self.layout.areas = from_space(space);
        }
    }

    /// Makes the space show the layout, when it changed outside it.
    pub fn project(&mut self) {
        if !self.stale {
            return;
        }
        self.space = Some(to_space(&self.layout));
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

    /// Whether a panel only borrows the keys: it floats above the layout, or
    /// opens over the centre from a folded area. A press in it leaves them
    /// where they are, and they go back when it lets go.
    fn borrows_keys(&self, panel: &PanelId) -> bool {
        self.layout.floating_of(panel).is_some()
            || self
                .layout
                .area_of(panel)
                .is_some_and(|a| self.layout.folded.contains(&a))
    }

    fn set_focused(&mut self, panel: &PanelId) {
        if !self.borrows_keys(panel) {
            self.settled = Some(panel.clone());
        }
        self.focused = Some(panel.clone());
    }

    /// A panel goes away: a panel only borrowing the keys gives them back.
    fn forget_focus(&mut self, panel: &PanelId) {
        if self.settled.as_ref() == Some(panel) {
            self.settled = None;
        }
        if self.focused.as_ref() == Some(panel) {
            self.focused = self.settled.clone();
        }
        if self.focus_request.as_ref().is_some_and(|(p, _)| p == panel) {
            self.focus_request = None;
        }
    }

    /// The panel drawn in these scopes (the innermost, should one be drawn
    /// inside another).
    fn panel_of(&self, chain: &[focus::ScopeRecord]) -> Option<PanelId> {
        chain.iter().rev().find_map(|r| {
            let spec = self
                .specs
                .iter()
                .find(|s| crate::app::scope_name(s.id.as_str()) == r.name)?;
            Some(spec.id.clone())
        })
    }

    /// Follows the widget focus and the pointer: see
    /// [the focused panel](self#the-focused-panel). Whether `focused` changed.
    pub(crate) fn follow_focus(&mut self, moved: focus::Moved) -> bool {
        let before = self.focused.clone();
        match moved {
            focus::Moved::Pressed(chain) => {
                if let Some(p) = self.panel_of(&chain).filter(|p| !self.borrows_keys(p)) {
                    self.set_focused(&p);
                }
            }
            focus::Moved::Focused(chain) => {
                if let Some(p) = self.panel_of(&chain) {
                    self.set_focused(&p);
                }
            }
            focus::Moved::Left => {
                if self.focused.as_ref().is_some_and(|p| self.borrows_keys(p)) {
                    self.focused = self.settled.clone();
                }
            }
        }
        // A flyout closed without the focus going anywhere.
        if let Some(p) = &self.focused
            && self.borrows_keys(p)
            && self.layout.floating_of(p).is_none()
            && self.flyout.as_ref() != Some(p)
        {
            self.focused = self.settled.clone();
        }
        before != self.focused
    }

    /// One more pass for a panel to take the widget focus. A panel that was
    /// not drawn in this time (a hidden zone, a rolled palette) is left with
    /// the logical focus: nothing steals the keyboard long after the command.
    pub(crate) fn age_focus_request(&mut self) {
        const PASSES: u8 = 4;
        if let Some((_, age)) = &mut self.focus_request {
            *age += 1;
            if *age > PASSES {
                self.focus_request = None;
            }
        }
    }

    /// The panel asked for keeps the logical focus only.
    pub(crate) fn cancel_focus_request(&mut self) {
        self.focus_request = None;
    }

    /// Takes the request to give a panel the widget focus, if it is this
    /// panel's and a pass old: the input of the pass that asked (the key
    /// bound to the command, an Enter on a button) is not read by the widget
    /// that gets the focus.
    pub(crate) fn take_focus_request(&mut self, panel: &PanelId) -> bool {
        let ripe = matches!(&self.focus_request, Some((p, age)) if p == panel && *age > 0);
        if ripe {
            self.focus_request = None;
        }
        ripe
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
                // Already there (a tab a drag dropped there): kept where
                // it was dropped.
                if self.layout.area_of(p) != Some(*area) {
                    self.layout.place(p, *area);
                }
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
                self.forget_focus(p);
                json!({ "panel": p, "shown": false })
            }
            Change::Focus(p) => {
                let spec = spec_of(p)?;
                if !self.layout.shows(p) {
                    self.reopen(&spec);
                }
                self.layout.activate(p);
                let area = self.layout.area_of(p);
                self.layout.folded.retain(|a| Some(*a) != area);
                self.set_focused(p);
                self.focus_request = Some((p.clone(), 0));
                json!({ "panel": p, "focused": true })
            }
            Change::Detach(p) => {
                let spec = spec_of(p)?;
                if let Some(why) = spec.refuses(&crate::mounts::panel_host(Place::Window)) {
                    return Err(why);
                }
                let window = self.layout.window_of(p);
                if window.is_some_and(|a| a.tabs.is_empty()) {
                    return Ok(json!({ "panel": p, "detached": true }));
                }
                // A tab of a window gets a window of its own beside it.
                let beside = window.map(|a| {
                    let [x, y, w, h] = a.rect;
                    [x + 40.0, y + 40.0, w, h]
                });
                // Where it was drawn, in the window's coordinates.
                let rect = beside.unwrap_or_else(|| {
                    self.layout
                        .area_of(p)
                        .and_then(|a| self.rects.get(&a))
                        .map_or(DETACHED, |r| {
                            [
                                r.min.x + 40.0,
                                r.min.y + 40.0,
                                r.width().max(320.0),
                                r.height().max(240.0),
                            ]
                        })
                });
                self.layout.remove(p);
                self.layout.hidden.retain(|h| h != p);
                self.layout
                    .auxiliary
                    .push(egui_pop_model::layout::Auxiliary::window(
                        p.clone(),
                        crate::app::SCREENS[0],
                        rect,
                    ));
                json!({ "panel": p, "detached": true })
            }
            Change::Attach(p) => {
                let spec = spec_of(p)?;
                self.layout.place(p, spec.area);
                json!({ "panel": p, "detached": false })
            }
            Change::CloseWindow(p) => {
                // Every panel of the window docks back.
                let panels: Vec<PanelId> = self
                    .layout
                    .window_of(p)
                    .map_or_else(|| vec![p.clone()], |a| a.panels().cloned().collect());
                spec_of(p)?;
                let homes: Vec<(PanelId, Area)> = (panels.iter())
                    .filter_map(|q| spec_of(q).ok().map(|s| (q.clone(), s.area)))
                    .collect();
                for (q, area) in &homes {
                    self.layout.place(q, *area);
                }
                json!({ "panel": p, "detached": false, "docked": panels })
            }
            Change::JoinWindow(p, window) => {
                let spec = spec_of(p)?;
                if let Some(why) = spec.refuses(&crate::mounts::panel_host(Place::Window)) {
                    return Err(why);
                }
                self.layout.join_window(p, window)?;
                let named = self.layout.window_of(p).map(|a| a.panel.clone());
                json!({ "panel": p, "window": named, "detached": true })
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
            Change::UngroupTabs(p, pos) => {
                self.layout.ungroup_tabs(p, *pos)?;
                self.anchor_palette(p, true);
                json!({ "panel": p, "pos": pos, "tabs": true })
            }
            Change::CloseGroup(p) => {
                let panels: Vec<PanelId> = self
                    .layout
                    .floating_of(p)
                    .ok_or_else(|| format!("`{p}` does not float"))?
                    .panels()
                    .cloned()
                    .collect();
                for q in &panels {
                    self.layout.hide(q);
                    self.forget_focus(q);
                }
                json!({ "panel": p, "panels": panels })
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
            Change::Arrange(p, how) => {
                let spec = spec_of(p)?;
                let (area, node) = self
                    .layout
                    .areas
                    .iter_mut()
                    .find(|(_, n)| n.panels().contains(&p))
                    .ok_or_else(|| format!("{} is not in an area", spec.title))?;
                *node = arrange(node, p, *how)
                    .ok_or_else(|| format!("{} is alone in its area", spec.title))?;
                let mut out = json!({ "panel": p, "area": area_name(*area), "as": how.as_str() });
                if let Arrangement::Grid(columns) = how {
                    out["columns"] = json!(columns);
                }
                out
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

    /// Takes the space out to draw one of its docks, projecting the
    /// layout first if it changed.
    fn take_space(&mut self) -> Option<DockSpace<PanelId>> {
        if self.taken == 0 {
            self.project();
        }
        let s = self.space.take();
        if s.is_some() {
            self.taken += 1;
        }
        s
    }

    /// Puts the space back once a dock is drawn; dropped if the layout
    /// changed meanwhile (it is projected again).
    fn put_space(&mut self, space: DockSpace<PanelId>) {
        self.taken = self.taken.saturating_sub(1);
        if !self.stale {
            self.space = Some(space);
        }
    }

    /// The areas holding panels, in order.
    pub fn areas(&self) -> Vec<Area> {
        let mut areas: Vec<Area> = Area::ALL
            .into_iter()
            .filter(|a| !self.panels_in(*a).is_empty())
            .collect();
        areas.sort();
        areas
    }

    /// The panels an area holds now.
    pub fn panels_in(&self, area: Area) -> Vec<PanelId> {
        self.space
            .as_ref()
            .and_then(|s| s.dock(area_name(area)))
            .map(|t| t.tabs().cloned().collect())
            .unwrap_or_default()
    }

    /// The area the space holds a panel in.
    fn area_in_space(&self, panel: &PanelId) -> Option<Area> {
        let space = self.space.as_ref()?;
        let path = space.state().find_tab(panel)?;
        space
            .dock_name(path.surface)
            .and_then(dock_area)
            .map(|(a, _)| a)
    }

    /// The tab being dragged between the areas, as the space's docks last
    /// saw it: its panel, its area and the pointer.
    fn dragged(&self, ctx: &egui::Context) -> Option<(PanelId, Area, Pos2)> {
        let dragged = DockArea::dragged_tab(ctx, space_id())?;
        let space = self.space.as_ref()?;
        let leaf = space.state().leaf(dragged.path.node_path()).ok()?;
        let panel = leaf.tabs.get(dragged.path.tab.0)?.clone();
        let (area, _) = space.dock_name(dragged.path.surface).and_then(dock_area)?;
        Some((panel, area, dragged.pointer))
    }

    /// The area a palette dropped at `at` docks in: a side or bottom area
    /// drawn there, or a closed one when the pointer is within `edge` of
    /// the areas' edges (narrower than for tabs: a palette snapped to an
    /// edge must not dock); `None` over the centre (it floats there).
    pub fn dock_at(&self, at: Pos2, edge: f32) -> Option<Area> {
        let shown = |a: Area| !self.panels_in(a).is_empty();
        let c = self.content;
        if c.is_positive() {
            let (left, right) = self.left_right();
            if !shown(left) && at.x < c.left() + edge {
                return Some(left);
            }
            if !shown(right) && at.x > c.right() - edge {
                return Some(right);
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

    /// Whether a side area lies on the window's left now (the start one,
    /// unless the primary side bar is on the right).
    pub fn on_left(&self, area: Area) -> bool {
        on_left(area, self.flipped)
    }

    /// The side areas on the left and on the right.
    fn left_right(&self) -> (Area, Area) {
        if self.flipped {
            (Area::EndSide, Area::StartSide)
        } else {
            (Area::StartSide, Area::EndSide)
        }
    }

    /// Keeps where a dock painted the dragged tab's drop zone, if it did.
    fn note_landing(&mut self, seen: Mutex<Option<Rect>>) {
        if let Some(rect) = seen.into_inner().ok().flatten() {
            self.landing_rect = Some(rect);
        }
    }

    /// Where a drop hint for `area` is drawn when no dock draws it: the
    /// area, or along the areas' edge for a closed one.
    pub fn hint_rect(&self, area: Area) -> Rect {
        let c = self.content;
        let left = Rect::from_min_max(c.min, egui::pos2(c.left() + EDGE, c.bottom()));
        let right = Rect::from_min_max(egui::pos2(c.right() - EDGE, c.top()), c.max);
        self.rects.get(&area).copied().unwrap_or(match area {
            Area::StartSide | Area::EndSide if self.on_left(area) => left,
            Area::StartSide | Area::EndSide => right,
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
/// panel), the `WINDOW_OVERLAYS` zone draws the overlays opened from it
/// over it (dialogs, the command palette, popup menus; the toasts while
/// it is the active window), and a modal open in another window blocks
/// it (`overlay::block_window`).
pub(crate) fn windows(
    app: &AppHandle,
    ctx: &egui::Context,
    app_name: &str,
    keys: &mut dyn FnMut(&egui::Context),
) {
    let (auxiliary, dragging) = {
        let mut wb = app.workbench_mut();
        // Found again by this pass's windows.
        wb.window_drop = None;
        (wb.layout.auxiliary.clone(), wb.dragging.clone())
    };
    // The main window's top-left on the screen, in pixels, for windows
    // that do not see a drag begun in the main one.
    let origin = ctx.input(|i| i.viewport().inner_rect).map(|r| {
        (
            r.min.to_vec2() * ctx.pixels_per_point(),
            ctx.pixels_per_point(),
        )
    });
    for aux in auxiliary {
        // Titled after the panel it shows.
        let Some(spec) = app.workbench_mut().spec(aux.shown()).cloned() else {
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
                egui_pop_core::overlay::block_window_among(&vctx, &|v| {
                    !crate::windows::is_app_window(app, v)
                });
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
                let out = egui::Panel::top(Id::unique(("egui_pop.aux.title", panel.as_str())))
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
            let body = egui::CentralPanel::default()
                .show(ui, |ui| {
                    if aux.tabs.is_empty() {
                        app.show_panel(ui, &panel);
                    } else {
                        window_tabs(app, ui, &aux);
                    }
                })
                .response
                .rect;
            if let Some((dragged, _, at)) = &dragging
                && !aux.holds(dragged)
            {
                let pointer = window_pointer(&vctx, own_window, *at, origin);
                if let Some(pointer) = pointer.filter(|p| body.contains(*p)) {
                    window_drop(app, &vctx, &aux, dragged, body, pointer);
                }
            }
            if own_window {
                app.show_zone(ui, &crate::zones::WINDOW_OVERLAYS);
                crate::menus::draw_popup(&vctx, app);
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

/// A detached window holding tabs: one tab bar (egui_stack's, painted
/// by the kit as the areas' are), no splits. The order the user gives
/// the tabs and the one shown go back to the layout.
fn window_tabs(app: &AppHandle, ui: &mut Ui, aux: &Auxiliary) {
    let ctx = ui.ctx().clone();
    let kit = Kit::of(&ctx);
    let panels: Vec<PanelId> = aux.panels().cloned().collect();
    let mut dock = DockState::new(panels.clone());
    if let Some(path) = dock.find_tab(aux.shown()) {
        let _ = dock.set_active_tab(path);
    }
    let id = window_dock_id(&aux.panel);
    let cache_id = TAB_CACHE.keyed(id);
    let mut cache = cache_id.update(&ctx, std::mem::take);
    cache.prepare(app, &ctx, &kit, &panels);
    let style = cache.dock_style(ui.style(), &kit);
    let mut viewer = Viewer {
        app,
        prepared: &cache.tabs,
    };
    DockArea::new(&mut dock)
        .id(id)
        .style(style)
        .painter(&*kit)
        .allowed_splits(egui_stack::AllowedSplits::None)
        .show_close_buttons(false)
        .show_add_buttons(false)
        .show_leaf_close_all_buttons(false)
        .show_leaf_collapse_buttons(false)
        .show_inside(ui, &mut viewer);
    cache_id.set(&ctx, cache);
    if let Some(Node::Tabs { panels, active }) = from_dock(&dock) {
        let mut wb = app.workbench_mut();
        if let Some(a) = wb
            .layout
            .auxiliary
            .iter_mut()
            .find(|a| a.panel == aux.panel)
        {
            a.reorder(&panels);
            if let Some(p) = panels.get(active) {
                a.show(p);
            }
        }
    }
}

/// A detached window's tab dock id.
fn window_dock_id(panel: &PanelId) -> Id {
    Id::unique(("egui_pop.window_dock", panel.as_str()))
}

/// Where the pointer is over a detached window while a tab drags, in
/// that window's points, if egui knows it.
///
/// egui keeps each window's input apart, the drag in the window it began
/// in. A window embedded in the main one (kittest, a backend without
/// windows of its own) sees the main one's pointer. An OS window of its
/// own sees the pointer only once the drag lets it go: while the button
/// is held, the system gives every pointer event to the window where it
/// was pressed (Wayland's and X11's implicit grab, the capture on
/// Windows). There the main window's pointer, which the grab keeps
/// following past its edges, is carried over the screen, when both
/// windows know where they are (not on Wayland, which hides window
/// positions: the tab menu's "Move to window" works everywhere).
fn window_pointer(
    vctx: &egui::Context,
    own_window: bool,
    at: Pos2,
    origin: Option<(egui::Vec2, f32)>,
) -> Option<Pos2> {
    if let Some(p) = vctx.input(|i| i.pointer.hover_pos()) {
        return Some(p);
    }
    if !own_window {
        return Some(at);
    }
    let (origin, ppp) = origin?;
    let inner = vctx.input(|i| i.viewport().inner_rect)?;
    let screen = origin + at.to_vec2() * ppp;
    Some((screen / vctx.pixels_per_point() - inner.min.to_vec2()).to_pos2())
}

/// A tab dragged over a detached window: the zone a drop takes there,
/// drawn as the areas' (the kit's `dock-preview`, disabled with its
/// reason where the panel may not go), kept for the drop.
///
/// A detached window holds tabs, not splits: the zone is the window's
/// whole body (`DropZone::at` with no split allowed), and a drop there
/// joins its tabs.
pub(crate) fn window_drop(
    app: &AppHandle,
    vctx: &egui::Context,
    aux: &Auxiliary,
    dragged: &PanelId,
    body: Rect,
    pointer: Pos2,
) {
    let Some(spec) = app.workbench_mut().spec(dragged).cloned() else {
        return;
    };
    let zone = egui_stack::DropZone::at(
        body,
        pointer,
        WINDOW_EDGE_COVERAGE,
        egui_stack::AllowedSplits::None,
    );
    let title = strings::panel_title(vctx, spec.id.as_str(), &spec.title);
    let refusal = spec
        .refuses(&crate::mounts::panel_host(Place::Window))
        .map(|_| strings::tr_args(vctx, &txt::WORKBENCH_STAYS_DOCKED, &[("panel", &title)]));
    let kit = Kit::of(vctx);
    let t = egui_pop_core::theme::Tokens::of(vctx);
    let painter = vctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::unique(("egui_pop.window_drop", aux.panel.as_str())),
    ));
    let label = refusal.clone().map(|why| {
        let font = egui::FontId::proportional(t.font.body);
        vctx.fonts_mut(|f| f.layout_no_wrap(why, font, Color32::PLACEHOLDER))
    });
    kit.dock_preview.paint_part(
        &KitCx {
            painter: &painter,
            ppp: vctx.pixels_per_point(),
        },
        &Part {
            rect: body,
            room: body,
            state: State::REST.with(State::DISABLED, label.is_some()),
            label: label.as_ref(),
        },
    );
    app.workbench_mut().window_drop = Some(WindowDrop {
        panel: dragged.clone(),
        window: aux.panel.clone(),
        zone,
        refusal,
    });
}

/// The share of a detached window's body along each edge a split would
/// take, as egui_stack's (unused while windows hold tabs only).
const WINDOW_EDGE_COVERAGE: f32 = 0.25;

/// Forgets the overlays of the windows closed since the last pass: what
/// they showed is gone with them (a dialog shown there comes to the main
/// window).
pub(crate) fn forget_closed_windows(app: &AppHandle, ctx: &egui::Context) {
    let mut open: Vec<ViewportId> = app
        .workbench_mut()
        .layout
        .auxiliary
        .iter()
        .map(|a| window_id(&a.panel))
        .collect();
    // The application's own windows keep theirs too (ADR 0025).
    open.extend(app.windows().iter().skip(1).map(|w| w.viewport));
    egui_pop_core::overlay::retain_windows(ctx, &open);
}

/// A detached panel's window id.
pub fn window_id(panel: &PanelId) -> ViewportId {
    ViewportId::from_hash_of(("egui_pop.aux", panel.as_str()))
}

/// An area's id: its egui panel's, and its tabs' cache key.
pub(crate) fn dock_id(area: Area) -> Id {
    Id::unique(("egui_pop.area", area_name(area)))
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

/// The layout's areas as one `egui_stack` space: each area a dock named
/// after it (`area_name`), holding its tree, kept when it is empty (a drop
/// target while a tab drags); and, for each area, its bare target. The
/// layout document stays the saved form, an area's tree as it was saved
/// before the areas shared a space: each tree becomes its area's dock
/// through `DockSpace::from_states`, the per-area docks of before taken in
/// once.
pub(crate) fn to_space(layout: &Layout) -> DockSpace<PanelId> {
    let states = Area::ALL.into_iter().map(|area| {
        let state = layout
            .areas
            .iter()
            .find(|(a, _)| *a == area)
            .map_or_else(empty_dock, |(_, node)| to_dock(node));
        (area_name(area).to_owned(), area_rules(), state)
    });
    let mut space = DockSpace::from_states(states).with_id(space_id());
    for area in Area::ALL {
        space.add_dock(bare_target(area), bare_rules(), Vec::new());
    }
    space
}

/// The areas' trees, read back from the space; a tab dropped on an area's
/// bare target and not yet settled joins the area's first tabs.
pub(crate) fn from_space(space: &DockSpace<PanelId>) -> Vec<(Area, Node)> {
    let mut areas: Vec<(Area, Node)> = Vec::new();
    for area in Area::ALL {
        let mut node = space
            .dock(area_name(area))
            .and_then(|tree| read(tree, NodeIndex::root()));
        for panel in space
            .dock(bare_target(area))
            .into_iter()
            .flat_map(|t| t.tabs())
        {
            match &mut node {
                Some(n) => push_first(n, panel.clone()),
                None => node = Some(Node::tab(panel.clone())),
            }
        }
        if let Some(node) = node {
            areas.push((area, node));
        }
    }
    areas.sort_by_key(|(a, _)| *a);
    areas
}

/// Adds a panel to a tree's first tabs, shown (as `Layout::place` does).
fn push_first(node: &mut Node, panel: PanelId) {
    match node {
        Node::Tabs { panels, active } => {
            panels.push(panel);
            *active = panels.len() - 1;
        }
        Node::Split { a, .. } => push_first(a, panel),
        Node::Grid { children, .. } => {
            if let Some(first) = children.first_mut() {
                push_first(first, panel);
            }
        }
    }
}

/// Moves the tabs dropped on the areas' bare targets into the areas: each
/// joins its area's first tabs, as a drop on an area drawn bare does.
fn settle_bare_drops(space: &mut DockSpace<PanelId>) {
    for area in Area::ALL {
        let (Some(from), Some(to)) = (
            space.dock_surface(bare_target(area)),
            space.dock_surface(area_name(area)),
        ) else {
            continue;
        };
        loop {
            let found = space
                .state()
                .iter_all_tabs()
                .find(|(p, _)| p.surface == from)
                .map(|(p, _)| p);
            let Some(path) = found else {
                break;
            };
            let tree = &space.state()[to];
            let destination: egui_stack::TabDestination = if tree.is_empty() {
                to.into()
            } else {
                let node = egui_stack::NodePath {
                    surface: to,
                    node: first_leaf(tree),
                };
                (node, egui_stack::TabInsert::Append).into()
            };
            space.state_mut().move_tab(path, destination);
        }
    }
}

/// A state whose main surface holds nothing: an empty dock, a drop target
/// as a whole.
fn empty_dock() -> DockState<PanelId> {
    let mut dock = DockState::new(Vec::new());
    *dock.main_surface_mut() = egui_stack::Tree::default();
    dock
}

/// An area's tree of tabs, splits and grids as an `egui_stack` state.
///
/// The tree is built slot by slot: its nodes are first taken as leaves in
/// one row, then each is given its part of the layout. A chain of splits
/// of one direction becomes one row or column, as egui_stack keeps them.
pub(crate) fn to_dock(node: &Node) -> DockState<PanelId> {
    let Some(any) = node.panels().first().map(|p| (*p).clone()) else {
        return empty_dock();
    };
    let mut dock = DockState::new(vec![any.clone()]);
    let tree = dock.main_surface_mut();
    let count = slots(node);
    let mut free = Vec::new();
    if count > 1 {
        let [a, b] = tree.split(
            NodeIndex::root(),
            Split::Right,
            0.5,
            egui_stack::Node::leaf(any.clone()),
        );
        free.extend([a, b]);
        while free.len() + 1 < count {
            free.push(tree.insert_child(NodeIndex::root(), 0, egui_stack::Node::leaf(any.clone())));
        }
    }
    let mut free = free.into_iter();
    build(tree, NodeIndex::root(), node, &mut free);
    let first = first_leaf(tree);
    tree.set_focused_node(first);
    dock
}

/// The children of a split as one row or column: the splits of the same
/// direction under it joined, each child with its share.
fn run<'a>(node: &'a Node, vertical: bool, weight: f32, out: &mut Vec<(&'a Node, f32)>) {
    match node {
        Node::Split {
            vertical: v,
            ratio,
            a,
            b,
        } if *v == vertical => {
            let r = ratio.clamp(0.05, 0.95);
            run(a, vertical, weight * r, out);
            run(b, vertical, weight * (1.0 - r), out);
        }
        _ => out.push((node, weight)),
    }
}

/// The number of egui_stack nodes a node takes.
fn slots(node: &Node) -> usize {
    match node {
        Node::Tabs { .. } => 1,
        Node::Split { vertical, .. } => {
            let mut children = Vec::new();
            run(node, *vertical, 1.0, &mut children);
            1 + children.iter().map(|(c, _)| slots(c)).sum::<usize>()
        }
        Node::Grid { children, .. } => 1 + children.iter().map(slots).sum::<usize>(),
    }
}

fn build(
    tree: &mut egui_stack::Tree<PanelId>,
    at: NodeIndex,
    node: &Node,
    free: &mut impl Iterator<Item = NodeIndex>,
) {
    let mut place = |tree: &mut egui_stack::Tree<PanelId>, children: &[&Node]| {
        let indices: Vec<NodeIndex> = children
            .iter()
            .map(|_| free.next().expect("one slot per node"))
            .collect();
        for (&child, &index) in children.iter().zip(&indices) {
            build(tree, index, child, free);
        }
        indices
    };
    match node {
        Node::Tabs { panels, active } => {
            tree[at] = egui_stack::Node::leaf_with(panels.clone());
            if let egui_stack::Node::Leaf(leaf) = &mut tree[at] {
                leaf.active = egui_stack::TabIndex((*active).min(panels.len().saturating_sub(1)));
            }
        }
        Node::Split { vertical, .. } => {
            let mut children = Vec::new();
            run(node, *vertical, 1.0, &mut children);
            let (nodes, shares): (Vec<&Node>, Vec<f32>) = children.into_iter().unzip();
            let indices = place(tree, &nodes);
            let dir = if *vertical {
                egui_stack::LinearDir::Vertical
            } else {
                egui_stack::LinearDir::Horizontal
            };
            tree[at] = egui_stack::Node::Linear(egui_stack::LinearNode::new(dir, indices, shares));
        }
        Node::Grid {
            columns,
            col_shares,
            row_shares,
            children,
        } => {
            let nodes: Vec<&Node> = children.iter().collect();
            let indices = place(tree, &nodes);
            let mut grid = egui_stack::GridNode::new(grid_layout(*columns), indices);
            let fits = |shares: &[f32], n: usize| {
                shares.len() == n && shares.iter().all(|s| s.is_finite() && *s > 0.0)
            };
            if fits(col_shares, grid.columns()) && fits(row_shares, grid.rows()) {
                grid.set_shares(col_shares.clone(), row_shares.clone());
            }
            tree[at] = egui_stack::Node::Grid(grid);
        }
    }
}

/// A grid's layout from its saved number of columns.
fn grid_layout(columns: Option<usize>) -> egui_stack::GridLayout {
    match columns {
        Some(n) if n > 0 => egui_stack::GridLayout::Columns(n),
        _ => egui_stack::GridLayout::Auto,
    }
}

/// The first leaf of a tree, as drawn (the root for a tree of one).
fn first_leaf(tree: &egui_stack::Tree<PanelId>) -> NodeIndex {
    let mut at = NodeIndex::root();
    while at.0 < tree.len()
        && let Some(&first) = tree[at].children().first()
    {
        at = first;
    }
    at
}

/// An area's tree with the row, column or grid holding `panel`'s group
/// laid out again; `None` when the group is alone in the area.
fn arrange(node: &Node, panel: &PanelId, how: Arrangement) -> Option<Node> {
    let mut dock = to_dock(node);
    let tree = dock.main_surface_mut();
    let (leaf, _) = tree.find_tab(panel)?;
    let container = tree.parent(leaf)?;
    let done = match how {
        Arrangement::Grid(columns) => tree.arrange_as_grid(container, grid_layout(columns)),
        Arrangement::Row => tree.arrange_as_linear(container, egui_stack::LinearDir::Horizontal),
        Arrangement::Column => tree.arrange_as_linear(container, egui_stack::LinearDir::Vertical),
    };
    done.then(|| from_dock(&dock)).flatten()
}

/// The tree a dock shows now; `None` when it holds no panel.
pub(crate) fn from_dock(dock: &DockState<PanelId>) -> Option<Node> {
    read(dock.main_surface(), NodeIndex::root())
}

fn read(tree: &egui_stack::Tree<PanelId>, at: NodeIndex) -> Option<Node> {
    if at.0 >= tree.len() {
        return None;
    }
    match &tree[at] {
        egui_stack::Node::Leaf(leaf) => (!leaf.tabs.is_empty()).then(|| Node::Tabs {
            panels: leaf.tabs.clone(),
            active: leaf.active.0.min(leaf.tabs.len() - 1),
        }),
        egui_stack::Node::Linear(linear) => {
            let vertical = linear.dir == egui_stack::LinearDir::Vertical;
            let children: Vec<(Node, f32)> = (linear.children().iter().zip(linear.shares()))
                .filter_map(|(&c, &share)| read(tree, c).map(|n| (n, share.max(0.0))))
                .collect();
            chain(vertical, children)
        }
        egui_stack::Node::Grid(grid) => {
            let all = grid.children().len();
            let mut children: Vec<Node> = grid
                .children()
                .iter()
                .filter_map(|&c| read(tree, c))
                .collect();
            match children.len() {
                0 => None,
                1 => children.pop(),
                n => {
                    // Children gone (their panels closed): even shares.
                    let kept = n == all;
                    Some(Node::Grid {
                        columns: match grid.layout {
                            egui_stack::GridLayout::Columns(n) => Some(n),
                            _ => None,
                        },
                        col_shares: if kept {
                            grid.col_shares().to_vec()
                        } else {
                            Vec::new()
                        },
                        row_shares: if kept {
                            grid.row_shares().to_vec()
                        } else {
                            Vec::new()
                        },
                        children,
                    })
                }
            }
        }
        _ => None,
    }
}

/// A row or column as splits nested on their second side.
fn chain(vertical: bool, mut children: Vec<(Node, f32)>) -> Option<Node> {
    match children.len() {
        0 => None,
        1 => children.pop().map(|(n, _)| n),
        _ => {
            let (a, share) = children.remove(0);
            let rest: f32 = children.iter().map(|(_, s)| s).sum();
            let ratio = if share + rest > 0.0 {
                share / (share + rest)
            } else {
                0.5
            };
            let b = chain(vertical, children)?;
            Some(Node::Split {
                vertical,
                ratio,
                a: Box::new(a),
                b: Box::new(b),
            })
        }
    }
}

/// Draws the docked areas in `ui` (side and bottom areas, the centre
/// takes what is left), each area's dock where the area is laid out;
/// while a tab drags, the closed areas and those drawn bare as drop
/// targets too. A tab dropped in another area is then moved through the
/// `workbench.move` step.
pub(crate) fn show(app: &AppHandle, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let across = ui.max_rect().width();
    let bounds = app.shell_bounds();
    let (areas, sizes, before) = {
        let mut wb = app.workbench_mut();
        wb.project();
        wb.content = ui.max_rect();
        wb.rects.clear();
        wb.bars.clear();
        wb.bare.clear();
        wb.landing = None;
        wb.landing_rect = None;
        // A tab dragged: the layout before a drop, to put it back if the
        // move is vetoed.
        let before = wb.dragged(&ctx).map(|(panel, from, _)| {
            wb.read_back();
            (panel, from, wb.layout.areas.clone())
        });
        let sizes: BTreeMap<Area, (f32, f32, f32)> = Area::ALL
            .into_iter()
            .map(|a| {
                let panels = wb.panels_in(a);
                let specs: Vec<&PanelSpec> = panels.iter().filter_map(|p| wb.spec(p)).collect();
                let set = |p: &PanelId| bounds.iter().find(|(id, _)| id == p).map(|(_, b)| *b);
                // A least size the application set replaces the panel's own.
                let min = panels
                    .iter()
                    .zip(&specs)
                    .map(|(p, s)| {
                        set(p)
                            .and_then(|b| b.min)
                            .map_or(s.min_size, |m| m.resolve(across))
                    })
                    .fold(0.0, f32::max);
                let max = panels
                    .iter()
                    .filter_map(|p| set(p).and_then(|b| b.max))
                    .map(|m| m.resolve(across))
                    .fold(f32::INFINITY, f32::min);
                let default = specs
                    .iter()
                    .filter_map(|s| s.default_size)
                    .fold(0.0, f32::max);
                (a, (min, default, max))
            })
            .collect();
        (wb.areas(), sizes, before)
    };
    let centre_min = app.centre_min_size().map(|m| m.resolve(across));
    let flipped = crate::layout_controls::side_bar_position(app).flipped();
    let zen = {
        let mut wb = app.workbench_mut();
        wb.flipped = flipped;
        wb.zen
    };
    app.set_drawing_areas(true);
    // The activity bar: a zone of the layout, beside the primary side bar
    // as VS Code's, of a fixed breadth (no sash) and refusing drops.
    let activity = !zen && app.zone_has_blocks(&crate::zones::ACTIVITY_BAR);
    {
        // Zones that are not docks refuse drops, before the docks resolve
        // the drag.
        let mut wb = app.workbench_mut();
        let zones = std::mem::take(&mut wb.no_drop);
        if let Some(space) = wb.space.as_mut() {
            space.no_drop_zones(zones);
        }
    }
    let (folded, over) = {
        let wb = app.workbench_mut();
        (wb.layout.folded.clone(), wb.layout.over.clone())
    };
    let kit = Kit::of(&ctx);
    let mut saved = shell_sizes(app, &ctx, ui.scope_id());
    // Side areas lying over the centre, open next to their strip: the
    // layout keeps their strip in the flow and draws them over the centre
    // (`ZoneState::over`), until their grip folds them.
    let open: BTreeSet<Area> = over
        .iter()
        .copied()
        .filter(|a| matches!(a, Area::StartSide | Area::EndSide) && !folded.contains(a))
        .collect();
    let shown_now: BTreeSet<Area> = Area::ALL
        .into_iter()
        .filter(|a| !zen && areas.contains(a) && visible_in(app, *a))
        .collect();
    let leaf = |area: Area| -> Zone {
        let name = area_name(area);
        let zone = Zone::leaf(name, Content::Dock(name.to_owned()));
        if area == Area::Center {
            return zone.with(|c| c.grow = true);
        }
        let (min, default, max) = sizes[&area];
        let min = min.max(48.0);
        let max = max.max(min);
        let default = if default > 0.0 {
            default
        } else {
            area_default(area, default).max(min)
        };
        let side = matches!(area, Area::StartSide | Area::EndSide);
        let strip = folded.contains(&area) || (side && over.contains(&area));
        zone.with(|c| {
            c.min = Some(Size::Points(min));
            c.max = max.is_finite().then_some(Size::Points(max));
            c.default = Some(Size::Points(default.clamp(min, max)));
            // A side area's sash dragged past half its least size folds it
            // to its strip, as in VS Code and JetBrains' IDEs.
            if side {
                c.fold = Fold::ToStrip;
            }
        })
        .with_state(|s| {
            s.size = saved.get(name).copied();
            if !shown_now.contains(&area) {
                s.hidden = true;
            } else if strip && side && open.contains(&area) {
                s.over = true;
            } else if strip && side {
                s.folded = true;
            } else if strip {
                // A folded bottom area leaves no strip.
                s.hidden = true;
            }
        })
    };
    let areas_tree = shell_tree(&panel_alignment(app).order(), flipped, centre_min, &leaf);
    let mut layout = ZoneLayout::new(with_activity_bar(
        areas_tree,
        flipped,
        activity.then(|| activity_bar_width(&ctx)),
    ));
    // One strip breadth for the layout: the widest folded side's.
    let strip = [Area::StartSide, Area::EndSide]
        .into_iter()
        .filter(|a| shown_now.contains(a) && (folded.contains(a) || over.contains(a)))
        .map(|a| strip_width(app, &ctx, &kit, a))
        .fold(kit.area_strip.data.width.max(1.0), f32::max);
    let mut style = LayoutStyle::from_egui(ui.style());
    style.strip = strip;
    style.key_step = SASH_STEP;
    style.large_key_step = SASH_STEP * 4.0;
    style.sash_grab = 2.0 * ctx.global_style().interaction.resize_grab_radius_side;
    style.sash_width = ui.visuals().widgets.noninteractive.bg_stroke.width.max(1.0);
    // As in VS Code and JetBrains' IDEs: a sash does not snap to sizes, a
    // side area folds by drag and a folded one drags out of its strip.
    style.snap = 0.0;
    style.strip_drag = true;
    let painter = ShellPainter {
        kit: &kit,
        open: &open,
        flipped,
    };
    // The flyout, while its panel is in a folded side area drawn with its
    // strip (the layout closes it otherwise), at its width.
    let flyout = app.workbench_mut().flyout.clone();
    // Another panel's flyout held last pass leaves the overlay stack
    // now: closed later, as no longer shown, it would close the new one
    // opened above it.
    let held = app.workbench_mut().flyout_held.take();
    if let Some(old) = held
        && flyout.as_ref() != Some(&old)
    {
        egui_pop_core::overlay::close(&ctx, Id::unique(flyout_overlay(&old)));
    }
    let flyout = flyout.and_then(|panel| {
        let area = app.workbench_mut().layout.area_of(&panel)?;
        let (width, ..) = flyout_width(app, &ctx, area, &panel, strip)?;
        layout.flyouts.insert(panel.as_str().to_owned(), width);
        Some((panel, area))
    });
    let mut viewer = ShellViewer {
        app,
        ctx: &ctx,
        kit: &kit,
        open: &open,
        areas: &areas,
        flipped,
        strip,
        flyout: flyout.as_ref().map(|(p, _)| p.clone()),
        flyout_closed: false,
    };
    let rect = ui.available_rect_before_wrap();
    let mut inner = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("egui_pop.shell")
            .max_rect(rect),
    );
    // A press on an overlay of the stack (a combo box's list or a context
    // menu opened in the flyout, a dialog above it) is not a press away:
    // the stack closes what it covers. Escape is the stack's too, the top
    // overlay first.
    let in_overlay = |pos: Pos2, layer: Option<egui::LayerId>| {
        egui_pop_core::overlay::hit_at(&ctx, pos, layer).is_some()
    };
    let laid = LayoutArea::new(&mut layout)
        .id(shell_id())
        .labels(shell_labels(&ctx))
        .style(style)
        .painter(&painter)
        .flyout(
            flyout
                .as_ref()
                .map(|(p, area)| Flyout::new(p.as_str(), area_name(*area))),
        )
        .flyout_dismiss(Dismiss::OUTSIDE_PRESS)
        .over_dismiss(Dismiss::NEVER)
        .inside(&in_overlay)
        // The zones lying over and the flyout at their rank: under the
        // overlays (cards, toasts) and the menus, whatever was clicked.
        .sublayer_of(egui_pop_core::layers::base(
            &ctx,
            egui_pop_core::layers::Rank::Over,
        ))
        .show_inside(&mut inner, &mut viewer);
    let flyout_closed = viewer.flyout_closed;
    // The shell covers `ui` without moving its cursor past the window's
    // bottom: past it (plus the item spacing), what the pass still
    // allocates in `ui` (the live regions, the overlay zones) would grow
    // `ui` below the window, and a frame drawn around `ui` with it.
    ui.expand_to_include_rect(rect);
    for zone in &laid.zones {
        let Some(area) = area_from(&zone.name.0) else {
            continue;
        };
        if zone.strip.is_some() {
            name_grip(&ctx, area, &zone.name.0, open.contains(&area));
        }
        if matches!(zone.shown, Shown::Open | Shown::Over) {
            app.workbench_mut().rects.insert(area, zone.rect);
            if area != Area::Center {
                grip(&ctx, &kit, area, zone.rect, flipped);
            }
        }
    }
    // What the user changed: sizes kept, folds asked through their step
    // (a plugin may veto; the layout is built again from the workbench's
    // state each pass, so a veto puts it back).
    let mut resized = false;
    // As egui kept a side panel's size from its first pass on, an open
    // area keeps the size it was first shown at: a panel made active
    // later with another default size does not resize it. An area its
    // least size widened keeps that size once the least goes.
    for zone in laid.zones.iter().filter(|z| z.shown == Shown::Open) {
        let Some(area) = area_from(&zone.name.0).filter(|a| *a != Area::Center) else {
            continue;
        };
        let along = if area == Area::Bottom {
            zone.rect.height()
        } else {
            zone.rect.width()
        };
        let kept = saved.get(&zone.name.0).copied();
        // What it would have without its least size: the size kept, else
        // its default (not raised to the least).
        let (_, default, _) = sizes[&area];
        let wanted = kept.unwrap_or_else(|| area_default(area, default));
        let size = if along > wanted + 0.5 && along <= zone.least + 0.5 {
            along
        } else if kept.is_none() {
            // Its default as the layout took it (within its bounds).
            layout
                .zone(&zone.name.0)
                .and_then(|z| z.constraints.default)
                .map_or(along, |d| d.points(0.0))
        } else {
            continue;
        };
        saved.insert(zone.name.0.clone(), size);
        resized = true;
    }
    for event in &laid.events {
        match event {
            LayoutEvent::Resized { zone, size } => {
                // An area lying over the centre, from where the pointer is on
                // its edge rather than from the layout's sum of moves, which
                // a pass egui discards and runs again would count twice.
                let lying = laid
                    .zones
                    .iter()
                    .find(|z| z.name == *zone && z.shown == Shown::Over);
                let size = lying
                    .and_then(|z| {
                        let area = area_from(&zone.0)?;
                        let edge = shell_id().with(("over", &zone.0)).with("edge");
                        let at = ctx.read_response(edge)?.interact_pointer_pos()?;
                        let width = if on_left(area, flipped) {
                            at.x - z.rect.left()
                        } else {
                            z.rect.right() - at.x
                        };
                        Some(width.clamp(z.least, z.most.max(z.least)))
                    })
                    .unwrap_or(*size);
                saved.insert(zone.0.clone(), size);
                resized = true;
            }
            LayoutEvent::Reset { zone } => {
                resized |= saved.remove(&zone.0).is_some();
            }
            LayoutEvent::Folded { zone } => {
                // Its sash dragged past its fold point: it keeps the size it
                // had before the drag, not the least size the drag reached.
                let Some(area) = area_from(&zone.0) else {
                    continue;
                };
                if let Some(size) = layout.zone(&zone.0).and_then(|z| z.state.size) {
                    saved.insert(zone.0.clone(), size);
                    resized = true;
                }
                app.fold_area(area, true);
            }
            LayoutEvent::Unfolded { zone } => {
                let Some(area) = area_from(&zone.0) else {
                    continue;
                };
                let edge = shell_id().with(("strip_edge", &zone.0));
                let dragged = ctx.read_response(edge).is_some_and(|r| r.drag_stopped());
                if dragged {
                    // Dragged out of its strip: an area lying over the
                    // centre is already open; one beside opens at the size
                    // it was dragged to.
                    if open.contains(&area) {
                        continue;
                    }
                    let size = layout.zone(&zone.0).and_then(|z| z.state.size);
                    if let Some(size) = size.filter(|_| !over.contains(&area)) {
                        saved.insert(zone.0.clone(), size);
                        resized = true;
                    }
                }
                app.fold_area(area, open.contains(&area));
            }
            LayoutEvent::FlyoutResized { key, size } => {
                // The width follows the edge for now, and is saved through
                // its step once the edge is let go (below). From where the
                // pointer is, not from the layout's sum of moves, which a
                // pass egui discards and runs again would count twice.
                let Some((panel, area)) = flyout.as_ref().filter(|(p, _)| p.as_str() == key) else {
                    continue;
                };
                let edge = shell_id().with(("flyout", panel.as_str())).with("edge");
                let at = ctx
                    .read_response(edge)
                    .and_then(|r| r.interact_pointer_pos());
                let width = match (at, laid.flyout) {
                    (Some(at), Some(rect)) if on_left(*area, flipped) => at.x - rect.left(),
                    (Some(at), Some(rect)) => rect.right() - at.x,
                    _ => *size,
                };
                if let Some((_, min, max)) = flyout_width(app, &ctx, *area, panel, strip) {
                    let live = flyout_live_id(panel);
                    ctx.data_mut(|d| d.insert_temp(live, width.clamp(min, max)));
                }
            }
            LayoutEvent::FlyoutClosed { key } => {
                let Some((panel, area)) = flyout.as_ref().filter(|(p, _)| p.as_str() == key) else {
                    continue;
                };
                let open_now = app.workbench_mut().flyout.as_ref() == Some(panel);
                if !open_now || flyout_closed {
                    continue;
                }
                if laid.flyout.is_none() {
                    // Its area is no longer folded with a strip: it is gone.
                    app.workbench_mut().flyout = None;
                    continue;
                }
                // A press away, or its strip's grip, which also unfolds the
                // area (the panel then shows in it).
                app.change_workbench(Change::Flyout(panel.clone(), false));
                let grip = shell_id().with(("grip", area_name(*area)));
                if ctx.read_response(grip).is_some_and(|r| r.clicked()) {
                    app.fold_area(*area, false);
                }
            }
            _ => {}
        }
    }
    if let Some((panel, _)) = &flyout {
        // The flyout's edge let go: its width is saved, through its step.
        let live = flyout_live_id(panel);
        let edge = shell_id().with(("flyout", panel.as_str())).with("edge");
        let dragging = ctx.read_response(edge).is_some_and(|r| r.dragged());
        if !dragging && let Some(width) = ctx.data_mut(|d| d.remove_temp::<f32>(live)) {
            app.change_workbench(Change::FlyoutSize(panel.clone(), width));
        }
    }
    if flyout_closed && let Some((panel, _)) = &flyout {
        // Closed by the overlay stack: Escape, or a press away.
        app.change_workbench(Change::Flyout(panel.clone(), false));
    }
    if resized {
        ctx.data_mut(|d| d.insert_persisted(sizes_id(), saved.clone()));
        app.workbench_mut().shell_sizes = Some(saved.clone());
    }
    if let Some(space) = app.workbench_mut().space.as_mut() {
        // The zones holding no dock refuse a tab's drop on the next pass.
        space.bind(&laid);
    }
    {
        let mut wb = app.workbench_mut();
        let drawn = laid.flyout.filter(|_| wb.flyout.is_some());
        wb.flyout_rect = drawn;
        wb.flyout_held = drawn.and(wb.flyout.clone());
    }
    if before.is_some() {
        drop_targets(app, ui, zen);
    }
    app.set_drawing_areas(false);
    if let Some(space) = app.workbench_mut().space.as_mut() {
        settle_bare_drops(space);
    }
    if !zen {
        crate::palettes::show(app, &ctx);
    }
    track_drag(app, &ctx, before);
    publish_hint(app, &ctx);
}

/// While a tab drags, the areas a dock does not draw as targets: a closed
/// side or bottom area along the areas' edge (its empty dock), an area
/// drawn bare over its whole rect (its bare target, where the tab joins
/// its tabs). Drawn last, so they win over the docks beneath, and the
/// closed areas' bands after the bare areas, so they win at the edges.
fn drop_targets(app: &AppHandle, ui: &mut Ui, zen: bool) {
    let mut targets: Vec<(&'static str, Rect)> = {
        let wb = app.workbench_mut();
        Area::ALL
            .into_iter()
            .filter(|a| !zen || *a == Area::Center)
            .filter_map(|area| {
                if wb.panels_in(area).is_empty() {
                    let rect = match area {
                        Area::Center => wb.rects.get(&area).copied()?,
                        _ => wb.hint_rect(area),
                    };
                    Some((area_name(area), rect))
                } else if wb.bare.contains(&area) {
                    Some((bare_target(area), *wb.rects.get(&area)?))
                } else {
                    None
                }
            })
            .collect()
    };
    targets.sort_by_key(|(dock, _)| !dock.ends_with(".bare"));
    let ctx = ui.ctx().clone();
    let kit = Kit::of(&ctx);
    let style = dock_style(ui.style(), &kit);
    let prepared = HashMap::new();
    for (dock, rect) in targets {
        let Some(mut space) = app.workbench_mut().take_space() else {
            return;
        };
        let mut target = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("egui_pop.drop_target", dock))
                .max_rect(rect),
        );
        let seen = Mutex::new(None);
        let painter = AreaPainter::new(app, &kit, false, &seen);
        let mut viewer = Viewer {
            app,
            prepared: &prepared,
        };
        DockArea::dock(&mut space, dock)
            .style(style.clone())
            .painter(&painter)
            .show_inside(&mut target, &mut viewer);
        let mut wb = app.workbench_mut();
        wb.put_space(space);
        wb.note_landing(seen);
    }
}

/// A folded side area's strip: the layout draws it, its grip a button
/// that unfolds the area (or folds it back while it lies over the centre);
/// the kit paints it (`ShellPainter`). Below the grip, an icon per panel
/// ([`strip_icon`]), the strip widened to hold them, unless the strip is
/// bare ([`AppHandle::bare_strip`]): `rect` is the whole strip.
fn strip_icons(app: &AppHandle, ui: &mut Ui, kit: &Kit, area: Area, rect: Rect, open: bool) {
    let grip = kit.area_strip.data.width.max(1.0);
    let panels = strip_panels(app, area);
    let t = egui_pop_core::theme::Tokens::of(ui.ctx());
    let cell = t.size.icon_inline + 2.0 * t.space.s;
    // The panels' icons, under the glyph, in tab order.
    let shown: Vec<PanelId> = {
        let wb = app.workbench_mut();
        wb.layout.active_in(area).into_iter().cloned().collect()
    };
    let mut top = rect.top() + grip * 2.5;
    for p in &panels {
        let cell_rect =
            Rect::from_min_size(egui::pos2(rect.left(), top), egui::vec2(rect.width(), cell));
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
}

/// A folded side area's strip breadth: the kit's, widened to an icon's
/// cell when it shows its panels' icons.
fn strip_width(app: &AppHandle, ctx: &egui::Context, kit: &Kit, area: Area) -> f32 {
    let grip = kit.area_strip.data.width.max(1.0);
    if strip_panels(app, area).is_empty() {
        return grip;
    }
    let t = egui_pop_core::theme::Tokens::of(ctx);
    grip.max(t.size.icon_inline + 2.0 * t.space.s)
}

/// The layout's labels, translated: its sashes ("Resize the start side
/// bar") and strip grips ("Unfold ...", "Fold ..."), `{zone}` being the
/// area's text ([`ShellViewer`]'s `title`).
fn shell_labels(ctx: &egui::Context) -> egui_stack::Labels {
    let template = |key| strings::tr_args(ctx, key, &[("area", "{zone}")]);
    let mut labels = egui_stack::Labels::default();
    labels.resize_zone = template(&txt::AREA_RESIZE).into();
    labels.unfold_zone = template(&txt::WORKBENCH_UNFOLD).into();
    labels.fold_zone = template(&txt::WORKBENCH_FOLD).into();
    labels
}

/// A strip's grip's tooltip: its name ("Unfold the start side bar",
/// "Fold ..." while it lies over the centre), which the layout gives it
/// for assistive technology ([`shell_labels`]).
fn name_grip(ctx: &egui::Context, area: Area, zone: &str, open: bool) {
    let name = strings::tr_args(
        ctx,
        if open {
            &txt::WORKBENCH_FOLD
        } else {
            &txt::WORKBENCH_UNFOLD
        },
        &[("area", &area_text(ctx, area))],
    );
    let id = shell_id().with(("grip", zone));
    if let Some(r) = ctx.read_response(id) {
        r.on_hover_text(name);
    }
}

/// The shell's layout: the side and bottom areas around the centre, each
/// a zone holding the area's dock. `order` is the order the areas are
/// peeled off the window ([`PanelAlignment::order`]): an area peeled
/// earlier spans the whole height (a side bar) or width (the bottom). The
/// rest grows; the centre's least width holds against the side areas.
fn shell_tree(
    order: &[Area],
    flipped: bool,
    centre_min: Option<f32>,
    leaf: &dyn Fn(Area) -> Zone,
) -> Zone {
    let Some((&first, rest)) = order.split_first() else {
        return leaf(Area::Center);
    };
    let side = matches!(first, Area::StartSide | Area::EndSide);
    let inner = shell_tree(rest, flipped, centre_min, leaf).with(|c| {
        c.grow = true;
        if side && let Some(least) = centre_min {
            c.min = Some(Size::Points(least));
        }
    });
    let name = format!("around.{}", area_name(first));
    if side {
        let children = if on_left(first, flipped) {
            vec![leaf(first), inner]
        } else {
            vec![inner, leaf(first)]
        };
        Zone::split(name, Dir::Horizontal, children)
    } else {
        Zone::split(name, Dir::Vertical, vec![inner, leaf(first)])
    }
}

/// The activity bar's zone (`Content::App`) and the shell's root, holding
/// it beside the areas' tree: on the primary side bar's side, `width`
/// wide (its least and largest size, so it has no sash), refusing drops;
/// hidden when `width` is `None`.
fn with_activity_bar(areas: Zone, flipped: bool, width: Option<f32>) -> Zone {
    let bar = Zone::leaf(ACTIVITY_ZONE, Content::App)
        .with(|c| {
            let width = Size::Points(width.unwrap_or(0.0));
            c.min = Some(width);
            c.max = Some(width);
            c.default = Some(width);
            c.drops = Drops::Refuse;
        })
        .with_state(|s| s.hidden = width.is_none());
    let areas = areas.with(|c| c.grow = true);
    let children = if flipped {
        vec![areas, bar]
    } else {
        vec![bar, areas]
    };
    Zone::split("shell", Dir::Horizontal, children)
}

/// The activity bar's zone's name.
const ACTIVITY_ZONE: &str = "activity_bar";

/// The activity bar's breadth: an icon and its padding.
fn activity_bar_width(ctx: &egui::Context) -> f32 {
    let t = egui_pop_core::theme::Tokens::of(ctx);
    t.size.icon_panel + 2.0 * t.space.s + 2.0 * t.space.xs
}

/// Draws the activity bar in its zone, as the egui side panel it was:
/// the side panel's frame, its line on the edge facing the areas.
fn activity_bar(app: &AppHandle, ui: &mut Ui, flipped: bool) {
    let rect = ui.max_rect();
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let line = stroke.width.round() as i8;
    let mut frame = egui::Frame::side_top_panel(ui.style());
    if flipped {
        frame.outer_margin.left = frame.outer_margin.left.saturating_add(line);
    } else {
        frame.outer_margin.right = frame.outer_margin.right.saturating_add(line);
    }
    let mut panel = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("egui_pop.activity_bar")
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let least = (rect.width() - frame.total_margin().sum().x).max(0.0);
    frame.show(&mut panel, |ui| {
        ui.set_min_height(ui.max_rect().height());
        ui.set_min_width(least);
        let region = focus::Region::new("activityBar", RANK_ACTIVITY_BAR);
        focus::region(ui, &region, |ui| {
            // One Tab stop, Up and Down between its items.
            let axis = egui_pop_core::tab::Axis::Vertical;
            egui_pop_core::tab::roving(ui, "activityBar", axis, |ui| {
                app.show_zone(ui, &crate::zones::ACTIVITY_BAR);
            });
        });
    });
    if stroke.width > 0.0 {
        // Just outside the frame, in the room kept for it.
        let margin = f32::from(line);
        let x = if flipped {
            rect.left() + margin - 0.5 * stroke.width
        } else {
            rect.right() - margin + 0.5 * stroke.width
        };
        ui.painter().vline(x, rect.y_range(), stroke);
    }
}

/// An area's default size: its panels' (`default`, 0 when none says),
/// else 240 for a side area and 200 for the bottom one.
fn area_default(area: Area, default: f32) -> f32 {
    if default > 0.0 {
        default
    } else if area == Area::Bottom {
        200.0
    } else {
        240.0
    }
}

/// The shell's layout's id: its sashes' and strips' ids derive from it.
fn shell_id() -> Id {
    Id::unique("egui_pop.shell")
}

/// Where the areas' sizes the user gave are kept in egui's memory (saved
/// with it, as egui's side panels kept theirs).
fn sizes_id() -> Id {
    Id::unique("egui_pop.shell.sizes")
}

/// The sizes the user gave the side and bottom areas, by zone name: kept
/// in the workbench, loaded once from egui's memory.
fn shell_sizes(app: &AppHandle, ctx: &egui::Context, scope: Id) -> BTreeMap<String, f32> {
    if let Some(sizes) = app.workbench_mut().shell_sizes.clone() {
        return sizes;
    }
    let saved = ctx.data_mut(|d| d.get_persisted::<BTreeMap<String, f32>>(sizes_id()));
    let sizes = migrate_sizes(saved, |area| old_panel_size(ctx, scope, area));
    app.workbench_mut().shell_sizes = Some(sizes.clone());
    sizes
}

/// The areas' sizes as saved, completed by what the areas' former egui
/// panels kept (`old`), for a session saved before the shell was laid out
/// by egui_stack: an area keeps the width it had.
fn migrate_sizes(
    saved: Option<BTreeMap<String, f32>>,
    old: impl Fn(Area) -> Option<f32>,
) -> BTreeMap<String, f32> {
    let mut sizes = saved.unwrap_or_default();
    for area in [Area::StartSide, Area::EndSide, Area::Bottom] {
        let name = area_name(area);
        if !sizes.contains_key(name)
            && let Some(size) = old(area)
        {
            sizes.insert(name.to_owned(), size);
        }
    }
    sizes
}

/// The size an area's egui panel had (its `PanelState`, under the `Ui`
/// the workbench was drawn in, `scope`): its width, the bottom's height.
fn old_panel_size(ctx: &egui::Context, scope: Id, area: Area) -> Option<f32> {
    let state = egui::containers::panel::PanelState::load(ctx, scope.with(dock_id(area)))?;
    let size = state.size();
    Some(if area == Area::Bottom { size.y } else { size.x })
}

/// Paints the shell's layout with the kit: a strip's grip `selected`
/// while its area lies over the centre.
struct ShellPainter<'a> {
    kit: &'a Kit,
    open: &'a BTreeSet<Area>,
    flipped: bool,
}

impl LayoutPainter for ShellPainter<'_> {
    fn sash(&self, painter: &egui::Painter, sash: &SashPaint<'_>) {
        // The line lies inside the area it resizes, on its side facing the
        // centre, where egui drew a side or bottom panel's separator.
        let inward =
            area_from(&sash.zone.0).map_or(egui::Vec2::ZERO, |area| inward(area, self.flipped));
        self.kit.paint_area_line(painter, sash, inward);
    }

    fn strip(&self, painter: &egui::Painter, strip: &StripPaint<'_>) {
        let Some(area) = area_from(&strip.zone.0) else {
            LayoutPainter::strip(self.kit, painter, strip);
            return;
        };
        let inner = if on_left(area, self.flipped) {
            Side::Right
        } else {
            Side::Left
        };
        let flags = strip.grip_state;
        let state = State::REST
            .with(State::HOVER, flags.contains(PartState::HOVERED))
            .with(State::ACTIVE, flags.contains(PartState::PRESSED))
            .with(State::FOCUS, flags.contains(PartState::FOCUSED))
            .with(State::SELECTED, self.open.contains(&area));
        self.kit.paint_area_strip(painter, strip.rect, state, inner);
    }

    /// A side area lying over the centre, or a flyout: the kit's
    /// `side-area` box, its shadow cast towards the centre, and the line on
    /// the centre's side, as a side panel's.
    fn flyout_frame(&self, painter: &egui::Painter, frame: &FlyoutPaint<'_>) {
        let Some(area) = area_from(&frame.zone.0) else {
            LayoutPainter::flyout_frame(self.kit, painter, frame);
            return;
        };
        let start = on_left(area, self.flipped);
        let side = &self.kit.side_area;
        let mut shadow = side.data.shadow;
        if !start {
            shadow.offset[0] = shadow.offset[0].saturating_neg();
        }
        let rect = frame.rect;
        if shadow != egui::Shadow::NONE {
            painter.add(shadow.as_shape(rect, egui::CornerRadius::ZERO));
        }
        let visuals = &painter.ctx().global_style().visuals;
        painter.rect_filled(rect, 0.0, visuals.panel_fill);
        if side.style(State::REST).border_width > 0.0 {
            let x = if start { rect.right() } else { rect.left() };
            painter.vline(x, rect.y_range(), visuals.widgets.noninteractive.bg_stroke);
        }
    }
}

/// The edge of a side or bottom area facing the centre, where its line is.
fn edge_toward_centre(area: Area, flipped: bool) -> Side {
    match area {
        Area::Bottom => Side::Top,
        _ if on_left(area, flipped) => Side::Right,
        _ => Side::Left,
    }
}

/// The way from an area's edge facing the centre into the area.
fn inward(area: Area, flipped: bool) -> egui::Vec2 {
    match edge_toward_centre(area, flipped) {
        Side::Left => egui::Vec2::X,
        Side::Right => -egui::Vec2::X,
        Side::Top => egui::Vec2::Y,
        Side::Bottom => -egui::Vec2::Y,
    }
}

/// Draws the shell's zones: each area in its frame, the strips' icons.
struct ShellViewer<'a> {
    app: &'a AppHandle,
    ctx: &'a egui::Context,
    kit: &'a Kit,
    open: &'a BTreeSet<Area>,
    areas: &'a [Area],
    flipped: bool,
    /// The strips' breadth.
    strip: f32,
    /// The panel shown as the flyout.
    flyout: Option<PanelId>,
    /// The overlay stack closed the flyout this pass.
    flyout_closed: bool,
}

impl ZoneViewer for ShellViewer<'_> {
    fn flyout_title(&mut self, flyout: &Flyout) -> String {
        match &self.flyout {
            Some(panel) => self.app.panel_title(self.ctx, panel),
            None => flyout.key.clone(),
        }
    }

    fn flyout(&mut self, ui: &mut Ui, flyout: &Flyout) {
        let (Some(panel), Some(area)) = (self.flyout.clone(), area_from(&flyout.zone.0)) else {
            return;
        };
        if !flyout_body(self.app, ui, self.kit, (area, self.strip), &panel) {
            self.flyout_closed = true;
        }
    }

    /// An area's zone is named after the area ("the centre"), not its key,
    /// for screen readers and tests: its pane, and its sash and grip ("Resize
    /// the start side bar"); a split keeps its name.
    fn title(&mut self, zone: &ZoneName) -> String {
        if zone.0 == ACTIVITY_ZONE {
            return strings::tr(self.ctx, &txt::AREA_ACTIVITY_BAR);
        }
        area_from(&zone.0).map_or_else(|| zone.0.clone(), |area| area_text(self.ctx, area))
    }

    fn ui(&mut self, ui: &mut Ui, zone: &ZoneName) {
        if zone.0 == ACTIVITY_ZONE {
            activity_bar(self.app, ui, self.flipped);
            return;
        }
        let Some(area) = area_from(&zone.0) else {
            return;
        };
        let app = self.app;
        if area == Area::Center {
            let mut frame = egui::Frame::central_panel(ui.style());
            if app.edge_to_edge() || app.frameless(Area::Center) {
                frame = frame.inner_margin(0.0);
            }
            frame.show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                if self.areas.contains(&Area::Center) && visible_in(app, Area::Center) {
                    draw_area(app, ui, Area::Center);
                }
            });
            return;
        }
        let pad = |p: f32| p.round().clamp(0.0, 127.0) as i8;
        let side = &self.kit.side_area.data;
        let margin = if app.frameless(area) {
            egui::Margin::ZERO
        } else {
            egui::Margin::symmetric(pad(side.padding_x), pad(side.padding_y))
        };
        // The room the line facing the centre takes, outside the frame, as
        // an egui panel reserved it.
        let room = self.kit.area_line_room(ui.style()).clamp(0.0, 127.0) as i8;
        let mut outer = egui::Margin::ZERO;
        match edge_toward_centre(area, self.flipped) {
            Side::Left => outer.left = room,
            Side::Right => outer.right = room,
            Side::Top => outer.top = room,
            Side::Bottom => outer.bottom = room,
        }
        egui::Frame::side_top_panel(ui.style())
            .inner_margin(margin)
            .outer_margin(outer)
            .show(ui, |ui| {
                // The area keeps its size; content narrower than it does
                // not shrink it.
                ui.set_min_size(ui.available_size());
                draw_area(app, ui, area);
            });
    }

    fn strip(&mut self, ui: &mut Ui, zone: &ZoneName) {
        let Some(area) = area_from(&zone.0) else {
            return;
        };
        // The `Ui` is the strip under its grip, a square at its top.
        let rest = ui.max_rect();
        let rect = Rect::from_min_max(egui::pos2(rest.left(), rest.top() - rest.width()), rest.max);
        ui.set_clip_rect(rect);
        strip_icons(
            self.app,
            ui,
            self.kit,
            area,
            rect,
            self.open.contains(&area),
        );
    }
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
/// indicator; none for a bare strip ([`AppHandle::bare_strip`]). Empty
/// in zen mode.
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
/// have something to draw, in tab order; none on a bare strip
/// ([`AppHandle::bare_strip`]).
fn strip_panels(app: &AppHandle, area: Area) -> Vec<PanelId> {
    if app.bare_strip(area) {
        return Vec::new();
    }
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
    r.widget_info(move || WidgetInfo::labeled(Role::Button, true, &label));
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
    if r.has_focus() {
        egui_pop_core::focus::own_ring(&r.ctx, r.id);
    }
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

// F6's order (`egui_pop_core::regions`); palettes, of one rank, go as drawn.
const RANK_ACTIVITY_BAR: u32 = 0;
const RANK_START_SIDE: u32 = 10;
const RANK_END_SIDE: u32 = 11;
const RANK_BOTTOM: u32 = 12;
const RANK_CENTER: u32 = 20;
pub(crate) const RANK_PALETTE: u32 = 30;
const RANK_FLYOUT: u32 = 40;
pub(crate) const RANK_STATUS_BAR: u32 = 50;

fn rank_of(area: Area) -> u32 {
    match area {
        Area::StartSide => RANK_START_SIDE,
        Area::EndSide => RANK_END_SIDE,
        Area::Bottom => RANK_BOTTOM,
        Area::Center => RANK_CENTER,
    }
}

/// An area's region's name.
fn area_region(area: Area) -> String {
    format!("region.area.{}", area_name(area))
}

/// Runs `workbench.next_region` or `workbench.previous_region`: see
/// [Moving between regions](self#moving-between-regions).
pub(crate) fn go_to_region(
    app: &AppHandle,
    origin: &egui_pop_model::commands::Origin,
    back: bool,
) -> Result<Json, String> {
    let ctx = app.egui();
    let agent = !matches!(origin, egui_pop_model::commands::Origin::User);
    if (agent && focus::text_input_owns_keyboard(&ctx)) || open_on_top(app, &ctx) {
        return Ok(json!({ "moved": false }));
    }
    let Some(hop) = egui_pop_core::regions::next(&ctx, back) else {
        return Ok(json!({ "moved": false }));
    };
    match hop.target {
        Some(id) => ctx.memory_mut(|m| {
            m.request_focus(id);
            m.move_focus(egui::FocusDirection::None);
        }),
        None => {
            // A panel of text alone holds the focus itself, its own stop.
            let area = Area::ALL
                .into_iter()
                .find(|a| area_region(*a) == hop.region);
            let active = area.map(|a| {
                let wb = app.workbench_mut();
                let panels: Vec<PanelId> = wb.layout.active_in(a).into_iter().cloned().collect();
                panels
            });
            let panel = active.and_then(|p| p.into_iter().find(|p| drawable(app, p)));
            if let Some(panel) = panel {
                app.focus_panel(&panel);
            }
        }
    }
    Ok(json!({ "moved": true, "region": hop.region }))
}

/// The panel beside `panel` in its tab group, the previous one with `back`,
/// wrapping; none for a panel alone in its group.
fn tab_beside(node: &Node, panel: &PanelId, back: bool) -> Option<PanelId> {
    match node {
        Node::Tabs { panels, .. } => {
            let i = panels.iter().position(|p| p == panel)?;
            let n = panels.len();
            (n > 1).then(|| panels[if back { (i + n - 1) % n } else { (i + 1) % n }].clone())
        }
        Node::Split { a, b, .. } => {
            tab_beside(a, panel, back).or_else(|| tab_beside(b, panel, back))
        }
        Node::Grid { children, .. } => children.iter().find_map(|c| tab_beside(c, panel, back)),
    }
}

/// The arrange commands: the row, column or grid around a panel's group
/// (`panel` in the arguments, or the focused one) laid out `as` says, a
/// grid with the arguments' `columns` if any. Nothing to arrange (no
/// panel focused, or its group alone in its area) is not an error.
/// The `workbench.move_to_window` command: `panel` (the focused one by
/// default) joins the window holding `window`, through the
/// `workbench.join_window` step.
pub(crate) fn move_to_window_command(app: &AppHandle, args: &Json) -> Result<Json, String> {
    let panel = match args.get("panel").and_then(Json::as_str) {
        Some(p) => Some(PanelId::new(p).map_err(|e| e.to_string())?),
        None => app.try_workbench().and_then(|wb| wb.focused.clone()),
    };
    let panel = panel.ok_or("no panel is focused: name one (`panel`)")?;
    let window = args
        .get("window")
        .and_then(Json::as_str)
        .ok_or("`window` is missing: a panel of the window")?;
    let window = PanelId::new(window).map_err(|e| e.to_string())?;
    match app.change_workbench(Change::JoinWindow(panel, window)) {
        Some(Outcome::Done(r)) => r,
        Some(Outcome::Vetoed { by, reason }) => Err(format!("stopped by `{by}`: {reason}")),
        _ => Ok(json!({ "queued": true })),
    }
}

pub(crate) fn arrange_command(app: &AppHandle, args: &Json, how: &str) -> Result<Json, String> {
    let panel = match args.get("panel").and_then(Json::as_str) {
        Some(p) => Some(PanelId::new(p).map_err(|e| e.to_string())?),
        None => app.try_workbench().and_then(|wb| wb.focused.clone()),
    };
    let Some(panel) = panel else {
        return Ok(json!({ "arranged": false }));
    };
    let mut step = json!({ "as": how });
    if let Some(columns) = args.get("columns") {
        step["columns"] = columns.clone();
    }
    let how = Arrangement::from_args(&step)?;
    let alone = {
        let layout = app.layout();
        !(layout.areas.iter()).any(|(_, n)| arrange(n, &panel, how).is_some())
    };
    if alone {
        return Ok(json!({ "arranged": false }));
    }
    match app.arrange(&panel, how) {
        Some(Outcome::Done(Ok(result))) => {
            let mut result = result;
            result["arranged"] = json!(true);
            Ok(result)
        }
        Some(Outcome::Done(Err(e))) => Err(e),
        Some(Outcome::Vetoed { by, reason }) => Err(format!("stopped by `{by}`: {reason}")),
        _ => Ok(json!({ "arranged": true, "queued": true })),
    }
}

/// Moves the keyboard to the tab beside the focused panel's, in its group, as
/// VS Code's Ctrl+Page Down and Up: that panel is shown and focused, through
/// the `workbench.focus` step.
pub(crate) fn go_to_tab(app: &AppHandle, back: bool) -> Result<Json, String> {
    let focused = app.try_workbench().and_then(|wb| wb.focused.clone());
    let Some(panel) = focused else {
        return Ok(json!({ "moved": false }));
    };
    let layout = app.layout();
    let target = (layout.areas.iter()).find_map(|(_, node)| tab_beside(node, &panel, back));
    let Some(target) = target else {
        return Ok(json!({ "moved": false }));
    };
    match app.focus_panel(&target) {
        Some(Outcome::Vetoed { by, reason }) => Err(format!("stopped by `{by}`: {reason}")),
        _ => Ok(json!({ "moved": true, "panel": target })),
    }
}

/// A menu or a popover is open, not the flyout: Escape comes first, F6 would
/// leave it open behind the focus.
fn open_on_top(app: &AppHandle, ctx: &egui::Context) -> bool {
    let flyout = app.workbench_mut().flyout.clone();
    let flyout = flyout.map(|p| Id::unique(flyout_overlay(&p)));
    egui::Popup::is_any_open(ctx)
        || egui_pop_core::overlay::stack(ctx)
            .iter()
            .any(|o| Some(o.id) != flyout)
}

/// What names a panel's flyout in the overlay stack.
fn flyout_overlay(panel: &PanelId) -> (&'static str, &str) {
    ("egui_pop.flyout", panel.as_str())
}

/// Where a flyout's width follows its edge while it is dragged.
fn flyout_live_id(panel: &PanelId) -> Id {
    Id::unique(("egui_pop.flyout.live", panel.as_str()))
}

/// A panel's flyout's width, then its least and largest: as its edge is
/// dragged, else as saved, else the panel's default, else its area's size,
/// else 240; `None` when the panel is not known. The largest leaves the
/// strips room (`strip` wide).
fn flyout_width(
    app: &AppHandle,
    ctx: &egui::Context,
    area: Area,
    panel: &PanelId,
    strip: f32,
) -> Option<(f32, f32, f32)> {
    let wb = app.workbench_mut();
    let spec = wb.spec(panel)?;
    let min = spec.min_size.max(48.0);
    let max = (wb.content.width() - 2.0 * strip).max(min);
    let width = ctx
        .data(|d| d.get_temp::<f32>(flyout_live_id(panel)))
        .or_else(|| wb.layout.flyouts.get(panel).copied())
        .or(spec.default_size)
        .or_else(|| wb.shell_sizes.as_ref()?.get(area_name(area)).copied())
        .unwrap_or(240.0)
        .clamp(min, max);
    Some((width, min, max))
}

/// A folded side area's panel shown alone over the centre, next to the
/// strip (the layout's flyout, `ui` its rect): a band with its title and a
/// pin button unfolding the area, then the panel. An overlay of the stack
/// (`overlay::hold`), so Escape (the top overlay first) or a click away
/// closes it; a combo box or a context menu opened in it is part of it.
/// Returns false once the stack closed it.
fn flyout_body(
    app: &AppHandle,
    ui: &mut Ui,
    kit: &Kit,
    (area, strip): (Area, f32),
    panel: &PanelId,
) -> bool {
    let ctx = ui.ctx().clone();
    let title = app.panel_title(&ctx, panel);
    let rect = ui.max_rect();
    // In the overlay stack: its rect takes in the resize edge, the strip
    // is its anchor (a click on another icon switches the flyout).
    let anchor = if app.workbench_mut().on_left(area) {
        Rect::from_x_y_ranges(rect.left() - strip..=rect.left(), rect.y_range())
    } else {
        Rect::from_x_y_ranges(rect.right()..=rect.right() + strip, rect.y_range())
    };
    let reach = ctx.global_style().interaction.resize_grab_radius_side;
    let spec = egui_pop_core::overlay::OverlaySpec::new(
        flyout_overlay(panel),
        &title,
        egui_pop_core::overlay::Kind::Popover,
    );
    let mut open = true;
    egui_pop_core::overlay::hold(
        &ctx,
        &spec,
        &mut open,
        Some(anchor),
        rect.expand2(egui::vec2(reach, 0.0)),
        || draw_flyout(app, ui, kit, panel, &title),
    )
    .is_some()
}

/// Draws an open flyout's content (`flyout_body`): its band, its panel.
fn draw_flyout(app: &AppHandle, ui: &mut Ui, kit: &Kit, panel: &PanelId, title: &str) {
    let side = &kit.side_area;
    let pad = egui::vec2(side.data.padding_x, side.data.padding_y);
    let t = egui_pop_core::theme::Tokens::of(ui.ctx());
    let body = ui.max_rect().shrink2(pad);
    let mut inner = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("egui_pop.flyout.inner")
            .max_rect(body),
    );
    inner.set_clip_rect(body);
    inner.set_min_size(inner.available_size());
    let region = focus::Region::new("region.flyout", RANK_FLYOUT);
    let pinned = focus::region(&mut inner, &region, |inner| {
        // The band: the title, then the pin button at the end.
        let pin = strings::tr_args(inner.ctx(), &txt::WORKBENCH_FLYOUT_PIN, &[("panel", title)]);
        let button = t.size.control_height.max(24.0);
        let header = kit.header_band.style(State::REST).text;
        // The pin first, from the end: a truncated title laid out
        // before it would take the whole row and push it out.
        let pinned = inner
            .horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let pinned = egui_pop_core::band::button(
                        ui,
                        button,
                        egui_pop_core::icon::Icon::Unlock,
                        &pin,
                        false,
                    )
                    .clicked();
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(title).strong().color(header))
                                .truncate()
                                .selectable(false),
                        );
                    });
                    pinned
                })
                .inner
            })
            .inner;
        inner.separator();
        app.show_panel(inner, panel);
        pinned
    });
    if pinned {
        app.change_workbench(Change::Flyout(panel.clone(), false));
        app.focus_panel(panel);
    }
}

/// How far an arrow moves an area's line, in points; Shift moves it four
/// times as far (as egui_stack's separators).
const SASH_STEP: f32 = 16.0;

/// A side area's resize grip (the kit's `area-grip`) at the middle of its
/// edge, above the panels; the layout's sash takes the drag. Nothing is
/// drawn while the kit's grip is transparent in every state (the default
/// kit).
fn grip(ctx: &egui::Context, kit: &Kit, area: Area, rect: Rect, flipped: bool) {
    let g = &kit.area_grip;
    if [State::REST, State::HOVER, State::ACTIVE]
        .iter()
        .all(|s| g.style(*s).fill.a() == 0)
    {
        return;
    }
    let (w, h, off) = (g.data.width, g.data.height, g.data.offset);
    let (centre, size) = match area {
        Area::StartSide | Area::EndSide if on_left(area, flipped) => (
            egui::pos2(rect.right() + off, rect.center().y),
            egui::vec2(w, h),
        ),
        Area::StartSide | Area::EndSide => (
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
        Id::unique("egui_pop.area_grips"),
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
pub(crate) fn drawable(app: &AppHandle, panel: &PanelId) -> bool {
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
    let region =
        focus::Region::new(area_region(area), rank_of(area)).holding(visible_in(app, area));
    focus::region(ui, &region, |ui| draw_dock(app, ui, area));
}

/// Draws an area's dock, or its one bare panel.
fn draw_dock(app: &AppHandle, ui: &mut Ui, area: Area) {
    let tabs = app.workbench_mut().panels_in(area);
    let drawable_tabs: Vec<&PanelId> = tabs.iter().filter(|p| drawable(app, p)).collect();
    let bare = drawable_tabs.len() == 1
        && app
            .workbench_mut()
            .spec(drawable_tabs[0])
            .is_some_and(|s| s.bare);
    if bare {
        let id = drawable_tabs[0].clone();
        app.show_panel(ui, &id);
        // A drop target while a tab drags (`drop_targets`).
        app.workbench_mut().bare.insert(area);
        return;
    }
    let Some(mut space) = app.workbench_mut().take_space() else {
        return;
    };
    let ctx = ui.ctx().clone();
    let kit = Kit::of(&ctx);
    let cache_id = TAB_CACHE.keyed(dock_id(area));
    let mut cache = cache_id.update(&ctx, std::mem::take);
    cache.prepare(app, &ctx, &kit, &tabs);
    let mut style = cache.dock_style(ui.style(), &kit);
    let frameless = app.frameless(area);
    if frameless {
        // The panels' bodies run to the area's edges, with no border
        // (`AreaPainter`); the separator's line alone sets them apart.
        let body = &mut style.tab.tab_body;
        body.inner_margin = egui::Margin::ZERO;
        body.stroke = egui::Stroke::NONE;
        body.corner_radius = egui::CornerRadius::ZERO;
    }
    let mut viewer = Viewer {
        app,
        prepared: &cache.tabs,
    };
    let seen = Mutex::new(None);
    let painter = AreaPainter::new(app, &kit, frameless, &seen);
    // egui_stack lays the tabs out and takes their input, and follows a
    // drag across the areas (the first dock drawn in a pass resolves it
    // for all of them); the kit measures and paints the tabs, bars,
    // bodies, separators and drop zones (`egui_pop_core::kit`).
    DockArea::dock(&mut space, area_name(area))
        .style(style)
        .painter(&painter)
        .show_close_buttons(false)
        .show_add_buttons(false)
        .show_leaf_close_all_buttons(false)
        .show_leaf_collapse_buttons(false)
        .show_inside(ui, &mut viewer);
    let bar = space
        .dock(area_name(area))
        .and_then(|tree| first_bar(tree, &kit, &cache.tabs));
    cache_id.set(&ctx, cache);
    let mut wb = app.workbench_mut();
    if let Some(bar) = bar {
        wb.bars.insert(area, bar);
    }
    wb.put_space(space);
    wb.note_landing(seen);
}

/// The tab bar of an area's first tabs (where `Layout::place` adds a
/// panel), as drawn this pass: `None` when hidden.
fn first_bar(
    tree: &egui_stack::Tree<PanelId>,
    kit: &Kit,
    prepared: &HashMap<PanelId, PreparedTab>,
) -> Option<TabBar> {
    let at = first_leaf(tree);
    if at.0 >= tree.len() {
        return None;
    }
    let leaf = tree[at].get_leaf()?;
    if leaf.tabs.is_empty() || leaf.tab_bar_hidden {
        return None;
    }
    let height = kit.tab_bar.data.height.max(kit.tab.data.height);
    let rect = Rect::from_min_size(leaf.rect.min, egui::vec2(leaf.rect.width(), height));
    // The right edge of its last tab: the tabs side by side as wide as
    // the kit measures them (`DockPainter::tab_width`), from where they
    // are scrolled to; tabs scrolled out end at the bar's right.
    let width: f32 = leaf
        .tabs
        .iter()
        .filter_map(|p| prepared.get(p))
        .map(|t| t.width)
        .sum();
    let end = (rect.left() + leaf.scroll + width)
        .max(rect.left())
        .min(rect.right());
    Some(TabBar {
        rect,
        end,
        tabs: leaf.tabs.len(),
    })
}

/// The kit as an area's dock painter: everything as the kit paints it,
/// except the panels' bodies in a frameless area, which have no border
/// there (`AppBuilder::frameless`). It notes where the drop zone of a
/// dragged tab is painted, for the drop hint; over a detached window,
/// which takes the drop, it paints none.
struct AreaPainter<'a> {
    kit: &'a Kit,
    frameless: bool,
    /// The dragged tab is over a detached window.
    quiet: bool,
    seen: &'a Mutex<Option<Rect>>,
}

impl<'a> AreaPainter<'a> {
    fn new(app: &AppHandle, kit: &'a Kit, frameless: bool, seen: &'a Mutex<Option<Rect>>) -> Self {
        let quiet = app.workbench_mut().window_drop.is_some();
        Self {
            kit,
            frameless,
            quiet,
            seen,
        }
    }
}

impl egui_stack::DockPainter for AreaPainter<'_> {
    fn tab_width(&self, tab: &egui_stack::TabMeasure<'_>) -> f32 {
        self.kit.tab_width(tab)
    }

    fn tab_bar(&self, painter: &egui::Painter, bar: &egui_stack::TabBarPaint<'_>) {
        self.kit.tab_bar(painter, bar);
    }

    fn tab(&self, painter: &egui::Painter, tab: &egui_stack::TabPaint<'_>) {
        self.kit.tab(painter, tab);
    }

    fn tab_bar_tail(&self, painter: &egui::Painter, tail: &egui_stack::TabBarTailPaint<'_>) {
        self.kit.tab_bar_tail(painter, tail);
    }

    fn tab_body(&self, painter: &egui::Painter, body: &egui_stack::TabBodyPaint<'_>) {
        if !self.frameless {
            self.kit.tab_body(painter, body);
        }
    }

    fn separator(&self, painter: &egui::Painter, separator: &egui_stack::SeparatorPaint<'_>) {
        self.kit.separator(painter, separator);
    }

    fn drop_target(&self, painter: &egui::Painter, target: &egui_stack::DropTargetPaint<'_>) {
        if !self.quiet {
            self.kit.drop_target(painter, target);
        }
    }

    fn drop_button(&self, painter: &egui::Painter, button: &egui_stack::DropButtonPaint<'_>) {
        if !self.quiet {
            self.kit.drop_button(painter, button);
        }
    }

    fn drop_preview(&self, painter: &egui::Painter, drop: &egui_stack::DropPaint<'_>) {
        if self.quiet {
            return;
        }
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(drop.rect);
        }
        self.kit.drop_preview(painter, drop);
    }

    fn section_band(&self, painter: &egui::Painter, band: &egui_stack::SectionBandPaint<'_>) {
        self.kit.section_band(painter, band);
    }

    fn section_boundary(
        &self,
        painter: &egui::Painter,
        boundary: &egui_stack::SectionBoundaryPaint<'_>,
    ) {
        self.kit.section_boundary(painter, boundary);
    }

    fn tool_item(&self, painter: &egui::Painter, item: &egui_stack::ToolItemPaint<'_>) {
        self.kit.tool_item(painter, item);
    }

    fn tool_more(&self, painter: &egui::Painter, more: &egui_stack::ToolMorePaint<'_>) {
        self.kit.tool_more(painter, more);
    }

    fn tool_separator(
        &self,
        painter: &egui::Painter,
        separator: &egui_stack::ToolSeparatorPaint<'_>,
    ) {
        self.kit.tool_separator(painter, separator);
    }
}

struct Viewer<'a> {
    app: &'a AppHandle,
    prepared: &'a HashMap<PanelId, PreparedTab>,
}

/// egui_stack's style for an area: its tab bars as high as the kit's
/// (the kit paints every part, `egui_pop_core::kit`).
fn dock_style(style: &egui::Style, kit: &Kit) -> egui_stack::Style {
    let mut s = egui_stack::Style::from_egui(style);
    s.tab_bar.height = kit.tab_bar.data.height.max(kit.tab.data.height);
    // The kit paints the bodies' borders (`dock-body`); their background
    // takes its corners at rest.
    let body = kit.dock_body.style(egui_pop_core::kit::State::REST);
    s.tab.tab_body.stroke = egui::Stroke::NONE;
    s.tab.tab_body.corner_radius =
        egui::CornerRadius::same(body.radius.round().clamp(0.0, 255.0) as u8);
    // Drops are shown on the node itself (the region the tab would take,
    // bordered and tinted), not on a cross of buttons. The kit paints
    // them (`dock-preview`); the style says the same for anything
    // egui_stack draws by itself.
    s.overlay.overlay_type = egui_stack::OverlayType::Zones;
    let zone = kit.dock_preview.style(egui_pop_core::kit::State::REST);
    s.overlay.zone_fill = zone.fill.gamma_multiply(kit.dock_preview.data.opacity);
    s.overlay.zone_stroke = egui::Stroke::new(zone.border_width, zone.border_color);
    s.overlay.zone_corner_radius =
        egui::CornerRadius::same(zone.radius.round().clamp(0.0, 255.0) as u8);
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

/// An area's prepared tabs and egui_stack style, kept across passes: a tab
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
    /// egui_stack's style and the egui style it was made from (kept, as
    /// the kit is).
    style: Option<(Arc<egui::Style>, Arc<egui_stack::Style>)>,
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

    fn dock_style(&mut self, style: &Arc<egui::Style>, kit: &Kit) -> egui_stack::Style {
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

impl TabViewer for Viewer<'_> {
    type Tab = PanelId;

    fn id(&mut self, tab: &mut PanelId) -> Id {
        Id::unique(("egui_pop.panel", tab.as_str()))
    }

    fn title(&mut self, tab: &mut PanelId) -> egui::WidgetText {
        // The prepared label: egui_stack lays nothing out again.
        match self.prepared.get(tab) {
            Some(p) => p.label.clone().into(),
            None => self.app.panel_title(&self.app.egui(), tab).into(),
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut PanelId) {
        self.app.show_panel(ui, tab);
    }

    fn on_tab_button(&mut self, tab: &mut PanelId, response: &egui::Response) {
        // egui_stack's tab buttons carry no name: give them their panel's.
        match self.prepared.get(tab) {
            Some(p) => egui_pop_core::a11y::name(response, egui::Role::Button, &p.title),
            None => {
                let title = self.app.panel_title(&response.ctx, tab);
                egui_pop_core::a11y::name(response, egui::Role::Button, &title);
            }
        }
        if response.has_focus() {
            // Painted in the kit's focus state (`DockPainter::tab`).
            egui_pop_core::focus::own_ring(&response.ctx, response.id);
        }
    }

    fn context_menu(&mut self, ui: &mut Ui, tab: &mut PanelId, _path: egui_stack::TabPath) {
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
        // The windows it may join, by their panels' titles: the way into
        // a window where a drag cannot reach it (`window_pointer`).
        let windows: Vec<(PanelId, Vec<PanelId>)> = if spec
            .refuses(&crate::mounts::panel_host(Place::Window))
            .is_none()
        {
            let wb = self.app.workbench_mut();
            (wb.layout.auxiliary.iter())
                .filter(|a| !a.holds(tab))
                .map(|a| (a.panel.clone(), a.panels().cloned().collect()))
                .collect()
        } else {
            Vec::new()
        };
        if !windows.is_empty() {
            ui.menu_button(strings::tr(&ctx, &txt::WORKBENCH_MOVE_TO_WINDOW), |ui| {
                for (window, panels) in windows {
                    let titles: Vec<String> = panels
                        .iter()
                        .map(|p| self.app.panel_title(&ctx, p))
                        .collect();
                    if ui.button(titles.join(", ")).clicked() {
                        self.app
                            .change_workbench(Change::JoinWindow(tab.clone(), window));
                        ui.close();
                    }
                }
            });
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
        // Detached panels get windows of their own (a step), not
        // egui_stack's floating windows.
        false
    }

    /// The panel's rules, as the drop zone shows them: an area it stays
    /// out of refuses it, with the reason. Moving inside its own area is
    /// always allowed; over a detached window, the window takes the drop.
    /// A detached window's own tabs (not the space's) move freely.
    fn dock_drop_allowed(&self, tab: &PanelId, drop: &DockDrop<'_>) -> DropVerdict {
        if drop.from.is_none() && drop.to.is_none() {
            return DropVerdict::Allow;
        }
        let Some((area, bare)) = drop.to.and_then(dock_area) else {
            return DropVerdict::Refuse(None);
        };
        let from = drop.from.and_then(dock_area).map(|(a, _)| a);
        let (spec, over_window) = {
            let wb = self.app.workbench_mut();
            let over = wb.window_drop.as_ref().is_some_and(|d| d.panel == *tab);
            (wb.spec(tab).cloned(), over)
        };
        if over_window {
            return DropVerdict::Refuse(None);
        }
        let refusal = match (&spec, from == Some(area)) {
            (Some(spec), false) => {
                let ctx = self.app.egui();
                let title = strings::panel_title(&ctx, spec.id.as_str(), &spec.title);
                refusal(&ctx, spec, &title, area)
                    .or_else(|| spec.refuses(&crate::mounts::panel_host(Place::Area(area))))
            }
            _ => None,
        };
        if let Some(landing) = Landing::of(drop.destination, bare) {
            self.app.workbench_mut().landing = Some(Seen {
                panel: tab.clone(),
                area,
                landing,
                refusal: refusal.clone(),
            });
        }
        match refusal {
            Some(why) => DropVerdict::Refuse(Some(why.into())),
            None => DropVerdict::Allow,
        }
    }
}

/// A tab dragged at the start of a pass: its panel, its area, and the
/// areas' trees before a drop.
type Before = (PanelId, Area, Vec<(Area, Node)>);

/// Follows a tab drag across the areas, which egui_stack's space resolves
/// (its zones, preview and veto): keeps the dragged tab for the detached
/// windows, which take a drop egui_stack cannot see (`windows`); once let
/// go, joins a detached window it was dropped on, and runs the
/// `workbench.move` step for a tab dropped in another area. A veto, or a
/// refusal, puts the areas back as they were before the drop.
fn track_drag(app: &AppHandle, ctx: &egui::Context, before: Option<Before>) {
    let dragged = app.workbench_mut().dragged(ctx);
    if let Some((panel, from, pointer)) = dragged {
        // The main window's pointer, which keeps following a drag past its
        // edges, for the detached windows.
        let at = ctx.input(|i| i.pointer.latest_pos()).unwrap_or(pointer);
        app.workbench_mut().dragging = Some((panel, from, at));
        return;
    }
    let ended = app.workbench_mut().dragging.take();
    // Over a detached window when it was let go (`windows`): it joins
    // that window's tabs.
    let over = app.workbench_mut().window_drop.take();
    if let Some((panel, _, _)) = &ended
        && let Some(drop) = over.filter(|d| d.panel == *panel)
    {
        let out = app.change_workbench(Change::JoinWindow(panel.clone(), drop.window));
        if let Some(Outcome::Done(Err(e))) = &out {
            app.workbench_refused(&format!("move {panel}"), e);
        }
        return;
    }
    let Some((panel, from, areas)) = before else {
        return;
    };
    let to = app.workbench_mut().area_in_space(&panel);
    let Some(to) = to.filter(|a| *a != from) else {
        return;
    };
    let out = app.change_workbench(Change::Move(panel.clone(), to));
    let undone = match &out {
        Some(Outcome::Done(Err(e))) => {
            app.workbench_refused(&format!("move {panel}"), e);
            true
        }
        Some(Outcome::Vetoed { .. }) => true,
        _ => false,
    };
    if undone {
        let mut wb = app.workbench_mut();
        wb.layout.areas = areas;
        wb.stale = true;
    }
}

/// The drop hint: where the dragged tab would land, as the space resolved
/// it this frame (the zone egui_stack painted, the panel's refusal), or a
/// palette dragged over an area it docks in, with the ghost of its tab
/// where it would go.
fn publish_hint(app: &AppHandle, ctx: &egui::Context) {
    let mut wb = app.workbench_mut();
    wb.hint = None;
    if let Some((panel, _, _)) = wb.dragging.clone() {
        // Over a detached window, the window shows where it goes.
        if wb.window_drop.is_some() {
            return;
        }
        let Some(seen) = wb.landing.take().filter(|s| s.panel == panel) else {
            return;
        };
        let rect = wb
            .landing_rect
            .take()
            .unwrap_or_else(|| wb.hint_rect(seen.area));
        let tabs_at = |path: egui_stack::NodePath| {
            wb.space
                .as_ref()
                .and_then(|s| s.state().leaf(path).ok())
                .map_or(0, |leaf| leaf.tabs.iter().filter(|p| **p != panel).count())
        };
        let (zone, index) = match seen.landing {
            Landing::At(i) => (DropZone::Center, Some(i)),
            Landing::End(path) => (DropZone::Center, Some(tabs_at(path))),
            Landing::Split(zone) => (zone, None),
            Landing::Whole { bare: false } => (DropZone::Center, Some(0)),
            Landing::Whole { bare: true } => {
                let first = wb.space.as_ref().and_then(|s| {
                    let surface = s.dock_surface(area_name(seen.area))?;
                    let tree = s.dock(area_name(seen.area))?;
                    Some(egui_stack::NodePath {
                        surface,
                        node: first_leaf(tree),
                    })
                });
                (DropZone::Center, Some(first.map_or(0, tabs_at)))
            }
        };
        wb.hint = Some(DropHint {
            panel,
            area: area_name(seen.area),
            rect,
            refusal: seen.refusal,
            zone,
            index,
            preview: None,
        });
        return;
    }
    // A palette over an area it docks in.
    let target = wb
        .palette_drag
        .clone()
        .filter(|_| wb.group_hint.is_none())
        .and_then(|d| {
            let edge = crate::palettes::magnet(ctx);
            wb.dock_at(d.at, edge).map(|to| (d.panel, to))
        });
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
        zone: DropZone::Center,
        index: Some(joined),
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
        Id::unique("egui_pop.workbench.drop"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_former_panels_sizes_carry_over_once() {
        // A session saved before the shell was laid out by egui_stack:
        // the areas' egui panels kept their outer rect.
        let ctx = egui::Context::default();
        let scope = Id::unique("scope");
        let store = |area: Area, rect: Rect| {
            let state = egui::containers::panel::PanelState { outer_rect: rect };
            ctx.data_mut(|d| d.insert_persisted(scope.with(dock_id(area)), state));
        };
        store(
            Area::StartSide,
            Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(312.0, 700.0)),
        );
        store(
            Area::Bottom,
            Rect::from_min_size(egui::pos2(0.0, 500.0), egui::vec2(900.0, 180.0)),
        );
        // The side's width, the bottom's height; none for the end side.
        let sizes = migrate_sizes(None, |a| old_panel_size(&ctx, scope, a));
        assert_eq!(sizes.get("start_side"), Some(&312.0));
        assert_eq!(sizes.get("bottom"), Some(&180.0));
        assert_eq!(sizes.get("end_side"), None);
        // What the layout saved since wins over the former panel's.
        let saved = BTreeMap::from([("start_side".to_owned(), 250.0)]);
        let sizes = migrate_sizes(Some(saved), |a| old_panel_size(&ctx, scope, a));
        assert_eq!(sizes.get("start_side"), Some(&250.0));
        assert_eq!(sizes.get("bottom"), Some(&180.0));
    }

    #[test]
    fn the_shell_tree_follows_the_alignment_and_the_side_bar_position() {
        let leaf = |area: Area| Zone::leaf(area_name(area), Content::Dock(area_name(area).into()));
        let order = |zone: &Zone| {
            let mut names = Vec::new();
            zone.visit(&mut |z| {
                if matches!(z.kind, egui_stack::layout::ZoneKind::Leaf { .. }) {
                    names.push(z.name.0.clone());
                }
            });
            names
        };
        // The default: the side bars down to the bottom, the bottom area
        // under the centre; leaves left to right, top to bottom.
        let tree = shell_tree(&PanelAlignment::Center.order(), false, Some(320.0), &leaf);
        assert_eq!(order(&tree), ["start_side", "center", "bottom", "end_side"]);
        let centre = tree.find("center").unwrap();
        assert!(centre.constraints.grow);
        // The middle, between the side bars, keeps the centre's least width.
        let middle = tree.find("around.end_side").unwrap();
        let inner = &middle.children()[0];
        assert_eq!(inner.constraints.min, Some(Size::Points(320.0)));
        // The side bar on the right: the start side last.
        let tree = shell_tree(&PanelAlignment::Center.order(), true, None, &leaf);
        assert_eq!(tree.children()[1].name.0, "start_side");
        // Justified: the bottom area across the whole width.
        let tree = shell_tree(&PanelAlignment::Justify.order(), false, None, &leaf);
        assert_eq!(tree.children()[1].name.0, "bottom");
    }
}
