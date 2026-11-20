//! Layout helpers named by role, for an application's own layout code:
//! text by role, spacing by intent, a card and a button in a tone,
//! drawn from the theme's tokens and kit.
//!
//! ```
//! use egui_pop_core::theme::{self, ThemeSpec};
//! use egui_pop_core::ui::{Space, Tone, UiPop};
//!
//! let ctx = egui::Context::default();
//! theme::install(&ctx, &ThemeSpec::new("pop-dark"));
//! let mut out = ctx.run_ui(Default::default(), |ui| {
//!     ui.section_title("Export");
//!     ui.muted("Writes a copy next to the original.");
//!     ui.space_role(Space::Group);
//!     ui.card(Tone::Neutral, |ui| {
//!         ui.label("IMG_0001.raw");
//!         ui.meta("24 MB, 12 October");
//!     });
//!     if ui.button_tone(Tone::Danger, "Delete").clicked() {
//!         // ...
//!     }
//! });
//! out.textures_delta.clear();
//! ```
//!
//! The helpers are few on purpose, so `Ui`'s completion list stays
//! short; [`role_text`] and [`Space::points`] serve the rest.

use std::ops::Range;

use egui::scroll_area::ScrollAreaOutput;
use egui::{
    ComboBox, InnerResponse, Layout, Rect, Response, RichText, ScrollArea, Sense, Shadow, Ui,
    UiBuilder, Vec2, Widget, WidgetText,
};

use crate::fonts::{self, Role, Weight};
use crate::kit::{Kit, State};
pub use crate::kit::{Stock, Tone};
pub use crate::theme::TextRole;
use crate::theme::Tokens;

pub mod layout;
pub use layout::{Align, Cells, Grid, Length, Row, Track};

/// Spacing by intent (`space.inline`, `group`, `section`, `page`): what
/// the space is for, each an alias of a step of the spacing scale, so
/// densities scale it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Space {
    /// Between items on one line.
    Inline,
    /// Between related groups of items.
    Group,
    /// Before a section of a panel or a page.
    Section,
    /// Around a page's content and between its main blocks.
    Page,
}

impl Space {
    pub const ALL: [Space; 4] = [Space::Inline, Space::Group, Space::Section, Space::Page];

    /// Its token (`space.group`).
    pub fn token(self) -> &'static str {
        match self {
            Space::Inline => "space.inline",
            Space::Group => "space.group",
            Space::Section => "space.section",
            Space::Page => "space.page",
        }
    }

    /// Its size in points in a theme.
    pub fn points(self, tokens: &Tokens) -> f32 {
        let s = &tokens.space;
        match self {
            Space::Inline => s.inline,
            Space::Group => s.group,
            Space::Section => s.section,
            Space::Page => s.page,
        }
    }
}

/// `text` in a role's size, colour and weight (`text-role.*`), for a
/// widget that takes a [`RichText`]. A weight other than regular is drawn
/// with the theme's interface font at that weight once the theme has
/// installed it (`fonts::family`), at the regular weight before.
pub fn role_text(ui: &Ui, role: TextRole, text: impl Into<String>) -> RichText {
    let t = Tokens::of(ui.ctx());
    styled_text(ui, t.text_role.get(role), text)
}

/// `text` as inline emphasis (`text-role.strong`): the body's size, a
/// heavier weight and the strong text colour, for a word or a value that
/// must stand out in running text. Not a heading: a title over a group
/// is [`UiPop::section_title`].
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn strong_text(ui: &Ui, text: impl Into<String>) -> RichText {
    let t = Tokens::of(ui.ctx());
    styled_text(ui, &t.text_role.strong, text)
}

fn styled_text(ui: &Ui, style: &crate::theme::TextRoleStyle, text: impl Into<String>) -> RichText {
    let rich = RichText::new(text)
        .size(style.size.max(4.0))
        .color(style.color);
    match role_family(ui.ctx(), style.weight) {
        Some(family) => rich.family(family),
        None => rich,
    }
}

/// The colour of text in `tone` ([`UiPop::text_tone`]): `danger`,
/// `warning`, `success` and `info` the status colours (`status.*`),
/// `accent` `accent.default`, `muted` `text-role.muted.color`, `record`
/// `status.danger`, `neutral` `text-role.body.color`.
///
/// <div class="warning">Experimental: may change in any release
/// (docs/stability.md).</div>
pub fn tone_color(tokens: &Tokens, tone: Tone) -> egui::Color32 {
    match tone {
        Tone::Neutral => tokens.text_role.body.color,
        Tone::Accent => tokens.accent.default,
        Tone::Info => tokens.status.info,
        Tone::Success => tokens.status.success,
        Tone::Warning => tokens.status.warning,
        Tone::Danger | Tone::Record => tokens.status.danger,
        Tone::Muted => tokens.text_role.muted.color,
    }
}

/// The family drawing a weight of interface text: `None` for the
/// regular weight (egui's own family, or the preview's text style in
/// `fonts::with_preview`), and while the theme's weighted families are
/// not installed. In `fonts::with_preview`, the preview family.
pub(crate) fn role_family(ctx: &egui::Context, weight: Weight) -> Option<egui::FontFamily> {
    if weight.nearest_installed() == Weight::REGULAR {
        return None;
    }
    if fonts::previewing(ctx) {
        return fonts::preview_family(ctx, Role::Ui, weight);
    }
    let family = fonts::family(Role::Ui, weight);
    // Asking for a family egui has no fonts for panics: the theme's
    // fonts are in use from the pass after they are installed.
    ctx.fonts(|f| f.definitions().families.contains_key(&family))
        .then_some(family)
}

/// Keeps the wheel over a scroll area just drawn from what scrolls around it, for
/// a view that shows its own `ScrollArea` (`show_viewport`, `show_rows`);
/// [`UiPop::scroll_area_contained`] does it for the others. Call it right after
/// the area, in the `Ui` it was shown in.
///
/// Only on the ways the area can go: with nothing to scroll one way, the wheel
/// goes on to the area around it that way, so a short list does not stop a panel
/// scrolling under the pointer. It reads those ways from the bars, as the keys do:
/// an area with its bars hidden holds nothing.
///
/// upstream: a scroll area at its end leaves the wheel to its parents, and nothing
/// asks it to keep it (docs/upstream.md).
pub fn contain_wheel<R>(ui: &Ui, out: &ScrollAreaOutput<R>) {
    let ways = crate::scroll_keys::axes(ui.ctx(), out);
    // A bar beside the content lies outside the area's inner rectangle, and egui
    // takes the wheel over it.
    let over = ui.ctx().viewport(|v| {
        [0_usize, 1_usize]
            .iter()
            .filter(|d| ways[**d])
            .filter_map(|d| v.this_pass.widgets.get(out.id.with(*d)))
            .fold(out.inner_rect, |rect, bar| rect.union(bar.rect))
    });
    contain_wheel_in(ui, over, ways);
}

/// [`contain_wheel`] for what scrolls in a way a [`ScrollAreaOutput`] does not
/// tell: over `rect`, the wheel stops on the ways of `ways` (`[x, y]`) that are
/// set. Taken as egui's own scroll area takes it, and only by the axis it would
/// take.
pub fn contain_wheel_in(ui: &Ui, rect: Rect, ways: [bool; 2]) {
    if !ways[0] && !ways[1] || !ui.is_enabled() || !ui.rect_contains_pointer(rect) {
        return;
    }
    let either = ui.style().always_scroll_the_only_direction && ways[0] != ways[1];
    ui.input_mut(|i| {
        if ways[0] || either {
            i.smooth_scroll_delta.x = 0.0;
        }
        if ways[1] || either {
            i.smooth_scroll_delta.y = 0.0;
        }
    });
}

/// Moves the scroll area of id `id` (its [`ScrollAreaOutput::id`]) to
/// `offset`, in points from its start, as from the next time it is drawn;
/// `false` when no such area has been drawn. Kinetic scrolling in
/// progress stops there.
///
/// upstream: egui 0.37 made the scroll state's offset private and has no
/// setter, so this goes through the state's serde form (docs/upstream.md).
pub fn set_scroll_offset(ctx: &egui::Context, id: egui::Id, offset: egui::Vec2) -> bool {
    use egui::scroll_area::State;
    let Some(state) = State::load(ctx, id) else {
        return false;
    };
    let Ok(mut value) = serde_json::to_value(state) else {
        return false;
    };
    let (Some(fields), Ok(offset)) = (value.as_object_mut(), serde_json::to_value(offset)) else {
        return false;
    };
    fields.insert("offset".to_owned(), offset);
    let Ok(state) = serde_json::from_value::<State>(value) else {
        return false;
    };
    state.store(ctx, id);
    true
}

/// The inherent methods of egui's [`Ui`] (egui 0.37), which an extension
/// trait on `Ui` must not reuse: an inherent method always wins, so a
/// [`UiPop`] method of the same name would never be called by
/// `ui.name(..)`. A test checks the trait's names against it; refresh it
/// on each egui bump (every `pub fn` in egui's `impl Ui` blocks).
#[cfg(test)]
const EGUI_UI_METHODS: &[&str] = &[
    "add",
    "add_enabled",
    "add_enabled_ui",
    "add_sized",
    "add_space",
    "add_visible",
    "advance_cursor_after_rect",
    "allocate_at_least",
    "allocate_exact_size",
    "allocate_painter",
    "allocate_rect",
    "allocate_response",
    "allocate_space",
    "allocate_ui",
    "allocate_ui_with_layout",
    "auto_id_with",
    "available_height",
    "available_rect_before_wrap",
    "available_size",
    "available_size_before_wrap",
    "available_width",
    "button",
    "centered_and_justified",
    "checkbox",
    "clip_rect",
    "close",
    "close_kind",
    "code",
    "code_editor",
    "collapsing",
    "color_edit_button_hsva",
    "color_edit_button_rgb",
    "color_edit_button_rgba_premultiplied",
    "color_edit_button_rgba_unmultiplied",
    "color_edit_button_srgb",
    "color_edit_button_srgba",
    "color_edit_button_srgba_premultiplied",
    "color_edit_button_srgba_unmultiplied",
    "colored_label",
    "columns",
    "columns_const",
    "ctx",
    "cursor",
    "debug_paint_cursor",
    "disable",
    "dnd_drag_source",
    "dnd_drop_zone",
    "drag_angle",
    "drag_angle_tau",
    "end_row",
    "expand_to_include_rect",
    "expand_to_include_x",
    "expand_to_include_y",
    "force_set_min_rect",
    "group",
    "heading",
    "horizontal",
    "horizontal_centered",
    "horizontal_top",
    "horizontal_wrapped",
    "hyperlink",
    "hyperlink_to",
    "id",
    "image",
    "indent",
    "interact",
    "interact_opt",
    "is_enabled",
    "is_grid",
    "is_rect_visible",
    "is_sizing_pass",
    "is_tooltip",
    "is_visible",
    "label",
    "label_inputs_by",
    "layer_id",
    "layout",
    "link",
    "make_persistent_id",
    "max_rect",
    "menu_button",
    "menu_image_button",
    "menu_image_text_button",
    "min_rect",
    "min_size",
    "monospace",
    "multiply_opacity",
    "name_inputs",
    "new",
    "new_child",
    "next_auto_id",
    "next_widget_position",
    "opacity",
    "painter",
    "painter_at",
    "pixels_per_point",
    "place",
    "placer",
    "push_id",
    "put",
    "radio",
    "radio_value",
    "rect_contains_pointer",
    "reset_style",
    "response",
    "save_grid",
    "scope",
    "scope_builder",
    "scope_dyn",
    "scope_id",
    "scroll_to_cursor",
    "scroll_to_cursor_animation",
    "scroll_to_rect",
    "scroll_to_rect_animation",
    "scroll_with_delta",
    "scroll_with_delta_animation",
    "selectable_label",
    "selectable_value",
    "separator",
    "set_clip_rect",
    "set_cursor",
    "set_grid",
    "set_height",
    "set_height_range",
    "set_invisible",
    "set_max_height",
    "set_max_size",
    "set_max_width",
    "set_min_height",
    "set_min_size",
    "set_min_width",
    "set_opacity",
    "set_row_height",
    "set_style",
    "set_width",
    "set_width_range",
    "should_close",
    "shrink_clip_rect",
    "shrink_height_to_current",
    "shrink_width_to_current",
    "skip_ahead_auto_ids",
    "small",
    "small_button",
    "spacing",
    "spacing_mut",
    "spinner",
    "stack",
    "strong",
    "style",
    "style_mut",
    "take_available_height",
    "take_available_space",
    "take_available_width",
    "text_edit_multiline",
    "text_edit_singleline",
    "text_style_height",
    "text_valign",
    "toggle_value",
    "ui_contains_pointer",
    "unique_id",
    "vertical",
    "vertical_centered",
    "vertical_centered_justified",
    "visuals",
    "visuals_mut",
    "weak",
    "widget_style",
    "will_parent_close",
    "with_layout",
    "with_visual_transform",
    "wrap_mode",
];

/// [`UiPop`] names kept only as deprecated forwarders, shadowed on purpose.
#[cfg(test)]
const SHADOWED_DEPRECATED: &[&str] = &["strong"];

mod sealed {
    pub trait Sealed {}
    impl Sealed for egui::Ui {}
}

/// Layout helpers named by role, on egui's [`Ui`]. Each is a few lines
/// over the theme's tokens and kit; see the [module](self).
pub trait UiPop: sealed::Sealed {
    /// A label in a text role ([`role_text`]).
    ///
    /// Need (text): a label in a named text role (title, section, body, muted, meta).
    fn text_role(&mut self, role: TextRole, text: impl Into<String>) -> Response;

    /// A section's title (`text-role.section`).
    ///
    /// Need (text): a section title.
    /// Need (text): a heading over a group of fields.
    fn section_title(&mut self, text: impl Into<String>) -> Response;

    /// Secondary text: a hint, a description (`text-role.muted`).
    ///
    /// Need (text): secondary text, a hint or a description.
    /// Need (text): greyed or weak text.
    fn muted(&mut self, text: impl Into<String>) -> Response;

    /// Small print about an item: a date, a size (`text-role.meta`).
    ///
    /// Need (text): small print about an item, a date or a size.
    fn meta(&mut self, text: impl Into<String>) -> Response;

    /// Inline emphasis: a word or a value that must stand out in running
    /// text, at the body's size in a heavier weight (`text-role.strong`,
    /// [`strong_text`]). egui's own `strong` only brightens the colour.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    ///
    /// Need (text): strong or bold text inline, a word that must stand out.
    fn strong_text(&mut self, text: impl Into<String>) -> Response;

    /// The former name of [`UiPop::strong_text`]. egui's `Ui` has an
    /// inherent `strong`, and an inherent method wins over a trait one:
    /// `ui.strong(..)` draws egui's label, never this one. Call
    /// [`UiPop::strong_text`]; ban `egui::Ui::strong` in Clippy's
    /// `disallowed-methods` (docs/first-app.md).
    #[deprecated(note = "egui's inherent `Ui::strong` shadows it: use `UiPop::strong_text`")]
    fn strong(&mut self, text: impl Into<String>) -> Response {
        self.strong_text(text)
    }

    /// Monospace text: a log's or a console's line, a path, a piece of
    /// code. egui's monospace text style (`font.text-style.monospace`,
    /// in `font.family.mono`) in the body role's colour
    /// (`text-role.body.color`), without the box egui's own `code` label
    /// draws, so a log's lines read as text.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    ///
    /// Need (text): monospace text, a log or console line, a path or code.
    fn code_text(&mut self, text: impl Into<String>) -> Response;

    /// Text in a tone: a failed state, an error, a warning, a success.
    /// The body role's size and weight in the tone's colour, from the
    /// tokens ([`tone_color`]): `danger`, `warning`, `success` and `info`
    /// the status colours (legible as text on panels), `accent` the
    /// accent, `muted` the muted role's colour, `record` the danger
    /// colour, [`Tone::Neutral`] the body's. The tone is in its colour
    /// only: say it in the words too ("Failed: ..."), as colour alone
    /// reaches neither every eye nor assistive technologies.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    ///
    /// Need (text): text in a tone, a failed state or an error in the danger colour.
    /// Need (status): an inline error, warning or success message.
    fn text_tone(&mut self, tone: Tone, text: impl Into<String>) -> Response;

    /// A divider: a rule across the layout between two sections, in
    /// `border.default`, one point wide on whole physical pixels, centred in a space of
    /// [`Space::Group`]. Horizontal in a vertical layout, vertical (the
    /// row's height) in a horizontal one. Decoration: it has no name and
    /// takes no input.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    ///
    /// Need (layout): a divider, a rule between two sections (separator).
    fn divider(&mut self) -> Response;

    /// `widget` in a box of `size` at the layout's cursor, filling it
    /// (centred and justified along the layout's direction): a row the
    /// width of its room, a field of a set width. egui's `add_sized`.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    ///
    /// Need (layout): a widget of a fixed size, filling its box (add_sized).
    fn sized(&mut self, size: Vec2, widget: impl Widget) -> Response;

    /// `add` in a box of exactly `size` at the layout's cursor, laid out
    /// by `layout` (`Layout::centered_and_justified` to fill it): a cell
    /// of a set width in a row. The box takes `size` whatever `add`
    /// draws. egui's `allocate_ui_with_layout`, whose box shrinks to its
    /// content.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    ///
    /// Need (layout): content in a box of a fixed size (allocate_ui_with_layout).
    fn sized_ui<R>(
        &mut self,
        size: Vec2,
        layout: Layout,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R>;

    /// `add` in `rect`, laid out by `layout` and clipped to it, for what
    /// places its parts by rectangles (a card laid out by a layout
    /// engine, a slot): nothing it draws goes past `rect`. egui's `put`,
    /// for any content.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    ///
    /// Need (layout): content placed in a given rectangle, clipped to it (put).
    fn rect_ui<R>(
        &mut self,
        rect: Rect,
        layout: Layout,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R>;

    /// Space by intent along the layout's direction ([`Space`]); egui's
    /// own `add_space` takes points.
    ///
    /// Need (layout): space between blocks, by intent (never a number of points).
    fn space_role(&mut self, space: Space);

    /// `add` inside the kit's `card` in a tone ([`Tone::Neutral`] for
    /// the plain card).
    ///
    /// Need (layout): a card, a framed group.
    fn card<R>(&mut self, tone: Tone, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R>;

    /// `add` on the kit's popover surface (its `popover` component, with
    /// egui's popup shadow): the look of egui-pop's own popovers, for an
    /// area the application places itself (a first-run notice, a hint by
    /// a control), outside the overlay stack. The area keeps its own kind
    /// and order; this draws only its surface.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    ///
    /// Need (dialog): a popover's surface in an area the application places itself.
    fn popover<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R>;

    /// egui's button in a tone of the kit's `button` (`button.tone.*`):
    /// a [`Stock::Button`] scope ([`UiPop::scoped`]), so the button keeps
    /// its layout, icon and shortcut text. [`Tone::Neutral`] is the plain
    /// button.
    ///
    /// Need (action): a button by tone (primary, danger, quiet).
    fn button_tone(&mut self, tone: Tone, text: impl Into<WidgetText>) -> Response;

    /// `add` in a scope where egui's own widgets of a [`Stock`] kind
    /// (buttons, combo boxes, text fields, buttons shown on or off) are
    /// drawn from the kit's component, in a tone: its states become
    /// egui's widget visuals, so egui keeps drawing the widget
    /// ([`Kit::scope`]). Visuals compiled with the kit: the scope only
    /// copies them.
    ///
    /// Need (layout): one of egui's own widgets, styled by the kit.
    fn scoped<R>(
        &mut self,
        stock: Stock,
        tone: Tone,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R>;

    /// Shows egui's combo box (`widgets::combo_box`, for the kit's
    /// arrow) with its box drawn from the kit's `combo-box`
    /// ([`Stock::ComboBox`]), `add` its list's content:
    /// `ComboBox::show_ui` in a scope.
    ///
    /// Need (input): a drop-down choice (combo box).
    fn combo_box<R>(
        &mut self,
        combo: ComboBox,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>>;

    /// Shows egui's scroll area with its bars drawn from the kit's
    /// `scroll-bar` ([`Stock::ScrollBar`]: the thumb's fills and corners,
    /// the track), `add` its content, drawn in this `Ui`'s own style.
    /// Every scroll area already takes the bar's width, margin,
    /// placement and visibility from the theme (egui's `ScrollStyle`);
    /// this scope adds the colours, which egui shares with buttons.
    ///
    /// It also answers Page Up, Page Down, Home, End and the arrows, under the
    /// pointer or holding the focus, unless a binding or a widget inside takes
    /// them, and takes the focus itself when it overflows with nothing focusable
    /// inside. A widget reading those keys without sensing drags says so with
    /// [`focus::takes_keys`](crate::focus::takes_keys).
    ///
    /// Need (layout): a scrolling region.
    fn scroll_area<R>(
        &mut self,
        area: ScrollArea,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> ScrollAreaOutput<R>;

    /// [`UiPop::scroll_area`] for a view drawing only what is in view: egui's
    /// `ScrollArea::show_viewport`, `add` given the rectangle in view, in the
    /// content's own coordinates.
    ///
    /// A view whose focusable widget lies around the area rather than in it (a
    /// list, a grid) says it reads the keys with
    /// [`focus::takes_keys`](crate::focus::takes_keys), inside `add`, so that the
    /// area does not become a Tab stop of its own.
    fn scroll_area_viewport<R>(
        &mut self,
        area: ScrollArea,
        add: impl FnOnce(&mut Ui, Rect) -> R,
    ) -> ScrollAreaOutput<R>;

    /// [`UiPop::scroll_area`] for `rows` rows `row_height` high, the `Ui`'s item
    /// spacing apart: egui's `ScrollArea::show_rows`, `add` given the rows in
    /// view.
    ///
    /// Need (data): many rows of equal height, only the visible ones drawn.
    fn scroll_area_rows<R>(
        &mut self,
        area: ScrollArea,
        row_height: f32,
        rows: usize,
        add: impl FnOnce(&mut Ui, Range<usize>) -> R,
    ) -> ScrollAreaOutput<R>;

    /// [`UiPop::scroll_area`] keeping the wheel at its ends ([`contain_wheel`]),
    /// for what nests in something else that scrolls: a list in a panel, a
    /// preview in a pane. A plain body keeps [`UiPop::scroll_area`], as a sizeable
    /// text in a panel should not trap the wheel. A popup or a dialog needs
    /// neither: it is a layer of its own, and what scrolls under it does not see
    /// the wheel.
    fn scroll_area_contained<R>(
        &mut self,
        area: ScrollArea,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> ScrollAreaOutput<R>;

    /// Shows the cursor of a drag of `kind` for `response`: its hover
    /// cursor while hovered when dragging is its only action, its drag
    /// cursor for the whole drag, even once the pointer has left the
    /// widget, from the kit and the pointer profile in force; the
    /// widget may change them before it is dropped
    /// ([`pointer::drag_cursor`](crate::pointer::drag_cursor)).
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    fn drag_cursor<'a>(
        &self,
        response: &'a Response,
        kind: crate::pointer::DragKind,
    ) -> crate::pointer::DragCursorFor<'a>;

    /// One line of cells on [`Track`]s, `add` filling them in order with
    /// [`Cells::cell`]: sized by content, fractions, percentages or em with
    /// minimum and maximum, [`Space`] apart, on one text baseline by
    /// default ([`Row`]; examples in the [`layout` module](self::layout)).
    fn row<R>(&mut self, row: Row, add: impl FnOnce(&mut Cells<'_>) -> R) -> InnerResponse<R>;

    /// Lines of cells on shared [`Track`]s, `add` filling them left to
    /// right with [`Cells::header`] and [`Cells::cell`]: a header/value
    /// grid, folding to one column when narrow ([`Grid::fold_below`]).
    fn grid<R>(&mut self, grid: Grid, add: impl FnOnce(&mut Cells<'_>) -> R) -> InnerResponse<R>;
}

impl UiPop for Ui {
    fn text_role(&mut self, role: TextRole, text: impl Into<String>) -> Response {
        let rich = role_text(self, role, text);
        self.label(rich)
    }

    fn section_title(&mut self, text: impl Into<String>) -> Response {
        self.text_role(TextRole::Section, text)
    }

    fn muted(&mut self, text: impl Into<String>) -> Response {
        self.text_role(TextRole::Muted, text)
    }

    fn meta(&mut self, text: impl Into<String>) -> Response {
        self.text_role(TextRole::Meta, text)
    }

    fn strong_text(&mut self, text: impl Into<String>) -> Response {
        let rich = strong_text(self, text);
        self.label(rich)
    }

    fn code_text(&mut self, text: impl Into<String>) -> Response {
        let color = Tokens::of(self.ctx()).text_role.body.color;
        self.label(RichText::new(text).monospace().color(color))
    }

    fn text_tone(&mut self, tone: Tone, text: impl Into<String>) -> Response {
        let color = tone_color(&Tokens::of(self.ctx()), tone);
        let rich = role_text(self, TextRole::Body, text).color(color);
        self.label(rich)
    }

    fn divider(&mut self) -> Response {
        let t = Tokens::of(self.ctx());
        let across = self.layout().main_dir().is_horizontal();
        let room = if self.is_sizing_pass() {
            egui::Vec2::ZERO
        } else {
            self.available_size_before_wrap()
        };
        let size = if across {
            egui::vec2(t.space.group, room.y)
        } else {
            egui::vec2(room.x, t.space.group)
        };
        let (rect, response) = self.allocate_exact_size(size, egui::Sense::hover());
        if self.is_rect_visible(rect) {
            let painter = self.painter();
            let width = crate::scale::snap(1.0, self.ctx().pixels_per_point());
            let stroke = egui::Stroke::new(width, t.border.default);
            if across {
                let x = painter.round_to_pixel_center(rect.center().x);
                painter.vline(x, rect.y_range(), stroke);
            } else {
                let y = painter.round_to_pixel_center(rect.center().y);
                painter.hline(rect.x_range(), y, stroke);
            }
        }
        response
    }

    fn sized(&mut self, size: Vec2, widget: impl Widget) -> Response {
        let layout = if self.layout().main_dir().is_horizontal() {
            Layout::centered_and_justified(egui::Direction::LeftToRight)
        } else {
            Layout::centered_and_justified(egui::Direction::TopDown)
        };
        self.sized_ui(size, layout, |ui| ui.add(widget)).inner
    }

    fn sized_ui<R>(
        &mut self,
        size: Vec2,
        layout: Layout,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let (rect, response) = self.allocate_exact_size(size, Sense::hover());
        let mut child = self.new_child(UiBuilder::new().max_rect(rect).layout(layout));
        let inner = add(&mut child);
        InnerResponse::new(inner, response)
    }

    fn rect_ui<R>(
        &mut self,
        rect: Rect,
        layout: Layout,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        let clip = rect.intersect(self.clip_rect());
        self.scope_builder(UiBuilder::new().max_rect(rect).layout(layout), |ui| {
            ui.set_clip_rect(clip);
            add(ui)
        })
    }

    fn space_role(&mut self, space: Space) {
        let points = space.points(&Tokens::of(self.ctx()));
        self.add_space(points);
    }

    fn card<R>(&mut self, tone: Tone, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let kit = Kit::of(self.ctx());
        kit.card
            .show(self, State::REST.toned(tone), Shadow::NONE, add)
    }

    fn popover<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let kit = Kit::of(self.ctx());
        let shadow = self.visuals().popup_shadow;
        kit.popover.show(self, State::REST, shadow, add)
    }

    fn button_tone(&mut self, tone: Tone, text: impl Into<WidgetText>) -> Response {
        if tone == Tone::Neutral {
            return self.button(text);
        }
        self.scoped(Stock::Button, tone, |ui| ui.button(text)).inner
    }

    fn scoped<R>(
        &mut self,
        stock: Stock,
        tone: Tone,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        Kit::of(self.ctx()).scope(self, stock, tone, add)
    }

    fn combo_box<R>(
        &mut self,
        combo: ComboBox,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        self.scoped(Stock::ComboBox, Tone::Neutral, |ui| combo.show_ui(ui, add))
            .inner
    }

    fn scroll_area<R>(
        &mut self,
        area: ScrollArea,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> ScrollAreaOutput<R> {
        self.scroll_area_viewport(area, |ui, _| add(ui))
    }

    fn scroll_area_viewport<R>(
        &mut self,
        area: ScrollArea,
        add: impl FnOnce(&mut Ui, Rect) -> R,
    ) -> ScrollAreaOutput<R> {
        // upstream: egui's `ScrollStyle` has no colours, the bars take
        // the widget states' (docs/upstream.md, "scroll bar colours").
        // egui paints the bars with the visuals of the `Ui` the area is
        // shown in, and builds its content's `Ui` from it: the content
        // gets the style outside the scope back.
        let style = self.style().clone();
        let kit = crate::kit::Kit::of(self.ctx());
        self.scoped(Stock::ScrollBar, Tone::Neutral, |ui| {
            let start = crate::scroll_keys::start(ui);
            // The track's inset along the bar (`scroll-bar.margin-track`):
            // egui takes the bar's rectangle before it lays the area out,
            // so it is the one the area showed in on the last pass.
            let track = ui.next_auto_id().with("egui_pop.scroll_track");
            let mut area = area;
            if kit.scroll.margin_track > 0.0
                && let Some(inner) = ui.data(|d| d.get_temp::<Rect>(track))
                && let Some(rect) = kit.scroll.track_rect(inner)
            {
                area = area.scroll_bar_rect(rect);
            }
            let out = area.show_viewport(ui, |ui, viewport| {
                ui.set_style(style);
                add(ui, viewport)
            });
            if kit.scroll.margin_track > 0.0 {
                let before = ui.data_mut(|d| d.get_temp::<Rect>(track));
                if before != Some(out.inner_rect) {
                    ui.data_mut(|d| d.insert_temp(track, out.inner_rect));
                    ui.ctx().request_repaint();
                }
            }
            crate::scroll_keys::answer(ui, &out, start);
            out
        })
        .inner
    }

    fn scroll_area_rows<R>(
        &mut self,
        area: ScrollArea,
        row_height: f32,
        rows: usize,
        add: impl FnOnce(&mut Ui, Range<usize>) -> R,
    ) -> ScrollAreaOutput<R> {
        // egui's `show_rows`, over `show_viewport`.
        let spacing = self.spacing().item_spacing.y;
        let pitch = row_height + spacing;
        self.scroll_area_viewport(area, |ui, viewport| {
            ui.set_height((pitch * rows as f32 - spacing).max(0.0));
            let mut first = (viewport.min.y / pitch).floor() as usize;
            let mut last = (viewport.max.y / pitch).ceil() as usize + 1;
            if last > rows {
                let span = last.saturating_sub(first);
                last = rows;
                first = rows.saturating_sub(span);
            }
            let top = ui.max_rect().top();
            let rect = Rect::from_x_y_ranges(
                ui.max_rect().x_range(),
                top + first as f32 * pitch..=top + last as f32 * pitch,
            );
            ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
                // The same ids, wherever the view starts.
                ui.skip_ahead_auto_ids(first);
                add(ui, first..last)
            })
            .inner
        })
    }

    fn scroll_area_contained<R>(
        &mut self,
        area: ScrollArea,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> ScrollAreaOutput<R> {
        let out = self.scroll_area(area, add);
        contain_wheel(self, &out);
        out
    }

    fn drag_cursor<'a>(
        &self,
        response: &'a Response,
        kind: crate::pointer::DragKind,
    ) -> crate::pointer::DragCursorFor<'a> {
        crate::pointer::drag_cursor(response, kind)
    }

    fn row<R>(&mut self, row: Row, add: impl FnOnce(&mut Cells<'_>) -> R) -> InnerResponse<R> {
        layout::show(self, row.0, add)
    }

    fn grid<R>(&mut self, grid: Grid, add: impl FnOnce(&mut Cells<'_>) -> R) -> InnerResponse<R> {
        layout::show(self, grid.0, add)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{self, ThemeSpec};

    /// The method names of the `UiPop` trait, read from this file.
    fn ui_pop_methods() -> Vec<&'static str> {
        let source = include_str!("ui.rs");
        let start = source.find("pub trait UiPop").expect("the trait");
        let end = start + source[start..].find("\n}\n").expect("its end");
        source[start..end]
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix("fn "))
            .map(|rest| {
                let len = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(rest.len());
                &rest[..len]
            })
            .collect()
    }

    #[test]
    fn no_ui_pop_method_is_shadowed_by_an_inherent_ui_method() {
        let names = ui_pop_methods();
        assert!(names.contains(&"strong_text") && names.contains(&"grid"));
        let shadowed: Vec<_> = names
            .iter()
            .filter(|n| EGUI_UI_METHODS.contains(n) && !SHADOWED_DEPRECATED.contains(n))
            .collect();
        assert!(
            shadowed.is_empty(),
            "egui's `Ui` has inherent methods of these names, which `ui.name(..)` \
             calls instead of `UiPop`'s: {shadowed:?}"
        );
        for name in SHADOWED_DEPRECATED {
            assert!(names.contains(name), "`{name}` is no longer in the trait");
        }
    }

    fn pass(ctx: &egui::Context, add: impl FnMut(&mut Ui)) {
        let mut out = ctx.run_ui(egui::RawInput::default(), add);
        out.textures_delta.clear();
    }

    #[test]
    fn a_popover_surface_in_an_app_placed_area_is_the_kits_popover() {
        let ctx = egui::Context::default();
        theme::install(&ctx, &ThemeSpec::new("pop-dark"));
        let kit = Kit::of(&ctx);
        let fill = kit.popover.style(State::REST).fill;
        assert_ne!(fill, egui::Color32::TRANSPARENT);
        let area = egui::Id::unique("test.notice");
        let mut shown = None;
        let mut frame = |ctx: &egui::Context, time: f64| {
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                egui::Area::new(area)
                    .order(egui::Order::Foreground)
                    .fixed_pos(egui::pos2(40.0, 40.0))
                    .show(ui.ctx(), |ui| {
                        shown = Some(ui.popover(|ui| ui.label("A notice")).response.rect);
                    });
            });
            out.textures_delta.clear();
            out
        };
        // An area's first pass only sizes it, then it fades in.
        frame(&ctx, 0.0);
        frame(&ctx, 1.0);
        let out = frame(&ctx, 2.0);
        let rect = shown.expect("drawn");
        // Its box, in the popover's fill, where the app placed its area.
        assert_eq!(rect.min, egui::pos2(40.0, 40.0));
        // egui's frame paints its shadow and its box as one list.
        fn rects(shape: &egui::Shape, into: &mut Vec<(Rect, egui::Color32)>) {
            match shape {
                egui::Shape::Rect(r) => into.push((r.rect, r.fill)),
                egui::Shape::Vec(v) => v.iter().for_each(|s| rects(s, into)),
                _ => {}
            }
        }
        let mut all = Vec::new();
        out.shapes.iter().for_each(|c| rects(&c.shape, &mut all));
        let boxed = all.iter().any(|(r, f)| {
            *f == fill && rect.expand(1.0).contains_rect(*r) && r.contains_rect(rect.shrink(2.0))
        });
        assert!(
            boxed,
            "the popover's box ({fill:?}) under the notice at {rect:?}: {all:?}"
        );
    }

    #[test]
    fn spacing_by_intent_reads_the_density() {
        let ctx = egui::Context::default();
        let t = theme::install(&ctx, &ThemeSpec::new("pop-dark"));
        assert_eq!(Space::Group.points(&t), t.space.m);
        assert_eq!(Space::Page.points(&t), t.space.xl);
        let compact = theme::install(
            &ctx,
            &ThemeSpec::new("pop-dark").density(theme::Density::Compact),
        );
        assert!(Space::Group.points(&compact) < Space::Group.points(&t));
        let mut moved = 0.0;
        pass(&ctx, |ui| {
            let before = ui.cursor().top();
            ui.space_role(Space::Section);
            moved = ui.cursor().top() - before;
        });
        assert_eq!(moved, compact.space.section);
    }

    #[test]
    fn a_role_s_weight_waits_for_the_theme_fonts() {
        let ctx = egui::Context::default();
        // No theme installed: no weighted family, nothing to panic on.
        pass(&ctx, |ui| {
            assert_eq!(role_family(ui.ctx(), Weight::SEMIBOLD), None);
            ui.text_role(TextRole::Title, "Title");
        });
        let t = theme::install(&ctx, &ThemeSpec::new("pop-dark"));
        for _ in 0..2 {
            pass(&ctx, |ui| {
                ui.section_title("Section");
                ui.muted("Muted");
                ui.meta("Meta");
                ui.text_role(TextRole::Title, "Title");
            });
        }
        assert_eq!(
            role_family(&ctx, t.text_role.section.weight),
            Some(fonts::family(Role::Ui, t.text_role.section.weight))
        );
        // Regular roles keep egui's own family.
        assert_eq!(role_family(&ctx, t.text_role.muted.weight), None);
    }

    #[test]
    fn a_toned_button_and_card_show_their_content() {
        let ctx = egui::Context::default();
        theme::install(&ctx, &ThemeSpec::new("pop-light"));
        pass(&ctx, |ui| {
            let fill = ui.visuals().widgets.inactive.weak_bg_fill;
            for tone in [Tone::Neutral, Tone::Accent, Tone::Danger, Tone::Muted] {
                let r = ui.button_tone(tone, "Go");
                assert!(r.rect.width() > 0.0);
            }
            // The tone stays in its scope.
            assert_eq!(ui.visuals().widgets.inactive.weak_bg_fill, fill);
            let inner = ui.card(Tone::Warning, |ui| ui.label("Inside").rect);
            assert!(inner.response.rect.contains_rect(inner.inner));
        });
    }
}
