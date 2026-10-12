//! Themes: built-in ones are complete, aliases and `$extends` resolve,
//! overlays touch what they should, problems fall back instead of breaking.

use egui_pop_tokens::{
    BUILT_IN_ACCENTS, BUILT_IN_THEMES, CONTRACT, Color, Density, Error, POP_THEMES, ThemeSpec,
    Themes, Tier, TokenFile, Value,
};
use serde_json::json;

fn all_specs() -> impl Iterator<Item = ThemeSpec> {
    BUILT_IN_THEMES.iter().flat_map(|base| {
        Density::ALL
            .into_iter()
            .map(move |d| ThemeSpec::new(*base).density(d))
    })
}

#[test]
fn built_in_themes_answer_the_whole_contract_cleanly() {
    let themes = Themes::default();
    for spec in all_specs() {
        let r = themes.resolve(&spec);
        assert!(
            r.problems().is_empty(),
            "{}: {:?}",
            spec.label(),
            r.problems()
        );
        for req in CONTRACT {
            let t = r
                .get(req.path)
                .unwrap_or_else(|| panic!("{}: {}", spec.label(), req.path));
            assert_eq!(t.ty, req.ty, "{}", req.path);
            assert!(!t.fallback, "{}", req.path);
            assert_eq!(t.tier(), req.tier);
        }
    }
}

#[test]
fn system_and_component_tokens_are_aliases_of_the_tier_below() {
    let themes = Themes::default();
    let r = themes.resolve(&ThemeSpec::default());
    for req in CONTRACT.iter().filter(|r| r.ty == "color") {
        let token = r.get(req.path).unwrap();
        let target = token
            .alias
            .as_deref()
            .unwrap_or_else(|| panic!("{} is a literal", req.path));
        let target_tier = r.get(target).unwrap().tier();
        match req.tier {
            // A role names the palette, or another role (`focus.ring`).
            Tier::System => assert_ne!(target_tier, Tier::Component, "{}", req.path),
            Tier::Component => assert_eq!(target_tier, Tier::System, "{}", req.path),
            Tier::Reference => unreachable!(),
        }
    }
}

#[test]
fn aliases_follow_through_extends_chains() {
    let mut themes = Themes::default();
    // A third level: mine → pop-light → pop-dark.
    themes
        .add_json(
            "mine",
            r##"{ "$extends": "pop-light", "gray": { "$type": "color", "2": { "$value": "#ffffff" } } }"##,
        )
        .unwrap();
    let mine = themes.resolve(&ThemeSpec::new("mine"));
    assert!(mine.problems().is_empty(), "{:?}", mine.problems());
    // surface.panel is declared in pop-dark as {gray.2}; gray.2 comes from mine.
    assert_eq!(mine.color("surface.panel"), Some(Color::WHITE));
    assert_eq!(
        mine.get("surface.panel").unwrap().alias.as_deref(),
        Some("gray.2")
    );
    // pop-light's own overrides stay.
    let light = themes.resolve(&ThemeSpec::new("pop-light"));
    assert_eq!(
        mine.color("surface.overlay"),
        light.color("surface.overlay")
    );
    // Component → system → reference, across files.
    assert_eq!(mine.color("button.background"), light.color("gray.4"));
}

#[test]
fn an_alias_cycle_is_reported_and_falls_back() {
    let mut themes = Themes::default();
    themes
        .add_json(
            "loop",
            r#"{ "$extends": "pop-dark",
                 "text": { "$type": "color",
                           "default": { "$value": "{text.muted}" },
                           "muted": { "$value": "{text.default}" } } }"#,
        )
        .unwrap();
    let r = themes.resolve(&ThemeSpec::new("loop"));
    let dark = themes.resolve(&ThemeSpec::default());
    assert!(
        r.problems()
            .iter()
            .any(|p| matches!(p, Error::AliasCycle { chain }
            if chain == &["text.default", "text.muted", "text.default"])),
        "{:?}",
        r.problems()
    );
    assert_eq!(r.color("text.default"), dark.color("text.default"));
    assert!(r.get("text.default").unwrap().fallback);
    // Something aliasing into the cycle is broken too, and falls back.
    assert!(r.get("button.text").unwrap().fallback);
}

#[test]
fn an_unknown_alias_is_reported_with_its_target() {
    let mut themes = Themes::default();
    themes
        .add_json(
            "typo",
            r#"{ "$extends": "pop-dark", "danger": { "$type": "color", "$value": "{redd.11}" } }"#,
        )
        .unwrap();
    let r = themes.resolve(&ThemeSpec::new("typo"));
    assert_eq!(
        r.problems(),
        [Error::UnknownAlias {
            path: "danger".into(),
            target: "redd.11".into()
        }]
    );
    assert_eq!(
        r.problems()[0].to_string(),
        "`danger` refers to `{redd.11}`, which is not a token"
    );
    assert_eq!(
        r.color("danger"),
        themes.resolve(&ThemeSpec::default()).color("danger")
    );
}

#[test]
fn extends_problems_fall_back_to_pop_dark() {
    let mut themes = Themes::default();
    themes
        .add_json("orphan", r##"{ "$extends": "nowhere", "gray": { "$type": "color", "0": { "$value": "#000000" } } }"##)
        .unwrap();
    themes.add_json("a", r#"{ "$extends": "b" }"#).unwrap();
    themes.add_json("b", r#"{ "$extends": "a" }"#).unwrap();

    let orphan = themes.resolve(&ThemeSpec::new("orphan"));
    assert_eq!(
        orphan.problems(),
        [Error::UnknownTheme {
            name: "nowhere".into()
        }]
    );
    assert_eq!(orphan.color("surface.sunken"), Some(Color::BLACK));

    let cycle = themes.resolve(&ThemeSpec::new("a"));
    assert!(
        matches!(&cycle.problems()[0], Error::ExtendsCycle { chain } if chain == &["a", "b", "a"])
    );
    // Every contract token still has a value: from pop-dark, but for the
    // density overlay's own.
    assert!(cycle.get("surface.panel").is_some_and(|t| t.fallback));
    for req in CONTRACT {
        assert!(cycle.get(req.path).is_some(), "{}", req.path);
    }

    let unknown = themes.resolve(&ThemeSpec::new("no-such-theme"));
    assert_eq!(
        unknown.problems(),
        [Error::UnknownTheme {
            name: "no-such-theme".into()
        }]
    );
    assert!(unknown.get("surface.panel").is_some());
}

#[test]
fn missing_or_mistyped_contract_tokens_fall_back_and_say_why() {
    let mut themes = Themes::default();
    themes
        .add_json(
            "bare",
            r##"{ "surface": { "$type": "color", "panel": { "$value": "#102030" } },
                  "radius": { "widget": { "$type": "color", "$value": "#ffffff" } } }"##,
        )
        .unwrap();
    let r = themes.resolve(&ThemeSpec::new("bare"));
    let dark = themes.resolve(&ThemeSpec::default());
    assert_eq!(r.color("surface.panel"), Color::from_hex("#102030"));
    assert!(r.problems().contains(&Error::Missing {
        path: "text.default".into()
    }));
    assert!(r.problems().contains(&Error::TypeMismatch {
        path: "radius.widget".into(),
        expected: "dimension".into(),
        found: "color".into(),
    }));
    assert_eq!(
        r.dimension("radius.widget"),
        dark.dimension("radius.widget")
    );
    assert_eq!(r.color("text.default"), dark.color("text.default"));
    for req in CONTRACT {
        assert_eq!(
            r.get(req.path).map(|t| t.ty.as_str()),
            Some(req.ty),
            "{}",
            req.path
        );
    }
}

#[test]
fn a_density_overlay_only_touches_space_and_size() {
    let themes = Themes::default();
    for density in Density::ALL {
        let file = density.built_in_file();
        for path in file.tokens.keys() {
            assert!(
                path.starts_with("space.") || path.starts_with("size."),
                "{density} touches {path}"
            );
        }
    }
    for base in BUILT_IN_THEMES {
        let comfortable = themes.resolve(&ThemeSpec::new(*base));
        let compact = themes.resolve(&ThemeSpec::new(*base).density(Density::Compact));
        let mut changed = 0;
        for token in comfortable.iter() {
            let other = compact.get(&token.path).unwrap();
            if other.value != token.value {
                changed += 1;
                // A component token may change through its alias
                // (dialog.padding is {space.l}), never a colour.
                assert!(
                    matches!(token.value, Value::Dimension(_)),
                    "{base}: {} changed",
                    token.path
                );
                let root = token.alias.as_deref().unwrap_or(&token.path);
                assert!(
                    root.starts_with("space.")
                        || root.starts_with("size.")
                        || token.path.starts_with("space.")
                        || token.path.starts_with("size."),
                    "{base}: {} changed",
                    token.path
                );
            }
        }
        assert!(changed > 0, "{base}: compact changes nothing");
        assert!(compact.dimension("space.m") < comfortable.dimension("space.m"));
    }
}

#[test]
fn the_comfortable_overlay_restates_the_base_values() {
    let themes = Themes::default();
    let file = Density::Comfortable.built_in_file();
    let dark = themes.resolve(&ThemeSpec::default());
    let mut bare = Themes::default();
    bare.set_density(Density::Comfortable, TokenFile::new("none"));
    let without = bare.resolve(&ThemeSpec::default());
    for path in file.tokens.keys() {
        assert_eq!(dark.value(path), without.value(path), "{path}");
    }
}

#[test]
fn an_accent_re_aliases_the_accent_roles_only() {
    let themes = Themes::default();
    // egui-pop's own themes: gold is their accent and they have the five
    // scales exactly (the editor-inspired ones are checked below).
    for base in POP_THEMES {
        let plain = themes.resolve(&ThemeSpec::new(*base));
        let blue = themes.resolve(&ThemeSpec::new(*base).accent("blue"));
        assert!(blue.problems().is_empty(), "{:?}", blue.problems());
        assert_eq!(blue.color("accent.default"), blue.color("blue.9"));
        assert_eq!(blue.color("text.on-accent"), blue.color("blue.contrast"));
        // Aliases of the accent follow it.
        assert_eq!(blue.color("focus.ring"), blue.color("blue.9"));
        assert_eq!(blue.color("slider.fill"), blue.color("blue.9"));
        // Everything else is untouched.
        assert_eq!(blue.color("surface.panel"), plain.color("surface.panel"));
        assert_eq!(blue.color("danger"), plain.color("danger"));
        // The default accent is gold: naming it changes nothing.
        let gold = themes.resolve(&ThemeSpec::new(*base).accent("gold"));
        assert_eq!(gold.color("accent.default"), plain.color("accent.default"));
        assert_eq!(themes.accents(base), BUILT_IN_ACCENTS);
    }
    let bad = themes.resolve(&ThemeSpec::default().accent("gray"));
    assert_eq!(
        bad.problems(),
        [Error::UnknownAccent {
            name: "gray".into()
        }]
    );
    assert_eq!(
        bad.color("accent.default"),
        themes
            .resolve(&ThemeSpec::default())
            .color("accent.default")
    );
}

#[test]
fn overrides_win_and_unknown_ones_are_reported() {
    let themes = Themes::default();
    let spec = ThemeSpec::default()
        .override_token("accent.default", json!("#3080ff"))
        .override_token("danger", json!("{blue.11}"))
        .override_token("acent", json!("#ff0000"));
    let r = themes.resolve(&spec);
    assert_eq!(r.color("accent.default"), Color::from_hex("#3080ff"));
    assert_eq!(r.color("focus.ring"), Color::from_hex("#3080ff"));
    assert_eq!(r.color("danger"), r.color("blue.11"));
    assert_eq!(
        r.problems(),
        [Error::UnknownOverride {
            path: "acent".into()
        }]
    );
    // A bad override value falls back for that token (and what aliases it).
    let r = themes.resolve(&ThemeSpec::default().override_token("radius.widget", json!("wide")));
    assert!(
        r.problems()
            .iter()
            .any(|p| matches!(p, Error::InvalidValue { path, .. } if path == "radius.widget")),
        "{:?}",
        r.problems()
    );
    assert_eq!(r.dimension("radius.widget"), Some(4.0));
}

#[test]
fn reduced_motion_zeroes_the_duration() {
    let themes = Themes::default();
    assert_eq!(
        themes
            .resolve(&ThemeSpec::default())
            .duration_ms("motion.duration"),
        Some(120.0)
    );
    let reduced = themes.resolve(&ThemeSpec::default().reduced_motion(true));
    assert_eq!(reduced.duration_ms("motion.duration"), Some(0.0));
}

#[test]
fn extensions_and_descriptions_are_preserved() {
    let mut themes = Themes::default();
    themes
        .add_json(
            "ext",
            r##"{ "$extends": "pop-dark",
                  "$extensions": { "org.example.tool": { "id": 7 } },
                  "danger": { "$type": "color", "$value": "#ff0000",
                              "$extensions": { "org.example.tool": { "locked": true } } } }"##,
        )
        .unwrap();
    assert_eq!(
        themes.get("ext").unwrap().extensions,
        Some(json!({ "org.example.tool": { "id": 7 } }))
    );
    let r = themes.resolve(&ThemeSpec::new("ext"));
    let danger = r.get("danger").unwrap();
    assert_eq!(
        danger.extensions,
        Some(json!({ "org.example.tool": { "locked": true } }))
    );
    // The description of what is overridden stays when the override gives none.
    assert!(
        danger
            .description
            .as_deref()
            .is_some_and(|d| d.contains("Errors"))
    );
    assert_eq!(
        r.to_dtcg()["danger"]["$extensions"]["org.example.tool"]["locked"],
        json!(true)
    );
}

#[test]
fn an_exported_theme_reads_back_the_same() {
    let themes = Themes::default();
    let spec = ThemeSpec::new("pop-light")
        .density(Density::Compact)
        .accent("green")
        .override_token("danger", json!("#ff0000cc"));
    let r = themes.resolve(&spec);
    let mut again = Themes::default();
    again
        .add_json("exported", &r.to_dtcg().to_string())
        .unwrap();
    // Read back without the compact overlay: the file already holds its values.
    let mut no_density = again.clone();
    no_density.set_density(Density::Comfortable, TokenFile::new("none"));
    let back = no_density.resolve(&ThemeSpec::new("exported"));
    assert!(back.problems().is_empty(), "{:?}", back.problems());
    for token in r.iter() {
        let b = back.get(&token.path).unwrap();
        match (&token.value, &b.value) {
            // Hex output rounds colours to 8 bits.
            (Value::Color(x), Value::Color(y)) => {
                assert_eq!(x.to_hex(), y.to_hex(), "{}", token.path)
            }
            (x, y) => assert_eq!(x, y, "{}", token.path),
        }
    }
}

#[test]
fn every_shipped_theme_resolves_from_its_own_files() {
    let themes = Themes::default();
    for base in BUILT_IN_THEMES {
        for density in Density::ALL {
            let spec = ThemeSpec::new(*base).density(density);
            let t = themes.resolve(&spec);
            assert!(
                t.problems().is_empty(),
                "{}: {:?}",
                spec.label(),
                t.problems()
            );
            for r in egui_pop_tokens::CONTRACT {
                let token = t
                    .get(r.path)
                    .unwrap_or_else(|| panic!("{base}: no {}", r.path));
                assert!(
                    !token.fallback,
                    "{base}: {} comes from the fallback",
                    r.path
                );
            }
        }
        // Every built-in accent works in every theme, and the theme's own
        // extra scales (Catppuccin's purple) are offered too.
        let accents = themes.accents(base);
        for a in BUILT_IN_ACCENTS {
            assert!(accents.iter().any(|x| x == a), "{base}: no {a} accent");
        }
        for a in &accents {
            let t = themes.resolve(&ThemeSpec::new(*base).accent(a.clone()));
            assert!(t.problems().is_empty(), "{base} {a}: {:?}", t.problems());
            assert_eq!(t.color("accent.default"), t.color(&format!("{a}.9")));
        }
    }
    assert!(
        themes
            .accents("catppuccin-mocha")
            .iter()
            .any(|a| a == "purple")
    );
}

#[test]
fn the_crate_ships_the_repositorys_notice() {
    assert_eq!(
        include_str!("../NOTICE"),
        include_str!("../../../NOTICE"),
        "crates/egui-pop-tokens/NOTICE is a copy of NOTICE"
    );
}
