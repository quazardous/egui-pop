//! A slider's gradient track: colours along the bar, so the bar shows
//! what its value does (a hue, a white balance's temperature).
//!
//! The colours are the content, not the theme's: they are drawn as a
//! strip along the bottom of the bar, under the thumb, and the label and
//! the value stay on the track's colour.

use egui::epaint::{Mesh, Shape};
use egui::{Color32, Painter, Rect, pos2};

/// Colours at places along the bar, from 0 (the soft start) to 1 (its
/// end), blended in between.
///
/// ```
/// use egui::Color32;
/// use egui_pop_core::slider::Gradient;
///
/// let g = Gradient::new([(0.0, Color32::BLACK), (1.0, Color32::WHITE)]);
/// assert_eq!(g.color_at(0.5), Color32::from_gray(128));
/// assert_eq!(Gradient::hue().color_at(0.0), Gradient::hue().color_at(1.0));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    stops: Vec<(f32, Color32)>,
}

impl Gradient {
    /// From stops `(place, colour)`, places in `[0, 1]` (sorted here; the
    /// first and last colours run to the ends).
    pub fn new(stops: impl IntoIterator<Item = (f32, Color32)>) -> Self {
        let mut stops: Vec<(f32, Color32)> = stops
            .into_iter()
            .filter(|(at, _)| at.is_finite())
            .map(|(at, c)| (at.clamp(0.0, 1.0), c))
            .collect();
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        if stops.is_empty() {
            stops.push((0.0, Color32::TRANSPARENT));
        }
        if let Some(&(at, c)) = stops.first()
            && at > 0.0
        {
            stops.insert(0, (0.0, c));
        }
        if let Some(&(at, c)) = stops.last()
            && at < 1.0
        {
            stops.push((1.0, c));
        }
        Self { stops }
    }

    /// Colours spread evenly along the bar.
    pub fn even(colors: &[Color32]) -> Self {
        let last = colors.len().saturating_sub(1).max(1) as f32;
        Self::new(
            colors
                .iter()
                .enumerate()
                .map(|(i, c)| (i as f32 / last, *c)),
        )
    }

    /// `samples` colours of `f` over `[0, 1]` (at least two).
    pub fn sampled(samples: usize, f: impl Fn(f32) -> Color32) -> Self {
        let n = samples.max(2);
        Self::new((0..n).map(|i| {
            let at = i as f32 / (n - 1) as f32;
            (at, f(at))
        }))
    }

    /// The hue circle, red to red: for a hue in degrees over `0..=360`.
    pub fn hue() -> Self {
        Self::sampled(13, |at| {
            egui::ecolor::Hsva::new(at.rem_euclid(1.0), 0.75, 0.9, 1.0).into()
        })
    }

    /// The colour of light from `from` to `to` kelvin (a white balance's
    /// illuminant: warm at low temperatures, blue at high ones), linear
    /// in kelvin. [`Gradient::reversed`] shows the correction instead.
    pub fn temperature(from: f32, to: f32) -> Self {
        Self::sampled(16, |at| kelvin(from + (to - from) * at))
    }

    /// The same colours from the other end.
    pub fn reversed(mut self) -> Self {
        self.stops.reverse();
        for s in &mut self.stops {
            s.0 = 1.0 - s.0;
        }
        self
    }

    /// The colour at a place along the bar.
    pub fn color_at(&self, at: f32) -> Color32 {
        let at = at.clamp(0.0, 1.0);
        let next = self
            .stops
            .iter()
            .position(|(p, _)| *p >= at)
            .unwrap_or(self.stops.len() - 1);
        if next == 0 {
            return self.stops[0].1;
        }
        let (a, ca) = self.stops[next - 1];
        let (b, cb) = self.stops[next];
        let t = if b > a { (at - a) / (b - a) } else { 1.0 };
        lerp(ca, cb, t)
    }

    /// Paints the gradient over `rect`, left to right.
    pub(crate) fn paint(&self, painter: &Painter, rect: Rect) {
        let mut mesh = Mesh::default();
        for (i, (at, c)) in self.stops.iter().enumerate() {
            let x = rect.left() + rect.width() * at;
            mesh.colored_vertex(pos2(x, rect.top()), *c);
            mesh.colored_vertex(pos2(x, rect.bottom()), *c);
            if i > 0 {
                let b = 2 * i as u32;
                mesh.add_triangle(b - 2, b - 1, b);
                mesh.add_triangle(b - 1, b, b + 1);
            }
        }
        painter.add(Shape::mesh(mesh));
    }
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

/// The sRGB colour of a black body at `k` kelvin, normalised to its
/// brightest channel: a fit of the Planckian locus good to a few percent
/// from 1000 K to 40 000 K, enough for a track.
fn kelvin(k: f32) -> Color32 {
    let t = (k.clamp(1000.0, 40_000.0)) / 100.0;
    let red = if t <= 66.0 {
        255.0
    } else {
        329.7 * (t - 60.0).powf(-0.1332)
    };
    let green = if t <= 66.0 {
        99.47 * t.ln() - 161.12
    } else {
        288.12 * (t - 60.0).powf(-0.0755)
    };
    let blue = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.52 * (t - 10.0).ln() - 305.04
    };
    let c = |v: f32| v.clamp(0.0, 255.0).round() as u8;
    Color32::from_rgb(c(red), c(green), c(blue))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_are_sorted_and_run_to_the_ends() {
        let g = Gradient::new([(0.75, Color32::WHITE), (0.25, Color32::BLACK)]);
        assert_eq!(g.color_at(0.0), Color32::BLACK);
        assert_eq!(g.color_at(1.0), Color32::WHITE);
        assert_eq!(g.color_at(0.5), Color32::from_gray(128));
        let r = g.reversed();
        assert_eq!(r.color_at(0.0), Color32::WHITE);
    }

    #[test]
    fn temperature_is_warm_then_blue() {
        let g = Gradient::temperature(2000.0, 12_000.0);
        let (warm, cold) = (g.color_at(0.0), g.color_at(1.0));
        assert!(warm.r() > warm.b(), "{warm:?}");
        assert!(cold.b() > cold.r(), "{cold:?}");
        assert_eq!(kelvin(6600.0).r(), 255);
    }
}
