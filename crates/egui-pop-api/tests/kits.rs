//! Kits in an application: one line adds a kit's theme, faces and
//! handlers; the user's kits are found in the persistence directory;
//! problems are diagnostics, never a failed build.

use std::sync::Arc;

use egui_pop_api::App;
use egui_pop_core::fonts;
use egui_pop_core::kit::{ComponentKit, Kit, KitCx, KitSource, Part, Primitive};
use egui_pop_core::theme::ThemeSpec;
use egui_pop_core::tokens::cache::Cache;
use egui_pop_core::tokens::kit::{KitData, Origin};

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../egui-pop-tokens/tests/kits")
        .join(name)
}

#[test]
fn a_kit_folder_brings_its_theme_and_its_faces() {
    let ctx = egui::Context::default();
    let app = App::builder()
        .kit(KitData::load(fixture("brand")))
        .theme(ThemeSpec::new("brand"))
        .build(&ctx)
        .unwrap();
    assert!(
        !app.handle()
            .diagnostics()
            .iter()
            .any(|d| d.code == "kit-problem"),
        "{:?}",
        app.handle().diagnostics()
    );
    let kits = app.handle().theme().kits().to_vec();
    assert_eq!(kits.len(), 1);
    assert_eq!(
        (kits[0].name.as_str(), kits[0].label.as_str()),
        ("brand", "Brand")
    );
    assert!(app.handle().theme().themes().get("brand").is_some());
    let report = fonts::report(&ctx).expect("fonts installed with the theme");
    assert!(report.problems.is_empty(), "{report:?}");
    assert!(
        report
            .primary
            .iter()
            .any(|(_, p)| p.as_deref().is_some_and(|p| p.contains("Inter"))),
        "{report:?}"
    );
    assert_eq!(Kit::of(&ctx).tab.data.padding_x, 14.0);
}

struct Notch;

impl Primitive<ComponentKit> for Notch {
    fn measure(&self, data: &ComponentKit, label: egui::Vec2) -> egui::Vec2 {
        egui::vec2(label.x + 2.0 * data.padding_x, data.height)
    }
    fn paint(&self, _: &KitCx<'_>, _: &ComponentKit, _: &Part<'_>) {}
    fn paint_label(&self, cx: &KitCx<'_>, data: &ComponentKit, part: &Part<'_>) {
        egui_pop_core::kit::paint_label(cx, data, part);
    }
}

/// What a kit crate exposes.
fn notched_kit() -> KitSource {
    let data = KitData::from_text(
        "notched",
        r#"
        tab:
          shape: {$type: "string", $value: "notch"}
        "#,
    );
    KitSource::new(data).shape("tab", "notch", Arc::new(Notch))
}

#[test]
fn a_kit_with_code_registers_its_handlers() {
    let ctx = egui::Context::default();
    let _app = App::builder()
        .kit(notched_kit())
        .theme(ThemeSpec::new("notched"))
        .build(&ctx)
        .unwrap();
    let kit = Kit::of(&ctx);
    assert_eq!(kit.tab.shape, "notch");
    assert!(kit.problems.is_empty(), "{:?}", kit.problems);
}

#[test]
fn user_kits_are_read_from_the_persistence_directory() {
    let dir = std::env::temp_dir().join(format!("egui-pop-user-kits-{}", std::process::id()));
    let kit_dir = dir.join("kits/quiet");
    std::fs::create_dir_all(&kit_dir).unwrap();
    std::fs::write(
        kit_dir.join("kit.yaml"),
        r#"
        $extensions:
          org.egui-pop.kit: {label: "Quiet"}
        tab:
          rest:
            fil: {$type: "color", $value: "{surface.panel}"}
        "#,
    )
    .unwrap();
    let ctx = egui::Context::default();
    let cache = dir.join("cache");
    let app = App::builder()
        .persist(&dir)
        .user_kits(true)
        .cache(Cache::at(&cache))
        .build(&ctx)
        .unwrap();
    // Compiled into the cache given, never next to the kit.
    assert_eq!(std::fs::read_dir(cache.join("tokens")).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(&kit_dir).unwrap().count(), 1);
    let kits = app.handle().theme().kits().to_vec();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].label, "Quiet");
    assert_eq!(kits[0].origin, Origin::Dir(kit_dir.clone()));
    // Its typo is a diagnostic; the kit is there anyway.
    let problems: Vec<_> = app
        .handle()
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "kit-problem")
        .collect();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].subject, "quiet");
    assert!(app.handle().theme().themes().get("quiet").is_some());

    // Not asked for: none read.
    let ctx = egui::Context::default();
    let app = App::builder().persist(&dir).build(&ctx).unwrap();
    assert!(app.handle().theme().kits().is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_later_kit_of_the_same_name_replaces_an_earlier_one() {
    let ctx = egui::Context::default();
    let first = KitData::from_text(
        "k",
        r#"{ "$extensions": { "org.egui-pop.kit": { "label": "First" } } }"#,
    );
    let second = KitData::from_text(
        "k",
        r#"{ "$extensions": { "org.egui-pop.kit": { "label": "Second" } } }"#,
    );
    let app = App::builder().kit(first).kit(second).build(&ctx).unwrap();
    let kits = app.handle().theme().kits().to_vec();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].label, "Second");
}
