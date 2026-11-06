//! The application's tests: headless egui_kittest harnesses from
//! egui-pop's test kit. Regenerate the picture with
//! `UPDATE_SNAPSHOTS=1 cargo test`, then look at it.

use egui_kittest::kittest::Queryable as _;
use egui_pop::api::App;
use egui_pop::egui;
use egui_pop::model::values::{Change, Value};
use egui_pop::prelude::*;
use serde_json::{Value as Json, json};

use crate::builder;
use crate::swatch::*;

const SIZE_PX: egui::Vec2 = egui::vec2(900.0, 560.0);

fn read(app: &App, path: &str) -> f64 {
    app.handle().value(path).unwrap()
}

#[test]
fn the_command_runs_from_its_key_and_undoes_in_one_step() {
    let mut h = egui_pop_test::app(builder(), SIZE_PX);
    h.run();
    let out = h
        .state_mut()
        .run(&SET, json!({ "hue": 30.0, "size": 80.0 }), Origin::User);
    assert_eq!(out.now(), Some(Ok(Json::Null)));
    assert_eq!(read(h.state(), HUE), 30.0);

    let command = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    h.key_press_modifiers(command, egui::Key::R);
    h.run();
    assert_eq!(read(h.state(), HUE), 200.0, "Ctrl+Shift+R resets");
    assert_eq!(read(h.state(), SIZE), 160.0);

    // Both values come back with one undo.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert_eq!(read(h.state(), HUE), 30.0);
    assert_eq!(read(h.state(), SIZE), 80.0);

    egui_pop_test::assert_keymap_clean(h.state());
    egui_pop_test::assert_writes_declared(h.state());
}

#[test]
fn the_panels_are_drawn_and_named() {
    let mut h = egui_pop_test::app(builder(), SIZE_PX);
    h.run();
    assert!(h.query_by_label("Reset").is_some(), "the parameters' panel");
    assert!(h.query_by_label_contains("Hue").is_some());
    egui_pop_test::assert_all_named(&h);
}

#[test]
fn the_setting_hides_the_code() {
    let mut h = egui_pop_test::app(builder(), SIZE_PX);
    h.run();
    assert!(
        h.query_by_label_contains("#").is_some(),
        "the code is shown"
    );
    // Written as the user would, in the settings window.
    let app = h.state().handle().clone();
    app.group(&Origin::User, "Hide the code", |g| {
        g.set(&SHOW_CODE.path(), Change::To(Value::Bool(false)))
    })
    .unwrap();
    h.run();
    assert!(
        h.query_by_label_contains("#").is_none(),
        "the code is hidden"
    );
}

#[test]
fn an_agent_sets_the_swatch_through_the_bridge() {
    let mut app = builder().build(&egui::Context::default()).unwrap();
    // What is offered to agents, with its arguments.
    let snapshot = app.handle().debug_snapshot();
    let commands = snapshot["commands"]["commands"].as_array().unwrap();
    let set = commands.iter().find(|c| c["id"] == SET.as_str()).unwrap();
    assert_eq!(set["agent"], true);
    assert_eq!(set["args"], json!({ "hue": 30.0, "size": 120.0 }));

    let f = app.debug_call(
        "commands",
        "run",
        &json!({ "id": SET.as_str(), "args": { "hue": 90.0 } }),
    );
    assert!(f.ok, "{}", f.message);
    assert_eq!(read(&app, HUE), 90.0);
    // The step is the agent's.
    let store = app.handle().store();
    let step = egui_pop::core::lock(&store)
        .undo_steps()
        .last()
        .cloned()
        .unwrap();
    assert_eq!(step.label, "Set the swatch");
    assert!(matches!(step.origin, Origin::Agent(_)), "{:?}", step.origin);
}

#[test]
fn the_kit_is_clean() {
    let problems = egui_pop::tokens::kit::check(&crate::kit());
    assert!(problems.is_empty(), "{problems:?}");
}

/// The picture is compared where it was made: on lavapipe, Mesa's
/// software Vulkan, as CI renders it. Without it (no adapter, or a GPU)
/// the test says so and passes; `EGUI_POP_SNAPSHOT_ANY_GPU=1` compares
/// on any GPU, within `kittest.toml`'s tolerances.
#[test]
fn the_window_looks_as_it_did() {
    let Some(mut h) = egui_pop_test::app_snapshot(builder(), SIZE_PX) else {
        return;
    };
    h.run();
    h.snapshot("swatch");
}

/// `Cargo.toml.liquid` is what `cargo generate` writes: it must ask for
/// the same crates as the manifest the repository builds. (In an
/// application made from the template, the `.liquid` file is gone:
/// nothing to check.)
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
    // The versions `cargo generate` offers are the repository's.
    let workspace = std::fs::read_to_string(dir.join("../../Cargo.toml")).unwrap();
    let version_of = |name: &str| {
        let line = workspace
            .lines()
            .find(|l| l.starts_with(&format!("{name} = ")))
            .unwrap();
        let v = line.split("version = \"").nth(1).unwrap();
        v[..v.find('"').unwrap()].to_string()
    };
    let generate = std::fs::read_to_string(dir.join("cargo-generate.toml")).unwrap();
    let pop = version_of("egui-pop");
    let pop_minor = pop.rsplit_once('.').unwrap().0;
    assert!(
        generate.contains(&format!("default = \"{pop_minor}\"")),
        "egui-pop is {pop}"
    );
    let egui = version_of("egui");
    assert!(
        generate.contains(&format!("default = \"{egui}\"")),
        "egui is {egui}"
    );
}
