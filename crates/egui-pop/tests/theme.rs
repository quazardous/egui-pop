//! Installing a theme changes egui's style as the tokens say, and egui's
//! widgets are drawn with it (rendered on wgpu by egui_kittest).

use egui::{Color32, TextStyle};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::core::kit::{Kit, State};
use egui_pop::theme::{self, BUILT_IN_THEMES, Density, ThemeSpec, Tokens};
use egui_pop::tokens::{Themes, TokenFile};
use egui_pop_test::SharedGpu as _;

/// Whether a theme is, through its `$extends` chain, a light one.
fn extends_pop_light(name: &str) -> bool {
    let themes = Themes::default();
    let mut at = Some(name.to_string());
    while let Some(n) = at {
        if n == "pop-light" {
            return true;
        }
        at = themes.get(&n).and_then(|t| t.extends.clone());
    }
    false
}

#[test]
fn installing_each_theme_sets_the_visuals_from_its_tokens() {
    for base in BUILT_IN_THEMES {
        let ctx = egui::Context::default();
        let t = theme::install(&ctx, &ThemeSpec::new(*base));
        assert!(t.resolved().problems().is_empty(), "{base}");
        let style = ctx.global_style();
        let v = &style.visuals;
        assert_eq!(v.dark_mode, !extends_pop_light(base), "{base}");
        assert_eq!(v.panel_fill, t.surface.panel, "{base}");
        assert_eq!(v.window_fill, t.surface.overlay, "{base}");
        assert_eq!(v.extreme_bg_color, t.surface.sunken, "{base}");
        assert_eq!(v.selection.bg_fill, t.selection, "{base}");
        assert_eq!(v.hyperlink_color, t.accent.default, "{base}");
        assert_eq!(v.error_fg_color, t.status.danger, "{base}");
        assert_eq!(v.warn_fg_color, t.status.warning, "{base}");
        assert_eq!(v.weak_text_color, Some(t.text.muted), "{base}");
        let button = |s| *Kit::of(&ctx).button.style(s);
        assert_eq!(
            v.widgets.inactive.bg_fill,
            button(State::REST).fill,
            "{base}"
        );
        assert_eq!(
            v.widgets.hovered.bg_fill,
            button(State::HOVER).fill,
            "{base}"
        );
        assert_eq!(
            v.widgets.active.bg_fill,
            button(State::ACTIVE).fill,
            "{base}"
        );
        assert_eq!(v.widgets.active.bg_stroke.color, t.focus_ring, "{base}");
        assert_eq!(
            v.widgets.inactive.fg_stroke.color,
            button(State::REST).text,
            "{base}"
        );
        assert_eq!(
            style.text_styles[&TextStyle::Body].size,
            t.font.body,
            "{base}"
        );
        // The values come from the theme file, through aliases.
        let r = t.resolved();
        if let Some(target) = r.get("surface.panel").unwrap().alias.as_deref() {
            assert_eq!(Some(v.panel_fill), r.color(target).map(theme::color32));
        }
        // egui's other style is the counterpart's, or the theme's own
        // when it is alone.
        let other = if t.dark_mode() {
            egui::Theme::Light
        } else {
            egui::Theme::Dark
        };
        ctx.set_theme(other);
        let expected = match Themes::default().counterpart_of(base) {
            Some(c) => {
                Tokens::new(Themes::default().resolve(&ThemeSpec::new(c)))
                    .surface
                    .panel
            }
            None => t.surface.panel,
        };
        assert_eq!(ctx.global_style().visuals.panel_fill, expected, "{base}");
    }
}

#[test]
fn themes_differ_where_they_should() {
    let style_of = |spec: &ThemeSpec| {
        let ctx = egui::Context::default();
        theme::install(&ctx, spec);
        ctx.global_style()
    };
    let dark = style_of(&ThemeSpec::new("pop-dark"));
    let light = style_of(&ThemeSpec::new("pop-light"));
    let high = style_of(&ThemeSpec::new("pop-high-contrast"));
    assert_ne!(dark.visuals.panel_fill, light.visuals.panel_fill);
    assert_eq!(high.visuals.panel_fill, Color32::BLACK);
    assert!(high.text_styles[&TextStyle::Body].size > dark.text_styles[&TextStyle::Body].size);

    let compact = style_of(&ThemeSpec::new("pop-dark").density(Density::Compact));
    assert!(compact.spacing.item_spacing.x < dark.spacing.item_spacing.x);
    assert!(compact.spacing.interact_size.y < dark.spacing.interact_size.y);
    // Density changes no colour.
    assert_eq!(compact.visuals, dark.visuals);

    let blue = style_of(&ThemeSpec::new("pop-dark").accent("blue"));
    assert_ne!(blue.visuals.hyperlink_color, dark.visuals.hyperlink_color);
    assert_eq!(blue.visuals.panel_fill, dark.visuals.panel_fill);
}

#[test]
fn reduced_motion_turns_animations_off() {
    let ctx = egui::Context::default();
    theme::install(&ctx, &ThemeSpec::default());
    assert!((ctx.global_style().animation_time - 0.12).abs() < 1e-6);
    theme::install(&ctx, &ThemeSpec::default().reduced_motion(true));
    let style = ctx.global_style();
    assert_eq!(style.animation_time, 0.0);
    assert!(!style.visuals.text_cursor.blink);
}

#[test]
fn tokens_are_read_back_from_the_context() {
    let ctx = egui::Context::default();
    // Nothing installed: the default theme.
    assert_eq!(*Tokens::of(&ctx), Tokens::default());
    let installed = theme::install(&ctx, &ThemeSpec::new("pop-light").accent("green"));
    let read = Tokens::of(&ctx);
    assert_eq!(read.accent.default, installed.accent.default);
    assert_eq!(read.resolved().spec().accent.as_deref(), Some("green"));
    assert_eq!(read.color("green.9"), Some(read.accent.default));
}

#[test]
fn a_broken_theme_installs_with_fallbacks_and_reports() {
    let text = r#"
            $extends: "pop-dark"
            surface:
              $type: "color"
              panel: {$value: "{nope}"}
            "#;
    let mut themes = Themes::default();
    // Loaded by the rule, the broken token is left out and said so.
    let warnings = themes.add_text("broken", text).unwrap();
    assert_eq!(
        warnings.iter().filter_map(|w| w.path()).collect::<Vec<_>>(),
        ["surface.panel"]
    );
    // Added as written, past the rule: what install does with it.
    themes.add(TokenFile::parse("broken", text).unwrap());
    let ctx = egui::Context::default();
    let t = theme::install_with(&ctx, &themes, &ThemeSpec::new("broken"));
    // The broken role, and the component tokens aliasing it.
    let problems = t.resolved().problems();
    assert!(!problems.is_empty());
    assert!(
        problems
            .iter()
            .all(|p| p.to_string().contains("surface.panel")),
        "{problems:?}"
    );
    assert_eq!(
        ctx.global_style().visuals.panel_fill,
        Tokens::default().surface.panel
    );
}

#[test]
fn egui_widgets_are_drawn_with_the_theme() {
    for base in BUILT_IN_THEMES {
        let spec = ThemeSpec::new(*base);
        let mut harness = Harness::builder()
            .with_size(egui::vec2(240.0, 120.0))
            .shared_wgpu()
            .build_ui_state(
                |ui, panel: &mut egui::Rect| {
                    *panel = egui::CentralPanel::default()
                        .show(ui, |ui| {
                            let _ = ui.button("Press");
                            ui.label("Label");
                        })
                        .response
                        .rect;
                },
                egui::Rect::NOTHING,
            );
        let t = theme::install(&harness.ctx, &spec);
        harness.run();
        assert!(harness.query_by_label("Press").is_some());
        let image = harness.render().expect("render");
        // The panel's corner, far from any widget, shows the panel fill.
        let corner = harness.state().right_bottom() - egui::vec2(3.0, 3.0);
        assert!(corner.x > 100.0 && corner.y > 60.0, "{corner:?}");
        let px = image.get_pixel(corner.x as u32, corner.y as u32).0;
        let panel = t.surface.panel;
        assert_eq!(px, [panel.r(), panel.g(), panel.b(), 255], "{base}");
    }
}

/// A harness drawing one egui button, its rest fill read back each frame.
fn button_harness() -> Harness<'static, Option<Color32>> {
    Harness::builder()
        .with_size(egui::vec2(200.0, 80.0))
        .build_ui_state(
            |ui, fill: &mut Option<Color32>| {
                let _ = ui.button("Press");
                *fill = Some(ui.visuals().widgets.inactive.bg_fill);
            },
            None,
        )
}

#[test]
fn a_pair_switches_tokens_style_and_kit_without_installing_again() {
    let mut h = button_harness();
    let dark = theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    let light = Tokens::new(Themes::default().resolve(&ThemeSpec::new("pop-light")));
    assert!(theme::is_paired(&h.ctx));
    // The theme picked is shown, as before.
    h.run();
    assert_eq!(h.ctx.theme(), egui::Theme::Dark);
    assert_eq!(Tokens::of(&h.ctx).surface.panel, dark.surface.panel);
    let dark_kit = Kit::of(&h.ctx);
    assert_eq!(*h.state(), Some(dark_kit.button.style(State::REST).fill));

    h.ctx.set_theme(egui::Theme::Light);
    h.run();
    let t = Tokens::of(&h.ctx);
    assert_eq!(t.surface.panel, light.surface.panel);
    assert_eq!(t.resolved().spec().base, "pop-light");
    assert_eq!(h.ctx.global_style().visuals.panel_fill, light.surface.panel);
    let light_kit = Kit::of(&h.ctx);
    assert!(!std::sync::Arc::ptr_eq(&dark_kit, &light_kit));
    assert_eq!(*h.state(), Some(light_kit.button.style(State::REST).fill));
    assert_ne!(h.state(), &Some(dark_kit.button.style(State::REST).fill));

    h.ctx.set_theme(egui::Theme::Dark);
    h.run();
    assert_eq!(Tokens::of(&h.ctx).surface.panel, dark.surface.panel);
    assert!(std::sync::Arc::ptr_eq(&Kit::of(&h.ctx), &dark_kit));
}

#[test]
fn a_pair_follows_the_system_when_asked() {
    let mut h = button_harness();
    let light = theme::install(
        &h.ctx,
        &ThemeSpec::new("github-light").density(Density::Compact),
    );
    theme::set_preference(&h.ctx, egui::ThemePreference::System);
    // The system says nothing: the theme picked.
    h.run();
    assert_eq!(Tokens::of(&h.ctx).surface.panel, light.surface.panel);
    h.input_mut().system_theme = Some(egui::Theme::Dark);
    h.run();
    let t = Tokens::of(&h.ctx);
    assert_eq!(t.resolved().spec().base, "github-dark");
    // The spec's density applies to the counterpart too.
    assert_eq!(t.resolved().spec().density, Density::Compact);
    assert_eq!(h.ctx.global_style().visuals.panel_fill, t.surface.panel);
    assert!(h.ctx.global_style().visuals.dark_mode);
    h.input_mut().system_theme = Some(egui::Theme::Light);
    h.run();
    assert_eq!(Tokens::of(&h.ctx).surface.panel, light.surface.panel);
}

#[test]
fn a_theme_alone_keeps_its_mode_whatever_is_asked() {
    let mut h = button_harness();
    let high = theme::install(&h.ctx, &ThemeSpec::new("pop-high-contrast"));
    assert!(!theme::is_paired(&h.ctx));
    for preference in [
        egui::ThemePreference::Light,
        egui::ThemePreference::System,
        egui::ThemePreference::Dark,
    ] {
        theme::set_preference(&h.ctx, preference);
        h.input_mut().system_theme = Some(egui::Theme::Light);
        h.run();
        assert_eq!(h.ctx.theme(), egui::Theme::Dark, "{preference:?}");
        assert_eq!(Tokens::of(&h.ctx).surface.panel, high.surface.panel);
    }
    // Even egui asked directly: both styles are the theme's.
    h.ctx.set_theme(egui::Theme::Light);
    h.run();
    assert_eq!(Tokens::of(&h.ctx).surface.panel, high.surface.panel);
    assert_eq!(h.ctx.global_style().visuals.panel_fill, high.surface.panel);
    // A paired theme asked alone is alone too.
    theme::install(&h.ctx, &ThemeSpec::new("pop-light").alone());
    assert!(!theme::is_paired(&h.ctx));
    // An explicit pair.
    theme::install(
        &h.ctx,
        &ThemeSpec::pair("pop-high-contrast", "github-light"),
    );
    assert!(theme::is_paired(&h.ctx));
    h.ctx.set_theme(egui::Theme::Light);
    assert_eq!(Tokens::of(&h.ctx).resolved().spec().base, "github-light");
}
