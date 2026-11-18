//! One surface for one to many images: one image is a loupe (the whole
//! surface), several are a survey, side by side in a grid that grows with
//! their count (four in a square, six in three columns), with their zoom
//! and pan synced (an option), an active image and a "best" marker.
//!
//! Each pane zooms around the pointer and pans as the pointer profile says
//! for the image canvas, through the same wheel path as the image viewport
//! (`egui_pop_core::pointer::view`: the wheel alone zooms with darktable's
//! profile, Ctrl and the wheel with the default one), pans by dragging,
//! and goes back to fit on a double click. Synced, every pane follows by
//! the same motion in points, so the image point under the pointer stays
//! under it in every pane, whatever each photo's shape.
//!
//! The application owns the list of images: the surface says what the
//! user asked for, as [`Intent`]s in [`CompareOutput::intents`]: a pane
//! made active (a click, the arrow keys, the keyboard focus), images
//! dropped on the surface to add at a place, a pane taken out (its close
//! button, Delete or Backspace on the active pane, a pane dragged out and
//! let go over nothing), a pane dragged to another place. Drops go
//! through the application's drag session (`egui_pop_api::bus::drag`,
//! feature `api`), across windows too, with the payload the application
//! names ([`CompareView::carry`]). The images come from the library's
//! [`ThumbSource`]; the application says which one is the best and hears
//! when the user marks another ([`CompareOutput::best`]).
//!
//! ```
//! use egui_pop_views::library::compare::{CompareState, CompareView, Intent, Pane};
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
//! struct Compared {
//!     keys: Vec<u64>,
//!     active: Option<u64>,
//!     best: Option<u64>,
//! }
//!
//! fn ui(ui: &mut egui::Ui, state: &mut CompareState, c: &mut Compared) {
//!     let panes: Vec<Pane<u64>> = c
//!         .keys
//!         .iter()
//!         .map(|k| Pane::new(*k, format!("IMG_{k:04}")))
//!         .collect();
//!     let out = CompareView::new("compare")
//!         .best(c.best)
//!         .active(c.active)
//!         .removable(true)
//!         .show(ui, state, &panes, &Thumbs);
//!     if let Some(key) = out.best {
//!         c.best = Some(key);
//!     }
//!     for intent in out.intents {
//!         match intent {
//!             Intent::Active { key } => c.active = Some(key),
//!             Intent::Remove { key } => {
//!                 if let Some(i) = c.keys.iter().position(|k| *k == key) {
//!                     c.keys.remove(i);
//!                     state.removed(i);
//!                 }
//!             }
//!             _ => {}
//!         }
//!     }
//! }
//! ```

use egui::{
    Align2, Color32, EventFilter, Id, Key as EKey, PointerButton, Pos2, Rect, Response, Role,
    Sense, Stroke, StrokeKind, TextStyle, Ui, Vec2, WidgetInfo, pos2, vec2,
};
use egui_pop_core::icon::Icon;
use egui_pop_core::kit::{self, Corners, Kit, KitCx, Part, State};
use egui_pop_core::model::pointer::SCOPE_IMAGE;
use egui_pop_core::placeholder::SkeletonTokens;
use egui_pop_core::pointer::view::{self as view_input, ViewGestures, ViewWheel};
use egui_pop_core::pointer::{DragKind, drag_cursor};
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_core::theme::Tokens;

use super::grid::{Thumb, ThumbSource};
use super::paint::{self, LibraryTokens};
use crate::select::Key;
use crate::strings as s;

/// How many panes a comparison shows unless [`CompareView::max_panes`]
/// says otherwise: four, in a square.
pub const MAX_PANES: usize = 4;
/// The most panes a comparison can show ([`CompareView::max_panes`]).
pub const PANE_LIMIT: usize = 16;
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
/// else one per pane, by the pane's place.
#[derive(Clone, Debug, Default)]
pub struct CompareState {
    shared: PaneView,
    panes: Vec<PaneView>,
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
            self.panes.get(i).copied().unwrap_or_default()
        }
    }

    /// Every pane back to fit.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Pane `i` was taken out of the list: the views of those after it
    /// follow their panes.
    pub fn removed(&mut self, i: usize) {
        if i < self.panes.len() {
            self.panes.remove(i);
        }
    }

    /// A pane was put in the list at `i`, at fit: the views of those
    /// from there on follow their panes.
    pub fn inserted(&mut self, i: usize) {
        if i < self.panes.len() {
            self.panes.insert(i, PaneView::default());
        }
    }

    /// The pane at `from` moved to `to` (its place once moved).
    pub fn moved(&mut self, from: usize, to: usize) {
        let view = self.view(from, false);
        self.removed(from);
        self.inserted(to);
        if view != PaneView::default() {
            *self.view_mut(to, false) = view;
        }
    }

    fn view_mut(&mut self, i: usize, synced: bool) -> &mut PaneView {
        if synced {
            &mut self.shared
        } else {
            let i = i.min(PANE_LIMIT - 1);
            if self.panes.len() <= i {
                self.panes.resize(i + 1, PaneView::default());
            }
            &mut self.panes[i]
        }
    }
}

/// What the user asked of the comparison's list, which the application
/// owns: it edits its list (or refuses), and shows it again.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent<K> {
    /// This image becomes the active one: a click on its pane, the arrow
    /// keys, the keyboard focus on its pane.
    Active { key: K },
    /// These images, dropped on the surface, go in before the pane at
    /// `at` (the count of panes: after the last one). Images already
    /// compared are left out.
    Add { keys: Vec<K>, at: usize },
    /// This image goes out of the comparison: its pane's close button,
    /// Delete or Backspace on the active pane, its pane dragged out and
    /// let go over nothing.
    Remove { key: K },
    /// This image's pane, dragged and let go on the surface, goes to
    /// `to` (its place once moved).
    Move { key: K, to: usize },
}

/// What [`CompareView::show`] returns.
#[derive(Debug)]
#[non_exhaustive]
pub struct CompareOutput<K> {
    pub response: Response,
    /// The image the user marked as the best this pass.
    pub best: Option<K>,
    /// What the user asked of the list this pass, in order.
    pub intents: Vec<Intent<K>>,
    /// Where each pane was drawn, in the order of the panes shown.
    pub panes: Vec<Rect>,
}

/// The comparison view.
#[must_use = "show it with `show`"]
pub struct CompareView<K> {
    id: String,
    synced: bool,
    best: Option<K>,
    active: Option<K>,
    height: Option<f32>,
    max_panes: usize,
    removable: bool,
    #[cfg(feature = "api")]
    carry: Option<carry::Carry<K>>,
}

impl<K: std::fmt::Debug> std::fmt::Debug for CompareView<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("CompareView");
        d.field("id", &self.id)
            .field("synced", &self.synced)
            .field("best", &self.best)
            .field("active", &self.active)
            .field("height", &self.height)
            .field("max_panes", &self.max_panes)
            .field("removable", &self.removable);
        #[cfg(feature = "api")]
        d.field("carry", &self.carry.as_ref().map(|c| c.kind));
        d.finish()
    }
}

/// The panes' places for `n` panes in `rect`, `gap` apart, row by row:
/// one fills it, two and three side by side, four in a square, more in
/// the grid that shows a 3:2 image the largest.
pub fn layout(n: usize, rect: Rect, gap: f32) -> Vec<Rect> {
    let (cols, rows) = grid(n, rect.size(), gap);
    let pw = ((rect.width() - gap * (cols as f32 - 1.0)) / cols as f32).max(0.0);
    let ph = ((rect.height() - gap * (rows as f32 - 1.0)) / rows as f32).max(0.0);
    (0..n)
        .map(|i| {
            let (c, r) = (i % cols, i / cols);
            Rect::from_min_size(
                rect.min + vec2(c as f32 * (pw + gap), r as f32 * (ph + gap)),
                vec2(pw, ph),
            )
        })
        .collect()
}

/// The grid's columns and rows for `n` panes.
fn grid(n: usize, size: Vec2, gap: f32) -> (usize, usize) {
    match n {
        0 | 1 => (1, 1),
        2 => (2, 1),
        3 => (3, 1),
        4 => (2, 2),
        _ => {
            let shown = |cols: usize| {
                let rows = n.div_ceil(cols);
                let w = (size.x - gap * (cols as f32 - 1.0)) / cols as f32;
                let h = (size.y - gap * (rows as f32 - 1.0)) / rows as f32;
                // A 3:2 image fitted in the pane.
                let (iw, ih) = (w.min(h * 1.5), h.min(w / 1.5));
                iw.max(0.0) * ih.max(0.0)
            };
            let mut best = 2;
            for cols in 3..=n {
                if shown(cols) > shown(best) {
                    best = cols;
                }
            }
            (best, n.div_ceil(best))
        }
    }
}

impl<K: Key> CompareView<K> {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            synced: true,
            best: None,
            active: None,
            height: None,
            max_panes: MAX_PANES,
            removable: false,
            #[cfg(feature = "api")]
            carry: None,
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

    /// The active image, outlined; the arrow keys move from it, Delete
    /// takes it out ([`CompareView::removable`]).
    pub fn active(mut self, key: Option<K>) -> Self {
        self.active = key;
        self
    }

    /// Its height (default: what is available).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// How many panes it shows at most ([`MAX_PANES`] by default, at
    /// most [`PANE_LIMIT`]); a drop that would go past it is refused.
    pub fn max_panes(mut self, n: usize) -> Self {
        self.max_panes = n.clamp(1, PANE_LIMIT);
        self
    }

    /// Panes the user can take out (off by default): a close button on
    /// each, Delete and Backspace on the active one, each an
    /// [`Intent::Remove`].
    pub fn removable(mut self, on: bool) -> Self {
        self.removable = on;
        self
    }

    /// Images carried in and out by the application's drag session
    /// (`egui_pop_api::bus::drag`), across its windows too: a drag of a
    /// `T` let go on the surface adds the images `unpack` finds in it
    /// ([`Intent::Add`], none: refused), a pane's caption drags its image
    /// out as `pack` makes it, under `kind` (`photos`), and a pane let go
    /// over nothing goes out ([`Intent::Remove`]), let go on the surface
    /// moves ([`Intent::Move`]).
    #[cfg(feature = "api")]
    pub fn carry<T: std::any::Any + Send + Sync>(
        mut self,
        kind: &'static str,
        pack: fn(Vec<K>) -> T,
        unpack: fn(&T) -> Option<Vec<K>>,
    ) -> Self {
        self.carry = Some(carry::Carry::new(kind, pack, unpack));
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
        let panes = &panes[..panes.len().min(self.max_panes)];
        let height = self
            .height
            .unwrap_or_else(|| ui.available_height())
            .max(80.0);
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        // The surface takes the keyboard: the arrows move the active
        // pane, Delete takes it out.
        let surface = ui.interact(rect, id, Sense::focusable_noninteractive());
        let name = tr(ui.ctx(), &s::COMPARE);
        egui_pop_core::a11y::name(&surface, Role::List, &name);
        let gap = t.space.s;
        let areas = layout(panes.len(), rect, gap);
        let active = self
            .active
            .as_ref()
            .and_then(|a| panes.iter().position(|p| p.key == *a));
        let small = TextStyle::Small.resolve(ui.style());
        // The image canvas's gestures, as in the image viewport.
        let gestures = ViewGestures::from_map(&egui_pop_core::pointer::map(ui.ctx()), SCOPE_IMAGE);
        let mut best = None;
        let mut intents = Vec::new();
        let mut focus_within = surface.has_focus();
        #[cfg(feature = "api")]
        let carrying: Option<K> = ui
            .data(|d| d.get_temp::<String>(id.with("carrying")))
            .and_then(|k| serde_json::from_str(&k).ok());
        if panes.is_empty() {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                tr(ui.ctx(), &s::COMPARE_EMPTY),
                TextStyle::Body.resolve(ui.style()),
                t.text.muted,
            );
        }
        for (i, (pane, &area)) in panes.iter().zip(&areas).enumerate() {
            let painter = ui.painter_at(area);
            let is_best = self.best.as_ref() == Some(&pane.key);
            let is_active = active == Some(i);
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
            // click; a click makes the pane active.
            let pr = ui.interact(area, id.with(("pane", i)), Sense::click_and_drag());
            drag_cursor(&pr, DragKind::Pan);
            let pane_name = pane.label.clone();
            pr.widget_info(move || {
                WidgetInfo::selected(Role::ListItem, true, is_active, &pane_name)
            });
            focus_within |= pr.has_focus();
            if (pr.clicked() || pr.gained_focus()) && !is_active {
                intents.push(Intent::Active {
                    key: pane.key.clone(),
                });
            }
            if pr.clicked() {
                surface.request_focus();
            }
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
            // The caption: the image's name, and the handle a pane is
            // carried out by (with a drag session).
            let caption = Rect::from_min_max(
                pos2(area.left(), area.bottom() - small.size - 2.0 * t.space.s),
                area.right_bottom(),
            );
            painter.text(
                area.left_bottom() + vec2(t.space.s, -t.space.s),
                Align2::LEFT_BOTTOM,
                &pane.label,
                small.clone(),
                pane_box.text,
            );
            #[cfg(feature = "api")]
            if let Some(c) = &self.carry {
                let hr = ui.interact(caption, id.with(("carry", i)), Sense::drag());
                drag_cursor(&hr, DragKind::Carry);
                let hname = tr_args(ui.ctx(), &s::ITEM_MOVE, &[("name", &pane.label)]);
                hr.widget_info(move || WidgetInfo::labeled(Role::Button, true, &hname));
                if hr.drag_started() {
                    (c.start)(ui.ctx(), vec![pane.key.clone()]);
                    let key = serde_json::to_string(&pane.key).unwrap_or_default();
                    ui.data_mut(|d| d.insert_temp(id.with("carrying"), key));
                }
                if hr.dragged() {
                    egui_pop_core::pointer::hold_drag_cursor(ui.ctx(), DragKind::Carry);
                }
                if carrying.as_ref() == Some(&pane.key) {
                    // The pane being carried, dimmed.
                    painter.rect_filled(area, 0.0, t.surface.canvas.gamma_multiply(0.6));
                }
            }
            #[cfg(not(feature = "api"))]
            let _ = caption;
            // The best marker (with two panes or more): a button in the
            // corner, the outline when set.
            let side = 28.0;
            let mut corner = area.right_top() + vec2(-side - t.space.xs, t.space.xs);
            if panes.len() > 1 {
                let b = Rect::from_min_size(corner, vec2(side, side));
                corner.x -= side + t.space.xs;
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
            }
            if self.removable {
                let x = Rect::from_min_size(corner, vec2(side, side));
                let xr = ui.interact(x, id.with(("remove", i)), Sense::click());
                let badge = &kit.thumb_overlay_badge;
                badge.paint(
                    &cx,
                    &Part {
                        rect: x,
                        room: x,
                        state: State::REST,
                        label: None,
                    },
                );
                Icon::Close.paint(
                    &painter,
                    Rect::from_center_size(x.center(), vec2(14.0, 14.0)),
                    if xr.hovered() {
                        badge.style(State::REST).text
                    } else {
                        lt.star_empty
                    },
                );
                let xname = tr_args(ui.ctx(), &s::COMPARE_REMOVE, &[("name", &pane.label)]);
                xr.widget_info(move || WidgetInfo::labeled(Role::Button, true, &xname));
                if xr.clicked() {
                    intents.push(Intent::Remove {
                        key: pane.key.clone(),
                    });
                }
            }
            // The ring again, over the image.
            kit::paint_ring(&cx, area, &pane_box, Corners::All);
            if is_active && panes.len() > 1 {
                painter.rect_stroke(
                    area.shrink(1.0),
                    0.0,
                    Stroke::new(2.0, t.focus_ring),
                    StrokeKind::Inside,
                );
            }
        }
        if focus_within {
            keys(ui, id, panes, &areas, active, self.removable, &mut intents);
        }
        #[cfg(feature = "api")]
        if let Some(c) = &self.carry {
            carry::surface(
                ui,
                c,
                &self.id,
                id,
                rect,
                &areas,
                panes,
                self.max_panes,
                carrying,
                &mut intents,
            );
        }
        if !intents.is_empty() {
            ui.ctx().request_repaint();
        }
        CompareOutput {
            response,
            best,
            intents,
            panes: areas,
        }
    }
}

/// The surface's keys, while it or one of its panes has the keyboard:
/// the arrows, Home and End move the active pane; Delete and Backspace
/// take it out.
fn keys<K: Key>(
    ui: &Ui,
    id: Id,
    panes: &[Pane<K>],
    areas: &[Rect],
    active: Option<usize>,
    removable: bool,
    intents: &mut Vec<Intent<K>>,
) {
    if panes.is_empty() {
        return;
    }
    // The arrows are the surface's, whether it or a pane has the focus.
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            m.focused().unwrap_or(id),
            EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                ..Default::default()
            },
        )
    });
    let n = panes.len();
    let at = active.unwrap_or(0);
    let cols = areas
        .iter()
        .take_while(|r| r.top() == areas[0].top())
        .count()
        .max(1);
    let moved = ui.input_mut(|i| {
        let none = egui::Modifiers::NONE;
        if i.consume_key(none, EKey::ArrowLeft) {
            Some(at.saturating_sub(1))
        } else if i.consume_key(none, EKey::ArrowRight) {
            Some((at + 1).min(n - 1))
        } else if i.consume_key(none, EKey::ArrowUp) {
            Some(at.saturating_sub(cols))
        } else if i.consume_key(none, EKey::ArrowDown) {
            Some((at + cols).min(n - 1))
        } else if i.consume_key(none, EKey::Home) {
            Some(0)
        } else if i.consume_key(none, EKey::End) {
            Some(n - 1)
        } else {
            None
        }
    });
    if let Some(to) = moved
        && (active != Some(to))
    {
        intents.push(Intent::Active {
            key: panes[to].key.clone(),
        });
    }
    if removable
        && let Some(a) = active
        && ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, EKey::Delete)
                | i.consume_key(egui::Modifiers::NONE, EKey::Backspace)
        })
    {
        intents.push(Intent::Remove {
            key: panes[a].key.clone(),
        });
    }
}

/// Images carried in and out of the surface by the application's drag
/// session.
#[cfg(feature = "api")]
mod carry {
    use std::any::Any;

    use egui::{Context, Id, Pos2, Rect, Stroke, StrokeKind, Ui};
    use egui_pop_api::bus::drag::{self, End, Seen};
    use egui_pop_core::strings::tr;
    use egui_pop_core::theme::Tokens;

    use super::{Intent, Pane};
    use crate::select::Key;
    use crate::strings as s;

    /// Where images let go at `at` go in among `panes`: before the pane under
    /// the pointer when on its left half, else after it; off every pane, by
    /// the nearest one.
    fn insertion(panes: &[Rect], at: Pos2) -> usize {
        let Some((i, r)) = panes.iter().enumerate().min_by(|(_, a), (_, b)| {
            a.distance_sq_to_pos(at)
                .total_cmp(&b.distance_sq_to_pos(at))
        }) else {
            return 0;
        };
        if at.x < r.center().x { i } else { i + 1 }
    }

    /// What a target sees of a drag of images, whatever the payload.
    pub(super) enum Over<K> {
        Nothing,
        At { at: Pos2 },
        Unknown,
        Dropped(Vec<K>),
    }

    type Target<K> = Box<dyn Fn(&Context, &str, Rect, bool) -> Over<K>>;
    type Start<K> = Box<dyn Fn(&Context, Vec<K>)>;
    type Ended = Box<dyn Fn(&Context) -> Option<End>>;

    /// The application's payload, behind closures.
    pub(super) struct Carry<K> {
        pub(super) kind: &'static str,
        pub(super) start: Start<K>,
        target: Target<K>,
        ended: Ended,
    }

    impl<K: Key> Carry<K> {
        pub(super) fn new<T: Any + Send + Sync>(
            kind: &'static str,
            pack: fn(Vec<K>) -> T,
            unpack: fn(&T) -> Option<Vec<K>>,
        ) -> Self {
            Carry {
                kind,
                start: Box::new(move |ctx, keys| drag::start(ctx, kind, pack(keys))),
                target: Box::new(move |ctx, id, rect, full| {
                    let refusal = |p: &T| match unpack(p) {
                        None => Some(tr(ctx, &s::COMPARE_NOT_IMAGES)),
                        Some(_) if full => Some(tr(ctx, &s::COMPARE_FULL)),
                        Some(_) => None,
                    };
                    match drag::target::<T>(ctx, id, rect, refusal) {
                        Seen::Over { at, .. } => Over::At { at },
                        Seen::Unknown(_) => Over::Unknown,
                        Seen::Dropped(p) => unpack(&p).map_or(Over::Nothing, Over::Dropped),
                        _ => Over::Nothing,
                    }
                }),
                ended: Box::new(|ctx| drag::take_ended::<T>(ctx).map(|(_, end)| end)),
            }
        }
    }

    /// The surface as a drop target, and the end of a pane carried out.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn surface<K: Key>(
        ui: &Ui,
        c: &Carry<K>,
        name: &str,
        id: Id,
        rect: Rect,
        areas: &[Rect],
        panes: &[Pane<K>],
        max: usize,
        carrying: Option<K>,
        intents: &mut Vec<Intent<K>>,
    ) {
        let ctx = ui.ctx();
        let t = Tokens::of(ctx);
        let full = carrying.is_none() && panes.len() >= max;
        let at_id = id.with("over_at");
        match (c.target)(ctx, name, rect, full) {
            Over::At { at } => {
                ui.data_mut(|d| d.insert_temp(at_id, at));
                let painter = ui.painter();
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(2.0, t.focus_ring),
                    StrokeKind::Inside,
                );
                // Where the images would go: a bar between the panes.
                let i = insertion(areas, at);
                let x = match areas.get(i) {
                    // At a row's start: left of its first pane.
                    Some(r) if i == 0 || areas[i - 1].top() != r.top() => {
                        Some((r.left() - t.space.s / 2.0, r.y_range()))
                    }
                    _ => areas
                        .get(i.wrapping_sub(1))
                        .map(|l| (l.right() + t.space.s / 2.0, l.y_range())),
                };
                if let Some((x, y)) = x {
                    painter.vline(
                        x.clamp(rect.left() + 2.0, rect.right() - 2.0),
                        y,
                        Stroke::new(3.0, t.focus_ring),
                    );
                }
            }
            Over::Unknown => {
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0, t.focus_ring),
                    StrokeKind::Inside,
                );
            }
            Over::Dropped(keys) => {
                let at = ui
                    .data_mut(|d| d.remove_temp::<Pos2>(at_id))
                    .map_or(panes.len(), |p| insertion(areas, p));
                match &carrying {
                    // A pane carried and let go on its own surface: moved.
                    Some(k) if keys == [k.clone()] => {
                        if let Some(from) = panes.iter().position(|p| p.key == *k) {
                            let to = if at > from { at - 1 } else { at };
                            if to != from {
                                intents.push(Intent::Move { key: k.clone(), to });
                            }
                        }
                    }
                    _ => {
                        let keys: Vec<K> = keys
                            .into_iter()
                            .filter(|k| !panes.iter().any(|p| p.key == *k))
                            .collect();
                        if !keys.is_empty() {
                            intents.push(Intent::Add { keys, at });
                        }
                    }
                }
            }
            Over::Nothing => {}
        }
        // A pane carried out: let go over nothing, it goes out; dropped on
        // a target, that target has it.
        if let Some(k) = carrying
            && !drag::active(ctx)
        {
            ui.data_mut(|d| d.remove::<String>(id.with("carrying")));
            if let Some(End::Left { .. }) = (c.ended)(ctx) {
                drag::claim(ctx, "compare.remove");
                intents.push(Intent::Remove { key: k });
            }
        }
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
