//! The graph's minimap, after the image navigator: the whole graph
//! small, the part in view framed, the rest veiled.

use egui::{
    Key, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType, vec2,
};
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;

use super::{GraphState, GraphTokens, zoom_text};
use crate::navigator::NavigatorTokens;

/// The whole graph and the part in view, sharing the editor's
/// [`GraphState`]. A drag of the frame pans (Escape puts the view
/// back), a press elsewhere centres the view there, the wheel zooms, a
/// double click fits; focused, the arrows pan, `+` and `-` zoom, `0`
/// fits.
#[derive(Debug)]
pub struct GraphMinimap<'a> {
    state: &'a mut GraphState,
    size: Option<Vec2>,
    label: Option<String>,
}

/// Where the minimap draws the graph: a scale and an offset.
#[derive(Clone, Copy, Debug)]
struct Map {
    scale: f32,
    origin: Pos2,
    world: Rect,
}

impl Map {
    fn to_screen(self, p: Pos2) -> Pos2 {
        self.origin + (p - self.world.min) * self.scale
    }

    fn to_world(self, p: Pos2) -> Pos2 {
        self.world.min + (p - self.origin) / self.scale
    }

    fn rect(self, r: Rect) -> Rect {
        Rect::from_min_max(self.to_screen(r.min), self.to_screen(r.max))
    }
}

impl<'a> GraphMinimap<'a> {
    pub fn new(state: &'a mut GraphState) -> GraphMinimap<'a> {
        GraphMinimap {
            state,
            size: None,
            label: None,
        }
    }

    /// The size in points (default: the available width, two thirds as
    /// high).
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// The accessible name (default "Graph overview").
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Draws the minimap; `changed()` when it moved the view.
    pub fn show(self, ui: &mut Ui) -> Response {
        let GraphMinimap { state, size, label } = self;
        let size = size.unwrap_or_else(|| {
            let w = ui.available_width().clamp(120.0, 320.0);
            vec2(w, (w * 2.0 / 3.0).round())
        });
        let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let drawn = state.drawn.clone();
        let world = drawn.bounds.union(drawn.visible).expand(20.0);
        let inner = rect.shrink(4.0);
        let nav = NavigatorTokens::of(ui.ctx());
        let g = GraphTokens::of(ui.ctx());
        let radius = Tokens::of(ui.ctx()).radius.widget;
        ui.painter().rect_filled(rect, radius, nav.background);
        let mut changed = false;
        if world.is_positive() && drawn.visible.is_positive() {
            let scale = (inner.size() / world.size()).min_elem();
            let origin = inner.center() - world.size() * scale / 2.0;
            let map = Map {
                scale,
                origin,
                world,
            };
            changed = gestures(ui, &response, state, &map);
            let painter = ui.painter_at(rect);
            for (r, selected) in &drawn.boxes {
                let s = map.rect(*r);
                painter.rect_filled(s, 1.0, if *selected { g.node_selected } else { g.node });
                painter.rect_stroke(s, 1.0, Stroke::new(1.0, g.node_border), StrokeKind::Inside);
            }
            let frame = map.rect(state.drawn.visible).intersect(inner);
            // The veil over what is out of view.
            let veil = nav.veil.gamma_multiply(0.55);
            for r in [
                Rect::from_min_max(inner.min, egui::pos2(inner.max.x, frame.min.y)),
                Rect::from_min_max(egui::pos2(inner.min.x, frame.max.y), inner.max),
                Rect::from_min_max(
                    egui::pos2(inner.min.x, frame.min.y),
                    egui::pos2(frame.min.x, frame.max.y),
                ),
                Rect::from_min_max(
                    egui::pos2(frame.max.x, frame.min.y),
                    egui::pos2(inner.max.x, frame.max.y),
                ),
            ] {
                if r.is_positive() {
                    painter.rect_filled(r, 0.0, veil);
                }
            }
            painter.rect_stroke(frame, 0.0, Stroke::new(1.5, nav.frame), StrokeKind::Inside);
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                radius,
                Stroke::new(2.0, g.focus),
                StrokeKind::Outside,
            );
        }
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        if changed {
            response.mark_changed();
        }
        let label = label.unwrap_or_else(|| strings::tr(ui.ctx(), &crate::strings::GRAPH_MINIMAP));
        let value = strings::tr_args(
            ui.ctx(),
            &crate::strings::GRAPH_MINIMAP_VALUE,
            &[("zoom", &zoom_text(ui.ctx(), drawn.zoom))],
        );
        let enabled = ui.is_enabled();
        response.widget_info(|| {
            let mut info = WidgetInfo::labeled(WidgetType::Other, enabled, &label);
            info.current_text_value = Some(value.clone());
            info
        });
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_role(egui::accesskit::Role::ScrollView);
        });
        response
    }
}

/// The minimap's pointer, wheel and keys; whether the view moved.
fn gestures(ui: &Ui, response: &Response, state: &mut GraphState, map: &Map) -> bool {
    let id = response.id.with("grab");
    let before = state.view;
    let pointer = response.interact_pointer_pos();
    if response.drag_started()
        && let Some(p) = pointer
    {
        let frame = map.rect(state.drawn.visible);
        // A press outside the frame first brings it there.
        if !frame.contains(p) {
            state.center_on(map.to_world(p));
        }
        ui.data_mut(|d| d.insert_temp(id, before));
    }
    if response.dragged() {
        state.pan(response.drag_delta() / map.scale);
    }
    if response.drag_stopped()
        && ui.input(|i| i.key_pressed(Key::Escape))
        && let Some(start) = ui.data(|d| d.get_temp::<Rect>(id))
    {
        state.view = start;
    }
    if response.clicked()
        && let Some(p) = pointer
    {
        state.center_on(map.to_world(p));
    }
    if response.double_clicked() {
        state.fit();
    }
    let visible = state.drawn.visible;
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            state.zoom_by((scroll * 0.003).exp(), visible.center());
        }
    }
    if response.has_focus() {
        crate::change::lock_arrows(ui, response.id);
        let [x, y] = crate::change::arrows(ui);
        if x != 0.0 || y != 0.0 {
            state.pan(vec2(x * visible.width(), -y * visible.height()) * 0.1);
        }
        let (zin, zout, fit) = ui.input(|i| {
            (
                i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals),
                i.key_pressed(Key::Minus),
                i.key_pressed(Key::Num0),
            )
        });
        if zin {
            state.zoom_by(1.25, visible.center());
        }
        if zout {
            state.zoom_by(0.8, visible.center());
        }
        if fit {
            state.fit();
        }
    }
    state.view != before || state.fit
}
