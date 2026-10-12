//! The gallery renders every page under every theme, and its pages are the
//! snapshot fixtures (`tests/snapshots/`, thresholds in `kittest.toml`).
//! Regenerate with `UPDATE_SNAPSHOTS=1 cargo test -p egui-pop-gallery`.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
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
        for page in ["overview", "builtins", "icons"] {
            let mut h = harness(options(theme, Density::Comfortable, page));
            h.run();
            results.add(h.try_snapshot(format!("{page}-{theme}")));
        }
    }
    let mut h = harness(options("pop-dark", Density::Compact, "builtins"));
    h.run();
    results.add(h.try_snapshot("builtins-pop-dark-compact"));
    let mut h = harness(options("pop-light", Density::Compact, "icons"));
    h.run();
    results.add(h.try_snapshot("icons-pop-light-compact"));
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
    assert!(
        h.query_by_label_contains("Alt+Down: runs gallery.page.next")
            .is_some()
    );
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

fn context_key(h: &Harness<'_, Gallery>, key: &str) -> String {
    h.state().app.handle().context().get(key).to_string()
}

#[test]
fn menus_show_keys_ticks_and_greyed_items_and_run_commands() {
    use egui::accesskit::{Role, Toggled};
    let mut h = harness(options("pop-dark", Density::Comfortable, "menus"));
    h.run();
    // The View menu of the bar: a toggle, ticked, with its key.
    h.get_by_label("View").click();
    h.run();
    // Drawn in place on the page, and in the open menu (the last one).
    for grid in h.query_all_by_label("Show grid") {
        assert_eq!(grid.accesskit_node().role(), Role::CheckBox);
        assert_eq!(grid.accesskit_node().toggled(), Some(Toggled::True));
    }
    assert_eq!(
        h.state()
            .app
            .handle()
            .keys_for(&egui_pop_gallery::pages::menus::CMD_GRID),
        ["Ctrl+Shift+G"]
    );
    h.query_all_by_label("Show grid").last().unwrap().click();
    h.run();
    assert_eq!(context_key(&h, "galleryGrid"), "false");
    // Snapping needs the grid: greyed now, in the card's menu.
    h.get_by_label("Card with a context menu").click_secondary();
    h.run();
    let snaps: Vec<_> = h.query_all_by_label("Snap to grid").collect();
    assert!(snaps.iter().all(|n| n.accesskit_node().is_disabled()));
    assert!(
        h.query_all_by_label("Delete the card")
            .all(|n| n.accesskit_node().is_disabled())
    );
    // The key toggles it back, whatever menu is open.
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::G,
    );
    h.run();
    assert_eq!(context_key(&h, "galleryGrid"), "true");
    // A submenu: its items run the command with their arguments.
    h.query_all_by_label("Zoom").last().unwrap().hover();
    h.run();
    h.query_all_by_label("200%").last().unwrap().click();
    h.run();
    assert_eq!(context_key(&h, "galleryZoom"), "200");
    egui_pop_test::assert_all_named(&h);
}

#[test]
fn menu_snapshots() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in ["pop-dark", "pop-light"] {
        let mut h = harness(options(theme, Density::Comfortable, "menus"));
        h.run();
        h.get_by_label("Card with a context menu").click_secondary();
        h.run();
        h.query_all_by_label("Zoom").last().unwrap().hover();
        h.run();
        results.add(h.try_snapshot(format!("menus-{theme}")));
    }
}

#[test]
fn dialogs_open_from_the_page_and_send_what_they_ran() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in ["pop-dark", "pop-light"] {
        let mut h = harness(options(theme, Density::Comfortable, "dialogs"));
        h.run();
        h.get_by_label("Export settings").click();
        h.run();
        egui_pop_test::assert_all_named(&h);
        results.add(h.try_snapshot(format!("dialogs-{theme}")));
        h.key_press(egui::Key::Escape);
        h.run();
        h.get_by_label("Delete three images").click();
        h.run();
        assert!(h.get_by_label("Cancel").is_focused());
        h.get_by_label("Delete 3 images").click();
        h.run();
        h.run();
        assert_eq!(context_key(&h, "galleryDialogSent"), r#"{"what":"delete"}"#);
    }
}

#[test]
fn toasts_show_in_a_corner_and_their_action_runs() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in ["pop-dark", "pop-light"] {
        let mut h = harness(options(theme, Density::Comfortable, "toasts"));
        h.run();
        for b in ["Warning", "Error", "Success with an action"] {
            h.get_by_label(b).click();
            h.run();
        }
        egui_pop_test::assert_all_named(&h);
        results.add(h.try_snapshot(format!("toasts-{theme}")));
        h.get_by_label("Show").click();
        h.run();
        h.run();
        assert_eq!(
            context_key(&h, "galleryDialogSent"),
            r#"{"what":"show the export"}"#
        );
    }
}

#[test]
fn values_bind_in_three_modes_and_watch_follows() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in ["pop-dark", "pop-light"] {
        let mut h = harness(options(theme, Density::Comfortable, "values"));
        h.run();
        egui_pop_test::assert_all_named(&h);
        results.add(h.try_snapshot(format!("values-{theme}")));
    }
    let mut h = harness(options("pop-dark", Density::Comfortable, "values"));
    h.run();
    h.get_by_label("Exposure").click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowRight);
    h.run();
    let store = h.state().app.handle().store();
    let ev = egui_pop::core::lock(&store)
        .get("gallery/values/exposure")
        .unwrap()
        .value
        .as_f64()
        .unwrap();
    assert!((ev - 0.1).abs() < 1e-9, "{ev}");
    assert!(
        h.get_by_label("Exposure (watched)")
            .accesskit_node()
            .is_disabled()
    );
}
