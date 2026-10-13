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
        .add_json(
            "murky",
            r##"{ "$extends": "pop-dark", "text": { "$type": "color", "muted": { "$value": "#3a3a3a" } } }"##,
        )
        .unwrap();
    let tokens = themes.resolve(&ThemeSpec::new("murky"));
    let failures = contrast::check(&tokens, Level::Normal);
    assert!(!failures.is_empty());
    assert!(
        // The muted text, and the editor tokens aliasing it.
        failures.iter().all(|f| f.pair.foreground.contains("muted")
            || ["editor.line-number", "syntax.comment"].contains(&f.pair.foreground)),
        "{failures:?}"
    );
    assert!(
        failures[0].to_string().contains(":1, needs 4.5:1"),
        "{}",
        failures[0]
    );
}
