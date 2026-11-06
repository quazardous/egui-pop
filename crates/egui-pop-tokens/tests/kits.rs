//! Kits as packages: folders and embedded kits read the same, problems
//! are reported and never stop a kit, user kits are found, the default
//! kit is a kit folder, and a kit may hold both its variants.

use egui_pop_tokens::kit::{self, KitData, Origin, Problem};
use egui_pop_tokens::{Error, ThemeSpec, Themes};

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/kits")
        .join(name)
}

#[test]
fn a_kit_folder_reads_its_tokens_fonts_and_licence() {
    let kit = KitData::load(fixture("brand"));
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());
    assert_eq!(kit.name, "brand");
    assert_eq!(kit.display_name(), "Brand");
    assert_eq!(kit.theme.extends.as_deref(), Some("pop-light"));
    // kit.yaml and tab.tokens.json (JSON is read too), merged.
    assert!(kit.theme.tokens.contains_key("font.family.ui"));
    assert!(kit.theme.tokens.contains_key("tab.padding-x"));
    assert_eq!(kit.fonts.len(), 1);
    assert_eq!(
        (kit.fonts[0].family.as_str(), kit.fonts[0].weight),
        ("Inter", 400)
    );
    assert!(
        kit.license
            .as_deref()
            .unwrap()
            .contains("Open Font License")
    );
    assert!(kit::check(&kit).is_empty(), "{:?}", kit::check(&kit));

    let mut themes = Themes::default();
    themes.add_kit(&kit);
    let r = themes.resolve(&ThemeSpec::new("brand"));
    assert_eq!(r.dimension("tab.padding-x"), Some(14.0));
    // What the kit does not give comes from what it extends.
    let light = themes.resolve(&ThemeSpec::new("pop-light"));
    assert_eq!(r.color("surface.panel"), light.color("surface.panel"));
}

#[test]
fn an_embedded_kit_is_the_same_kit() {
    let embedded = egui_pop_tokens::kit!(
        "kits/brand",
        tokens = ["tab.tokens.json"],
        fonts = ["InterVariable-subset.ttf"],
        license = "LICENSE",
    );
    let loaded = KitData::load(fixture("brand"));
    assert_eq!(embedded.origin, Origin::Embedded("kits/brand"));
    assert_eq!(embedded.name, loaded.name);
    assert_eq!(embedded.theme, loaded.theme);
    assert_eq!(embedded.fonts, loaded.fonts);
    assert_eq!(embedded.license, loaded.license);
    assert!(kit::check(&embedded).is_empty());
}

#[test]
fn check_finds_what_an_embedded_kit_forgot() {
    let embedded = egui_pop_tokens::kit!("kits/brand");
    let problems = kit::check(&embedded);
    assert!(
        matches!(&problems[..], [Problem::Font { file, .. }] if file == "InterVariable-subset.ttf"),
        "{problems:?}"
    );
}

#[test]
fn a_broken_kit_reports_each_problem_and_still_installs() {
    let kit = KitData::load(fixture("broken"));
    let text: Vec<String> = kit.problems().iter().map(ToString::to_string).collect();
    let has = |f: fn(&Problem) -> bool| kit.problems().iter().any(f);
    assert!(
        has(|p| matches!(p, Problem::Tokens(Error::Json { .. }))),
        "{text:#?}"
    );
    // An unquoted `#colour` in YAML is refused, not read as nothing.
    assert!(
        has(
            |p| matches!(p, Problem::Tokens(Error::Yaml { message, .. }) if message.contains("line 6"))
        ),
        "{text:#?}"
    );
    assert!(has(
        |p| matches!(p, Problem::UnknownTerm { path, .. } if path == "tab.rest.fil")
    ));
    assert!(has(
        |p| matches!(p, Problem::Font { file, .. } if file == "Missing-Regular.ttf")
    ));
    assert!(has(
        |p| matches!(p, Problem::Font { file, .. } if file == "Fake-Bold.ttf")
    ));
    assert!(has(|p| matches!(p, Problem::NotRead { .. })));
    assert_eq!(kit.problems().len(), 6, "{text:#?}");
    assert!(kit.fonts.is_empty());
    // Usable: its theme resolves, the contract filled.
    let mut themes = Themes::default();
    themes.add_kit(&kit);
    let r = themes.resolve(&ThemeSpec::new("broken"));
    assert!(r.color("tab.rest.fill").is_some());
}

#[test]
fn a_missing_folder_is_a_problem_not_a_panic() {
    let kit = KitData::load(fixture("nowhere"));
    assert_eq!(kit.name, "nowhere");
    assert!(matches!(kit.problems()[0], Problem::Io { .. }));
}

#[test]
fn user_kits_are_the_folders_holding_a_kit_file() {
    let found = kit::discover(fixture(""));
    let names: Vec<&str> = found.iter().map(|k| k.name.as_str()).collect();
    assert_eq!(names, ["brand", "broken"]);
    let info = found[0].info();
    assert_eq!(info.label, "Brand");
    assert_eq!(info.problems, 0);
    assert!(kit::discover(fixture("nowhere")).is_empty());
}

#[egui_pop_test_macros::critical]
#[test]
fn the_default_kit_is_a_kit_folder() {
    // `themes/kit/` read as any kit folder, from its YAML: clean, and the
    // same tokens as the built-in list `build.rs` compiles from it.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("themes/kit");
    let kit = KitData::load(&dir);
    assert!(kit.problems().is_empty(), "{:?}", kit.problems());
    assert_eq!(kit.name, "pop");
    let pop = Themes::default();
    let pop_dark = pop.get("pop-dark").unwrap();
    for (path, token) in &kit.theme.tokens {
        assert_eq!(pop_dark.tokens.get(path), Some(token), "{path}");
    }
    let built_in = kit
        .theme
        .tokens
        .keys()
        .filter(|p| pop_dark.tokens.contains_key(*p))
        .count();
    assert_eq!(built_in, kit.theme.tokens.len());
    // And every kit token pop-dark has comes from the folder.
    let components: Vec<&str> = egui_pop_tokens::contract::kit_contract()
        .filter_map(|r| r.path.split('.').next())
        .collect();
    // Only the vocabulary's tokens: a component's other tokens are not
    // kit tokens.
    let states = [
        "rest", "hover", "active", "focus", "selected", "disabled", "tone",
    ];
    let in_vocabulary = |path: &str| {
        let mut parts = path.split('.');
        let (component, second) = (parts.next().unwrap(), parts.next().unwrap_or(""));
        components.contains(&component)
            && (states.contains(&second)
                || egui_pop_tokens::kit::COMPONENT_TERMS
                    .iter()
                    .any(|(t, _)| *t == second)
                || component == "glyph"
                || component == "drag-cursor")
    };
    for path in pop_dark.tokens.keys() {
        if in_vocabulary(path) {
            assert!(
                kit.theme.tokens.contains_key(path),
                "{path} is not in themes/kit/"
            );
        }
    }
}

#[test]
fn the_example_kits_are_clean() {
    let underline = egui_pop_tokens::kit!("../../../examples/kits/underline-tabs");
    assert!(
        kit::check(&underline).is_empty(),
        "{:?}",
        kit::check(&underline)
    );
    assert_eq!(underline.name, "underline-tabs");
    let macos = egui_pop_tokens::kit!("../../../examples/kits/flavour-macos");
    assert!(kit::check(&macos).is_empty(), "{:?}", kit::check(&macos));
    assert_eq!(macos.name, "flavour-macos");
    let rerun = egui_pop_tokens::kit!(
        "../../../examples/kits/rerun-like",
        tokens = ["shared.tokens.yaml"]
    );
    assert!(kit::check(&rerun).is_empty(), "{:?}", kit::check(&rerun));
    assert_eq!(rerun.name, "rerun-like-dark");
    assert_eq!(
        rerun.counterpart.as_ref().map(|c| c.name.as_str()),
        Some("rerun-like-light")
    );
}

const PAIR: &str = r##"
$extends: "pop-dark"
$extensions:
  org.egui-pop.kit:
    name: "brand-dark"
    label: "Brand Dark"
    counterpart:
      name: "brand-light"
      label: "Brand Light"
      tokens:
        $description: "The light one."
        $extends: "pop-light"
        surface:
          panel: {$type: "color", $value: "#fafafa"}
surface:
  panel: {$type: "color", $value: "#101010"}
"##;

const SHARED: &str = r##"
tab:
  padding-x: {$type: "dimension", $value: {value: 13, unit: "px"}}
"##;

fn pair() -> KitData {
    KitData::embedded(kit::Embedded {
        dir: "kits/brand",
        kit: PAIR,
        tokens: vec![("tab.tokens.yaml".into(), SHARED)],
        fonts: Vec::new(),
        license: None,
    })
}

#[test]
fn a_kit_holds_both_variants_in_one_manifest() {
    let kit = pair();
    assert!(kit::check(&kit).is_empty(), "{:?}", kit::check(&kit));
    assert_eq!(kit.name, "brand-dark");
    let light = kit.counterpart.as_ref().expect("the light variant");
    assert_eq!(light.name, "brand-light");
    assert_eq!(light.display_name(), "Brand Light");
    assert_eq!(light.theme.extends.as_deref(), Some("pop-light"));
    assert_eq!(light.theme.description.as_deref(), Some("The light one."));
    // The other token files serve both variants; kit.yaml's own tokens
    // are its variant's.
    assert!(kit.theme.tokens.contains_key("tab.padding-x"));
    assert!(light.theme.tokens.contains_key("tab.padding-x"));
    // Two themes, each the other's counterpart.
    let mut themes = Themes::default();
    themes.add_kit(&kit);
    assert_eq!(themes.counterpart_of("brand-dark"), Some("brand-light"));
    assert_eq!(themes.counterpart_of("brand-light"), Some("brand-dark"));
    let dark = themes.resolve(&ThemeSpec::new("brand-dark"));
    let light = themes.resolve(&ThemeSpec::new("brand-light"));
    assert_eq!(
        dark.color("surface.panel").map(|c| c.to_hex()),
        Some("#101010".into())
    );
    assert_eq!(
        light.color("surface.panel").map(|c| c.to_hex()),
        Some("#fafafa".into())
    );
    assert_eq!(light.dimension("tab.padding-x"), Some(13.0));
    // A picker lists both.
    let infos: Vec<(String, String)> = kit.infos().into_iter().map(|i| (i.name, i.label)).collect();
    assert_eq!(
        infos,
        [
            ("brand-dark".to_string(), "Brand Dark".to_string()),
            ("brand-light".to_string(), "Brand Light".to_string())
        ]
    );
}

#[test]
fn a_counterpart_that_cannot_be_used_leaves_the_kit_one_variant() {
    for (counterpart, why) in [
        (r#"{name: "x-light"}"#, "needs a `name` and its `tokens`"),
        (r#"{tokens: {}}"#, "needs a `name` and its `tokens`"),
        (r#""x-light""#, "is not a map"),
        (r#"{name: "x", tokens: {}}"#, "the kit's own name"),
        (
            r#"{name: "pop-light", tokens: {}}"#,
            "a built-in theme's name",
        ),
        (r#"{name: "a/b", tokens: {}}"#, "may not contain"),
    ] {
        let text = format!(
            "$extensions:\n  org.egui-pop.kit:\n    name: \"x\"\n    counterpart: {counterpart}\n"
        );
        let kit = KitData::from_text("x", &text);
        assert!(kit.counterpart.is_none(), "{counterpart}");
        let reported = kit
            .problems()
            .iter()
            .any(|p| matches!(p, Problem::Manifest { reason } if reason.contains(why)));
        assert!(reported, "{counterpart}: {:?}", kit.problems());
        // Still a kit, of one theme.
        assert_eq!(kit.name, "x");
    }
}

#[test]
fn a_counterpart_token_outside_the_vocabulary_is_reported() {
    let kit = KitData::from_text(
        "x",
        r##"
$extensions:
  org.egui-pop.kit:
    counterpart:
      name: "x-light"
      tokens:
        tab:
          rest:
            fil: {$type: "color", $value: "#000000"}
"##,
    );
    assert!(kit.counterpart.is_some());
    assert!(
        kit.problems()
            .iter()
            .any(|p| matches!(p, Problem::UnknownTerm { path, .. } if path == "tab.rest.fil")),
        "{:?}",
        kit.problems()
    );
}

#[test]
fn check_reports_two_states_left_to_colour_alone() {
    let kit = KitData::from_text(
        "merged",
        r##"
        glyph:
          flag-pinned: {$type: "string", $value: "asterisk"}
        "##,
    );
    let problems = kit::check(&kit);
    assert!(
        problems.iter().any(|p| matches!(
            p,
            Problem::SameGlyph { roles, icon }
                if roles.0 == "flag-pinned" && roles.1 == "flag-required" && icon == "asterisk"
        )),
        "{problems:?}"
    );
    // The default kit keeps every state's shape apart.
    let themes = Themes::default();
    assert!(kit::same_glyphs(&themes.resolve(&ThemeSpec::new("pop-dark"))).is_empty());
}
