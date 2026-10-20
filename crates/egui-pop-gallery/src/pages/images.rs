//! The image widgets page: thumbnails (an image, or its placeholder
//! until it is there) in a virtualised grid, and a histogram from bins
//! the application computed, with its clipping markers and a drag
//! adjusting an exposure.

use egui::{Color32, ColorImage, TextureHandle, TextureOptions, Ui, vec2};
use egui_pop::core::widgets;
use egui_pop::viewport::demo;
use egui_pop::viewport::histogram::{Channel, Clipping, Histogram};
use egui_pop::viewport::thumbnail::{Thumbnail, thumbnail_grid};

use super::section;

/// How many thumbnails the grid holds.
pub const COUNT: usize = 2000;

/// The page's state.
#[derive(Default)]
pub struct State {
    textures: Vec<TextureHandle>,
    /// Red, green, blue bins of the demo image.
    bins: Option<[Vec<f32>; 3]>,
    pub selected: usize,
    pub log: bool,
    /// What the histogram's drag adjusts, in stops.
    pub exposure: f64,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("textures", &self.textures.len())
            .field("selected", &self.selected)
            .finish()
    }
}

const THUMB: [usize; 2] = [96, 72];

/// A thumbnail of the demo image at a hue shift.
fn thumbnail(shift: f32) -> ColorImage {
    let size = [4096u32, 3072];
    let mut px = Vec::with_capacity(THUMB[0] * THUMB[1]);
    for j in 0..THUMB[1] {
        for i in 0..THUMB[0] {
            let x = (i as f64 + 0.5) / THUMB[0] as f64 * f64::from(size[0]);
            let y = (j as f64 + 0.5) / THUMB[1] as f64 * f64::from(size[1]);
            let c = demo::sample(x, y, size, shift);
            let e = |v: f32| egui::ecolor::gamma_u8_from_linear_f32(v.clamp(0.0, 1.0));
            px.push(Color32::from_rgb(e(c[0]), e(c[1]), e(c[2])));
        }
    }
    ColorImage::new(THUMB, px)
}

/// 64 bins per channel of the demo image, sampled.
fn bins() -> [Vec<f32>; 3] {
    let size = [4096u32, 3072];
    let mut b = [vec![0.0f32; 64], vec![0.0f32; 64], vec![0.0f32; 64]];
    for j in 0..96 {
        for i in 0..128 {
            let c = demo::sample(
                f64::from(i) * 32.0 + 16.0,
                f64::from(j) * 32.0 + 16.0,
                size,
                0.0,
            );
            for k in 0..3 {
                let v = egui::ecolor::gamma_from_linear(c[k].clamp(0.0, 1.0));
                b[k][((v * 63.0).round() as usize).min(63)] += 1.0;
            }
        }
    }
    b
}

/// The page, a plain function over its state.
pub fn show(ui: &mut Ui, state: &mut State) {
    if state.textures.is_empty() {
        state.textures = (0..8)
            .map(|i| {
                let image = thumbnail(i as f32 / 8.0);
                ui.ctx()
                    .load_texture(format!("gallery.images.{i}"), image, TextureOptions::LINEAR)
            })
            .collect();
    }
    let bins = state.bins.get_or_insert_with(bins);
    section(ui, "Histogram (bins supplied by the application)");
    let [r, g, b] = &*bins;
    let clipping = clipping(&[r, g, b]);
    let out = Histogram::new([
        Channel::new("Red", r, Color32::from_rgb(220, 70, 60)),
        Channel::new("Green", g, Color32::from_rgb(80, 190, 90)),
        Channel::new("Blue", b, Color32::from_rgb(70, 120, 230)),
    ])
    .log(state.log)
    .clipping(clipping)
    .draggable(true)
    .size(vec2(ui.available_width().min(480.0), 140.0))
    .show(ui);
    // Four stops per width of the histogram.
    state.exposure += f64::from(out.moved()) * 4.0;
    ui.label(format!(
        "Drag the histogram (or focus it and use the arrows): exposure {:+.2} EV",
        state.exposure
    ));
    section(
        ui,
        &format!("Thumbnails: {COUNT} in a virtualised grid (placeholders where no image yet)"),
    );
    let textures = &state.textures;
    let selected = &mut state.selected;
    thumbnail_grid(ui, COUNT, vec2(96.0, 72.0), |ui, i| {
        let mut t = Thumbnail::new(format!("Image {}", i + 1)).selected(i == *selected);
        // Every third has no image yet: its average colour stands in.
        if i % 3 == 2 {
            t = t.placeholder(placeholder(i));
        } else {
            let tex = &textures[i % textures.len()];
            t = t.image(tex.id(), [THUMB[0] as u32, THUMB[1] as u32]);
        }
        if ui.add(t).clicked() {
            *selected = i;
        }
    });
}

/// The share of samples in the first and the last bin, over every
/// channel: what is clipped.
fn clipping(channels: &[&Vec<f32>]) -> Clipping {
    let total: f32 = channels.iter().flat_map(|c| c.iter()).sum();
    if total <= 0.0 {
        return Clipping::default();
    }
    let first: f32 = channels.iter().filter_map(|c| c.first()).sum();
    let last: f32 = channels.iter().filter_map(|c| c.last()).sum();
    Clipping {
        shadows: first / total,
        highlights: last / total,
    }
}

/// The average colour standing in for a thumbnail not there yet.
fn placeholder(i: usize) -> Color32 {
    let shades = [
        Color32::from_rgb(120, 110, 70),
        Color32::from_rgb(70, 110, 120),
        Color32::from_rgb(110, 80, 110),
    ];
    shades[(i / 3) % shades.len()]
}

/// The playground.
pub fn playground(ui: &mut Ui, state: &mut State) {
    widgets::checkbox(ui, &mut state.log, "Histogram: logarithmic");
    ui.label(format!("Selected: Image {}", state.selected + 1));
}
