//! Rows and grids: [`UiPop::row`](super::UiPop::row) and
//! [`UiPop::grid`](super::UiPop::grid) place cells on tracks, align them on
//! their text's baseline, and leave gaps of [`Space`] only.
//!
//! A track is sized by its content ([`Track::CONTENT`]), by a share of what
//! is left ([`Track::fr`]), by a share of the container ([`Track::percent`])
//! or in lines of body text ([`Track::em`]), each between an optional
//! [`Length`] minimum and maximum.
//!
//! ```
//! use egui_pop_core::theme::{self, ThemeSpec};
//! use egui_pop_core::ui::{Align, Grid, Length, Row, Space, Track, UiPop};
//!
//! let ctx = egui::Context::default();
//! theme::install(&ctx, &ThemeSpec::new("pop-dark"));
//! let mut name = String::from("IMG_0001");
//! let mut out = ctx.run_ui(Default::default(), |ui| {
//!     // A line: a title, a field taking what is left (between 8 and 24
//!     // em), a button sized to its content, all on one baseline.
//!     let field = Track::fr(1.0).min(Length::Em(8.0)).max(Length::Em(24.0));
//!     ui.row(Row::new([Track::CONTENT, field, Track::CONTENT]), |r| {
//!         r.cell(|ui| ui.section_title("Name"));
//!         r.cell(|ui| ui.text_edit_singleline(&mut name));
//!         r.cell(|ui| ui.button("Rename"));
//!     });
//!     // Headers as wide as the widest, values take the rest; one column
//!     // when narrower than 30 em.
//!     let grid = Grid::new("details", [Track::CONTENT, Track::fr(1.0)])
//!         .fold_below(Length::Em(30.0));
//!     ui.grid(grid, |g| {
//!         g.header("Kind");
//!         g.cell(|ui| ui.label("Raw image"));
//!         g.header("Size");
//!         g.cell(|ui| ui.label("24 MB"));
//!     });
//!     // A button bar: buttons as wide as the widest, at the end.
//!     let bar = Row::new([Track::CONTENT; 2]).justify(Align::End).equal_widths();
//!     ui.row(bar, |r| {
//!         r.cell(|ui| ui.button("Cancel"));
//!         r.cell(|ui| ui.button("OK"));
//!     });
//!     ui.space_role(Space::Group);
//! });
//! out.textures_delta.clear();
//! ```
//!
//! How it measures: egui places each widget once, so cells are placed from
//! the sizes and baselines they had on the previous pass (kept in the
//! context's temporary memory under the layout's id). When they change, the
//! layout asks egui to discard the pass and run another, as egui's own
//! `Grid` does; at rest nothing is measured twice. A cell's baseline is the
//! first baseline of the text it paints; a cell painting no text is taken
//! as a single-line widget with body text centred in it, as egui draws its
//! own.

use std::hash::{Hash, Hasher};

use egui::{
    Align as EguiAlign, AsIdSalt, IdSalt, InnerResponse, Layout, Pos2, Rect, Response, Sense,
    Shape, TextStyle, TextWrapMode, Ui, UiBuilder, pos2,
};

use super::{Space, TextRole, UiPop};
use crate::theme::Tokens;

/// A length for a track's size, minimum or maximum, never in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    /// Lines of body text: `n` times the body text role's size
    /// (`text-role.body.size`), so it follows the text scale.
    Em(f32),
    /// A share of the container's width, `0.0..=100.0`.
    Percent(f32),
}

impl Length {
    /// In points, for a body text size `em` and a container `width` wide.
    pub fn points(self, em: f32, width: f32) -> f32 {
        match self {
            Length::Em(n) => n * em,
            Length::Percent(p) => p / 100.0 * width,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Size {
    Content,
    Fr(f32),
    Fixed(Length),
}

/// The width of a column of a [`Row`] or a [`Grid`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    size: Size,
    min: Option<Length>,
    max: Option<Length>,
}

impl Track {
    /// As wide as its content (the widest cell of the column in a grid), as
    /// measured on the previous pass.
    pub const CONTENT: Track = Track {
        size: Size::Content,
        min: None,
        max: None,
    };

    /// A weight of what the other tracks and the gaps leave: two `fr(1.0)`
    /// share it equally.
    pub fn fr(weight: f32) -> Self {
        Self::of(Size::Fr(weight.max(0.0)))
    }

    /// `n` lines of body text ([`Length::Em`]).
    pub fn em(n: f32) -> Self {
        Self::of(Size::Fixed(Length::Em(n)))
    }

    /// A share of the container's width ([`Length::Percent`]).
    pub fn percent(p: f32) -> Self {
        Self::of(Size::Fixed(Length::Percent(p)))
    }

    /// Never narrower than `min`.
    pub fn min(mut self, min: Length) -> Self {
        self.min = Some(min);
        self
    }

    /// Never wider than `max` (a minimum wins over it).
    pub fn max(mut self, max: Length) -> Self {
        self.max = Some(max);
        self
    }

    fn of(size: Size) -> Self {
        Self {
            size,
            min: None,
            max: None,
        }
    }

    fn clamp(&self, w: f32, em: f32, width: f32) -> f32 {
        let w = self.max.map_or(w, |m| w.min(m.points(em, width)));
        self.min.map_or(w, |m| w.max(m.points(em, width))).max(0.0)
    }
}

/// How cells sit in their line ([`Row::align`], [`Grid::align`]), or a
/// line in its container ([`Row::justify`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Align {
    /// At the top (or the start of the line).
    Start,
    /// Centred.
    Center,
    /// At the bottom (or the end of the line).
    End,
    /// Every cell's first text baseline on one line; across a line, the
    /// same as [`Align::Start`].
    #[default]
    Baseline,
}

impl Align {
    fn share(self) -> f32 {
        match self {
            Align::Start | Align::Baseline => 0.0,
            Align::Center => 0.5,
            Align::End => 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Spec {
    id_salt: Option<IdSalt>,
    tracks: Vec<Track>,
    gap: Space,
    row_gap: Space,
    align: Align,
    justify: Align,
    equal: bool,
    fold_below: Option<Length>,
}

impl Spec {
    fn new(tracks: impl IntoIterator<Item = Track>) -> Self {
        let mut tracks: Vec<Track> = tracks.into_iter().collect();
        if tracks.is_empty() {
            tracks.push(Track::CONTENT);
        }
        Self {
            id_salt: None,
            tracks,
            gap: Space::Inline,
            row_gap: Space::Inline,
            align: Align::Baseline,
            justify: Align::Start,
            equal: false,
            fold_below: None,
        }
    }
}

/// One line of cells, one per track, shown by [`UiPop::row`]. Cells are
/// [`Space::Inline`] apart and on one baseline unless told otherwise.
#[derive(Clone, Debug)]
pub struct Row(pub(super) Spec);

impl Row {
    /// A row of `tracks`, one cell each.
    pub fn new(tracks: impl IntoIterator<Item = Track>) -> Self {
        Self(Spec::new(tracks))
    }

    /// Its id, for the sizes it keeps between passes; by default the
    /// `Ui`'s next automatic id.
    pub fn id_salt(mut self, salt: impl AsIdSalt) -> Self {
        self.0.id_salt = Some(IdSalt::new(salt));
        self
    }

    /// The space between cells (`Space::Inline` by default).
    pub fn gap(mut self, gap: Space) -> Self {
        self.0.gap = gap;
        self
    }

    /// How cells sit vertically ([`Align::Baseline`] by default).
    pub fn align(mut self, align: Align) -> Self {
        self.0.align = align;
        self
    }

    /// Where the cells go when they leave room (no [`Track::fr`]):
    /// [`Align::End`] for a button bar.
    pub fn justify(mut self, justify: Align) -> Self {
        self.0.justify = justify;
        self
    }

    /// Every content-sized cell as wide as the widest, its content
    /// stretched across it (buttons of a bar).
    pub fn equal_widths(mut self) -> Self {
        self.0.equal = true;
        self
    }
}

/// Lines of cells on shared tracks, shown by [`UiPop::grid`]: cells fill
/// the tracks left to right, a line per track count. Columns are
/// [`Space::Group`] apart, lines [`Space::Inline`].
#[derive(Clone, Debug)]
pub struct Grid(pub(super) Spec);

impl Grid {
    /// A grid of `tracks`, under an id unique in its `Ui`.
    pub fn new(id_salt: impl AsIdSalt, tracks: impl IntoIterator<Item = Track>) -> Self {
        let mut spec = Spec::new(tracks);
        spec.id_salt = Some(IdSalt::new(id_salt));
        spec.gap = Space::Group;
        Self(spec)
    }

    /// The space between columns (`Space::Group` by default).
    pub fn gap(mut self, gap: Space) -> Self {
        self.0.gap = gap;
        self
    }

    /// The space between lines (`Space::Inline` by default).
    pub fn row_gap(mut self, gap: Space) -> Self {
        self.0.row_gap = gap;
        self
    }

    /// How cells sit vertically in their line ([`Align::Baseline`] by
    /// default).
    pub fn align(mut self, align: Align) -> Self {
        self.0.align = align;
        self
    }

    /// Every content-sized column as wide as the widest.
    pub fn equal_widths(mut self) -> Self {
        self.0.equal = true;
        self
    }

    /// Below `width`, one column: each line's cells stacked full width (a
    /// header over its value), [`Space::Inline`] apart, lines
    /// [`Space::Group`] apart.
    pub fn fold_below(mut self, width: Length) -> Self {
        self.0.fold_below = Some(width);
        self
    }
}

/// A cell's size, measured from its top.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Cell {
    height: f32,
    baseline: f32,
}

/// What a layout keeps between passes.
#[derive(Clone, Debug, Default)]
struct Measured {
    /// The widest content of each column.
    content: Vec<f32>,
    /// Each line's cells.
    lines: Vec<Vec<Cell>>,
    /// The text painted in each equal-width column, to measure it again
    /// when it changes.
    texts: Vec<u64>,
    /// Measure the equal-width columns at their natural width next pass.
    remeasure: bool,
}

impl Measured {
    fn near(&self, other: &Self) -> bool {
        let close = |a: f32, b: f32| (a - b).abs() < 0.5;
        self.content.len() == other.content.len()
            && self
                .content
                .iter()
                .zip(&other.content)
                .all(|(a, b)| close(*a, *b))
            && self.lines.len() == other.lines.len()
            && self.lines.iter().zip(&other.lines).all(|(a, b)| {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|(a, b)| close(a.height, b.height) && close(a.baseline, b.baseline))
            })
            && self.texts == other.texts
            && self.remeasure == other.remeasure
    }
}

/// The cells of a [`Row`] or a [`Grid`], filled in order.
pub struct Cells<'a> {
    ui: &'a mut Ui,
    spec: Spec,
    id: egui::Id,
    prev: Measured,
    next: Measured,
    folded: bool,
    measuring: bool,
    xs: Vec<f32>,
    widths: Vec<f32>,
    origin: Pos2,
    full_width: f32,
    bottom_limit: f32,
    body_row: f32,
    body_baseline: f32,
    index: usize,
    line_top: f32,
    line_bottom: f32,
    right: f32,
    ppp: f32,
}

impl std::fmt::Debug for Cells<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cells")
            .field("id", &self.id)
            .field("index", &self.index)
            .field("folded", &self.folded)
            .field("widths", &self.widths)
            .finish_non_exhaustive()
    }
}

impl Cells<'_> {
    /// The next cell, `add` drawing its content top-down from its top-left
    /// corner.
    pub fn cell<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> R {
        let n = self.spec.tracks.len();
        let (line, col) = (self.index / n, self.index % n);
        let index = self.index;
        self.index += 1;
        let space = |s: Space| s.points(&Tokens::of(self.ui.ctx()));
        if index > 0 && (self.folded || col == 0) {
            let gap = match (self.folded, col) {
                (true, 0) => space(Space::Group).max(space(self.spec.row_gap)),
                (true, _) => space(Space::Inline),
                _ => space(self.spec.row_gap),
            };
            self.line_top = self.round(self.line_bottom + gap);
            self.line_bottom = self.line_top;
        }
        let track = self.spec.tracks[col];
        let content = track.size == Size::Content;
        let equal = self.spec.equal && content;
        let (x, width) = if self.folded {
            (self.origin.x, self.full_width)
        } else if equal && self.measuring {
            (self.xs[col], 0.0)
        } else {
            (self.xs[col], self.widths[col])
        };
        let offset = if self.folded {
            0.0
        } else {
            self.round(self.offset(line, col))
        };
        let top = self.line_top + offset;
        let rect = Rect::from_min_max(pos2(x, top), pos2(x + width, self.bottom_limit.max(top)));
        let layout = if equal {
            Layout::top_down_justified(EguiAlign::Center)
        } else {
            Layout::top_down(EguiAlign::Min)
        };
        let mut child = self.ui.new_child(
            UiBuilder::new()
                .id_salt(("egui_pop.cell", index))
                .max_rect(rect)
                .layout(layout),
        );
        if content && !self.folded {
            child.style_mut().wrap_mode = Some(TextWrapMode::Extend);
        }
        let layer = child.layer_id();
        let start = self
            .ui
            .ctx()
            .graphics(|g| g.get(layer).map_or(0, |l| l.next_idx().0));
        let out = add(&mut child);
        let used = child.min_rect();
        let (first, texts) = scan(self.ui.ctx(), layer, start);
        let height = (used.bottom() - top).max(0.0);
        let baseline = first.map_or((height - self.body_row) / 2.0 + self.body_baseline, |y| {
            y - top
        });
        self.line_bottom = self.line_bottom.max(used.bottom());
        self.right = self.right.max(used.right()).max(rect.right());
        if !self.folded {
            if content {
                // Its width, not its right edge: content wider than a
                // cell measured at no width spills on both sides.
                let w = used.width().max(0.0);
                self.next.content[col] = self.next.content[col].max(w);
            }
            if equal {
                let mut h = std::hash::DefaultHasher::new();
                (self.next.texts[col], texts).hash(&mut h);
                self.next.texts[col] = h.finish();
            }
            if self.next.lines.len() <= line {
                self.next.lines.resize(line + 1, vec![Cell::default(); n]);
            }
            self.next.lines[line][col] = Cell { height, baseline };
        }
        out
    }

    /// A header cell: `text` in the muted text role, a header/value grid's
    /// first column.
    pub fn header(&mut self, text: impl Into<String>) -> Response {
        let text = text.into();
        self.cell(|ui| ui.text_role(TextRole::Muted, text))
    }

    /// The offset of a cell from its line's top, from the previous pass.
    fn offset(&self, line: usize, col: usize) -> f32 {
        let Some(cells) = self.prev.lines.get(line) else {
            return 0.0;
        };
        let Some(cell) = cells.get(col) else {
            return 0.0;
        };
        match self.spec.align {
            Align::Start => 0.0,
            Align::Baseline => {
                let base = cells.iter().map(|c| c.baseline).fold(0.0, f32::max);
                base - cell.baseline
            }
            align => {
                let tallest = cells.iter().map(|c| c.height).fold(0.0, f32::max);
                (tallest - cell.height) * align.share()
            }
        }
    }

    fn round(&self, v: f32) -> f32 {
        (v * self.ppp).round() / self.ppp
    }
}

/// The topmost text baseline painted on `layer` from shape `start`, and a
/// hash of the texts painted.
fn scan(ctx: &egui::Context, layer: egui::LayerId, start: usize) -> (Option<f32>, u64) {
    fn visit(shape: &Shape, first: &mut Option<f32>, hash: &mut std::hash::DefaultHasher) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| visit(s, first, hash)),
            Shape::Text(t) => {
                t.galley.text().hash(hash);
                if t.angle != 0.0 {
                    return;
                }
                let Some(row) = t.galley.rows.iter().find(|r| !r.row.glyphs.is_empty()) else {
                    return;
                };
                let y = t.pos.y + row.pos.y + row.row.glyphs[0].pos.y;
                *first = Some(first.map_or(y, |f| f.min(y)));
            }
            _ => {}
        }
    }
    let mut first = None;
    let mut hash = std::hash::DefaultHasher::new();
    ctx.graphics(|g| {
        if let Some(list) = g.get(layer) {
            list.all_entries()
                .skip(start)
                .for_each(|c| visit(&c.shape, &mut first, &mut hash));
        }
    });
    (first, hash.finish())
}

/// Widths of `tracks` in a container `width` wide with `gap` between them.
fn widths(
    tracks: &[Track],
    content: &[f32],
    width: f32,
    gap: f32,
    em: f32,
    equal: bool,
) -> Vec<f32> {
    let mut out: Vec<f32> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| match t.size {
            Size::Content => t.clamp(content.get(i).copied().unwrap_or(0.0), em, width),
            Size::Fixed(l) => t.clamp(l.points(em, width), em, width),
            Size::Fr(_) => 0.0,
        })
        .collect();
    if equal {
        let widest = tracks
            .iter()
            .zip(&out)
            .filter(|(t, _)| t.size == Size::Content)
            .map(|(_, w)| *w)
            .fold(0.0, f32::max);
        for (t, w) in tracks.iter().zip(out.iter_mut()) {
            if t.size == Size::Content {
                *w = widest;
            }
        }
    }
    // What is left goes to the fractions by weight; a fraction held at its
    // minimum or maximum leaves the rest to the others.
    let gaps = gap * tracks.len().saturating_sub(1) as f32;
    let mut left = width - gaps - out.iter().sum::<f32>();
    let mut open: Vec<usize> = (0..tracks.len())
        .filter(|i| matches!(tracks[*i].size, Size::Fr(_)))
        .collect();
    while !open.is_empty() {
        let weight: f32 = open
            .iter()
            .map(|i| match tracks[*i].size {
                Size::Fr(w) => w,
                _ => 0.0,
            })
            .sum();
        let share = |i: usize| match tracks[i].size {
            Size::Fr(w) if weight > 0.0 => left.max(0.0) * w / weight,
            _ => 0.0,
        };
        let held = open.iter().position(|&i| {
            let s = share(i);
            (tracks[i].clamp(s, em, width) - s).abs() > 0.01
        });
        match held {
            Some(k) => {
                let i = open.remove(k);
                out[i] = tracks[i].clamp(share(i), em, width);
                left -= out[i];
            }
            None => {
                for &i in &open {
                    out[i] = share(i);
                }
                break;
            }
        }
    }
    out
}

pub(super) fn show<R>(
    ui: &mut Ui,
    spec: Spec,
    add: impl FnOnce(&mut Cells<'_>) -> R,
) -> InnerResponse<R> {
    let id = match spec.id_salt {
        Some(salt) => ui.make_persistent_id(salt),
        None => {
            let id = ui.next_auto_id();
            ui.skip_ahead_auto_ids(1);
            id
        }
    };
    let tokens = Tokens::of(ui.ctx());
    let em = tokens.text_role.body.size.max(1.0);
    let avail = ui.available_rect_before_wrap();
    let full_width = avail.width().max(0.0);
    let prev = ui
        .data(|d| d.get_temp::<Measured>(id))
        .unwrap_or_else(|| Measured {
            remeasure: true,
            ..Measured::default()
        });
    let folded = spec
        .fold_below
        .is_some_and(|l| full_width < l.points(em, full_width));
    let n = spec.tracks.len();
    let gap = spec.gap.points(&tokens);
    let mut content = prev.content.clone();
    content.resize(n, 0.0);
    let ws = widths(&spec.tracks, &content, full_width, gap, em, spec.equal);
    let ppp = ui.ctx().pixels_per_point();
    let round = |v: f32| (v * ppp).round() / ppp;
    let total = ws.iter().sum::<f32>() + gap * n.saturating_sub(1) as f32;
    let mut x = avail.left() + (full_width - total).max(0.0) * spec.justify.share();
    let mut xs = Vec::with_capacity(n);
    for w in &ws {
        xs.push(round(x));
        x += w + gap;
    }
    let body = TextStyle::Body.resolve(ui.style());
    let probe = ui
        .painter()
        .layout_no_wrap("Hg".to_owned(), body, egui::Color32::WHITE);
    let body_row = probe.size().y;
    let body_baseline = probe
        .rows
        .first()
        .and_then(|r| r.row.glyphs.first().map(|g| r.pos.y + g.pos.y))
        .unwrap_or(body_row);
    let top = round(avail.top());
    let measuring = prev.remeasure;
    let mut next = Measured {
        content: vec![0.0; n],
        texts: vec![0; n],
        ..Measured::default()
    };
    if folded {
        next = prev.clone();
    }
    let mut cells = Cells {
        ui,
        id,
        prev,
        next,
        folded,
        measuring,
        xs,
        widths: ws,
        origin: avail.min,
        full_width,
        bottom_limit: avail.bottom(),
        body_row,
        body_baseline,
        index: 0,
        line_top: top,
        line_bottom: top,
        right: avail.left(),
        ppp,
        spec,
    };
    let inner = add(&mut cells);
    let Cells {
        ui,
        id,
        prev,
        mut next,
        folded,
        spec,
        line_bottom,
        right,
        ..
    } = cells;
    if !folded {
        next.remeasure = spec.equal && next.texts != prev.texts;
        if measuring {
            // Natural widths measured: the next pass shows them.
            next.remeasure = false;
        }
    }
    let changed = !next.near(&prev) || measuring;
    if changed {
        ui.data_mut(|d| d.insert_temp(id, next));
        ui.ctx()
            .request_discard("egui-pop row or grid sizes changed");
    }
    let has_fr = spec.tracks.iter().any(|t| matches!(t.size, Size::Fr(_)));
    let right = if has_fr || folded {
        right.max(avail.right())
    } else {
        right
    };
    let rect = Rect::from_min_max(avail.min, pos2(right, line_bottom.max(avail.top())));
    let response = ui.allocate_rect(rect, Sense::hover());
    InnerResponse::new(inner, response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_share_what_is_left_and_respect_their_bounds() {
        let em = 10.0;
        let t = [Track::fr(1.0), Track::fr(3.0)];
        assert_eq!(widths(&t, &[], 410.0, 10.0, em, false), vec![100.0, 300.0]);
        let t = [Track::fr(1.0).min(Length::Em(20.0)), Track::fr(3.0)];
        assert_eq!(widths(&t, &[], 410.0, 10.0, em, false), vec![200.0, 200.0]);
        let t = [Track::fr(1.0), Track::fr(1.0).max(Length::Percent(10.0))];
        assert_eq!(widths(&t, &[], 400.0, 0.0, em, false), vec![360.0, 40.0]);
        let t = [
            Track::CONTENT,
            Track::em(5.0),
            Track::percent(25.0),
            Track::fr(1.0),
        ];
        assert_eq!(
            widths(&t, &[30.0], 400.0, 10.0, em, false),
            vec![30.0, 50.0, 100.0, 190.0]
        );
    }

    #[test]
    fn equal_widths_take_the_widest_content() {
        let t = [Track::CONTENT; 3];
        assert_eq!(
            widths(&t, &[20.0, 50.0, 35.0], 400.0, 0.0, 10.0, true),
            vec![50.0; 3]
        );
    }
}
