//! Folds inside a panel: a group whose header row folds and opens its
//! body ([`Fold`]), and a row of a hand-built tree with its disclosure
//! toggle ([`TreeRow`]), for the folds that are neither a whole side bar
//! of stacked sections ([`crate::sections::Sections`]) nor a data view's
//! tree.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! - A fold's header is a section band in the kit's look (its disclosure
//!   mark, its title, an optional count), with optional actions at its
//!   right end. A click on the band, or Space or Enter while it has the
//!   keyboard, folds or opens it; Left folds it and Right opens it.
//! - A tree row is the row the caller draws, after its toggle: a branch's
//!   toggle folds and opens the rows below it (a click, Space or Enter,
//!   Left and Right while it has the keyboard); a leaf's row takes the
//!   same room, so that branches and leaves line up. Rows below a branch
//!   are indented by one level.
//! - What is open is kept by id in egui's persisted memory, with the
//!   window's layout, as egui's own collapsing regions keep it: a fold
//!   has the id an egui `CollapsingHeader` given the same `id_salt` has,
//!   and a tree row the id it is given, so either replaces egui's without
//!   losing what the user opened. [`is_open`] reads it and [`set_open`]
//!   changes it.
//! - **Named and observable.** A fold's header is a button named by its
//!   title and count ("Folders, 3"), a tree row's toggle a disclosure
//!   triangle named after its row ("Show or hide Photos", the strings
//!   provider's `egui-pop-fold-toggle`); both say whether they are
//!   expanded and answer assistive technology's expand and collapse
//!   actions.
//!
//! ```
//! use egui_pop_core::fold::{Fold, TreeRow};
//!
//! let ctx = egui::Context::default();
//! let mut out = ctx.run_ui(Default::default(), |ui| {
//!     Fold::new("folders", "Folders").count(2).default_open(true).show(ui, |ui| {
//!         let id = ui.make_persistent_id("photos");
//!         TreeRow::new(id, "Photos")
//!             .show(ui, |ui| ui.label("Photos"))
//!             .body(ui, |ui| {
//!                 TreeRow::new(ui.make_persistent_id("2024"), "2024")
//!                     .leaf(true)
//!                     .show(ui, |ui| ui.label("2024"));
//!             });
//!     });
//! });
//! out.textures_delta.clear();
//! ```

use egui::collapsing_header::CollapsingState;
use egui::{
    Context, EventFilter, Id, IdSalt, InnerResponse, Key, Response, Sense, Ui, WidgetInfo,
    accesskit,
};
use egui_stack::PartState;

use crate::kit::{Glyph, Kit};
use crate::sections::row_height;
use crate::strings::{self, FOLD_TOGGLE};

/// Whether the fold or the tree row of id `id` is open, if it was ever
/// shown or set.
pub fn is_open(ctx: &Context, id: Id) -> Option<bool> {
    CollapsingState::load(ctx, id).map(|s| s.is_open())
}

/// Opens or folds the fold or the tree row of id `id` (shown so from the
/// next pass), and keeps it so.
pub fn set_open(ctx: &Context, id: Id, open: bool) {
    let mut state = CollapsingState::load_with_default_open(ctx, id, open);
    state.set_open(open);
    state.store(ctx);
    ctx.request_repaint();
}

/// Whether the keyboard or assistive technology asks the open or folded
/// widget `r` to change: Left folds it and Right opens it while it has
/// the keyboard (the arrows then do not move the focus), and an expand or
/// collapse action. Space and Enter come as a click.
fn asked_to_toggle(ui: &Ui, r: &Response, open: bool) -> bool {
    let mut asked = false;
    if r.has_focus() {
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
        asked |= ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
    }
    let action = if open {
        accesskit::Action::Collapse
    } else {
        accesskit::Action::Expand
    };
    asked | ui.input(|i| i.has_accesskit_action_request(r.id, action))
}

/// Says to assistive technology whether `id` is open, and what it can
/// ask of it.
fn expanded_node(ctx: &Context, id: Id, open: bool) {
    ctx.accesskit_node_builder(id, |node| {
        node.set_expanded(open);
        node.add_action(if open {
            accesskit::Action::Collapse
        } else {
            accesskit::Action::Expand
        });
    });
}

/// What a fold did this pass.
#[derive(Debug)]
pub struct FoldResponse<R> {
    /// The fold's id: where its open state is kept ([`is_open`]).
    pub id: Id,
    /// The header band's response.
    pub header: Response,
    /// Whether it is open after the pass.
    pub open: bool,
    /// Whether the user folded or opened it this pass.
    pub toggled: bool,
    /// What the body returned, when it was shown.
    pub body: Option<InnerResponse<R>>,
}

/// What a fold's band shows at its right end.
type Actions<'a> = Box<dyn FnOnce(&mut Ui) + 'a>;

/// A group inside a panel whose header row folds and opens its body: a
/// section band (disclosure mark, title, count) with optional actions at
/// its right end, its body indented below it when open. Its open state
/// is kept by id (see the module).
///
/// Need (layout): a group that folds and opens inside a panel (a disclosure, a collapsing header).
pub struct Fold<'a> {
    salt: IdSalt,
    title: String,
    count: Option<u64>,
    default_open: bool,
    indent: bool,
    actions: Option<Actions<'a>>,
}

impl std::fmt::Debug for Fold<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fold")
            .field("title", &self.title)
            .field("count", &self.count)
            .field("default_open", &self.default_open)
            .field("indent", &self.indent)
            .field("actions", &self.actions.is_some())
            .finish_non_exhaustive()
    }
}

impl<'a> Fold<'a> {
    /// A fold titled `title`, its state kept under `id_salt` (unique in
    /// the parent `Ui`; the id egui's `CollapsingHeader` gives the same
    /// salt). Folded until opened, unless [`Fold::default_open`].
    pub fn new(id_salt: impl egui::AsIdSalt, title: impl Into<String>) -> Self {
        Fold {
            salt: IdSalt::new(id_salt),
            title: title.into(),
            count: None,
            default_open: false,
            indent: true,
            actions: None,
        }
    }

    /// The count shown on the band, and in its name ("Folders, 3").
    pub fn count(mut self, count: u64) -> Self {
        self.count = Some(count);
        self
    }

    /// Whether it is open the first time it is shown (no by default).
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    /// Whether the body is indented under the band (yes by default).
    pub fn indent(mut self, indent: bool) -> Self {
        self.indent = indent;
        self
    }

    /// Actions at the band's right end (a button, a menu), laid out from
    /// right to left; a click on them does not fold the group.
    pub fn actions(mut self, add: impl FnOnce(&mut Ui) + 'a) -> Self {
        self.actions = Some(Box::new(add));
        self
    }

    /// Shows the band, and the body `add` gives when open.
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> FoldResponse<R> {
        // A vertical scope, as egui's CollapsingHeader has: the same ids
        // for the fold and for what its body holds.
        ui.vertical(|ui| self.show_inside(ui, add)).inner
    }

    fn show_inside<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> FoldResponse<R> {
        let id = ui.make_persistent_id(self.salt);
        let ctx = ui.ctx().clone();
        let kit = Kit::of(&ctx);
        let mut state = CollapsingState::load_with_default_open(&ctx, id, self.default_open);
        let height = if kit.section_band.data.height > 0.0 {
            kit.section_band.data.height
        } else {
            row_height(ui)
        };
        let (row, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(height), height),
            Sense::hover(),
        );
        // The actions first, from the right end: the band takes what
        // they leave.
        let mut band = row;
        if let Some(actions) = self.actions {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(id.with("actions"))
                    .max_rect(row)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            actions(&mut child);
            let used = child.min_rect().width();
            if used > 0.0 {
                band.max.x = (row.right() - used - ui.spacing().item_spacing.x).max(row.left());
            }
        }
        let header = ui.interact(band, id, Sense::click());
        let mut open = state.is_open();
        let toggled = header.clicked() || asked_to_toggle(ui, &header, open);
        if toggled {
            state.toggle(ui);
            open = state.is_open();
        }
        let name = match self.count {
            Some(n) => format!("{}, {n}", self.title),
            None => self.title.clone(),
        };
        let enabled = ui.is_enabled();
        header.widget_info(|| WidgetInfo::labeled(egui::Role::Button, enabled, &name));
        expanded_node(&ctx, id, open);
        if ui.is_rect_visible(band) {
            let part = PartState::NONE
                .with(PartState::HOVERED, header.hovered())
                .with(PartState::PRESSED, header.is_pointer_button_down_on())
                .with(PartState::FOCUSED, header.has_focus());
            kit.paint_section_band(
                ui.painter(),
                &crate::kit::dock::Band {
                    rect: band,
                    id,
                    title: &self.title,
                    count: self.count,
                    open,
                    state: part,
                },
            );
        }
        let body = if self.indent {
            state.show_body_indented(&header, ui, add)
        } else {
            state.show_body_unindented(ui, add)
        };
        FoldResponse {
            id,
            header,
            open,
            toggled,
            body,
        }
    }
}

/// A row of a hand-built tree: a branch's toggle, or a leaf's room for
/// one, then the row the caller draws; [`TreeRowResponse::body`] shows
/// the rows below a branch, indented, when it is open. Its open state is
/// kept under the id it is given (see the module).
///
/// Need (data): a row of a hand-built tree with its disclosure toggle (a folder tree in a side panel).
#[derive(Clone, Debug)]
pub struct TreeRow {
    id: Id,
    name: String,
    toggle_name: Option<String>,
    leaf: bool,
    default_open: bool,
}

/// A shown tree row: its toggle, its row, and the rows below it.
#[derive(Debug)]
pub struct TreeRowResponse<R> {
    /// The row's id: where its open state is kept ([`is_open`]).
    pub id: Id,
    /// The toggle's response, a branch's only.
    pub toggle: Option<Response>,
    /// What the row returned, and the response of the whole row.
    pub row: InnerResponse<R>,
    /// Whether it is open after the pass (a leaf never is).
    pub open: bool,
    /// Whether the user folded or opened it this pass.
    pub toggled: bool,
    state: Option<CollapsingState>,
}

impl TreeRow {
    /// A branch kept under `id` (`ui.make_persistent_id(..)`, the id
    /// egui's `CollapsingState` would be given), its toggle named after
    /// `name` (its row's text, as read aloud). Folded until opened,
    /// unless [`TreeRow::default_open`].
    pub fn new(id: Id, name: impl Into<String>) -> Self {
        TreeRow {
            id,
            name: name.into(),
            toggle_name: None,
            leaf: false,
            default_open: false,
        }
    }

    /// The toggle's whole accessible name, in place of "Show or hide"
    /// and the row's name ("Show or hide the albums of Travel").
    pub fn toggle_name(mut self, name: impl Into<String>) -> Self {
        self.toggle_name = Some(name.into());
        self
    }

    /// A leaf: no toggle, its room kept so that rows line up.
    pub fn leaf(mut self, leaf: bool) -> Self {
        self.leaf = leaf;
        self
    }

    /// Whether a branch is open the first time it is shown (no by
    /// default).
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    /// Shows the toggle and the row `add` gives, in one horizontal line.
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> TreeRowResponse<R> {
        let ctx = ui.ctx().clone();
        let mut state = (!self.leaf)
            .then(|| CollapsingState::load_with_default_open(&ctx, self.id, self.default_open));
        let mut toggled = false;
        let mut toggle = None;
        let row = ui.horizontal(|ui| {
            let spacing = ui.spacing().item_spacing;
            // The toggle takes the whole indent, as egui's does.
            ui.spacing_mut().item_spacing.x = 0.0;
            let size = egui::vec2(ui.spacing().indent, ui.spacing().icon_width);
            match state.as_mut() {
                None => {
                    ui.allocate_space(size);
                }
                Some(state) => {
                    let (_, rect) = ui.allocate_space(size);
                    let r = ui.interact(rect, self.id, Sense::click());
                    if r.clicked() || asked_to_toggle(ui, &r, state.is_open()) {
                        state.toggle(ui);
                        toggled = true;
                    }
                    let open = state.is_open();
                    let name = self.toggle_name.clone().unwrap_or_else(|| {
                        strings::tr_args(&ctx, &FOLD_TOGGLE, &[("item", &self.name)])
                    });
                    let enabled = ui.is_enabled();
                    r.widget_info(|| {
                        WidgetInfo::labeled(egui::Role::DisclosureTriangle, enabled, &name)
                    });
                    expanded_node(&ctx, self.id, open);
                    if ui.is_rect_visible(rect) {
                        paint_toggle(ui, &r, open);
                    }
                    toggle = Some(r);
                }
            }
            ui.spacing_mut().item_spacing = spacing;
            add(ui)
        });
        let open = state.as_ref().is_some_and(CollapsingState::is_open);
        TreeRowResponse {
            id: self.id,
            toggle,
            row,
            open,
            toggled,
            state,
        }
    }
}

impl<R> TreeRowResponse<R> {
    /// Shows the rows below a branch, indented, when it is open (nothing
    /// for a leaf), and keeps its state.
    pub fn body<B>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> B) -> Option<InnerResponse<B>> {
        let mut state = self.state?;
        state.show_body_indented(&self.row.response, ui, add)
    }

    /// Keeps a branch's state without showing rows below it (the caller
    /// draws them itself when [`TreeRowResponse::open`]).
    pub fn keep(self, ctx: &Context) {
        if let Some(state) = self.state {
            state.store(ctx);
        }
    }
}

/// A toggle's mark: the kit's disclosure glyph, in the colour of the
/// toggle's state, its focus shown by the kit's ring.
fn paint_toggle(ui: &Ui, r: &Response, open: bool) {
    let kit = Kit::of(ui.ctx());
    let glyph = if open {
        Glyph::DisclosureOpen
    } else {
        Glyph::DisclosureClosed
    };
    let (mut icon, _) = ui.spacing().icon_rectangles(r.rect);
    icon.set_center(r.rect.center());
    let color = ui.style().interact(r).fg_stroke.color;
    kit.glyph(glyph).paint(ui.painter(), icon, color);
    if r.has_focus() {
        crate::ring::own(ui.ctx(), r.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass(ctx: &Context, input: egui::RawInput, add: impl FnMut(&mut Ui)) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(300.0, 400.0),
            )),
            ..input
        };
        let mut out = ctx.run_ui(input, add);
        out.textures_delta.clear();
    }

    #[test]
    fn a_fold_has_the_id_egui_s_collapsing_header_has() {
        let ctx = Context::default();
        let mut fold = None;
        let mut header = None;
        pass(&ctx, Default::default(), |ui| {
            ui.push_id("a", |ui| {
                fold = Some(
                    Fold::new("helios.library.folders", "Folders")
                        .show(ui, |_| ())
                        .id,
                );
            });
            ui.push_id("a", |ui| {
                header = Some(
                    egui::CollapsingHeader::new("Folders")
                        .id_salt("helios.library.folders")
                        .show(ui, |_| ())
                        .header_response
                        .id,
                );
            });
        });
        assert_eq!(fold, header);
    }

    #[test]
    fn a_fold_keeps_what_set_open_says() {
        fn show(ctx: &Context) -> (Id, bool) {
            let mut seen = (Id::NULL, false);
            pass(ctx, Default::default(), |ui| {
                let r = Fold::new("f", "F").show(ui, |_| ());
                seen = (r.id, r.body.is_some());
            });
            seen
        }
        let ctx = Context::default();
        let (id, shown) = show(&ctx);
        assert!(!shown, "folded by default");
        assert_eq!(is_open(&ctx, id), Some(false));
        set_open(&ctx, id, true);
        assert!(show(&ctx).1);
        assert_eq!(is_open(&ctx, id), Some(true));
    }

    #[test]
    fn a_leaf_row_has_no_toggle_and_is_never_open() {
        let ctx = Context::default();
        pass(&ctx, Default::default(), |ui| {
            let r = TreeRow::new(Id::unique("leaf"), "Leaf")
                .leaf(true)
                .default_open(true)
                .show(ui, |ui| ui.label("Leaf"));
            assert!(r.toggle.is_none());
            assert!(!r.open);
            assert!(r.body(ui, |_| ()).is_none());
        });
    }
}
