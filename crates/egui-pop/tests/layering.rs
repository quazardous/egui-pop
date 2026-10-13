//! The layers of ADR 0001, checked against `cargo metadata`: every
//! dependency between workspace crates points to a lower layer, every
//! workspace crate has a layer, and the lowest layers stay free of egui.

use std::collections::BTreeMap;
use std::process::Command;

/// Each workspace crate's layer.
const LAYERS: &[(&str, u32)] = &[
    ("egui-pop-tokens", 0),
    ("egui-pop-win32", 0),
    ("egui-pop-model", 1),
    ("egui-pop-core", 2),
    ("egui-pop-api", 3),
    ("egui-pop-viewport", 3),
    ("egui-pop-test", 4),
    ("egui-pop-i18n", 4),
    ("egui-pop-platform", 4),
    ("egui-pop-views", 4),
    ("egui-pop-editors", 4),
    ("egui-pop-imaging", 4),
    ("egui-pop", 5),
    ("egui-pop-gallery", 6),
    ("consumer-smoke", 6),
    ("xtask", 6),
];

/// Layers that must not depend on a GUI toolkit.
const NO_EGUI_BELOW: u32 = 2;

#[test]
fn dependencies_point_down_the_layers() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let layers: BTreeMap<&str, u32> = LAYERS.iter().copied().collect();
    let mut problems = Vec::new();
    for package in meta["packages"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap();
        let Some(&layer) = layers.get(name) else {
            problems.push(format!(
                "`{name}` has no layer: add it to LAYERS and ADR 0001"
            ));
            continue;
        };
        for dep in package["dependencies"].as_array().unwrap() {
            let dep_name = dep["name"].as_str().unwrap();
            // Dev-dependencies (tests, examples) may use any layer.
            if dep["kind"].as_str() == Some("dev") {
                continue;
            }
            if let Some(&dep_layer) = layers.get(dep_name)
                && dep_layer >= layer
            {
                problems.push(format!(
                    "`{name}` (L{layer}) depends on `{dep_name}` (L{dep_layer})"
                ));
            }
            if layer < NO_EGUI_BELOW
                && (dep_name.starts_with("egui") && !layers.contains_key(dep_name)
                    || dep_name == "eframe")
            {
                problems.push(format!("`{name}` (L{layer}) depends on `{dep_name}`"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "layering broken:\n{}",
        problems.join("\n")
    );
}
