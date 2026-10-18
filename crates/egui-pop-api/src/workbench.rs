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
//! | `workbench.detach` | `{ "panel" }`: into a window of its own (its own scale) |
//! | `workbench.attach` | `{ "panel" }`: a detached panel docked back |
//! | `workbench.close_window` | `{ "panel" }`: a detached panel's window closes; the default docks it back, a veto keeps the window |
//!
//! Each declared panel gets the commands `<panel>.focus` and
//! `<panel>.toggle`.
//!
//! A panel may also float over the interface: a palette
//! (`PanelSpec::palette`) does when it opens, any floatable panel when
//! asked (its tab menu's Float). Floating panels are drawn by
//! [`crate::palettes`], with steps of their own (`palette.open`,
//! `palette.float`, `palette.dock`, `palette.move`, `palette.roll`,
//! `palette.pin`).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use egui::{Color32, Id, Pos2, Rect, Ui, ViewportBuilder, ViewportId};
use egui_dock::{DockArea, DockState, NodeIndex, Split, TabViewer};
use egui_pop_core::kit::{Kit, KitCx, Part, State};
use egui_pop_core::strings;
use egui_pop_model::ids::{PanelId, StepId, ZoneId};
use egui_pop_model::layout::{Floating, Layout, Node};
use egui_pop_model::panels::{Area, PanelSpec, Panels};
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
pub const STEP_DETACH: StepId = StepId::from_static("workbench.detach");
pub const STEP_ATTACH: StepId = StepId::from_static("workbench.attach");
pub const STEP_CLOSE_WINDOW: StepId = StepId::from_static("workbench.close_window");

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
    /// A floating panel docks in an area.
    Dock(PanelId, Area),
    /// A floating panel moves.
    MovePalette(PanelId, [f32; 2]),
    /// A floating panel rolls up to its header band (`true`) or down.
    Roll(PanelId, bool),
    /// A floating panel is pinned (`true`) or unpinned.
    Pin(PanelId, bool),
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

impl Change {
    pub(crate) fn step(&self) -> (StepId, Json) {
        match self {
            Change::Move(p, a) => (STEP_MOVE, json!({ "panel": p, "area": area_name(*a) })),
            Change::Show(p) => (STEP_SHOW, json!({ "panel": p })),
            Change::Hide(p) => (STEP_HIDE, json!({ "panel": p })),
            Change::Focus(p) => (STEP_FOCUS, json!({ "panel": p })),
            Change::Fold(a, f) => (STEP_FOLD, json!({ "area": area_name(*a), "folded": f })),
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
            Change::MovePalette(p, pos) => (
                crate::palettes::STEP_MOVE,
                json!({ "panel": p, "pos": pos }),
            ),
            Change::Roll(p, on) => (
                crate::palettes::STEP_ROLL,
                json!({ "panel": p, "rolled": on }),
            ),
            Change::Pin(p, on) => (
                crate::palettes::STEP_PIN,
                json!({ "panel": p, "pinned": on }),
            ),
        }
    }

    /// The event a palette's change publishes once done.
    pub(crate) fn event(&self) -> Option<&'static str> {
        Some(match self {
            Change::OpenPalette(_) => "palette.opened",
            Change::Float(..) => "palette.floated",
            Change::Dock(..) => "palette.docked",
            Change::MovePalette(..) => "palette.moved",
            Change::Roll(..) => "palette.rolled",
            Change::Pin(..) => "palette.pinned",
            _ => return None,
        })
    }

    /// The change a step's (possibly redirected) arguments ask for.
    pub(crate) fn from_args(step: &StepId, args: &Json) -> Result<Change, String> {
        if *step == STEP_FOLD {
            let name = args
                .get("area")
                .and_then(Json::as_str)
                .ok_or("`area` is missing")?;
            let area = area_from(name).ok_or_else(|| format!("no area `{name}`"))?;
            let folded = args.get("folded").and_then(Json::as_bool).unwrap_or(true);
            return Ok(Change::Fold(area, folded));
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
            "palette.roll" => Change::Roll(panel, flag("rolled")),
            "palette.pin" => Change::Pin(panel, flag("pinned")),
            "workbench.move" | "palette.dock" => {
                let name = args
                    .get("area")
                    .and_then(Json::as_str)
                    .ok_or("`area` is missing")?;
                let area = area_from(name).ok_or_else(|| format!("no area `{name}`"))?;
                if step.as_str() == "palette.dock" {
                    Change::Dock(panel, area)
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
    /// A palette being dragged: its panel, its place when the drag
    /// started, the pointer.
    pub palette_drag: Option<(PanelId, [f32; 2], Pos2)>,
    /// Where each floating panel was drawn last frame.
    pub palette_rects: BTreeMap<PanelId, Rect>,
}

impl Default for Workbench {
    fn default() -> Self {
        Self {
            specs: Vec::new(),
            zone_of: BTreeMap::new(),
            layout: Layout::default(),
            docks: BTreeMap::new(),
            rects: BTreeMap::new(),
            content: Rect::NOTHING,
            dragging: None,
            hint: None,
            focused: None,
            stale: true,
            taken: 0,
            templates: Vec::new(),
            palette_drag: None,
            palette_rects: BTreeMap::new(),
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
            self.layout.float(&spec.id, Floating::cascade(n));
        } else {
            self.layout.place(&spec.id, spec.area);
        }
    }

    /// Applies a change; refused when the panel's rules do not allow it.
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
                if let Some(why) = spec.refuses(*area) {
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
                if !spec.detachable {
                    return Err(format!("{} cannot be detached", spec.title));
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
                        self.layout.float(p, Floating::cascade(n));
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
                if !spec.floatable {
                    return Err(format!("{} cannot float", spec.title));
                }
                self.layout.float(p, *pos);
                json!({ "panel": p, "pos": pos })
            }
            Change::MovePalette(p, pos) => {
                let f = self
                    .layout
                    .floating_mut(p)
                    .ok_or_else(|| format!("`{p}` does not float"))?;
                f.pos = *pos;
                json!({ "panel": p, "pos": pos })
            }
            Change::Roll(p, on) => {
                let f = self
                    .layout
                    .floating_mut(p)
                    .ok_or_else(|| format!("`{p}` does not float"))?;
                f.rolled = *on;
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
                self.layout.folded.retain(|a| a != area);
                if *folded {
                    self.layout.folded.push(*area);
                }
                json!({ "area": area_name(*area), "folded": folded })
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
pub(crate) fn windows(app: &AppHandle, ctx: &egui::Context, app_name: &str) {
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
        ctx.show_viewport_immediate(window_id(&panel), builder, |ui, _| {
            let vctx = ui.ctx().clone();
            if let Some((from, to)) = egui_pop_core::scale::changed(&vctx) {
                app.publish(
                    "egui_pop",
                    None,
                    "scale.changed",
                    json!({ "viewport": panel, "from": from, "to": to }),
                );
            }
            // No title bar in full screen, as the main window's.
            let full_screen = vctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            if own && !full_screen {
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
    app.set_drawing_areas(true);
    if app.zone_has_blocks(&crate::zones::ACTIVITY_BAR) {
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
    let folded = app.workbench_mut().layout.folded.clone();
    for area in panel_alignment(app).order() {
        if !areas.contains(&area) || !visible_in(app, area) || folded.contains(&area) {
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
        let shown = panel
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
    app.set_drawing_areas(false);
    app.workbench_mut()
        .rects
        .insert(Area::Center, shown.response.rect);
    crate::palettes::show(app, &ctx);
    follow_drag(app, &ctx);
    drop_hint(app, &ctx);
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
        let cache_id = dock_id(area).with("egui_pop.tabs");
        let mut cache =
            ctx.data_mut(|d| std::mem::take(d.get_temp_mut_or_default::<TabCache>(cache_id)));
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
        ctx.data_mut(|d| d.insert_temp(cache_id, cache));
    }
    app.workbench_mut().put_dock(area, dock);
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
/// is laid out and measured again only when its title changes, and
/// everything when the kit (a theme install) or the scale changes.
#[derive(Clone, Default)]
struct TabCache {
    /// The kit (its address: a theme install makes a new one), the
    /// pixels per point and the strings provider the cache was made for.
    key: Option<(usize, u32, Option<usize>)>,
    tabs: HashMap<PanelId, PreparedTab>,
    style: Option<Arc<egui_dock::Style>>,
}

impl TabCache {
    fn prepare(&mut self, app: &AppHandle, ctx: &egui::Context, kit: &Arc<Kit>, tabs: &[PanelId]) {
        let key = (
            Arc::as_ptr(kit) as usize,
            ctx.pixels_per_point().to_bits(),
            strings::provider_id(ctx),
        );
        if self.key != Some(key) {
            *self = TabCache {
                key: Some(key),
                ..TabCache::default()
            };
        }
        self.tabs.retain(|p, _| tabs.contains(p));
        let mut font = None;
        for panel in tabs {
            let english = {
                let wb = app.workbench_mut();
                let english = wb.spec(panel).map_or(panel.as_str(), |s| s.title.as_str());
                if self.tabs.get(panel).is_some_and(|t| t.english == english) {
                    continue;
                }
                english.to_string()
            };
            let title = app.panel_title(ctx, panel);
            let font = font
                .get_or_insert_with(|| egui::TextStyle::Button.resolve(&ctx.global_style()))
                .clone();
            let label =
                ctx.fonts_mut(|f| f.layout_no_wrap(title.clone(), font, Color32::PLACEHOLDER));
            let width = kit.tab.measure(label.size()).x;
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

    fn dock_style(&mut self, style: &egui::Style, kit: &Kit) -> egui_dock::Style {
        (**self
            .style
            .get_or_insert_with(|| Arc::new(dock_style(style, kit))))
        .clone()
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
/// there.
fn drop_hint(app: &AppHandle, ctx: &egui::Context) {
    let mut wb = app.workbench_mut();
    wb.hint = None;
    // A tab over another area, or a palette over an area it docks in.
    let target = if let Some((panel, from, at)) = wb.dragging.clone() {
        wb.area_at(at).filter(|a| *a != from).map(|to| (panel, to))
    } else if let Some((panel, _, at)) = wb.palette_drag.clone() {
        let edge = egui_pop_core::theme::Tokens::of(ctx).palette.magnet;
        wb.dock_at(at, edge).map(|to| (panel, to))
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
    let title = strings::panel_title(ctx, spec.id.as_str(), &spec.title);
    let why = refusal(ctx, &spec, &title, to);
    wb.hint = Some(DropHint {
        panel,
        area: area_name(to),
        rect,
        refusal: why.clone(),
    });
    drop(wb);
    let t = egui_pop_core::theme::Tokens::of(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::new("egui_pop.workbench.drop"),
    ));
    let corner = t.radius.widget;
    match why {
        None => {
            painter.rect_filled(rect, corner, t.accent.subtle.gamma_multiply(0.5));
            painter.rect_stroke(
                rect,
                corner,
                egui::Stroke::new(1.0, t.accent.default),
                egui::StrokeKind::Inside,
            );
        }
        Some(why) => {
            painter.rect_filled(rect, corner, t.status.danger.gamma_multiply(0.15));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                why,
                egui::FontId::proportional(t.font.body),
                t.status.danger,
            );
        }
    }
}
