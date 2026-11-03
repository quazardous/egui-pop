//! A header band: the handle a movable surface (a movable dialog, a
//! palette) is dragged by. It sits at the top of a frame's content, its
//! fill reaching the frame's edges, with a separator below it and the
//! move cursor over it.
//!
//! [`begin`] registers the band's drag area first (so the buttons drawn
//! in its row, registered after it, stay on top of it), then lays out its
//! row; [`paint`] has the kit's `header-band` paint it once the frame's
//! width is known, at the end of the frame's content, under the row: in
//! the `hover` state under the pointer, `active` while dragged. The band
//! is not focusable: the keyboard moves a surface through a command of
//! its own.
//!
//! ```
//! use egui_pop_core::band::{self, BandStyle};
//!
//! let ctx = egui::Context::default();
//! let mut out = ctx.run_ui(Default::default(), |ui| {
//!     let style = BandStyle {
//!         padding: 12.0,
//!         gap: 8.0,
//!     };
//!     egui::Frame::NONE.inner_margin(style.padding).show(ui, |ui| {
//!         // The row is told the band's state, for its title's colour.
//!         let (_, band) = band::begin(ui, &style, |ui, _state| ui.label("Levels"));
//!         ui.label("content");
//!         band::paint(ui, &band, &style);
//!         assert!(!band.dragging);
//!     });
//! });
//! out.textures_delta.clear();
//! ```

use egui::{CursorIcon, Id, Rect, Response, Role, Sense, Shape, Ui, Vec2};

use crate::icon::Icon;
use crate::kit::{Kit, KitCx, Part, State};
use crate::state::Slot;
use crate::theme::Tokens;

/// Where a band sits in its frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandStyle {
    /// The frame's inner margin: how far the band reaches out of the
    /// content to the frame's edges.
    pub padding: f32,
    /// The room below its row, inside the band (and as much again below
    /// its line): the kit's `header-band.padding-y`, or a dialog's
    /// `dialog-header.padding-y`.
    pub gap: f32,
}

/// A band as laid out this pass, and what the pointer did to it.
#[derive(Clone, Debug, Default)]
pub struct Band {
    id: Option<Id>,
    shape: Option<egui::layers::ShapeIdx>,
    top: f32,
    bottom: f32,
    /// Being dragged.
    pub dragging: bool,
    /// Double-clicked.
    pub double_clicked: bool,
    /// The drag's move this pass.
    pub delta: Vec2,
    /// Under the pointer (not under one of its buttons).
    pub hovered: bool,
}

impl Band {
    /// Its state for the kit: `HOVER` under the pointer, `ACTIVE` while
    /// dragged.
    pub fn state(&self) -> State {
        State::REST
            .with(State::HOVER, self.hovered)
            .with(State::ACTIVE, self.dragging)
    }
}

/// Starts a band at the top of a frame's content and lays out its row
/// with `add` (a title, buttons), told the band's state. Its size is the
/// last pass's (a guess on the first one).
/// A band's size on the last pass, per band.
const SIZE: Slot<Vec2> = Slot::new("core.band.size");

pub fn begin<R>(
    ui: &mut Ui,
    style: &BandStyle,
    add: impl FnOnce(&mut Ui, State) -> R,
) -> (R, Band) {
    let id = ui.scope_id().with("egui_pop.band");
    let pad = style.padding;
    let top_left = ui.max_rect().min - egui::vec2(pad, pad);
    let size = SIZE.keyed(id).get(ui.ctx()).unwrap_or(egui::vec2(
        ui.available_width() + 2.0 * pad,
        pad + ui.spacing().interact_size.y + style.gap,
    ));
    let response = ui.interact(
        Rect::from_min_size(top_left, size),
        id,
        Sense::CLICK | Sense::DRAG,
    );
    let shape = ui.painter().add(Shape::Noop);
    let mut band = Band {
        id: Some(id),
        shape: Some(shape),
        top: top_left.y,
        bottom: 0.0,
        dragging: response.dragged(),
        double_clicked: response.double_clicked(),
        delta: response.drag_delta(),
        hovered: response.hovered(),
    };
    let inner = add(ui, band.state());
    band.bottom = ui.min_rect().bottom() + style.gap;
    ui.add_space(2.0 * style.gap);
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Move);
    }
    (inner, band)
}

/// Has the kit's `header-band` paint the band across the frame's width
/// (known once the content is laid out), under its row; call it last in
/// the frame's content.
pub fn paint(ui: &Ui, band: &Band, style: &BandStyle) {
    let (Some(id), Some(shape)) = (band.id, band.shape) else {
        return;
    };
    let pad = style.padding;
    let content = ui.min_rect();
    let rect = Rect::from_min_max(
        egui::pos2(content.left() - pad, band.top),
        egui::pos2(content.right() + pad, band.bottom),
    );
    let part = Part {
        rect,
        room: rect,
        state: band.state(),
        label: None,
    };
    Kit::of(ui.ctx())
        .header_band
        .paint_under(ui, shape, &part, None);
    SIZE.keyed(id).set(ui.ctx(), rect.size());
}

/// A square icon button of a band, `side` points wide, named `name`
/// (for assistive technology and its tooltip), drawn by the kit's
/// `band-icon-button`: `selected` when `on` (a pinned palette), the icon
/// in its state's `text`.
pub fn button(ui: &mut Ui, side: f32, icon: Icon, name: &str, on: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), Sense::click());
    if ui.is_rect_visible(rect) {
        let kit = Kit::of(ui.ctx());
        let part = Part {
            rect,
            room: rect,
            state: State::from_response(&response).with(State::SELECTED, on),
            label: None,
        };
        let cx = KitCx {
            painter: ui.painter(),
            ppp: ui.ctx().pixels_per_point(),
        };
        let button = &kit.band_icon_button;
        button.paint(&cx, &part);
        let size = Tokens::of(ui.ctx()).size.icon_inline;
        icon.paint(
            ui.painter(),
            Rect::from_center_size(rect.center(), egui::vec2(size, size)),
            button.style(part.state).text,
        );
    }
    crate::a11y::name(&response, Role::Button, name);
    response.on_hover_text(name)
}
