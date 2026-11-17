//! Two to four images side by side (four in a square), with their zoom
//! and pan synced (an option), and a "best" marker.
//!
//! Each pane zooms around the pointer and pans as the pointer profile says
//! for the image canvas, through the same wheel path as the image viewport
//! (`egui_pop_core::pointer::view`: the wheel alone zooms with darktable's
//! profile, Ctrl and the wheel with the default one), pans by dragging,
//! and goes back to fit on a double click. Synced, every pane follows by
//! the same motion in points, so the image point under the pointer stays
//! under it in every pane, whatever each photo's shape.
//! The images come from the library's [`ThumbSource`]; the application
//! says which one is the best and hears when the user marks another
//! ([`CompareOutput::best`]).
//!
//! ```
//! use egui_pop_views::library::compare::{CompareState, CompareView, Pane};
//! use egui_pop_views::library::grid::{Thumb, ThumbSource};
//!
//! struct Thumbs;
//!
//! impl ThumbSource<u64> for Thumbs {
//!     fn thumb(&self, _: &u64, _: u32) -> Thumb {
//!         Thumb::Pending { average: None }
//!     }
//! }
//!
//! fn ui(ui: &mut egui::Ui, state: &mut CompareState, best: &mut Option<u64>) {
//!     let panes = [Pane::new(1u64, "IMG_0001"), Pane::new(2, "IMG_0002")];
//!     let out = CompareView::new("compare").best(*best).show(ui, state, &panes, &Thumbs);
//!     if let Some(key) = out.best {
//!         *best = Some(key);
//!     }
//! }
//! ```

use egui::{
    Align2, Color32, Id, PointerButton, Pos2, Rect, Response, Role, Sense, TextStyle, Ui, Vec2,
    WidgetInfo, pos2, vec2,
};
use egui_pop_core::icon::Icon;
use egui_pop_core::kit::{self, Corners, Kit, KitCx, Part, State};
use egui_pop_core::model::pointer::SCOPE_IMAGE;
use egui_pop_core::placeholder::SkeletonTokens;
use egui_pop_core::pointer::view::{self as view_input, ViewGestures, ViewWheel};
use egui_pop_core::strings::tr;
use egui_pop_core::theme::Tokens;

use super::grid::{Thumb, ThumbSource};
use super::paint::{self, LibraryTokens};
use crate::select::Key;
use crate::strings as s;

/// The most panes a comparison shows.
pub const MAX_PANES: usize = 4;
/// The highest zoom, relative to fit.
pub const MAX_ZOOM: f32 = 32.0;

/// One image to compare.
#[derive(Clone, Debug, PartialEq)]
pub struct Pane<K> {
    pub key: K,
    pub label: String,
}

impl<K> Pane<K> {
    pub fn new(key: K, label: impl Into<String>) -> Pane<K> {
        Pane {
            key,
            label: label.into(),
        }
    }
}

/// A view of one pane: its zoom (1 is fit) and where its image's centre
/// is from the pane's centre, in the pane's widths and heights times the
/// zoom (the same for every photo, so synced panes move together).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneView {
    pub zoom: f32,
    pub pan: Vec2,
}

impl Default for PaneView {
    fn default() -> Self {
        PaneView {
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

/// The comparison's state: one view shared by every pane when synced,
/// else one per pane.
#[derive(Clone, Debug, Default)]
pub struct CompareState {
    shared: PaneView,
    panes: [PaneView; MAX_PANES],
}

impl CompareState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pane `i`'s view (the shared one when synced).
    pub fn view(&self, i: usize, synced: bool) -> PaneView {
        if synced {
            self.shared
        } else {
            self.panes[i.min(MAX_PANES - 1)]
        }
    }

    /// Every pane back to fit.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn view_mut(&mut self, i: usize, synced: bool) -> &mut PaneView {
        if synced {
            &mut self.shared
        } else {
            &mut self.panes[i.min(MAX_PANES - 1)]
        }
    }
}

/// What [`CompareView::show`] returns.
#[derive(Debug)]
pub struct CompareOutput<K> {
    pub response: Response,
    /// The image the user marked as the best this pass.
    pub best: Option<K>,
}

/// The comparison view.
#[derive(Debug)]
#[must_use = "show it with `show`"]
pub struct CompareView<K> {
    id: String,
    synced: bool,
    best: Option<K>,
    height: Option<f32>,
}

impl<K: Key> CompareView<K> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            synced: true,
            best: None,
            height: None,
        }
    }

    /// Zoom and pan synced across the panes (on by default).
    pub fn synced(mut self, on: bool) -> Self {
        self.synced = on;
        self
    }

    /// The image marked as the best.
    pub fn best(mut self, key: Option<K>) -> Self {
        self.best = key;
        self
    }

    /// Its height (default: what is available).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn show(
        self,
        ui: &mut Ui,
        state: &mut CompareState,
        panes: &[Pane<K>],
        source: &dyn ThumbSource<K>,
    ) -> CompareOutput<K> {
        let t = Tokens::of(ui.ctx());
        let lt = LibraryTokens::of(ui.ctx());
        let kit = Kit::of(ui.ctx());
        let id = Id::unique(("egui_pop_views::compare", &self.id));
        let panes = &panes[..panes.len().min(MAX_PANES)];
        let height = self
            .height
            .unwrap_or_else(|| ui.available_height())
            .max(80.0);
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let name = tr(ui.ctx(), &s::COMPARE);
        egui_pop_core::a11y::name(&response, Role::Unknown, &name);
        let gap = t.space.s;
        let (cols, rows) = match panes.len() {
            0 | 1 => (1, 1),
            2 => (2, 1),
            3 => (3, 1),
            _ => (2, 2),
        };
        let pw = (rect.width() - gap * (cols as f32 - 1.0)) / cols as f32;
        let ph = (rect.height() - gap * (rows as f32 - 1.0)) / rows as f32;
        let small = TextStyle::Small.resolve(ui.style());
        // The image canvas's gestures, as in the image viewport.
        let gestures = ViewGestures::from_map(&egui_pop_core::pointer::map(ui.ctx()), SCOPE_IMAGE);
        let mut best = None;
        for (i, pane) in panes.iter().enumerate() {
            let (c, r) = (i % cols, i / cols);
            let area = Rect::from_min_size(
                rect.min + vec2(c as f32 * (pw + gap), r as f32 * (ph + gap)),
                vec2(pw, ph),
            );
            let painter = ui.painter_at(area);
            let is_best = self.best.as_ref() == Some(&pane.key);
            let cx = KitCx {
                painter: &painter,
                ppp: ui.ctx().pixels_per_point(),
            };
            let pane_state = State::REST.with(State::SELECTED, is_best);
            kit.compare_pane.paint(
                &cx,
                &Part {
                    rect: area,
                    room: area,
                    state: pane_state,
                    label: None,
                },
            );
            let pane_box = *kit.compare_pane.style(pane_state);
            let px = (area.width().max(area.height()) * ui.ctx().pixels_per_point()) as u32;
            let thumb = source.thumb(&pane.key, px);
            let size = match thumb {
                Thumb::Ready { size, .. } => size,
                _ => [3, 2],
            };
            let inner = area.shrink(t.space.xs).size();
            let fit = crate::library::fitted(area.shrink(t.space.xs), size);
            let v = state.view(i, self.synced);
            let drawn =
                Rect::from_center_size(area.center() + v.pan * inner * v.zoom, fit.size() * v.zoom);
            match thumb {
                Thumb::Ready { texture, .. } => {
                    painter.image(
                        texture,
                        drawn,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                Thumb::Pending { average } => {
                    let fill = average.unwrap_or_else(|| SkeletonTokens::of(ui.ctx()).fill);
                    painter.rect_filled(drawn, 0.0, fill);
                }
                Thumb::Failed => {
                    let side = t.size.icon_panel;
                    Icon::Warning.paint(
                        &painter,
                        Rect::from_center_size(area.center(), vec2(side, side)),
                        t.text.muted,
                    );
                }
            }
            // Zoom around the pointer, pan by dragging, fit on a double
            // click.
            let pr = ui.interact(area, id.with(("pane", i)), Sense::click_and_drag());
            egui_pop_core::pointer::drag_cursor(&pr, egui_pop_core::pointer::DragKind::Pan);
            let pane_name = pane.label.clone();
            pr.widget_info(move || WidgetInfo::labeled(Role::Unknown, true, &pane_name));
            let drag_id = id.with(("drag", i));
            if pr.double_clicked() {
                *state.view_mut(i, self.synced) = PaneView::default();
            } else if pr.dragged() && pans(&pr, &gestures, ui.input(|i| i.modifiers)) {
                // From where the press was, whole: no motion lost to the
                // drag threshold, no error piling up frame after frame.
                let (origin, now) =
                    ui.input(|i| (i.pointer.press_origin(), i.pointer.latest_pos()));
                if let (Some(origin), Some(now)) = (origin, now) {
                    let v = state.view_mut(i, self.synced);
                    let (start, from) = ui
                        .data(|d| d.get_temp::<(PaneView, Pos2)>(drag_id))
                        .unwrap_or((*v, origin));
                    ui.data_mut(|d| d.insert_temp(drag_id, (start, from)));
                    v.pan = start.pan + (now - from) / (inner * start.zoom);
                }
            } else {
                ui.data_mut(|d| d.remove::<(PaneView, Pos2)>(drag_id));
            }
            if let Some(m) = pr.hover_pos() {
                let steps = view_input::view_wheel(ui.ctx(), &gestures);
                let changed = !steps.is_empty();
                let v = state.view_mut(i, self.synced);
                for step in steps {
                    match step {
                        ViewWheel::Zoom { points, .. } => zoom_at(
                            v,
                            area.center(),
                            inner,
                            m,
                            view_input::wheel_factor(points) as f32,
                        ),
                        ViewWheel::Pan(d) => v.pan += d / (inner * v.zoom),
                    }
                }
                if changed && pr.dragged() {
                    // A drag going on starts again from here.
                    ui.data_mut(|d| d.insert_temp(drag_id, (*v, m)));
                }
            }
            painter.text(
                area.left_bottom() + vec2(t.space.s, -t.space.s),
                Align2::LEFT_BOTTOM,
                &pane.label,
                small.clone(),
                pane_box.text,
            );
            // The best marker: a button in the corner, the outline when set.
            let b = Rect::from_min_size(
                area.right_top() + vec2(-28.0 - t.space.xs, t.space.xs),
                vec2(28.0, 28.0),
            );
            let br = ui.interact(b, id.with(("best", i)), Sense::click());
            let badge = &kit.thumb_overlay_badge;
            badge.paint(
                &cx,
                &Part {
                    rect: b,
                    room: b,
                    state: State::REST,
                    label: None,
                },
            );
            // The best image's star is its pane's ring colour.
            let best_color = kit.compare_pane.style(State::SELECTED).ring_color;
            paint::star(
                &painter,
                b.center(),
                9.0,
                is_best.then_some(best_color),
                if br.hovered() {
                    badge.style(State::REST).text
                } else {
                    lt.star_empty
                },
            );
            let bname = tr(ui.ctx(), &s::MARK_BEST);
            br.widget_info(move || WidgetInfo::selected(Role::CheckBox, true, is_best, &bname));
            if br.clicked() && !is_best {
                best = Some(pane.key.clone());
            }
            // The ring again, over the image.
            kit::paint_ring(&cx, area, &pane_box, Corners::All);
        }
        CompareOutput { response, best }
    }
}

/// Whether a drag on a pane pans, as the profile says (any button it
/// binds a pan drag to).
fn pans(r: &Response, g: &ViewGestures, modifiers: egui::Modifiers) -> bool {
    [
        PointerButton::Primary,
        PointerButton::Middle,
        PointerButton::Secondary,
    ]
    .into_iter()
    .any(|b| r.dragged_by(b) && g.drag_pans(b, modifiers))
}

/// Zooms a view by `factor` keeping the point under `at` still; `unit`
/// is the pane's size, [`PaneView::pan`]'s unit.
fn zoom_at(v: &mut PaneView, center: Pos2, unit: Vec2, at: Pos2, factor: f32) {
    let z0 = v.zoom;
    let z1 = (z0 * factor).clamp(1.0, MAX_ZOOM);
    let c0 = center + v.pan * unit * z0;
    let c1 = at + (c0 - at) * (z1 / z0);
    v.zoom = z1;
    v.pan = if z1 <= 1.0 {
        Vec2::ZERO
    } else {
        (c1 - center) / (unit * z1)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let mut v = PaneView::default();
        let center = pos2(100.0, 100.0);
        let fit = vec2(200.0, 100.0);
        let at = pos2(150.0, 120.0);
        // The image point under `at`, in fitted units, before and after.
        let point = |v: &PaneView| (at - (center + v.pan * fit * v.zoom)) / (fit * v.zoom);
        let before = point(&v);
        zoom_at(&mut v, center, fit, at, 2.0);
        assert_eq!(v.zoom, 2.0);
        let after = point(&v);
        assert!((before - after).length() < 1e-5, "{before:?} {after:?}");
        zoom_at(&mut v, center, fit, at, 0.1);
        assert_eq!(v, PaneView::default(), "back to fit, centred");
    }
}
