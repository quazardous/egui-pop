//! Thumbnails and a virtualised grid of them, from images the
//! application supplies (a texture and its size), with a flat
//! placeholder in the image's average colour until the image is there:
//! no spinner per thumbnail.
//!
//! ```
//! use egui_pop_viewport::thumbnail::{Thumbnail, thumbnail_grid};
//!
//! fn ui(ui: &mut egui::Ui, selected: &mut usize) {
//!     thumbnail_grid(ui, 10_000, egui::vec2(96.0, 72.0), |ui, i| {
//!         let r = ui.add(
//!             Thumbnail::new(format!("Image {i}"))
//!                 .placeholder(egui::Color32::from_gray(90))
//!                 .selected(i == *selected),
//!         );
//!         if r.clicked() {
//!             *selected = i;
//!         }
//!     });
//! }
//! ```

use egui::{
    Color32, Rect, Response, Role, Sense, TextureId, Ui, Vec2, Widget, WidgetInfo, pos2, vec2,
};
use egui_pop_core::kit::{Kit, KitCx, Part, State};
use egui_pop_core::theme::Tokens;

/// One thumbnail.
#[derive(Debug)]
pub struct Thumbnail {
    label: String,
    image: Option<(TextureId, [u32; 2])>,
    placeholder: Option<Color32>,
    selected: bool,
    size: Option<Vec2>,
}

impl Thumbnail {
    /// A thumbnail named `label` (its accessible name).
    pub fn new(label: impl Into<String>) -> Thumbnail {
        Thumbnail {
            label: label.into(),
            image: None,
            placeholder: None,
            selected: false,
            size: None,
        }
    }

    /// The image: a texture and its size in pixels (drawn fitted, its
    /// aspect kept).
    pub fn image(mut self, texture: TextureId, size: [u32; 2]) -> Thumbnail {
        self.image = Some((texture, size));
        self
    }

    /// The colour shown until the image is there (the image's average,
    /// for example).
    pub fn placeholder(mut self, color: Color32) -> Thumbnail {
        self.placeholder = Some(color);
        self
    }

    pub fn selected(mut self, selected: bool) -> Thumbnail {
        self.selected = selected;
        self
    }

    /// The size in points (default: the grid cell, or 96 × 72).
    pub fn size(mut self, size: Vec2) -> Thumbnail {
        self.size = Some(size);
        self
    }
}

/// The largest rectangle of aspect `w:h` inside `outer`, centred.
pub fn fitted(outer: Rect, size: [u32; 2]) -> Rect {
    if size[0] == 0 || size[1] == 0 {
        return outer;
    }
    let a = size[0] as f32 / size[1] as f32;
    let (w, h) = if outer.width() / outer.height() > a {
        (outer.height() * a, outer.height())
    } else {
        (outer.width(), outer.width() / a)
    };
    Rect::from_center_size(outer.center(), vec2(w, h))
}

impl Widget for Thumbnail {
    fn ui(self, ui: &mut Ui) -> Response {
        let size = self.size.unwrap_or_else(|| {
            let s = ui.available_size_before_wrap();
            if s.x.is_finite() && s.x > 0.0 && s.y.is_finite() && s.y > 0.0 && s.x < 400.0 {
                s
            } else {
                vec2(96.0, 72.0)
            }
        });
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let tokens = Tokens::of(ui.ctx());
        // The kit's box (`thumbnail`): a selected thumbnail is not
        // hovered. Painted unclipped, so the focus ring outside it shows.
        let state = State::REST
            .with(State::SELECTED, self.selected)
            .with(State::HOVER, response.hovered() && !self.selected)
            .with(State::FOCUS, response.has_focus());
        Kit::of(ui.ctx()).thumbnail.paint(
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
        let painter = ui.painter_at(rect);
        let inner = rect.shrink(tokens.space.xs.max(2.0));
        match (self.image, self.placeholder) {
            (Some((id, px)), _) => {
                painter.image(
                    id,
                    fitted(inner, px),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            (None, Some(c)) => {
                painter.rect_filled(inner, 0.0, c);
            }
            (None, None) => {}
        }
        let label = self.label;
        let selected = self.selected;
        response.widget_info(move || WidgetInfo::selected(Role::Button, true, selected, &label));
        response
    }
}

/// A virtualised grid: only the rows in view are built, so ten thousand
/// thumbnails cost what a screenful does. `item` draws cell `i`.
pub fn thumbnail_grid(ui: &mut Ui, count: usize, cell: Vec2, mut item: impl FnMut(&mut Ui, usize)) {
    let gap = Tokens::of(ui.ctx()).space.xs;
    let width = ui.available_width();
    let per_row = (((width + gap) / (cell.x + gap)).floor() as usize).max(1);
    let rows = count.div_ceil(per_row);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, cell.y + gap, rows, |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(gap, gap);
                    for col in 0..per_row {
                        let i = row * per_row + col;
                        if i >= count {
                            break;
                        }
                        ui.allocate_ui(cell, |ui| {
                            ui.set_min_size(cell);
                            item(ui, i);
                        });
                    }
                });
            }
        });
}
