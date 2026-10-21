//! Themes: built-in ones are complete, aliases and `$extends` resolve,
//! overlays touch what they should, problems fall back instead of breaking.

use egui_pop_tokens::{
    BUILT_IN_ACCENTS, BUILT_IN_THEMES, Color, Density, Error, POP_THEMES, ThemeSpec, Themes, Tier,
    TokenFile, Value, Warning, contract,
};
use serde_json::json;

/// Adds a theme file as written, without the loading rule of
/// `Themes::add_text` (which leaves broken tokens out).
fn add_raw(themes: &mut Themes, name: &str, text: &str) {
    themes.add(TokenFile::parse(name, text).unwrap());
}

fn all_specs() -> impl Iterator<Item = ThemeSpec> {
    BUILT_IN_THEMES.iter().flat_map(|base| {
        Density::ALL
            .into_iter()
            .map(move |d| ThemeSpec::new(*base).density(d))
    })
}

#[egui_pop_test_macros::critical]
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
        for req in contract() {
            let t = r
                .get(req.path)
                .unwrap_or_else(|| panic!("{}: {}", spec.label(), req.path));
            assert_eq!(t.ty, req.ty, "{}", req.path);
            assert!(!t.fallback, "{}", req.path);
            assert_eq!(t.tier(), req.tier);
        }
    }
}

#[egui_pop_test_macros::critical]
#[test]
fn system_and_component_tokens_are_aliases_of_the_tier_below() {
    let themes = Themes::default();
    let r = themes.resolve(&ThemeSpec::default());
    // The one alias across the component tier: a component based on
    // another (`<component>.based-on`) receives its base's token under the
    // same term, followed here to the token the base gives.
    let inherited = |path: &str, target: &str| {
        let (component, term) = path.split_once('.')?;
        let base = match r.value(&format!("{component}.based-on")) {
            Some(Value::Other(serde_json::Value::String(b))) => b.clone(),
            _ => return None,
        };
        (target == format!("{base}.{term}")).then_some(())
    };
    for req in contract().filter(|r| r.ty == "color") {
        let mut path = req.path.to_string();
        let mut token = r.get(&path).unwrap();
        let mut target = token
            .alias
            .clone()
            .unwrap_or_else(|| panic!("{} is a literal", req.path));
        while inherited(&path, &target).is_some() {
            path = target.clone();
            token = r.get(&path).unwrap();
            target = token
                .alias
                .clone()
                .unwrap_or_else(|| panic!("{path} (the base of {}) is a literal", req.path));
        }
        let target_tier = r.get(&target).unwrap().tier();
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
        .add_text(
            "mine",
            r##"
            $extends: "pop-light"
            gray:
              $type: "color"
              "2": {$value: "#ffffff"}
            "##,
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
    assert_eq!(mine.color("button.rest.fill"), light.color("gray.4"));
}

#[test]
fn an_alias_cycle_is_reported_and_falls_back() {
    let mut themes = Themes::default();
    // Added as written, past the loading rule: what the resolver does
    // with a broken token.
    add_raw(
        &mut themes,
        "loop",
        r#"
            $extends: "pop-dark"
            text:
              $type: "color"
              default: {$value: "{text.muted}"}
              muted: {$value: "{text.default}"}
            "#,
    );
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
    assert!(r.get("button.rest.text").unwrap().fallback);
}

#[test]
fn an_unknown_alias_is_reported_with_its_target() {
    let mut themes = Themes::default();
    // Added as written, past the loading rule: what the resolver does
    // with a broken token.
    add_raw(
        &mut themes,
        "typo",
        r#"
            $extends: "pop-dark"
            danger: {$type: "color", $value: "{redd.11}"}
            "#,
    );
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
        .add_text(
            "orphan",
            r##"
                            $extends: "nowhere"
                            gray:
                              $type: "color"
                              "0": {$value: "#000000"}
                            "##,
        )
        .unwrap();
    themes
        .add_text(
            "a",
            r#"
                         $extends: "b"
                         "#,
        )
        .unwrap();
    themes
        .add_text(
            "b",
            r#"
                         $extends: "a"
                         "#,
        )
        .unwrap();

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
    for req in contract() {
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
    // Added as written, past the loading rule: what the resolver does
    // with a broken token.
    add_raw(
        &mut themes,
        "bare",
        r##"
            surface:
              $type: "color"
              panel: {$value: "#102030"}
            radius:
              widget: {$type: "color", $value: "#ffffff"}
            "##,
    );
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
    for req in contract() {
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
                // Followed to the end of its aliases (a kit radius is its
                // height, which is {space.m}).
                let mut root = token.path.as_str();
                while let Some(next) = comfortable.get(root).and_then(|t| t.alias.as_deref()) {
                    root = next;
                }
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
fn spacing_by_intent_is_a_step_of_the_scale_in_every_density() {
    let themes = Themes::default();
    for base in BUILT_IN_THEMES {
        for density in Density::ALL {
            let r = themes.resolve(&ThemeSpec::new(*base).density(density));
            for (intent, step) in [
                ("inline", "s"),
                ("group", "m"),
                ("section", "l"),
                ("page", "xl"),
            ] {
                let path = format!("space.{intent}");
                assert_eq!(
                    r.get(&path).and_then(|t| t.alias.as_deref()),
                    Some(format!("space.{step}").as_str()),
                    "{base} {density}: {path}"
                );
                assert_eq!(
                    r.dimension(&path),
                    r.dimension(&format!("space.{step}")),
                    "{base} {density}: {path}"
                );
            }
        }
    }
}

#[test]
fn text_roles_take_a_step_of_the_size_scale() {
    let themes = Themes::default();
    for base in BUILT_IN_THEMES {
        let r = themes.resolve(&ThemeSpec::new(*base));
        for role in ["title", "section", "body", "muted", "meta"] {
            let size = format!("text-role.{role}.size");
            let alias = r.get(&size).and_then(|t| t.alias.as_deref()).unwrap_or("");
            assert!(
                alias.starts_with("font.size."),
                "{base}: {size} is {alias:?}"
            );
            let weight = r.dimension(&format!("text-role.{role}.weight")).unwrap();
            assert!(
                (100.0..=900.0).contains(&weight),
                "{base}: {role} weight {weight}"
            );
        }
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
        assert_eq!(blue.color("slider-fill.rest.fill"), blue.color("blue.9"));
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
        .add_text(
            "ext",
            r##"
            $extends: "pop-dark"
            $extensions:
              org.example.tool: {id: 7}
            danger: {$type: "color", $value: "#ff0000", $extensions: {org.example.tool: {locked: true}}}
            "##,
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
        .add_text("exported", &r.to_dtcg().to_string())
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
            for r in egui_pop_tokens::contract() {
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

#[test]
fn control_and_icon_sizes_are_multiples_of_four_points() {
    // Whole pixels at every quarter scale: 4 x 1.25 = 5, 4 x 1.75 = 7.
    let themes = Themes::default();
    for spec in all_specs() {
        let r = themes.resolve(&spec);
        for path in [
            "size.control.height",
            "size.icon.inline",
            "size.icon.toolbar",
            "size.icon.panel",
            "size.title-bar.height",
            "size.window-button.width",
        ] {
            let v = r.dimension(path).expect(path);
            assert_eq!(v % 4.0, 0.0, "{} {path}: {v}", spec.label());
        }
    }
}

#[test]
fn an_alpha_modifier_multiplies_the_aliased_colours_alpha() {
    let mut themes = Themes::default();
    themes
        .add_text(
            "alpha",
            r##"
            $extends: "pop-dark"
            veil:
              $type: "color"
              plain: {$value: "{surface.overlay}", $extensions: {org.egui-pop.alpha: 0.85}}
              half: {$value: "#ff000080", $extensions: {org.egui-pop.alpha: 0.5}}
              # an alias of a modified alias keeps its modifier, and adds its own
              through: {$value: "{veil.plain}"}
              twice: {$value: "{veil.plain}", $extensions: {org.egui-pop.alpha: 0.5, org.example: 1}}
            "##,
        )
        .unwrap();
    let r = themes.resolve(&ThemeSpec::new("alpha"));
    assert!(r.problems().is_empty(), "{:?}", r.problems());
    let overlay = r.color("surface.overlay").unwrap();
    let plain = r.color("veil.plain").unwrap();
    assert_eq!(
        (plain.r, plain.g, plain.b),
        (overlay.r, overlay.g, overlay.b)
    );
    assert!((plain.a - 0.85 * overlay.a).abs() < 1e-6);
    assert_eq!(
        r.get("veil.plain").unwrap().alias.as_deref(),
        Some("surface.overlay")
    );
    assert!((r.color("veil.half").unwrap().a - 0.25).abs() < 0.01);
    assert_eq!(r.color("veil.through"), Some(plain));
    assert!((r.color("veil.twice").unwrap().a - 0.425 * overlay.a).abs() < 1e-6);
    // Exported, the value holds the alpha and the modifier is gone (it
    // would apply twice); other extensions stay.
    let out = r.to_dtcg();
    assert_eq!(out["veil"]["plain"].get("$extensions"), None);
    assert_eq!(
        out["veil"]["twice"]["$extensions"],
        json!({"org.example": 1})
    );
    let mut again = Themes::default();
    again.add_text("back", &out.to_string()).unwrap();
    let back = again.resolve(&ThemeSpec::new("back"));
    assert_eq!(
        back.color("veil.twice").unwrap().to_hex(),
        r.color("veil.twice").unwrap().to_hex()
    );
}

#[test]
fn a_bad_alpha_modifier_is_reported() {
    let mut themes = Themes::default();
    // Added as written, past the loading rule: what the resolver does
    // with a broken token.
    add_raw(
        &mut themes,
        "bad-alpha",
        r##"
            $extends: "pop-dark"
            veil:
              over: {$type: "color", $value: "{surface.overlay}", $extensions: {org.egui-pop.alpha: 1.5}}
              word: {$type: "color", $value: "{surface.overlay}", $extensions: {org.egui-pop.alpha: "half"}}
              wide: {$type: "dimension", $value: "{radius.widget}", $extensions: {org.egui-pop.alpha: 0.5}}
              after: {$type: "color", $value: "{veil.over}"}
            "##,
    );
    let r = themes.resolve(&ThemeSpec::new("bad-alpha"));
    let said: Vec<String> = r.problems().iter().map(ToString::to_string).collect();
    for path in ["veil.over", "veil.word", "veil.wide"] {
        assert!(
            r.problems()
                .iter()
                .any(|p| matches!(p, Error::InvalidModifier { path: p, .. } if p == path)),
            "{path}: {said:?}"
        );
        assert!(r.get(path).is_none(), "{path}");
    }
    // Reported once, not again for what aliases it.
    assert_eq!(said.len(), 3, "{said:?}");
    assert!(r.get("veil.after").is_none());
    assert!(said.iter().any(|p| p.contains("not a number from 0 to 1")));
    assert!(
        said.iter()
            .any(|p| p.contains("is for colours, this is a dimension"))
    );
}

#[test]
fn an_override_replaces_the_alpha_modifier_with_its_value() {
    let mut themes = Themes::default();
    themes
        .add_text(
            "veiled",
            r##"
            $extends: "pop-dark"
            veil: {$type: "color", $value: "{surface.overlay}", $extensions: {org.egui-pop.alpha: 0.5}}
            "##,
        )
        .unwrap();
    let r = themes.resolve(&ThemeSpec::new("veiled").override_token("veil", json!("#102030")));
    assert!(r.problems().is_empty(), "{:?}", r.problems());
    assert_eq!(r.color("veil"), Color::from_hex("#102030"));
}

#[test]
fn text_styles_are_steps_of_the_size_scale_with_todays_sizes() {
    let themes = Themes::default();
    let r = themes.resolve(&ThemeSpec::new("pop-dark"));
    let d = |p: &str| r.dimension(p).expect(p);
    let scale: Vec<f32> = ["xs", "s", "m", "l", "xl"]
        .iter()
        .map(|s| d(&format!("font.size.{s}")))
        .collect();
    assert!(scale.windows(2).all(|w| w[0] < w[1]), "{scale:?}");
    // The sizes egui's text styles had before the scale.
    assert_eq!(d("font.text-style.small"), 10.0);
    assert_eq!(d("font.text-style.body"), 13.0);
    assert_eq!(d("font.text-style.button"), 13.0);
    assert_eq!(d("font.text-style.heading"), 18.0);
    assert_eq!(d("font.text-style.monospace"), 12.5);
    let hc = themes.resolve(&ThemeSpec::new("pop-high-contrast"));
    let h = |p: &str| hc.dimension(p).expect(p);
    assert_eq!(h("font.text-style.small"), 12.0);
    assert_eq!(h("font.text-style.body"), 15.0);
    assert_eq!(h("font.text-style.heading"), 18.0);
    assert_eq!(h("font.text-style.monospace"), 14.0);
    for theme in BUILT_IN_THEMES {
        let r = themes.resolve(&ThemeSpec::new(*theme));
        let scale: Vec<f32> = ["xs", "s", "m", "l", "xl"]
            .iter()
            .map(|s| r.dimension(&format!("font.size.{s}")).unwrap())
            .collect();
        assert!(scale.windows(2).all(|w| w[0] < w[1]), "{theme}: {scale:?}");
    }
}

#[test]
fn a_component_token_may_alias_any_step_and_a_step_moves_its_styles() {
    let mut themes = Themes::default();
    themes
        .add_text(
            "bigger",
            r##"
            $extends: "pop-dark"
            font:
              size:
                m: {$type: "dimension", $value: {value: 14, unit: "px"}}
            card:
              title-size: {$type: "dimension", $value: "{font.size.l}"}
            "##,
        )
        .unwrap();
    let r = themes.resolve(&ThemeSpec::new("bigger"));
    assert!(r.problems().is_empty(), "{:?}", r.problems());
    assert_eq!(r.dimension("card.title-size"), Some(15.0));
    assert_eq!(r.dimension("font.text-style.body"), Some(14.0));
    assert_eq!(r.dimension("font.text-style.button"), Some(14.0));
}

#[test]
fn a_former_token_name_is_no_longer_read() {
    // `titlebar.background` became `title-bar.rest.fill`: the former name
    // is an ordinary token nothing reads.
    let mut themes = Themes::default();
    themes
        .add_text(
            "old-names",
            r##"
            $extends: "pop-dark"
            titlebar:
              background: {$type: "color", $value: "#123456"}
            "##,
        )
        .unwrap();
    let r = themes.resolve(&ThemeSpec::new("old-names"));
    let default = themes.resolve(&ThemeSpec::default());
    assert!(r.problems().is_empty(), "{:?}", r.problems());
    assert_eq!(
        r.color("title-bar.rest.fill"),
        default.color("title-bar.rest.fill")
    );
    // In a kit, a former name of a kit component is outside the vocabulary.
    let kit = egui_pop_tokens::kit::KitData::from_text(
        "old",
        r##"
        button:
          background: {$type: "color", $value: "#123456"}
        "##,
    );
    let problems = egui_pop_tokens::kit::check(&kit);
    assert!(
        problems
            .iter()
            .any(|p| p.to_string().starts_with("`button.background`")),
        "{problems:?}"
    );
}

/// A theme over pop-dark with two components of its own, `base-box` and
/// `child-box`, the child based on the base.
const BASED: &str = r##"
$extends: "pop-dark"
base-box:
  shape: {$type: "string", $value: "box"}
  rest:
    fill: {$type: "color", $value: "{surface.raised}"}
    text: {$type: "color", $value: "{text.default}"}
  hover:
    fill: {$type: "color", $value: "{surface.hover}"}
  tone:
    danger:
      fill: {$type: "color", $value: "{danger}"}
child-box:
  based-on: {$type: "string", $value: "base-box"}
  rest:
    text: {$type: "color", $value: "#123456"}
"##;

#[test]
fn a_component_based_on_another_receives_what_it_does_not_give() {
    let mut themes = Themes::default();
    themes.add_text("based", BASED).unwrap();
    let r = themes.resolve(&ThemeSpec::new("based"));
    assert!(r.problems().is_empty(), "{:?}", r.problems());
    // Inherited: states, tones and component terms, as aliases of the base.
    for term in ["rest.fill", "hover.fill", "tone.danger.fill", "shape"] {
        let token = r.get(&format!("child-box.{term}")).expect(term);
        assert_eq!(token.alias.as_deref(), Some(&*format!("base-box.{term}")));
        assert_eq!(
            Some(&token.value),
            r.value(&format!("base-box.{term}")),
            "{term}"
        );
    }
    // Its own value wins.
    assert_eq!(r.color("child-box.rest.text"), Color::from_hex("#123456"));
    // The base names no base: nothing comes back to it.
    assert!(r.get("base-box.based-on").is_none());
}

#[test]
fn restyling_the_base_restyles_what_is_based_on_it() {
    let mut themes = Themes::default();
    themes.add_text("based", BASED).unwrap();
    themes
        .add_text(
            "restyled",
            r##"
            $extends: "based"
            base-box:
              hover:
                fill: {$type: "color", $value: "#00ff00"}
            "##,
        )
        .unwrap();
    let r = themes.resolve(&ThemeSpec::new("restyled"));
    assert_eq!(r.color("child-box.hover.fill"), Color::from_hex("#00ff00"));
    // An override of the base reaches it too; one of its own stays its own.
    let r = themes.resolve(
        &ThemeSpec::new("based")
            .override_token("base-box.rest.fill", json!("#0000ff"))
            .override_token("child-box.hover.fill", json!("#ff0000")),
    );
    assert!(r.problems().is_empty(), "{:?}", r.problems());
    assert_eq!(r.color("child-box.rest.fill"), Color::from_hex("#0000ff"));
    assert_eq!(r.color("child-box.hover.fill"), Color::from_hex("#ff0000"));
    assert_eq!(r.color("base-box.hover.fill"), r.color("surface.hover"));
}

#[test]
fn bases_chain_each_completed_before_what_is_based_on_it() {
    let mut themes = Themes::default();
    themes.add_text("based", BASED).unwrap();
    themes
        .add_text(
            "chain",
            r##"
            $extends: "based"
            # Named before its base in the file and in the map's order.
            a-grandchild:
              based-on: {$type: "string", $value: "child-box"}
              hover:
                text: {$type: "color", $value: "#654321"}
            "##,
        )
        .unwrap();
    let r = themes.resolve(&ThemeSpec::new("chain"));
    assert!(r.problems().is_empty(), "{:?}", r.problems());
    assert_eq!(
        r.color("a-grandchild.rest.text"),
        Color::from_hex("#123456")
    );
    assert_eq!(r.color("a-grandchild.rest.fill"), r.color("surface.raised"));
    assert_eq!(r.color("a-grandchild.tone.danger.fill"), r.color("danger"));
    assert_eq!(
        r.color("a-grandchild.hover.text"),
        Color::from_hex("#654321")
    );
}

#[test]
fn a_cycle_of_bases_is_reported_and_each_keeps_its_own_tokens() {
    let mut themes = Themes::default();
    // Added as written, past the loading rule: what the resolver does
    // with a broken token.
    add_raw(
        &mut themes,
        "cycle",
        r##"
            $extends: "pop-dark"
            one-box:
              based-on: {$type: "string", $value: "two-box"}
              rest:
                fill: {$type: "color", $value: "#111111"}
            two-box:
              based-on: {$type: "string", $value: "one-box"}
              rest:
                text: {$type: "color", $value: "#222222"}
            three-box:
              based-on: {$type: "string", $value: "one-box"}
            lost-box:
              based-on: {$type: "string", $value: "no-such-box"}
            "##,
    );
    let r = themes.resolve(&ThemeSpec::new("cycle"));
    let cycles: Vec<&Error> = r
        .problems()
        .iter()
        .filter(|p| matches!(p, Error::BasedOnCycle { .. }))
        .collect();
    assert_eq!(cycles.len(), 1, "{:?}", r.problems());
    assert!(
        cycles[0]
            .to_string()
            .contains("one-box → two-box → one-box")
    );
    assert!(
        r.problems()
            .iter()
            .any(|p| matches!(p, Error::UnknownBase { base, .. } if base == "no-such-box"))
    );
    // Each member of the cycle keeps its own tokens only.
    assert!(r.get("one-box.rest.text").is_none());
    assert!(r.get("two-box.rest.fill").is_none());
    // Based on a member of the cycle, a component still gets its tokens.
    assert_eq!(r.color("three-box.rest.fill"), Color::from_hex("#111111"));
}

#[test]
fn kit_check_reports_a_cycle_and_an_unknown_base() {
    let kit = egui_pop_tokens::kit::KitData::from_text(
        "loops",
        r##"
        button:
          based-on: {$type: "string", $value: "command-button"}
        command-button:
          based-on: {$type: "string", $value: "button"}
        icon-button:
          based-on: {$type: "string", $value: "buton"}
        "##,
    );
    let said: Vec<String> = egui_pop_tokens::kit::check(&kit)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        said.iter()
            .any(|p| p.contains("components are based on each other in a circle")),
        "{said:?}"
    );
    assert!(
        said.iter().any(|p| p.contains("`icon-button.based-on`")),
        "{said:?}"
    );
}

#[test]
fn built_in_themes_name_their_counterpart() {
    let themes = Themes::default();
    let pairs = [
        ("pop-dark", "pop-light"),
        ("github-dark", "github-light"),
        ("github-dark-high-contrast", "github-light-high-contrast"),
        ("catppuccin-mocha", "catppuccin-latte"),
        ("one-dark-pro", "one-light"),
        ("solarized-dark", "solarized-light"),
        ("gruvbox-dark", "gruvbox-light"),
    ];
    for (dark, light) in pairs {
        assert_eq!(themes.counterpart_of(dark), Some(light));
        assert_eq!(themes.counterpart_of(light), Some(dark));
        assert!(themes.resolve(&ThemeSpec::new(dark)).is_dark(), "{dark}");
        assert!(!themes.resolve(&ThemeSpec::new(light)).is_dark(), "{light}");
    }
    // Every built-in theme but high contrast has one.
    for name in BUILT_IN_THEMES {
        let paired = pairs.iter().any(|(d, l)| d == name || l == name);
        assert_eq!(themes.counterpart_of(name).is_some(), paired, "{name}");
    }
    assert_eq!(themes.counterpart_of("pop-high-contrast"), None);
}

#[test]
fn a_counterpart_is_resolved_with_the_spec_s_choices() {
    let themes = Themes::default();
    let spec = ThemeSpec::new("pop-light")
        .density(Density::Compact)
        .accent("blue")
        .override_token("danger", json!("#ff0000"));
    let dark = themes
        .resolve_counterpart(&spec)
        .expect("pop-light has one");
    assert_eq!(dark.spec().base, "pop-dark");
    assert!(dark.problems().is_empty(), "{:?}", dark.problems());
    assert_eq!(dark.dimension("space.m"), Some(4.0));
    assert_eq!(dark.spec().accent.as_deref(), Some("blue"));
    assert_eq!(dark.color("danger"), Color::from_hex("#ff0000"));
    // Named by the spec, or none at all.
    assert_eq!(
        themes.counterpart(&ThemeSpec::new("pop-high-contrast").counterpart("pop-light")),
        Some("pop-light".into())
    );
    assert_eq!(
        themes.counterpart(&ThemeSpec::new("pop-dark").alone()),
        None
    );
    assert_eq!(
        themes.counterpart(&ThemeSpec::pair("pop-dark", "no-such-theme")),
        None
    );
}

#[test]
fn a_user_theme_names_its_counterpart_on_one_side() {
    let mut themes = Themes::default();
    themes
        .add_text("night", r#"{"$extends": "pop-dark"}"#)
        .unwrap();
    themes
        .add_text(
            "day",
            r#"{"$extends": "pop-light", "$extensions": {"org.egui-pop.variant-of": "night"}}"#,
        )
        .unwrap();
    assert_eq!(themes.counterpart_of("day"), Some("night"));
    assert_eq!(themes.counterpart_of("night"), Some("day"));
    // Not inherited: a theme built on pop-light has none.
    themes
        .add_text("dusk", r#"{"$extends": "pop-light"}"#)
        .unwrap();
    assert_eq!(themes.counterpart_of("dusk"), None);
}

#[test]
fn a_theme_file_loads_over_its_base_and_warns_instead_of_failing() {
    let mut themes = Themes::default();
    let warnings = themes
        .add_text(
            "mine",
            r##"
            $extends: "pop-light"
            $extensions:
              org.egui-pop.format: {version: 1, contract: 99}
            surface:
              $type: "color"
              panel: {$value: "blue"}
              pannel: {$value: "#000000"}
            text:
              default: {$type: "color", $value: "{nowhere.1}"}
            radius:
              widget: {$type: "color", $value: "#ffffff"}
            gray:
              "2": {$type: "dimension", $value: {value: 2, unit: "px"}}
            teal:
              $type: "color"
              "9": {$value: "#00a0a0"}
            accent:
              default: {$type: "color", $value: "{teal.9}"}
            "##,
        )
        .unwrap();
    let said: Vec<String> = warnings.iter().map(ToString::to_string).collect();
    let left: Vec<&str> = warnings
        .iter()
        .filter(|w| w.left_out())
        .filter_map(Warning::path)
        .collect();
    assert_eq!(
        left,
        ["gray.2", "radius.widget", "surface.panel", "text.default"],
        "{said:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, Warning::Newer { found: 99, .. })),
        "{said:?}"
    );
    assert!(
        warnings.iter().any(|w| matches!(
            w,
            Warning::Unknown { path, kept: true, .. } if path == "surface.pannel"
        )),
        "{said:?}"
    );
    assert_eq!(warnings.len(), 6, "{said:?}");
    assert!(
        said.iter()
            .any(|w| w.contains("a dimension, where `pop-light` has a color")),
        "{said:?}"
    );
    // What was left out keeps the base theme's value, not the fallback's.
    let r = themes.resolve(&ThemeSpec::new("mine"));
    let light = themes.resolve(&ThemeSpec::new("pop-light"));
    assert!(r.problems().is_empty(), "{:?}", r.problems());
    for path in ["surface.panel", "text.default", "radius.widget", "gray.2"] {
        assert_eq!(r.value(path), light.value(path), "{path}");
        assert!(!r.get(path).unwrap().fallback, "{path}");
    }
    // The file's own tokens stay.
    assert_eq!(r.color("accent.default"), Color::from_hex("#00a0a0"));
    assert!(r.get("surface.pannel").is_some());

    // A base not known yet: nothing checked, said so.
    let warnings = themes
        .add_text("early", r#"{ "$extends": "later" }"#)
        .unwrap();
    assert_eq!(
        warnings,
        [Warning::Unchecked {
            base: "later".into()
        }]
    );
    // Only a file that cannot be read is an error.
    assert!(themes.add_text("broken", "{ nope").is_err());
}

#[test]
fn a_scale_given_by_its_seed_is_generated_when_resolved() {
    use egui_pop_tokens::palette::{Backdrop, Ramp, SEEDS_EXTENSION, Scheme, Seed};
    let mut themes = Themes::default();
    let warnings = themes
        .add_text(
            "seeded",
            r##"{
              "$extends": "pop-light",
              "$extensions": { "org.egui-pop.seeds": {
                "blue": { "seed": "#8e44ad", "scheme": "light" },
                "brick": { "seed": "#c0392b" },
                "bad": { "seed": "purple" }
              } },
              "blue": { "4": { "$type": "color", "$value": "#010203" } }
            }"##,
        )
        .unwrap();
    // An entry that cannot be read is a warning, the rest loads.
    assert!(
        matches!(&warnings[..], [Warning::Invalid { path, .. }] if path == &format!("$extensions.{SEEDS_EXTENSION}.bad")),
        "{warnings:?}"
    );
    let r = themes.resolve(&ThemeSpec::new("seeded"));
    assert!(r.problems().is_empty(), "{:?}", r.problems());
    let base = themes.resolve(&ThemeSpec::new("pop-light"));
    let backdrop = Backdrop::from_tokens(|p| base.color(p));
    let seed = Color::from_hex("#8e44ad").unwrap();
    let ramp = Ramp::fitted(seed, Scheme::Light, &backdrop);
    assert_eq!(r.color("blue.9"), Some(ramp.steps[8]));
    assert_eq!(r.color("blue.contrast"), Some(ramp.contrast));
    // A step the file gives keeps the file's value.
    assert_eq!(r.color("blue.4"), Color::from_hex("#010203"));
    // The roles aliasing the scale follow it.
    let blue = themes.resolve(&ThemeSpec::new("seeded").accent("blue"));
    assert_eq!(blue.color("accent.default"), Some(ramp.steps[8]));
    // A scale the base does not have gets its twelve steps and contrast,
    // for the theme's scheme when the seed does not say.
    let brick = Ramp::fitted(
        Color::from_hex("#c0392b").unwrap(),
        Scheme::Light,
        &backdrop,
    );
    for (i, step) in brick.steps.iter().enumerate() {
        assert_eq!(
            r.color(&format!("brick.{}", i + 1)),
            Some(*step),
            "brick.{}",
            i + 1
        );
    }
    assert_eq!(r.color("brick.contrast"), Some(brick.contrast));
    // Written back, the seeds stay.
    let mut file = TokenFile::parse("x", "{}").unwrap();
    file.set_seed("blue", Some(Seed::new(seed, Scheme::Dark)));
    let back = TokenFile::parse("x", &file.to_json_pretty()).unwrap();
    assert_eq!(back.seeds().0["blue"], Seed::new(seed, Scheme::Dark));
    file.set_seed("blue", None);
    assert_eq!(file.extensions, None);
}
