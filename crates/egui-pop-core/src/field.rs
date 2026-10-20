//! A field's state drawn, announced and reported: the egui side of
//! [`egui_pop_model::field`].
//!
//! Every input shows its [`FieldState`] the same way, the decorated
//! input ([`crate::input`]) and the sliders alike:
//!
//! - **an edge**: a strip along the field's leading side in the colour
//!   of its first state ([`FieldState::edge`]: disabled, invalid,
//!   pending, modified), from the `field.*` tokens;
//! - **a marker icon** per state, so the state never rests on colour
//!   alone: a dot (modified), an asterisk (required), a crossed circle
//!   (invalid), a clock (pending), a lock (read-only), an "i" (disabled,
//!   whose reason is its tooltip);
//! - **assistive technologies** get it too: required, invalid (with the
//!   message as the description), disabled (with the reason), read-only,
//!   busy (pending);
//! - **agents** read every field drawn in the last pass, with its
//!   state and its disabled reason, through [`describe`] (and the debug
//!   bridge's `fields` collector).
//!
//! ```
//! use egui_pop_core::field;
//! use egui_pop_model::field::FieldState;
//!
//! fn exposure_row(ui: &mut egui::Ui) {
//!     let state = FieldState::new().disabled("Needs an open image");
//!     let r = ui.add_enabled(false, egui::Button::new("Pick"));
//!     field::hover(ui, &r, &state);
//!     field::access(ui.ctx(), r.id, &state);
//!     field::record(ui.ctx(), r.id, "Pick", "button", &state);
//! }
//! ```

use egui::{Align2, Color32, Context, CornerRadius, FontId, Id, Rect, Response, Ui, pos2, vec2};
use serde::Serialize;
use serde_json::{Value as Json, json};

pub use egui_pop_model::field::{Baseline, FieldState, Mark, Modified};

use crate::icon::Icon;
use crate::kit::{Kit, State, Tone};
use crate::state::{lock, pass, shared};
use crate::strings::{self, FIELD_DISABLED, FIELD_MODIFIED_DEFAULT, FIELD_MODIFIED_SAVED};
use crate::strings::{FIELD_PENDING, FIELD_REQUIRED, READ_ONLY};
use crate::theme::Tokens;

/// The colour of a mark, from the kit's `field`: the muted tone's text
/// for a disabled or read-only field, else the border colour of the
/// mark's tone (`danger` invalid, `info` pending, `accent` modified,
/// `warning` required).
pub fn color(kit: &Kit, mark: Mark) -> Color32 {
    let tone = |tone| kit.field.style_toned(tone, State::REST);
    match mark {
        Mark::Disabled | Mark::ReadOnly => tone(Tone::Muted).text,
        Mark::Invalid => tone(Tone::Danger).border_color,
        Mark::Pending => tone(Tone::Info).border_color,
        Mark::Modified => tone(Tone::Accent).border_color,
        Mark::Required => tone(Tone::Warning).border_color,
    }
}

/// The icon of a mark; the required mark is an asterisk, painted as
/// text ([`paint_marker`]).
pub fn icon(mark: Mark) -> Option<Icon> {
    match mark {
        Mark::Disabled => Some(Icon::Info),
        Mark::Invalid => Some(Icon::Error),
        Mark::Pending => Some(Icon::History),
        Mark::Modified => Some(Icon::Dot),
        Mark::ReadOnly => Some(Icon::Lock),
        Mark::Required => None,
    }
}

/// What a mark says, in the interface's language (the disabled and
/// invalid marks say their reason and message).
pub fn text(ctx: &Context, state: &FieldState, mark: Mark) -> String {
    match mark {
        Mark::Disabled => strings::tr_args(
            ctx,
            &FIELD_DISABLED,
            &[("reason", state.disabled_reason().unwrap_or_default())],
        ),
        Mark::Invalid => state.message().unwrap_or_default().to_string(),
        Mark::Pending => strings::tr(ctx, &FIELD_PENDING),
        Mark::Modified => match state.modified.map(|m| m.against) {
            Some(Baseline::Saved) => strings::tr(ctx, &FIELD_MODIFIED_SAVED),
            _ => strings::tr(ctx, &FIELD_MODIFIED_DEFAULT),
        },
        Mark::Required => strings::tr(ctx, &FIELD_REQUIRED),
        Mark::ReadOnly => strings::tr(ctx, &READ_ONLY),
    }
}

/// The width of a field's edge strip.
pub fn edge_width(t: &Tokens) -> f32 {
    (t.space.xs * 0.75).max(2.0)
}

/// Paints the edge along a field's leading side, in its first state's
/// colour; an invalid or pending field is outlined in it too.
pub fn paint_edge(ui: &Ui, rect: Rect, state: &FieldState) {
    let Some(mark) = state.edge() else {
        return;
    };
    if !ui.is_rect_visible(rect) {
        return;
    }
    let t = Tokens::of(ui.ctx());
    let c = color(&Kit::of(ui.ctx()), mark);
    let radius = t.radius.widget.round().clamp(0.0, 255.0) as u8;
    let w = edge_width(&t);
    let strip = Rect::from_min_max(rect.min, pos2(rect.left() + w, rect.bottom()));
    ui.painter().rect_filled(
        strip,
        CornerRadius {
            nw: radius,
            sw: radius,
            ne: 0,
            se: 0,
        },
        c,
    );
    if matches!(mark, Mark::Invalid | Mark::Pending) {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(radius),
            egui::Stroke::new(1.0, c),
            egui::StrokeKind::Inside,
        );
    }
}

/// Paints one mark's marker in a square.
pub fn paint_marker(ui: &Ui, rect: Rect, mark: Mark) {
    let c = color(&Kit::of(ui.ctx()), mark);
    match icon(mark) {
        Some(i) => i.paint(ui.painter(), rect, c),
        None => {
            ui.painter().text(
                rect.center() + vec2(0.0, rect.height() * 0.15),
                Align2::CENTER_CENTER,
                "*",
                FontId::proportional(rect.height() * 1.3),
                c,
            );
        }
    }
}

/// The marks a field shows as markers, in order: every mark it has.
pub fn markers(state: &FieldState) -> Vec<Mark> {
    state.marks()
}

/// The width the markers of a state take, `side` points each.
pub fn markers_width(state: &FieldState, side: f32, gap: f32) -> f32 {
    match markers(state).len() {
        0 => 0.0,
        n => n as f32 * side + (n - 1) as f32 * gap,
    }
}

/// Paints a state's markers from `at` (the left of the first), centred
/// on `y`; returns where they end.
pub fn paint_markers(ui: &Ui, at: egui::Pos2, side: f32, gap: f32, state: &FieldState) -> f32 {
    let mut x = at.x;
    for m in markers(state) {
        let r = Rect::from_min_size(pos2(x, at.y - side / 2.0), vec2(side, side));
        paint_marker(ui, r, m);
        x += side + gap;
    }
    x - gap
}

/// The tooltip a field's state adds: the disabled reason (shown while
/// disabled), the invalid message, "being checked". Returns the
/// response with them.
pub fn hover(ui: &Ui, response: &Response, state: &FieldState) -> Response {
    let mut r = response.clone();
    let ctx = ui.ctx();
    if state.is_disabled() {
        r = r.on_disabled_hover_text(text(ctx, state, Mark::Disabled));
    }
    if state.has(Mark::Invalid) {
        r = r.on_hover_text(text(ctx, state, Mark::Invalid));
    }
    if state.pending {
        r = r.on_hover_text(text(ctx, state, Mark::Pending));
    }
    r
}

/// Gives a field's state to assistive technologies: required, invalid
/// (its message as the description), disabled (its reason as the
/// description), read-only, busy while pending. A modified field says so
/// in its description.
pub fn access(ctx: &Context, id: Id, state: &FieldState) {
    if state.is_plain() {
        return;
    }
    let mut said: Vec<String> = Vec::new();
    for m in state.marks() {
        if m != Mark::Required && m != Mark::ReadOnly {
            said.push(text(ctx, state, m));
        }
    }
    let state = state.clone();
    ctx.accesskit_node_builder(id, move |node| {
        use egui::accesskit::Invalid;
        if state.required {
            node.set_required();
        }
        if state.invalid.is_some() {
            node.set_invalid(Invalid::True);
        }
        if state.is_disabled() {
            node.set_disabled();
        }
        if state.read_only {
            node.set_read_only();
        }
        if state.pending {
            node.set_busy();
        }
        if !said.is_empty() {
            node.set_description(said.join(". "));
        }
    });
}

/// One field as drawn, for agents.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FieldRecord {
    /// Its widget id, as the focus collector writes ids.
    pub id: String,
    pub label: String,
    /// What it edits: `text`, `number`, `path`, `search`, `slider`...
    pub kind: String,
    pub state: FieldState,
}

#[derive(Debug, Default)]
struct Fields {
    pass: u64,
    building: Vec<FieldRecord>,
    last: Vec<FieldRecord>,
}

fn rotate(ctx: &Context, s: &mut Fields) {
    let now = pass(ctx);
    if s.pass != now {
        s.pass = now;
        s.last = std::mem::take(&mut s.building);
    }
}

/// Records a field drawn this pass (its widget calls it), so agents see
/// it in [`fields`] and [`describe`] from the next pass.
pub fn record(ctx: &Context, id: Id, label: &str, kind: &str, state: &FieldState) {
    let st = shared::<Fields>(ctx, "fields");
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.building.push(FieldRecord {
        id: format!("{id:?}"),
        label: label.into(),
        kind: kind.into(),
        state: state.clone(),
    });
}

/// The fields drawn in the last complete pass.
pub fn fields(ctx: &Context) -> Vec<FieldRecord> {
    let st = shared::<Fields>(ctx, "fields");
    let mut s = lock(&st);
    rotate(ctx, &mut s);
    s.last.clone()
}

/// The fields drawn in the last pass as agents read them: each one's
/// label, kind, marks, whether it can be focused and edited, and the
/// disabled reason and invalid message.
pub fn describe(ctx: &Context) -> Json {
    Json::Array(
        fields(ctx)
            .into_iter()
            .map(|f| {
                let mut d = f.state.describe();
                if let Some(o) = d.as_object_mut() {
                    o.insert("id".into(), json!(f.id));
                    o.insert("label".into(), json!(f.label));
                    o.insert("kind".into(), json!(f.kind));
                }
                d
            })
            .collect(),
    )
}
