//! Collapsible sections stacked in a panel: each a title band (a
//! disclosure mark, a title, a count) over its own scrolling body, as a
//! code editor's side bar stacks its views.
//!
//! - A click on a band, or Space or Enter while it has the keyboard,
//!   folds or opens its section. A folded section keeps its band in
//!   place; open sections share the panel's height, so the bands of
//!   folded sections below the last open one sit at the panel's bottom.
//! - The line above a band, between two open sections, is a boundary:
//!   dragged, it resizes the open section above it against the next
//!   open one below; a double click gives every section its share back
//!   (equal shares).
//! - Each section keeps a least height (two rows by default,
//!   [`Section::min_height`]) and scrolls its own body.
//! - What is folded and the shares are remembered with the window's
//!   layout (egui's persisted memory, under the stack's name), and
//!   readable as a [`Layout`] an application keeps with its own.
//!
//! The kit draws the band (`section-band`, handler `section`), its mark
//! (`section-disclosure`, the `disclosure-open` and `disclosure-closed`
//! glyph roles) and its count (`section-count`, based on
//! `indicator-badge`).
//!
//! **Named and observable.** A band is a button named by its title and
//! count ("Live, 3"), expanded or not for assistive technology; a
//! boundary is a splitter named after the section above it ("Resize
//! Live"), its value the height of that section's body. [`laid_out`]
//! gives each section as drawn on the last pass (the debug bridge's
//! `state` collector lists it too), so an agent reads the stack rather
//! than pixels.
//!
//! ```
//! use egui_pop_core::sections::{Section, Sections};
//!
//! let ctx = egui::Context::default();
//! let mut out = ctx.run_ui(Default::default(), |ui| {
//!     Sections::new("sessions").show(ui, |stack| {
//!         stack.section(Section::new("live", "Live").count(2), |ui| {
//!             ui.label("build");
//!             ui.label("tests");
//!         });
//!         stack.section(Section::new("idle", "Idle").count(0).open(false), |ui| {
//!             ui.weak("No idle session");
//!         });
//!     });
//! });
//! out.textures_delta.clear();
//! let live = &egui_pop_core::sections::laid_out(&ctx, "sessions")[0];
//! assert!(live.open && live.body.is_some());
//! ```

use std::collections::BTreeMap;

use egui::accesskit::Role;
use egui::{
    Color32, Context, CursorIcon, FontFamily, FontId, Id, Rect, Response, Sense, Ui, UiBuilder,
    WidgetInfo, WidgetType, pos2, vec2,
};
use serde::{Deserialize, Serialize};

use crate::indicator;
use crate::kit::{Glyph, Kit, KitCx, Part, State};
use crate::state::Slot;
use crate::strings::{self, SECTION_RESIZE};
use crate::theme::{TextRole, Tokens};

/// What a stack remembers of one section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Remembered {
    /// Whether the section is open.
    pub open: bool,
    /// Its share of the height the open sections divide (1 by default):
    /// kept while folded, so a section opens again at its size.
    pub share: f32,
}

/// What a stack remembers, by section key: kept in egui's persisted
/// memory under the stack's name, and readable and writable whole
/// ([`Layout::load`], [`Layout::store`]) for an application that keeps
/// it with its own layout.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub sections: BTreeMap<String, Remembered>,
}

impl Layout {
    /// The layout a stack named `name` remembers, if any.
    pub fn load(ctx: &Context, name: &str) -> Option<Layout> {
        let id = memory_id(name);
        ctx.data_mut(|d| d.get_persisted::<Layout>(id))
    }

    /// Makes it the layout of the stack named `name` (shown so from the
    /// next pass).
    pub fn store(&self, ctx: &Context, name: &str) {
        let id = memory_id(name);
        let layout = self.clone();
        ctx.data_mut(|d| d.insert_persisted(id, layout));
    }

    /// Whether a section is open, if the layout knows it.
    pub fn is_open(&self, key: &str) -> Option<bool> {
        self.sections.get(key).map(|s| s.open)
    }

    /// Opens or folds a section.
    pub fn set_open(&mut self, key: &str, open: bool) {
        self.sections
            .entry(key.to_string())
            .or_insert(Remembered { open, share: 1.0 })
            .open = open;
    }

    /// Gives every section its share back (a boundary's double click).
    pub fn reset_heights(&mut self) {
        for s in self.sections.values_mut() {
            s.share = 1.0;
        }
    }
}

fn memory_id(name: &str) -> Id {
    Id::new(("egui_pop.sections", name))
}

/// A section as laid out on the last pass ([`laid_out`]).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Laid {
    pub key: String,
    pub title: String,
    pub count: Option<u64>,
    pub open: bool,
    /// Its band.
    #[serde(skip)]
    pub band: Rect,
    /// Its body, when open.
    #[serde(skip)]
    pub body: Option<Rect>,
}

/// Each stack's sections as laid out on the last pass, by name.
const LAID: Slot<Vec<Laid>> = Slot::new("core.sections.laid");

/// The sections of the stack named `name` as drawn on the last pass, in
/// order (empty when it was not drawn).
pub fn laid_out(ctx: &Context, name: &str) -> Vec<Laid> {
    LAID.keyed(name.to_string()).get(ctx).unwrap_or_default()
}

/// One section: a key (stable, what the layout remembers it by), a
/// title, an optional count, its least height and whether it starts
/// open.
#[derive(Clone, Debug)]
pub struct Section {
    key: String,
    title: String,
    count: Option<u64>,
    min_height: Option<f32>,
    open: bool,
}

impl Section {
    /// An open section with no count.
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        Section {
            key: key.into(),
            title: title.into(),
            count: None,
            min_height: None,
            open: true,
        }
    }

    /// The count on its band (`99+` past 99; its name says the real
    /// count).
    pub fn count(mut self, count: u64) -> Self {
        self.count = Some(count);
        self
    }

    /// Its body's least height, in points: two rows by default (room
    /// for two rows, or the line saying it is empty).
    pub fn min_height(mut self, points: f32) -> Self {
        self.min_height = Some(points.max(0.0));
        self
    }

    /// Whether it is open until the user says otherwise (open by
    /// default); the layout remembered wins.
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Its band's accessible name: the title, and the count when there
    /// is one ("Live, 3").
    fn name(&self) -> String {
        match self.count {
            Some(n) => format!("{}, {n}", self.title),
            None => self.title.clone(),
        }
    }
}

type Body<'a> = Box<dyn FnOnce(&mut Ui) + 'a>;

/// The sections a stack is given, in order ([`Sections::show`]).
pub struct Stack<'a> {
    sections: Vec<(Section, Body<'a>)>,
}

impl std::fmt::Debug for Stack<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.sections.iter().map(|(s, _)| &s.key))
            .finish()
    }
}

impl<'a> Stack<'a> {
    /// Adds a section, `body` its rows (called only while it is open,
    /// in a scroll area of its own).
    pub fn section(&mut self, section: Section, body: impl FnOnce(&mut Ui) + 'a) -> &mut Self {
        self.sections.push((section, Box::new(body)));
        self
    }
}

/// What the user did to a stack this pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SectionsResponse {
    /// A section folded (`false`) or opened (`true`), by key.
    pub toggled: Option<(String, bool)>,
    /// A boundary was dragged.
    pub resized: bool,
    /// A boundary was double-clicked: every share given back.
    pub reset: bool,
    /// The layout after the pass.
    pub layout: Layout,
}

/// A stack of collapsible sections filling the room it is given (a
/// panel's height), named for its memory and for agents.
#[derive(Clone, Debug)]
pub struct Sections {
    name: String,
    capitals: bool,
}

impl Sections {
    /// A stack named `name`, unique in the application: its layout is
    /// remembered under it and [`laid_out`] reads it.
    pub fn new(name: impl Into<String>) -> Self {
        Sections {
            name: name.into(),
            capitals: true,
        }
    }

    /// Whether titles are drawn in capitals (yes by default, as a code
    /// editor's side bar does); their accessible names keep their case.
    pub fn capitals(mut self, capitals: bool) -> Self {
        self.capitals = capitals;
        self
    }

    /// Lays the sections `add` gives out in the room left in `ui`.
    pub fn show<'a>(self, ui: &mut Ui, add: impl FnOnce(&mut Stack<'a>)) -> SectionsResponse {
        let mut stack = Stack {
            sections: Vec::new(),
        };
        add(&mut stack);
        self.lay_out(ui, stack.sections)
    }

    fn lay_out(&self, ui: &mut Ui, sections: Vec<(Section, Body<'_>)>) -> SectionsResponse {
        let ctx = ui.ctx().clone();
        let kit = Kit::of(&ctx);
        let tokens = Tokens::of(&ctx);
        let row = ui.spacing().interact_size.y;
        let band_h = if kit.section_band.data.height > 0.0 {
            kit.section_band.data.height
        } else {
            row + 4.0
        };

        let mut layout = Layout::load(&ctx, &self.name).unwrap_or_default();
        for (s, _) in &sections {
            layout.sections.entry(s.key.clone()).or_insert(Remembered {
                open: s.open,
                share: 1.0,
            });
        }
        let remembered: Vec<Remembered> = sections
            .iter()
            .map(|(s, _)| layout.sections[&s.key])
            .collect();
        let mins: Vec<f32> = sections
            .iter()
            .map(|(s, _)| s.min_height.unwrap_or(2.0 * row))
            .collect();

        let room = ui.available_rect_before_wrap();
        let n = sections.len() as f32;
        let space = if room.height().is_finite() {
            (room.height() - n * band_h).max(0.0)
        } else {
            0.0
        };
        let open: Vec<usize> = (0..sections.len())
            .filter(|&i| remembered[i].open)
            .collect();
        let shares: Vec<(f32, f32)> = open
            .iter()
            .map(|&i| (remembered[i].share.max(0.01), mins[i]))
            .collect();
        let heights = distribute(space, &shares);
        let mut body_h = vec![0.0; sections.len()];
        for (k, &i) in open.iter().enumerate() {
            body_h[i] = heights[k];
        }

        let mut response = SectionsResponse::default();
        let mut laid = Vec::with_capacity(sections.len());
        let mut y = room.top();
        let base = memory_id(&self.name);
        for (i, (section, body)) in sections.into_iter().enumerate() {
            let is_open = remembered[i].open;
            let band = Rect::from_min_size(pos2(room.left(), y), vec2(room.width(), band_h));
            let r = ui.interact(band, base.with(("band", &section.key)), Sense::click());
            self.paint_band(ui, &kit, &tokens, &section, band, &r, is_open);
            name_band(&ctx, &r, &section.name(), is_open);
            if r.clicked() {
                layout.set_open(&section.key, !is_open);
                response.toggled = Some((section.key.clone(), !is_open));
            }
            y = band.bottom();
            let mut body_rect = None;
            if is_open {
                let rect = Rect::from_min_size(pos2(room.left(), y), vec2(room.width(), body_h[i]));
                let mut child = ui.new_child(
                    UiBuilder::new()
                        .id_salt(("egui_pop.sections.body", &self.name, &section.key))
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                child.set_clip_rect(rect.intersect(ui.clip_rect()));
                egui::ScrollArea::vertical()
                    .id_salt(("egui_pop.sections.scroll", &section.key))
                    .auto_shrink([false, false])
                    .max_height(rect.height())
                    .show(&mut child, body);
                y = rect.bottom();
                body_rect = Some(rect);
            }
            laid.push(Laid {
                key: section.key,
                title: section.title,
                count: section.count,
                open: is_open,
                band,
                body: body_rect,
            });
        }

        // The boundaries, registered after the bands so they win the
        // pointer over a band's top edge.
        let reach = ui.style().interaction.resize_grab_radius_side;
        for i in 1..laid.len() {
            let above = i - 1;
            let Some(below) = (i..laid.len()).find(|&j| laid[j].open) else {
                break;
            };
            if !laid[above].open {
                continue;
            }
            let top = laid[i].band.top();
            let edge = Rect::from_x_y_ranges(room.x_range(), top - reach..=top + reach);
            let id = base.with(("boundary", &laid[above].key));
            let r = ui.interact(edge, id, Sense::DRAG | Sense::CLICK);
            let name = strings::tr_args(&ctx, &SECTION_RESIZE, &[("section", &laid[above].title)]);
            let value = body_h[above];
            r.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &name));
            ctx.accesskit_node_builder(r.id, |node| {
                node.set_role(Role::Splitter);
                node.set_numeric_value(f64::from(value));
            });
            if r.hovered() || r.dragged() {
                ctx.set_cursor_icon(CursorIcon::ResizeVertical);
            }
            if r.double_clicked() {
                layout.reset_heights();
                response.reset = true;
            } else if r.dragged() && r.drag_delta().y != 0.0 {
                let (ha, hb) = (body_h[above], body_h[below]);
                let total = ha + hb;
                if total > 0.0 {
                    let max_a = (total - mins[below]).max(mins[above]);
                    let new_a = (ha + r.drag_delta().y).clamp(mins[above], max_a);
                    let (ka, kb) = (&laid[above].key, &laid[below].key);
                    let pool = layout.sections[ka].share + layout.sections[kb].share;
                    let share_a = pool * new_a / total;
                    if let Some(s) = layout.sections.get_mut(ka) {
                        s.share = share_a.max(0.01);
                    }
                    if let Some(s) = layout.sections.get_mut(kb) {
                        s.share = (pool - share_a).max(0.01);
                    }
                    response.resized = true;
                }
            }
        }

        if response.toggled.is_some() || response.resized || response.reset {
            ctx.request_repaint();
        }
        layout.store(&ctx, &self.name);
        LAID.keyed(self.name.clone()).set(&ctx, laid);
        ui.advance_cursor_after_rect(Rect::from_min_max(
            room.min,
            pos2(room.right(), y.max(room.top())),
        ));
        response.layout = layout;
        response
    }

    /// Paints a band: the kit's `section-band`, its mark, its title and
    /// its count.
    #[allow(clippy::too_many_arguments)]
    fn paint_band(
        &self,
        ui: &Ui,
        kit: &Kit,
        tokens: &Tokens,
        section: &Section,
        band: Rect,
        r: &Response,
        open: bool,
    ) {
        if !ui.is_rect_visible(band) {
            return;
        }
        let painter = ui.painter();
        let cx = KitCx {
            painter,
            ppp: ui.ctx().pixels_per_point(),
        };
        let state = State::from_response(r);
        let data = &kit.section_band.data;
        let gap = tokens.space.inline;
        kit.section_band.paint(
            &cx,
            &Part {
                rect: band,
                room: band,
                state,
                label: None,
            },
        );

        // The disclosure mark at the start.
        let mark = &kit.section_disclosure;
        let side = if mark.data.width > 0.0 {
            mark.data.width
        } else {
            10.0
        };
        let mark_rect = Rect::from_center_size(
            pos2(band.left() + data.padding_x + side / 2.0, band.center().y),
            vec2(side, side),
        );
        let mark_part = Part {
            rect: mark_rect,
            room: mark_rect,
            state,
            label: None,
        };
        mark.paint(&cx, &mark_part);
        let glyph = if open {
            Glyph::DisclosureOpen
        } else {
            Glyph::DisclosureClosed
        };
        kit.glyph(glyph)
            .paint(painter, mark_rect, mark.style(state).text);

        // The count at the end.
        let mut end = band.right() - data.padding_x;
        if let Some(n) = section.count {
            let count = &kit.section_count;
            let size = if count.data.font_size > 0.0 {
                count.data.font_size
            } else {
                tokens.font.small
            };
            let galley = painter.layout_no_wrap(
                indicator::badge_text(n, indicator::CAP),
                FontId::proportional(size),
                Color32::PLACEHOLDER,
            );
            let measured = count.measure(galley.size());
            let pill = Rect::from_min_size(
                pos2(end - measured.x, band.center().y - measured.y / 2.0),
                measured,
            );
            count.paint_part(
                &cx,
                &Part {
                    rect: pill,
                    room: pill,
                    state: State::REST,
                    label: Some(&galley),
                },
            );
            end = pill.left() - gap;
        }

        // The title between them, clipped to its room.
        let title = if self.capitals {
            section.title.to_uppercase()
        } else {
            section.title.clone()
        };
        let size = if data.font_size > 0.0 {
            data.font_size
        } else {
            tokens.text_role.get(TextRole::Section).size
        };
        let family =
            crate::ui::role_family(ui.ctx(), tokens.text_role.get(TextRole::Section).weight)
                .unwrap_or(FontFamily::Proportional);
        let galley = painter.layout_no_wrap(title, FontId::new(size, family), Color32::PLACEHOLDER);
        // The label's room, less the padding `paint_label` takes off.
        let start = mark_rect.right() + gap - data.padding_x;
        let room = Rect::from_x_y_ranges(start..=(end + data.padding_x).max(start), band.y_range());
        let clipped = cx.painter.with_clip_rect(
            Rect::from_x_y_ranges(
                mark_rect.right()..=end.max(mark_rect.right()),
                band.y_range(),
            )
            .intersect(painter.clip_rect()),
        );
        kit.section_band.paint_label(
            &KitCx {
                painter: &clipped,
                ppp: cx.ppp,
            },
            &Part {
                rect: band,
                room,
                state,
                label: Some(&galley),
            },
        );
    }
}

/// Names a band for assistive technology: a button, expanded or not.
fn name_band(ctx: &Context, r: &Response, name: &str, open: bool) {
    crate::a11y::name(r, WidgetType::Button, name);
    ctx.accesskit_node_builder(r.id, |node| node.set_expanded(open));
}

/// Divides `space` between parts by their shares, each at least its
/// least height (`(share, min)`); when the least heights do not fit,
/// each gets its least and the stack overflows.
fn distribute(space: f32, parts: &[(f32, f32)]) -> Vec<f32> {
    let mut pinned = vec![false; parts.len()];
    loop {
        let fixed: f32 = parts
            .iter()
            .zip(&pinned)
            .filter(|(_, p)| **p)
            .map(|((_, m), _)| *m)
            .sum();
        let shares: f32 = parts
            .iter()
            .zip(&pinned)
            .filter(|(_, p)| !**p)
            .map(|((s, _), _)| *s)
            .sum();
        let free = (space - fixed).max(0.0);
        let mut changed = false;
        for (i, (share, min)) in parts.iter().enumerate() {
            if !pinned[i] && shares > 0.0 && free * share / shares < *min {
                pinned[i] = true;
                changed = true;
            }
        }
        if !changed {
            return parts
                .iter()
                .zip(&pinned)
                .map(|((share, min), p)| {
                    if *p || shares <= 0.0 {
                        *min
                    } else {
                        free * share / shares
                    }
                })
                .collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::distribute;

    #[test]
    fn shares_divide_the_room_and_least_heights_hold() {
        assert_eq!(
            distribute(300.0, &[(1.0, 10.0), (2.0, 10.0)]),
            [100.0, 200.0]
        );
        // The small share is pinned at its least height.
        assert_eq!(distribute(100.0, &[(0.1, 40.0), (1.0, 10.0)]), [40.0, 60.0]);
        // Not enough room: each its least.
        assert_eq!(distribute(30.0, &[(1.0, 20.0), (1.0, 20.0)]), [20.0, 20.0]);
        assert!(distribute(100.0, &[]).is_empty());
    }
}
