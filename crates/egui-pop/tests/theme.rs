//! Installing a theme changes egui's style as the tokens say, and egui's
//! widgets are drawn with it (rendered on wgpu by egui_kittest).

use egui::{Color32, TextStyle};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::theme::{self, BUILT_IN_THEMES, Density, ThemeSpec, Tokens};
use egui_pop::tokens::Themes;

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
        assert_eq!(v.widgets.inactive.bg_fill, t.button.background, "{base}");
        assert_eq!(
            v.widgets.hovered.bg_fill, t.button.background_hover,
            "{base}"
        );
        assert_eq!(
            v.widgets.active.bg_fill, t.button.background_active,
            "{base}"
        );
        assert_eq!(v.widgets.active.bg_stroke.color, t.focus_ring, "{base}");
        assert_eq!(v.widgets.inactive.fg_stroke.color, t.button.text, "{base}");
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
        // Both of egui's styles are the theme's.
        ctx.set_theme(egui::Theme::Light);
        assert_eq!(
            ctx.global_style().visuals.panel_fill,
            t.surface.panel,
            "{base}"
        );
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
    let mut themes = Themes::default();
    themes
        .add_json(
            "broken",
            r#"{ "$extends": "pop-dark", "surface": { "$type": "color", "panel": { "$value": "{nope}" } } }"#,
        )
        .unwrap();
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
            .wgpu()
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
