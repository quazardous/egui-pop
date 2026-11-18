//! Colour ramps on sliders and knobs, headless: the value's fill (a
//! knob's arc and pointer) in the ramp's colour at the value, the track
//! mode drawing the ramp across the track instead, a theme switch
//! followed, and a high-contrast theme keeping the theme's own fill.
//! The value stays announced whatever the colour.

use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Context, RawInput, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::knob::Knob;
use egui_pop_core::slider::{Ramp, Slider};
use egui_pop_core::theme::{self, ThemeSpec, Tokens};

const BLUE: Color32 = Color32::from_rgb(40, 110, 255);
const AMBER: Color32 = Color32::from_rgb(255, 176, 40);

/// Every colour a frame paints: fills and strokes, meshes' vertices.
fn colours(shapes: &[ClippedShape]) -> Vec<Color32> {
    fn walk(s: &Shape, out: &mut Vec<Color32>) {
        match s {
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            Shape::Rect(r) => out.push(r.fill),
            Shape::Path(p) => {
                if let egui::epaint::ColorMode::Solid(c) = p.stroke.color {
                    out.push(c);
                }
            }
            Shape::LineSegment { stroke, .. } => out.push(stroke.color),
            Shape::Mesh(m) => out.extend(m.vertices.iter().map(|v| v.color)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in shapes {
        walk(&c.shape, &mut out);
    }
    out
}

/// What `theme` paints for `add` (a slider or a knob over 0..=1 at `v`).
fn paint(theme_name: &str, add: impl Fn(&mut egui::Ui)) -> Vec<Color32> {
    let ctx = Context::default();
    theme::install(&ctx, &ThemeSpec::new(theme_name));
    let input = || RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            vec2(400.0, 200.0),
        )),
        ..RawInput::default()
    };
    let run = |ctx: &Context| {
        let mut out = ctx.run_ui(input(), |ui| add(ui));
        out.textures_delta.clear();
        out
    };
    // A first pass lays out; the second paints in place.
    let _ = run(&ctx);
    colours(&run(&ctx).shapes)
}

fn tokens(name: &str) -> Tokens {
    Tokens::new(egui_pop_core::theme::Themes::default().resolve(&ThemeSpec::new(name)))
}

fn slider(v: f64, track: bool) -> impl Fn(&mut egui::Ui) {
    move |ui| {
        let mut v = v;
        let s = Slider::new(&mut v, "Temperature", 0.0..=1.0)
            .ramp(BLUE, AMBER)
            .size(vec2(300.0, 24.0));
        let s = if track { s.ramp_track() } else { s };
        s.show(ui);
    }
}

#[test]
fn the_fill_takes_the_ramp_colour_at_0_half_and_1() {
    let t = tokens("pop-dark");
    let ramp = Ramp::new(BLUE, AMBER);
    for v in [0.0, 0.5, 1.0] {
        let want = ramp.color_at(v as f32, &t);
        let painted = paint("pop-dark", slider(v, false));
        // Stood 3:1 against the track, the colour may be lightened: the
        // closest painted colour is the ramp's, hue kept.
        let near = painted.iter().any(|c| {
            let d = |a: u8, b: u8| i32::from(a).abs_diff(i32::from(b));
            *c == want
                || (d(c.r(), want.r()) < 40
                    && d(c.g(), want.g()) < 40
                    && d(c.b(), want.b()) < 40
                    && c.a() == 255)
        });
        assert!(near, "{v}: {want:?} not in {painted:?}");
    }
    // Blue at the start, amber at the end: the two never swap.
    let start = paint("pop-dark", slider(0.0, false));
    assert!(!start.contains(&AMBER));
}

#[test]
fn the_track_mode_draws_the_whole_ramp() {
    // At the start the value mode paints no amber; the track mode
    // paints the ramp to its amber end whatever the value.
    let painted = paint("pop-dark", slider(0.0, true));
    assert!(painted.contains(&AMBER), "{painted:?}");
    assert!(painted.contains(&BLUE), "{painted:?}");
}

#[test]
fn a_token_ramp_follows_a_theme_switch() {
    let add = |ui: &mut egui::Ui| {
        let mut v = 1.0;
        Slider::new(&mut v, "Clipping", 0.0..=1.0)
            .ramp("text.muted", "danger")
            .ramp_track()
            .size(vec2(300.0, 24.0))
            .show(ui);
    };
    for name in ["pop-dark", "pop-light"] {
        let danger = tokens(name).status.danger;
        assert!(paint(name, add).contains(&danger), "{name}");
    }
    assert_ne!(
        tokens("pop-dark").status.danger,
        tokens("pop-light").status.danger
    );
}

#[test]
fn a_high_contrast_theme_keeps_its_own_fill() {
    for track in [false, true] {
        let painted = paint("pop-high-contrast", slider(1.0, track));
        assert!(!painted.contains(&AMBER), "track {track}: {painted:?}");
        let knob = paint("pop-high-contrast", |ui| {
            let mut v = 1.0;
            let k = Knob::new(&mut v, "Gain", 0.0..=1.0).ramp(BLUE, AMBER);
            let k = if track { k.ramp_track() } else { k };
            k.size(vec2(80.0, 80.0)).show(ui);
        });
        assert!(!knob.contains(&AMBER), "knob, track {track}");
    }
}

#[test]
fn a_knob_draws_its_arc_and_pointer_in_the_ramp() {
    let t = tokens("pop-dark");
    let painted = paint("pop-dark", |ui| {
        let mut v = 1.0;
        Knob::new(&mut v, "Gain", 0.0..=1.0)
            .ramp(BLUE, AMBER)
            .size(vec2(80.0, 80.0))
            .show(ui);
    });
    let want = Ramp::new(BLUE, AMBER).color_at(1.0, &t);
    assert!(
        painted.iter().filter(|c| **c == want).count() >= 2,
        "the arc and the pointer: {painted:?}"
    );
    // The track mode: the ramp along the arc, blue included, at the end.
    let track = paint("pop-dark", |ui| {
        let mut v = 1.0;
        Knob::new(&mut v, "Gain", 0.0..=1.0)
            .ramp(BLUE, AMBER)
            .ramp_track()
            .size(vec2(80.0, 80.0))
            .show(ui);
    });
    assert!(
        track.iter().any(|c| c.b() > 200 && c.r() < 120),
        "{track:?}"
    );
}

#[test]
fn the_value_is_still_announced() {
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 80.0))
        .build_ui_state(
            |ui, v: &mut f64| {
                Slider::new(v, "Temperature", 2000.0..=12_000.0)
                    .unit("K")
                    .integer()
                    .ramp(BLUE, AMBER)
                    .show(ui);
            },
            6500.0,
        );
    h.run();
    let node = h.get_by_label("Temperature");
    assert_eq!(node.accesskit_node().numeric_value(), Some(6500.0));
    let text = node.accesskit_node().value().unwrap_or_default();
    assert!(text.contains("6500") || text.contains("6,500"), "{text}");
}
