//! The gallery renders every page under every theme, and its pages are the
//! snapshot fixtures (`tests/snapshots/`, thresholds in `kittest.toml`).
//! Regenerate with `UPDATE_SNAPSHOTS=1 cargo test -p egui-pop-gallery`.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop::model::ids::StoryId;
use egui_pop::theme::{BUILT_IN_THEMES, Density, POP_THEMES, Tokens};
use egui_pop_gallery::{Gallery, Options, PAGES};
use egui_pop_test::SharedGpu as _;

fn harness(options: Options) -> Harness<'static, Gallery> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .shared_wgpu()
        .build_eframe(move |cc| {
            Gallery::from_creation_context(cc, options).expect("the gallery builds")
        })
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
    for theme in POP_THEMES {
        for density in Density::ALL {
            for page in PAGES {
                let mut h = harness(options(theme, density, page));
                run_page(&mut h, page);
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
        run_page(&mut h, page);
        egui_pop_test::assert_all_named(&h);
    }
}

#[test]
fn snapshots_per_theme() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in POP_THEMES {
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
fn long_toasts_wrap_before_their_buttons_in_two_languages() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "toasts"));
    h.run();
    h.get_by_label("Long text").click();
    h.run();
    egui_pop_test::assert_all_named(&h);
    h.snapshot("toasts-long-pop-dark");
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

#[test]
fn the_settings_window_searches_and_edits_shortcuts() {
    let mut results = egui_kittest::SnapshotResults::new();
    let mut h = harness(options("pop-dark", Density::Comfortable, "settings"));
    h.run();
    h.get_by_label("Open the settings window (Ctrl+,)").click();
    h.run();
    h.run();
    egui_pop_test::assert_all_named(&h);
    results.add(h.try_snapshot("settings-window-pop-dark"));
    h.get_by_label("Keyboard shortcuts").click();
    h.run();
    h.run();
    egui_pop_test::assert_all_named(&h);
    results.add(h.try_snapshot("shortcuts-pop-dark"));
}

#[test]
fn the_gallerys_keymap_is_clean() {
    let h = harness(Options::default());
    egui_pop_test::assert_keymap_clean(&h.state().app);
}

#[test]
fn every_editor_inspired_theme_draws_the_overview() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in BUILT_IN_THEMES.iter().filter(|t| !POP_THEMES.contains(t)) {
        let mut h = harness(options(theme, Density::Comfortable, "overview"));
        h.run();
        let tokens = Tokens::of(&h.ctx);
        assert!(tokens.resolved().problems().is_empty(), "{theme}");
        assert_eq!(tokens.resolved().spec().base, *theme);
        results.add(h.try_snapshot(format!("overview-{theme}")));
    }
}

#[test]
fn the_viewport_page_zooms_through_commands_and_reports_signals() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "viewport"));
    settle_viewport(&mut h);
    h.get_by_label("100 %").click();
    settle_viewport(&mut h);
    assert!(h.query_by_label("100.0 %").is_some());
    h.get_by_label("Zoom in").click();
    settle_viewport(&mut h);
    assert!(h.query_by_label("200.0 %").is_some());
    assert!(h.query_by_label_contains("command once: 200.0 %").is_some());
    let snap = h.state().app.handle().debug_snapshot();
    assert_eq!(snap["viewport"]["viewports"]["gallery"]["zoom"], 2.0);
    assert!(
        h.query_by_label_contains("Display target: Srgb primaries")
            .is_some()
    );
    // Fit, split before and after: the snapshot shows both, the
    // overlays and their handles.
    h.get_by_label("Fit").click();
    h.get_by_label("Split").click();
    settle_viewport(&mut h);
    let snap = h.state().app.handle().debug_snapshot();
    assert_eq!(
        snap["viewport"]["viewports"]["gallery"]["tiles"]["complete"],
        true
    );
    h.snapshot("viewport-pop-dark");
}

/// Runs a page until it settles: the viewport page until its tiles are
/// all on screen (they arrive from worker threads, each asking for a
/// frame), the others until they stop asking for frames.
fn run_page(h: &mut Harness<'_, Gallery>, page: &str) {
    if page == "viewport" {
        settle_viewport(h);
    } else {
        h.run();
    }
}

/// Runs until the viewport page's tiles are all on screen, waiting on the
/// demo source's own signal (never sleeping).
fn settle_viewport(h: &mut Harness<'_, Gallery>) {
    for _ in 0..40 {
        // Tiles arriving one by one keep asking for frames: past the
        // harness's step limit, the loop goes on waiting on the source.
        let _ = h.run_ok();
        let snap = h.state().app.handle().debug_snapshot();
        if snap["gallery.viewport"]["complete"] == true {
            return;
        }
        let f =
            h.state_mut()
                .app
                .debug_call("gallery.viewport", "wait_idle", &serde_json::json!(null));
        assert!(f.ok, "{f:?}");
    }
    panic!("the viewport page's tiles never completed");
}

#[test]
fn the_frame_page_draws_the_gallerys_own_frame_and_vetoes_closing() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "frame"));
    h.run();
    // The preview's buttons and regions, then the gallery's own frame.
    assert!(h.query_by_label("Preview menu").is_some());
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("frame-pop-dark"));
    let store = h.state().app.handle().store();
    egui_pop::core::lock(&store)
        .set(egui_pop::model::values::Set::new(
            "settings/window.frame",
            egui_pop::model::values::Change::To(egui_pop::model::values::Value::Choice(
                "own".into(),
            )),
            egui_pop::model::commands::Origin::User,
        ))
        .unwrap();
    h.run();
    assert!(egui_pop::core::frame::regions(&h.ctx).is_some());
    assert!(h.query_by_label("Window menu").is_some());
    h.get_by_label("Veto closing the window (a before-hook on frame.close)")
        .click();
    h.run();
    // The title bar's close button: the page's plugin vetoes it.
    // The title bar's, then the preview's.
    let closes: Vec<_> = h.query_all_by_label("Close").collect();
    assert_eq!(closes.len(), 2);
    closes[0].click();
    h.run();
    let trace = h.state().app.handle().step_traces().pop().unwrap();
    assert_eq!(trace.step.as_str(), "frame.close");
    assert!(trace.vetoed_by.is_some());
    results.add(h.try_snapshot("frame-own-pop-dark"));
}

#[test]
fn the_scale_page_sets_the_interface_zoom_and_draws_at_twice_the_pixels() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "scale"));
    h.run();
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("scale-pop-dark"));
    h.get_by_label("125%").click();
    h.run();
    assert!((h.ctx.zoom_factor() - 1.25).abs() < 1e-4);
    h.run();
    let app = h.state().app.handle().clone();
    assert!(
        app.events_since(0)
            .iter()
            .any(|e| e.name == "scale.changed" && e.data["to"] == 1.25)
    );
    // The overview at a native scale of 2: the same layout, twice the
    // pixels.
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .with_pixels_per_point(2.0)
        .shared_wgpu()
        .build_eframe(|cc| {
            Gallery::new(
                &cc.egui_ctx,
                options("pop-dark", Density::Comfortable, "overview"),
            )
            .expect("the gallery builds")
        });
    h.run();
    results.add(h.try_snapshot("overview-pop-dark@2x"));
}

#[test]
fn the_image_widgets_page_selects_a_thumbnail() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "images"));
    h.run();
    h.get_by_label("Image 3").click();
    h.run();
    assert!(h.query_by_label("Selected: Image 3").is_some());
    assert!(h.query_by_label("Histogram").is_some());
    assert!(h.query_by_label("Image 2000").is_none(), "virtualised");
    h.snapshot("images-pop-dark");
}

#[test]
fn the_workbench_page_opens_moves_and_vetoes_panels() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "workbench"));
    h.run();
    h.get_by_label("Open the demonstration panels").click();
    h.run();
    h.run();
    let app = h.state().app.handle().clone();
    let outline = egui_pop::model::ids::PanelId::from_static("gallery.outline");
    let console = egui_pop::model::ids::PanelId::from_static("gallery.console");
    assert!(app.panel_visible(&outline));
    assert!(h.query_by_label("Sections of the document.").is_some());
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("workbench-pop-dark"));
    // The console stays in the bottom panel: refused.
    h.get_by_label("Move Console to the start side").click();
    h.run();
    assert_eq!(
        app.layout().area_of(&console),
        Some(egui_pop::model::panels::Area::Bottom)
    );
    // Moves are vetoed while the box is ticked.
    h.get_by_label("Veto moves (a before-hook on workbench.move)")
        .click();
    h.run();
    h.get_by_label("Move Outline to the end side").click();
    h.run();
    assert_eq!(
        app.layout().area_of(&outline),
        Some(egui_pop::model::panels::Area::StartSide)
    );
    h.get_by_label("Veto moves (a before-hook on workbench.move)")
        .click();
    h.run();
    h.get_by_label("Move Outline to the end side").click();
    h.run();
    assert_eq!(
        app.layout().area_of(&outline),
        Some(egui_pop::model::panels::Area::EndSide)
    );
}

#[test]
fn the_strips_page_feeds_an_indicator_and_folds_the_side_bar() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "strips"));
    h.run();
    assert!(h.query_by_label("No job").is_some());
    h.get_by_label("Start").click();
    h.run();
    h.get_by_label("50%").click();
    h.run();
    h.run();
    // In the status bar, and in the page's list.
    assert_eq!(h.query_all_by_label_contains("Exporting 50").count(), 2);
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("strips-pop-dark"));
    h.get_by_label("Fold the side bar (Ctrl+B)").click();
    h.run();
    let app = h.state().app.handle().clone();
    assert!(
        app.layout()
            .folded
            .contains(&egui_pop::model::panels::Area::StartSide)
    );
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::B);
    h.run();
    assert!(app.layout().folded.is_empty());
}

#[test]
fn the_workbench_page_detaches_a_panel_into_its_own_window() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "workbench"));
    h.run();
    h.get_by_label("Open the demonstration panels").click();
    h.run();
    h.get_by_label("Detach Notes into a window").click();
    h.run();
    h.run();
    let app = h.state().app.handle().clone();
    let notes = egui_pop::model::ids::PanelId::from_static("gallery.notes");
    assert_eq!(app.layout().auxiliary.len(), 1);
    assert_eq!(app.layout().auxiliary[0].panel, notes);
    assert!(h.query_by_label("Free notes.").is_some());
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("workbench-detached-pop-dark"));
    // The window may cover the page's button (kittest draws it inside
    // the main one): docked back through the handle.
    app.attach_panel(&notes);
    h.run();
    assert!(app.layout().auxiliary.is_empty());
    assert!(app.panel_visible(&notes));
}

#[test]
fn the_arrangements_page_switches_saves_and_vetoes() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "arrangements"));
    h.run();
    let app = h.state().app.handle().clone();
    let outline = egui_pop::model::ids::PanelId::from_static("gallery.outline");
    h.get_by_label("Writing").click();
    h.run();
    h.run();
    assert!(app.panel_visible(&outline));
    assert_eq!(egui_pop::api::arrangements::current(&app), "Writing");
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("arrangements-pop-dark"));
    h.get_by_label("Veto switching (a before-hook on arrangement.switch)")
        .click();
    h.run();
    h.get_by_label("Review").click();
    h.run();
    assert_eq!(egui_pop::api::arrangements::current(&app), "Writing");
    assert!(
        h.query_by_label_contains("vetoed by gallery.arrangements")
            .is_some()
    );
}

#[test]
fn the_shortcuts_editor_searches_by_key_and_removes_every_key() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "settings"));
    h.run();
    h.get_by_label("Open the settings window (Ctrl+,)").click();
    h.run();
    h.get_by_label("Keyboard shortcuts").click();
    h.run();
    h.get_by_label("Search by key").click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowDown);
    h.run();
    assert!(h.query_by_label("Bound to Alt+Down").is_some());
    assert!(h.query_by_label("Gallery: Next page").is_some());
    assert!(h.query_by_label("Gallery: Previous page").is_none());
    h.get_by_label("Remove every key of Gallery: Next page")
        .click();
    h.run();
    h.run();
    let app = h.state().app.handle().clone();
    let next = egui_pop::model::ids::CommandId::from_static("gallery.page.next");
    assert!(app.keys_for(&next).is_empty());
    let doc = app.export_keymap(egui_pop::model::keymap::Layer::User);
    assert_eq!(doc.bindings[0].keys, "");
}

#[test]
fn the_gallery_opens_devtools_once_its_setting_is_on() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "debug"));
    h.run();
    let app = h.state().app.handle().clone();
    egui_pop::core::lock(&app.store())
        .set(egui_pop::model::values::Set::new(
            "settings/developer.devtools",
            egui_pop::model::values::Change::To(egui_pop::model::values::Value::Bool(true)),
            egui_pop::model::commands::Origin::User,
        ))
        .unwrap();
    h.run();
    h.key_press(egui::Key::F12);
    h.run();
    h.run();
    assert!(app.panel_visible(&egui_pop::api::devtools::PANEL_DEVTOOLS));
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("devtools-pop-dark"));
}

#[test]
fn the_translations_page_switches_language_and_shows_keys() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "i18n"));
    h.run();
    let app = h.state().app.handle().clone();
    h.get_by_label("Retry");
    h.get_by_label("1 image selected");
    h.get_by_label("Size: 1,234.6 MiB");
    h.get_by_label("Taken on Monday, October 12, 2026");

    h.get_by_label("Français").click();
    h.run();
    h.run();
    h.get_by_label("Réessayer");
    h.get_by_label("1 image sélectionnée");
    h.get_by_label("Taille : 1\u{202f}234,6 Mio");
    h.get_by_label("Prise le lundi 12 octobre 2026");
    let changed = app
        .events_since(0)
        .into_iter()
        .find(|e| e.name == "locale.changed")
        .expect("the change is published");
    assert_eq!(changed.data["from"], "en");
    assert_eq!(changed.data["to"], "fr");
    assert_eq!(egui_pop::i18n::plugin::of(&h.ctx).unwrap().locale(), "fr");

    h.get_by_label("Afficher les clés de traduction").click();
    for _ in 0..4 {
        h.run();
    }
    h.get_by_label("egui-pop-retry");
    h.get_by_label("gallery-i18n-show-keys").click();
    for _ in 0..4 {
        h.run();
    }
    h.get_by_label("Réessayer");

    h.get_by_label("English").click();
    h.run();
    h.run();
    h.get_by_label("Retry");
}

/// Runs frames until `done` holds: the simulated desktop answers from
/// threads.
#[track_caller]
fn run_until(h: &mut Harness<'_, Gallery>, done: impl Fn(&Harness<'_, Gallery>) -> bool) {
    for _ in 0..200 {
        let _ = h.run_ok();
        if done(h) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("timed out");
}

#[test]
fn the_platform_page_plays_the_desktop_and_each_service_answers() {
    use egui_pop::platform::crash::CRASHES;
    use egui_pop::platform::notify::NOTIFIER;
    let mut h = harness(options("pop-dark", Density::Comfortable, "platform"));
    h.run();
    egui_pop_test::assert_all_named(&h);
    // A notification on the simulated desktop, its action clicked there.
    h.get_by_label("Notify with an action").click();
    run_until(&mut h, |h| h.query_by_label("Click Show").is_some());
    let notifier = h.state().app.handle().service(&NOTIFIER).unwrap();
    // The button works once the desktop's thread showed it.
    run_until(&mut h, |h| {
        if context_key(h, "galleryNotified").is_empty() {
            if let Some(b) = h.query_by_label("Click Show") {
                b.click();
            }
            false
        } else {
            true
        }
    });
    assert_eq!(
        context_key(&h, "galleryNotified"),
        r#"{"what":"show the export"}"#
    );
    assert_eq!(notifier.history()[0].picked, ["show"]);
    // The global shortcut, pressed outside the window, and from the tray.
    h.get_by_label("Press Ctrl+Alt+G outside the window")
        .click();
    run_until(&mut h, |h| context_key(h, "galleryPings") == "1");
    h.get_by_label("Pick Ping the gallery").click();
    run_until(&mut h, |h| context_key(h, "galleryPings") == "2");
    // A crash in a past session, offered in a toast.
    h.get_by_label("Simulate a crash in a past session")
        .scroll_to_me();
    h.run();
    h.get_by_label("Simulate a crash in a past session").click();
    run_until(&mut h, |h| {
        h.query_all_by_label_contains("closed unexpectedly last time")
            .next()
            .is_some()
    });
    let crashes = h.state().app.handle().service(&CRASHES).unwrap();
    for r in crashes.reports() {
        crashes.dismiss(&r).unwrap();
    }
    // A second launch's link shows the page it names.
    h.get_by_label("Simulate a second launch with a link")
        .scroll_to_me();
    h.run();
    h.get_by_label("Simulate a second launch with a link")
        .click();
    run_until(&mut h, |h| {
        h.state()
            .app
            .handle()
            .step_traces()
            .iter()
            .any(|t| t.step.as_str() == "deeplink.open")
    });
    run_until(&mut h, |h| h.state().page().as_str() == "tokens");
}

#[test]
fn the_display_page_says_why_a_wide_gamut_falls_back() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "display"));
    h.run();
    h.get_by_label("Display P3").click();
    h.run();
    h.run();
    // The test surface is 8-bit sRGB, as eframe configures it.
    let reason = h
        .query_all_by_label_contains("the Display P3 gamut cannot be shown")
        .count();
    assert!(reason >= 1);
    let r = egui_pop::viewport::display::resolution(&h.ctx);
    assert!(!r.faithful());
    assert_eq!(
        r.drawn.color_space(),
        Some(egui_pop::viewport::display::ColorSpace::Srgb)
    );
    h.snapshot("display-pop-dark");
}

#[test]
fn the_files_page_chooses_with_the_in_app_picker() {
    let root = egui_pop_gallery::pages::files::demo_folder();
    let mut h = harness(options("pop-dark", Density::Comfortable, "files"));
    h.run();
    h.get_by_label("Open an image…").click();
    h.run();
    h.run();
    assert!(h.query_by_label("harbour.png").is_some());
    assert!(h.query_by_label("notes.txt").is_none(), "filtered out");
    h.snapshot("files-pop-dark");
    h.get_by_label("Filter, or type a path").type_text("light");
    h.run();
    h.key_press(egui::Key::Enter);
    for _ in 0..4 {
        h.run();
    }
    assert!(
        h.query_by_label("Last choice: Photos/lighthouse.jpg")
            .is_some(),
        "{:?}",
        h.state().app.handle().debug_snapshot()["files"]
    );
    assert!(egui_pop::api::files::recent(h.state().app.handle()).contains(&root.join("Photos")));
}

/// Drags from one node to a point of another (`at`: how far down it).
fn drag_to(h: &mut Harness<'_, Gallery>, from: &str, to: &str, at: f32) {
    let a = h.get_by_label(from).rect().center();
    let r = h.get_by_label(to).rect();
    let b = egui::pos2(r.center().x, r.top() + r.height() * at);
    h.drag_at(a);
    h.run();
    for i in 1..=4 {
        h.hover_at(a + (b - a) * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(b);
    h.run();
}

/// The last run of a step: its arguments, or why it was vetoed.
fn last_step(h: &Harness<'_, Gallery>, step: &str) -> Option<egui_pop::api::steps::StepTrace> {
    h.state()
        .app
        .handle()
        .step_traces()
        .into_iter()
        .rev()
        .find(|t| t.step.as_str() == step)
}

#[test]
fn the_tree_page_loads_lazily_follows_changes_and_vetoes_a_drop() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "tree"));
    run_until(&mut h, |h| h.query_by_label("Photos").is_some());
    // Lazy: a branch's files come from a thread once it is opened.
    egui_pop_test::double_click(&mut h, "Photos");
    h.run();
    assert!(h.query_by_label("Loading…").is_some());
    run_until(&mut h, |h| h.query_by_label("cover.jpg").is_some());
    assert!(
        h.query_by_label("IMG_2023_0001.raw").is_none(),
        "2023 is not open"
    );
    // A change made under the open branch: the selection stays.
    h.get_by_label("cover.jpg").click();
    h.run();
    h.get_by_label("Add a photo to Photos").click();
    h.run();
    assert!(h.query_by_label("new-1.jpg").is_some());
    assert!(h.query_by_label("Selected: cover.jpg").is_some());
    // A row's toggles act on that row, the selection stays.
    h.get_by_label("Hide Photos").click();
    h.run();
    h.get_by_label("Lock 2023").click();
    h.run();
    assert!(h.query_by_label("Show Photos").is_some());
    assert!(h.query_by_label("Unlock 2023").is_some());
    assert!(h.query_by_label("Selected: cover.jpg").is_some());
    // The keyboard (the button took the focus: back to the tree): up to
    // the new photo, End, Home, type-ahead.
    h.get_by_label("cover.jpg").click();
    h.run();
    // Above cover.jpg: 2024, 2023, then the new photo first.
    for _ in 0..3 {
        h.key_press(egui::Key::ArrowUp);
        h.run();
    }
    assert!(h.query_by_label("Selected: new-1.jpg").is_some());
    h.key_press(egui::Key::End);
    h.run();
    assert!(h.query_by_label("Selected: Offline drive").is_some());
    h.key_press(egui::Key::Home);
    h.run();
    assert!(h.query_by_label("Selected: Photos").is_some());
    h.event(egui::Event::Text("ar".into()));
    h.run();
    assert!(h.query_by_label("Selected: Archive").is_some());
    // A rename starts from the name; the decoration stays.
    egui_pop_test::double_click(&mut h, "Documents");
    run_until(&mut h, |h| h.query_by_label("notes.txt · 2 KB").is_some());
    h.get_by_label("notes.txt · 2 KB").click();
    h.run();
    h.key_press(egui::Key::F2);
    h.run();
    h.key_press(egui::Key::Home);
    h.event(egui::Event::Text("my-".into()));
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    assert!(h.query_by_label("my-notes.txt · 2 KB").is_some());
    egui_pop_test::double_click(&mut h, "Documents");
    h.run();
    // A failing branch says why.
    egui_pop_test::double_click(&mut h, "Offline drive");
    run_until(&mut h, |h| {
        h.query_by_label("Could not load: the drive is not connected")
            .is_some()
    });
    // A drop into Archive, vetoed while the box is ticked.
    h.get_by_label("Veto drops into Archive (a before-hook on tree.drop)")
        .click();
    h.run();
    drag_to(&mut h, "cover.jpg", "Archive", 0.5);
    let t = last_step(&h, "tree.drop").expect("ran");
    assert!(t.vetoed_by.is_some(), "{t:?}");
    assert!(
        h.query_by_label("cover.jpg").is_some(),
        "still under Photos"
    );
    h.get_by_label("Veto drops into Archive (a before-hook on tree.drop)")
        .click();
    h.run();
    drag_to(&mut h, "cover.jpg", "Archive", 0.5);
    let t = last_step(&h, "tree.drop").expect("ran");
    assert_eq!(t.args["target"], "archive");
    assert!(t.vetoed_by.is_none(), "{t:?}");
    assert!(
        h.query_by_label("cover.jpg").is_none(),
        "into Archive, closed"
    );
    h.remove_cursor();
    h.run();
    h.snapshot("tree-pop-dark");
}

#[test]
fn the_tree_page_opens_a_hundred_thousand_files() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "tree"));
    run_until(&mut h, |h| h.query_by_label("Many files").is_some());
    egui_pop_test::double_click(&mut h, "Many files");
    run_until(&mut h, |h| h.query_by_label("file-000000.dat").is_some());
    let built = h.query_all_by_label_contains("file-").count();
    assert!(built < 40, "{built} rows built");
}

#[test]
fn the_table_page_sorts_through_the_store_and_vetoes_one_sort() {
    use egui::accesskit::Role;
    let mut h = harness(options("pop-dark", Density::Comfortable, "table"));
    h.run();
    assert!(
        h.query_by_role_and_label(Role::Button, "IMG_00000.raw")
            .is_some()
    );
    assert!(
        h.query_by_role_and_label(Role::Button, "IMG_09999.raw")
            .is_none(),
        "virtualised"
    );
    h.get_by_label("Name").click();
    h.run();
    h.get_by_label("Name, sorted ascending").click();
    h.run();
    assert!(h.query_by_label("Name, sorted descending").is_some());
    let store = h.state().app.handle().store();
    assert_eq!(
        store
            .lock()
            .unwrap()
            .get("gallery/photos/sort")
            .unwrap()
            .value,
        egui_pop::model::values::Value::Text("-name".into())
    );
    assert!(
        h.query_by_role_and_label(Role::Button, "IMG_09999.raw")
            .is_some()
    );
    // The keyboard from a clicked row.
    h.get_by_role_and_label(Role::Button, "IMG_09999.raw")
        .click();
    h.run();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    let t = last_step(&h, "table.select").expect("ran");
    assert_eq!(t.args["keys"], serde_json::json!([9998]));
    // Sorting by rating, vetoed while the box is ticked.
    h.get_by_label("Veto sorting by rating (a before-hook on table.sort)")
        .click();
    h.run();
    h.get_by_label("Rating").click();
    h.run();
    assert!(last_step(&h, "table.sort").unwrap().vetoed_by.is_some());
    assert!(h.query_by_label("Name, sorted descending").is_some());
    // The list: a click, then type-ahead.
    h.get_by_label("beach").click();
    h.run();
    h.event(egui::Event::Text("s".into()));
    h.run();
    let t = last_step(&h, "list.select").expect("ran");
    assert_eq!(t.args["cursor"], serde_json::json!(23), "snow");
    h.remove_cursor();
    h.run();
    h.snapshot("table-pop-dark");
}
