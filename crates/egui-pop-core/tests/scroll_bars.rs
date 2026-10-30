//! The user's scroll bars (`ThemeSpec::scroll_bars`) in a real scroll area:
//! what egui's style says, and the room the content is left with.

use egui_kittest::Harness;
use egui_pop_core::kit::Kit;
use egui_pop_core::theme::{self, BUILT_IN_THEMES, ScrollBars, ThemeSpec};

/// What a scroll area overflowing vertically takes from its content's width,
/// under `spec`.
fn room_taken(spec: &ThemeSpec) -> f32 {
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 200.0))
        .build_ui_state(
            |ui, width: &mut (f32, f32)| {
                *width = (ui.available_width(), 0.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    width.1 = ui.available_width();
                    ui.allocate_space(egui::vec2(10.0, 2000.0));
                });
            },
            (0.0, 0.0),
        );
    theme::install(&h.ctx, spec);
    h.run();
    let (outer, inner) = *h.state();
    assert!(outer > 0.0);
    outer - inner
}

#[test]
fn each_choice_leaves_the_content_its_own_room() {
    let spec = |choice| ThemeSpec::new("pop-dark").scroll_bars(choice);
    // Auto is pop-dark's quiet bar: its 6 points of room.
    assert_eq!(room_taken(&spec(ScrollBars::Auto)), 6.0);
    // Egui's own: the bar lies over the content, which keeps all its width.
    assert_eq!(room_taken(&spec(ScrollBars::Hover)), 0.0);
    // Beside the content: its 10 points and egui's gap of 4.
    assert_eq!(room_taken(&spec(ScrollBars::Always)), 14.0);
}

#[test]
fn every_theme_draws_each_choice_without_a_problem() {
    for name in BUILT_IN_THEMES {
        let style = |choice| {
            let spec = ThemeSpec::new(*name).scroll_bars(choice);
            let tokens = theme::Tokens::new(theme::Themes::default().resolve(&spec));
            let kit = Kit::compile(&tokens, &Default::default());
            assert!(kit.problems.is_empty(), "{name}: {:?}", kit.problems);
            kit.scroll.style(&egui::style::ScrollStyle::default())
        };
        let hover = style(ScrollBars::Hover);
        assert!(hover.floating, "{name}");
        assert_eq!(hover.dormant_handle_opacity, 0.0, "{name}: nothing at rest");
        assert_eq!(hover.floating_allocated_width, 0.0, "{name}: no room kept");
        let always = style(ScrollBars::Always);
        assert!(!always.floating, "{name}: beside the content, opaque");
        assert!(always.dormant_background_opacity > 0.0, "{name}: a track");
    }
}

#[test]
fn a_pair_s_two_styles_take_the_choice() {
    let ctx = egui::Context::default();
    theme::install(
        &ctx,
        &ThemeSpec::new("pop-dark").scroll_bars(ScrollBars::Always),
    );
    for t in [egui::Theme::Dark, egui::Theme::Light] {
        assert!(!ctx.style_of(t).spacing.scroll.floating, "{t:?}");
    }
}
