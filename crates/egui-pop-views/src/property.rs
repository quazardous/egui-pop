//! Property rows: a name and a value, their name column aligned.
//!
//! A [`PropertyRow`] is a [`ListItem`](crate::item::ListItem)'s row (its
//! height, its box, hovered under the pointer) split in two: the name, in
//! the row's muted tone, and the value, any widget. The name column is as
//! wide as the widest name of its rows: the rows sharing a column id
//! ([`PropertyRow::column`]; by default, the rows of one `Ui`) line their
//! values up, as a selection panel does. The column never takes more than
//! [`NAME_SHARE`] of the row.
//!
//! A value is editable unless the row is read-only
//! ([`PropertyRow::read_only`]: its widgets are disabled). A plain value
//! is text ([`PropertyRow::text`]); a widget given with
//! [`PropertyRow::widget`] is named by the property for screen readers.
//!
//! ```
//! use egui_pop_views::property::PropertyRow;
//!
//! fn entity(ui: &mut egui::Ui, radius: &mut f32, visible: &mut bool) {
//!     PropertyRow::new("Path").text(ui, "world/points");
//!     PropertyRow::new("Archetype").text(ui, "Points3D");
//!     PropertyRow::new("Radius").widget(ui, egui::DragValue::new(radius).speed(0.1));
//!     PropertyRow::new("Visible").widget(ui, egui::Checkbox::without_text(visible));
//! }
//! # let _ = entity;
//! ```

use egui::{
    Align, AsIdSalt, FontSelection, Id, InnerResponse, Layout, Rect, Response, Role, Sense, Shape,
    TextStyle, Ui, UiBuilder, Widget, WidgetInfo, WidgetText, pos2, vec2,
};
use egui_pop_core::kit::{Kit, Part, State, Tone};
use egui_pop_core::theme::Tokens;

use crate::item::row_height;

/// The most of a row's width its name column takes.
pub const NAME_SHARE: f32 = 0.5;

/// A name and a value: see the [module](self).
#[derive(Clone, Debug)]
#[must_use]
pub struct PropertyRow {
    name: String,
    column: Option<Id>,
    depth: usize,
    read_only: bool,
    height: Option<f32>,
}

impl PropertyRow {
    /// A row for the property `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            column: None,
            depth: 0,
            read_only: false,
            height: None,
        }
    }

    /// The rows whose names line up: those given the same id (default:
    /// the rows of one `Ui`).
    pub fn column(mut self, id: impl AsIdSalt) -> Self {
        self.column = Some(Id::unique(id));
        self
    }

    /// Nested under another property: its name indented one step per
    /// level (the value column stays where it is).
    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    /// The value cannot be changed: its widgets are disabled.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Its least height (default: [`row_height`]); a taller value grows
    /// it.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// A plain value: text, selectable for copying.
    pub fn text(self, ui: &mut Ui, value: impl Into<WidgetText>) -> Response {
        let value = value.into();
        self.show(ui, |ui| {
            ui.add(egui::Label::new(value).truncate().selectable(true))
        })
        .inner
    }

    /// A widget as the value, named by the property for screen readers.
    pub fn widget(self, ui: &mut Ui, widget: impl Widget) -> Response {
        let out = self.show(ui, |ui| ui.add(widget));
        out.inner.labelled_by(name_id(out.response.id))
    }

    /// The value drawn by `add`, left to right in the value column. The
    /// response is the row's; its name's accessible node is
    /// `response.id.with("name")`, for `Response::labelled_by`.
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let kit = Kit::of(ui.ctx());
        let t = Tokens::of(ui.ctx());
        let h = self.height.unwrap_or_else(|| row_height(ui));
        let id = ui.next_auto_id();
        let width = ui.available_width();
        let top = ui.cursor().min;
        let indent = ui.spacing().indent;
        let pad = ui.spacing().item_spacing.x;
        let font = FontSelection::Style(TextStyle::Body).resolve(ui.style());
        let name = ui
            .painter()
            .layout_no_wrap(self.name.clone(), font, egui::Color32::PLACEHOLDER);
        let start = self.depth as f32 * indent + pad;
        let column = self
            .column
            .unwrap_or_else(|| ui.scope_id().with("property_column"));
        let split = name_column(
            ui,
            column,
            start + name.size().x + t.space.l,
            width * NAME_SHARE,
        );

        // The box goes under the name and the value once the row's height
        // is known.
        let slot = ui.painter().add(Shape::Noop);
        let value_rect = Rect::from_min_max(
            pos2(top.x + split, top.y),
            pos2(top.x + width - pad, top.y + h),
        );
        let mut value = ui.new_child(
            UiBuilder::new()
                .id_salt(id.with("value"))
                .max_rect(value_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        if self.read_only {
            value.disable();
        }
        let inner = add(&mut value);
        let height = h.max(value.min_rect().height());
        let rect = Rect::from_min_size(top, vec2(width, height));
        let response = ui.allocate_rect(rect, Sense::hover());
        let state = State::REST.with(State::HOVER, response.contains_pointer());
        kit.list_item.paint_under(
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
        let color = kit.list_item.style_toned(Tone::Muted, state).text;
        let name_rect = Rect::from_min_max(
            pos2(top.x + start, top.y),
            pos2((top.x + split - pad).max(top.x + start), top.y + h),
        );
        let at = pos2(name_rect.left(), name_rect.center().y - name.size().y / 2.0);
        ui.painter()
            .with_clip_rect(name_rect.intersect(ui.clip_rect()))
            .galley(at, name, color);
        let named = ui.interact(name_rect, name_id(response.id), Sense::hover());
        let text = self.name;
        named.widget_info(move || WidgetInfo::labeled(Role::Label, true, &text));
        InnerResponse { inner, response }
    }
}

/// The accessible node of a row's name.
fn name_id(row: Id) -> Id {
    row.with("name")
}

/// Where the value column starts for the rows of `column`: the widest
/// name they asked for (`wanted`), at most `most`. The widest of the last
/// pass is used, so names shrink a pass later; a name wider than any
/// before discards the pass, and every row is drawn again with it.
fn name_column(ui: &Ui, column: Id, wanted: f32, most: f32) -> f32 {
    #[derive(Clone, Copy, Default)]
    struct Widest {
        pass: u64,
        last: f32,
        now: f32,
    }
    let pass = ui.ctx().cumulative_pass_nr();
    let id = column.with("egui_pop_views::property");
    let (widest, grew) = ui.ctx().data_mut(|d| {
        let w = d.get_temp_mut_or_default::<Widest>(id);
        if w.pass != pass {
            w.last = w.now;
            w.now = 0.0;
            w.pass = pass;
        }
        w.now = w.now.max(wanted);
        let grew = wanted > w.last;
        if grew {
            w.last = wanted;
        }
        (w.last, grew)
    });
    if grew {
        ui.ctx()
            .request_discard("a property name wider than before");
    }
    widest.min(most).max(0.0)
}
