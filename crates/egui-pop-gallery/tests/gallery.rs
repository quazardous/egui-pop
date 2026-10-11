//! The gallery renders every page under every theme, and its pages are the
//! snapshot fixtures (`tests/snapshots/`, thresholds in `kittest.toml`).
//! Regenerate with `UPDATE_SNAPSHOTS=1 cargo test -p egui-pop-gallery`.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::model::ids::StoryId;
use egui_pop::theme::{BUILT_IN_THEMES, Density, Tokens};
use egui_pop_gallery::{Gallery, Options, PAGES};

fn harness(options: Options) -> Harness<'static, Gallery> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .wgpu()
        .build_eframe(move |cc| Gallery::new(&cc.egui_ctx, options).expect("the gallery builds"))
}

fn options(theme: &str, density: Density, page: &str) -> Options {
    Options {
        theme: theme.to_string(),
        density,
        page: page.to_string(),
        ..Options::default()
    }
}

fn title(h: &Harness<'_, Gallery>, page: &str) -> String {
    h.state()
        .app
        .handle()
        .stories()
        .into_iter()
        .find(|s| s.id.as_str() == page)
        .unwrap()
        .title
}

#[test]
fn every_page_renders_under_every_theme_and_density() {
    for theme in BUILT_IN_THEMES {
        for density in Density::ALL {
            for page in PAGES {
                let mut h = harness(options(theme, density, page));
                h.run();
                // In the side bar and as the page's heading.
                let titles = h.query_all_by_label(&title(&h, page)).count();
                assert_eq!(titles, 2, "{theme} {page}");
                let tokens = Tokens::of(&h.ctx);
                assert!(tokens.resolved().problems().is_empty());
                assert_eq!(tokens.resolved().spec().base, *theme);
                assert!(
                    h.state().app.handle().diagnostics().is_empty(),
                    "{:?}",
                    h.state().app.handle().diagnostics()
                );
                h.render().expect("render");
            }
        }
    }
}

#[test]
fn every_interactive_node_of_every_page_has_a_name() {
    for page in PAGES {
        let mut h = harness(options("pop-dark", Density::Comfortable, page));
        h.run();
        egui_pop_test::assert_all_named(&h);
    }
}

#[test]
fn snapshots_per_theme() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in BUILT_IN_THEMES {
        for page in ["overview", "builtins"] {
            let mut h = harness(options(theme, Density::Comfortable, page));
            h.run();
            results.add(h.try_snapshot(format!("{page}-{theme}")));
        }
    }
    let mut h = harness(options("pop-dark", Density::Compact, "builtins"));
    h.run();
    results.add(h.try_snapshot("builtins-pop-dark-compact"));
    for page in ["tokens", "zones", "settings", "debug"] {
        let mut h = harness(options("pop-dark", Density::Comfortable, page));
        h.run();
        results.add(h.try_snapshot(format!("{page}-pop-dark")));
    }
}

#[test]
fn switching_theme_and_page_from_the_ui_and_the_keyboard() {
    let mut h = harness(Options::default());
    h.run();
    h.get_by_label("Tokens table").click();
    h.run();
    assert_eq!(h.state().page(), StoryId::from_static("tokens"));
    // Alt+Down: the next page.
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowDown);
    h.run();
    assert_eq!(h.state().page(), StoryId::from_static("builtins"));
    // The switchers change the spec; the theme is installed between frames.
    h.state()
        .app
        .handle()
        .theme()
        .update(|s| s.base = "pop-light".into());
    h.run();
    let tokens = Tokens::of(&h.ctx);
    assert_eq!(tokens.resolved().spec().base, "pop-light");
    assert!(!h.ctx.global_style().visuals.dark_mode);
}

#[test]
fn the_zones_page_shows_one_plugin_altering_anothers_blocks() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "zones"));
    h.run();
    assert!(
        h.query_by_label("The body, replaced by gallery.zones-extender.")
            .is_some()
    );
    assert!(h.query_by_label("Extended after the title").is_some());
    assert!(
        h.query_by_label("Wrapped by gallery.zones-extender:")
            .is_some()
    );
    // Ctrl+E on this page switches the extender off: the card is back.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::E);
    h.run();
    assert!(
        h.query_by_label("The body as gallery.zones draws it.")
            .is_some()
    );
    assert!(h.query_by_label("Extended after the title").is_none());
    let extender = h
        .state()
        .app
        .handle()
        .plugins()
        .into_iter()
        .find(|p| p.id.as_str() == "gallery.zones-extender")
        .unwrap();
    assert!(!extender.connected);
    // And on again from the playground.
    h.get_by_label("Extender plugin connected").click();
    h.run();
    assert!(h.query_by_label("Extended after the title").is_some());
}

#[test]
fn the_debug_page_shows_key_traces_and_a_caught_panic() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "debug"));
    h.run();
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowUp);
    h.run();
    // Alt+Up moved to the previous page; come back with Alt+Down.
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowDown);
    h.run();
    assert_eq!(h.state().page(), StoryId::from_static("debug"));
    assert!(h.query_by_label_contains("Alt+Down -> Run").is_some());
    h.get_by_label("Make the box below panic").click();
    h.run();
    assert!(
        h.query_by_label("The fragile box stopped working")
            .is_some()
    );
    assert_eq!(egui_pop::core::fault::faults(&h.ctx).len(), 1);
}

#[test]
fn quit_after_closes_the_window() {
    let mut h = harness(Options {
        quit_after: Some(3),
        ..Options::default()
    });
    for _ in 0..3 {
        h.step();
    }
    let commands = &h.output().viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(
        commands.contains(&egui::ViewportCommand::Close),
        "{commands:?}"
    );
}
