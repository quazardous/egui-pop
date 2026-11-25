//! What the tree and the list share: rows of one height in a scroll
//! area that builds only the rows in view, the focus the view takes as
//! a whole, and how a row looks.

use std::ops::Range;

use egui::{EventFilter, Id, Rect, Response, ScrollArea, Sense, TextStyle, Ui, pos2, vec2};
use egui_pop_core::kit::{Component, Kit, KitCx, Part, State};
use egui_pop_core::theme::Tokens;
use egui_pop_core::ui::UiPop as _;

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
/// whole is one focusable widget (`id`) of role `role` (a tree, a list
/// box), named `label`. Returns what
/// `rows` returned, the view's response and its visible rectangle.
#[allow(clippy::too_many_arguments)]
pub(crate) fn show<R>(
    ui: &mut Ui,
    id: Id,
    label: &str,
    role: egui::Role,
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
    // Before its first pass a view has no height to bring a row into: the
    // row waits for the next pass.
    let measured = scroll.height > 0.0;
    if !measured && scroll.reveal.is_some() {
        ui.ctx().request_repaint();
    }
    let reveal = if measured { scroll.reveal.take() } else { None };
    let target = match (scroll.to.take(), reveal) {
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
    let out = ui.scroll_area_viewport(area, |ui, viewport| {
        // The list is the stop, around the area.
        egui_pop_core::focus::takes_keys(ui.ctx(), id);
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
    egui_pop_core::ui::contain_wheel(ui, &out);
    scroll.offset = out.state.clamped_offset().y;
    scroll.height = out.inner_rect.height();
    let response = ui.interact(out.inner_rect, id, Sense::focusable_noninteractive());
    egui_pop_core::a11y::name(&response, role, label);
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
    if response.has_focus() {
        egui_pop_core::focus::own_ring(ui.ctx(), id);
    }
    paint_view(ui, &kit, out.inner_rect, response.has_focus());
    (out.inner, response, out.inner_rect)
}

/// The cursor row as the view's active descendant: the focus stays on the
/// view while the arrows move the cursor, and a screen reader reads the
/// row the cursor is on.
pub(crate) fn active_row(ui: &Ui, view: Id, row: Option<Id>) {
    if let Some(row) = row {
        ui.ctx().accesskit_node_builder(view, |node| {
            node.set_active_descendant(row.accesskit_id());
        });
    }
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

/// A row's height: the theme's row height (`size.row.height`).
pub(crate) fn height(ui: &Ui) -> f32 {
    crate::item::row_height(ui)
}

/// Rows clickable and draggable, not focusable: the view takes the
/// focus as a whole.
pub(crate) const ROW_SENSE: Sense = Sense::CLICK.union(Sense::DRAG);

/// How many items a drag out of a view carries, beside the pointer.
pub(crate) fn carry_count(ui: &Ui, count: usize) {
    let ctx = ui.ctx();
    let Some(at) = ctx.pointer_latest_pos() else {
        return;
    };
    let t = Tokens::of(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        Id::unique("egui_pop_views::library::carry"),
    ));
    let visuals = ui.visuals().clone();
    let galley = painter.layout_no_wrap(
        count.to_string(),
        TextStyle::Small.resolve(ui.style()),
        visuals.selection.stroke.color,
    );
    let pad = vec2(t.space.xs * 2.0, t.space.xs);
    let rect = Rect::from_min_size(at + vec2(14.0, 14.0), galley.size() + pad * 2.0);
    painter.rect_filled(rect, rect.height() / 2.0, visuals.selection.bg_fill);
    painter.galley(rect.min + pad, galley, visuals.selection.stroke.color);
}

/// Keeps a drag out of a row going while the pointer's button is held:
/// the carry cursor and how many items go, beside the pointer; ends it
/// once the button is let go.
pub(crate) fn carry(ui: &Ui, carry: &mut Option<usize>) {
    let Some(count) = *carry else {
        return;
    };
    if ui.input(|i| i.pointer.any_down()) {
        egui_pop_core::pointer::hold_drag_cursor(ui.ctx(), egui_pop_core::pointer::DragKind::Carry);
        carry_count(ui, count);
    } else {
        *carry = None;
    }
}

/// What a drag out starting on the row of `key` carries: the selection
/// when the row is in it, else that row alone (the selection is left as
/// it is).
pub(crate) fn carried<K: crate::select::Key>(
    selection: &crate::select::Selection<K>,
    key: K,
) -> Vec<K> {
    if selection.contains(&key) {
        selection.keys().to_vec()
    } else {
        vec![key]
    }
}
