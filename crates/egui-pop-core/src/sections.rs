//! Collapsible sections stacked in a panel: each a title band (a
//! disclosure mark, a title, a count) over its own scrolling body, as a
//! code editor's side bar stacks its views.
//!
//! The stack is egui_stack's ([`egui_stack::sections`]), which holds the
//! layout arithmetic ([`plan`]), the drag, the fold, the keyboard, the
//! accessibility, the windowed rows and the wheel. egui-pop gives it its
//! look, its words and its gestures:
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
//!   least ([`plan`]). A double click, or Enter, drops the heights set by
//!   hand and any empty room.
//! - In a panel too short, sections sized by the list give way first,
//!   then those set by hand, from the bottom up; their stored heights
//!   come back with the room, until the next drag aligns them on what is
//!   shown. When even the least heights do not fit, the whole stack
//!   scrolls, with a scrollbar then only (the kit's, seen at rest).
//! - [`Sections::grab_titles`] (off by default) makes the titles drag too:
//!   a press that moves resizes as the line above does, one that does not
//!   folds, and a double click drops the heights set by hand.
//! - The keys of `gesture.sections.pull` (Ctrl with a drag by default, Cmd
//!   on macOS, read from the pointer map) move the sections below as a
//!   block, heights kept: only the section above changes, and empty room
//!   opens under the last. The empty room is kept, with the layout, until
//!   a drag takes it back or a double click drops it.
//! - [`Sections::keep_titles`] (off by default) leaves a title where it is
//!   when its section folds or opens ([`plan::keep_title`]).
//! - A body is scrolled by the stack ([`Stack::section`]), windowed over
//!   rows of one height, so that only those in view are drawn
//!   ([`Stack::rows`]), or left to an application's own view
//!   ([`Stack::bare`]). A wheel pushing past the end of a body's scroll
//!   stops there: neither the stack nor a parent moves.
//! - What is folded, the heights set by hand and the empty room are
//!   remembered with the window's layout (egui's persisted memory, under
//!   the stack's name, egui_stack's key), and readable as a [`Layout`] an
//!   application keeps with its own. A layout stored by egui-pop 0.4 and
//!   earlier, under egui-pop's own key, moves to egui_stack's the first
//!   time the stack is shown or its layout loaded.
//!
//! The kit draws the band (`section-band`, handler `section`), its mark
//! (`section-disclosure`, the `disclosure-open` and `disclosure-closed`
//! glyph roles) and its count (`section-count`, based on
//! `indicator-badge`), as the kit's [`egui_stack::DockPainter`].
//!
//! **Named and observable.** A band is a button named by its title and
//! count ("Live, 3"), expanded or not for assistive technology; a
//! boundary is a splitter named after the section above it ("Resize
//! Live", the strings provider's `egui-pop-section-resize`), its value
//! the height of that section's body. [`laid_out`] gives each section as
//! drawn on the last pass, with its height and whether that was set by
//! hand (the debug bridge's `state` collector lists it too), so an agent
//! reads the stack rather than pixels.
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

use egui::{Context, Id, PointerButton, Ui};
use egui_pop_model::keymap::Pointer;
use egui_pop_model::pointer::{PointerMap, SCOPE_SECTIONS, gesture};
use serde::{Deserialize, Serialize};

pub use egui_stack::sections::plan::{self, DragMode};
pub use egui_stack::sections::{Laid, Remembered, Section, Stack, laid_out, row_height};

use crate::kit::Kit;
use crate::pointer;
use crate::state::Slot;
use crate::strings::{self, SECTION_RESIZE};

/// What a stack remembers, by section key: kept in egui's persisted
/// memory under the stack's name, and readable and writable whole
/// ([`Layout::load`], [`Layout::store`]) for an application that keeps
/// it with its own layout. egui_stack's own
/// ([`egui_stack::sections::Layout`]) converts to and from it.
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
        move_old_layout(ctx, name);
        egui_stack::sections::Layout::load(ctx, name).map(Layout::from)
    }

    /// Makes it the layout of the stack named `name` (shown so from the
    /// next pass).
    pub fn store(&self, ctx: &Context, name: &str) {
        // A layout given whole wins over one an older egui-pop left.
        ctx.data_mut(|d| d.remove::<Layout>(old_memory_id(name)));
        egui_stack::sections::Layout::from(self.clone()).store(ctx, name);
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

impl From<egui_stack::sections::Layout> for Layout {
    fn from(layout: egui_stack::sections::Layout) -> Self {
        Layout {
            sections: layout.sections,
            empty: layout.empty,
        }
    }
}

impl From<Layout> for egui_stack::sections::Layout {
    fn from(layout: Layout) -> Self {
        egui_stack::sections::Layout {
            sections: layout.sections,
            empty: layout.empty,
        }
    }
}

/// Where egui-pop 0.4 and earlier kept a stack's layout, as this crate's
/// [`Layout`] (egui's persisted memory goes by the value's type too).
fn old_memory_id(name: &str) -> Id {
    Id::unique(("egui_pop.sections", name))
}

/// The stacks whose old layout was looked for, by name: once a run.
const MOVED: Slot<()> = Slot::new("core.sections.moved");

/// Moves the layout an older egui-pop stored for the stack named `name`
/// to egui_stack's key, unless egui_stack already has one; looked for
/// once a run.
fn move_old_layout(ctx: &Context, name: &str) {
    let moved = MOVED.keyed(name.to_string());
    if moved.get(ctx).is_some() {
        return;
    }
    moved.set(ctx, ());
    let id = old_memory_id(name);
    let Some(old) = ctx.data_mut(|d| d.get_persisted::<Layout>(id)) else {
        return;
    };
    ctx.data_mut(|d| d.remove::<Layout>(id));
    if egui_stack::sections::Layout::load(ctx, name).is_none() {
        egui_stack::sections::Layout::from(old).store(ctx, name);
    }
}

/// Each stack's sections as laid out on the last pass, by name: for the
/// debug bridge, egui_stack's [`laid_out`] read once a pass.
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
        let ctx = ui.ctx().clone();
        move_old_layout(&ctx, &self.name);
        let kit = Kit::of(&ctx);
        let map = pointer::map(&ctx);
        let mut labels = egui_stack::Labels::ENGLISH;
        labels.resize_section =
            strings::tr_args(&ctx, &SECTION_RESIZE, &[("section", "{section}")]).into();
        // The stack's bar is seen as soon as it overflows, quiet, whatever
        // the theme does for the bars of the bodies, which keep theirs.
        let bodies = ui.spacing().scroll;
        let mut stack = egui_stack::sections::Sections::new(self.name.clone())
            .painter(&*kit)
            .labels(labels)
            .capitals(self.capitals)
            .grab_titles(self.grab_titles)
            .keep_titles(self.keep_titles)
            .pull_when(move |modifiers, button| pulls(&map, modifiers, button))
            .overflow_scroll(kit.scroll.seen_at_rest().style(&bodies));
        if kit.section_band.data.height > 0.0 {
            stack = stack.band_height(kit.section_band.data.height);
        }
        let response = stack.show(ui, add);
        LAID.keyed(self.name.clone())
            .set(&ctx, laid_out(&ctx, &self.name));
        SectionsResponse {
            toggled: response.toggled,
            resized: response.resized,
            reset: response.reset,
            layout: response.layout.into(),
        }
    }
}

/// Whether these keys and this button make a drag pull, in the map in force.
fn pulls(map: &PointerMap, modifiers: egui::Modifiers, button: PointerButton) -> bool {
    let drag = Pointer::Drag(pointer::button(button));
    map.command(SCOPE_SECTIONS, &pointer::chord(modifiers, drag)) == Some(&gesture::SECTIONS_PULL)
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

    fn pass(ctx: &Context, add: impl FnMut(&mut Ui)) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(300.0, 400.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, add);
        out.textures_delta.clear();
    }

    /// Three sections, `shut` folded, `log` windowed over 1000 rows.
    fn three(ui: &mut Ui) {
        Sections::new("stack").show(ui, |stack| {
            stack.section(Section::new("live", "Live"), |ui| {
                ui.label("build");
            });
            stack.rows(Section::new("log", "Log"), 1000, |ui, range| {
                for i in range {
                    ui.label(format!("Line {i}"));
                }
            });
            stack.section(Section::new("shut", "Shut").open(false), |ui| {
                ui.label("hidden");
            });
        });
    }

    #[test]
    fn the_debug_summary_names_every_section_height() {
        let ctx = Context::default();
        let mut layout = Layout::default();
        layout.set_height("live", Some(120.0));
        layout.set_open("shut", false);
        layout.store(&ctx, "stack");
        pass(&ctx, three);
        let laid = laid_out(&ctx, "stack");
        let log = laid[1].height;
        let window = laid[1].window.clone().expect("windowed");
        assert_eq!(
            laid_summary(&laid),
            format!(
                "live 120 by hand, log {log:.0} rows {}..{}, shut folded",
                window.start, window.end
            )
        );
        // The empty room under the last section, after a pull.
        let mut layout = Layout::default();
        layout.set_height("live", Some(120.0));
        layout.set_height("log", Some(80.0));
        layout.set_open("shut", false);
        layout.empty = 60.0;
        layout.store(&ctx, "stack");
        pass(&ctx, three);
        let text = laid_summary(&laid_out(&ctx, "stack"));
        assert!(text.ends_with(", 60 empty"), "{text}");
        // A drag under way names how it resizes.
        let mut laid = laid_out(&ctx, "stack");
        let mut pulled = laid.remove(0);
        pulled.drag = Some(DragMode::Pull);
        assert_eq!(laid_summary(&vec![pulled]), "live 120 by hand pulled");
    }

    #[test]
    fn a_layout_stored_under_the_old_key_moves_to_egui_stack_s_on_first_load() {
        let ctx = Context::default();
        let mut old = Layout::default();
        old.set_height("live", Some(150.0));
        old.set_open("shut", false);
        ctx.data_mut(|d| d.insert_persisted(old_memory_id("stack"), old.clone()));
        assert_eq!(egui_stack::sections::Layout::load(&ctx, "stack"), None);

        pass(&ctx, three);
        let laid = laid_out(&ctx, "stack");
        assert_eq!(laid[0].height, 150.0);
        assert!(laid[0].by_hand);
        assert!(!laid[2].open);
        let moved = egui_stack::sections::Layout::load(&ctx, "stack").expect("moved");
        assert_eq!(moved.height("live"), Some(150.0));
        assert_eq!(moved.is_open("shut"), Some(false));
        assert_eq!(
            ctx.data_mut(|d| d.get_persisted::<Layout>(old_memory_id("stack"))),
            None,
            "the old key is dropped"
        );
    }

    #[test]
    fn a_layout_already_under_egui_stack_s_key_wins_over_the_old_one() {
        let ctx = Context::default();
        let mut old = Layout::default();
        old.set_height("live", Some(150.0));
        ctx.data_mut(|d| d.insert_persisted(old_memory_id("stack"), old));
        let mut new = egui_stack::sections::Layout::default();
        new.set_height("live", Some(90.0));
        new.store(&ctx, "stack");

        assert_eq!(
            Layout::load(&ctx, "stack").and_then(|l| l.height("live")),
            Some(90.0)
        );
        assert_eq!(
            ctx.data_mut(|d| d.get_persisted::<Layout>(old_memory_id("stack"))),
            None
        );
    }
}
