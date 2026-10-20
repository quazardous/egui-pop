//! What the tree and the list share: rows of one height in a scroll
//! area that builds only the rows in view, the focus the view takes as
//! a whole, and how a row looks.

use std::ops::Range;

use egui::{EventFilter, Id, Rect, Response, ScrollArea, Sense, Ui, pos2, vec2};
use egui_pop_core::kit::{Component, Kit, KitCx, Part, State};

use crate::nav;

/// Where a view is scrolled, kept between passes.
#[derive(Clone, Debug, Default)]
pub(crate) struct Scroll {
    pub offset: f32,
    pub height: f32,
    /// A row to bring into view this pass.
    pub reveal: Option<usize>,
    /// An offset to scroll to this pass (dragging near an edge).
    pub to: Option<f32>,
}

/// The rows in view, with the rectangle of each, and the kit they are
/// painted with (fetched once per view and pass).
pub(crate) struct Visible<'a> {
    pub range: Range<usize>,
    pub rect_of: &'a dyn Fn(usize) -> Rect,
    pub kit: &'a Kit,
}

/// Draws `count` rows `row_h` high, building only those in view; the
/// whole is one focusable widget (`id`), named `label`. Returns what
/// `rows` returned, the view's response and its visible rectangle.
#[allow(clippy::too_many_arguments)]
pub(crate) fn show<R>(
    ui: &mut Ui,
    id: Id,
    label: &str,
    scroll: &mut Scroll,
    count: usize,
    row_h: f32,
    height: Option<f32>,
    rows: impl FnOnce(&mut Ui, Visible<'_>) -> R,
) -> (R, Response, Rect) {
    let mut area = ScrollArea::vertical()
        .id_salt(id.with("scroll"))
        .auto_shrink([false, false]);
    if let Some(h) = height {
        area = area.max_height(h).min_scrolled_height(h);
    }
    let target = match (scroll.to.take(), scroll.reveal.take()) {
        (Some(o), _) => Some(o),
        (None, Some(i)) => Some(nav::reveal(i, row_h, scroll.offset, scroll.height)),
        _ => None,
    };
    if let Some(o) = target
        && o != scroll.offset
    {
        area = area.vertical_scroll_offset(o.max(0.0));
    }
    let kit = Kit::of(ui.ctx());
    let out = area.show_viewport(ui, |ui, viewport| {
        let origin = ui.max_rect().min;
        let width = ui.available_width();
        ui.set_min_size(vec2(width, count as f32 * row_h));
        let first = ((viewport.min.y / row_h).floor().max(0.0) as usize).min(count);
        let last = ((viewport.max.y / row_h).ceil().max(0.0) as usize).min(count);
        let rect_of = move |i: usize| {
            Rect::from_min_size(
                pos2(origin.x, origin.y + i as f32 * row_h),
                vec2(width, row_h),
            )
        };
        rows(
            ui,
            Visible {
                range: first..last,
                rect_of: &rect_of,
                kit: &kit,
            },
        )
    });
    scroll.offset = out.state.clamped_offset().y;
    scroll.height = out.inner_rect.height();
    let response = ui.interact(out.inner_rect, id, Sense::focusable_noninteractive());
    egui_pop_core::a11y::name(&response, egui::Role::Unknown, label);
    if response.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            )
        });
    }
    paint_view(ui, &kit, out.inner_rect, response.has_focus());
    (out.inner, response, out.inner_rect)
}

/// How many rows a page holds.
pub(crate) fn page(scroll: &Scroll, row_h: f32) -> usize {
    ((scroll.height / row_h).floor() as usize)
        .saturating_sub(1)
        .max(1)
}

/// Paints a data view's own box (`list-view`) over its rows: its
/// outline while it has the focus.
pub(crate) fn paint_view(ui: &Ui, kit: &Kit, rect: Rect, focused: bool) {
    kit.list_view.paint(
        &KitCx {
            painter: ui.painter(),
            ppp: ui.ctx().pixels_per_point(),
        },
        &Part {
            rect,
            room: rect,
            state: State::REST.with(State::FOCUS, focused),
            label: None,
        },
    );
}

/// A row's state: selected, under the pointer, the cursor of a focused
/// view (`focus`). A selected row is not hovered: the selection's fill
/// stays under the pointer.
pub(crate) fn state(selected: bool, hovered: bool, cursor: bool) -> State {
    State::REST
        .with(State::SELECTED, selected)
        .with(State::HOVER, hovered && !selected)
        .with(State::FOCUS, cursor)
}

/// Paints a row's box (`list-row`), under its content.
pub(crate) fn paint(ui: &Ui, row: &Component, rect: Rect, state: State) {
    row.paint(
        &KitCx {
            painter: ui.painter(),
            ppp: ui.ctx().pixels_per_point(),
        },
        &Part {
            rect,
            room: rect,
            state,
            label: None,
        },
    );
}

/// The colour of a row's text.
pub(crate) fn text_color(row: &Component, state: State) -> egui::Color32 {
    row.style(state).text
}

/// A row's height: the theme's interactive height.
pub(crate) fn height(ui: &Ui) -> f32 {
    ui.spacing()
        .interact_size
        .y
        .max(ui.text_style_height(&egui::TextStyle::Body) + 4.0)
}

/// Rows clickable and draggable, not focusable: the view takes the
/// focus as a whole.
pub(crate) const ROW_SENSE: Sense = Sense::CLICK.union(Sense::DRAG);
