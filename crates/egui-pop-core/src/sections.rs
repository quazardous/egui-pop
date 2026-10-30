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
//! - The line above a band, between two open sections, is a boundary (the
//!   first open section above is the one it resizes, past any folded
//!   ones). It follows the pointer when dragged, or moves by a row with
//!   Up and Down while it has the keyboard. Sections give way in turn, the nearest
//!   first, never in proportion, and it stops once they are all at their
//!   least (`egui_pop_model::sections`). A double click, or Enter, drops
//!   the heights set by hand and any empty room.
//! - In a panel too short, sections sized by the list give way first,
//!   then those set by hand, from the bottom up; their stored heights
//!   come back with the room, until the next drag aligns them on what is
//!   shown. When even the least heights do not fit, the whole stack
//!   scrolls, with a scrollbar then only.
//! - [`Sections::grab_titles`] (off by default) makes the titles drag too:
//!   a press that moves resizes as the line above does, one that does not
//!   folds, and a double click drops the heights set by hand.
//! - The keys of `gesture.sections.pull` (Ctrl with a drag by default, Cmd
//!   on macOS, read from the pointer map) move the sections below as a
//!   block, heights kept: only the section above changes, and empty room
//!   opens under the last. The empty room is kept, with the layout, until
//!   a drag takes it back or a double click drops it.
//! - [`Sections::keep_titles`] (off by default) leaves a title where it is
//!   when its section folds or opens ([`egui_pop_model::sections::keep_title`]).
//! - A body is scrolled by the stack ([`Stack::section`]), windowed over
//!   rows of one height, so that only those in view are drawn
//!   ([`Stack::rows`]), or left to an application's own view
//!   ([`Stack::bare`]). A wheel pushing past the end of a body's scroll
//!   stops there: neither the stack nor a parent moves.
//! - What is folded, the heights set by hand and the empty room are
//!   remembered with the
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
use std::ops::Range;

use egui::accesskit::{Action, Role};
use egui::scroll_area::{DragScroll, ScrollBarVisibility, ScrollSource};
use egui::{
    Color32, Context, CursorIcon, EventFilter, FontFamily, FontId, Id, Key, PointerButton, Rect,
    Response, Sense, TextStyle, Ui, UiBuilder, WidgetInfo, WidgetType, pos2, vec2,
};
use egui_pop_model::keymap::Pointer;
use egui_pop_model::pointer::{PointerMap, SCOPE_SECTIONS, gesture};
use egui_pop_model::sections::{self as model, Asked, DragMode, Placed};
use serde::{Deserialize, Serialize};

use crate::indicator;
use crate::kit::{Corners, Glyph, Kit, KitCx, Part, State, Visibility, paint_ring};
use crate::pointer;
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
    /// The empty room kept under the last section, in points.
    #[serde(default)]
    pub empty: f32,
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

    /// Drops every height set by hand and the empty room (a boundary's
    /// double click): the sections share the panel again.
    pub fn reset_heights(&mut self) {
        for s in self.sections.values_mut() {
            s.height = None;
        }
        self.empty = 0.0;
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
    /// How far its body is scrolled, in points (0 when folded, and for a
    /// bare body, whose scroll is the application's).
    pub scroll: f32,
    /// The rows drawn on that pass, when windowed and open.
    pub window: Option<Range<usize>>,
    /// The empty room under it, in points: only the last section has any
    /// (all of it, when no section is open).
    pub empty: f32,
    /// How a drag under way resizes it, if one does.
    pub drag: Option<DragMode>,
}

/// Each stack's sections as laid out on the last pass, by name.
const LAID: Slot<Vec<Laid>> = Slot::summarised("core.sections.laid", laid_summary);

/// Each section in a few words, for the debug bridge: all of them fit in
/// a slot's summary, which `Debug` and the rectangles would not.
#[allow(clippy::ptr_arg)] // A slot's summary takes a reference to its value.
fn laid_summary(laid: &Vec<Laid>) -> String {
    let parts: Vec<String> = laid
        .iter()
        .map(|s| {
            let window = s
                .window
                .as_ref()
                .map_or(String::new(), |w| format!(" rows {}..{}", w.start, w.end));
            let part = match (s.open, s.by_hand) {
                (false, _) => format!("{} folded", s.key),
                (true, false) => format!("{} {:.0}{window}", s.key, s.height),
                (true, true) => format!("{} {:.0} by hand{window}", s.key, s.height),
            };
            match s.drag {
                Some(DragMode::Pull) => format!("{part} pulled"),
                Some(_) => format!("{part} dragged"),
                None => part,
            }
        })
        .collect();
    let mut text = parts.join(", ");
    let empty: f32 = laid.iter().map(|s| s.empty).sum();
    if empty > 0.5 {
        text.push_str(&format!(", {empty:.0} empty"));
    }
    text
}

/// The stack as a drag found it, what its plans start from.
#[derive(Clone, Debug)]
struct Start {
    heights: Vec<f32>,
    hands: Vec<Option<f32>>,
    empty: f32,
}

impl Start {
    /// What the stack asks for, with the heights kept by hand as they were.
    fn asked(&self, stack: &Shown<'_>) -> Vec<Asked> {
        stack
            .asked
            .iter()
            .zip(&self.hands)
            .map(|(a, &hand)| Asked { hand, ..*a })
            .collect()
    }
}

/// A drag on a boundary: where it began and the stack it began from, so
/// that the border follows the pointer without drifting.
#[derive(Clone, Debug)]
struct Drag {
    origin: f32,
    from: Start,
    /// How its plans move the stack.
    mode: DragMode,
    /// Where the pointer was on the last plan: where a change of mode
    /// starts again from.
    last: f32,
}

/// Each stack's drag under way, by name.
const DRAG: Slot<Drag> = Slot::new("core.sections.drag");

/// A click on a title, for as long as a second one would be a double click.
#[derive(Clone, Debug)]
struct TitleClick {
    key: String,
    /// The band it landed on, where the second click will land.
    band: Rect,
    /// When, in egui's time.
    at: f64,
}

/// Each stack's last click on a title, by name.
const TITLE_CLICK: Slot<TitleClick> = Slot::new("core.sections.title_click");

/// The sections of the stack named `name` as drawn on the last pass, in
/// order (empty when it was not drawn).
pub fn laid_out(ctx: &Context, name: &str) -> Vec<Laid> {
    LAID.keyed(name.to_string()).get(ctx).unwrap_or_default()
}

/// The height of a row when the application names none
/// ([`Section::row_height`]): the theme's interactive height, or the body
/// text's when that is taller.
pub fn row_height(ui: &Ui) -> f32 {
    ui.spacing()
        .interact_size
        .y
        .max(ui.text_style_height(&TextStyle::Body))
}

/// One section: a key (stable, what the layout remembers it by), a
/// title, an optional count, its least height, its rows' height and
/// whether it starts open.
#[derive(Clone, Debug)]
pub struct Section {
    key: String,
    title: String,
    count: Option<u64>,
    min_height: Option<f32>,
    row_height: Option<f32>,
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
            row_height: None,
            open: true,
        }
    }

    /// The count on its band (`99+` past 99; its name says the real
    /// count).
    pub fn count(mut self, count: u64) -> Self {
        self.count = Some(count);
        self
    }

    /// Its body's least height, in points. By default, room for two rows
    /// (or the line saying it is empty); for a windowed section
    /// ([`Stack::rows`]), two rows, or all of them when it has fewer.
    pub fn min_height(mut self, points: f32) -> Self {
        self.min_height = Some(points.max(0.0));
        self
    }

    /// The height of its rows, in points, at least one (by default the
    /// theme's, [`row_height`]): what its least height counts, and what
    /// [`Stack::rows`] windows by.
    pub fn row_height(mut self, points: f32) -> Self {
        self.row_height = Some(points.max(1.0));
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

/// Draws the rows in a window, handed their range.
type DrawRows<'a> = Box<dyn FnOnce(&mut Ui, Range<usize>) + 'a>;

/// What a section draws under its band.
enum Body<'a> {
    /// Anything, in a scroll area.
    Scrolled(Box<dyn FnOnce(&mut Ui) + 'a>),
    /// `count` rows of one height, in a scroll area, drawn by the window.
    Rows { count: usize, draw: DrawRows<'a> },
    /// An application's own view, which scrolls itself.
    Bare(Box<dyn FnOnce(&mut Ui) + 'a>),
}

/// What drawing a body says.
struct Drawn {
    /// How far it is scrolled.
    scroll: f32,
    /// The rows drawn.
    window: Option<Range<usize>>,
    /// Whether the wheel over it stops there: it has a scroll of its own,
    /// with somewhere to go.
    holds_wheel: bool,
}

impl Body<'_> {
    /// Its least height, unless the section names one.
    fn least(&self, ui: &Ui, row: f32) -> f32 {
        match self {
            Body::Rows { count, .. } => {
                model::rows_least(row, *count) + ui.spacing().scroll.content_margin.sum().y
            }
            Body::Scrolled(_) | Body::Bare(_) => 2.0 * row,
        }
    }

    /// Draws it in `ui`, as tall as the section. `salt` tells its scroll
    /// from the others'; `row` is its rows' height.
    fn draw(self, ui: &mut Ui, salt: &str, row: f32) -> Drawn {
        let height = ui.max_rect().height();
        let area = egui::ScrollArea::vertical()
            .id_salt(("egui_pop.sections.scroll", salt))
            .auto_shrink([false, false])
            .max_height(height)
            .min_scrolled_height(0.0);
        match self {
            Body::Scrolled(draw) => {
                let out = area.show(ui, draw);
                Drawn {
                    scroll: out.state.offset.y,
                    window: None,
                    holds_wheel: scrolled(ui.ctx(), &out),
                }
            }
            Body::Rows { count, draw } => {
                // Rows touch, as a list's: their height is all that places them.
                ui.spacing_mut().item_spacing.y = 0.0;
                let mut window = None;
                let out = area.show_rows(ui, row, count, |ui, range| {
                    window = Some(range.clone());
                    draw(ui, range);
                });
                Drawn {
                    scroll: out.state.offset.y,
                    window,
                    holds_wheel: scrolled(ui.ctx(), &out),
                }
            }
            Body::Bare(draw) => {
                draw(ui);
                Drawn {
                    scroll: 0.0,
                    window: None,
                    holds_wheel: true,
                }
            }
        }
    }
}

/// After a body's scroll area: says whether it has somewhere to scroll,
/// and asks for another pass when its offset changed.
///
/// upstream: a scroll area clamps its offset after drawing and asks for no
/// repaint, so a body grown, or left with fewer rows, would show blank rows
/// under its last one until something else repaints (docs/upstream.md).
fn scrolled<R>(ctx: &Context, out: &egui::scroll_area::ScrollAreaOutput<R>) -> bool {
    let id = out.id.with("egui_pop.sections.offset");
    let offset = out.state.offset.y;
    let before = ctx.data_mut(|d| d.get_temp::<f32>(id));
    if before != Some(offset) {
        ctx.data_mut(|d| d.insert_temp(id, offset));
        if before.is_some() {
            ctx.request_repaint();
        }
    }
    out.content_size.y - out.inner_rect.height() > 0.5
}

/// Keeps the wheel over a body from reaching what scrolls around it.
///
/// upstream: a scroll area at its end leaves the wheel to its parents, and
/// nothing asks it to keep it (docs/upstream.md). Taken as egui's own scroll
/// area takes it, and only by the axis it would take.
fn contain_wheel(ui: &Ui, body: Rect) {
    if !ui.is_enabled() || !ui.rect_contains_pointer(body) {
        return;
    }
    let both = ui.style().always_scroll_the_only_direction;
    ui.input_mut(|i| {
        i.smooth_scroll_delta.y = 0.0;
        if both {
            i.smooth_scroll_delta.x = 0.0;
        }
    });
}

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
        self.sections
            .push((section, Body::Scrolled(Box::new(body))));
        self
    }

    /// Adds a section of `count` rows of one height ([`Section::row_height`])
    /// in a scroll area of its own, windowed: `draw` is called only while
    /// it is open, with the rows in view and the one past them, on every
    /// pass, so the window follows the section's size and scroll.
    ///
    /// It draws them from the top of its `ui`, which has no spacing between
    /// items, each exactly that tall (with [`Ui::add_sized`], say), or they
    /// drift from their place in the scroll.
    /// Its least height is two rows, or all of them when it has fewer.
    ///
    /// ```
    /// use egui_pop_core::sections::{Section, Sections, laid_out, row_height};
    ///
    /// let ctx = egui::Context::default();
    /// let mut out = ctx.run_ui(Default::default(), |ui| {
    ///     let height = row_height(ui);
    ///     Sections::new("log").show(ui, |stack| {
    ///         stack.rows(Section::new("lines", "Lines"), 100_000, move |ui, range| {
    ///             for i in range {
    ///                 let width = ui.available_width();
    ///                 ui.add_sized([width, height], egui::Label::new(format!("Line {i}")));
    ///             }
    ///         });
    ///     });
    /// });
    /// out.textures_delta.clear();
    /// let window = laid_out(&ctx, "log")[0].window.clone().unwrap();
    /// assert!(window.len() < 1_000, "{window:?}");
    /// ```
    pub fn rows(
        &mut self,
        section: Section,
        count: usize,
        draw: impl FnOnce(&mut Ui, Range<usize>) + 'a,
    ) -> &mut Self {
        let draw = Box::new(draw);
        self.sections.push((section, Body::Rows { count, draw }));
        self
    }

    /// Adds a section whose `body` has no scroll area around it (called
    /// only while it is open): for an application's own view that scrolls
    /// itself, such as a virtualised list, which would otherwise nest two
    /// scrolls. It fills the body, clipped to it.
    ///
    /// The stack cannot see into that view, so the wheel over a bare body
    /// always stops there, whether or not the view has anywhere to go.
    pub fn bare(&mut self, section: Section, body: impl FnOnce(&mut Ui) + 'a) -> &mut Self {
        self.sections.push((section, Body::Bare(Box::new(body))));
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
    grab_titles: bool,
    keep_titles: bool,
}

impl Sections {
    /// A stack named `name`, unique in the application: its layout is
    /// remembered under it and [`laid_out`] reads it.
    pub fn new(name: impl Into<String>) -> Self {
        Sections {
            name: name.into(),
            capitals: true,
            grab_titles: false,
            keep_titles: false,
        }
    }

    /// Whether titles are drawn in capitals (yes by default, as a code
    /// editor's side bar does); their accessible names keep their case.
    pub fn capitals(mut self, capitals: bool) -> Self {
        self.capitals = capitals;
        self
    }

    /// Whether a title drags as well as folds (no by default, as the line
    /// above a band is what classic apps resize by, and a code editor's
    /// headers reorder when dragged).
    ///
    /// A press that moves is a drag, by the same rules as a boundary's, the
    /// title staying under the pointer; one that does not is a fold. A title
    /// under a folded section resizes the first open one above, and with
    /// nothing open above it does not move. A double click drops the heights
    /// set by hand; as its first click has already folded, the section is
    /// seen folded until the second. A band keeps its name and its role.
    pub fn grab_titles(mut self, grab: bool) -> Self {
        self.grab_titles = grab;
        self
    }

    /// Whether a title stays where it is when its section folds or opens
    /// (no by default, as a code editor's panes divide the room again, and
    /// the title moves).
    ///
    /// Folding, the open sections above keep their shown height, as if set
    /// by hand; those below move up, and with nothing below the room stays
    /// empty. Opening, the section takes its room below its title, from the
    /// sections below, then from the empty room. In a panel with no room for
    /// it, those above still give way, and the title moves. A double click
    /// gets the shares back.
    pub fn keep_titles(mut self, keep: bool) -> Self {
        self.keep_titles = keep;
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
        let row = row_height(ui);
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
            .map(|(s, body)| {
                let remembered = layout.sections[&s.key];
                Asked {
                    open: remembered.open,
                    least: s
                        .min_height
                        .unwrap_or_else(|| body.least(ui, s.row_height.unwrap_or(row))),
                    hand: remembered.height.filter(|h| h.is_finite()),
                }
            })
            .collect();
        let keys: Vec<String> = sections.iter().map(|(s, _)| s.key.clone()).collect();
        let open: Vec<bool> = asked.iter().map(|a| a.open).collect();

        let room = ui.available_rect_before_wrap();
        let finite = room.height().is_finite();
        let bands = sections.len() as f32 * band_h;
        let space = if finite {
            (room.height() - bands).max(0.0)
        } else {
            0.0
        };
        let fit = model::fit(&asked, space, layout.empty);
        let tall = bands + fit.heights.iter().sum::<f32>();
        let overflows = finite && tall - room.height() > 0.5;
        let stack = Shown {
            keys: &keys,
            asked: &asked,
            heights: &fit.heights,
            space,
            empty: fit.empty,
            step: row,
        };

        let mut response = SectionsResponse::default();
        let mut laid: Vec<Laid> = Vec::with_capacity(sections.len());
        let base = memory_id(&self.name);
        let reach = ui.style().interaction.resize_grab_radius_side;
        let window = ctx.options(|o| o.input_options.max_double_click_delay);
        let (now, released) =
            ui.input(|i| (i.time, i.pointer.any_down() || i.pointer.any_released()));
        let title_click = TITLE_CLICK.keyed(self.name.clone());
        let last_click = title_click.get(&ctx);
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
                // win under the pointer. Under a folded section it resizes
                // the first open one above, as the line above that folded
                // section does, which alone takes the focus: one stop per
                // pair of open sections.
                let above = i
                    .checked_sub(1)
                    .and_then(|line| model::resized_above(&open, line));
                let room_below = fit.empty > 0.5 || asked[i..].iter().any(|a| a.open);
                // A title that drags keeps the whole band, or a press on its
                // top would drag but never fold: the line is on the section's
                // side alone, and under a folded section, whose title does
                // the same, there is none.
                let line = above
                    .filter(|_| room_below)
                    .filter(|&above| !self.grab_titles || above + 1 == i);
                if let Some(above) = line {
                    let edge = if self.grab_titles {
                        Rect::from_x_y_ranges(band.x_range(), y - reach..=y)
                    } else {
                        hit.min.y = (hit.min.y + reach).min(hit.max.y);
                        Rect::from_x_y_ranges(band.x_range(), y - reach..=y + reach)
                    };
                    let id = base.with(("boundary", &laid[i - 1].key));
                    let sense = if above == i - 1 {
                        Sense::click_and_drag()
                    } else {
                        Sense::CLICK | Sense::DRAG
                    };
                    let r = ui.interact(edge, id, sense);
                    let title = &laid[above].title;
                    let name = strings::tr_args(&ctx, &SECTION_RESIZE, &[("section", title)]);
                    name_boundary(&ctx, &r, &name, &stack, above);
                    if overflows && r.gained_focus() {
                        r.scroll_to_me(None);
                    }
                    if r.has_focus() {
                        ring = Some(edge);
                    }
                    laid[above].drag = work_boundary(
                        ui,
                        &r,
                        &self.name,
                        &stack,
                        above,
                        &mut layout,
                        &mut response,
                    )
                    .or(laid[above].drag);
                }
                let grabbed = above.filter(|_| self.grab_titles);
                let sense = if grabbed.is_some() {
                    Sense::click_and_drag()
                } else {
                    Sense::click()
                };
                let r = ui.interact(hit, base.with(("band", &section.key)), sense);
                if let Some(above) = grabbed {
                    laid[above].drag = work_drag(
                        ui,
                        &r,
                        &self.name,
                        &stack,
                        above,
                        &mut layout,
                        &mut response,
                    )
                    .or(laid[above].drag);
                }
                self.paint_band(ui, &kit, &tokens, &section, band, &r, is_open);
                name_band(&ctx, &r, &section.name(), is_open);
                if overflows && r.gained_focus() {
                    r.scroll_to_me(None);
                }
                if self.grab_titles && r.double_clicked() {
                    // Its first click folded: the second puts that back. Both
                    // in one pass, they cancel out.
                    if last_click.as_ref().is_some_and(|c| c.key == section.key) {
                        stack.fold(&mut layout, &mut response, i, false);
                    }
                    self.reset(&ctx, &mut layout, &mut response);
                } else if r.clicked() || arrow_folds(ui, &r, is_open) {
                    stack.fold(&mut layout, &mut response, i, self.keep_titles);
                    if self.grab_titles && r.clicked() {
                        let at = now;
                        let key = section.key.clone();
                        title_click.set(&ctx, TitleClick { key, band, at });
                        ctx.request_repaint_after_secs(window as f32);
                    }
                }
                y = band.bottom();
                let mut body_rect = None;
                let mut drawn = None;
                if is_open {
                    let rect = Rect::from_min_size(pos2(left, y), vec2(width, fit.heights[i]));
                    let mut child = ui.new_child(
                        UiBuilder::new()
                            .id_salt(("egui_pop.sections.body", &self.name, &section.key))
                            .max_rect(rect)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    child.set_clip_rect(rect.intersect(ui.clip_rect()));
                    let shown =
                        body.draw(&mut child, &section.key, section.row_height.unwrap_or(row));
                    if shown.holds_wheel {
                        contain_wheel(&child, rect);
                    }
                    drawn = Some(shown);
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
                    scroll: drawn.as_ref().map_or(0.0, |d| d.scroll),
                    window: drawn.and_then(|d| d.window),
                    empty: if i + 1 == asked.len() { fit.empty } else { 0.0 },
                    drag: None,
                });
            }
            // The fold may have taken the title from under the pointer: what
            // moved there must not take the second click of a double click,
            // nor the title lose it. Drawn last, so it has the pointer.
            let moved = last_click
                .as_ref()
                .filter(|c| now - c.at <= window || released)
                .and_then(|c| {
                    let at = stack.keys.iter().position(|k| *k == c.key)?;
                    ((laid[at].band.top() - c.band.top()).abs() > 0.5).then_some((at, c.band))
                });
            if let Some((at, band)) = moved {
                let r = ui.interact(band, base.with("title_click"), Sense::click());
                if r.double_clicked() {
                    stack.fold(&mut layout, &mut response, at, false);
                    self.reset(&ctx, &mut layout, &mut response);
                } else if r.clicked() {
                    stack.fold(&mut layout, &mut response, at, self.keep_titles);
                }
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

    /// Drops the heights set by hand and the empty room.
    fn reset(&self, ctx: &Context, layout: &mut Layout, response: &mut SectionsResponse) {
        layout.reset_heights();
        response.reset = true;
        DRAG.keyed(self.name.clone()).remove(ctx);
        TITLE_CLICK.keyed(self.name.clone()).remove(ctx);
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
    /// The empty room shown under the last section.
    empty: f32,
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

    /// The boundary under `above` moved `delta` from `from`, in `mode`.
    /// Says whether this changes what is shown.
    fn drag(
        &self,
        layout: &mut Layout,
        from: &Start,
        above: usize,
        delta: f32,
        mode: DragMode,
    ) -> bool {
        let placed = self.placed(&from.heights);
        let plan = model::plan_drag(&placed, above, delta, from.empty, mode);
        let hands = model::pin_plan(&from.asked(self), self.space, plan.empty, &plan.heights);
        for (key, hand) in self.keys.iter().zip(hands) {
            layout.set_height(key, hand);
        }
        layout.empty = plan.empty;
        let moved = |(a, b): (&f32, &f32)| (a - b).abs() > 0.01;
        (plan.empty - self.empty).abs() > 0.01 || plan.heights.iter().zip(self.heights).any(moved)
    }

    /// Where a drag or a key press starts from: the stack as shown.
    fn start(&self) -> Start {
        Start {
            heights: self.heights.to_vec(),
            hands: self.asked.iter().map(|a| a.hand).collect(),
            empty: self.empty,
        }
    }

    /// Folds or opens `section`, its title staying where it is if `keep`.
    fn fold(
        &self,
        layout: &mut Layout,
        response: &mut SectionsResponse,
        section: usize,
        keep: bool,
    ) {
        if keep {
            let kept = model::keep_title(self.asked, self.heights, self.empty, section);
            for (key, hand) in self.keys.iter().zip(kept.hands) {
                layout.set_height(key, hand);
            }
            layout.empty = kept.empty;
        }
        let (key, open) = (&self.keys[section], !self.asked[section].open);
        layout.set_open(key, open);
        response.toggled = Some((key.clone(), open));
    }

    /// What the boundary under `above` can do from here, up and down.
    fn reach(&self, above: usize) -> (bool, f32) {
        let placed = self.placed(self.heights);
        let plan = |delta| model::plan_drag(&placed, above, delta, self.empty, DragMode::Push);
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

/// Whether these keys and this button make a drag pull, in the map in force.
fn pulls(map: &PointerMap, modifiers: egui::Modifiers, button: PointerButton) -> bool {
    let drag = Pointer::Drag(pointer::button(button));
    map.command(SCOPE_SECTIONS, &pointer::chord(modifiers, drag)) == Some(&gesture::SECTIONS_PULL)
}

/// How a drag under way moves the stack, by the keys held now and the
/// button that holds it.
fn drag_mode(ui: &Ui) -> DragMode {
    let map = pointer::map(ui.ctx());
    let pull = ui.input(|i| {
        [
            PointerButton::Primary,
            PointerButton::Secondary,
            PointerButton::Middle,
            PointerButton::Extra1,
            PointerButton::Extra2,
        ]
        .into_iter()
        .any(|b| i.pointer.button_down(b) && pulls(&map, i.modifiers, b))
    });
    if pull { DragMode::Pull } else { DragMode::Push }
}

/// How the arrows on a boundary move the stack: as a primary drag with the
/// same keys held would.
fn keys_mode(ui: &Ui) -> DragMode {
    let map = pointer::map(ui.ctx());
    if ui.input(|i| pulls(&map, i.modifiers, PointerButton::Primary)) {
        DragMode::Pull
    } else {
        DragMode::Push
    }
}

/// What a pointer dragging `r` does to the section `above`, a boundary or a
/// title alike. Says how it moves the stack, while it does.
///
/// The keys pressed or released during the drag change its mode from there:
/// it starts again from the stack as shown, where the pointer last was, so
/// nothing jumps.
fn work_drag(
    ui: &Ui,
    r: &Response,
    name: &str,
    stack: &Shown<'_>,
    above: usize,
    layout: &mut Layout,
    response: &mut SectionsResponse,
) -> Option<DragMode> {
    let ctx = ui.ctx();
    let slot = DRAG.keyed(name.to_string());
    if r.drag_started() {
        let origin = ui
            .input(|i| i.pointer.press_origin())
            .map_or(r.rect.center().y, |p| p.y);
        slot.set(
            ctx,
            Drag {
                origin,
                from: stack.start(),
                mode: drag_mode(ui),
                last: origin,
            },
        );
    }
    let mut mode = None;
    if r.dragged() {
        ctx.set_cursor_icon(CursorIcon::ResizeVertical);
        let drag = slot
            .get(ctx)
            .filter(|d| d.from.heights.len() == stack.keys.len());
        if let (Some(mut drag), Some(pointer)) = (drag, r.interact_pointer_pos()) {
            let now = drag_mode(ui);
            if now != drag.mode {
                drag.origin = drag.last;
                drag.from = stack.start();
                drag.mode = now;
            }
            drag.last = pointer.y;
            let delta = pointer.y - drag.origin;
            response.resized |= stack.drag(layout, &drag.from, above, delta, drag.mode);
            mode = Some(drag.mode);
            slot.set(ctx, drag);
        }
    }
    if r.drag_stopped() {
        slot.remove(ctx);
    }
    mode
}

/// What the user does to the boundary under `above`: the pointer, the
/// arrow keys, Enter and assistive technology's steps. Says how a drag
/// moves the stack, while one does.
fn work_boundary(
    ui: &Ui,
    r: &Response,
    name: &str,
    stack: &Shown<'_>,
    above: usize,
    layout: &mut Layout,
    response: &mut SectionsResponse,
) -> Option<DragMode> {
    let ctx = ui.ctx();
    if r.hovered() {
        ctx.set_cursor_icon(CursorIcon::ResizeVertical);
    }
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
        DRAG.keyed(name.to_string()).remove(ctx);
        return None;
    }
    let dragging = work_drag(ui, r, name, stack, above, layout, response);
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
        let mode = keys_mode(ui);
        response.resized |= stack.drag(layout, &stack.start(), above, delta, mode);
    }
    dragging
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
            scroll: 0.0,
            window: None,
            empty: 0.0,
            drag: None,
        };
        assert_eq!(
            laid_summary(&vec![
                laid("live", true, 120.0, true),
                laid("idle", true, 80.0, false),
                laid("shut", false, 0.0, false),
            ]),
            "live 120 by hand, idle 80, shut folded"
        );
        let windowed = Laid {
            window: Some(40..52),
            ..laid("log", true, 200.0, false)
        };
        assert_eq!(laid_summary(&vec![windowed]), "log 200 rows 40..52");
        // The empty room, and how a drag under way resizes a section.
        let pulled = Laid {
            drag: Some(DragMode::Pull),
            ..laid("live", true, 120.0, true)
        };
        let last = Laid {
            empty: 60.0,
            ..laid("idle", true, 80.0, false)
        };
        assert_eq!(
            laid_summary(&vec![pulled, last]),
            "live 120 by hand pulled, idle 80, 60 empty"
        );
    }
}
