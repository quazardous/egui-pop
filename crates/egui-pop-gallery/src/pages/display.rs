//! The display page: what a window draws for when the application wants
//! wide gamut or HDR, what each surface carries, and the transfers'
//! curves.

use egui::{Color32, Sense, Stroke, Ui, pos2, vec2};
use egui_pop::theme::Tokens;
use egui_pop::viewport::display::{
    self, ColorSpace, DisplayTarget, DisplayTransform, Linear, LinearSrgb, Primaries,
    SurfaceFormat, TargetSource, Transfer,
};

use super::section;

/// What the page asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wanted {
    #[default]
    Srgb,
    DisplayP3,
    Pq,
    Hlg,
}

impl Wanted {
    const ALL: [(Wanted, &'static str); 4] = [
        (Wanted::Srgb, "sRGB"),
        (Wanted::DisplayP3, "Display P3"),
        (Wanted::Pq, "HDR (PQ, headroom 4)"),
        (Wanted::Hlg, "HDR (HLG)"),
    ];

    fn target(self) -> DisplayTarget {
        match self {
            Wanted::Srgb => DisplayTarget::srgb(),
            Wanted::DisplayP3 => DisplayTarget::display_p3(),
            Wanted::Pq => DisplayTarget::hdr_pq(4.0),
            Wanted::Hlg => DisplayTarget::hdr_hlg(),
        }
    }
}

/// The page's state.
#[derive(Debug, Default)]
pub struct DisplayState {
    pub wanted: Wanted,
}

/// Surfaces a window may have, as a host could configure them.
fn surfaces() -> [(&'static str, DisplayTarget); 5] {
    let c = |space, format, headroom| {
        DisplayTarget::for_color_space(space, format, headroom, TargetSource::Wsi)
    };
    [
        (
            "8-bit sRGB (eframe)",
            c(ColorSpace::Srgb, SurfaceFormat::Rgba8Unorm, 1.0),
        ),
        (
            "Extended sRGB, half floats",
            c(ColorSpace::ExtendedSrgb, SurfaceFormat::Rgba16Float, 4.0),
        ),
        (
            "scRGB, half floats",
            c(
                ColorSpace::ExtendedSrgbLinear,
                SurfaceFormat::Rgba16Float,
                4.0,
            ),
        ),
        (
            "HDR10 (PQ), 10 bits",
            c(ColorSpace::Bt2100Pq, SurfaceFormat::Rgb10a2Unorm, 10.0),
        ),
        (
            "HLG, 10 bits",
            c(ColorSpace::Bt2100Hlg, SurfaceFormat::Rgb10a2Unorm, 4.0),
        ),
    ]
}

pub fn show(ui: &mut Ui, s: &mut DisplayState) {
    let t = Tokens::of(ui.ctx());
    let ctx = ui.ctx().clone();
    ui.label("An application wants a target for a window; the viewport draws for what the window's surface carries, and says why when that is less.");
    section(ui, "This window");
    ui.horizontal_wrapped(|ui| {
        ui.label("Wanted:");
        for (w, name) in Wanted::ALL {
            if ui.selectable_label(s.wanted == w, name).clicked() {
                s.wanted = w;
            }
        }
    });
    let wanted = s.wanted.target();
    if display::wanted(&ctx) != wanted {
        display::want(&ctx, ctx.viewport_id(), wanted);
    }
    let r = display::resolution(&ctx);
    egui::Grid::new("gallery.display.window")
        .num_columns(2)
        .spacing([t.space.l, t.space.xs])
        .show(ui, |ui| {
            ui.label("Surface");
            ui.monospace(r.surface.describe());
            ui.end_row();
            ui.label("Drawn");
            ui.monospace(r.drawn.describe());
            ui.end_row();
            ui.label("Faithful");
            ui.monospace(if r.faithful() { "yes" } else { "no" });
            ui.end_row();
        });
    if let Some(reason) = r.reason() {
        ui.colored_label(t.status.warning, reason);
    }

    section(ui, "Surfaces");
    ui.label(
        "What the same wish gives on each surface (GPU path), and Display P3 red encoded for it.",
    );
    egui::Grid::new("gallery.display.surfaces")
        .num_columns(3)
        .striped(true)
        .spacing([t.space.l, t.space.xs])
        .show(ui, |ui| {
            ui.strong("Surface");
            ui.strong("P3 red");
            ui.strong("Lost");
            ui.end_row();
            for (name, surface) in surfaces() {
                let r = display::resolve(&wanted, &surface, true);
                ui.label(name).on_hover_text(r.drawn.describe());
                let red = Linear(Primaries::DisplayP3)
                    .stage(&r.drawn)
                    .encode([1.0, 0.0, 0.0]);
                ui.monospace(format!("{:.2} {:.2} {:.2}", red[0], red[1], red[2]));
                ui.label(r.drawn.missing(&wanted).unwrap_or_else(|| "nothing".into()));
                ui.end_row();
            }
        });

    section(ui, "Transfers");
    ui.label("Signal against linear light, SDR white at 1.0 (PQ with white at 203 nits, HLG with white at 75 %).");
    curves(ui);
    let white = LinearSrgb.stage(&surfaces()[3].1).encode([1.0; 3]);
    ui.monospace(format!(
        "PQ: SDR white {:.4}, 1000 nits {:.4}; HLG: SDR white {:.3}",
        white[0],
        display::pq_encode(1000.0),
        Transfer::Hlg.encode(1.0)
    ));
}

/// The transfers' curves over 0..4 times SDR white.
fn curves(ui: &mut Ui) {
    let t = Tokens::of(ui.ctx());
    let size = vec2(ui.available_width().min(420.0), 160.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, t.border.default),
        egui::StrokeKind::Inside,
    );
    let max = 4.0;
    let at = |v: f32, e: f32| {
        pos2(
            rect.left() + v / max * rect.width(),
            rect.bottom() - e.clamp(0.0, 1.0) * rect.height(),
        )
    };
    // SDR white.
    p.line_segment(
        [at(1.0, 0.0), at(1.0, 1.0)],
        Stroke::new(1.0, t.border.default),
    );
    let lines: [(Transfer, Color32); 4] = [
        (Transfer::SrgbPiecewise, t.text.muted),
        (Transfer::ExtSrgb, t.accent.default),
        (Transfer::Pq, t.status.warning),
        (Transfer::Hlg, t.status.success),
    ];
    for (tr, colour) in lines {
        let points: Vec<_> = (0..=200)
            .map(|i| {
                let v = i as f32 / 200.0 * max;
                // The extended curve is drawn at half scale, to fit.
                let e = match tr {
                    Transfer::ExtSrgb => tr.encode(v) / 2.0,
                    _ => tr.encode(v),
                };
                at(v, e)
            })
            .collect();
        p.add(egui::Shape::line(points, Stroke::new(1.5, colour)));
    }
    ui.horizontal_wrapped(|ui| {
        for (tr, colour) in lines {
            super::chip(ui, colour);
            ui.label(match tr {
                Transfer::ExtSrgb => "extended sRGB (half scale)",
                other => other.name(),
            });
        }
    });
}
