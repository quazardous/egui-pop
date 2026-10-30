//! Thumbnails and a virtualised grid of them, from images the
//! application supplies (a texture and its size), with a flat
//! placeholder in the image's average colour until the image is there:
//! no spinner per thumbnail. In this layer so that both the image tools
//! (`egui_pop_viewport::thumbnail` re-exports it) and the file picker of
//! the application API can draw them.
//!
//! ```
//! use egui_pop_core::thumbnail::{Thumbnail, thumbnail_grid};
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
    Color32, Id, Rect, Response, Sense, TextStyle, TextWrapMode, TextureId, Ui, Vec2, Widget,
    WidgetInfo, WidgetText, WidgetType, pos2, vec2,
};

use crate::icon::Icon;
use crate::kit::{Kit, KitCx, Part, State};
use crate::theme::Tokens;

/// One thumbnail.
#[derive(Debug)]
pub struct Thumbnail {
    label: String,
    image: Option<(TextureId, [u32; 2])>,
    placeholder: Option<Color32>,
    icon: Option<Icon>,
    caption: Option<String>,
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
            icon: None,
            caption: None,
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

    /// A sign over the placeholder, for what has no image: a folder, a
    /// file without a preview, a failure.
    pub fn icon(mut self, icon: Icon) -> Thumbnail {
        self.icon = Some(icon);
        self
    }

    /// A line of text under the image, cut to the thumbnail's width. Give
    /// the thumbnail [`caption_height`] more room for it.
    pub fn caption(mut self, caption: impl Into<String>) -> Thumbnail {
        self.caption = Some(caption.into());
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

/// The room [`Thumbnail::caption`] takes under the image, in points.
pub fn caption_height(ui: &Ui) -> f32 {
    ui.text_style_height(&TextStyle::Small) + Tokens::of(ui.ctx()).space.xs
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
        let kit = Kit::of(ui.ctx());
        // The kit's box (`thumbnail`): a selected thumbnail is not
        // hovered. Painted unclipped, so the focus ring outside it shows.
        let state = State::REST
            .with(State::SELECTED, self.selected)
            .with(State::HOVER, response.hovered() && !self.selected)
            .with(State::FOCUS, response.has_focus());
        kit.thumbnail.paint(
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
        let mut inner = rect.shrink(tokens.space.xs.max(2.0));
        let text = kit.thumbnail.style(state).text;
        if let Some(caption) = &self.caption {
            let room = caption_height(ui);
            let line = Rect::from_min_max(pos2(inner.left(), inner.bottom() - room), inner.max);
            inner.set_bottom(line.top());
            let galley = WidgetText::from(caption.as_str()).into_galley(
                ui,
                Some(TextWrapMode::Truncate),
                line.width(),
                TextStyle::Small,
            );
            let at = pos2(
                line.center().x - galley.size().x / 2.0,
                line.bottom() - galley.size().y,
            );
            painter.galley(at, galley, text);
        }
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
        if let (None, Some(icon)) = (self.image, self.icon) {
            let side = inner.width().min(inner.height()) / 2.0;
            icon.paint(
                &painter,
                Rect::from_center_size(inner.center(), vec2(side, side)),
                text,
            );
        }
        let label = self.label;
        let selected = self.selected;
        response.widget_info(move || {
            WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, &label)
        });
        response
    }
}

/// A virtualised grid: only the rows in view are built, so ten thousand
/// thumbnails cost what a screenful does. `item` draws cell `i`.
pub fn thumbnail_grid(ui: &mut Ui, count: usize, cell: Vec2, item: impl FnMut(&mut Ui, usize)) {
    ThumbnailGrid::new(count, cell).show(ui, item);
}

/// [`thumbnail_grid`] with its options: where it keeps its scrolling, how
/// high it gets, and a cell to bring into view, drawn or not.
///
/// ```
/// use egui_pop_core::thumbnail::{Thumbnail, ThumbnailGrid};
///
/// fn ui(ui: &mut egui::Ui, cursor: usize) {
///     let columns = ThumbnailGrid::new(10_000, egui::vec2(96.0, 72.0))
///         .id_salt("photos")
///         .max_height(300.0)
///         .reveal(Some(cursor))
///         .show(ui, |ui, i| {
///             ui.add(Thumbnail::new(format!("Image {i}")).selected(i == cursor));
///         });
///     // Up and Down move the cursor by `columns`.
///     let _ = columns;
/// }
/// ```
#[derive(Clone, Debug)]
pub struct ThumbnailGrid {
    count: usize,
    cell: Vec2,
    id_salt: Option<Id>,
    max_height: Option<f32>,
    reveal: Option<usize>,
}

impl ThumbnailGrid {
    /// A grid of `count` cells of size `cell`.
    pub fn new(count: usize, cell: Vec2) -> ThumbnailGrid {
        ThumbnailGrid {
            count,
            cell,
            id_salt: None,
            max_height: None,
            reveal: None,
        }
    }

    /// What tells its scrolling from another grid's in the same `Ui`.
    pub fn id_salt(mut self, salt: impl egui::AsId) -> ThumbnailGrid {
        self.id_salt = Some(Id::new(salt));
        self
    }

    /// The most it takes in height.
    pub fn max_height(mut self, height: f32) -> ThumbnailGrid {
        self.max_height = Some(height);
        self
    }

    /// Scrolls by the least that shows this cell, which may not be drawn
    /// yet: for a keyboard moving through a long grid.
    pub fn reveal(mut self, cell: Option<usize>) -> ThumbnailGrid {
        self.reveal = cell;
        self
    }

    /// Draws the rows in view, `item` drawing cell `i`. Gives the number of
    /// cells in a row, what a move up or down skips.
    pub fn show(self, ui: &mut Ui, mut item: impl FnMut(&mut Ui, usize)) -> usize {
        let ThumbnailGrid {
            count,
            cell,
            id_salt,
            max_height,
            reveal,
        } = self;
        let gap = Tokens::of(ui.ctx()).space.xs;
        let width = ui.available_width() - ui.spacing().scroll.allocated_width();
        let per_row = (((width + gap) / (cell.x + gap)).floor() as usize).max(1);
        let rows = count.div_ceil(per_row);
        let mut area = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .animated(false);
        if let Some(salt) = id_salt {
            area = area.id_salt(salt);
        }
        if let Some(height) = max_height {
            area = area.max_height(height);
        }
        // Rows lie one row height and the `Ui`'s spacing apart: the pitch
        // `show_rows` works the first row in view out from.
        let pitch = cell.y + ui.spacing().item_spacing.y;
        area.show_rows(ui, cell.y, rows, |ui, range| {
            if let Some(i) = reveal {
                let top = ui.max_rect().top() + ((i / per_row) as f32 - range.start as f32) * pitch;
                let left = ui.max_rect().left();
                ui.scroll_to_rect(
                    Rect::from_min_size(pos2(left, top), vec2(cell.x, cell.y)),
                    None,
                );
                ui.ctx().request_repaint();
            }
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
        per_row
    }
}
