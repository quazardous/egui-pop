//! egui's small widgets drawn through the kit (`docs/kit.md`): a
//! checkbox, a radio button, a selectable row, a toggle chip, and the
//! arrow of a combo box.
//!
//! Each keeps egui's layout (egui's atoms, sizes and spacing), its
//! interaction and what it tells assistive technology; the kit paints
//! the box (the `checkbox`, `radio`, `selectable-row`, `toggle-chip` and
//! `combo-arrow` components) and the marks are glyph roles (`check`,
//! `indeterminate`, `radio`, `dropdown`). A role's default icon is drawn
//! with egui's own geometry, so the default kit looks like egui; a kit
//! naming another icon gets that icon. They replace egui's own:
//!
//! | egui | egui-pop |
//! |---|---|
//! | `ui.checkbox(&mut on, text)` | [`checkbox`]`(ui, &mut on, text)`, or [`Checkbox`] |
//! | `ui.radio(on, text)` | [`radio`]`(ui, on, text)`, or [`RadioButton`] |
//! | `ui.radio_value(&mut v, x, text)` | [`radio_value`]`(ui, &mut v, x, text)` |
//! | `ui.selectable_label(on, text)` | [`selectable_label`]`(ui, on, text)`, or [`SelectableRow`] |
//! | `ui.selectable_value(&mut v, x, text)` | [`selectable_value`]`(ui, &mut v, x, text)` |
//! | `ComboBox::from_id_salt(id)` | [`combo_box`]`(id)` |
//! | `ComboBox::new(id, label)` | [`combo_box_labelled`]`(id, label)` |
//! | a selectable label used as a toggle | [`ToggleChip`] |
//!
//! A combo box's own box stays egui's: it follows `button` through
//! egui's `Visuals` (`theme::visuals_with`); only its arrow is the kit's.

use egui::{
    AsIdSalt, Atom, AtomLayout, Atoms, Color32, ComboBox, Frame, Id, IntoAtoms, Margin,
    NumExt as _, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, Widget, WidgetInfo, WidgetText,
    WidgetType, epaint, pos2, style::WidgetVisuals, vec2,
};

use crate::icon::Icon;
use crate::kit::{Component, Glyph, Kit, KitCx, Part, State};

/// The colour of a part's label and mark: egui's override when the style
/// sets one, else the state's `text`.
fn text_color(ui: &Ui, component: &Component, state: State) -> Color32 {
    ui.visuals()
        .override_text_color
        .unwrap_or(component.style(state).text)
}

/// Paints a component's box over `rect` in `state`.
fn paint_box(ui: &Ui, component: &Component, rect: Rect, state: State) {
    component.paint(
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
}

/// The state egui's response shows, with `SELECTED` when `on` and
/// `DISABLED` in a disabled `ui`.
fn state_of(ui: &Ui, r: &Response, on: bool) -> State {
    State::from_response(r)
        .with(State::SELECTED, on)
        .with(State::DISABLED, !ui.is_enabled())
}

/// A checkbox: its box is the kit's `checkbox` component (`selected`
/// while checked), its mark the `check` glyph (`indeterminate` when so
/// shown), in the state's `text`, its label in the `text` of the same
/// state unchecked. egui's
/// [`egui::Checkbox`], drawn through the kit.
#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct Checkbox<'a> {
    checked: &'a mut bool,
    atoms: Atoms<'a>,
    indeterminate: bool,
}

impl std::fmt::Debug for Checkbox<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Checkbox")
            .field("checked", &self.checked)
            .field("indeterminate", &self.indeterminate)
            .finish_non_exhaustive()
    }
}

impl<'a> Checkbox<'a> {
    pub fn new(checked: &'a mut bool, atoms: impl IntoAtoms<'a>) -> Self {
        Checkbox {
            checked,
            atoms: atoms.into_atoms(),
            indeterminate: false,
        }
    }

    /// A checkbox without a label (name it for assistive technology).
    pub fn without_text(checked: &'a mut bool) -> Self {
        Self::new(checked, ())
    }

    /// Shows the `indeterminate` mark (neither checked nor unchecked); a
    /// click still toggles the value.
    #[inline]
    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }
}

impl Widget for Checkbox<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let Checkbox {
            checked,
            atoms,
            indeterminate,
        } = self;
        let kit = Kit::of(ui.ctx());
        let mut toggle = Toggle::allocate(ui, &kit.checkbox, atoms, "egui_pop::checkbox");
        if toggle.response().clicked() {
            *checked = !*checked;
            toggle.response_mut().mark_changed();
        }
        let (on, text) = (*checked, toggle.text.clone());
        toggle.response().widget_info(|| {
            let name = text.as_deref().unwrap_or("");
            if indeterminate {
                WidgetInfo::labeled(WidgetType::Checkbox, ui.is_enabled(), name)
            } else {
                WidgetInfo::selected(WidgetType::Checkbox, ui.is_enabled(), on, name)
            }
        });
        let mark = if indeterminate {
            Some(Glyph::Indeterminate)
        } else {
            on.then_some(Glyph::Check)
        };
        toggle.paint(ui, &kit, &kit.checkbox, on, mark, 0.0)
    }
}

/// A radio button: its circle is the kit's `radio` component (`selected`
/// while chosen), its mark the `radio` glyph in the state's `text`, its
/// label in the `text` of the same state unchosen. egui's [`egui::RadioButton`], drawn through the
/// kit.
#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct RadioButton<'a> {
    checked: bool,
    atoms: Atoms<'a>,
}

impl std::fmt::Debug for RadioButton<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioButton")
            .field("checked", &self.checked)
            .finish_non_exhaustive()
    }
}

impl<'a> RadioButton<'a> {
    pub fn new(checked: bool, atoms: impl IntoAtoms<'a>) -> Self {
        RadioButton {
            checked,
            atoms: atoms.into_atoms(),
        }
    }
}

impl Widget for RadioButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let RadioButton { checked, atoms } = self;
        let kit = Kit::of(ui.ctx());
        let toggle = Toggle::allocate(ui, &kit.radio, atoms, "egui_pop::radio_button");
        let text = toggle.text.clone();
        toggle.response().widget_info(|| {
            WidgetInfo::selected(
                WidgetType::RadioButton,
                ui.is_enabled(),
                checked,
                text.as_deref().unwrap_or(""),
            )
        });
        // egui's circle grows by its `expansion` under the pointer.
        let expansion = ui.style().interact(toggle.response()).expansion;
        toggle.paint(
            ui,
            &kit,
            &kit.radio,
            checked,
            checked.then_some(Glyph::Radio),
            expansion,
        )
    }
}

/// A checkbox or a radio button laid out as egui lays them out: a square
/// of the box's side (`width`, else egui's icon width) before the label,
/// at least egui's interaction height.
struct Toggle<'a> {
    prepared: egui::AllocatedAtomLayout<'a>,
    rect_id: Id,
    side: f32,
    text: Option<String>,
}

impl<'a> Toggle<'a> {
    fn allocate(ui: &mut Ui, component: &Component, mut atoms: Atoms<'a>, id: &str) -> Self {
        let side = if component.data.width > 0.0 {
            component.data.width
        } else {
            ui.spacing().icon_width
        };
        let mut min_size = Vec2::splat(ui.spacing().interact_size.y);
        min_size.y = min_size.y.at_least(side);
        // Centred on the line: the icon is as tall as the line.
        let mut icon_size = Vec2::splat(side);
        icon_size.y = icon_size.y.at_least(min_size.y);
        let rect_id = Id::new(id);
        atoms.push_left(Atom::custom(rect_id, icon_size));
        let text = atoms.text().map(String::from);
        let prepared = AtomLayout::new(atoms)
            .sense(Sense::click())
            .min_size(min_size)
            .allocate(ui);
        Toggle {
            prepared,
            rect_id,
            side,
            text,
        }
    }

    fn response(&self) -> &Response {
        &self.prepared.response
    }

    fn response_mut(&mut self) -> &mut Response {
        &mut self.prepared.response
    }

    /// Paints the label, the box (grown by `expansion`) and the mark.
    fn paint(
        mut self,
        ui: &Ui,
        kit: &Kit,
        component: &Component,
        on: bool,
        mark: Option<Glyph>,
        expansion: f32,
    ) -> Response {
        if !ui.is_rect_visible(self.prepared.response.rect) {
            return self.prepared.response;
        }
        let state = state_of(ui, &self.prepared.response, on);
        let color = text_color(ui, component, state);
        // The label beside the box does not change when checked: a kit
        // filling a checked box keeps its label readable.
        let label = text_color(ui, component, state_of(ui, self.response(), false));
        self.prepared.fallback_text_color = label;
        let (side, rect_id) = (self.side, self.rect_id);
        let r = self.prepared.paint(ui);
        if let Some(rect) = r.rect(rect_id) {
            let big = Rect::from_center_size(
                pos2(rect.left() + side / 2.0, rect.center().y),
                Vec2::splat(side),
            );
            paint_box(ui, component, big.expand(expansion), state);
            if let Some(mark) = mark {
                paint_mark(ui, kit, mark, big, color);
            }
        }
        r.response
    }
}

/// Paints a toggle's mark in its box `big`: the role's icon, or, for the
/// role's default icon, egui's own mark (a check line, a dash, a dot).
fn paint_mark(ui: &Ui, kit: &Kit, mark: Glyph, big: Rect, color: Color32) {
    let icon = kit.glyph(mark);
    let small = Rect::from_center_size(big.center(), Vec2::splat(ui.spacing().icon_width_inner));
    let stroke = Stroke::new(ui.visuals().widgets.inactive.fg_stroke.width, color);
    let painter = ui.painter();
    if icon != mark.default_icon() {
        // A kit's own icon fills most of the box.
        let side = (big.width() * 0.8).round();
        icon.paint(
            painter,
            Rect::from_center_size(big.center(), Vec2::splat(side)),
            color,
        );
        return;
    }
    match mark {
        Glyph::Check => {
            painter.add(Shape::line(
                vec![
                    pos2(small.left(), small.center().y),
                    pos2(small.center().x, small.bottom()),
                    pos2(small.right(), small.top()),
                ],
                stroke,
            ));
        }
        Glyph::Indeterminate => {
            painter.add(Shape::hline(small.x_range(), small.center().y, stroke));
        }
        Glyph::Radio => {
            painter.add(epaint::CircleShape {
                center: small.center(),
                radius: small.width() / 3.0,
                fill: color,
                stroke: Stroke::NONE,
            });
        }
        _ => icon.paint(painter, small, color),
    }
}

/// A checkbox ([`Checkbox`]): `ui.checkbox`, drawn through the kit.
pub fn checkbox<'a>(ui: &mut Ui, checked: &'a mut bool, atoms: impl IntoAtoms<'a>) -> Response {
    ui.add(Checkbox::new(checked, atoms))
}

/// A radio button ([`RadioButton`]): `ui.radio`, drawn through the kit.
pub fn radio<'a>(ui: &mut Ui, checked: bool, atoms: impl IntoAtoms<'a>) -> Response {
    ui.add(RadioButton::new(checked, atoms))
}

/// A radio button setting `current` to `alternative` when clicked:
/// `ui.radio_value`, drawn through the kit.
pub fn radio_value<'a, V: PartialEq>(
    ui: &mut Ui,
    current: &mut V,
    alternative: V,
    atoms: impl IntoAtoms<'a>,
) -> Response {
    let mut r = radio(ui, *current == alternative, atoms);
    if r.clicked() && *current != alternative {
        *current = alternative;
        r.mark_changed();
    }
    r
}

/// A row picked from a short list: its box is the kit's `selectable-row`
/// component, `selected` for the chosen one (never `hover` then, so the
/// choice stays visible under the pointer), its label in the state's
/// `text`. egui's `ui.selectable_label` (a selectable [`egui::Button`]),
/// drawn through the kit, with egui's padding and height.
#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct SelectableRow<'a> {
    selected: bool,
    atoms: Atoms<'a>,
    toggle: bool,
    glyph: Option<Icon>,
    steady: bool,
    wrap: bool,
}

impl std::fmt::Debug for SelectableRow<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelectableRow")
            .field("selected", &self.selected)
            .finish_non_exhaustive()
    }
}

impl<'a> SelectableRow<'a> {
    pub fn new(selected: bool, atoms: impl IntoAtoms<'a>) -> Self {
        SelectableRow {
            selected,
            atoms: atoms.into_atoms(),
            toggle: false,
            glyph: None,
            steady: false,
            wrap: false,
        }
    }

    /// Keeps the row's size in every state. Like egui's selectable
    /// button, a row is otherwise its border's width smaller on each side
    /// at rest than chosen, hovered or pressed (docs/upstream.md): a list
    /// of steady rows does not move under the pointer.
    #[inline]
    pub fn steady(mut self) -> Self {
        self.steady = true;
        self
    }

    /// Wraps its text at the available width.
    #[inline]
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }
}

impl Widget for SelectableRow<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let kit = Kit::of(ui.ctx());
        row_ui(ui, &kit.selectable_row, self, None)
    }
}

/// A small toggle showing a short text or a glyph, on while `selected`:
/// its box is the kit's `toggle-chip` component (based on
/// `selectable-row`), its text or glyph in the state's `text`. It is a
/// checkbox for assistive technology, named by [`ToggleChip::name`]
/// (else its text). The find bar's case, whole word and regular
/// expression options.
#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct ToggleChip<'a> {
    on: &'a mut bool,
    atoms: Atoms<'a>,
    glyph: Option<Icon>,
    name: Option<String>,
}

impl std::fmt::Debug for ToggleChip<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToggleChip")
            .field("on", &self.on)
            .field("glyph", &self.glyph)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl<'a> ToggleChip<'a> {
    /// A chip showing `atoms` (a short text), toggling `on` when clicked.
    pub fn new(on: &'a mut bool, atoms: impl IntoAtoms<'a>) -> Self {
        ToggleChip {
            on,
            atoms: atoms.into_atoms(),
            glyph: None,
            name: None,
        }
    }

    /// Draws `icon` centred on the chip, over its text (a blank text of
    /// the width wanted, such as one monospace space, sizes it).
    #[inline]
    pub fn glyph(mut self, icon: Icon) -> Self {
        self.glyph = Some(icon);
        self
    }

    /// Its name for assistive technology, when its text says too little.
    #[inline]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

impl Widget for ToggleChip<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let ToggleChip {
            on,
            atoms,
            glyph,
            name,
        } = self;
        let kit = Kit::of(ui.ctx());
        let row = SelectableRow {
            selected: *on,
            atoms,
            toggle: true,
            glyph,
            steady: false,
            wrap: false,
        };
        let mut r = row_ui(ui, &kit.toggle_chip, row, name);
        if r.clicked() {
            *on = !*on;
            r.mark_changed();
        }
        r
    }
}

/// Lays a selectable row out as egui lays out a selectable button
/// (egui's button padding less its resting border, at least egui's
/// interaction height), then paints its box, its label and its glyph.
fn row_ui(
    ui: &mut Ui,
    component: &Component,
    row: SelectableRow<'_>,
    name: Option<String>,
) -> Response {
    let SelectableRow {
        selected,
        atoms,
        toggle,
        glyph,
        steady,
        wrap,
    } = row;
    let text = atoms.text().map(String::from);
    // upstream: as egui's selectable button, the row is framed (its
    // margin the button padding) when chosen or in any state but rest
    // in the last pass, and unframed at rest (the padding less the
    // resting border): its size changes under the pointer unless steady
    // (docs/upstream.md).
    let last = ui
        .ctx()
        .read_response(ui.next_auto_id())
        .map(|r| r.widget_state())
        .unwrap_or_default();
    let framed = steady || selected || last != egui::widget_style::WidgetState::Inactive;
    let mut padding = ui.spacing().button_padding;
    if !framed {
        padding -= Vec2::splat(ui.visuals().widgets.inactive.bg_stroke.width);
    }
    let mut prepared = AtomLayout::new(atoms)
        .sense(Sense::click())
        .fallback_font(
            ui.style()
                .override_font_id
                .clone()
                .unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style())),
        )
        .frame(Frame::new().inner_margin(Margin::from(padding)))
        .min_size(vec2(0.0, ui.spacing().interact_size.y));
    if wrap {
        prepared = prepared.wrap_mode(egui::TextWrapMode::Wrap);
    }
    let mut prepared = prepared.allocate(ui);
    let r = &prepared.response;
    let name = name.or(text).unwrap_or_default();
    let enabled = ui.is_enabled();
    r.widget_info(|| {
        if toggle {
            WidgetInfo::selected(WidgetType::Checkbox, enabled, selected, &name)
        } else {
            WidgetInfo::selected(WidgetType::Button, enabled, selected, &name)
        }
    });
    if let Some(cursor) = ui.visuals().interact_cursor
        && r.hovered()
    {
        ui.ctx().set_cursor_icon(cursor);
    }
    if !ui.is_rect_visible(r.rect) {
        return prepared.response;
    }
    // The chosen row is never `hover`: the choice stays under the pointer.
    let state = state_of(ui, r, selected);
    let state = if selected {
        State::REST
            .with(State::SELECTED, true)
            .with(State::FOCUS, state.contains(State::FOCUS))
            .with(State::ACTIVE, state.contains(State::ACTIVE))
            .with(State::DISABLED, state.contains(State::DISABLED))
    } else {
        state
    };
    // egui's frame grows by its `expansion` under the pointer.
    let expansion = if state.contains(State::HOVER) || state.contains(State::ACTIVE) {
        ui.style().interact(r).expansion
    } else {
        0.0
    };
    paint_box(ui, component, r.rect.expand(expansion), state);
    let color = text_color(ui, component, state);
    prepared.fallback_text_color = color;
    let rect = prepared.response.rect;
    let r = prepared.paint(ui).response;
    if let Some(icon) = glyph {
        let side = rect.height() * 0.6;
        icon.paint(
            ui.painter(),
            Rect::from_center_size(rect.center(), Vec2::splat(side)),
            color,
        );
    }
    r
}

/// A selectable row ([`SelectableRow`]): `ui.selectable_label`, drawn
/// through the kit.
pub fn selectable_label<'a>(ui: &mut Ui, selected: bool, atoms: impl IntoAtoms<'a>) -> Response {
    ui.add(SelectableRow::new(selected, atoms))
}

/// A selectable row setting `current` to `value` when clicked:
/// `ui.selectable_value`, drawn through the kit.
pub fn selectable_value<'a, V: PartialEq>(
    ui: &mut Ui,
    current: &mut V,
    value: V,
    atoms: impl IntoAtoms<'a>,
) -> Response {
    let mut r = selectable_label(ui, *current == value, atoms);
    if r.clicked() && *current != value {
        *current = value;
        r.mark_changed();
    }
    r
}

/// egui's [`ComboBox`] with the kit's arrow ([`paint_combo_arrow`]):
/// `ComboBox::from_id_salt`.
pub fn combo_box(id_salt: impl AsIdSalt) -> ComboBox {
    ComboBox::from_id_salt(id_salt).icon(paint_combo_arrow)
}

/// egui's [`ComboBox`] with a label and the kit's arrow:
/// `ComboBox::new`.
pub fn combo_box_labelled(id_salt: impl AsIdSalt, label: impl Into<WidgetText>) -> ComboBox {
    ComboBox::new(id_salt, label).icon(paint_combo_arrow)
}

/// Paints a combo box's arrow ([`ComboBox::icon`]): the kit's
/// `combo-arrow` box over `rect`, `selected` while the list is open, then
/// the `dropdown` glyph in its `text` (the default icon with egui's own
/// geometry, a triangle as wide as egui's).
pub fn paint_combo_arrow(ui: &Ui, rect: Rect, visuals: &WidgetVisuals, open: bool) {
    let kit = Kit::of(ui.ctx());
    let arrow = &kit.combo_arrow;
    let w = &ui.visuals().widgets;
    let state = State::REST
        .with(State::SELECTED, open)
        .with(State::ACTIVE, !open && *visuals == w.active)
        .with(State::HOVER, !open && *visuals == w.hovered)
        .with(State::DISABLED, !ui.is_enabled());
    // egui gives the rectangle grown by its `expansion` already.
    paint_box(ui, arrow, rect, state);
    let color = text_color(ui, arrow, state);
    let icon = kit.glyph(Glyph::Dropdown);
    if icon == Glyph::Dropdown.default_icon() {
        let r = Rect::from_center_size(
            rect.center(),
            vec2(rect.width() * 0.7, rect.height() * 0.45),
        );
        ui.painter().add(Shape::convex_polygon(
            vec![r.left_top(), r.right_top(), r.center_bottom()],
            color,
            Stroke::NONE,
        ));
    } else {
        icon.paint(ui.painter(), rect, color);
    }
}
