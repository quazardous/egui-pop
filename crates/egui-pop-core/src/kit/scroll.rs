//! What every scroll area takes from the kit's `scroll-bar` (its
//! `width`, `margin`, `placement`, `visibility` and `track-fill`):
//! egui's `ScrollStyle`, written into egui's style with the theme
//! (`theme::style_with`), so egui's scroll areas, an application's
//! included, follow it. The thumb's colours and corners are egui's
//! widget visuals: they are the kit's in a `ScrollBar` scope
//! (`Stock::ScrollBar`, `UiPop::scroll_area`).
//! Compiled with the kit, read as plain fields.

use egui::Color32;
use egui::style::ScrollStyle;

use super::window::choice;
use super::{State, Tokens};

/// Where a scroll bar goes (`scroll-bar.placement`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    /// Over the content, which runs to the edge under it (egui's
    /// floating bars). The default.
    #[default]
    Over,
    /// Beside the content, which is narrower by the bar, its margin and
    /// egui's gap between them.
    Beside,
}

/// When a scroll bar shows (`scroll-bar.visibility`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visibility {
    /// At rest too, at its full width.
    Always,
    /// While the pointer is over the scroll area, thin until it is over
    /// the bar (egui's floating bars). The default.
    #[default]
    Hover,
    /// While scrolling. egui 0.36 has no such state: drawn as `Hover`,
    /// and reported in the kit's problems.
    Scroll,
}

/// The scroll bars' terms of a kit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollBarKit {
    pub placement: Placement,
    pub visibility: Visibility,
    /// Points: the bar at its widest (`scroll-bar.width`).
    pub width: f32,
    /// Points between the bar and the scroll area's edge
    /// (`scroll-bar.margin`).
    pub margin: f32,
    /// The track behind the thumb (`scroll-bar.track-fill`); none when
    /// transparent.
    pub track: Color32,
}

impl Default for ScrollBarKit {
    /// egui's own floating bars.
    fn default() -> Self {
        let egui = ScrollStyle::floating();
        ScrollBarKit {
            placement: Placement::Over,
            visibility: Visibility::Hover,
            width: egui.bar_width,
            margin: egui.bar_outer_margin,
            track: Color32::TRANSPARENT,
        }
    }
}

impl ScrollBarKit {
    pub(super) fn compile(
        t: &Tokens,
        component: &super::ComponentKit,
        problems: &mut Vec<String>,
    ) -> ScrollBarKit {
        let r = t.resolved();
        let egui = ScrollBarKit::default();
        let placement = choice(
            t,
            "scroll-bar.placement",
            &[("over", Placement::Over), ("beside", Placement::Beside)],
            problems,
        );
        let visibility = choice(
            t,
            "scroll-bar.visibility",
            &[
                ("always", Visibility::Always),
                ("hover", Visibility::Hover),
                ("scroll", Visibility::Scroll),
            ],
            problems,
        );
        if visibility == Visibility::Scroll {
            problems.push(
                "scroll-bar.visibility: egui has no while-scrolling state, `scroll` is drawn as `hover`"
                    .into(),
            );
        }
        if placement == Placement::Beside && visibility != Visibility::Always {
            problems.push(
                "scroll-bar.visibility: egui always shows a bar `beside` the content when the content overflows"
                    .into(),
            );
        }
        let width = if component.width > 0.0 {
            component.width
        } else {
            egui.width
        };
        ScrollBarKit {
            placement,
            visibility,
            width,
            margin: r
                .dimension("scroll-bar.margin")
                .map_or(egui.margin, |m| m.max(0.0)),
            track: t
                .color("scroll-bar.track-fill")
                .unwrap_or(Color32::TRANSPARENT),
        }
    }

    /// egui's `ScrollStyle` for these terms, over `base` (what the kit
    /// does not decide is kept: the content margin, the handle's least
    /// length, the gap between a bar beside the content and the content,
    /// the fades). The thumb is drawn in the widget states' foreground
    /// colour (`foreground_color`), which a `ScrollBar` scope sets to the
    /// kit's thumb fills. With the default kit, this is egui's own
    /// `ScrollStyle::floating()`.
    pub fn style(&self, base: &ScrollStyle) -> ScrollStyle {
        let egui = ScrollStyle::floating();
        let mut s = *base;
        s.floating = self.placement == Placement::Over;
        s.bar_width = self.width;
        s.bar_outer_margin = self.margin;
        s.floating_allocated_width = 0.0;
        s.foreground_color = true;
        match self.visibility {
            Visibility::Always => {
                s.floating_width = self.width;
                s.dormant_handle_opacity = 1.0;
                s.active_handle_opacity = 1.0;
                s.interact_handle_opacity = 1.0;
                s.dormant_background_opacity = 1.0;
                s.active_background_opacity = 1.0;
                s.interact_background_opacity = 1.0;
            }
            Visibility::Hover | Visibility::Scroll => {
                s.floating_width = egui.floating_width.min(self.width);
                s.dormant_handle_opacity = egui.dormant_handle_opacity;
                s.active_handle_opacity = egui.active_handle_opacity;
                s.interact_handle_opacity = egui.interact_handle_opacity;
                s.dormant_background_opacity = egui.dormant_background_opacity;
                s.active_background_opacity = egui.active_background_opacity;
                s.interact_background_opacity = egui.interact_background_opacity;
            }
        }
        if self.track.a() == 0 {
            // No track, in every scroll area: egui draws it in
            // `extreme_bg_color`, which other widgets share.
            s.dormant_background_opacity = 0.0;
            s.active_background_opacity = 0.0;
            s.interact_background_opacity = 0.0;
        }
        s
    }
}

/// What egui's scroll areas outside a `ScrollBar` scope cannot show of
/// `scroll-bar`: egui draws their thumb in the widget states' text
/// colours and corners (the button's), their track in `surface.sunken`.
pub(super) fn losses(t: &Tokens, kit: &super::Kit) -> Vec<String> {
    let mut lost = Vec::new();
    let (bar, button) = (&kit.scroll_bar, &kit.button);
    let thumb_differs = [State::REST, State::HOVER, State::ACTIVE]
        .into_iter()
        .any(|s| bar.style(s).fill != button.style(s).text)
        || bar.style(State::REST).radius != button.style(State::REST).radius;
    if thumb_differs {
        lost.push(
            "scroll-bar: egui's scroll areas outside a `ScrollBar` scope (`UiPop::scroll_area`) draw the thumb in the button's text colours and corners"
                .into(),
        );
    }
    let track = kit.scroll.track;
    if track.a() > 0 && track != t.surface.sunken {
        lost.push(
            "scroll-bar.track-fill: egui's scroll areas outside a `ScrollBar` scope draw the track in `surface.sunken`"
                .into(),
        );
    }
    lost
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::{Kit, Shapes};
    use egui_pop_tokens::{ThemeSpec, Themes};

    fn kit(yaml: &str) -> Kit {
        let mut themes = Themes::default();
        themes
            .add_text("scroll-test", yaml)
            .expect("test theme reads");
        let t = Tokens::new(themes.resolve(&ThemeSpec::new("scroll-test")));
        Kit::compile(&t, &Shapes::default())
    }

    #[test]
    fn the_default_kit_gives_egui_s_own_floating_bars() {
        for name in egui_pop_tokens::BUILT_IN_THEMES {
            let t = Tokens::new(Themes::default().resolve(&ThemeSpec::new(*name)));
            let kit = Kit::compile(&t, &Shapes::default());
            assert!(kit.problems.is_empty(), "{name}: {:?}", kit.problems);
            assert_eq!(
                kit.scroll.style(&ScrollStyle::default()),
                ScrollStyle::floating(),
                "{name}"
            );
        }
    }

    #[test]
    fn what_egui_cannot_draw_of_a_scroll_bar_is_a_problem() {
        let k = kit(r##"
            $extends: "pop-dark"
            scroll-bar:
              placement: {$type: "string", $value: "beside"}
              visibility: {$type: "string", $value: "scroll"}
              track-fill: {$type: "color", $value: "#ff0000"}
              rest:
                fill: {$type: "color", $value: "#4b4b4b"}
            "##);
        let p = k.problems.join("\n");
        assert!(p.contains("`scroll` is drawn as `hover`"), "{p}");
        assert!(p.contains("`beside` the content"), "{p}");
        assert!(p.contains("scroll-bar: egui's scroll areas"), "{p}");
        assert!(p.contains("scroll-bar.track-fill"), "{p}");
        let bad = kit(r##"
            $extends: "pop-dark"
            scroll-bar:
              placement: {$type: "string", $value: "under"}
            "##);
        assert_eq!(bad.scroll.placement, Placement::Over);
        assert_eq!(
            bad.problems,
            ["scroll-bar.placement: `under` is not over or beside"]
        );
    }
}
