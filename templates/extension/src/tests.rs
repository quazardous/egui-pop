//! The extension's tests: headless egui_kittest harnesses, no GPU.
//! Written against `crate::` so they keep working in a crate made from
//! the template under another name.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::Node;
use egui_kittest::kittest::Queryable as _;
use egui_pop::api::App;
use egui_pop::i18n::I18nPlugin;
use egui_pop::prelude::*;
use egui_pop::tokens::{BUILT_IN_THEMES, Color, contrast::contrast_ratio};
use serde_json::json;

use crate::*;

/// Draws the extension's story in the main zone, as a gallery would.
struct StoryPage;

impl Plugin for StoryPage {
    fn id(&self) -> PluginId {
        PluginId::from_static("story-page")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("story-page.story"), 0, |ui, app| {
                assert!(app.show_story(ui, &STORY), "the story is registered");
            }),
        );
        Ok(())
    }
}

/// Keeps every tally at zero or above.
struct NeverNegative;

impl Plugin for NeverNegative {
    fn id(&self) -> PluginId {
        PluginId::from_static("never-negative")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_CHANGE, |_, args| {
            if args["to"].as_i64().unwrap_or_default() < 0 {
                Before::Veto("a tally cannot go below zero".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

/// A harness drawing the application with the extension and its story.
/// The application is built on the harness's own context, so what the
/// plugins install in it (the translations) is what the harness draws
/// with.
fn harness(builder: AppBuilder) -> Harness<'static, Option<App>> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 500.0))
        .build_ui_state(
            |ui, app: &mut Option<App>| {
                if let Some(app) = app {
                    app.show(ui);
                }
            },
            None,
        );
    let app = builder
        .plugin(TallyPlugin)
        .plugin(StoryPage)
        .build(&h.ctx)
        .unwrap();
    *h.state_mut() = Some(app);
    h.run();
    h
}

fn app<'h>(h: &'h Harness<'_, Option<App>>) -> &'h App {
    h.state().as_ref().unwrap()
}

fn button<'h>(h: &'h Harness<'_, Option<App>>, label: &'h str) -> Node<'h> {
    h.get_by_role_and_label(Role::Button, label)
}

fn count(h: &Harness<'_, Option<App>>, name: &str) -> i64 {
    app(h).handle().service(&TALLIES).unwrap().get(name)
}

#[test]
fn the_plugin_connects_cleanly() {
    let h = harness(App::builder().plugins(DefaultPlugins));
    egui_pop_test::assert_keymap_clean(app(&h));
    let app = app(&h).handle();
    assert!(app.diagnostics().is_empty(), "{:?}", app.diagnostics());
    assert!(app.steps().iter().any(|s| s.spec.id == STEP_CHANGE));
    assert!(
        app.stories()
            .iter()
            .any(|s| s.id == STORY && s.has_playground)
    );
    // People find the increment in the palette; agents get both.
    let increment = app.command(&CMD_INCREMENT).unwrap();
    assert!(increment.palette && increment.agent);
    // Its icon is declared once, by name, and resolves (no diagnostic).
    assert_eq!(increment.icon.as_deref(), Some("plus"));
    let set = app.command(&CMD_SET).unwrap();
    assert!(!set.palette && set.agent);
    assert_eq!(set.args["value"], 3);
}

#[test]
fn the_story_draws_named_widgets_and_clicks_run_the_step() {
    let mut h = harness(App::builder().plugins(DefaultPlugins));
    egui_pop_test::assert_all_named(&h);
    assert!(h.query_by_label("Apples: 0").is_some());
    button(&h, "Increase Apples").click();
    h.run();
    assert_eq!(count(&h, "apples"), 1);
    assert!(h.query_by_label("Apples: 1").is_some());
    button(&h, "Decrease Apples").click();
    h.run();
    button(&h, "Decrease Apples").click();
    h.run();
    assert_eq!(count(&h, "apples"), -1, "nothing keeps it above zero yet");
}

#[test]
fn another_plugin_vetoes_a_change() {
    let mut h = harness(App::builder().plugin(NeverNegative));
    button(&h, "Decrease Pears").click();
    h.run();
    assert_eq!(count(&h, "pears"), 0, "vetoed");
    let traced = app(&h).handle().steps();
    let step = traced.iter().find(|s| s.spec.id == STEP_CHANGE).unwrap();
    assert!(step.hooks.iter().any(|k| k.kind == "before"));
}

#[test]
fn agents_set_a_tally_through_the_step() {
    let mut a = App::builder()
        .plugin(TallyPlugin)
        .plugin(NeverNegative)
        .build(&egui::Context::default())
        .unwrap();
    let set = |a: &mut App, args| a.run(&CMD_SET, args, Origin::Agent("test".into())).now();
    assert_eq!(
        set(&mut a, json!({ "tally": "apples", "value": 7 })),
        Some(Ok(json!(7)))
    );
    assert_eq!(a.handle().service(&TALLIES).unwrap().get("apples"), 7);
    assert_eq!(
        set(&mut a, json!({ "tally": "apples", "value": -1 })),
        Some(Err("vetoed: a tally cannot go below zero".into()))
    );
    assert!(matches!(
        set(&mut a, json!({ "value": 1 })),
        Some(Err(e)) if e.contains("`tally`")
    ));
    let out = a.run(&CMD_INCREMENT, json!({ "tally": "apples" }), Origin::User);
    assert_eq!(out.now(), Some(Ok(json!(8))));
}

#[test]
fn it_speaks_french_with_i18n() {
    let mut h = harness(
        App::builder()
            .plugins(DefaultPlugins)
            .plugin(I18nPlugin::new()),
    );
    egui_pop::i18n::plugin::set_language(app(&h).handle(), "fr").unwrap();
    h.run();
    h.run();
    assert!(
        h.query_by_role_and_label(Role::Button, "Augmenter Apples")
            .is_some()
    );
    assert!(h.query_by_label("Apples : 0").is_some());
    let app = app(&h).handle();
    let spec = app.command(&CMD_SET).unwrap();
    assert_eq!(app.command_title(&spec), "Régler un compteur");
}

#[test]
fn without_i18n_the_english_in_the_code_shows() {
    let h = harness(App::builder());
    assert!(
        h.query_by_role_and_label(Role::Button, "Increase Apples")
            .is_some()
    );
}

#[test]
fn disconnected_it_leaves_nothing_behind_and_comes_back() {
    let mut h = harness(App::builder().plugin(I18nPlugin::new()));
    let app = app(&h).handle().clone();
    app.set_plugin_enabled(PluginId::from_static("story-page"), false);
    app.set_plugin_enabled(PLUGIN, false);
    h.run();
    assert!(app.service(&TALLIES).is_none());
    assert!(app.command(&CMD_SET).is_none());
    assert!(app.stories().iter().all(|s| s.id != STORY));
    assert!(app.steps().iter().all(|s| s.spec.id != STEP_CHANGE));
    app.set_plugin_enabled(PLUGIN, true);
    app.set_plugin_enabled(PluginId::from_static("story-page"), true);
    h.run();
    assert!(app.diagnostics().is_empty(), "{:?}", app.diagnostics());
    assert!(
        h.query_by_role_and_label(Role::Button, "Increase Apples")
            .is_some()
    );
}

#[test]
fn the_catalogues_define_every_key_and_only_their_own() {
    for (locale, _) in CATALOGUES {
        let i18n = egui_pop::i18n::I18n::new("en");
        add_catalogues(&i18n).unwrap();
        let keys = i18n.keys(locale);
        for key in strings::ALL {
            assert!(keys.contains(key.key), "{locale}: `{}` is missing", key.key);
        }
        for command in [&CMD_INCREMENT, &CMD_SET] {
            let id = format!("command-{}.title", command.as_str().replace('.', "-"));
            assert!(
                i18n.get_in(locale, &id, &[]).is_some(),
                "{locale}: `{id}` is missing"
            );
        }
    }
}

#[test]
fn the_tokens_stay_legible_in_every_built_in_theme() {
    let hex = |c: egui::Color32| {
        Color::from_hex(&format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())).unwrap()
    };
    for theme in BUILT_IN_THEMES {
        let ctx = egui::Context::default();
        let _ = egui_pop::theme::install(&ctx, &ThemeSpec::new(*theme));
        let t = TallyTokens::of(&ctx);
        let text = if theme.contains("high-contrast") {
            7.0
        } else {
            4.5
        };
        for (what, fg, needs) in [
            ("text", t.text, text),
            ("border", t.border, 3.0),
            ("accent", t.accent, 3.0),
        ] {
            let ratio = contrast_ratio(hex(fg), hex(t.background));
            assert!(
                ratio >= needs,
                "{theme}: {what} at {ratio:.2}:1, needs {needs}:1"
            );
        }
    }
}

/// `Cargo.toml.liquid` is what `cargo generate` writes: it must ask for
/// the same crates as the manifest the repository builds. (In a crate
/// made from the template, the `.liquid` file is gone: nothing to check.)
#[test]
fn the_generated_manifest_matches_the_built_one() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let Ok(liquid) = std::fs::read_to_string(dir.join("Cargo.toml.liquid")) else {
        return;
    };
    let built = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    let deps = |manifest: &str| {
        let mut out = Vec::new();
        let mut section = String::new();
        for line in manifest.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                section = line.to_string();
            } else if section.ends_with("dependencies]")
                && let Some((name, spec)) = line.split_once('=')
            {
                let features = spec
                    .contains("features")
                    .then(|| spec[spec.find("features").unwrap()..].trim().to_string());
                out.push((
                    section.clone(),
                    name.trim().split('.').next().unwrap().to_string(),
                    features,
                ));
            }
        }
        out
    };
    assert_eq!(deps(&built), deps(&liquid));
}
