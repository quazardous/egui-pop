//! What the editor keeps between frames: the view, the selection, the
//! collapsed nodes and folded groups (all the application's to save),
//! and the gestures under way.

use std::collections::BTreeSet;

use egui::{Pos2, Rect, Vec2};

use super::model::{Edge, Level, PortRef, Request, Side};

/// A wire being drawn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Wire {
    /// The port it starts from.
    pub from: PortRef,
    pub side: Side,
    /// Dragged by the pointer (released on a port, or nowhere), or
    /// picked by a click or a key (finished by a click on another port).
    pub by_pointer: bool,
    /// The wire dragged off an input, if that is how it began.
    pub detached: Option<Edge>,
}

/// Nodes being dragged.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Moving {
    /// The nodes and where they were.
    pub nodes: Vec<(String, [f32; 2])>,
    /// How far they have gone, in graph units.
    pub offset: Vec2,
}

/// What the last frame drew, for the minimap and for agents.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Drawn {
    /// Every box drawn (nodes and stacks) and whether it is selected.
    pub boxes: Vec<(Rect, bool)>,
    /// Around them all.
    pub bounds: Rect,
    /// The part of the graph in view.
    pub visible: Rect,
    /// Screen points per graph unit.
    pub zoom: f32,
}

/// A node editor's state: keep it with the graph's view.
#[derive(Clone, Debug)]
pub struct GraphState {
    /// The part of the graph to show (graph units); empty until the
    /// first frame fits the graph.
    pub view: Rect,
    /// The selected nodes.
    pub selection: BTreeSet<String>,
    /// Nodes drawn as their title bar only.
    pub collapsed: BTreeSet<String>,
    /// Groups folded into a stack in the ports and detail views.
    pub folded: BTreeSet<String>,
    /// Groups left unfolded in the overview (where groups fold by
    /// default).
    pub unfolded: BTreeSet<String>,
    pub(crate) wire: Option<Wire>,
    pub(crate) moving: Option<Moving>,
    /// A rubber band: where it began and where the pointer is.
    pub(crate) band: Option<[Pos2; 2]>,
    /// Where the canvas's menu was opened (graph units).
    pub(crate) menu_at: Option<Pos2>,
    pub(crate) level: Option<Level>,
    pub(crate) drawn: Drawn,
    pub(crate) requests: Vec<Request>,
    pub(crate) fit: bool,
}

impl Default for Drawn {
    fn default() -> Self {
        Drawn {
            boxes: Vec::new(),
            bounds: Rect::NOTHING,
            visible: Rect::NOTHING,
            zoom: 1.0,
        }
    }
}

impl Default for GraphState {
    fn default() -> Self {
        GraphState {
            view: Rect::NOTHING,
            selection: BTreeSet::new(),
            collapsed: BTreeSet::new(),
            folded: BTreeSet::new(),
            unfolded: BTreeSet::new(),
            wire: None,
            moving: None,
            band: None,
            menu_at: None,
            level: None,
            drawn: Drawn::default(),
            requests: Vec::new(),
            fit: false,
        }
    }
}

impl GraphState {
    pub fn new() -> GraphState {
        GraphState::default()
    }

    /// Fits the whole graph in view at the next frame.
    pub fn fit(&mut self) {
        self.fit = true;
    }

    /// Asks the editor to do something at its next frame (as
    /// [`GraphEditor::requests`](super::GraphEditor::requests)).
    pub fn request(&mut self, request: Request) {
        self.requests.push(request);
    }

    /// The level drawn last (`None` before the first frame).
    pub fn level(&self) -> Option<Level> {
        self.level
    }

    /// Screen points per graph unit, as drawn last.
    pub fn zoom(&self) -> f32 {
        self.drawn.zoom
    }

    /// The part of the graph in view at the last frame.
    pub fn visible(&self) -> Rect {
        self.drawn.visible
    }

    /// Whether a wire is being drawn.
    pub fn wiring(&self) -> bool {
        self.wire.is_some()
    }

    /// The view is usable (not empty, finite).
    pub(crate) fn has_view(&self) -> bool {
        self.view.is_finite() && self.view.width() > 0.0 && self.view.height() > 0.0
    }

    /// Moves the view by `d` graph units.
    pub fn pan(&mut self, d: Vec2) {
        if self.has_view() {
            self.view = self.view.translate(d);
        }
    }

    /// Centres the view on a point of the graph.
    pub fn center_on(&mut self, p: Pos2) {
        if self.has_view() {
            self.view = Rect::from_center_size(p, self.view.size());
        }
    }

    /// Zooms by `factor` (above 1 closer) around a point of the graph.
    pub fn zoom_by(&mut self, factor: f32, around: Pos2) {
        if self.has_view() && factor > 0.0 {
            let v = self.view;
            let min = around + (v.min - around) / factor;
            let max = around + (v.max - around) / factor;
            self.view = Rect::from_min_max(min, max);
        }
    }
}
