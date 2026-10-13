//! The terminal's component tokens (`terminal.*` in the theme files),
//! each an alias of a system token, and the colours programs ask for.

use egui::{Color32, Context};
use egui_pop_core::theme::Tokens;

/// The terminal's colours, from the installed theme.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalTokens {
    pub background: Color32,
    pub foreground: Color32,
    pub bold: Color32,
    pub cursor: Color32,
    pub selection: Color32,
    pub link: Color32,
    /// `terminal.ansi.0` to `terminal.ansi.15`.
    pub ansi: [Color32; 16],
}

impl TerminalTokens {
    /// The tokens of the theme installed in `ctx`.
    pub fn of(ctx: &Context) -> TerminalTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        let fallbacks = [
            t.surface.active,
            t.status.danger,
            t.status.success,
            t.status.warning,
            t.status.info,
            t.accent.default,
            t.status.info,
            t.text.default,
            t.border.strong,
            t.status.danger,
            t.status.success,
            t.status.warning,
            t.status.info,
            t.accent.hover,
            t.status.info,
            t.text.strong,
        ];
        let mut ansi = fallbacks;
        for (i, slot) in ansi.iter_mut().enumerate() {
            *slot = c(&format!("terminal.ansi.{i}"), fallbacks[i]);
        }
        TerminalTokens {
            background: c("terminal.background", t.surface.sunken),
            foreground: c("terminal.foreground", t.text.default),
            bold: c("terminal.bold", t.text.strong),
            cursor: c("terminal.cursor", t.text.strong),
            selection: c("terminal.selection", t.selection),
            link: c("terminal.link", t.accent.default),
            ansi,
        }
    }

    /// The colour of a vt100 colour: the theme's 16 for indexes 0 to 15
    /// (a bold 0 to 7 takes its stronger form), the xterm cube and grey
    /// ramp for 16 to 255, RGB as given; `default` for the default.
    pub fn color(&self, c: vt100::Color, default: Color32, bold: bool) -> Color32 {
        match c {
            vt100::Color::Default => default,
            vt100::Color::Idx(i) if i < 8 && bold => self.ansi[usize::from(i) + 8],
            vt100::Color::Idx(i) if i < 16 => self.ansi[usize::from(i)],
            vt100::Color::Idx(i) => indexed(i),
            vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        }
    }
}

/// xterm's colours 16 to 255: a 6×6×6 cube, then 24 greys.
pub fn indexed(i: u8) -> Color32 {
    if i >= 232 {
        let v = 8 + (i - 232) * 10;
        return Color32::from_rgb(v, v, v);
    }
    let n = i.saturating_sub(16);
    let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
    Color32::from_rgb(level(n / 36), level((n / 6) % 6), level(n % 6))
}

/// `a` moved towards `b` by `t` (0: `a`, 1: `b`).
pub(crate) fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cube_and_the_greys_are_xterms() {
        assert_eq!(indexed(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(indexed(196), Color32::from_rgb(255, 0, 0));
        assert_eq!(indexed(231), Color32::from_rgb(255, 255, 255));
        assert_eq!(indexed(232), Color32::from_rgb(8, 8, 8));
        assert_eq!(indexed(255), Color32::from_rgb(238, 238, 238));
    }
}
