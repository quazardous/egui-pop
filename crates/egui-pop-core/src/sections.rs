//! Collapsible sections stacked in a panel: each a title band (a
//! disclosure mark, a title, a count) over its own scrolling body, as a
//! code editor's side bar stacks its views.
//!
//! - A click on a band, or Space or Enter while it has the keyboard,
//!   folds or opens its section; Left folds it and Right opens it. A
//!   folded section keeps its band in place; open sections fill the
//!   panel's height, so the bands of folded sections below the last open
//!   one sit at the panel's bottom.
//! - A section's height is set by hand, in points, or sized by the list:
//!   those never set by hand share what the others leave, and the last
//!   open section always takes what is left. Each keeps a least height
//!   (two rows by default, [`Section::min_height`]) and scrolls its own
//!   body.
//! - The line above a band, between two open sections, is a boundary. It
//!   follows the pointer when dragged, or moves by a row with Up and Down
//!   while it has the keyboard. Sections give way in turn, the nearest
//!   first, never in proportion, and it stops once they are all at their
//!   least (`egui_pop_model::sections`). A double click, or Enter, drops
//!   the heights set by hand.
//! - In a panel too short, sections sized by the list give way first,
//!   then those set by hand, from the bottom up; their stored heights
//!   come back with the room, until the next drag aligns them on what is
//!   shown. When even the least heights do not fit, the whole stack
//!   scrolls, with a scrollbar then only.
//! - What is folded and the heights set by hand are remembered with the
//!   window's layout (egui's persisted memory, under the stack's name),
//!   and readable as a [`Layout`] an application keeps with its own.
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
//! gives each section as drawn on the last pass, with its height and
//! whether that was set by hand (the debug bridge's `state` collector
//! lists it too), so an agent reads the stack rather than pixels.
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

use egui::accesskit::{Action, Role};
use egui::scroll_area::{DragScroll, ScrollBarVisibility, ScrollSource};
use egui::{
    Color32, Context, CursorIcon, EventFilter, FontFamily, FontId, Id, Key, Rect, Response, Sense,
    Ui, UiBuilder, WidgetInfo, WidgetType, pos2, vec2,
};
use egui_pop_model::sections::{self as model, Asked, DragMode, Placed};
use serde::{Deserialize, Serialize};

use crate::indicator;
use crate::kit::{Corners, Glyph, Kit, KitCx, Part, State, Visibility, paint_ring};
use crate::state::Slot;
use crate::strings::{self, SECTION_RESIZE};
use crate::theme::{TextRole, Tokens};
use crate::ui::UiPop;

/// What a stack remembers of one section.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Remembered {
    /// Whether the section is open.
    pub open: bool,
    /// Its height set by hand, in points; none when sized by the list
    /// (a layout stored with the shares of earlier versions reads so).
    /// Kept while folded, so a section opens again at its size, and while
    /// it is the last open one, which takes what the others leave.
    #[serde(default)]
    pub height: Option<f32>,
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
            .or_insert(Remembered { open, height: None })
            .open = open;
    }

    /// A section's height set by hand, if it has one.
    pub fn height(&self, key: &str) -> Option<f32> {
        self.sections.get(key).and_then(|s| s.height)
    }

    /// Sets a section's height by hand, as a drag would, or gives it back
    /// to the list. It opens as the stack first shows it when the layout
    /// does not know it yet.
    pub fn set_height(&mut self, key: &str, height: Option<f32>) {
        match self.sections.get_mut(key) {
            Some(s) => s.height = height,
            None => {
                self.sections
                    .insert(key.to_string(), Remembered { open: true, height });
            }
        }
    }

    /// Drops every height set by hand (a boundary's double click).
    pub fn reset_heights(&mut self) {
        for s in self.sections.values_mut() {
            s.height = None;
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
    /// Its body's height, in points (0 when folded).
    pub height: f32,
    /// Whether that height is its own, set by hand. Never for the last
    /// open section, which takes what the others leave.
    pub by_hand: bool,
    /// Its band.
    #[serde(skip)]
    pub band: Rect,
    /// Its body, when open.
    #[serde(skip)]
    pub body: Option<Rect>,
}

/// Each stack's sections as laid out on the last pass, by name.
const LAID: Slot<Vec<Laid>> = Slot::summarised("core.sections.laid", laid_summary);

/// Each section in a few words, for the debug bridge: all of them fit in
/// a slot's summary, which `Debug` and the rectangles would not.
#[allow(clippy::ptr_arg)] // A slot's summary takes a reference to its value.
fn laid_summary(laid: &Vec<Laid>) -> String {
    let parts: Vec<String> = laid
        .iter()
        .map(|s| match (s.open, s.by_hand) {
            (false, _) => format!("{} folded", s.key),
            (true, false) => format!("{} {:.0}", s.key, s.height),
            (true, true) => format!("{} {:.0} by hand", s.key, s.height),
        })
        .collect();
    parts.join(", ")
}

/// A drag on a boundary: where it began and the stack it began from, so
/// that the border follows the pointer without drifting.
#[derive(Clone, Debug)]
struct Drag {
    origin: f32,
    heights: Vec<f32>,
    hands: Vec<Option<f32>>,
}

/// Each stack's drag under way, by name.
const DRAG: Slot<Drag> = Slot::new("core.sections.drag");

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
    /// A boundary moved, dragged or from the keyboard.
    pub resized: bool,
    /// A boundary was double-clicked, or Enter pressed on it: every
    /// height set by hand dropped.
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
                height: None,
            });
        }
        let asked: Vec<Asked> = sections
            .iter()
            .map(|(s, _)| {
                let remembered = layout.sections[&s.key];
                Asked {
                    open: remembered.open,
                    least: s.min_height.unwrap_or(2.0 * row),
                    hand: remembered.height.filter(|h| h.is_finite()),
                }
            })
            .collect();
        let keys: Vec<String> = sections.iter().map(|(s, _)| s.key.clone()).collect();

        let room = ui.available_rect_before_wrap();
        let finite = room.height().is_finite();
        let bands = sections.len() as f32 * band_h;
        let space = if finite {
            (room.height() - bands).max(0.0)
        } else {
            0.0
        };
        let fit = model::fit(&asked, space);
        let tall = bands + fit.heights.iter().sum::<f32>();
        let overflows = finite && tall - room.height() > 0.5;
        let stack = Shown {
            keys: &keys,
            asked: &asked,
            heights: &fit.heights,
            space,
            step: row,
        };

        let mut response = SectionsResponse::default();
        let mut laid: Vec<Laid> = Vec::with_capacity(sections.len());
        let base = memory_id(&self.name);
        let reach = ui.style().interaction.resize_grab_radius_side;
        let draw = |ui: &mut Ui| {
            let left = ui.max_rect().left();
            let width = ui.available_width();
            let top = ui.cursor().min.y;
            let mut y = top;
            let mut ring = None;
            for (i, (section, body)) in sections.into_iter().enumerate() {
                let is_open = asked[i].open;
                let band = Rect::from_min_size(pos2(left, y), vec2(width, band_h));
                let mut hit = band;
                // A boundary is registered between the section above it
                // and the band under it, so Tab reaches it in reading
                // order; the band leaves it the room it would otherwise
                // win under the pointer.
                if i > 0 && asked[i - 1].open && asked[i..].iter().any(|a| a.open) {
                    let edge = Rect::from_x_y_ranges(band.x_range(), y - reach..=y + reach);
                    hit.min.y = (hit.min.y + reach).min(hit.max.y);
                    let id = base.with(("boundary", &laid[i - 1].key));
                    let r = ui.interact(edge, id, Sense::click_and_drag());
                    let title = &laid[i - 1].title;
                    let name = strings::tr_args(&ctx, &SECTION_RESIZE, &[("section", title)]);
                    name_boundary(&ctx, &r, &name, &stack, i - 1);
                    if overflows && r.gained_focus() {
                        r.scroll_to_me(None);
                    }
                    if r.has_focus() {
                        ring = Some(edge);
                    }
                    work_boundary(
                        ui,
                        &r,
                        &self.name,
                        &stack,
                        i - 1,
                        &mut layout,
                        &mut response,
                    );
                }
                let r = ui.interact(hit, base.with(("band", &section.key)), Sense::click());
                self.paint_band(ui, &kit, &tokens, &section, band, &r, is_open);
                name_band(&ctx, &r, &section.name(), is_open);
                if overflows && r.gained_focus() {
                    r.scroll_to_me(None);
                }
                if r.clicked() || arrow_folds(ui, &r, is_open) {
                    layout.set_open(&section.key, !is_open);
                    response.toggled = Some((section.key.clone(), !is_open));
                }
                y = band.bottom();
                let mut body_rect = None;
                if is_open {
                    let rect = Rect::from_min_size(pos2(left, y), vec2(width, fit.heights[i]));
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
                        .min_scrolled_height(0.0)
                        .show(&mut child, body);
                    y = rect.bottom();
                    body_rect = Some(rect);
                }
                laid.push(Laid {
                    key: section.key,
                    title: section.title,
                    count: section.count,
                    open: is_open,
                    height: fit.heights[i],
                    by_hand: fit.by_hand[i],
                    band,
                    body: body_rect,
                });
            }
            // Painted last: the band under a boundary would cover its ring.
            if let Some(edge) = ring {
                let painter = ui.painter();
                let cx = KitCx {
                    painter,
                    ppp: ui.ctx().pixels_per_point(),
                };
                let focus = kit.section_band.data.states.get(State::FOCUS);
                paint_ring(&cx, edge, focus, Corners::All);
            }
            let end = if finite && !overflows {
                y.min(top + room.height())
            } else {
                y
            };
            ui.advance_cursor_after_rect(Rect::from_min_max(
                pos2(left, top),
                pos2(left + width, end.max(top)),
            ));
        };
        if finite {
            // Always in this area, overflowing or not, so that a body's
            // own scroll keeps its place when the panel crosses the line.
            let area = egui::ScrollArea::vertical()
                .id_salt(("egui_pop.sections.stack", &self.name))
                .auto_shrink([false, false])
                .max_height(room.height())
                .min_scrolled_height(0.0)
                .scroll_source(ScrollSource {
                    drag: DragScroll::Never,
                    ..ScrollSource::default()
                })
                .scroll_bar_visibility(if overflows {
                    ScrollBarVisibility::AlwaysVisible
                } else {
                    ScrollBarVisibility::AlwaysHidden
                });
            // The stack's bar is seen as soon as it overflows, whatever
            // the theme does for the bars of the bodies, which keep theirs.
            let bodies = ui.spacing().scroll;
            ui.scope(|ui| {
                if overflows {
                    let mut bar = kit.scroll;
                    bar.visibility = Visibility::Always;
                    ui.spacing_mut().scroll = bar.style(&bodies);
                }
                ui.scroll_area(area, |ui| {
                    ui.spacing_mut().scroll = bodies;
                    draw(ui);
                });
            });
        } else {
            draw(ui);
        }

        if response.toggled.is_some() || response.resized || response.reset {
            ctx.request_repaint();
        }
        layout.store(&ctx, &self.name);
        LAID.keyed(self.name.clone()).set(&ctx, laid);
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

/// The stack as this pass shows it: what a boundary works from.
struct Shown<'a> {
    keys: &'a [String],
    asked: &'a [Asked],
    heights: &'a [f32],
    /// The height the open sections divide.
    space: f32,
    /// How far an arrow key moves a boundary.
    step: f32,
}

impl Shown<'_> {
    fn placed(&self, heights: &[f32]) -> Vec<Placed> {
        self.asked
            .iter()
            .zip(heights)
            .map(|(a, &height)| Placed {
                open: a.open,
                least: a.least,
                height,
            })
            .collect()
    }

    /// The boundary under `above` moved `delta` from `heights`, what the
    /// layout keeps by hand from `asked` on. The last open section fills
    /// the panel, so a stack has no empty room under it. Says whether
    /// this changes what is shown.
    fn drag(
        &self,
        layout: &mut Layout,
        asked: &[Asked],
        heights: &[f32],
        above: usize,
        delta: f32,
    ) -> bool {
        let plan = model::plan_drag(&self.placed(heights), above, delta, 0.0, DragMode::Push);
        let hands = model::pin_plan(asked, self.space, &plan.heights);
        for (key, hand) in self.keys.iter().zip(hands) {
            layout.set_height(key, hand);
        }
        let moved = |(a, b): (&f32, &f32)| (a - b).abs() > 0.01;
        plan.heights.iter().zip(self.heights).any(moved)
    }

    /// What the boundary under `above` can do from here, up and down.
    fn reach(&self, above: usize) -> (bool, f32) {
        let placed = self.placed(self.heights);
        let plan = |delta| model::plan_drag(&placed, above, delta, 0.0, DragMode::Push);
        (
            plan(-self.step).moved < 0.0,
            plan(self.space).heights[above],
        )
    }
}

/// Names a boundary for assistive technology: a splitter, its value the
/// height of the section above it, with the actions the arrows have.
fn name_boundary(ctx: &Context, r: &Response, name: &str, stack: &Shown<'_>, above: usize) {
    let value = stack.heights[above];
    let (shrinks, most) = stack.reach(above);
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, name));
    ctx.accesskit_node_builder(r.id, |node| {
        node.set_role(Role::Splitter);
        node.set_numeric_value(f64::from(value));
        node.set_min_numeric_value(f64::from(stack.asked[above].least.min(value)));
        node.set_max_numeric_value(f64::from(most));
        node.set_numeric_value_step(f64::from(stack.step));
        if most > value {
            node.add_action(Action::Increment);
        }
        if shrinks {
            node.add_action(Action::Decrement);
        }
    });
}

/// What the user does to the boundary under `above`: the pointer, the
/// arrow keys, Enter and assistive technology's steps.
fn work_boundary(
    ui: &Ui,
    r: &Response,
    name: &str,
    stack: &Shown<'_>,
    above: usize,
    layout: &mut Layout,
    response: &mut SectionsResponse,
) {
    let ctx = ui.ctx();
    if r.hovered() || r.dragged() {
        ctx.set_cursor_icon(CursorIcon::ResizeVertical);
    }
    let slot = DRAG.keyed(name.to_string());
    if r.has_focus() {
        // upstream: egui moves the focus with an arrow key unless the
        // widget says it uses them.
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                r.id,
                EventFilter {
                    vertical_arrows: true,
                    ..Default::default()
                },
            );
        });
    }
    if r.double_clicked() || (r.has_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
        layout.reset_heights();
        response.reset = true;
        slot.remove(ctx);
        return;
    }
    if r.drag_started() {
        let origin = ui.input(|i| i.pointer.press_origin());
        slot.set(
            ctx,
            Drag {
                origin: origin.map_or(r.rect.center().y, |p| p.y),
                heights: stack.heights.to_vec(),
                hands: stack.asked.iter().map(|a| a.hand).collect(),
            },
        );
    }
    if r.dragged() {
        let drag = slot
            .get(ctx)
            .filter(|d| d.heights.len() == stack.keys.len());
        if let (Some(drag), Some(pointer)) = (drag, r.interact_pointer_pos()) {
            let asked: Vec<Asked> = stack
                .asked
                .iter()
                .zip(&drag.hands)
                .map(|(a, &hand)| Asked { hand, ..*a })
                .collect();
            let delta = pointer.y - drag.origin;
            response.resized |= stack.drag(layout, &asked, &drag.heights, above, delta);
        }
    }
    if r.drag_stopped() {
        slot.remove(ctx);
    }
    let focused = r.has_focus();
    let steps = ui.input(|i| {
        let keys = if focused {
            i.num_presses(Key::ArrowDown) as f32 - i.num_presses(Key::ArrowUp) as f32
        } else {
            0.0
        };
        keys + i.num_accesskit_action_requests(r.id, Action::Increment) as f32
            - i.num_accesskit_action_requests(r.id, Action::Decrement) as f32
    });
    if steps != 0.0 {
        let delta = steps * stack.step;
        response.resized |= stack.drag(layout, stack.asked, stack.heights, above, delta);
    }
}

/// Left folds a band with the keyboard and Right opens it, as a code
/// editor's headers do. Says whether the band changes.
fn arrow_folds(ui: &Ui, r: &Response, open: bool) -> bool {
    if !r.has_focus() {
        return false;
    }
    // upstream: as for a boundary, the arrows would move the focus.
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            r.id,
            EventFilter {
                horizontal_arrows: true,
                ..Default::default()
            },
        );
    });
    let key = if open {
        Key::ArrowLeft
    } else {
        Key::ArrowRight
    };
    ui.input(|i| i.key_pressed(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layout_with_the_old_shares_reads_as_sized_by_the_list() {
        let old =
            r#"{"sections":{"live":{"open":false,"share":2.5},"idle":{"open":true,"share":0.5}}}"#;
        let layout: Layout = serde_json::from_str(old).unwrap();
        assert_eq!(layout.is_open("live"), Some(false));
        assert_eq!(layout.is_open("idle"), Some(true));
        assert_eq!(layout.height("live"), None);
        assert_eq!(layout.height("idle"), None);
    }

    #[test]
    fn a_layout_keeps_its_heights_through_a_round_trip_and_drops_them_on_reset() {
        let mut layout = Layout::default();
        layout.set_open("live", false);
        layout.set_height("live", Some(120.0));
        layout.set_height("idle", Some(40.0));
        let text = serde_json::to_string(&layout).unwrap();
        let mut back: Layout = serde_json::from_str(&text).unwrap();
        assert_eq!(back, layout);
        back.reset_heights();
        assert_eq!(back.height("live"), None);
        assert_eq!(back.is_open("live"), Some(false), "the fold stays");
    }

    #[test]
    fn the_debug_summary_names_every_section_height() {
        let laid = |key: &str, open, height, by_hand| Laid {
            key: key.to_string(),
            title: key.to_string(),
            count: None,
            open,
            height,
            by_hand,
            band: Rect::ZERO,
            body: None,
        };
        assert_eq!(
            laid_summary(&vec![
                laid("live", true, 120.0, true),
                laid("idle", true, 80.0, false),
                laid("shut", false, 0.0, false),
            ]),
            "live 120 by hand, idle 80, shut folded"
        );
    }
}
