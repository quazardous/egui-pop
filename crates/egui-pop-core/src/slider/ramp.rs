//! A colour ramp on a slider or a knob: a colour for the minimum, one
//! for the maximum (and optionally stops between), the value drawn in
//! the colour between them (a white balance's temperature from blue to
//! amber, a saturation from grey to colour, a warning from neutral to
//! red).
//!
//! - **Two modes** ([`RampMode`]): [`RampMode::Value`] (the default)
//!   paints what shows the value (a slider's fill, a knob's arc and
//!   pointer) in the ramp's colour at the value; [`RampMode::Track`]
//!   paints the whole track with the ramp, so the scale reads before a
//!   touch, the value marked on it by the thumb or the pointer.
//! - **Colours** are a theme's tokens (`"danger"`, `"accent.default"`,
//!   any colour token by path, read in the theme drawn now, so a theme
//!   switch follows) or explicit colours ([`RampColor`]).
//! - **Interpolation in Oklab**, a perceptual space: the middle of a
//!   ramp between two saturated colours stays bright and saturated,
//!   where a blend of sRGB channels goes muddy. The ramp spans the soft
//!   range; a value past it takes the end's colour.
//! - **Accessibility**: the colour never carries the value alone, the
//!   number and the thumb's place stay. In the value mode the colour is
//!   lightened or darkened in Oklab until it stands 3:1 against the
//!   track (WCAG 1.4.11), in dark and light themes alike. A
//!   high-contrast theme draws no ramp: the control keeps the theme's
//!   own fill, its accent.
//!
//! ```
//! use egui::Color32;
//! use egui_pop_core::slider::{Ramp, RampColor};
//! use egui_pop_core::theme::Tokens;
//!
//! let t = Tokens::default();
//! let ramp = Ramp::new(Color32::from_rgb(0, 0, 255), "danger");
//! assert_eq!(ramp.color_at(0.0, &t), Color32::from_rgb(0, 0, 255));
//! assert_eq!(ramp.color_at(1.0, &t), t.status.danger);
//! // Past the ends: the ends' colours.
//! assert_eq!(ramp.color_at(-2.0, &t), ramp.color_at(0.0, &t));
//! let hue = Ramp::stops(&[
//!     (0.0, RampColor::from(Color32::RED)),
//!     (0.5, RampColor::from(Color32::GREEN)),
//!     (1.0, RampColor::token("accent.default")),
//! ]);
//! assert_eq!(hue.color_at(0.5, &t), Color32::GREEN);
//! ```

use egui::epaint::{Mesh, Shape};
use egui::{Color32, Painter, Rect, pos2};

use crate::theme::Tokens;

/// A ramp's colour: a theme token, read in the theme drawn now, or an
/// explicit colour. A token the theme lacks shows as the accent.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum RampColor {
    /// An explicit colour.
    Color(Color32),
    /// A colour token by path (`danger`, `accent.default`,
    /// `warning`).
    Token(String),
}

impl RampColor {
    /// A colour token by path.
    pub fn token(path: impl Into<String>) -> Self {
        RampColor::Token(path.into())
    }

    /// The colour in `tokens`' theme.
    pub fn resolve(&self, tokens: &Tokens) -> Color32 {
        match self {
            RampColor::Color(c) => *c,
            RampColor::Token(path) => tokens.color(path).unwrap_or(tokens.accent.default),
        }
    }
}

impl From<Color32> for RampColor {
    fn from(c: Color32) -> Self {
        RampColor::Color(c)
    }
}

impl From<&str> for RampColor {
    fn from(path: &str) -> Self {
        RampColor::token(path)
    }
}

impl From<String> for RampColor {
    fn from(path: String) -> Self {
        RampColor::Token(path)
    }
}

/// How a ramp is drawn (`Slider::ramp_track`, `Knob::ramp_track`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RampMode {
    /// What shows the value (a slider's fill, a knob's arc and pointer)
    /// in the ramp's colour at the value.
    #[default]
    Value,
    /// The whole track in the ramp, the value marked on it.
    Track,
}

/// Colours along the soft range, from 0 (its start) to 1 (its end),
/// blended in Oklab in between.
#[derive(Clone, Debug, PartialEq)]
pub struct Ramp {
    stops: Vec<(f32, RampColor)>,
}

impl Ramp {
    /// From the minimum's colour to the maximum's.
    pub fn new(min: impl Into<RampColor>, max: impl Into<RampColor>) -> Self {
        Ramp {
            stops: vec![(0.0, min.into()), (1.0, max.into())],
        }
    }

    /// From stops `(place, colour)`, places in `[0, 1]` (sorted here; the
    /// first and last colours run to the ends). No stop at all is the
    /// accent throughout.
    pub fn stops(stops: &[(f32, RampColor)]) -> Self {
        let mut stops: Vec<(f32, RampColor)> = stops
            .iter()
            .filter(|(at, _)| at.is_finite())
            .map(|(at, c)| (at.clamp(0.0, 1.0), c.clone()))
            .collect();
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        if stops.is_empty() {
            stops.push((0.0, RampColor::token("accent.default")));
        }
        Ramp { stops }
    }

    /// The colour at `at` along the soft range (0 its start, 1 its end;
    /// clamped), in `tokens`' theme.
    pub fn color_at(&self, at: f32, tokens: &Tokens) -> Color32 {
        let at = if at.is_finite() {
            at.clamp(0.0, 1.0)
        } else {
            0.0
        };
        // Past the last stop, or before the first: the end's colour.
        let Some(next) = self.stops.iter().position(|(p, _)| *p >= at) else {
            return self.stops[self.stops.len() - 1].1.resolve(tokens);
        };
        let (b, cb) = &self.stops[next];
        if next == 0 {
            return cb.resolve(tokens);
        }
        let (a, ca) = &self.stops[next - 1];
        let t = if b > a { (at - a) / (b - a) } else { 1.0 };
        oklab_mix(ca.resolve(tokens), cb.resolve(tokens), t)
    }

    /// The colour at `at`, made to stand 3:1 against `track`
    /// ([`with_contrast`]).
    pub(crate) fn value_color(&self, at: f32, tokens: &Tokens, track: Color32) -> Color32 {
        with_contrast(self.color_at(at, tokens), track, 3.0)
    }

    /// Paints the ramp over `rect`, left to right (`vertical`: bottom to
    /// top), finely enough for the Oklab blend to show.
    pub(crate) fn paint(&self, painter: &Painter, rect: Rect, tokens: &Tokens, vertical: bool) {
        const N: usize = 32;
        let mut mesh = Mesh::default();
        for i in 0..=N {
            let at = i as f32 / N as f32;
            let c = self.color_at(at, tokens);
            if vertical {
                let y = rect.bottom() - rect.height() * at;
                mesh.colored_vertex(pos2(rect.left(), y), c);
                mesh.colored_vertex(pos2(rect.right(), y), c);
            } else {
                let x = rect.left() + rect.width() * at;
                mesh.colored_vertex(pos2(x, rect.top()), c);
                mesh.colored_vertex(pos2(x, rect.bottom()), c);
            }
            if i > 0 {
                let b = 2 * i as u32;
                mesh.add_triangle(b - 2, b - 1, b);
                mesh.add_triangle(b - 1, b, b + 1);
            }
        }
        painter.add(Shape::mesh(mesh));
    }
}

/// Whether `tokens` are a high-contrast theme's, where a ramp gives way
/// to the theme's own fill (its accent).
pub(crate) fn high_contrast(tokens: &Tokens) -> bool {
    tokens.resolved().spec().base.contains("high-contrast")
}

/// The ramp a control draws in `tokens`' theme: none in a high-contrast
/// theme.
pub(crate) fn active<'r>(ramp: Option<&'r Ramp>, tokens: &Tokens) -> Option<&'r Ramp> {
    ramp.filter(|_| !high_contrast(tokens))
}

fn to_tokens(c: Color32) -> egui_pop_tokens::Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let f = |v: u8| f32::from(v) / 255.0;
    egui_pop_tokens::Color {
        r: f(r),
        g: f(g),
        b: f(b),
        a: f(a),
    }
}

/// `a` to `b` at `t` (0 to 1, clamped), blended in Oklab; alpha
/// blended straight.
pub fn oklab_mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let (ca, cb) = (to_tokens(a), to_tokens(b));
    let (la, lb) = (ca.to_oklab(), cb.to_oklab());
    let tt = f64::from(t);
    let mix = |i: usize| la[i] + (lb[i] - la[i]) * tt;
    crate::theme::color32(egui_pop_tokens::Color::from_oklab(
        mix(0),
        mix(1),
        mix(2),
        ca.a + (cb.a - ca.a) * t,
    ))
}

/// `c`, lightened or darkened in Oklab (its hue and chroma kept) until
/// it stands `ratio` against `against`, or as far as it goes; the
/// direction is the one with more room.
pub(crate) fn with_contrast(c: Color32, against: Color32, ratio: f32) -> Color32 {
    let bg = to_tokens(against);
    let fg = to_tokens(c);
    if egui_pop_tokens::contrast::contrast_ratio(fg, bg) >= ratio {
        return c;
    }
    let [l, a, b] = fg.to_oklab();
    let up = bg.to_oklab()[0] < 0.5;
    let mut l2 = l;
    for _ in 0..50 {
        l2 = if up {
            (l2 + 0.02).min(1.0)
        } else {
            (l2 - 0.02).max(0.0)
        };
        let next = egui_pop_tokens::Color::from_oklab(l2, a, b, fg.a);
        if egui_pop_tokens::contrast::contrast_ratio(next, bg) >= ratio || l2 <= 0.0 || l2 >= 1.0 {
            return crate::theme::color32(next);
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_pop_tokens::{ThemeSpec, Themes};

    fn theme(name: &str) -> Tokens {
        Tokens::new(Themes::default().resolve(&ThemeSpec::new(name)))
    }

    /// Distance from grey: the largest channel minus the smallest.
    fn chroma(c: Color32) -> i32 {
        let [r, g, b, _] = c.to_array();
        i32::from(r.max(g).max(b)) - i32::from(r.min(g).min(b))
    }

    #[test]
    fn the_colour_at_the_ends_and_the_middle() {
        let t = theme("pop-dark");
        let (blue, amber) = (
            Color32::from_rgb(40, 90, 255),
            Color32::from_rgb(255, 170, 30),
        );
        let ramp = Ramp::new(blue, amber);
        assert_eq!(ramp.color_at(0.0, &t), blue);
        assert_eq!(ramp.color_at(1.0, &t), amber);
        assert_eq!(ramp.color_at(0.5, &t), oklab_mix(blue, amber, 0.5));
        // Clamped outside the soft range.
        assert_eq!(ramp.color_at(-1.0, &t), blue);
        assert_eq!(ramp.color_at(7.0, &t), amber);
        assert_eq!(ramp.color_at(f32::NAN, &t), blue);
        // Grey to grey: the middle grey, in Oklab (its lightness halfway).
        let g = Ramp::new(Color32::BLACK, Color32::WHITE).color_at(0.5, &t);
        assert_eq!((g.r(), g.r()), (g.g(), g.b()));
        assert!((95..=105).contains(&g.r()), "{g:?}");
    }

    #[test]
    fn stops_are_sorted_and_run_to_the_ends() {
        let t = theme("pop-dark");
        let ramp = Ramp::stops(&[
            (0.75, Color32::RED.into()),
            (0.25, Color32::GREEN.into()),
            (0.5, Color32::BLUE.into()),
        ]);
        assert_eq!(ramp.color_at(0.0, &t), Color32::GREEN);
        assert_eq!(ramp.color_at(0.25, &t), Color32::GREEN);
        assert_eq!(ramp.color_at(0.5, &t), Color32::BLUE);
        assert_eq!(ramp.color_at(0.75, &t), Color32::RED);
        assert_eq!(ramp.color_at(1.0, &t), Color32::RED);
        assert_eq!(
            Ramp::stops(&[]).color_at(0.3, &t),
            t.accent.default,
            "no stop: the accent"
        );
    }

    #[test]
    fn the_oklab_midpoint_is_not_muddy() {
        // Blue to amber, green to magenta: in sRGB the middle sinks to a
        // dull grey; in Oklab it keeps its lightness and some colour.
        for (a, b) in [
            (
                Color32::from_rgb(0, 80, 255),
                Color32::from_rgb(255, 176, 0),
            ),
            (Color32::from_rgb(0, 200, 0), Color32::from_rgb(220, 0, 220)),
        ] {
            let ok = oklab_mix(a, b, 0.5);
            let srgb = Color32::from_rgb(
                ((u16::from(a.r()) + u16::from(b.r())) / 2) as u8,
                ((u16::from(a.g()) + u16::from(b.g())) / 2) as u8,
                ((u16::from(a.b()) + u16::from(b.b())) / 2) as u8,
            );
            let l = |c: Color32| to_tokens(c).to_oklab()[0];
            let ends = (l(a) + l(b)) / 2.0;
            assert!(
                (l(ok) - ends).abs() < 0.02,
                "Oklab keeps the lightness: {ok:?}"
            );
            assert!(l(srgb) < l(ok) - 0.03, "sRGB darkens: {srgb:?} vs {ok:?}");
            assert!(chroma(ok) >= chroma(srgb), "{ok:?} vs {srgb:?}");
        }
    }

    #[test]
    fn tokens_follow_a_theme_switch() {
        let ramp = Ramp::new("surface.panel", "danger");
        let (dark, light) = (theme("pop-dark"), theme("pop-light"));
        assert_eq!(ramp.color_at(0.0, &dark), dark.surface.panel);
        assert_eq!(ramp.color_at(0.0, &light), light.surface.panel);
        assert_ne!(ramp.color_at(0.0, &dark), ramp.color_at(0.0, &light));
        assert_eq!(ramp.color_at(1.0, &light), light.status.danger);
        // A token the theme lacks: the accent.
        let missing = Ramp::new("no.such.token", "no.such.token");
        assert_eq!(missing.color_at(0.5, &dark), dark.accent.default);
    }

    #[test]
    fn the_value_colour_stands_against_the_track_in_dark_and_light() {
        let grey = Color32::from_gray(128);
        for name in ["pop-dark", "pop-light", "github-dark", "solarized-light"] {
            let t = theme(name);
            // The track: the panel itself, the hardest case.
            let track = t.surface.panel;
            for c in [grey, track, Color32::from_rgb(255, 176, 0), Color32::BLUE] {
                let v = Ramp::new(c, c).value_color(0.5, &t, track);
                let ratio =
                    egui_pop_tokens::contrast::contrast_ratio(to_tokens(v), to_tokens(track));
                assert!(ratio >= 2.95, "{name}: {c:?} -> {v:?} at {ratio:.2}:1");
            }
        }
    }

    #[test]
    fn high_contrast_themes_draw_no_ramp() {
        let ramp = Ramp::new(Color32::BLUE, Color32::RED);
        for name in egui_pop_tokens::BUILT_IN_THEMES {
            let t = theme(name);
            let shown = active(Some(&ramp), &t).is_some();
            assert_eq!(shown, !name.contains("high-contrast"), "{name}");
        }
    }
}
