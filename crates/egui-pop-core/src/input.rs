//! The decorated input: one component around a text, number, path or
//! search field, with what desktop and web toolkits put around a field.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! - a **placeholder** shown while the field is empty (`field.tone.muted.text`);
//! - **leading and trailing slots**: an icon, a prefix or a suffix text,
//!   a unit;
//! - a **clear ✕**, shown while there is a value and the field can be
//!   edited, reachable by Tab (Enter or Space presses it); clearing is
//!   one change, so one undo step when the field is bound to a store
//!   (and one in the field's own Ctrl+Z history);
//! - a **reset** arrow while the [field state](crate::field) offers
//!   "reset to default";
//! - **attached buttons** joined to the field's end (Browse, a picker),
//!   each a button of its own, or a **split button**: its action and a
//!   chevron opening a menu;
//! - a **dropdown chevron** opening a list of choices for the value;
//! - the **field state** drawn as every field draws it (an edge, a marker
//!   icon per state, the disabled reason as a tooltip), given to
//!   assistive technologies and reported to agents ([`crate::field`]).
//!
//! Two text fields go beyond one visible line of plain text:
//!
//! - a **masked** field ([`Input::masked`]: a password, a token, an API
//!   key) draws every character as a bullet, cannot be copied or cut
//!   from, and gives assistive technologies its name and the
//!   password-input role, never its value (the value and the text runs
//!   they read are bullets too); agents get its kind (`masked`), never
//!   its value;
//! - a **multi-line** field ([`Input::multiline`]) shows
//!   [`rows`](Input::rows) lines when it is short (2 by default), grows
//!   with its text up to [`max_rows`](Input::max_rows) lines (8 by
//!   default), then scrolls. Enter starts a new line and Ctrl+Enter
//!   (Cmd+Enter on macOS) submits ([`InputOutput::submitted`], the field
//!   keeps the focus); with [`submit_on_enter`](Input::submit_on_enter),
//!   Enter submits and Shift+Enter starts a new line, as in a chat box.
//!   Ctrl+Enter is a key binding's when the keymap binds it (the
//!   dispatcher leaves only Escape and Enter without Ctrl or Alt to a
//!   text field). Tab still moves the focus; the slots, the buttons and
//!   the markers stay on the first line.
//!
//! The field itself is egui's `TextEdit`, so it follows the focus rules
//! of every text input: while it has the focus, Escape and Enter are its
//! own (Escape leaves the field, Enter commits it), and no key binding
//! fires on them. A number field commits the value typed (a number or
//! an expression, as a slider's typed entry reads it) when it loses the
//! focus, except to Escape, which drops what was typed.
//!
//! Every part that takes the focus joins the Tab order of the
//! [`crate::tab`] scope it is drawn in, as one group: the field, then its
//! buttons. A disabled input is skipped; a read-only one is not.
//!
//! ```
//! use egui_pop_core::icon::Icon;
//! use egui_pop_core::input::{Attached, Input};
//! use egui_pop_model::field::FieldState;
//!
//! fn row(ui: &mut egui::Ui, query: &mut String, folder: &mut String, iso: &mut f64) {
//!     Input::search(query, "Search").placeholder("Search photos").show(ui);
//!     let out = Input::path(folder, "Folder")
//!         .button(Attached::text("Browse…", "Browse for a folder"))
//!         .state(FieldState::new().required())
//!         .show(ui);
//!     if out.button == Some(0) { /* open a file dialog */ }
//!     Input::number(iso, "ISO").unit("ISO").choices(["100", "400", "1600"]).show(ui);
//! }
//!
//! fn account(ui: &mut egui::Ui, token: &mut String, notes: &mut String) {
//!     Input::masked(token, "API token").show(ui);
//!     let out = Input::multiline(notes, "Notes").rows(3).max_rows(6).show(ui);
//!     if out.submitted { /* Ctrl+Enter: send the notes */ }
//! }
//! ```

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Rect, Response, Role, Sense, Stroke, StrokeKind,
    TextStyle, Ui, Vec2, WidgetInfo, pos2, vec2,
};
use egui_pop_model::field::FieldState;

use crate::field;
use crate::icon::{Icon, IconRef};
use crate::kit::{Kit, State, Tone};
use crate::state;
use crate::strings::{self, FIELD_RESET, INPUT_CHOICES, INPUT_CLEAR, INPUT_MORE};
use crate::tab::{self, TabStop};
use crate::theme::Tokens;
use crate::widgets;

/// What is typed in a number field while it has the focus, per field.
const BUFFER: state::Slot<String> = state::Slot::new("core.input.buffer");

/// What a field edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    /// A number, typed or as an expression; committed on leaving.
    Number,
    /// A file or folder path.
    Path,
    /// A search query: a magnifier before it, clearable.
    Search,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Number => "number",
            Kind::Path => "path",
            Kind::Search => "search",
        }
    }
}

/// Something drawn inside the field, before or after the value.
#[derive(Clone, Debug, PartialEq)]
pub enum Slot {
    Icon(Icon),
    /// A prefix, a suffix or a unit.
    Text(String),
}

/// A button joined to the field's end.
#[derive(Clone, Debug, PartialEq)]
pub struct Attached {
    icon: Option<Icon>,
    text: Option<String>,
    label: String,
    menu: Vec<String>,
}

impl Attached {
    /// An icon button, named `label` for assistive technologies; the
    /// icon by value or by name (`"folder"`, `"phosphor:export"`; an
    /// unknown name is reported and drawn as the placeholder).
    pub fn icon(icon: impl Into<IconRef>, label: &str) -> Self {
        Self {
            icon: Some(icon.into().or_placeholder("input button")),
            text: None,
            label: label.into(),
            menu: Vec::new(),
        }
    }

    /// A text button (`Browse…`), named `label`.
    pub fn text(text: &str, label: &str) -> Self {
        Self {
            icon: None,
            text: Some(text.into()),
            label: label.into(),
            menu: Vec::new(),
        }
    }

    /// Makes it a split button: its action, and a chevron opening a menu
    /// of these items.
    pub fn menu<S: Into<String>>(mut self, items: impl IntoIterator<Item = S>) -> Self {
        self.menu = items.into_iter().map(Into::into).collect();
        self
    }
}

enum Value<'a> {
    Text(&'a mut String),
    Number(&'a mut f64),
}

/// A decorated input (see [the module](self)).
#[must_use]
pub struct Input<'a> {
    value: Value<'a>,
    kind: Kind,
    label: String,
    placeholder: Option<String>,
    leading: Vec<Slot>,
    trailing: Vec<Slot>,
    clearable: bool,
    buttons: Vec<Attached>,
    choices: Vec<String>,
    state: FieldState,
    stop: TabStop,
    width: Option<f32>,
    cell: Option<Vec2>,
    salt: Option<Id>,
    default: Option<String>,
    precision: Option<usize>,
    masked: bool,
    lines: Option<Lines>,
}

/// A multi-line field's rows and its Enter key.
#[derive(Clone, Copy, Debug)]
struct Lines {
    rows: usize,
    max_rows: usize,
    submit_on_enter: bool,
}

impl std::fmt::Debug for Input<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Input")
            .field("kind", &self.kind)
            .field("label", &self.label)
            .field("masked", &self.masked)
            .field("multiline", &self.lines.is_some())
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// What an input did in a frame.
#[derive(Debug)]
pub struct InputOutput {
    /// The field's response (the text), with the state's tooltips.
    pub response: Response,
    /// The value changed (typed, cleared, chosen, reset, committed).
    pub changed: bool,
    /// The clear ✕ was pressed.
    pub cleared: bool,
    /// The reset arrow was pressed.
    pub reset: bool,
    /// Enter was pressed in the field (in a multi-line field,
    /// Ctrl+Enter, or Enter with [`Input::submit_on_enter`]).
    pub submitted: bool,
    /// An attached button (its action, for a split button), by index.
    pub button: Option<usize>,
    /// A split button's menu item: the button's index and the item's.
    pub menu: Option<(usize, usize)>,
    /// A choice from the dropdown, by index (the value is set to it).
    pub choice: Option<usize>,
}

impl<'a> Input<'a> {
    fn with(value: Value<'a>, kind: Kind, label: &str) -> Self {
        Self {
            value,
            kind,
            label: label.into(),
            placeholder: None,
            leading: Vec::new(),
            trailing: Vec::new(),
            clearable: matches!(kind, Kind::Search),
            buttons: Vec::new(),
            choices: Vec::new(),
            state: FieldState::default(),
            stop: TabStop::new(),
            width: None,
            cell: None,
            salt: None,
            default: None,
            precision: None,
            masked: false,
            lines: None,
        }
    }

    /// A text field named `label` (for assistive technologies and
    /// agents; the label itself is drawn by the form around it).
    ///
    /// Need (input): a single-line text field.
    /// Need (input): a text field with a placeholder.
    pub fn text(value: &'a mut String, label: &str) -> Self {
        Self::with(Value::Text(value), Kind::Text, label)
    }

    /// A masked text field (a password, a token, a key): every character
    /// a bullet, nothing to copy, and its value never given to assistive
    /// technologies or agents, only its name.
    ///
    /// Need (input): a password field.
    /// Need (input): a masked field for a secret (token, API key).
    pub fn masked(value: &'a mut String, label: &str) -> Self {
        let mut input = Self::with(Value::Text(value), Kind::Text, label);
        input.masked = true;
        input
    }

    /// A multi-line text field: [`rows`](Self::rows) lines tall, growing
    /// with its text up to [`max_rows`](Self::max_rows), then scrolling;
    /// Enter starts a new line, Ctrl+Enter submits (see
    /// [`submit_on_enter`](Self::submit_on_enter)).
    ///
    /// Need (input): a multi-line text field (a description, notes).
    /// Need (input): a text area that grows with its text.
    pub fn multiline(value: &'a mut String, label: &str) -> Self {
        let mut input = Self::with(Value::Text(value), Kind::Text, label);
        input.lines = Some(Lines {
            rows: 2,
            max_rows: 8,
            submit_on_enter: false,
        });
        input
    }

    /// A multi-line field's height when its text is short, in lines (at
    /// least one; 2 by default). Raises [`max_rows`](Self::max_rows) to
    /// it if needed. Nothing on a single-line field.
    pub fn rows(mut self, rows: usize) -> Self {
        if let Some(l) = &mut self.lines {
            l.rows = rows.max(1);
            l.max_rows = l.max_rows.max(l.rows);
        }
        self
    }

    /// The most lines a multi-line field grows to before it scrolls (8 by
    /// default; never under [`rows`](Self::rows)). Nothing on a
    /// single-line field.
    pub fn max_rows(mut self, max_rows: usize) -> Self {
        if let Some(l) = &mut self.lines {
            l.max_rows = max_rows.max(l.rows);
        }
        self
    }

    /// In a multi-line field, Enter submits and Shift+Enter starts a new
    /// line (a chat box); by default Enter starts a new line and
    /// Ctrl+Enter submits. Nothing on a single-line field.
    pub fn submit_on_enter(mut self, on: bool) -> Self {
        if let Some(l) = &mut self.lines {
            l.submit_on_enter = on;
        }
        self
    }

    /// A search field: a magnifier before the query, clearable.
    ///
    /// Need (input): a search field.
    /// Need (input): a filter box.
    pub fn search(value: &'a mut String, label: &str) -> Self {
        Self::with(Value::Text(value), Kind::Search, label).leading_icon(Icon::Search)
    }

    /// A path field.
    ///
    /// Need (input): a file or folder path field.
    pub fn path(value: &'a mut String, label: &str) -> Self {
        Self::with(Value::Text(value), Kind::Path, label)
    }

    /// A number field: a number or an expression (`x * 2`, `1,5`),
    /// committed when it loses the focus.
    ///
    /// Need (input): a number field with a unit.
    pub fn number(value: &'a mut f64, label: &str) -> Self {
        Self::with(Value::Number(value), Kind::Number, label)
    }

    /// The hint shown while the field is empty.
    pub fn placeholder(mut self, text: &str) -> Self {
        self.placeholder = Some(text.into());
        self
    }

    /// An icon before the value, by value or by name (an unknown name is
    /// reported and drawn as the placeholder).
    pub fn leading_icon(mut self, icon: impl Into<IconRef>) -> Self {
        self.leading
            .push(Slot::Icon(icon.into().or_placeholder("input icon")));
        self
    }

    /// A text before the value (`https://`, `#`).
    pub fn prefix(mut self, text: &str) -> Self {
        self.leading.push(Slot::Text(text.into()));
        self
    }

    /// A text after the value (`.jpg`).
    pub fn suffix(mut self, text: &str) -> Self {
        self.trailing.push(Slot::Text(text.into()));
        self
    }

    /// A unit after the value (`px`, `EV`).
    pub fn unit(self, unit: &str) -> Self {
        self.suffix(unit)
    }

    /// An icon after the value, by value or by name.
    pub fn trailing_icon(mut self, icon: impl Into<IconRef>) -> Self {
        self.trailing
            .push(Slot::Icon(icon.into().or_placeholder("input icon")));
        self
    }

    /// Shows the clear ✕ while there is a value (on by default for a
    /// search field). A number field has none: its reset is the default.
    pub fn clearable(mut self, on: bool) -> Self {
        self.clearable = on;
        self
    }

    /// A button joined to the field's end; its click is
    /// [`InputOutput::button`].
    pub fn button(mut self, button: Attached) -> Self {
        self.buttons.push(button);
        self
    }

    /// A dropdown chevron offering these values.
    pub fn choices<S: Into<String>>(mut self, items: impl IntoIterator<Item = S>) -> Self {
        self.choices = items.into_iter().map(Into::into).collect();
        self
    }

    /// The field's state: modified, required, invalid, disabled (with a
    /// reason), read-only, pending.
    pub fn state(mut self, state: FieldState) -> Self {
        self.state = state;
        self
    }

    /// Its place in the Tab order of the scope it is drawn in.
    pub fn tab(mut self, stop: TabStop) -> Self {
        self.stop = stop;
        self
    }

    /// Its width, attached buttons included (the available width by
    /// default).
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// A compact cell: exactly `size` (attached buttons included), for
    /// a slot of a dense layout (a HUD part, a table cell, a property
    /// row), its padding the small step (`space.xs`) instead of
    /// `space.s`, and none of the room an input otherwise keeps for its
    /// text (four rows' height). Everything else is the input's: the
    /// placeholder, the slots, the state, the keys and the name.
    pub fn cell(mut self, size: Vec2) -> Self {
        self.cell = Some(size);
        self
    }

    /// Distinguishes inputs with the same label in one place.
    pub fn id_salt(mut self, salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.salt = Some(Id::unique(salt));
        self
    }

    /// The value "reset to default" puts back, as text (for a number,
    /// its text: `"0"`).
    pub fn default_value(mut self, value: &str) -> Self {
        self.default = Some(value.into());
        self
    }

    /// A number field's decimals when it is not being typed in.
    pub fn precision(mut self, decimals: usize) -> Self {
        self.precision = Some(decimals);
        self
    }

    /// Draws the input; says what it did.
    pub fn show(self, ui: &mut Ui) -> InputOutput {
        let base = match self.salt {
            Some(s) => ui.make_persistent_id(("egui_pop_input", s)),
            None => ui.make_persistent_id(("egui_pop_input", &self.label)),
        };
        let disabled = self.state.is_disabled();
        ui.scope(|ui| {
            if disabled {
                ui.disable();
            }
            self.draw(ui, base)
        })
        .inner
    }

    fn draw(self, ui: &mut Ui, base: Id) -> InputOutput {
        let Input {
            mut value,
            kind,
            label,
            placeholder,
            leading,
            trailing,
            clearable,
            buttons,
            choices,
            state,
            stop,
            width,
            cell,
            salt: _,
            default,
            precision,
            masked,
            lines,
        } = self;
        let ctx = ui.ctx().clone();
        let t = Tokens::of(&ctx);
        let kit = Kit::of(&ctx);
        let fk = &kit.field;
        let h = cell.map_or(ui.spacing().interact_size.y, |c| c.y.max(1.0));
        if cell.is_some() {
            // The text field inside is no taller than the cell (egui's
            // text edit is at least a row high); `draw` runs in a scope.
            ui.spacing_mut().interact_size.y = h;
        }
        let pad = if cell.is_some() {
            t.space.xs
        } else {
            t.space.s
        };
        let gap = t.space.xs;
        let side = t.size.icon_inline.min(h - 2.0);
        let font = TextStyle::Body.resolve(ui.style());
        let measure = ui.painter().clone();
        let text_w = |s: &str| {
            measure
                .layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x
        };
        let button_font = TextStyle::Button.resolve(ui.style());
        let button_w = |s: &str| {
            measure
                .layout_no_wrap(s.to_string(), button_font.clone(), Color32::WHITE)
                .size()
                .x
        };
        let editable = state.is_editable();
        let group = stop
            .group_name()
            .map_or_else(|| format!("{base:?}"), str::to_string);
        let mut part_stop = TabStop::new().group(group);
        if let Some(i) = stop.explicit_index() {
            part_stop = part_stop.index(i);
        }

        // The row: the field, then the attached buttons.
        let widths: Vec<(f32, f32)> = buttons
            .iter()
            .map(|b| {
                let main = match &b.text {
                    Some(s) => button_w(s) + 2.0 * pad,
                    None => h,
                };
                let chevron = if b.menu.is_empty() { 0.0 } else { h * 0.8 };
                (main, chevron)
            })
            .collect();
        let attached: f32 = widths.iter().map(|(a, b)| a + b).sum();
        let total = match cell {
            Some(c) => c.x.max(attached + h),
            None => width
                .unwrap_or_else(|| ui.available_width())
                .max(attached + h * 4.0),
        };
        let text_id = base.with("text");

        // Its text as shown.
        let typing = ui.memory(|m| m.has_focus(text_id));
        let buffer_id = BUFFER.keyed(base);
        let mut buffer = match &value {
            Value::Text(s) => (*s).clone(),
            Value::Number(v) => match buffer_id.get(&ctx) {
                Some(b) if typing => b,
                _ => format_number(&ctx, **v, precision),
            },
        };
        let has_value = !buffer.is_empty();
        let mut changed = false;
        let mut cleared = false;
        let mut reset_pressed = false;
        let mut clicked = None;
        let mut menu = None;
        let mut choice = None;

        // What the field keeps beside its text, on its first line.
        let slot_w = |s: &Slot| match s {
            Slot::Icon(_) => side + gap,
            Slot::Text(x) => text_w(x) + gap,
        };
        let has_chevron = !choices.is_empty() && editable;
        let show_clear = clearable && kind != Kind::Number && editable && has_value;
        let markers_w = field::markers_width(&state, side * 0.8, gap / 2.0);
        let parts = [has_chevron, state.is_resettable(), show_clear]
            .iter()
            .filter(|on| **on)
            .count() as f32;
        let right_used = gap
            + parts * h * 0.8
            + trailing.iter().map(slot_w).sum::<f32>()
            + if markers_w > 0.0 {
                markers_w + gap
            } else {
                0.0
            };
        let left_used = field::edge_width(&t) + pad + leading.iter().map(slot_w).sum::<f32>();

        // A multi-line field: as many lines as its text, between its rows
        // and its most rows; the first line where a single-line text sits.
        let row_h = ui.text_style_height(&TextStyle::Body);
        let inset = ((h - row_h) / 2.0).max(0.0);
        let (field_h, shown_rows) = match lines {
            None => (h, 1),
            Some(l) => {
                let wrap = (total - attached - right_used - left_used).max(h);
                let n = measure
                    .layout(buffer.clone(), font.clone(), Color32::WHITE, wrap)
                    .rows
                    .len()
                    .max(1);
                let shown = n.clamp(l.rows, l.max_rows);
                (h.max(shown as f32 * row_h + 2.0 * inset), shown)
            }
        };
        let (_, row) = ui.allocate_space(vec2(total, field_h));
        let field = Rect::from_min_max(row.min, pos2(row.right() - attached, row.bottom()));
        let line = Rect::from_min_max(field.min, pos2(field.right(), field.top() + h));

        // From the right end of the field: the chevron, the reset, the
        // clear ✕, the trailing slots, the state's markers.
        let mut right = line.right() - gap;
        let mut take = |w: f32| {
            let r = Rect::from_min_max(pos2(right - w, line.top()), pos2(right, line.bottom()));
            right -= w;
            r
        };
        let chevron_rect = has_chevron.then(|| take(h * 0.8));
        let reset_rect = state.is_resettable().then(|| take(h * 0.8));
        let clear_rect = show_clear.then(|| take(h * 0.8));
        let mut trailing_rects = Vec::new();
        for s in trailing.iter().rev() {
            trailing_rects.push((take(slot_w(s)), s));
        }
        let markers_rect = (markers_w > 0.0).then(|| take(markers_w + gap));
        let mut left = line.left() + field::edge_width(&t) + pad;
        let mut leading_rects = Vec::new();
        for s in &leading {
            let w = slot_w(s);
            leading_rects.push((
                Rect::from_min_max(pos2(left, line.top()), pos2(left + w, line.bottom())),
                s,
            ));
            left += w;
        }
        let text_rect = if lines.is_some() {
            Rect::from_min_max(
                pos2(left, field.top() + inset),
                pos2(right.max(left + h), field.bottom() - inset),
            )
        } else {
            Rect::from_min_max(
                pos2(left, field.top()),
                pos2(right.max(left + h), field.bottom()),
            )
        };

        // The frame, then the text (drawn before the buttons inside the
        // field: egui's own Tab order is the order of drawing).
        if ui.is_rect_visible(field) {
            let radius = CornerRadius::same(t.radius.widget.round().clamp(0.0, 255.0) as u8);
            let fill = if state.read_only {
                fk.style_toned(Tone::Muted, State::REST).fill
            } else {
                fk.style(State::REST).fill
            };
            ui.painter().rect_filled(field, radius, fill);
        }
        let look = Look {
            id: text_id,
            width: text_rect.width(),
            color: fk.style(State::REST).text,
            number: kind == Kind::Number,
            masked,
            lines: lines.map(|l| (l, shown_rows)),
        };
        let response = if editable {
            let mut text = look.apply(if lines.is_some() {
                egui::TextEdit::multiline(&mut buffer)
            } else {
                egui::TextEdit::singleline(&mut buffer)
            });
            if let Some(p) = &placeholder {
                text = text.hint_text(
                    egui::RichText::new(p).color(fk.style_toned(Tone::Muted, State::REST).text),
                );
            }
            put_text(ui, text_rect, base, lines.is_some(), text)
        } else {
            // Read-only: selectable and copyable (unless masked), never
            // changed.
            let shown_text = buffer.clone();
            let mut shown: &str = shown_text.as_str();
            let text = look.apply(if lines.is_some() {
                egui::TextEdit::multiline(&mut shown)
            } else {
                egui::TextEdit::singleline(&mut shown)
            });
            put_text(ui, text_rect, base, lines.is_some(), text)
        };
        tab::stop(ui, &response, part_stop.clone().skip(!state.is_focusable()));

        // The buttons inside the field: what they change shows from the
        // next frame.
        let clear = clear_rect.map(|r| {
            let label = strings::tr_args(&ctx, &INPUT_CLEAR, &[("label", label.as_str())]);
            part(
                ui,
                r,
                base.with("clear"),
                PartLook::Icon(Icon::Close),
                &label,
            )
        });
        if let Some(c) = &clear {
            tab::stop(ui, c, part_stop.clone());
            if c.clicked() {
                buffer.clear();
                cleared = true;
                ui.memory_mut(|m| m.request_focus(text_id));
                ui.ctx().request_repaint();
            }
        }
        let reset = reset_rect.map(|r| {
            let label = strings::tr_args(&ctx, &FIELD_RESET, &[("label", label.as_str())]);
            part(
                ui,
                r,
                base.with("reset"),
                PartLook::Icon(Icon::Reset),
                &label,
            )
        });
        if let Some(r) = &reset {
            tab::stop(ui, r, part_stop.clone());
            if r.clicked() {
                reset_pressed = true;
                if let Some(d) = &default {
                    buffer = d.clone();
                }
            }
        }
        let chevron = chevron_rect.map(|r| {
            let label = strings::tr_args(&ctx, &INPUT_CHOICES, &[("label", label.as_str())]);
            part(
                ui,
                r,
                base.with("chevron"),
                PartLook::Icon(Icon::ChevronDown),
                &label,
            )
        });
        if let Some(c) = &chevron {
            tab::stop(ui, c, part_stop.clone());
            let popup = egui::Popup::menu(c).id(base.with("choices"));
            let popup_id = popup.get_id();
            let open = egui::Popup::is_id_open(&ctx, popup_id);
            ctx.accesskit_node_builder(c.id, |n| {
                n.set_has_popup(egui::accesskit::HasPopup::Listbox);
                n.set_expanded(open);
            });
            popup.show(|ui| {
                for (i, item) in choices.iter().enumerate() {
                    if widgets::selectable_label(ui, *item == buffer, item).clicked() {
                        choice = Some(i);
                    }
                }
            });
            if let Some(i) = choice {
                buffer = choices[i].clone();
            }
        }

        let submitted = match lines {
            None => response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)),
            // A multi-line field keeps the focus: its submit key is read
            // as it is pressed (the text edit left it in the events).
            Some(l) => {
                response.has_focus()
                    && ui.input(|i| {
                        i.events.iter().any(|e| match e {
                            egui::Event::Key {
                                key: egui::Key::Enter,
                                pressed: true,
                                modifiers,
                                ..
                            } => {
                                if l.submit_on_enter {
                                    !modifiers.shift
                                } else {
                                    modifiers.command
                                }
                            }
                            _ => false,
                        })
                    })
            }
        };

        // The value written back.
        match &mut value {
            Value::Text(s) => {
                if **s != buffer {
                    **s = buffer.clone();
                    changed = true;
                }
            }
            Value::Number(v) => {
                let commit = choice.is_some() || reset_pressed || !response.has_focus();
                if response.has_focus() {
                    buffer_id.set(&ctx, buffer.clone());
                }
                if commit {
                    buffer_id.remove(&ctx);
                    // Escape leaves the field with nothing: what was
                    // typed is dropped, the value stays.
                    let escaped =
                        typing && !response.has_focus() && crate::focus::escape_left(&ctx, text_id);
                    let typed_now = (typing || response.lost_focus()) && !escaped;
                    if (typed_now || choice.is_some() || reset_pressed)
                        && let Some(n) = crate::slider::parse_typed(&buffer)
                        && n != **v
                        && n.is_finite()
                    {
                        **v = n;
                        changed = true;
                    }
                }
            }
        }

        // Around the text: the slots, the markers, the border, the edge.
        if ui.is_rect_visible(field) {
            let painter = ui.painter();
            let muted = if ui.is_enabled() {
                t.text.muted
            } else {
                t.text.muted.gamma_multiply(0.6)
            };
            for (r, s) in leading_rects.iter().chain(trailing_rects.iter()) {
                paint_slot(ui, *r, s, side, &font, muted);
            }
            if let Some(r) = markers_rect {
                field::paint_markers(
                    ui,
                    pos2(r.left() + gap, r.center().y),
                    side * 0.8,
                    gap / 2.0,
                    &state,
                );
            }
            let radius = CornerRadius::same(t.radius.widget.round().clamp(0.0, 255.0) as u8);
            let focused = response.has_focus()
                || [&clear, &reset, &chevron]
                    .iter()
                    .any(|r| r.as_ref().is_some_and(Response::has_focus));
            // The field's outline shows the focus of the field and its parts.
            for r in [
                Some(&response),
                clear.as_ref(),
                reset.as_ref(),
                chevron.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                crate::focus::own_ring(&r.ctx, r.id);
            }
            let stroke = if focused {
                Stroke::new(2.0, t.focus_ring)
            } else if response.hovered() && ui.is_enabled() {
                Stroke::new(1.0, fk.style(State::HOVER).border_color)
            } else {
                Stroke::new(1.0, fk.style(State::REST).border_color)
            };
            painter.rect_stroke(field, radius, stroke, StrokeKind::Inside);
            field::paint_edge(ui, field, &state);
        }

        // The attached buttons.
        let mut x = field.right();
        for (i, (b, (main_w, chevron_w))) in buttons.iter().zip(&widths).enumerate() {
            let r = Rect::from_min_max(pos2(x, row.top()), pos2(x + main_w, row.bottom()));
            x += main_w;
            let look = match (&b.text, b.icon) {
                (Some(s), _) => PartLook::Text(s),
                (None, Some(icon)) => PartLook::Icon(icon),
                (None, None) => PartLook::Text(&b.label),
            };
            let resp = attached_part(ui, r, base.with(("button", i)), look, &b.label);
            tab::stop(ui, &resp, part_stop.clone());
            if resp.clicked() {
                clicked = Some(i);
            }
            if b.menu.is_empty() {
                continue;
            }
            let r = Rect::from_min_max(pos2(x, row.top()), pos2(x + chevron_w, row.bottom()));
            x += chevron_w;
            let more = strings::tr_args(&ctx, &INPUT_MORE, &[("label", b.label.as_str())]);
            let resp = attached_part(
                ui,
                r,
                base.with(("menu", i)),
                PartLook::Icon(Icon::ChevronDown),
                &more,
            );
            tab::stop(ui, &resp, part_stop.clone());
            if let Some(item) = split_menu(ui, &resp, base.with(("menu-popup", i)), &b.menu) {
                menu = Some((i, item));
            }
        }

        // The state: named, announced, reported.
        let shown_label = label.clone();
        let hint = placeholder.clone();
        ctx.accesskit_node_builder(response.id, |n| {
            n.set_label(shown_label);
            // The placeholder, whatever egui makes of a styled hint.
            if let Some(p) = hint {
                n.set_placeholder(p);
            }
        });
        field::access(&ctx, response.id, &state);
        // What agents read: its kind, never a masked field's value.
        let kind_name = if masked {
            "masked"
        } else if lines.is_some() {
            "multiline"
        } else {
            kind.name()
        };
        field::record(&ctx, response.id, &label, kind_name, &state);
        crate::a11y::input(&ctx, response.id, &label);
        // Its icons, by name, for agents.
        for slot in leading.iter().chain(trailing.iter()) {
            if let Slot::Icon(i) = slot {
                crate::icon::registry::record(&ctx, "input", &label, i.name());
            }
        }
        for b in &buttons {
            if let Some(i) = b.icon {
                crate::icon::registry::record(&ctx, "input button", &b.label, i.name());
            }
        }
        let mut response = field::hover(ui, &response, &state);
        if changed {
            response.mark_changed();
        }
        InputOutput {
            response,
            changed,
            cleared,
            reset: reset_pressed,
            submitted,
            button: clicked,
            menu,
            choice,
        }
    }
}

/// How the field's text is drawn: no frame of its own (the input draws
/// it), a single line centred in the row (a number to the right), a
/// multi-line text from the top.
struct Look {
    id: Id,
    width: f32,
    color: Color32,
    number: bool,
    masked: bool,
    /// A multi-line field: its rows and the lines shown.
    lines: Option<(Lines, usize)>,
}

impl Look {
    fn apply<'t>(&self, text: egui::TextEdit<'t>) -> egui::TextEdit<'t> {
        let align = match (self.lines, self.number) {
            (Some(_), _) => Align2::LEFT_TOP,
            (None, true) => Align2::RIGHT_CENTER,
            (None, false) => Align2::LEFT_CENTER,
        };
        let mut text = text
            .id(self.id)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .align(align)
            .desired_width(self.width)
            .text_color(self.color)
            .password(self.masked);
        if let Some((l, shown)) = self.lines {
            // Enter starts a new line unless it submits; then Shift+Enter
            // does.
            let newline = if l.submit_on_enter {
                egui::Modifiers::SHIFT
            } else {
                egui::Modifiers::NONE
            };
            text = text
                .desired_rows(shown)
                .return_key(egui::KeyboardShortcut::new(newline, egui::Key::Enter));
        }
        text
    }
}

/// Puts the field's text in its place: a single line as it is, a
/// multi-line text in a scroll area as tall as the lines shown.
fn put_text(
    ui: &mut Ui,
    rect: Rect,
    base: Id,
    multiline: bool,
    text: egui::TextEdit<'_>,
) -> Response {
    if !multiline {
        return ui.put(rect, text);
    }
    // From the top-left of its place, whatever the layout around (a
    // grid's cell centres its content), in a child that takes no room of
    // its own: the field's row is allocated already.
    let builder = egui::UiBuilder::new()
        .id_salt(base.with("area"))
        .max_rect(rect)
        .layout(egui::Layout::top_down(egui::Align::Min));
    let mut child = ui.new_child(builder);
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical()
        .id_salt(base.with("scroll"))
        .max_height(rect.height())
        .auto_shrink([false, true])
        .show(&mut child, |ui| ui.add(text))
        .inner
}

fn format_number(ctx: &egui::Context, v: f64, precision: Option<usize>) -> String {
    match precision {
        Some(p) => strings::number(ctx, v, p),
        None => format!("{v}"),
    }
}

fn paint_slot(ui: &Ui, rect: Rect, slot: &Slot, side: f32, font: &FontId, color: Color32) {
    match slot {
        Slot::Icon(i) => {
            let r = Rect::from_center_size(
                pos2(rect.left() + side / 2.0, rect.center().y),
                vec2(side, side),
            );
            i.paint(ui.painter(), r, color);
        }
        Slot::Text(s) => {
            ui.painter().text(
                pos2(rect.left(), rect.center().y),
                Align2::LEFT_CENTER,
                s,
                font.clone(),
                color,
            );
        }
    }
}

#[derive(Clone, Copy)]
enum PartLook<'s> {
    Icon(Icon),
    Text(&'s str),
}

/// A small button inside the field (✕, reset, chevron): focusable,
/// named, pressed by a click, Enter or Space.
fn part(ui: &mut Ui, rect: Rect, id: Id, look: PartLook<'_>, label: &str) -> Response {
    let r = ui.interact(rect, id, Sense::click());
    let enabled = ui.is_enabled();
    r.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));
    if ui.is_rect_visible(rect) {
        let t = Tokens::of(ui.ctx());
        let side = t.size.icon_inline.min(rect.height() - 4.0) * 0.8;
        let sq = Rect::from_center_size(rect.center(), vec2(side, side) + vec2(4.0, 4.0));
        let radius = CornerRadius::same(t.radius.widget.round().clamp(0.0, 255.0) as u8);
        if r.hovered() || r.has_focus() {
            ui.painter().rect_filled(sq, radius, t.surface.hover);
        }
        if r.has_focus() {
            crate::focus::own_ring(ui.ctx(), r.id);
            ui.painter().rect_stroke(
                sq,
                radius,
                Stroke::new(1.5, t.focus_ring),
                StrokeKind::Inside,
            );
        }
        let color = if !enabled {
            t.text.muted
        } else if r.hovered() {
            t.text.strong
        } else {
            t.text.default
        };
        if let PartLook::Icon(i) = look {
            i.paint(
                ui.painter(),
                Rect::from_center_size(rect.center(), vec2(side, side)),
                color,
            );
        }
    }
    r.on_hover_text(label)
}

/// A button joined to the field: the button tokens, its own border.
fn attached_part(ui: &mut Ui, rect: Rect, id: Id, look: PartLook<'_>, label: &str) -> Response {
    let r = ui.interact(rect, id, Sense::click());
    let enabled = ui.is_enabled();
    r.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));
    if ui.is_rect_visible(rect) {
        let t = Tokens::of(ui.ctx());
        let kit = Kit::of(ui.ctx());
        let b = |state| kit.button.style(state);
        let fill = if r.is_pointer_button_down_on() {
            b(State::ACTIVE).fill
        } else if r.hovered() {
            b(State::HOVER).fill
        } else {
            b(State::REST).fill
        };
        let radius = CornerRadius::same(b(State::REST).radius.round().clamp(0.0, 255.0) as u8);
        ui.painter().rect_filled(rect, radius, fill);
        let stroke = if r.has_focus() {
            crate::focus::own_ring(ui.ctx(), r.id);
            Stroke::new(2.0, t.focus_ring)
        } else {
            Stroke::new(1.0, kit.field.style(State::REST).border_color)
        };
        ui.painter()
            .rect_stroke(rect, radius, stroke, StrokeKind::Inside);
        let color = if enabled {
            b(State::REST).text
        } else {
            t.text.muted
        };
        match look {
            PartLook::Icon(i) => {
                let side = t.size.icon_inline.min(rect.height() - 4.0);
                i.paint(
                    ui.painter(),
                    Rect::from_center_size(rect.center(), vec2(side, side)),
                    color,
                );
            }
            PartLook::Text(s) => {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    s,
                    TextStyle::Button.resolve(ui.style()),
                    color,
                );
            }
        }
    }
    if matches!(look, PartLook::Icon(_)) {
        r.on_hover_text(label)
    } else {
        r
    }
}

/// A split button's menu under its chevron: the item chosen.
fn split_menu(ui: &Ui, chevron: &Response, id: Id, items: &[String]) -> Option<usize> {
    let popup = egui::Popup::menu(chevron).id(id);
    let open = egui::Popup::is_id_open(ui.ctx(), id);
    ui.ctx().accesskit_node_builder(chevron.id, |n| {
        n.set_has_popup(egui::accesskit::HasPopup::Menu);
        n.set_expanded(open);
    });
    let mut chosen = None;
    popup.show(|ui| {
        for (i, item) in items.iter().enumerate() {
            if ui.button(item).clicked() {
                chosen = Some(i);
            }
        }
    });
    chosen
}

/// What a split button did.
#[derive(Debug)]
pub struct SplitOutput {
    /// The action's response.
    pub response: Response,
    /// The action was pressed.
    pub clicked: bool,
    /// A menu item, by index.
    pub chosen: Option<usize>,
}

/// A split button on its own: an action (`text`) and a chevron opening
/// a menu of `items`. Both parts take the focus and join the Tab order.
///
/// Need (action): a button with a menu of alternatives (split button).
pub fn split_button(ui: &mut Ui, text: &str, items: &[&str]) -> SplitOutput {
    let t = Tokens::of(ui.ctx());
    let h = ui.spacing().interact_size.y;
    let font = TextStyle::Button.resolve(ui.style());
    let w = ui
        .painter()
        .layout_no_wrap(text.to_string(), font, Color32::WHITE)
        .size()
        .x
        + 2.0 * t.space.s;
    let (id, row) = ui.allocate_space(vec2(w + h * 0.8, h));
    let main = Rect::from_min_max(row.min, pos2(row.left() + w, row.bottom()));
    let chev = Rect::from_min_max(pos2(main.right(), row.top()), row.max);
    let group = format!("{id:?}");
    let response = attached_part(ui, main, id.with("action"), PartLook::Text(text), text);
    tab::stop(ui, &response, TabStop::new().group(group.clone()));
    let more = strings::tr_args(ui.ctx(), &INPUT_MORE, &[("label", text)]);
    let c = attached_part(
        ui,
        chev,
        id.with("chevron"),
        PartLook::Icon(Icon::ChevronDown),
        &more,
    );
    tab::stop(ui, &c, TabStop::new().group(group));
    let items: Vec<String> = items.iter().map(|s| s.to_string()).collect();
    let chosen = split_menu(ui, &c, id.with("menu"), &items);
    SplitOutput {
        clicked: response.clicked(),
        response,
        chosen,
    }
}
