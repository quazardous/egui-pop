//! Thumbnails and a virtualised grid of them, from images the
//! application supplies (a texture and its size), with a flat
//! placeholder in the image's average colour until the image is there:
//! no spinner per thumbnail. In this layer so that both the image tools
//! (`egui_pop_viewport::thumbnail` re-exports it) and the file picker of
//! the application API can draw them.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
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
    Color32, Id, Modifiers, Rect, Response, Role, Sense, Stroke, StrokeKind, TextStyle,
    TextWrapMode, TextureId, Ui, Vec2, Widget, WidgetInfo, WidgetText, pos2, vec2,
};

use crate::icon::Icon;
use crate::kit::{Kit, KitCx, Part, State};
use crate::theme::Tokens;
use crate::ui::UiPop as _;

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
        if response.has_focus() {
            crate::focus::own_ring(ui.ctx(), response.id);
        }
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
        response.widget_info(move || WidgetInfo::selected(Role::Button, true, selected, &label));
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
///
/// Need (image): a grid of thumbnails.
#[derive(Clone, Debug)]
pub struct ThumbnailGrid {
    count: usize,
    cell: Vec2,
    id_salt: Option<Id>,
    max_height: Option<f32>,
    reveal: Option<usize>,
    band: bool,
}

/// What a [`ThumbnailGrid`] drawn with [`ThumbnailGrid::show_with_band`]
/// tells.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct GridOutput {
    /// The number of cells in a row, what a move up or down skips.
    pub per_row: usize,
    /// A rubber band being drawn, or let go this frame.
    pub band: Option<Band>,
}

/// A rubber band dragged across a grid ([`ThumbnailGrid::rubber_band`]).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Band {
    /// The cells it touches, in order, scrolled away or not.
    pub cells: Vec<usize>,
    /// The keys held when it started: Ctrl or Shift add to a selection.
    pub modifiers: Modifiers,
    /// Let go this frame: the selection it makes is final.
    pub finished: bool,
}

/// How far the pointer moves before a drag draws a rubber band: a click is
/// not a band.
pub const BAND_MIN: f32 = 4.0;

/// The cells of a grid `per_row` wide that `rect` touches, `rect` in the
/// grid's own coordinates (its first cell's corner at the origin), each
/// cell `cell` large, `gap` apart in a row and `pitch` from one row's top
/// to the next's.
pub fn cells_in(
    rect: Rect,
    per_row: usize,
    count: usize,
    cell: Vec2,
    gap: f32,
    pitch: f32,
) -> Vec<usize> {
    let per_row = per_row.max(1);
    if count == 0 || rect.max.x < 0.0 || rect.max.y < 0.0 {
        return Vec::new();
    }
    let step = cell.x + gap;
    let rows = count.div_ceil(per_row);
    let first_row = (rect.min.y.max(0.0) / pitch).floor() as usize;
    let first_col = (rect.min.x.max(0.0) / step).floor() as usize;
    let last_row = ((rect.max.y / pitch).floor() as usize).min(rows - 1);
    let last_col = ((rect.max.x / step).floor() as usize).min(per_row - 1);
    let mut out = Vec::new();
    for row in first_row..=last_row {
        for col in first_col..=last_col {
            let i = row * per_row + col;
            let min = pos2(col as f32 * step, row as f32 * pitch);
            if i < count && Rect::from_min_size(min, cell).intersects(rect) {
                out.push(i);
            }
        }
    }
    out
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
            band: false,
        }
    }

    /// A drag from the room around the cells draws a rubber band
    /// (Finder's, Explorer's), which [`ThumbnailGrid::show_with_band`]
    /// tells: the cells it touches, scrolled away or not.
    pub fn rubber_band(mut self, on: bool) -> ThumbnailGrid {
        self.band = on;
        self
    }

    /// What tells its scrolling from another grid's in the same `Ui`.
    pub fn id_salt(mut self, salt: impl egui::AsIdSalt) -> ThumbnailGrid {
        self.id_salt = Some(Id::unique(salt));
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

    /// The room a grid drawn in `ui` lays its cells out in: its width
    /// (the scroll bar aside) and the gap between cells. `n` cells of
    /// `(width + gap) / n - gap` points fill a row.
    pub fn room(ui: &Ui) -> (f32, f32) {
        let gap = Tokens::of(ui.ctx()).space.xs;
        let width = ui.available_width() - ui.spacing().scroll.allocated_width();
        (width, gap)
    }

    /// Draws the rows in view, `item` drawing cell `i`. Gives the number of
    /// cells in a row, what a move up or down skips.
    pub fn show(self, ui: &mut Ui, item: impl FnMut(&mut Ui, usize)) -> usize {
        self.show_with_band(ui, item).per_row
    }

    /// [`ThumbnailGrid::show`], telling the rubber band too.
    pub fn show_with_band(self, ui: &mut Ui, mut item: impl FnMut(&mut Ui, usize)) -> GridOutput {
        let ThumbnailGrid {
            count,
            cell,
            id_salt,
            max_height,
            reveal,
            band,
        } = self;
        let (width, gap) = ThumbnailGrid::room(ui);
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
        let band_id = ui.scope_id().with(("thumbnail_grid.band", id_salt));
        let mut told = None;
        let out = ui.scroll_area_rows(area, cell.y, rows, |ui, range| {
            if band {
                // Under the cells, which take the clicks: the grid's own
                // coordinates start at its first cell's corner.
                let origin = ui.max_rect().min - vec2(0.0, range.start as f32 * pitch);
                told = rubber_band(ui, band_id, origin, |r| {
                    cells_in(r, per_row, count, cell, gap, pitch)
                });
            }
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
        crate::ui::contain_wheel(ui, &out);
        GridOutput {
            per_row,
            band: told,
        }
    }
}

/// A band's corners in the grid's coordinates, and the keys held when it
/// started; kept between frames under the grid's id.
#[derive(Clone, Copy, Debug)]
struct BandDrag {
    from: Vec2,
    to: Vec2,
    modifiers: Modifiers,
}

/// Reads and paints a rubber band over the room in view, `origin` the
/// grid's first cell's corner on screen; `cells` says what a rectangle in
/// the grid's coordinates touches.
fn rubber_band(
    ui: &mut Ui,
    id: Id,
    origin: egui::Pos2,
    cells: impl Fn(Rect) -> Vec<usize>,
) -> Option<Band> {
    // The room in view, the grid's empty end included: the scroll area
    // clips its rows only up and down, so the clip rect is narrowed to the
    // grid's own width, never sensing what lies beside the grid.
    let clip = ui.clip_rect();
    let room = Rect::from_x_y_ranges(ui.max_rect().x_range(), clip.y_range()).intersect(clip);
    let bg = ui.interact(room, id, Sense::drag());
    let mut drag: Option<BandDrag> = ui.data(|d| d.get_temp(id));
    if bg.drag_started()
        && let Some(p) = bg.interact_pointer_pos()
    {
        let at = p - origin;
        drag = Some(BandDrag {
            from: at,
            to: at,
            modifiers: ui.input(|i| i.modifiers),
        });
    }
    if bg.dragged()
        && let (Some(d), Some(p)) = (drag.as_mut(), bg.interact_pointer_pos())
    {
        d.to = p - origin;
    }
    let finished = bg.drag_stopped();
    let shown = drag.filter(|d| (d.to - d.from).length() >= BAND_MIN);
    let told = shown.map(|d| {
        let r = Rect::from_two_pos(d.from.to_pos2(), d.to.to_pos2());
        if !finished {
            let t = Tokens::of(ui.ctx());
            let fill = t
                .color("thumbnail.band")
                .unwrap_or(t.accent.default.gamma_multiply(0.18));
            let outline = t
                .color("thumbnail.band-outline")
                .unwrap_or(t.accent.default);
            let on_screen = r.translate(origin.to_vec2());
            let painter = ui.painter().clone();
            painter.rect_filled(on_screen, 0.0, fill);
            painter.rect_stroke(
                on_screen,
                0.0,
                Stroke::new(1.0, outline),
                StrokeKind::Inside,
            );
            crate::pointer::hold_drag_cursor(ui.ctx(), crate::pointer::DragKind::Select);
        }
        Band {
            cells: cells(r),
            modifiers: d.modifiers,
            finished,
        }
    });
    if finished || !(bg.dragged() || bg.drag_started()) {
        drag = None;
    }
    ui.data_mut(|d| match drag {
        Some(b) => {
            d.insert_temp(id, b);
        }
        None => d.remove::<BandDrag>(id),
    });
    told
}

#[cfg(test)]
mod band_tests {
    use super::*;

    #[test]
    fn a_band_touches_the_cells_it_crosses_and_none_beyond_the_last() {
        let cell = vec2(10.0, 10.0);
        // Three a row, 2 apart; rows 12 apart.
        let at = |a: (f32, f32), b: (f32, f32)| {
            cells_in(
                Rect::from_two_pos(pos2(a.0, a.1), pos2(b.0, b.1)),
                3,
                7,
                cell,
                2.0,
                12.0,
            )
        };
        assert_eq!(at((1.0, 1.0), (2.0, 2.0)), [0]);
        // Across the first two cells of the first two rows.
        assert_eq!(at((5.0, 5.0), (15.0, 15.0)), [0, 1, 3, 4]);
        // In a gap alone: nothing.
        assert!(at((10.5, 0.0), (11.5, 30.0)).is_empty());
        // Down past the end: the short last row's cell only.
        assert_eq!(at((0.0, 30.0), (100.0, 100.0)), [6]);
        // Above and left of the grid.
        assert!(at((-20.0, -20.0), (-1.0, -1.0)).is_empty());
    }
}
