//! What every scroll area takes from the kit's `scroll-bar` (its
//! `width`, `width-rest`, `opacity-rest`, `room`, `min-length`, `margin`,
//! `placement`, `visibility` and `track-fill`): egui's `ScrollStyle`,
//! written into egui's style with the theme
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
    /// At rest too, thin and quiet (`width-rest`, `opacity-rest`, no track),
    /// widening to `width` over the bar; `room` is kept for it, so that it
    /// does not lie over the content at rest.
    Rest,
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
    /// Points: the bar at rest, under [`Visibility::Rest`]
    /// (`scroll-bar.width-rest`). It is sensed at `width` all the same.
    pub width_rest: f32,
    /// The thumb's opacity at rest, 0 to 1, under [`Visibility::Rest`]
    /// (`scroll-bar.opacity-rest`).
    pub opacity_rest: f32,
    /// Points of the content's width kept for the bar while it shows, under
    /// [`Visibility::Rest`] (`scroll-bar.room`).
    pub room: f32,
    /// Points: the thumb's least length (`scroll-bar.min-length`).
    pub min_length: f32,
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
            width_rest: egui.floating_width,
            opacity_rest: egui.active_handle_opacity,
            room: ScrollStyle::thin().floating_allocated_width,
            min_length: egui.handle_min_length,
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
                ("rest", Visibility::Rest),
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
        if placement == Placement::Beside && visibility == Visibility::Rest {
            problems.push(
                "scroll-bar.visibility: egui draws a bar `beside` the content opaque and at full width, `rest` needs `over`"
                    .into(),
            );
        } else if placement == Placement::Beside && visibility != Visibility::Always {
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
        let width_rest = r
            .dimension("scroll-bar.width-rest")
            .map_or(egui.width_rest, |w| w.max(0.0));
        if visibility == Visibility::Rest && width_rest > width {
            problems.push(format!(
                "scroll-bar.width-rest: {width_rest} is wider than `width`, drawn at {width}"
            ));
        }
        let opacity_rest = r
            .dimension("scroll-bar.opacity-rest")
            .unwrap_or(egui.opacity_rest);
        if !(0.0..=1.0).contains(&opacity_rest) {
            problems.push(format!(
                "scroll-bar.opacity-rest: {opacity_rest} is not between 0 and 1"
            ));
        }
        ScrollBarKit {
            placement,
            visibility,
            width,
            width_rest,
            opacity_rest: opacity_rest.clamp(0.0, 1.0),
            room: r
                .dimension("scroll-bar.room")
                .map_or(egui.room, |m| m.max(0.0)),
            min_length: r
                .dimension("scroll-bar.min-length")
                .map_or(egui.min_length, |m| m.max(0.0)),
            margin: r
                .dimension("scroll-bar.margin")
                .map_or(egui.margin, |m| m.max(0.0)),
            track: t
                .color("scroll-bar.track-fill")
                .unwrap_or(Color32::TRANSPARENT),
        }
    }

    /// These terms for a bar that must be seen as soon as its content
    /// overflows, whatever the theme does for the others: quiet at rest,
    /// unless the theme's bars are already `always`.
    pub fn seen_at_rest(self) -> ScrollBarKit {
        ScrollBarKit {
            visibility: match self.visibility {
                Visibility::Always => Visibility::Always,
                _ => Visibility::Rest,
            },
            ..self
        }
    }

    /// egui's `ScrollStyle` for these terms, over `base` (what the kit
    /// does not decide is kept: the content margin, the gap between a bar beside the content and the content,
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
        s.handle_min_length = self.min_length;
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
            Visibility::Rest => {
                s.floating_width = self.width_rest.min(self.width);
                s.floating_allocated_width = self.room;
                s.dormant_handle_opacity = self.opacity_rest;
                // Coming near never dims it.
                s.active_handle_opacity = egui.active_handle_opacity.max(self.opacity_rest);
                s.interact_handle_opacity = egui.interact_handle_opacity;
                // The track only over the bar, as egui's own draws it there.
                s.dormant_background_opacity = 0.0;
                s.active_background_opacity = 0.0;
                s.interact_background_opacity = egui.interact_background_opacity;
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
    fn rest_writes_its_tokens_into_egui_s_style() {
        let k = kit(r##"
            $extends: "pop-dark"
            scroll-bar:
              visibility: {$type: "string", $value: "rest"}
              width: {$type: "dimension", $value: {value: 12, unit: "px"}}
              width-rest: {$type: "dimension", $value: {value: 3, unit: "px"}}
              opacity-rest: {$type: "number", $value: 0.8}
              room: {$type: "dimension", $value: {value: 5, unit: "px"}}
              min-length: {$type: "dimension", $value: {value: 30, unit: "px"}}
            "##);
        assert!(k.problems.is_empty(), "{:?}", k.problems);
        assert_eq!(k.scroll.visibility, Visibility::Rest);
        let s = k.scroll.style(&ScrollStyle::default());
        assert!(s.floating);
        assert_eq!(s.bar_width, 12.0, "the hit area, and the bar at its widest");
        assert_eq!(s.floating_width, 3.0);
        assert_eq!(s.floating_allocated_width, 5.0);
        assert_eq!(s.handle_min_length, 30.0);
        assert_eq!(s.dormant_handle_opacity, 0.8);
        assert_eq!(s.active_handle_opacity, 0.8, "coming near never dims it");
        assert_eq!(s.interact_handle_opacity, 1.0);
        assert_eq!(s.dormant_background_opacity, 0.0, "no track at rest");
        assert_eq!(s.active_background_opacity, 0.0);
        assert!(s.interact_background_opacity > 0.0, "a track over the bar");
    }

    #[test]
    fn rest_s_terms_are_kept_out_of_the_other_visibilities() {
        let k = kit(r##"
            $extends: "pop-dark"
            scroll-bar:
              visibility: {$type: "string", $value: "hover"}
              width-rest: {$type: "dimension", $value: {value: 3, unit: "px"}}
              opacity-rest: {$type: "number", $value: 0.8}
              room: {$type: "dimension", $value: {value: 5, unit: "px"}}
            "##);
        let s = k.scroll.style(&ScrollStyle::default());
        assert_eq!(s.floating_allocated_width, 0.0);
        assert_eq!(s.dormant_handle_opacity, 0.0);
        assert_eq!(s.floating_width, ScrollStyle::floating().floating_width);
    }

    #[test]
    fn seen_at_rest_never_lowers_an_always_bar() {
        let mut bar = ScrollBarKit::default();
        assert_eq!(bar.seen_at_rest().visibility, Visibility::Rest);
        bar.visibility = Visibility::Scroll;
        assert_eq!(bar.seen_at_rest().visibility, Visibility::Rest);
        bar.visibility = Visibility::Always;
        assert_eq!(bar.seen_at_rest().visibility, Visibility::Always);
    }

    #[test]
    fn a_rest_bar_egui_cannot_draw_is_a_problem() {
        let k = kit(r##"
            $extends: "pop-dark"
            scroll-bar:
              placement: {$type: "string", $value: "beside"}
              visibility: {$type: "string", $value: "rest"}
              width-rest: {$type: "dimension", $value: {value: 20, unit: "px"}}
              opacity-rest: {$type: "number", $value: 1.5}
            "##);
        let p = k.problems.join("\n");
        assert!(p.contains("`rest` needs `over`"), "{p}");
        assert!(p.contains("wider than `width`"), "{p}");
        assert!(p.contains("not between 0 and 1"), "{p}");
        assert_eq!(k.scroll.opacity_rest, 1.0);
        assert_eq!(
            k.scroll.style(&ScrollStyle::default()).floating_width,
            k.scroll.width
        );
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
