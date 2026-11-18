//! The text helpers of `UiPop` beside the roles (`egui_pop_core::ui`): code
//! in the monospace style and the body's colour, text in a tone's colour
//! from the tokens, inline emphasis at the body's size in a heavier weight,
//! a divider across the layout in `border.default`. Each label is named by
//! its words for assistive technologies.

use egui::accesskit::Role;
use egui::epaint::{Shape, TextShape};
use egui::{Color32, FontFamily, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::theme::{self, ThemeSpec, Tokens};
use egui_pop_core::ui::{Tone, UiPop, tone_color};

#[derive(Default)]
struct Texts {
    rule: Option<Rect>,
    upright: Option<Rect>,
    room: f32,
}

fn texts() -> Harness<'static, Texts> {
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui_state(
            |ui, s: &mut Texts| {
                s.room = ui.available_width();
                ui.code_text("12:00:01 import started");
                ui.text_tone(Tone::Danger, "Failed: disk full");
                ui.text_tone(Tone::Success, "Done");
                ui.text_tone(Tone::Muted, "Skipped");
                ui.strong("Two copies");
                s.rule = Some(ui.divider().rect);
                ui.horizontal(|ui| {
                    ui.label("left");
                    s.upright = Some(ui.divider().rect);
                    ui.label("right");
                });
            },
            Texts::default(),
        );
    theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    h.run();
    h
}

/// Every shape painted on the last frame, nested ones flattened.
fn shapes(h: &Harness<'static, Texts>) -> Vec<Shape> {
    fn walk(shape: &Shape, out: &mut Vec<Shape>) {
        match shape {
            Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, out)),
            other => out.push(other.clone()),
        }
    }
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        walk(&clipped.shape, &mut out);
    }
    out
}

/// The text shapes painted on the last frame.
fn text_shapes(h: &Harness<'static, Texts>) -> Vec<TextShape> {
    shapes(h)
        .into_iter()
        .filter_map(|s| match s {
            Shape::Text(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn shape_of(h: &Harness<'static, Texts>, text: &str) -> TextShape {
    text_shapes(h)
        .into_iter()
        .find(|t| t.galley.text() == text)
        .unwrap_or_else(|| panic!("no text `{text}` painted"))
}

fn colour(t: &TextShape) -> Color32 {
    t.galley.job.sections[0].format.color
}

#[test]
fn each_line_is_a_label_named_by_its_words() {
    let h = texts();
    for name in [
        "12:00:01 import started",
        "Failed: disk full",
        "Done",
        "Skipped",
        "Two copies",
    ] {
        assert_eq!(h.get_by_label(name).accesskit_node().role(), Role::Label);
    }
}

#[test]
fn code_is_monospace_in_the_body_colour() {
    let h = texts();
    let t = Tokens::of(&h.ctx);
    let code = shape_of(&h, "12:00:01 import started");
    let format = &code.galley.job.sections[0].format;
    assert_eq!(format.font_id.family, FontFamily::Monospace);
    assert_eq!(format.font_id.size, t.font.mono);
    assert_eq!(colour(&code), t.text_role.body.color);
}

#[test]
fn a_tone_colours_the_text_from_the_tokens() {
    let h = texts();
    let t = Tokens::of(&h.ctx);
    assert_eq!(colour(&shape_of(&h, "Failed: disk full")), t.status.danger);
    assert_eq!(colour(&shape_of(&h, "Done")), t.status.success);
    assert_eq!(colour(&shape_of(&h, "Skipped")), t.text_role.muted.color);
    assert_eq!(tone_color(&t, Tone::Warning), t.status.warning);
    assert_eq!(tone_color(&t, Tone::Neutral), t.text_role.body.color);
    assert_eq!(
        shape_of(&h, "Done").galley.job.sections[0]
            .format
            .font_id
            .size,
        t.text_role.body.size,
        "the body's size"
    );
}

#[test]
fn a_divider_crosses_the_layout_in_the_border_colour() {
    let h = texts();
    let t = Tokens::of(&h.ctx);
    let s = h.state();
    let rule = s.rule.unwrap();
    assert!(
        (rule.width() - s.room).abs() < 0.5,
        "{rule:?} across {}",
        s.room
    );
    assert_eq!(rule.height(), t.space.group);
    let upright = s.upright.unwrap();
    assert_eq!(upright.width(), t.space.group, "upright in a row");
    assert!(upright.height() > 0.0);
    let lines = shapes(&h)
        .iter()
        .filter(
            |s| matches!(s, Shape::LineSegment { stroke, .. } if stroke.color == t.border.default),
        )
        .count();
    assert!(lines >= 2, "both rules painted in border.default: {lines}");
}

#[test]
fn strong_text_is_the_body_s_size_in_a_heavier_weight() {
    let h = texts();
    let t = Tokens::of(&h.ctx);
    let strong = &t.text_role.strong;
    assert_eq!(strong.size, t.text_role.body.size, "the body's size");
    assert!(
        strong.weight > t.text_role.body.weight,
        "heavier than the body"
    );
    let shape = shape_of(&h, "Two copies");
    assert_eq!(
        shape.galley.job.sections[0].format.font_id.size,
        strong.size
    );
    assert_eq!(colour(&shape), strong.color);
}

#[test]
fn strong_text_is_drawn_in_the_weighted_family_once_the_fonts_are_in() {
    // A plain context: the theme's weighted families are in use from the
    // pass after they are installed.
    let ctx = egui::Context::default();
    let t = theme::install(&ctx, &ThemeSpec::new("pop-dark"));
    let mut family = None;
    for _ in 0..2 {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut job = egui::text::LayoutJob::default();
            egui_pop_core::ui::strong_text(ui, "Two copies").append_to(
                &mut job,
                ui.style(),
                egui::FontSelection::Default,
                egui::Align::Center,
            );
            family = job
                .sections
                .first()
                .map(|s| s.format.font_id.family.clone());
        });
        out.textures_delta.clear();
    }
    assert_eq!(
        family,
        Some(egui_pop_core::fonts::family(
            egui_pop_core::fonts::Role::Ui,
            t.text_role.strong.weight
        ))
    );
}
