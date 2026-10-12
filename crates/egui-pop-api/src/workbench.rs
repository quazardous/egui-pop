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

use std::collections::BTreeMap;

use egui::{Id, Pos2, Rect, Ui, ViewportBuilder, ViewportId};
use egui_dock::{DockArea, DockState, NodeIndex, Split, TabViewer};
use egui_pop_model::ids::{PanelId, StepId, ZoneId};
use egui_pop_model::layout::{Layout, Node};
use egui_pop_model::panels::{Area, PanelSpec, Panels};
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::steps::Outcome;
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
        }
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
        Ok(match step.as_str() {
            "workbench.move" => {
                let name = args
                    .get("area")
                    .and_then(Json::as_str)
                    .ok_or("`area` is missing")?;
                let area = area_from(name).ok_or_else(|| format!("no area `{name}`"))?;
                Change::Move(panel, area)
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
    pub focused: Option<PanelId>,
    /// The layout changed outside the docks: project it again.
    stale: bool,
    /// Docks taken out to be drawn: the layout is not read back then.
    taken: usize,
    /// The application's named layouts (`Ctx::arrangement`): owner,
    /// name, layout.
    pub templates: Vec<(egui_pop_model::ids::OwnerId, String, Layout)>,
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
            focused: None,
            stale: true,
            taken: 0,
            templates: Vec::new(),
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

    /// Applies a change; refused when the panel's rules do not allow it.
    pub fn apply(&mut self, change: &Change) -> Result<Json, String> {
        self.read_back();
        let spec_of = |p: &PanelId| {
            self.spec(p)
                .cloned()
                .ok_or_else(|| format!("no panel `{p}`"))
        };
        let out = match change {
            Change::Move(p, area) => {
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
                    self.layout.place(p, spec.area);
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
                    self.layout.place(p, spec.area);
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
pub(crate) fn windows(app: &AppHandle, ctx: &egui::Context, app_title: &str) {
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
        let title = if app_title.is_empty() {
            spec.title.clone()
        } else {
            format!("{} — {app_title}", spec.title)
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
            if own {
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
    for area in [Area::Bottom, Area::StartSide, Area::EndSide] {
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
        let mut viewer = Viewer { app };
        DockArea::new(&mut dock)
            .id(dock_id(area))
            .show_close_buttons(false)
            .show_add_buttons(false)
            .show_leaf_close_all_buttons(false)
            .show_leaf_collapse_buttons(false)
            .show_inside(ui, &mut viewer);
    }
    app.workbench_mut().put_dock(area, dock);
}

struct Viewer<'a> {
    app: &'a AppHandle,
}

impl TabViewer for Viewer<'_> {
    type Tab = PanelId;

    fn id(&mut self, tab: &mut PanelId) -> Id {
        Id::new(("egui_pop.panel", tab.as_str()))
    }

    fn title(&mut self, tab: &mut PanelId) -> egui::WidgetText {
        self.app
            .workbench_mut()
            .spec(tab)
            .map_or_else(|| tab.to_string(), |s| s.title.clone())
            .into()
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
        let title = self
            .app
            .workbench_mut()
            .spec(tab)
            .map_or_else(|| tab.to_string(), |s| s.title.clone());
        egui_pop_core::a11y::name(response, egui::WidgetType::Button, &title);
    }

    fn context_menu(&mut self, ui: &mut Ui, tab: &mut PanelId, _path: egui_dock::NodePath) {
        let spec = self.app.workbench_mut().spec(tab).cloned();
        let Some(spec) = spec else { return };
        for area in Area::ALL {
            let allowed = spec.allows(area);
            let r = ui
                .add_enabled(
                    allowed,
                    egui::Button::new(format!("Move to {}", area.name())),
                )
                .on_disabled_hover_text(spec.refuses(area).unwrap_or_default());
            if r.clicked() {
                self.app.change_workbench(Change::Move(tab.clone(), area));
                ui.close();
            }
        }
        ui.separator();
        if spec.detachable && ui.button("Detach into a window").clicked() {
            self.app.change_workbench(Change::Detach(tab.clone()));
            ui.close();
        }
        if ui.button(format!("Close {}", spec.title)).clicked() {
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
    let wb = app.workbench_mut();
    let Some((panel, from, at)) = wb.dragging.clone() else {
        return;
    };
    let Some(to) = wb.area_at(at).filter(|a| *a != from) else {
        return;
    };
    let Some(spec) = wb.spec(&panel).cloned() else {
        return;
    };
    let rect = wb.hint_rect(to);
    drop(wb);
    let t = egui_pop_core::theme::Tokens::of(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::new("egui_pop.workbench.drop"),
    ));
    let corner = t.radius.widget;
    match spec.refuses(to) {
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
