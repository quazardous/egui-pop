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
            // Motion moves only when a test says: the same frames on
            // every run.
            egui_pop::core::motion::freeze(&cc.egui_ctx, 0.0);
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
fn a_closable_dialog_closes_by_its_button_as_by_escape() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in ["pop-dark", "pop-light"] {
        let mut h = harness(options(theme, Density::Comfortable, "dialogs"));
        h.run();
        h.get_by_label("Confirm, with a close button").click();
        h.run();
        egui_pop_test::assert_all_named(&h);
        assert!(h.get_by_label("Share").is_focused());
        results.add(h.try_snapshot(format!("dialogs-closable-{theme}")));
        // The dialog's, above anything else named so.
        h.query_all_by_role_and_label(egui::accesskit::Role::Button, "Close")
            .last()
            .unwrap()
            .click();
        h.run();
        assert!(h.query_by_label("Share the page?").is_none());
    }
}

#[test]
fn a_movable_dialog_has_its_header_band_and_is_dragged_by_it() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in ["pop-dark", "pop-light"] {
        let mut h = harness(options(theme, Density::Comfortable, "dialogs"));
        h.run();
        h.get_by_label("Movable levels").click();
        h.run();
        egui_pop_test::assert_all_named(&h);
        // Dragged by its band, up and to the left.
        let a = h.get_by_label("Levels").rect().center();
        let b = a + egui::vec2(-240.0, -120.0);
        h.drag_at(a);
        h.run();
        for i in 1..=4 {
            h.hover_at(a + (b - a) * (i as f32 / 4.0));
            h.run();
        }
        h.drop_at(b);
        h.run();
        let moved = h.get_by_label("Levels").rect().center();
        assert!(
            moved.x < a.x - 150.0 && moved.y < a.y - 60.0,
            "{a:?} {moved:?}"
        );
        results.add(h.try_snapshot(format!("dialogs-movable-{theme}")));
        // Its close button is on the band.
        h.query_all_by_role_and_label(egui::accesskit::Role::Button, "Close")
            .last()
            .unwrap()
            .click();
        h.run();
        assert!(h.query_by_label("Levels").is_none());
        // It comes back where it was left.
        h.get_by_label("Movable levels").click();
        h.run();
        h.run();
        let back = h.get_by_label("Levels").rect().center();
        assert!((back - moved).length() < 2.0, "{back:?} {moved:?}");
    }
    // A clear backdrop: the page stays as it is behind the dialog.
    let mut h = harness(options("pop-dark", Density::Comfortable, "dialogs"));
    h.run();
    h.get_by_label("Preview, clear backdrop").click();
    h.run();
    egui_pop_test::assert_all_named(&h);
    results.add(h.try_snapshot("dialogs-clear-pop-dark"));
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
fn sliders_run_their_step_and_a_plugin_snaps_one() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in ["pop-dark", "pop-light"] {
        let mut h = harness(options(theme, Density::Comfortable, "sliders"));
        h.run();
        egui_pop_test::assert_all_named(&h);
        results.add(h.try_snapshot(format!("sliders-{theme}")));
    }
    let mut h = harness(options("pop-dark", Density::Comfortable, "sliders"));
    h.run();
    h.get_by_label("Snapped to halves").click();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    for _ in 0..8 {
        h.key_press(egui::Key::Backspace);
    }
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("7.3");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    let snapped = h.get_by_label("Snapped to halves");
    assert_eq!(snapped.accesskit_node().numeric_value(), Some(7.5));
    let trace = h.state().app.handle().step_traces().pop().unwrap();
    assert_eq!(trace.step.as_str(), "slider.change");
    assert_eq!(trace.args["to"], 7.5);
    assert_eq!(trace.redirected_by.len(), 1);
    assert!(
        h.query_by_label("Last change: value from 5.0 to 7.5 by typed")
            .is_some()
    );
    // The quad toggle and the picker.
    h.get_by_label("Pick the black level").click();
    h.run();
    assert!(h.query_by_label("Picker clicked 1 times").is_some());
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

/// The last trace of a step, by its id.
fn last_trace_of(h: &Harness<'_, Gallery>, step: &str) -> egui_pop::api::steps::StepTrace {
    h.state()
        .app
        .handle()
        .step_traces()
        .into_iter()
        .rev()
        .find(|t| t.step.as_str() == step)
        .unwrap_or_else(|| panic!("no {step} step ran"))
}

#[test]
fn the_curves_and_wheels_page_runs_their_steps_and_caps_the_gain() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in ["pop-dark", "pop-light"] {
        let mut h = harness(options(theme, Density::Comfortable, "grading"));
        h.run();
        egui_pop_test::assert_all_named(&h);
        results.add(h.try_snapshot(format!("grading-{theme}")));
    }
    let mut h = harness(options("pop-dark", Density::Comfortable, "grading"));
    h.run();
    // R resets the focused curve: one curve.change step.
    h.get_by_label("Tone curve").click_accesskit();
    h.run();
    h.key_press(egui::Key::R);
    h.run();
    let t = last_trace_of(&h, "curve.change");
    assert_eq!(t.args["curve"], "tone");
    assert_eq!(t.args["cause"], "reset");
    assert_eq!(t.args["to"], serde_json::json!([[0.0, 0.0], [1.0, 1.0]]));
    // Shift and the arrows push the gain's puck out: the page's plugin
    // keeps its chroma under the cap.
    h.get_by_label("Gain").click_accesskit();
    h.run();
    for _ in 0..8 {
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowRight);
        h.run();
    }
    let t = last_trace_of(&h, "wheel.change");
    assert_eq!(t.args["wheel"], "gain");
    assert_eq!(t.redirected_by.len(), 1);
    let chroma = t.args["to"]["chroma"].as_f64().unwrap();
    assert!(chroma <= 0.6 + 1e-6, "{chroma}");
}

#[test]
fn the_cursors_page_shows_a_brush_outline_and_the_loupe() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in ["pop-dark", "pop-light"] {
        let mut h = harness(options(theme, Density::Comfortable, "cursors"));
        h.run();
        egui_pop_test::assert_all_named(&h);
        let r = h.get_by_label("Image viewport").rect();
        // Low and left, so the loupe has room above and to the right.
        h.hover_at(r.left_bottom() + egui::vec2(r.width() * 0.3, -r.height() * 0.25));
        h.run();
        h.run();
        let snap = h.state().app.handle().debug_snapshot();
        let vp = &snap["viewport"]["viewports"]["cursors-demo"];
        assert!(
            vp["cursor_shown"]
                .as_str()
                .unwrap_or("")
                .starts_with("Outline"),
            "{vp}"
        );
        assert!(
            vp["loupe"]["shown"]["tiles"].as_u64().unwrap_or(0) > 0,
            "{vp}"
        );
        assert_eq!(vp["loupe"]["shown"]["gpu"], true, "the GPU path");
        let t = last_trace_of(&h, "cursor.resolve");
        assert_eq!(t.args["viewport"], "gallery");
        assert_eq!(t.args["query"]["tool"], "brush");
        results.add(h.try_snapshot(format!("cursors-{theme}")));
    }
}

#[test]
fn the_navigator_page_zooms_through_its_step_and_a_plugin_caps_it() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "navigator"));
    h.run();
    egui_pop_test::assert_all_named(&h);
    assert!(h.query_by_label("Navigator").is_some());
    for _ in 0..12 {
        h.get_by_label("Zoom in").click();
        h.run();
    }
    let t = last_trace_of(&h, "zoom.change");
    assert_eq!(t.args["viewport"], "gallery");
    assert_eq!(t.args["to"]["zoom"], 4.0, "capped at 400 %");
    h.get_by_label("Fit").click();
    h.run();
    assert_eq!(last_trace_of(&h, "zoom.change").args["to"]["zoom"], "fit");
    h.get_by_label("Zoom in").click();
    h.run();
    h.snapshot("navigator-pop-dark");
}

#[test]
fn the_node_graph_page_shows_fifty_nodes_and_edits_through_its_steps() {
    use egui_pop::imaging::graph::{CMD_GRAPH_EDIT, CMD_GRAPH_READ, CMD_GRAPH_VIEW};
    use egui_pop::model::commands::Origin;
    let agent = || Origin::Agent("test".into());
    let mut h = harness(options("pop-dark", Density::Comfortable, "node-graph"));
    h.run();
    egui_pop_test::assert_all_named(&h);
    assert!(h.query_by_label("Processing graph").is_some());
    assert!(h.query_by_label("Graph overview").is_some());
    let read = |h: &mut Harness<'_, Gallery>| {
        h.state_mut()
            .app
            .run(
                &CMD_GRAPH_READ,
                serde_json::json!({ "graph": "gallery" }),
                agent(),
            )
            .now()
            .expect("answered now")
            .expect("read")
    };
    let graph = read(&mut h);
    assert_eq!(graph["nodes"].as_array().map(Vec::len), Some(50));
    assert_eq!(
        graph["level"], "overview",
        "fitted, fifty nodes are an overview"
    );
    // Groups fold into stacks in the overview.
    assert!(h.query_by_label("Unfold Tone (12 nodes)").is_some());
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("node-graph-pop-dark"));

    // The page's plugin keeps the output.
    let _ = h.state_mut().app.run(
        &CMD_GRAPH_EDIT,
        serde_json::json!({ "graph": "gallery", "edit": "delete", "nodes": ["output"] }),
        agent(),
    );
    h.run();
    h.run();
    let t = last_trace_of(&h, "graph.delete");
    assert!(t.vetoed_by.is_some(), "{t:?}");
    assert_eq!(read(&mut h)["nodes"].as_array().map(Vec::len), Some(50));

    // The detail view: each node's parameters, edited inline.
    for args in [
        serde_json::json!({ "graph": "gallery", "level": "detail" }),
        serde_json::json!({ "graph": "gallery", "focus": "exposure-1" }),
    ] {
        let _ = h.state_mut().app.run(&CMD_GRAPH_VIEW, args, agent());
    }
    h.run();
    h.run();
    assert_eq!(read(&mut h)["level"], "detail");
    // Every exposure node's slider (the first in view).
    let sliders: Vec<_> = h.query_all_by_label("Exposure").collect();
    assert!(!sliders.is_empty());
    assert!(
        sliders
            .iter()
            .all(|n| n.accesskit_node().role() == egui::accesskit::Role::Slider)
    );
    egui_pop_test::assert_all_named(&h);
    results.add(h.try_snapshot("node-graph-detail-pop-dark"));
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
fn the_palettes_page_floats_a_toolbox_a_colour_palette_and_a_navigator() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "palettes"));
    h.run();
    let app = h.state().app.handle().clone();
    let toolbox = egui_pop::model::ids::PanelId::from_static("gallery.toolbox");
    let navigator = egui_pop::model::ids::PanelId::from_static("gallery.navigator");
    assert!(
        !app.panel_visible(&toolbox),
        "closed until the page opens it"
    );
    h.get_by_label("Open the palettes").click();
    h.run();
    h.run();
    assert!(app.layout().floating_of(&toolbox).is_some());
    assert!(app.layout().floating_of(&navigator).is_some());
    h.get_by_label("Brush").click();
    h.run();
    h.get_by_label("Blue").click();
    h.run();
    h.remove_cursor();
    h.run();
    egui_pop_test::assert_all_named(&h);
    assert!(h.query_by_label_contains("Tool: Brush").is_some());
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("palettes-pop-dark"));
    // The navigator, dropped on the playground's side bar, docks there.
    let a = h.get_by_label("Navigator").rect().center();
    let b = h.get_by_label("Playground").rect().center();
    h.drag_at(a);
    h.run();
    for i in 1..=4 {
        h.hover_at(a + (b - a) * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(b);
    h.run();
    h.run();
    assert!(app.layout().floating_of(&navigator).is_none());
    assert_eq!(
        app.layout().area_of(&navigator),
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
fn the_context_stack_page_drills_down_switches_modes_and_comes_back() {
    use egui_pop_gallery::pages::contexts::{RATINGS, SECOND};
    let mut h = harness(options("pop-dark", Density::Comfortable, "contexts"));
    h.run();
    let app = h.state().app.handle().clone();
    assert_eq!(context_key(&h, "place"), "root");
    // Three levels down; the breadcrumb follows.
    h.get_by_label("Open the album Holidays").click();
    h.run();
    h.get_by_label("Open photo 3").click();
    h.run();
    assert_eq!(context_key(&h, "place"), "gallery.photo");
    assert_eq!(context_key(&h, "placeDepth"), "2");
    assert!(h.query_by_label("Album Holidays").is_some());
    assert!(h.query_by_label("Photo 3").is_some());
    // An orientation set: its panel shows, its keys work.
    h.get_by_label("Culling").click();
    h.run();
    assert_eq!(context_key(&h, "mode.orientation"), "gallery.culling");
    assert!(app.panel_visible(&RATINGS));
    h.key_press(egui::Key::P);
    h.run();
    assert_eq!(context_key(&h, "galleryContextKey"), "picked");
    // A compare pushed on top.
    h.get_by_label("Compare").click();
    h.run();
    h.run();
    assert!(app.panel_visible(&SECOND));
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("contexts-pop-dark"));
    // Escape pops it: its panel goes, the orientation stays.
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!app.panel_visible(&SECOND));
    assert_eq!(context_key(&h, "mode.orientation"), "gallery.culling");
    // Reviewing: the same key approves.
    h.get_by_label("Review").click();
    h.run();
    assert!(!app.panel_visible(&RATINGS));
    h.key_press(egui::Key::P);
    h.run();
    assert_eq!(context_key(&h, "galleryContextKey"), "approved");
    // A locked photo: its guard refuses, nothing moves.
    h.get_by_label("Lock the photos (their guard refuses to leave)")
        .click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowLeft);
    h.run();
    assert_eq!(context_key(&h, "place"), "gallery.photo");
    assert!(
        h.query_all_by_label_contains("the photo is locked")
            .next()
            .is_some()
    );
    h.get_by_label("Lock the photos (their guard refuses to leave)")
        .click();
    h.run();
    // The root crumb jumps back up.
    h.get_by_label("All photos").click();
    h.run();
    assert_eq!(context_key(&h, "place"), "root");
    assert_eq!(context_key(&h, "mode.orientation"), "gallery.review");
    egui_pop_test::assert_all_named(&h);
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
    // A row's context menu acts on that row, the selection stays.
    h.get_by_label("2024").click_secondary();
    h.run();
    h.get_by_label("Lock or unlock").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Unlock 2024").is_some());
    assert!(h.query_by_label("Selected: cover.jpg").is_some());
    let t = last_step(&h, "tree.context").expect("ran");
    assert_eq!(t.args["key"], "2024");
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
    // Into a locked folder: refused at the call site.
    drag_to(&mut h, "cover.jpg", "2023", 0.5);
    let t = last_step(&h, "tree.drop").expect("ran");
    assert_eq!(t.result, Some(Err("2023 is locked".to_string())), "{t:?}");
    assert!(h.query_by_label("cover.jpg").is_some(), "still in Photos");
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
fn the_tree_page_selects_a_photo_before_its_folder_loads() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "tree"));
    run_until(&mut h, |h| h.query_by_label("Photos").is_some());
    h.get_by_label("Select a 2024 photo").click();
    h.run();
    assert!(h.query_by_label("Selected: nothing").is_some(), "queued");
    run_until(&mut h, |h| {
        h.query_by_label("Selected: IMG_2024_0005.raw").is_some()
    });
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

#[test]
fn the_text_editor_page_edits_searches_through_the_keymap_and_vetoes_the_title() {
    use egui::{Key, Modifiers};
    let mut h = harness(options("pop-dark", Density::Comfortable, "text-editor"));
    h.run();
    h.get_by_label("Notes").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.event(egui::Event::Text("\n- archive".into()));
    h.run();
    assert!(h.query_by_label_contains("version 1, modified").is_some());
    // Ctrl+F is the `editor.find` command, bound on the editor surface.
    h.key_press_modifiers(Modifiers::COMMAND, Key::F);
    h.run();
    let k = h.state().app.handle().key_traces();
    let last = k.last().expect("a trace");
    assert_eq!(last.scope.as_deref(), Some("surface:editor"), "{last:?}");
    assert!(format!("{last:?}").contains("editor.find"), "{last:?}");
    h.get_by_label("Find").type_text("mill");
    h.run();
    assert!(h.query_by_label("1 of 2").is_some());
    // The title line, locked by a before-hook on edit.change.
    h.get_by_label("Lock the title line (a before-hook on edit.change)")
        .click();
    h.run();
    h.get_by_label("Notes").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Home);
    h.event(egui::Event::Text("X".into()));
    h.run();
    let t = last_step(&h, "edit.change").expect("ran");
    assert!(t.vetoed_by.is_some(), "{t:?}");
    assert!(
        h.query_by_label("edit.change refused: the title is locked")
            .is_some()
    );
    h.remove_cursor();
    h.run();
    h.snapshot("text-editor-pop-dark");
}

#[test]
fn the_code_editor_page_marks_completes_saves_and_holds_a_hundred_thousand_lines() {
    use egui::{Key, Modifiers};
    let mut h = harness(options("pop-dark", Density::Comfortable, "code-editor"));
    h.run();
    h.get_by_label("Run the checker").click();
    h.run();
    let snapshot = h.state().app.handle().debug_snapshot();
    assert_eq!(
        snapshot["code_diagnostics"]["gallery.main.rs"][0]["message"],
        "unused variable: `unused`"
    );
    h.get_by_label("main.rs").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::End);
    h.key_press(Key::Enter);
    h.event(egui::Event::Text("hi".into()));
    run_until(&mut h, |h| h.query_by_label("highlights").is_some());
    h.key_press(Key::Enter);
    h.run();
    let text = h.state().app.handle().debug_snapshot()["edit_changes"]["changes"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(text["kind"], "completion", "{text}");
    // F9 and Ctrl+S through the keymap, on the editor surface.
    h.key_press(Key::F9);
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run();
    let t = last_step(&h, "edit.save").expect("saved");
    assert_eq!(t.args["editor"], "gallery.main.rs");
    let editors = &h.state().app.handle().debug_snapshot()["edit_changes"]["editors"];
    assert_eq!(
        editors[0]["breakpoints"],
        serde_json::json!([13, 18]),
        "{editors}"
    );
    h.remove_cursor();
    h.run();
    h.snapshot("code-editor-pop-dark");
    h.get_by_label("Load 100 000 lines").click();
    h.run();
    assert!(
        h.query_by_label_contains("const C99999").is_none(),
        "virtualised"
    );
}

#[test]
fn the_progress_page_gives_values_and_ranges_and_still_frames() {
    use egui::accesskit::Role;
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in ["pop-dark", "pop-light"] {
        let mut h = harness(options(theme, Density::Comfortable, "progress"));
        h.run();
        results.add(h.try_snapshot(format!("progress-{theme}")));
    }
    let mut h = harness(Options {
        reduced_motion: true,
        ..options("pop-dark", Density::Comfortable, "progress")
    });
    h.run();
    results.add(h.try_snapshot("progress-reduced-pop-dark"));
    let node = |h: &Harness<'_, Gallery>, role: Role, label: &str| {
        let n = h.get_by_role_and_label(role, label);
        let a = n.accesskit_node();
        (
            a.numeric_value(),
            a.min_numeric_value(),
            a.max_numeric_value(),
        )
    };
    let p = Role::ProgressIndicator;
    assert_eq!(
        node(&h, p, "Exporting 12 images"),
        (Some(42.0), Some(0.0), Some(100.0))
    );
    assert_eq!(
        node(&h, p, "Importing photos"),
        (Some(340.0), Some(0.0), Some(1204.0))
    );
    // Indeterminate: no value.
    assert_eq!(node(&h, p, "Scanning the library"), (None, None, None));
    assert_eq!(node(&h, p, "Waiting for the GPU"), (None, None, None));
    assert_eq!(
        node(&h, p, "Rendering, a quarter"),
        (Some(25.0), Some(0.0), Some(100.0))
    );
    assert_eq!(
        node(&h, Role::Meter, "GPU memory"),
        (Some(95.0), Some(0.0), Some(100.0))
    );
    // The delayed spinner: nothing before 300 ms.
    let spinner =
        |h: &Harness<'_, Gallery>| h.query_by_role_and_label(p, "Loading thumbnails").is_some();
    h.get_by_label("Load for 2 s").click();
    h.run();
    egui_pop::core::motion::freeze(&h.ctx, 0.25);
    h.run();
    assert!(!spinner(&h));
    egui_pop::core::motion::freeze(&h.ctx, 0.35);
    h.run();
    assert!(spinner(&h));
}

#[test]
fn a_command_the_host_answers_later_keeps_its_button_busy() {
    use egui_pop::core::motion;
    use egui_pop_gallery::pages::progress::CMD_SLOW_EXPORT;
    let mut h = harness(options("pop-dark", Density::Comfortable, "progress"));
    h.run();
    h.get_by_label("Export").click();
    h.run();
    let app = h.state().app.handle().clone();
    assert!(app.running(&CMD_SLOW_EXPORT));
    let button = h.get_by_label("Export");
    assert!(button.accesskit_node().is_disabled());
    assert!(button.accesskit_node().is_busy());
    // After the delay, a spinner in the icon's place.
    motion::freeze(&h.ctx, 0.4);
    h.run();
    h.snapshot("progress-busy-pop-dark");
    h.get_by_label("Refuse them").click();
    h.run();
    assert!(!app.running(&CMD_SLOW_EXPORT));
    assert_eq!(
        app.history().last().unwrap().result,
        Err("the export folder is read-only".to_string())
    );
    assert!(!h.get_by_label("Export").accesskit_node().is_busy());
}

#[test]
fn skeletons_are_busy_until_the_content_loads_and_empty_states_act() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "placeholders"));
    h.run();
    h.snapshot("placeholders-pop-dark");
    let queue = h.get_by_label("Loading the queue");
    assert!(queue.accesskit_node().is_busy());
    h.get_by_label("Import images").click();
    h.run();
    h.get_by_label("Retry").click();
    h.run();
    assert!(
        h.query_by_label_contains("import 1 times, retry 1 times")
            .is_some()
    );
    h.get_by_label("Loaded").click();
    h.run();
    assert!(h.query_by_label("Loading the queue").is_none());
    assert!(h.query_by_label("IMG_0001.raw").is_some());
}

#[test]
fn the_motion_page_shows_the_busy_indicator_after_its_delay_for_its_minimum() {
    use egui_pop::core::motion;
    let mut h = harness(options("pop-dark", Density::Comfortable, "motion"));
    h.run();
    let at = |h: &mut Harness<'_, Gallery>, seconds: f64| {
        motion::freeze(&h.ctx, seconds);
        h.run();
    };
    assert!(h.query_by_label_contains("a given clock").is_some());
    h.get_by_label("Work for 1 s").click();
    h.run();
    at(&mut h, 0.2);
    assert!(
        h.query_by_label_contains("Busy indicator: hidden")
            .is_some()
    );
    at(&mut h, 0.35);
    assert!(h.query_by_label_contains("Busy indicator: shown").is_some());
    // The work ended at 1 s; shown since 0.35 s, the minimum is met.
    at(&mut h, 1.2);
    assert!(
        h.query_by_label_contains("Busy indicator: hidden")
            .is_some()
    );
    // Short work never shows it.
    h.get_by_label("Work for 0.2 s").click();
    h.run();
    for t in [1.3, 1.45, 2.0] {
        at(&mut h, t);
        assert!(
            h.query_by_label_contains("(shown 1 times)").is_some(),
            "{t}"
        );
    }
}

#[test]
fn the_jobs_page_folds_a_pretend_hosts_jobs() {
    use egui_pop::api::jobs::JOBS;
    use egui_pop::model::jobs::JobState;
    let mut h = harness(options("pop-dark", Density::Comfortable, "jobs"));
    h.run();
    h.get_by_label("Start the demo jobs").click();
    h.run();
    for _ in 0..2 {
        h.get_by_label("Advance one step").click();
        h.run();
    }
    // The application estimates the scan while it cannot.
    h.get_by_label("Estimate the scan (a hook on jobs.estimate)")
        .click();
    h.run();
    h.get_by_label("Advance one step").click();
    h.run();
    let app = h.state().app.handle().clone();
    let jobs = app.service(&JOBS).unwrap();
    let fraction = |id: &str| jobs.job(id).unwrap().fraction;
    // 3 of 12 exported.
    assert_eq!(fraction("gallery-export"), Some(0.25));
    // Decode done (1), a quarter of process (3 x 0.25), of 5.
    assert!((fraction("gallery-render").unwrap() - 0.35).abs() < 1e-9);
    // The scan does not know its size yet: 90 folders of about 500.
    assert!((fraction("gallery-scan").unwrap() - 90.0 / 500.0).abs() < 1e-9);
    // The first folder 3 of 4, two more to come.
    assert_eq!(fraction("gallery-import"), Some(0.25));
    assert_eq!(jobs.job("gallery-upload").unwrap().state, JobState::Failed);
    assert!(h.query_by_label_contains("connection reset").is_some());
    // The jobs centre's buttons, named with their job.
    h.get_by_label("Cancel: Export 12 images").click_accesskit();
    h.run();
    assert_eq!(
        jobs.job("gallery-export").unwrap().state,
        JobState::Cancelled
    );
    h.get_by_label("Retry: Upload to the server")
        .click_accesskit();
    h.run();
    let upload = jobs.job("gallery-upload").unwrap();
    assert_eq!((upload.state, upload.attempts), (JobState::Running, 2));
    // Once the scan knows its total, the model's own estimate is back.
    h.get_by_label("Advance one step").click();
    h.run();
    assert_eq!(fraction("gallery-scan"), Some(0.2));
}

#[test]
fn the_jobs_centre_opens_a_jobs_log_console_in_the_page_and_the_popover() {
    use egui::accesskit::Role;
    use egui_pop::api::jobs::JOBS;
    let mut h = harness(options("pop-dark", Density::Comfortable, "jobs"));
    h.run();
    h.get_by_label("Start the demo jobs").click();
    h.run();
    for _ in 0..3 {
        h.get_by_label("Advance one step").click();
        h.run();
    }
    let app = h.state().app.handle().clone();
    let jobs = app.service(&JOBS).unwrap();
    // The chevron opens the export's console: a terminal over its log.
    h.get_by_label("Show the log of Export 12 images")
        .click_accesskit();
    h.run();
    assert!(jobs.expanded("gallery-export"));
    let console = h.get_by_label("Log of Export 12 images");
    assert_eq!(console.accesskit_node().role(), Role::Terminal);
    h.snapshot("jobs-pop-dark");
    // The status bar's item opens the same list; the console is open
    // there too, its state remembered per job.
    h.get_by_label_contains("Jobs: ").click();
    h.run();
    assert_eq!(
        h.query_all_by_label("Hide the log of Export 12 images")
            .count(),
        2
    );
    h.snapshot("jobs-popover-pop-dark");
    // Closed from the popover, closed everywhere.
    h.query_all_by_label("Hide the log of Export 12 images")
        .last()
        .unwrap()
        .click_accesskit();
    h.run();
    assert!(!jobs.expanded("gallery-export"));
    // A toast carries the export's progress and stays while it runs.
    h.get_by_label("Show the export as a toast")
        .click_accesskit();
    h.run();
    // On screen and in the live region screen readers hear.
    assert!(h.query_all_by_label("Exporting 12 images").count() >= 1);
    let toasts = app.service(&egui_pop::api::toasts::TOASTS).unwrap();
    assert_eq!(toasts.shown().len(), 1);
    assert_eq!(
        toasts.shown()[0].toast.job.as_deref(),
        Some("gallery-export")
    );
    // Its bar is the export's, named after the job, beside the centre's.
    assert!(
        h.query_all_by_role_and_label(Role::ProgressIndicator, "Export 12 images")
            .count()
            >= 2
    );
}

#[test]
fn the_terminal_page_shows_a_log_and_routes_keys_through_hooks_and_the_keymap() {
    use egui::{Key, Modifiers};
    use egui_pop::editors::terminal::steps::CMD_READ;
    use egui_pop::model::commands::Origin;
    let mut h = harness(options("pop-dark", Density::Comfortable, "terminal"));
    h.run();
    h.remove_cursor();
    h.run();
    h.snapshot("terminal-pop-dark");
    let read = |h: &mut Harness<'_, Gallery>, id: &str| {
        h.state_mut()
            .app
            .run(
                &CMD_READ,
                serde_json::json!({ "terminal": id }),
                Origin::Agent("test".into()),
            )
            .now()
            .expect("answered now")
            .expect("read")
    };
    let log = read(&mut h, "gallery.build-log");
    assert!(
        log["text"].as_str().unwrap().contains("Compiling exposure"),
        "{log}"
    );
    assert_eq!(log["writable"], false);
    // A redirect on terminal.input upper-cases the demo program's input.
    h.get_by_label("Upper-case the input (a redirect on terminal.input)")
        .click();
    h.run();
    h.get_by_label("Echo").click();
    h.run();
    h.event(egui::Event::Text("hi".into()));
    h.key_press(Key::Enter);
    h.run();
    let t = last_step(&h, "terminal.input").expect("ran");
    assert!(!t.redirected_by.is_empty(), "{t:?}");
    let echo = read(&mut h, "gallery.echo");
    assert!(
        echo["text"].as_str().unwrap().contains("you said: HI"),
        "{echo}"
    );
    // Ctrl+Shift chords stay the keymap's: terminal.clear, on the
    // terminal surface.
    h.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::K);
    h.run();
    let k = h.state().app.handle().key_traces();
    let last = k.last().expect("a trace");
    assert_eq!(last.scope.as_deref(), Some("surface:terminal"), "{last:?}");
    assert_eq!(read(&mut h, "gallery.echo")["text"], "");
}

#[test]
fn the_library_page_culls_filters_and_compares() {
    use egui::accesskit::Role;
    let mut h = harness(options("pop-dark", Density::Comfortable, "library"));
    h.run();
    assert_eq!(
        h.query_all_by_label("DSC_00000").count(),
        2,
        "in the grid and the filmstrip"
    );
    assert!(
        h.query_all_by_label("DSC_09999").next().is_none(),
        "virtualised"
    );
    // A click, then a rating key: the step runs for that image.
    h.query_all_by_label("DSC_00001").next().unwrap().click();
    h.run();
    h.key_press(egui::Key::Num4);
    h.run();
    let t = last_step(&h, "library.rate").expect("ran");
    assert_eq!(t.args["rating"], serde_json::json!(4));
    assert_eq!(t.args["keys"], serde_json::json!([1]));
    // A criterion typed in the search field, then a chip.
    h.get_by_role_and_label(Role::TextInput, "Search images")
        .click();
    h.run();
    h.event(egui::Event::Text("rating>=5".into()));
    h.run();
    let t = last_step(&h, "library.filter").expect("ran");
    assert_eq!(t.args["filter"]["min_rating"], serde_json::json!(5));
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.query_by_label("Rating ≥ 5").is_some());
    h.get_by_label("Remove Rating ≥ 5").click();
    h.run();
    assert_eq!(
        last_step(&h, "library.filter").unwrap().args["filter"],
        serde_json::json!({})
    );
    h.remove_cursor();
    h.run();
    h.snapshot("library-pop-dark");
    // The comparison: the first three images, one marked the best.
    h.get_by_label("Compare").click();
    h.run();
    h.query_all_by_label("Best of the comparison")
        .next()
        .unwrap()
        .click();
    h.run();
    h.remove_cursor();
    h.run();
    h.snapshot("library-compare-pop-dark");
}

#[test]
fn the_theme_editor_page_edits_a_token_measures_it_and_undoes() {
    use egui_pop::api::theme_editor::{
        CMD_THEME_READ, CMD_THEME_SET, CMD_THEME_UNDO, THEME_EDITOR,
    };
    use egui_pop::model::commands::Origin;
    let agent = || Origin::Agent("test".into());
    let mut results = egui_kittest::SnapshotResults::new();
    for (theme, token) in [
        ("pop-dark", "accent.default"),
        ("pop-light", "surface.panel"),
    ] {
        let mut h = harness(options(theme, Density::Comfortable, "theme-editor"));
        h.run();
        let editor = h.state().app.handle().service(&THEME_EDITOR).unwrap();
        editor.select(token);
        h.run();
        egui_pop_test::assert_all_named(&h);
        assert!(h.query_by_label(&format!("Value of {token}")).is_some());
        results.add(h.try_snapshot(format!("theme-editor-{theme}")));
    }

    let mut h = harness(options("pop-dark", Density::Comfortable, "theme-editor"));
    h.run();
    // Accent fills as dark as the label on them: refused by the pairs.
    let on_accent = Tokens::of(&h.ctx)
        .resolved()
        .color("text.on-accent")
        .unwrap();
    let out = h
        .state_mut()
        .app
        .run(
            &CMD_THEME_SET,
            serde_json::json!({ "path": "accent.default", "value": on_accent.to_hex() }),
            agent(),
        )
        .now()
        .unwrap()
        .unwrap();
    assert_eq!(out["changed"], true);
    h.run();
    let read = |h: &mut Harness<'_, Gallery>| {
        h.state_mut()
            .app
            .run(&CMD_THEME_READ, serde_json::json!({}), agent())
            .now()
            .unwrap()
            .unwrap()
    };
    let failing = read(&mut h)["failing"].to_string();
    assert!(
        failing.contains("text.on-accent on accent.default"),
        "{failing}"
    );
    assert_eq!(
        Tokens::of(&h.ctx).color("accent.default"),
        Some(egui_pop::theme::color32(on_accent))
    );
    let _ = h
        .state_mut()
        .app
        .run(&CMD_THEME_UNDO, serde_json::json!({}), agent());
    h.run();
    assert!(read(&mut h)["failing"].as_array().unwrap().is_empty());
    assert!(h.state().app.handle().theme().spec().overrides.is_empty());
}

#[test]
fn the_tips_page_dismisses_a_tip_and_remembers_it_in_the_settings() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "tips"));
    h.run();
    egui_pop_test::assert_all_named(&h);
    assert!(h.query_by_label("Go through the pages").is_some());
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("tips-pop-dark"));
    h.get_by_label("Don't show again").click();
    h.run();
    assert!(h.query_by_label("Go through the pages").is_none());
    let store = h.state().app.handle().store();
    let dismissed = egui_pop::core::lock(&store)
        .get("settings/tips.dismissed")
        .and_then(|e| e.value.as_text().map(str::to_string));
    assert_eq!(dismissed.as_deref(), Some("gallery.next-page"));
    h.get_by_label("Show every tip again").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Go through the pages").is_some());
}

#[test]
fn a_kit_redraws_the_workbench_tabs_from_the_theme_alone() {
    // A kit folder that only changes the tab component's data and shape:
    // no code, the `underline` handler egui-pop ships.
    let kit = egui_pop::tokens::kit!("../../../examples/kits/underline-tabs");
    assert!(egui_pop::tokens::kit::check(&kit).is_empty());
    let mut themes = egui_pop::tokens::Themes::default();
    themes.add_kit(&kit);
    let mut h = harness(options("pop-dark", Density::Comfortable, "workbench"));
    h.run();
    h.get_by_label("Open the demonstration panels").click();
    h.run();
    let tokens = Tokens::new(themes.resolve(&egui_pop::tokens::ThemeSpec::new("underline-tabs")));
    egui_pop::core::theme::install_tokens(&h.ctx, tokens);
    let kit = egui_pop::core::kit::Kit::of(&h.ctx);
    assert_eq!(kit.tab.shape, "underline");
    assert!(kit.problems.is_empty(), "{:?}", kit.problems);
    h.run();
    h.run();
    h.snapshot("workbench-kit-underline");
}

#[test]
fn a_kit_puts_the_title_at_the_start_of_the_title_bar() {
    // Data only: `title-bar.align` and the bar's colours, as tvty draws it.
    let mut themes = egui_pop::tokens::Themes::default();
    themes
        .add_json(
            "kit-title-start",
            r##"{
              "$extends": "pop-dark",
              "title-bar": {
                "align": {"$type": "string", "$value": "start"},
                "rest": {
                  "text": {"$type": "color", "$value": "{text.muted}"},
                  "border-width": {"$type": "dimension", "$value": {"value": 1, "unit": "px"}}
                }
              }
            }"##,
        )
        .expect("the test theme reads");
    let mut h = harness(options("pop-dark", Density::Comfortable, "frame"));
    h.run();
    let tokens = Tokens::new(themes.resolve(&egui_pop::tokens::ThemeSpec::new("kit-title-start")));
    egui_pop::core::theme::install_tokens(&h.ctx, tokens);
    let kit = egui_pop::core::kit::Kit::of(&h.ctx);
    assert_eq!(kit.title_bar.data.align, egui_pop::core::kit::Align::Start);
    h.run();
    h.run();
    h.snapshot("frame-kit-title-start");
}
