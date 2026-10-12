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
