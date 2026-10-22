//! The kit's `scroll-bar`: egui's `ScrollStyle` with the theme, so every
//! scroll area follows it, and the thumb's colours in a `ScrollBar`
//! scope (`UiPop::scroll_area`).

use egui::style::ScrollStyle;
use egui::{Color32, Context, FullOutput, RawInput, Rect, ScrollArea, Shape, Ui, pos2, vec2};
use egui_pop_core::kit::{Kit, Placement, Visibility};
use egui_pop_core::theme::{self, ScopedTheme, ThemeSpec, Themes, Tokens};
use egui_pop_core::ui::UiPop;

/// tvty's bar (docs/kit.md): a 6 point thumb, fully rounded, 4 points
/// from the edge, no track, over the content, shown at rest.
const TVTY: &str = r##"
$extends: "pop-dark"
scroll-bar:
  width: {$type: "dimension", $value: {value: 6, unit: "px"}}
  margin: {$type: "dimension", $value: {value: 4, unit: "px"}}
  placement: {$type: "string", $value: "over"}
  visibility: {$type: "string", $value: "always"}
  track-fill: {$type: "color", $value: "{transparent}"}
  rest:
    fill: {$type: "color", $value: "#4b4b4b"}
    radius: {$type: "dimension", $value: {value: 999, unit: "px"}}
  hover:
    fill: {$type: "color", $value: "#5a5a5a"}
  active:
    fill: {$type: "color", $value: "#6a6a6a"}
"##;

const THUMB: Color32 = Color32::from_rgb(0x4b, 0x4b, 0x4b);

fn tokens(name: &str, yaml: &str) -> Tokens {
    let mut themes = Themes::default();
    themes.add_text(name, yaml).expect("the test theme reads");
    Tokens::new(themes.resolve(&ThemeSpec::new(name)))
}

/// tvty's bar with another placement and visibility.
fn tvty_with(placement: &str, visibility: &str) -> String {
    TVTY.replace(r#"$value: "over""#, &format!(r#"$value: "{placement}""#))
        .replace(r#"$value: "always""#, &format!(r#"$value: "{visibility}""#))
}

fn frame(ctx: &Context, add: impl FnMut(&mut Ui)) -> FullOutput {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, add);
    out.textures_delta.clear();
    out
}

/// A scroll area whose content overflows, its bars always there.
fn tall_area() -> ScrollArea {
    ScrollArea::vertical()
        .id_salt("tall")
        .max_height(120.0)
        .auto_shrink(false)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
}

fn tall_content(ui: &mut Ui) {
    for i in 0..40 {
        ui.label(format!("Row {i}"));
    }
}

/// Whether a frame painted a rectangle filled with `fill`.
fn painted(out: &FullOutput, fill: Color32) -> bool {
    out.shapes
        .iter()
        .any(|c| matches!(&c.shape, Shape::Rect(r) if r.fill == fill))
}

#[test]
fn the_scroll_bar_tokens_reach_egui_s_scroll_style() {
    // The default kit is egui's own floating bars.
    let ctx = Context::default();
    theme::install(&ctx, &ThemeSpec::new("pop-dark"));
    assert_eq!(ctx.global_style().spacing.scroll, ScrollStyle::floating());

    theme::install_tokens(&ctx, tokens("tvty", TVTY));
    let kit = Kit::of(&ctx);
    assert_eq!(kit.scroll.placement, Placement::Over);
    assert_eq!(kit.scroll.visibility, Visibility::Always);
    let s = ctx.global_style().spacing.scroll;
    assert!(s.floating);
    assert_eq!(s.bar_width, 6.0);
    assert_eq!(s.floating_width, 6.0, "full width at rest");
    assert_eq!(s.bar_outer_margin, 4.0);
    assert_eq!(s.floating_allocated_width, 0.0);
    assert!(s.foreground_color);
    // No track anywhere.
    assert_eq!(s.dormant_background_opacity, 0.0);
    assert_eq!(s.interact_background_opacity, 0.0);
    // The thumb's colours and corners, in a `ScrollBar` scope.
    let scope = kit.scoped.get(
        egui_pop_core::kit::Stock::ScrollBar,
        egui_pop_core::kit::Tone::Neutral,
    );
    assert_eq!(scope.widgets[0].fg_stroke.color, THUMB);
    assert_eq!(
        scope.widgets[1].fg_stroke.color,
        Color32::from_rgb(0x5a, 0x5a, 0x5a)
    );
    assert_eq!(
        scope.widgets[2].fg_stroke.color,
        Color32::from_rgb(0x6a, 0x6a, 0x6a)
    );
    assert_eq!(
        scope.widgets[0].corner_radius,
        egui::CornerRadius::same(255)
    );
    assert_eq!(scope.extreme_bg, Some(Color32::TRANSPARENT));
}

#[test]
fn a_floating_bar_leaves_the_content_its_width_and_a_bar_beside_takes_room() {
    let width_of = |yaml: &str| {
        let ctx = Context::default();
        theme::install_tokens(&ctx, tokens("bar", yaml));
        let (mut outer, mut inner) = (0.0, 0.0);
        for _ in 0..3 {
            frame(&ctx, |ui| {
                outer = ui.available_width();
                let out = ui.scroll_area(tall_area(), tall_content);
                inner = out.inner_rect.width();
            });
        }
        (outer, inner)
    };
    let (outer, over) = width_of(&tvty_with("over", "always"));
    assert_eq!(over, outer, "the content runs under a floating bar");
    let (outer, beside) = width_of(&tvty_with("beside", "always"));
    // egui's gap between the content and the bar, the bar, its margin.
    let gap = ScrollStyle::floating().bar_inner_margin;
    assert_eq!(beside, outer - (gap + 6.0 + 4.0));
}

#[test]
fn a_bar_shows_at_rest_only_when_its_visibility_says_always() {
    let at_rest = |visibility: &str| {
        let ctx = Context::default();
        theme::install_tokens(&ctx, tokens("bar", &tvty_with("over", visibility)));
        let mut out = None;
        for _ in 0..3 {
            // No pointer: the scroll area is at rest.
            out = Some(frame(&ctx, |ui| {
                ui.scroll_area(tall_area(), tall_content);
            }));
        }
        painted(&out.expect("a frame ran"), THUMB)
    };
    assert!(at_rest("always"));
    assert!(!at_rest("hover"));
}

#[test]
fn a_scoped_theme_s_scroll_bar_follows_the_scope() {
    let ctx = Context::default();
    theme::install(&ctx, &ThemeSpec::new("pop-dark"));
    let tvty = ScopedTheme::new(&ctx, tokens("tvty", TVTY));
    let (mut inside, mut outside) = (None, None);
    let mut content_visuals = None;
    let mut out = None;
    for _ in 0..3 {
        out = Some(frame(&ctx, |ui| {
            tvty.show(ui, |ui| {
                inside = Some(ui.spacing().scroll);
                let scoped = ui.visuals().widgets.inactive;
                ui.scroll_area(tall_area(), |ui| {
                    // The content keeps the scope's style, not the bar's.
                    content_visuals = Some(ui.visuals().widgets.inactive);
                    tall_content(ui);
                });
                assert_eq!(content_visuals, Some(scoped));
                assert_ne!(scoped.fg_stroke.color, THUMB);
            });
            outside = Some(ui.spacing().scroll);
        }));
    }
    let (inside, outside) = (inside.unwrap(), outside.unwrap());
    assert_eq!(inside.bar_width, 6.0);
    assert_eq!(inside.dormant_handle_opacity, 1.0);
    assert_eq!(outside, ScrollStyle::floating());
    // The scoped theme's thumb, at rest.
    assert!(painted(&out.expect("a frame ran"), THUMB));
}
