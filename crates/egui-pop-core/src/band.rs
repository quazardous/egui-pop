//! A header band: the handle a movable surface (a movable dialog, a
//! palette) is dragged by. It sits at the top of a frame's content, its
//! fill reaching the frame's edges, with a separator below it and the
//! move cursor over it.
//!
//! [`begin`] registers the band's drag area first (so the buttons drawn
//! in its row, registered after it, stay on top of it), then lays out its
//! row; [`paint`] fills it once the frame's width is known, at the end of
//! the frame's content. The band is not focusable: the keyboard moves a
//! surface through a command of its own.
//!
//! ```
//! use egui_pop_core::band::{self, BandStyle};
//!
//! let ctx = egui::Context::default();
//! let mut out = ctx.run_ui(Default::default(), |ui| {
//!     let style = BandStyle {
//!         fill: egui::Color32::DARK_GRAY,
//!         separator: egui::Color32::GRAY,
//!         radius: 8.0,
//!         padding: 12.0,
//!         gap: 8.0,
//!     };
//!     egui::Frame::NONE.inner_margin(style.padding).show(ui, |ui| {
//!         let (_, band) = band::begin(ui, &style, |ui| ui.label("Levels"));
//!         ui.label("content");
//!         band::paint(ui, &band, &style);
//!         assert!(!band.dragging);
//!     });
//! });
//! out.textures_delta.clear();
//! ```

use egui::{Color32, CornerRadius, CursorIcon, Id, Rect, Sense, Shape, Ui, Vec2};

/// How a band looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandStyle {
    /// Its fill.
    pub fill: Color32,
    /// The line below it.
    pub separator: Color32,
    /// The frame's corner radius (the band's top corners follow it).
    pub radius: f32,
    /// The frame's inner margin: how far the band reaches out of the
    /// content to the frame's edges.
    pub padding: f32,
    /// The room below its row, inside the band (and as much again below
    /// the separator).
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

/// Starts a band at the top of a frame's content and lays out its row
/// with `add` (a title, buttons). Its size is the last pass's (a guess on
/// the first one).
pub fn begin<R>(ui: &mut Ui, style: &BandStyle, add: impl FnOnce(&mut Ui) -> R) -> (R, Band) {
    let id = ui.id().with("egui_pop.band");
    let pad = style.padding;
    let top_left = ui.max_rect().min - egui::vec2(pad, pad);
    let size = ui
        .ctx()
        .data(|d| d.get_temp::<Vec2>(id))
        .unwrap_or(egui::vec2(
            ui.available_width() + 2.0 * pad,
            pad + ui.spacing().interact_size.y + style.gap,
        ));
    let response = ui.interact(
        Rect::from_min_size(top_left, size),
        id,
        Sense::CLICK | Sense::DRAG,
    );
    let shape = ui.painter().add(Shape::Noop);
    let inner = add(ui);
    let bottom = ui.min_rect().bottom() + style.gap;
    ui.add_space(2.0 * style.gap);
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Move);
    }
    let band = Band {
        id: Some(id),
        shape: Some(shape),
        top: top_left.y,
        bottom,
        dragging: response.dragged(),
        double_clicked: response.double_clicked(),
        delta: response.drag_delta(),
        hovered: response.hovered(),
    };
    (inner, band)
}

/// Fills the band across the frame's width (known once the content is
/// laid out), with its separator; call it last in the frame's content.
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
    // Inside the frame's outline: its corners, less the stroke.
    let r = (style.radius - 1.0).clamp(0.0, 255.0) as u8;
    let corners = CornerRadius {
        nw: r,
        ne: r,
        sw: 0,
        se: 0,
    };
    ui.painter()
        .set(shape, Shape::rect_filled(rect, corners, style.fill));
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, style.separator),
    );
    ui.ctx().data_mut(|d| d.insert_temp(id, rect.size()));
}
