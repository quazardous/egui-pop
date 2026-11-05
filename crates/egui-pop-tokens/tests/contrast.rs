//! Contrast: every built-in theme, in every density, keeps its text and
//! its controls legible (WCAG 2.x).

use egui_pop_tokens::contrast::{self, Level};
use egui_pop_tokens::{BUILT_IN_ACCENTS, BUILT_IN_THEMES, Density, ThemeSpec, Themes};

fn level(base: &str) -> Level {
    if base.contains("high-contrast") {
        Level::High
    } else {
        Level::Normal
    }
}

#[egui_pop_test_macros::critical]
#[test]
fn every_built_in_theme_and_density_passes_its_contrast_pairs() {
    let themes = Themes::default();
    let mut failures = Vec::new();
    for base in BUILT_IN_THEMES {
        for density in Density::ALL {
            let spec = ThemeSpec::new(*base).density(density);
            let tokens = themes.resolve(&spec);
            for f in contrast::check(&tokens, level(base)) {
                failures.push(format!("{}: {f}", spec.label()));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_accent_keeps_aa_contrast_in_every_theme() {
    // High contrast is checked at AA here: its 7:1 text applies to its own
    // accent; other accents are a user's choice.
    let themes = Themes::default();
    let mut failures = Vec::new();
    for base in BUILT_IN_THEMES {
        // The five built-in scales, and a theme's own (Catppuccin's purple).
        let mut accents: Vec<String> = BUILT_IN_ACCENTS.iter().map(|a| a.to_string()).collect();
        for a in themes.accents(base) {
            if !accents.contains(&a) {
                accents.push(a);
            }
        }
        for accent in accents {
            let spec = ThemeSpec::new(*base).accent(accent);
            let tokens = themes.resolve(&spec);
            for f in contrast::check(&tokens, Level::Normal) {
                failures.push(format!("{}: {f}", spec.label()));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_theme_with_a_poor_pair_fails_the_check() {
    let mut themes = Themes::default();
    themes
        .add_text(
            "murky",
            r##"
            $extends: "pop-dark"
            text:
              $type: "color"
              muted: {$value: "#3a3a3a"}
            "##,
        )
        .unwrap();
    let tokens = themes.resolve(&ThemeSpec::new("murky"));
    let failures = contrast::check(&tokens, Level::Normal);
    assert!(!failures.is_empty());
    assert!(
        // The muted text, the editor and field tokens aliasing it, and
        // the badge text on the muted badge's fill, the meta text role.
        failures.iter().all(|f| f.pair.foreground.contains("muted")
            || f.pair.background.contains("muted")
            || [
                "editor.line-number",
                "syntax.comment",
                "field.tone.muted.text",
                "text-role.meta.color"
            ]
            .contains(&f.pair.foreground)),
        "{failures:?}"
    );
    assert!(
        failures[0].to_string().contains(":1, needs 4.5:1"),
        "{}",
        failures[0]
    );
}

#[test]
fn an_accent_from_any_seed_keeps_aa_contrast_in_every_theme() {
    // Seeds round the hue circle, vivid and muted, light and dark.
    use egui_pop_tokens::Color;
    use egui_pop_tokens::palette::Okhsl;
    let themes = Themes::default();
    let mut failures = Vec::new();
    let mut seeds = Vec::new();
    for h in (0..360).step_by(30) {
        for (s, l) in [(1.0, 0.55), (0.5, 0.35), (0.8, 0.8)] {
            seeds.push(Okhsl { h: h as f32, s, l }.to_color());
        }
    }
    seeds.push(Color::from_hex("#808080").unwrap());
    for base in BUILT_IN_THEMES {
        for seed in &seeds {
            let spec = ThemeSpec::new(*base).accent_color(*seed);
            let tokens = themes.resolve(&spec);
            assert!(tokens.problems().is_empty(), "{:?}", tokens.problems());
            for f in contrast::check(&tokens, Level::Normal) {
                failures.push(format!("{}: {f}", spec.label()));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_pair_generates_each_variant_s_ramp_from_the_same_seed() {
    use egui_pop_tokens::Color;
    let themes = Themes::default();
    let spec = ThemeSpec::new("pop-dark").accent_color(Color::from_hex("#2e64a6").unwrap());
    let dark = themes.resolve(&spec);
    let light = themes.resolve_counterpart(&spec).unwrap();
    let solid = |t: &egui_pop_tokens::ResolvedTokens| t.color("accent.default").unwrap();
    assert!(!solid(&dark).is_dark());
    assert!(solid(&light).is_dark());
    assert!(dark.color("custom.1").unwrap().is_dark());
    assert!(!light.color("custom.1").unwrap().is_dark());
    // Not a colour: reported, the theme's own accent kept.
    let bad = themes.resolve(&ThemeSpec::new("pop-dark").accent("#zzz"));
    assert_eq!(bad.problems().len(), 1, "{:?}", bad.problems());
}

#[egui_pop_test_macros::critical]
#[test]
fn every_built_in_theme_tells_its_category_slots_apart() {
    let themes = Themes::default();
    let mut failures = Vec::new();
    for base in BUILT_IN_THEMES {
        let tokens = themes.resolve(&ThemeSpec::new(*base));
        for f in contrast::check_category(&tokens) {
            failures.push(format!("{base}: {f}"));
        }
        // Not grey: a slot carries a hue (OKLCH chroma 0.08 at least).
        for path in egui_pop_tokens::CATEGORY {
            let [_, a, b] = tokens.color(path).unwrap().to_oklab();
            let chroma = a.hypot(b);
            if chroma < 0.08 {
                failures.push(format!("{base}: {path} is greyish (chroma {chroma:.3})"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn delta_e_measures_oklab_distance_and_simulates_colour_blindness() {
    use contrast::Vision;
    use egui_pop_tokens::Color;
    let c = |h: &str| Color::from_hex(h).unwrap();
    // Black to white is the whole lightness axis.
    let d = contrast::delta_e(Color::BLACK, Color::WHITE, Vision::Normal);
    assert!((d - 100.0).abs() < 0.5, "{d}");
    // A pure red and a pure green, far apart for most, close without red
    // or green cones.
    let (red, green) = (c("#d03030"), c("#30a030"));
    let normal = contrast::delta_e(red, green, Vision::Normal);
    assert!(normal > 30.0, "{normal}");
    assert!(contrast::delta_e(red, green, Vision::Protanopia) < normal * 0.7);
    assert!(contrast::delta_e(red, green, Vision::Deuteranopia) < normal * 0.2);
    // Greys are seen alike.
    let grey = c("#808080");
    for v in Vision::ALL {
        let s = v.simulate(grey).to_srgba_u8();
        assert!(
            s[0].abs_diff(128) <= 1 && s[1].abs_diff(128) <= 1,
            "{v:?} {s:?}"
        );
    }
}

#[test]
fn a_theme_with_two_close_slots_fails_the_category_check() {
    let mut themes = Themes::default();
    themes
        .add_text(
            "twins",
            r##"
            $extends: "pop-dark"
            palette:
              category:
                $type: "color"
                "4": {$value: "{palette.category.3}"}
            "##,
        )
        .unwrap();
    let tokens = themes.resolve(&ThemeSpec::new("twins"));
    let failures = contrast::check_category(&tokens);
    assert!(failures.iter().any(|f| f.slots == (3, 4)), "{failures:?}");
    assert!(failures[0].to_string().contains("needs"), "{}", failures[0]);
}
