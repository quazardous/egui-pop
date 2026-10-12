//! The minimal example (and so the application template) runs headless:
//! its panel, its command with its key, the toast, undo.

#[allow(dead_code)]
#[path = "../examples/minimal.rs"]
mod minimal;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::api::App;
use egui_pop::prelude::*;

#[test]
fn the_minimal_application_works() {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(minimal::Notes)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    assert!(h.query_by_label("Clear").is_some(), "the panel is drawn");
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::K,
    );
    h.run();
    h.run();
    let text = |h: &Harness<'_, App>| {
        egui_pop::core::binding::read::<String>(&h.state().handle().store(), "settings/notes.text")
            .unwrap()
    };
    assert_eq!(text(&h), "");
    assert!(h.query_all_by_label_contains("The note is cleared").count() >= 1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert_eq!(text(&h), "Write here.");
    egui_pop_test::assert_keymap_clean(h.state());
}

#[test]
fn the_template_is_the_minimal_example() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let example =
        std::fs::read_to_string(root.join("crates/egui-pop/examples/minimal.rs")).unwrap();
    let template = std::fs::read_to_string(root.join("template/src/main.rs")).unwrap();
    let from_use = |s: &str| s[s.find("use egui_pop::").unwrap()..].to_string();
    let filled = from_use(&template).replace("{{project-name}}", "egui-pop-minimal");
    let expected = from_use(&example).replace(
        ".run_native(\"Notes\",",
        ".run_native(\"egui-pop-minimal\",",
    );
    assert_eq!(
        filled, expected,
        "template/src/main.rs drifted from examples/minimal.rs"
    );
    // `.liquid`: cargo-generate renders it to `Cargo.toml`, and cargo
    // never reads it as a package of the repository.
    let manifest = std::fs::read_to_string(root.join("template/Cargo.toml.liquid")).unwrap();
    assert!(manifest.contains("egui-pop = \"{{egui_pop_version}}\""));
}

/// Cargo reads every `Cargo.toml` of a git dependency's repository: none
/// may hold a template placeholder, or every dependent's cargo run says
/// "invalid character `{` in package name".
#[test]
fn no_manifest_of_the_repository_holds_a_placeholder() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            let name = e.file_name();
            if p.is_dir() {
                if name != "target" && name != ".git" {
                    walk(&p, out);
                }
            } else if name == "Cargo.toml" {
                out.push(p);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut manifests = Vec::new();
    walk(&root, &mut manifests);
    assert!(manifests.len() > 5, "{manifests:?}");
    for m in manifests {
        let text = std::fs::read_to_string(&m).unwrap();
        assert!(!text.contains("{{"), "{} holds a placeholder", m.display());
    }
}
