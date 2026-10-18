//! Placeholders: skeletons standing in for content that fills
//! progressively (a metadata panel, a queue's rows, parameters while
//! they load), and empty states saying why a place is empty and what to
//! do about it.
//!
//! - [`Skeleton`]: grey shapes in the content's layout (the kit's
//!   `skeleton`) with a shimmer crossing them (`skeleton-shimmer`, in
//!   `motion.skeleton.shimmer`); no shimmer under reduced motion, nor
//!   under a given motion clock (tests). Its region carries AccessKit's
//!   `busy` flag and the name "Loading", so assistive technologies wait
//!   for the content rather than read grey boxes. Not for thumbnails:
//!   use what is known (an embedded preview, an average colour) instead;
//! - [`EmptyState`]: an icon, a title, a sentence and an optional
//!   action; [`EmptyState::show`] says whether the action was clicked
//!   (the application runs its command).
//!
//! ```
//! use egui_pop_core::icon::Icon;
//! use egui_pop_core::placeholder::{EmptyState, Skeleton};
//!
//! fn ui(ui: &mut egui::Ui, loaded: bool, images: usize) {
//!     if !loaded {
//!         ui.add(Skeleton::rows(4));
//!     } else if images == 0 {
//!         let import = EmptyState::new(Icon::Folder, "No images in this folder")
//!             .text("Import images to see them here.")
//!             .action("Import images")
//!             .show(ui);
//!         let _ = import;
//!     }
//! }
//! ```

use egui::{Color32, Rect, Response, Sense, Ui, Widget, WidgetInfo, WidgetType, pos2, vec2};

use crate::icon::Icon;
use crate::kit::{Kit, KitCx, Part, State, Tone};
use crate::motion;
use crate::strings;
use crate::theme::Tokens;

/// The skeleton colours of the installed kit, for what stands in for
/// content without being a skeleton (a thumbnail not loaded yet).
#[derive(Clone, Debug, PartialEq)]
pub struct SkeletonTokens {
    /// `skeleton.rest.fill`.
    pub fill: Color32,
    /// `skeleton-shimmer.rest.fill`.
    pub shimmer: Color32,
}

impl SkeletonTokens {
    pub fn of(ctx: &egui::Context) -> SkeletonTokens {
        let kit = Kit::of(ctx);
        SkeletonTokens {
            fill: kit.skeleton.style(State::REST).fill,
            shimmer: kit.skeleton_shimmer.style(State::REST).fill,
        }
    }
}

/// A shape of a skeleton.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    /// A line of text, this share of the width.
    Line(f32),
    /// A square block (an avatar, an icon).
    Block,
}

/// Grey shapes standing in for content.
#[derive(Clone, Debug, PartialEq)]
#[must_use = "add it with `ui.add`"]
pub struct Skeleton {
    /// Rows of shapes, each row its shapes left to right.
    rows: Vec<Vec<Shape>>,
    width: Option<f32>,
    label: Option<String>,
}

impl Skeleton {
    /// A paragraph of `lines` lines, the last one shorter.
    pub fn text(lines: usize) -> Self {
        let rows = (0..lines.max(1))
            .map(|i| vec![Shape::Line(if i + 1 == lines.max(1) { 0.6 } else { 1.0 })])
            .collect();
        Self {
            rows,
            width: None,
            label: None,
        }
    }

    /// List rows: a block and two lines each.
    pub fn rows(rows: usize) -> Self {
        Self {
            rows: (0..rows.max(1))
                .map(|_| vec![Shape::Block, Shape::Line(1.0)])
                .collect(),
            width: None,
            label: None,
        }
    }

    /// Its width (the available width by default).
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// What is loading ("Loading the metadata"); "Loading" by default.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Widget for Skeleton {
    fn ui(self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx().clone();
        let t = Tokens::of(&ctx);
        let kit = Kit::of(&ctx);
        let line = t.font.body;
        let gap = t.space.s;
        let block = t.size.control_height;
        let width = self
            .width
            .unwrap_or_else(|| ui.available_width())
            .max(block * 2.0);
        let row_h = |row: &Vec<Shape>| {
            if row.iter().any(|s| matches!(s, Shape::Block)) {
                block
            } else {
                line
            }
        };
        let height: f32 = self.rows.iter().map(row_h).sum::<f32>()
            + gap * (self.rows.len().saturating_sub(1)) as f32;
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);
            let cx = KitCx {
                painter: &painter,
                ppp: ctx.pixels_per_point(),
            };
            let part_at = |rect: Rect| Part {
                rect,
                room: rect,
                state: State::REST,
                label: None,
            };
            let mut shapes = Vec::new();
            let mut y = rect.top();
            for row in &self.rows {
                let h = row_h(row);
                let mut x = rect.left();
                for shape in row {
                    match *shape {
                        Shape::Block => {
                            shapes.push(Rect::from_min_size(pos2(x, y), vec2(block, block)));
                            x += block + t.space.m;
                        }
                        Shape::Line(share) if h > line => {
                            // Beside a block: two lines.
                            let w = (rect.right() - x) * share;
                            let l = (h - gap) / 2.0;
                            shapes.push(Rect::from_min_size(pos2(x, y), vec2(w, l * 0.8)));
                            shapes.push(Rect::from_min_size(
                                pos2(x, y + l + gap),
                                vec2(w * 0.6, l * 0.8),
                            ));
                        }
                        Shape::Line(share) => {
                            let w = (rect.right() - x) * share;
                            shapes.push(Rect::from_min_size(pos2(x, y), vec2(w, line * 0.8)));
                        }
                    }
                }
                y += h + gap;
            }
            for r in &shapes {
                kit.skeleton.paint(&cx, &part_at(*r));
            }
            // The shimmer: a lighter band crossing, never in tests nor
            // under reduced motion.
            if !motion::reduced(&ctx) && !motion::controlled(&ctx) {
                let p = motion::phase(&ctx, f64::from(t.motion.skeleton_shimmer), 0.0);
                let band = rect.width() * 0.25;
                let x = rect.left() - band + p * (rect.width() + band);
                let shine = Rect::from_min_max(pos2(x, rect.top()), pos2(x + band, rect.bottom()));
                for r in &shapes {
                    let part = r.intersect(shine);
                    if part.is_positive() {
                        kit.skeleton_shimmer.paint(&cx, &part_at(part));
                    }
                }
                motion::animate(&ctx);
            }
        }
        let label = self
            .label
            .unwrap_or_else(|| strings::tr(&ctx, &strings::LOADING));
        let enabled = ui.is_enabled();
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, enabled, &label));
        ctx.accesskit_node_builder(response.id, |node| node.set_busy());
        response
    }
}

/// An empty place: an icon, a title, a sentence, an action.
#[derive(Clone, Debug, PartialEq)]
#[must_use = "show it with `show`"]
pub struct EmptyState {
    icon: Icon,
    title: String,
    text: Option<String>,
    action: Option<String>,
    danger: bool,
}

impl EmptyState {
    pub fn new(icon: Icon, title: impl Into<String>) -> Self {
        Self {
            icon,
            title: title.into(),
            text: None,
            action: None,
            danger: false,
        }
    }

    /// A sentence under the title: why, and what to do.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// A button under the sentence ("Import images", "Retry").
    pub fn action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self
    }

    /// Something went wrong: the icon in the danger colour.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// Draws it centred in the available width; whether its action was
    /// clicked.
    pub fn show(self, ui: &mut Ui) -> bool {
        let t = Tokens::of(ui.ctx());
        let kit = Kit::of(ui.ctx());
        let empty = &kit.empty_state;
        let tone = if self.danger {
            Tone::Danger
        } else {
            Tone::Neutral
        };
        let state = State::REST.toned(tone);
        let pad = empty.data.padding_y;
        let mut clicked = false;
        // The box goes under the content, painted once its size is known.
        let slot = ui.painter().add(egui::Shape::Noop);
        let r = ui.vertical_centered(|ui| {
            ui.add_space(pad);
            let side = t.size.icon_panel * 2.0;
            let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
            if ui.is_rect_visible(rect) {
                self.icon.paint(ui.painter(), rect, empty.style(state).text);
            }
            ui.add_space(t.space.s);
            ui.label(egui::RichText::new(&self.title).strong());
            if let Some(text) = &self.text {
                ui.label(egui::RichText::new(text).color(empty.style(State::REST).text));
            }
            if let Some(action) = &self.action {
                ui.add_space(t.space.s);
                clicked = ui.button(action).clicked();
            }
            ui.add_space(pad);
        });
        let rect = r.response.rect;
        empty.paint_under(
            ui,
            slot,
            &Part {
                rect,
                room: rect,
                state,
                label: None,
            },
            None,
        );
        clicked
    }
}
