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

// Not critical: it renders every page (over 30 s on CI); it runs whenever
// the gallery or a crate it uses changes.
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
    // The switchers pick a theme; it is installed between frames.
    let ctx = h.ctx.clone();
    h.state().app.handle().theme().pick(&ctx, "pop-light");
    h.run();
    let tokens = Tokens::of(&h.ctx);
    assert_eq!(tokens.resolved().spec().base, "pop-light");
    assert!(!h.ctx.global_style().visuals.dark_mode);
    // Its pair and the light or dark preference are in the picker.
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Theme")
        .click();
    h.run();
    assert!(h.query_by_label("Pop dark (its pair)").is_some());
    h.get_by_label("Always dark").click();
    h.run();
    h.run();
    assert_eq!(
        h.state().app.handle().theme().preference(),
        egui::ThemePreference::Dark
    );
    // The pair's other theme, shown without installing again.
    assert_eq!(Tokens::of(&h.ctx).resolved().spec().base, "pop-dark");
    assert_eq!(h.state().app.handle().theme().spec().base, "pop-light");
    assert!(h.ctx.global_style().visuals.dark_mode);
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

/// Opens the card's context menu and its Zoom submenu.
fn open_card_menu(h: &mut Harness<'_, Gallery>) {
    h.run();
    h.get_by_label("Card with a context menu").click_secondary();
    h.run();
    h.query_all_by_label("Zoom").last().unwrap().hover();
    h.run();
}

#[test]
fn the_menus_gutter_aligns_icons_ticks_and_radio_dots() {
    use egui_pop::api::menus::{self, Mark};
    use egui_pop_gallery::pages::menus::{CARD_MENU, ZOOM_MENU};
    let mut results = egui_kittest::SnapshotResults::new();
    for (theme, density, gutter, name) in [
        ("pop-dark", Density::Comfortable, 24.0, None),
        ("pop-dark", Density::Compact, 16.0, Some("menus-compact")),
        (
            "pop-high-contrast",
            Density::Comfortable,
            24.0,
            Some("menus-high-contrast"),
        ),
    ] {
        let mut h = harness(options(theme, density, "menus"));
        open_card_menu(&mut h);
        let rows = menus::rows_drawn(&h.ctx);
        for menu in [CARD_MENU, ZOOM_MENU] {
            let rows: Vec<_> = rows.iter().filter(|r| r.menu == menu).collect();
            assert!(!rows.is_empty(), "{menu}");
            for r in &rows {
                assert_eq!(r.gutter, gutter, "{theme} {r:?}");
                assert!((r.label_x - rows[0].label_x).abs() < 0.01, "{theme} {r:?}");
            }
        }
        let mark = |label: &str| {
            rows.iter()
                .find(|r| r.label == label)
                .unwrap_or_else(|| panic!("{label} in {rows:?}"))
                .mark
        };
        assert_eq!(mark("Show grid"), Mark::Framed);
        assert_eq!(mark("Lock the card"), Mark::Framed);
        assert_eq!(mark("Snap to grid"), Mark::Check);
        assert_eq!(mark("Duplicate the card"), Mark::Icon);
        assert_eq!(mark("Delete the card"), Mark::Icon);
        assert_eq!(mark("100%"), Mark::Radio);
        assert_eq!(mark("50%"), Mark::None);
        if let Some(name) = name {
            results.add(h.try_snapshot(name));
        }
    }
}

#[test]
fn menu_icons_off_keeps_ticks_and_dots_in_the_gutter() {
    use egui_pop::api::menus::{self, Mark, MenuIcons, SETTING_ICONS};
    use egui_pop::model::commands::Origin;
    use egui_pop::model::values::{Change, Set, Value};
    let mut h = harness(options("pop-dark", Density::Comfortable, "menus"));
    h.run();
    egui_pop::core::lock(&h.state().app.handle().store())
        .set(Set::new(
            &format!("settings/{SETTING_ICONS}"),
            Change::To(Value::Choice(MenuIcons::Off.as_str().into())),
            Origin::User,
        ))
        .unwrap();
    open_card_menu(&mut h);
    let rows = menus::rows_drawn(&h.ctx);
    assert!(rows.iter().all(|r| r.icon.is_none()), "{rows:?}");
    let mark = |label: &str| rows.iter().find(|r| r.label == label).unwrap().mark;
    assert_eq!(mark("Show grid"), Mark::Check);
    assert_eq!(mark("100%"), Mark::Radio);
    h.snapshot("menus-icons-off");
}

#[test]
fn the_palette_shows_the_commands_icons_in_a_column() {
    use egui::accesskit::Role;
    let mut h = harness(options("pop-dark", Density::Comfortable, "menus"));
    h.run();
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::P,
    );
    h.run();
    h.query_all_by_label("Command palette")
        .find(|n| n.accesskit_node().role() == Role::TextInput)
        .expect("the palette's field")
        .type_text("card");
    h.run();
    h.run();
    let drawn = egui_pop::core::icon::registry::drawn(&h.ctx);
    let icon = |label: &str| {
        drawn
            .iter()
            .find(|d| d.surface == "palette" && d.label.ends_with(label))
            .map(|d| d.icon.clone())
    };
    assert_eq!(
        icon("Duplicate the card").as_deref(),
        Some("copy"),
        "{drawn:?}"
    );
    assert_eq!(
        icon("Lock the card").as_deref(),
        Some("unlock"),
        "{drawn:?}"
    );
    h.snapshot("palette-icons");
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
fn the_inputs_page_shows_every_decorator_and_state() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in ["pop-dark", "pop-light", "pop-high-contrast"] {
        let mut h = harness(options(theme, Density::Comfortable, "inputs"));
        h.run();
        egui_pop_test::assert_all_named(&h);
        results.add(h.try_snapshot(format!("inputs-{theme}")));
    }
    let mut h = harness(options("pop-dark", Density::Compact, "inputs"));
    h.run();
    results.add(h.try_snapshot("inputs-pop-dark-compact"));
}

fn text_input<'h>(h: &'h Harness<'_, Gallery>, label: &'h str) -> egui_kittest::Node<'h> {
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, label)
}

#[test]
fn an_input_shows_its_placeholder_and_clears_by_click_and_by_keys() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "inputs"));
    h.run();
    assert_eq!(
        text_input(&h, "Search")
            .accesskit_node()
            .data()
            .placeholder(),
        Some("Search photos")
    );
    assert_eq!(text_input(&h, "Search").value().as_deref(), Some("harbour"));
    h.get_by_label("Clear Search").click();
    h.run();
    assert_eq!(text_input(&h, "Search").value().as_deref(), Some(""));
    assert!(
        h.query_by_label("Clear Search").is_none(),
        "no clear button without a value"
    );
    assert!(text_input(&h, "Search").is_focused(), "back in the field");
    // By keys: Tab from the field reaches the ✕, Space presses it.
    text_input(&h, "Search").type_text("dunes");
    h.run();
    assert_eq!(text_input(&h, "Search").value().as_deref(), Some("dunes"));
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.get_by_label("Clear Search").is_focused());
    h.key_press(egui::Key::Space);
    h.run();
    assert_eq!(text_input(&h, "Search").value().as_deref(), Some(""));
}

#[test]
fn clearing_a_bound_input_is_one_undo_step() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "inputs"));
    h.run();
    let store = h.state().app.handle().store();
    let steps = || egui_pop::core::lock(&store).undo_steps().len();
    let before = steps();
    h.get_by_label("Clear Caption").click();
    h.run();
    assert_eq!(text_input(&h, "Caption").value().as_deref(), Some(""));
    assert_eq!(steps(), before + 1, "one step for the clear");
    egui_pop::core::lock(&store)
        .undo(&egui_pop::model::commands::Origin::User)
        .unwrap();
    h.run();
    assert_eq!(text_input(&h, "Caption").value().as_deref(), Some("Sunset"));
}

#[test]
fn field_states_reach_assistive_technologies_tooltips_and_agents() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "inputs"));
    h.run();
    let keyword = text_input(&h, "Keyword");
    assert!(keyword.accesskit_node().is_required());
    assert!(keyword.accesskit_node().data().invalid().is_some());
    let image = text_input(&h, "Image");
    assert!(image.accesskit_node().is_disabled());
    assert!(
        image
            .accesskit_node()
            .description()
            .is_some_and(|d| d.contains("Needs an open image")),
        "{:?}",
        image.accesskit_node().description()
    );
    assert!(
        text_input(&h, "Camera")
            .accesskit_node()
            .data()
            .is_read_only()
    );
    assert!(text_input(&h, "Collection").accesskit_node().is_busy());
    // The modified title offers its reset, which puts the default back.
    text_input(&h, "Title").focus();
    h.run();
    text_input(&h, "Title").type_text("!");
    h.run();
    h.get_by_label("Reset Title to default").click();
    h.run();
    assert_eq!(text_input(&h, "Title").value().as_deref(), Some("Sunset"));
    assert!(h.query_by_label("Reset Title to default").is_none());
    // The disabled reason as the tooltip.
    text_input(&h, "Image").hover();
    // Past egui's tooltip delay.
    h.run_steps(45);
    assert!(
        h.query_by_label("Unavailable: Needs an open image")
            .is_some(),
        "the reason shows on hover"
    );
    // Agents read every field's state, the sliders' too.
    let snap = h.state().app.handle().debug_snapshot();
    let fields = snap["fields"]["fields"].as_array().unwrap();
    let field = |label: &str| {
        fields
            .iter()
            .find(|f| f["label"] == label)
            .unwrap_or_else(|| panic!("{label} in {fields:?}"))
    };
    assert_eq!(field("Image")["disabled_reason"], "Needs an open image");
    assert_eq!(field("Image")["focusable"], false);
    assert_eq!(field("Album")["invalid"], "At least three characters");
    assert_eq!(field("Camera")["editable"], false);
    assert_eq!(field("Camera")["focusable"], true);
    assert_eq!(field("Denoise")["kind"], "slider");
    assert_eq!(field("Denoise")["disabled_reason"], "Only for raw files");
    assert_eq!(field("White balance")["marks"][0], "read-only");
}

#[test]
fn a_read_only_slider_takes_the_focus_and_refuses_changes() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "inputs"));
    h.run();
    h.get_by_label("White balance").focus();
    h.run();
    assert!(h.get_by_label("White balance").is_focused());
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert_eq!(
        h.get_by_label("White balance")
            .accesskit_node()
            .numeric_value(),
        Some(5600.0)
    );
    assert!(h.get_by_label("Denoise").accesskit_node().is_disabled());
}

/// Presses a key (with Shift when `back`) and lets the focus settle.
fn tab(h: &mut Harness<'_, Gallery>, key: egui::Key, back: bool) {
    if back {
        h.key_press_modifiers(egui::Modifiers::SHIFT, key);
    } else {
        h.key_press(key);
    }
    h.run();
}

fn focused_label(h: &Harness<'_, Gallery>, labels: &[&str]) -> String {
    labels
        .iter()
        .find(|l| h.query_all_by_label(l).any(|n| n.is_focused()))
        .map_or_else(|| "nothing".to_string(), |l| l.to_string())
}

#[test]
fn tab_follows_reading_order_and_groups_and_f6_moves_between_panels() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "inputs"));
    h.run();
    let all = [
        "Street",
        "City",
        "Postcode",
        "Country",
        "Tags",
        "Quality",
        "Size",
        "Open the form in a modal",
    ];
    text_input(&h, "Street").focus();
    h.run();
    let mut seen = vec![focused_label(&h, &all)];
    for _ in 0..3 {
        tab(&mut h, egui::Key::Tab, false);
        seen.push(focused_label(&h, &all));
    }
    assert_eq!(seen, ["Street", "City", "Postcode", "Country"]);
    tab(&mut h, egui::Key::Tab, true);
    assert_eq!(focused_label(&h, &all), "Postcode");
    // F6: the other panel, its first stop; the group keeps Tags and
    // Quality together.
    tab(&mut h, egui::Key::F6, false);
    let mut seen = vec![focused_label(&h, &all)];
    for _ in 0..3 {
        tab(&mut h, egui::Key::Tab, false);
        seen.push(focused_label(&h, &all));
    }
    assert_eq!(
        seen,
        ["Tags", "Quality", "Size", "Open the form in a modal"]
    );
    // F6 again: back to the address, where the focus was.
    tab(&mut h, egui::Key::F6, false);
    assert_eq!(focused_label(&h, &all), "Postcode");
    let order = egui_pop::core::tab::order(&h.ctx, "inputs-options");
    assert_eq!(order.len(), 4, "one stop per field and the button");
}

#[test]
fn tab_cycles_inside_a_modal() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "inputs"));
    h.run();
    h.get_by_label("Open the form in a modal").click();
    h.run();
    let all = ["Book title", "Author", "OK", "Cancel"];
    text_input(&h, "Book title").focus();
    h.run();
    let mut seen = vec![focused_label(&h, &all)];
    for _ in 0..4 {
        tab(&mut h, egui::Key::Tab, false);
        seen.push(focused_label(&h, &all));
    }
    assert_eq!(seen, ["Book title", "Author", "OK", "Cancel", "Book title"]);
    tab(&mut h, egui::Key::Tab, true);
    assert_eq!(focused_label(&h, &all), "Cancel");
    // F6 does not leave a modal.
    tab(&mut h, egui::Key::F6, false);
    assert_eq!(focused_label(&h, &all), "Cancel");
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.query_by_label("Modal answered: cancelled").is_some());
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
fn the_settings_window_shows_the_gallerys_custom_page() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "settings"));
    h.run();
    h.get_by_label("Open the settings window (Ctrl+,)").click();
    h.run();
    h.run();
    // Under the gallery's section, found by a keyword (the search field
    // is labelled by the window's title; the page behind has fields too;
    // clicked first: an accessibility focus alone does not give it the
    // keyboard).
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Settings")
        .click();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Settings")
        .type_text("specimen");
    h.run();
    h.run();
    h.get_by_label("Open the Greeting preview page").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Reset Size").is_some());
    assert!(h.query_by_label("Reset Strong").is_some());
    assert!(h.query_by_label("Reset Greeting").is_none());
    egui_pop_test::assert_all_named(&h);
    let snap = h.state().app.handle().debug_snapshot();
    assert_eq!(
        snap["settings_window"]["section"],
        serde_json::json!(egui_pop_gallery::pages::settings::PREVIEW_PAGE)
    );
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
    } else if page == "preview" {
        // Its tiles are computed when asked: a few frames' work.
        for _ in 0..8 {
            let _ = h.run_ok();
        }
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
fn in_full_screen_the_gallerys_own_title_bar_stays_with_its_way_out() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "frame"));
    h.run();
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
    h.input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .fullscreen = Some(true);
    h.run();
    // The logo and the menus stay; close and the way out of full screen,
    // no minimize or maximize (the preview on the page keeps its own).
    assert!(h.query_by_label("Window menu").is_some());
    assert_eq!(h.query_all_by_label("Exit full screen").count(), 1);
    assert_eq!(h.query_all_by_label("Minimize").count(), 1);
    assert_eq!(h.query_all_by_label("Close").count(), 2);
    let r = egui_pop::core::frame::regions(&h.ctx).expect("published");
    assert!(r.fullscreen && r.caption.is_some() && r.grip == 0, "{r:?}");
    h.snapshot("frame-full-screen-pop-dark");
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
fn the_node_graph_page_routes_wires_around_the_nodes() {
    use egui_pop::imaging::graph::CMD_GRAPH_READ;
    use egui_pop::model::commands::Origin;
    let mut h = harness(options("pop-dark", Density::Comfortable, "node-graph"));
    h.run();
    h.get_by_label("Orthogonal wires").click();
    h.run();
    h.run();
    let graph = h
        .state_mut()
        .app
        .run(
            &CMD_GRAPH_READ,
            serde_json::json!({ "graph": "gallery" }),
            Origin::Agent("test".into()),
        )
        .now()
        .expect("answered now")
        .expect("read");
    assert_eq!(graph["wires"], "orthogonal");
    let routes = graph["routes"].as_array().expect("the routes");
    assert!(!routes.is_empty());
    for r in routes {
        let points = r["points"].as_array().expect("points");
        assert!(points.len() >= 2, "{r}");
        for w in points.windows(2) {
            assert!(w[0][0] == w[1][0] || w[0][1] == w[1][1], "slanted: {r}");
        }
    }
    egui_pop_test::assert_all_named(&h);
    h.snapshot("node-graph-orthogonal-pop-dark");
}

#[test]
fn the_node_graph_page_exports_the_graph_as_svg() {
    use egui_pop::imaging::graph::{CMD_GRAPH_EXPORT, CMD_GRAPH_SELECT};
    use egui_pop::model::commands::Origin;
    let mut h = harness(options("pop-dark", Density::Comfortable, "node-graph"));
    h.run();
    assert!(
        h.query_by_label("Export").is_some(),
        "the page's Export menu"
    );
    let export = |h: &mut Harness<'static, Gallery>, args: serde_json::Value| {
        h.state_mut()
            .app
            .run(&CMD_GRAPH_EXPORT, args, Origin::Agent("test".into()))
            .now()
            .expect("answered now")
            .expect("exported")["svg"]
            .as_str()
            .expect("the svg")
            .to_string()
    };
    let svg = export(&mut h, serde_json::json!({ "graph": "gallery" }));
    assert!(svg.starts_with("<svg "), "{svg}");
    assert_eq!(svg.matches("<g data-node=").count(), 50);
    assert_eq!(
        svg,
        export(&mut h, serde_json::json!({ "graph": "gallery" }))
    );
    let _ = h.state_mut().app.run(
        &CMD_GRAPH_SELECT,
        serde_json::json!({ "graph": "gallery", "nodes": ["blur-1", "sharpen-1"] }),
        Origin::Agent("test".into()),
    );
    h.run();
    let svg = export(
        &mut h,
        serde_json::json!({ "graph": "gallery", "selection": true, "level": "detail" }),
    );
    assert_eq!(svg.matches("<g data-node=").count(), 2, "{svg}");
    assert_eq!(svg.matches("data-edge=").count(), 1, "{svg}");
    assert!(svg.contains("data-param=\"radius\""), "{svg}");
}

#[test]
fn the_graph_file_page_writes_an_edit_into_the_file() {
    use egui_pop::imaging::graph::CMD_GRAPH_EDIT;
    use egui_pop::model::commands::Origin;
    let mut h = harness(options("pop-dark", Density::Comfortable, "graph-file"));
    h.run();
    egui_pop_test::assert_all_named(&h);
    assert!(h.query_by_label("Graph file").is_some());
    assert!(
        h.query_by_label_contains("5 nodes, 5 wires, format 1, 0 edits")
            .is_some()
    );
    assert!(h.query_by_label_contains("    pos: [240, 0]\n").is_some());
    // An agent moves a node: a step, then the line in the file.
    let _ = h.state_mut().app.run(
        &CMD_GRAPH_EDIT,
        serde_json::json!({
            "graph": "file",
            "edit": "move",
            "moves": [{ "node": "blur", "to": [260.0, 20.0] }],
        }),
        Origin::Agent("test".into()),
    );
    h.run();
    h.run();
    let t = last_trace_of(&h, "graph.move");
    assert_eq!(t.args["graph"], "file");
    assert!(h.query_by_label_contains("1 edits").is_some());
    assert!(h.query_by_label_contains("    pos: [260, 20]\n").is_some());
    // The comments stay.
    assert!(
        h.query_by_label_contains("radius: 3.5 # softer than the kind's")
            .is_some()
    );
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
fn the_workbench_page_shows_a_dragged_tab_s_ghost_where_it_would_land() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "workbench"));
    h.run();
    h.get_by_label("Open the demonstration panels").click();
    h.run();
    h.run();
    let app = h.state().app.handle().clone();
    let notes = egui_pop::model::ids::PanelId::from_static("gallery.notes");
    // The tabs (buttons), not the placement table's rows.
    let tab = |h: &Harness<'_, Gallery>, name: &str| {
        h.get_by_role_and_label(egui::accesskit::Role::Button, name)
            .rect()
    };
    let properties = tab(&h, "Properties");
    let notes_tab = tab(&h, "Notes");
    // The Notes tab, from the bottom panel over the end side bar, held
    // there.
    let a = notes_tab.center();
    let b = h
        .get_by_label("Properties of the selection.")
        .rect()
        .center();
    // Event by event, a frame each, as egui_dock follows a tab's drag.
    let pointer = |h: &mut Harness<'_, Gallery>, e: egui::Event| {
        h.input_mut().events.push(e);
        h.step();
    };
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    pointer(&mut h, egui::Event::PointerMoved(a));
    pointer(&mut h, button(a, true));
    for i in 1..=8 {
        pointer(
            &mut h,
            egui::Event::PointerMoved(a + (b - a) * (i as f32 / 8.0)),
        );
    }
    h.step();
    let hint = app.drop_hint().expect("a hint while dragging");
    assert_eq!((hint.panel.clone(), hint.area), (notes.clone(), "end_side"));
    let ghost = hint.preview.expect("the tab's ghost");
    assert_eq!(
        (ghost.area, ghost.index),
        ("end_side", 2),
        "after Inspector and Properties"
    );
    assert!(ghost.rect.left() >= properties.right() - 0.5);
    assert!((ghost.rect.top() - properties.top()).abs() < 0.5);
    let snap = app.debug_snapshot();
    assert_eq!(snap["workbench"]["drop_hint"]["preview"]["index"], 2);
    h.snapshot("workbench-drag-pop-dark");
    pointer(&mut h, button(b, false));
    h.run();
    assert!(app.drop_hint().is_none());
    assert_eq!(
        app.layout().area_of(&notes),
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
    // Before the drop, the ghost of its tab where it would land.
    let hint = app.drop_hint().expect("a hint while dragging");
    let ghost = hint.preview.expect("a ghost tab where it may dock");
    assert_eq!(ghost.area, "end_side");
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
fn the_strips_page_shows_live_states_a_popover_and_a_hover_card() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "strips"));
    h.run();
    let app = h.state().app.handle().clone();
    // The page's own items show on this page only.
    assert!(h.query_by_label("No problem").is_some());
    assert!(h.query_by_label("Node: connected").is_some());
    h.get_by_label("Add an error").click();
    h.run();
    h.get_by_label("Add a warning").click();
    h.run();
    h.get_by_label("Add a warning").click();
    h.run();
    assert!(h.query_by_label("Errors: 1").is_some());
    assert!(h.query_by_label("Warnings: 2").is_some());
    // The popover, as an agent or the page's button opens it.
    h.get_by_label("Open the problems popover").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Dismiss all").is_some());
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("strips-popover-pop-dark"));
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.query_by_label("Dismiss all").is_none());
    // The node's card, its link running a command.
    h.get_by_label("Show the node's card").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Reconnect").is_some());
    results.add(h.try_snapshot("strips-card-pop-dark"));
    let node = egui_pop::api::strips::status(&app)
        .into_iter()
        .find(|e| e.id == "gallery.node")
        .unwrap();
    assert_eq!(node.priority, 10);
    assert!(node.card.is_some());
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
    // The status bar starts with where the user is.
    assert!(h.query_by_label("You are in All photos").is_some());
    // Three levels down; the breadcrumb follows.
    h.get_by_label("Open the album Holidays").click();
    h.run();
    // The album's own status item.
    assert!(h.query_by_label("12 photos").is_some());
    h.get_by_label("Open photo 3").click();
    h.run();
    assert!(h.query_by_label("12 photos").is_none());
    assert_eq!(context_key(&h, "place"), "gallery.photo");
    assert_eq!(context_key(&h, "placeDepth"), "2");
    assert!(h.query_by_label("Album Holidays").is_some());
    assert!(h.query_by_label("Photo 3").is_some());
    assert!(h.query_by_label("Go to Album Holidays").is_some());
    assert!(h.query_by_label("You are in Photo 3").is_some());
    // An orientation set: its panel shows, its keys work, its chip and
    // its picks show in the status bar.
    h.get_by_label("Culling").click();
    h.run();
    assert_eq!(context_key(&h, "mode.orientation"), "gallery.culling");
    assert!(app.panel_visible(&RATINGS));
    h.key_press(egui::Key::P);
    h.run();
    assert_eq!(context_key(&h, "galleryContextKey"), "picked");
    assert!(h.query_by_label("Picks (1)").is_some());
    // A compare pushed on top: a chip with a back mark.
    h.get_by_label("Compare").click();
    h.run();
    h.run();
    assert!(app.panel_visible(&SECOND));
    assert!(h.query_by_label("Back from Compare").is_some());
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("contexts-pop-dark"));
    // Its chip goes back, as Escape does: its panel goes, the
    // orientation stays.
    h.get_by_label("Back from Compare").click();
    h.run();
    assert!(!app.panel_visible(&SECOND));
    assert_eq!(context_key(&h, "mode.orientation"), "gallery.culling");
    h.get_by_label("Compare").click();
    h.run();
    assert!(app.panel_visible(&SECOND));
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
fn the_clipboard_page_copies_a_colour_between_palettes_and_previews_paste_special() {
    use egui::accesskit::Role;
    use egui_pop::api::dialogs::{self, DIALOGS, Dialog};
    use egui_pop::model::commands::Origin;
    let mut h = harness(options("pop-dark", Density::Comfortable, "clipboard"));
    h.run();
    let app = h.state().app.handle().clone();
    let value = |h: &Harness<'_, Gallery>, label: &str| {
        h.get_by_label(label)
            .accesskit_node()
            .value()
            .unwrap_or_default()
    };
    // Nothing focused answers: the Edit commands are greyed.
    assert_eq!(app.when_context().get("canCopy"), "false");
    // Ctrl+C on palette A copies its selected colour, a typed item.
    h.get_by_label("Palette A").focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    h.run();
    assert!(
        h.query_by_label_contains("a colour (revision 1)").is_some(),
        "the page shows what the clipboard holds"
    );
    // Paste special's ways into palette B, each with its preview.
    assert!(h.query_by_label("As a colour").is_some());
    assert!(h.query_by_label("As text").is_some());
    assert!(
        h.query_all_by_label("adds #3a7bd5 as colour 2")
            .next()
            .is_some()
    );
    // Ctrl+V on palette B pastes it as a colour.
    h.get_by_label("Palette B").focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::V);
    h.run();
    assert_eq!(value(&h, "Palette B"), "#2e8b57, #3a7bd5");
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("clipboard-pop-dark"));
    // The greys take it only through a lossy transform, which a toast
    // names.
    h.get_by_label("Greys").focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::V);
    h.run();
    assert_eq!(value(&h, "Greys"), "#808080, #747474");
    // (The toast's close button names it too.)
    assert!(
        h.query_all_by_label("Pasted; lost: the hue")
            .next()
            .is_some()
    );
    // Ctrl+Shift+V on palette B: the choice of every way; text chosen.
    h.get_by_label("Palette B").focus();
    h.run();
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::V,
    );
    h.run();
    let d = app.service(&DIALOGS).unwrap();
    let Some((_, Dialog::Choose(c))) = d.current() else {
        panic!("paste special is open");
    };
    let text = c.options.iter().position(|o| o.label == "As text").unwrap();
    assert_eq!(c.options[text].hint, "adds #3a7bd5 as colour 3");
    d.answer(dialogs::Answer::Choose { index: text }, Origin::User)
        .unwrap();
    h.run();
    assert_eq!(value(&h, "Palette B"), "#2e8b57, #3a7bd5, #3a7bd5");
    // A text field keeps egui's own paste: the clipboard is untouched.
    let rev = egui_pop::api::clipboard::with_owner(&app, |c| c.rev()).unwrap();
    h.query_all_by_label("Note")
        .find(|n| n.accesskit_node().role() == Role::TextInput)
        .unwrap()
        .focus();
    h.run();
    h.event(egui::Event::Paste(" pasted".into()));
    h.run();
    assert_eq!(
        egui_pop::api::clipboard::with_owner(&app, |c| c.rev()),
        Some(rev)
    );
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
    // The sort direction is the kit's glyph after the column's title.
    h.snapshot("table-sorted-pop-dark");
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
fn the_indicators_page_in_every_pop_theme_and_still_under_reduced_motion() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in POP_THEMES {
        let mut h = harness(options(theme, Density::Comfortable, "indicators"));
        h.run();
        results.add(h.try_snapshot(format!("indicators-{theme}")));
    }
    let mut h = harness(Options {
        reduced_motion: true,
        ..options("pop-dark", Density::Comfortable, "indicators")
    });
    h.run();
    results.add(h.try_snapshot("indicators-reduced-pop-dark"));
    // The busy LED keeps its state, drawn still.
    let node = egui_pop::core::indicator::find(&h.ctx, "led.busy").unwrap();
    assert!(node.pulsing);
}

#[test]
fn the_indicators_page_names_its_marks_and_answers_like_buttons() {
    use egui::accesskit::{Role, Toggled};
    let mut h = harness(options("pop-dark", Density::Comfortable, "indicators"));
    h.run();
    // A passive badge joins its host's name and adds no node.
    let host = h.get_by_label("Notifications, 12");
    assert_eq!(host.accesskit_node().role(), Role::Button);
    assert!(h.query_by_label("12").is_none());
    // An interactive dot is a button: a click, then the keyboard.
    h.get_by_label("Reset exposure").click();
    h.run();
    assert!(h.query_by_label("Exposure resets: 1").is_some());
    h.get_by_label("Reset exposure").focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.query_by_label("Exposure resets: 2").is_some());
    assert!(
        h.get_by_label("Reset contrast")
            .accesskit_node()
            .is_disabled()
    );
    // A toggle is a check box.
    let mask = h.get_by_label("Mask visible");
    assert_eq!(mask.accesskit_node().role(), Role::CheckBox);
    assert_eq!(mask.accesskit_node().toggled(), Some(Toggled::True));
    mask.click();
    h.run();
    assert_eq!(
        h.get_by_label("Mask visible").accesskit_node().toggled(),
        Some(Toggled::False)
    );
    // Agents read the states through the debug bridge.
    egui_pop_test::assert_indicator(&h, "led.busy", "busy");
    egui_pop_test::assert_indicator(&h, "flag.modified", "modified");
    egui_pop_test::assert_indicator(&h, "mask.visible", "off");
    let snap = h.state().app.handle().debug_snapshot();
    let ids: Vec<&str> = snap["indicators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    for id in [
        "status.node",
        "status.record",
        "status.unread",
        "on.notifications",
    ] {
        assert!(ids.contains(&id), "{id} in {ids:?}");
    }
    let record = egui_pop::core::indicator::find(&h.ctx, "status.record").unwrap();
    assert_eq!(record.tone, egui_pop::core::kit::Tone::Record);
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

fn layout_by_role(theme: &str) {
    let mut h = harness(options(theme, Density::Comfortable, "roles"));
    h.run();
    h.snapshot(format!("roles-{theme}"));
    assert!(h.query_by_label("Nothing clicked yet.").is_some());
    h.get_by_label("danger").click();
    h.run();
    assert!(h.query_by_label("Last clicked: danger").is_some());
}

#[test]
fn the_layout_by_role_page_shows_roles_spacing_and_tones() {
    layout_by_role("pop-dark");
}

#[test]
fn the_layout_by_role_page_in_the_light_theme() {
    layout_by_role("pop-light");
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
fn a_kit_restyles_the_library() {
    // Data only: square cells with a bar under the selected one, square
    // pills and badges with an outline, mark toggles and the search field
    // square.
    let mut themes = egui_pop::tokens::Themes::default();
    themes
        .add_text(
            "kit-library",
            r#"
            $extends: "pop-dark"
            thumb-cell:
              indicator-side: {$type: "string", $value: "bottom"}
              rest:
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
              selected:
                fill: {$type: "color", $value: "{surface.raised}"}
                border-width: {$type: "dimension", $value: {value: 0, unit: "px"}}
                indicator-color: {$type: "color", $value: "{accent.default}"}
                indicator-size: {$type: "dimension", $value: {value: 4, unit: "px"}}
            pill:
              shape: {$type: "string", $value: "box"}
              rest:
                fill: {$type: "color", $value: "{surface.canvas}", $extensions: {org.egui-pop.alpha: 0.6}}
                border-color: {$type: "color", $value: "{border.strong}"}
                border-width: {$type: "dimension", $value: {value: 1, unit: "px"}}
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
            # Based on `pill`: its fill and outline follow; only its own
            # corners are given again.
            thumb-overlay-badge:
              rest:
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
            mark-toggle:
              rest:
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
              selected:
                border-color: {$type: "color", $value: "{accent.default}"}
            search-field:
              rest:
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
                border-color: {$type: "color", $value: "{accent.default}"}
            "#,
        )
        .expect("the test theme reads");
    let mut h = harness(options("pop-dark", Density::Comfortable, "library"));
    h.run();
    let tokens = Tokens::new(themes.resolve(&egui_pop::tokens::ThemeSpec::new("kit-library")));
    assert!(tokens.resolved().problems().is_empty());
    egui_pop::core::theme::install_tokens(&h.ctx, tokens);
    let kit = egui_pop::core::kit::Kit::of(&h.ctx);
    assert_eq!(kit.pill.shape, "box");
    assert!(kit.problems.is_empty(), "{:?}", kit.problems);
    h.query_all_by_label("DSC_00001").next().unwrap().click();
    h.run();
    h.remove_cursor();
    h.run();
    h.snapshot("library-kit");
}

#[test]
fn the_theme_editor_page_edits_a_token_measures_it_and_undoes() {
    use egui_pop::api::theme_editor::{
        CMD_THEME_READ, CMD_THEME_SET, CMD_THEME_UNDO, THEME_EDITOR,
    };
    use egui_pop::model::commands::Origin;
    let agent = || Origin::Agent("test".into());
    let mut results = egui_kittest::SnapshotResults::new();
    for (theme, token) in [("pop-dark", "accent.default"), ("pop-light", "blue.9")] {
        let mut h = harness(options(theme, Density::Comfortable, "theme-editor"));
        h.run();
        let editor = h.state().app.handle().service(&THEME_EDITOR).unwrap();
        editor.select(token);
        h.run();
        egui_pop_test::assert_all_named(&h);
        assert!(h.query_by_label(&format!("Value of {token}")).is_some());
        assert!(h.query_by_label(&format!("Hex of {token}")).is_some());
        // The preview pane draws the gallery's sampler.
        assert!(h.query_by_label("Selected row").is_some());
        if token == "blue.9" {
            assert!(h.query_by_label("Generate blue").is_some());
        }
        results.add(h.try_snapshot(format!("theme-editor-{theme}")));
    }
    // The fonts tab.
    let mut h = harness(options("pop-dark", Density::Comfortable, "theme-editor"));
    h.run();
    h.get_by_label("Fonts").click();
    h.run();
    egui_pop_test::assert_all_named(&h);
    assert!(h.query_by_label("Apply the fonts").is_some());
    results.add(h.try_snapshot("theme-editor-fonts-pop-dark"));

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
fn a_kit_restyles_the_rows_of_a_table() {
    // Data only: rounded rows with a bar on the selected one, no stripes,
    // and headers drawn as a line by the `plain` handler.
    use egui::accesskit::Role;
    let mut themes = egui_pop::tokens::Themes::default();
    themes
        .add_text(
            "kit-rows",
            r#"
            $extends: "pop-dark"
            list-row:
              indicator-side: {$type: "string", $value: "left"}
              rest:
                radius: {$type: "dimension", $value: {value: 4, unit: "px"}}
              selected:
                fill: {$type: "color", $value: "{accent.subtle}"}
                indicator-color: {$type: "color", $value: "{accent.default}"}
                indicator-size: {$type: "dimension", $value: {value: 3, unit: "px"}}
              focus:
                ring-color: {$type: "color", $value: "{border.strong}"}
            table-stripe:
              rest:
                fill: {$type: "color", $value: "{transparent}"}
            table-header-cell:
              shape: {$type: "string", $value: "plain"}
              indicator-side: {$type: "string", $value: "bottom"}
              rest:
                fill: {$type: "color", $value: "{surface.sunken}"}
                text: {$type: "color", $value: "{text.muted}"}
                border-color: {$type: "color", $value: "{accent.default}"}
                border-width: {$type: "dimension", $value: {value: 1, unit: "px"}}
            "#,
        )
        .expect("the test theme reads");
    let mut h = harness(options("pop-dark", Density::Comfortable, "table"));
    h.run();
    let tokens = Tokens::new(themes.resolve(&egui_pop::tokens::ThemeSpec::new("kit-rows")));
    egui_pop::core::theme::install_tokens(&h.ctx, tokens);
    let kit = egui_pop::core::kit::Kit::of(&h.ctx);
    assert_eq!(kit.table_header_cell.shape, "plain");
    assert!(kit.problems.is_empty(), "{:?}", kit.problems);
    h.get_by_role_and_label(Role::Button, "IMG_00002.raw")
        .click();
    h.run();
    h.remove_cursor();
    h.run();
    h.snapshot("table-kit-rows");
}

#[test]
fn a_kit_puts_the_title_at_the_start_of_the_title_bar() {
    // Data only: `title-bar.align` and the bar's colours, as tvty draws it.
    let mut themes = egui_pop::tokens::Themes::default();
    themes
        .add_text(
            "kit-title-start",
            r#"
            $extends: "pop-dark"
            title-bar:
              align: {$type: "string", $value: "start"}
              rest:
                text: {$type: "color", $value: "{text.muted}"}
                border-width: {$type: "dimension", $value: {value: 1, unit: "px"}}
            "#,
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

#[test]
fn a_kit_gives_the_window_buttons_the_macos_flavour() {
    // A kit folder, data only: `traffic-light` circles at the start of
    // the bar, every glyph shown while the pointer is over one of them.
    let kit = egui_pop::tokens::kit!("../../../examples/kits/flavour-macos");
    assert!(egui_pop::tokens::kit::check(&kit).is_empty());
    let mut themes = egui_pop::tokens::Themes::default();
    themes.add_kit(&kit);
    let mut h = harness(options("pop-dark", Density::Comfortable, "frame"));
    h.run();
    let tokens = Tokens::new(themes.resolve(&egui_pop::tokens::ThemeSpec::new("flavour-macos")));
    egui_pop::core::theme::install_tokens(&h.ctx, tokens);
    let kit = egui_pop::core::kit::Kit::of(&h.ctx);
    assert!(kit.problems.is_empty(), "{:?}", kit.problems);
    assert_eq!(kit.window_close.shape, "traffic-light");
    assert_eq!(
        kit.window.buttons_side,
        egui_pop::core::kit::ButtonsSide::Start
    );
    h.run();
    h.get_by_label("Minimize").hover();
    h.run();
    h.run();
    h.snapshot("frame-kit-macos");
}

#[test]
fn a_kit_restyles_a_dialogs_frame_from_the_theme_alone() {
    // Data only: the dialog's box (square, an accent border, more room)
    // and a darker backdrop; the `card` handler egui-pop ships.
    let mut themes = egui_pop::tokens::Themes::default();
    themes
        .add_text(
            "kit-dialog-square",
            r#"
            $extends: "pop-dark"
            dialog:
              padding-x: {$type: "dimension", $value: {value: 24, unit: "px"}}
              padding-y: {$type: "dimension", $value: {value: 20, unit: "px"}}
              rest:
                fill: {$type: "color", $value: "{surface.panel}"}
                border-color: {$type: "color", $value: "{accent.default}"}
                border-width: {$type: "dimension", $value: {value: 2, unit: "px"}}
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
            backdrop:
              opacity: {$type: "number", $value: 0.7}
            "#,
        )
        .expect("the test theme reads");
    let mut h = harness(options("pop-dark", Density::Comfortable, "dialogs"));
    h.run();
    let tokens =
        Tokens::new(themes.resolve(&egui_pop::tokens::ThemeSpec::new("kit-dialog-square")));
    egui_pop::core::theme::install_tokens(&h.ctx, tokens);
    let kit = egui_pop::core::kit::Kit::of(&h.ctx);
    assert!(kit.problems.is_empty(), "{:?}", kit.problems);
    assert_eq!(
        kit.dialog.style(egui_pop::core::kit::State::REST).radius,
        0.0
    );
    h.get_by_label("Export settings").click();
    h.run();
    h.run();
    h.snapshot("dialogs-kit-square");
}

#[test]
fn a_kit_restyles_the_palettes_bands_from_the_theme_alone() {
    // Data only: an accent band with a thicker line, filled while the
    // pointer is over it, and square, outlined icon buttons on it.
    let mut themes = egui_pop::tokens::Themes::default();
    themes
        .add_text(
            "kit-band-accent",
            r#"
            $extends: "pop-dark"
            header-band:
              rest:
                fill: {$type: "color", $value: "{accent.subtle}"}
                border-color: {$type: "color", $value: "{accent.default}"}
                border-width: {$type: "dimension", $value: {value: 2, unit: "px"}}
                text: {$type: "color", $value: "{text.strong}"}
              hover:
                fill: {$type: "color", $value: "{accent.default}"}
                text: {$type: "color", $value: "{text.on-accent}"}
            band-icon-button:
              rest:
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
                border-color: {$type: "color", $value: "{border.strong}"}
                border-width: {$type: "dimension", $value: {value: 1, unit: "px"}}
            "#,
        )
        .expect("the test theme reads");
    let mut h = harness(options("pop-dark", Density::Comfortable, "palettes"));
    h.run();
    let tokens = Tokens::new(themes.resolve(&egui_pop::tokens::ThemeSpec::new("kit-band-accent")));
    egui_pop::core::theme::install_tokens(&h.ctx, tokens);
    let kit = egui_pop::core::kit::Kit::of(&h.ctx);
    assert!(kit.problems.is_empty(), "{:?}", kit.problems);
    assert_eq!(kit.header_band.shape, "band");
    h.get_by_label("Open the palettes").click();
    h.run();
    h.run();
    // The pointer over the toolbox's title: its band is hovered.
    h.query_all_by_label("Toolbox").last().unwrap().hover();
    h.run();
    h.run();
    h.snapshot("palettes-kit-band");
}

#[test]
fn a_kit_restyles_menu_items_from_the_theme_alone() {
    // Data only: square items, an accent bar on the side of the item
    // under the pointer and of an open submenu.
    let mut themes = egui_pop::tokens::Themes::default();
    themes
        .add_text(
            "kit-menu-bar",
            r#"
            $extends: "pop-dark"
            menu-item:
              indicator-side: {$type: "string", $value: "left"}
              rest:
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
                indicator-color: {$type: "color", $value: "{accent.default}"}
              hover:
                fill: {$type: "color", $value: "{accent.subtle}"}
                indicator-size: {$type: "dimension", $value: {value: 3, unit: "px"}}
              selected:
                fill: {$type: "color", $value: "{accent.subtle}"}
                indicator-size: {$type: "dimension", $value: {value: 3, unit: "px"}}
            "#,
        )
        .expect("the test theme reads");
    let mut h = harness(options("pop-dark", Density::Comfortable, "menus"));
    h.run();
    let tokens = Tokens::new(themes.resolve(&egui_pop::tokens::ThemeSpec::new("kit-menu-bar")));
    egui_pop::core::theme::install_tokens(&h.ctx, tokens);
    assert!(egui_pop::core::kit::Kit::of(&h.ctx).problems.is_empty());
    h.run();
    h.get_by_label("Card with a context menu").click_secondary();
    h.run();
    h.query_all_by_label("Zoom").last().unwrap().hover();
    h.run();
    h.snapshot("menus-kit-bar");
}

#[test]
fn a_kit_restyles_sliders_from_the_theme_alone() {
    // Data only: flat outlined bars, the fill the bar's whole height, a
    // thicker accent thumb, a ring outside the focused bar, its value
    // past the soft range in the danger colour, square toggles.
    let mut themes = egui_pop::tokens::Themes::default();
    themes
        .add_text(
            "kit-sliders-flat",
            r#"
            $extends: "pop-dark"
            slider:
              rest:
                fill: {$type: "color", $value: "{surface.sunken}"}
                border-color: {$type: "color", $value: "{border.default}"}
                border-width: {$type: "dimension", $value: {value: 1, unit: "px"}}
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
              focus:
                border-width: {$type: "dimension", $value: {value: 1, unit: "px"}}
                ring-color: {$type: "color", $value: "{accent.default}"}
                ring-width: {$type: "dimension", $value: {value: 1, unit: "px"}}
                ring-offset: {$type: "dimension", $value: {value: 1, unit: "px"}}
            slider-fill:
              height: {$type: "dimension", $value: {value: 100, unit: "px"}}
              rest:
                fill: {$type: "color", $value: "{accent.subtle}"}
            slider-thumb:
              width: {$type: "dimension", $value: {value: 3, unit: "px"}}
              rest:
                fill: {$type: "color", $value: "{accent.default}"}
            slider-value:
              tone:
                warning:
                  text: {$type: "color", $value: "{danger}"}
            slider-toggle:
              rest:
                fill: {$type: "color", $value: "{surface.sunken}"}
                border-color: {$type: "color", $value: "{border.default}"}
                border-width: {$type: "dimension", $value: {value: 1, unit: "px"}}
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
              selected:
                fill: {$type: "color", $value: "{accent.subtle}"}
            "#,
        )
        .expect("the test theme reads");
    let mut h = harness(options("pop-dark", Density::Comfortable, "sliders"));
    h.run();
    let tokens = Tokens::new(themes.resolve(&egui_pop::tokens::ThemeSpec::new("kit-sliders-flat")));
    egui_pop::core::theme::install_tokens(&h.ctx, tokens);
    assert!(egui_pop::core::kit::Kit::of(&h.ctx).problems.is_empty());
    h.run();
    // Exposure typed past its soft range (-3 to 4 EV), kept focused.
    h.get_by_label("Exposure").click();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    for _ in 0..8 {
        h.key_press(egui::Key::Backspace);
    }
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("6");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(
        h.get_by_label("Exposure").accesskit_node().numeric_value(),
        Some(6.0)
    );
    h.remove_cursor();
    h.run();
    h.snapshot("sliders-kit-flat");
}

#[test]
fn a_kit_restyles_egui_s_small_widgets_from_the_theme_alone() {
    // Data only: square checkboxes filled with the accent when checked,
    // diamond radio marks, rows with a bar at their start, outlined
    // chips, a chevron as the combo box's arrow. egui's own widgets on
    // the same page keep following `button` only.
    let mut themes = egui_pop::tokens::Themes::default();
    themes
        .add_text(
            "kit-toggles",
            r#"
            $extends: "pop-dark"
            glyph:
              radio: {$type: "string", $value: "diamond"}
              dropdown: {$type: "string", $value: "chevron-down"}
            checkbox:
              rest:
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
                fill: {$type: "color", $value: "{surface.sunken}"}
              selected:
                fill: {$type: "color", $value: "{accent.default}"}
                border-color: {$type: "color", $value: "{accent.default}"}
                text: {$type: "color", $value: "{text.on-accent}"}
            radio:
              selected:
                fill: {$type: "color", $value: "{surface.sunken}"}
                border-color: {$type: "color", $value: "{accent.default}"}
                text: {$type: "color", $value: "{accent.default}"}
            selectable-row:
              indicator-side: {$type: "string", $value: "left"}
              selected:
                fill: {$type: "color", $value: "{accent.subtle}"}
                border-width: {$type: "dimension", $value: {value: 0, unit: "px"}}
                indicator-color: {$type: "color", $value: "{accent.default}"}
                indicator-size: {$type: "dimension", $value: {value: 3, unit: "px"}}
            toggle-chip:
              rest:
                border-width: {$type: "dimension", $value: {value: 1, unit: "px"}}
                border-color: {$type: "color", $value: "{border.default}"}
              selected:
                fill: {$type: "color", $value: "{accent.subtle}"}
                border-color: {$type: "color", $value: "{accent.default}"}
                indicator-size: {$type: "dimension", $value: {value: 0, unit: "px"}}
            combo-arrow:
              rest:
                text: {$type: "color", $value: "{accent.default}"}
            "#,
        )
        .expect("the test theme reads");
    let mut h = harness(options("pop-dark", Density::Comfortable, "builtins"));
    h.run();
    let tokens = Tokens::new(themes.resolve(&egui_pop::tokens::ThemeSpec::new("kit-toggles")));
    egui_pop::core::theme::install_tokens(&h.ctx, tokens);
    let kit = egui_pop::core::kit::Kit::of(&h.ctx);
    assert!(kit.problems.is_empty(), "{:?}", kit.problems);
    h.run();
    // The toggle chip is a named checkbox, on.
    let case = h.get_by_label("Match case");
    assert_eq!(
        case.accesskit_node().role(),
        egui::accesskit::Role::CheckBox
    );
    h.remove_cursor();
    h.run();
    h.snapshot("builtins-kit-toggles");
}

#[test]
fn egui_s_widgets_in_a_scope_follow_their_kit_component() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "scoped"));
    h.run();
    h.snapshot("scoped-pop-dark");
    h.get_by_label("danger").click();
    h.run();
    assert!(h.query_by_label("Last clicked: danger").is_some());
    h.get_by_label("200 %").click();
    h.run();
}

#[test]
fn a_kit_restyles_egui_s_own_widgets_in_a_scope_from_the_theme_alone() {
    // Data only: square accent buttons, a rounded accent combo box, a
    // filled toggle, flat fields with a thick accent outline while
    // focused, a thin rounded scroll bar shown at rest without a track
    // (docs/kit.md, tvty's). egui draws every one of them.
    let mut themes = egui_pop::tokens::Themes::default();
    themes
        .add_text(
            "kit-scoped",
            r#"
            $extends: "pop-dark"
            button:
              rest:
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
            combo-box:
              rest:
                fill: {$type: "color", $value: "{accent.subtle}"}
                border-color: {$type: "color", $value: "{accent.default}"}
                radius: {$type: "dimension", $value: {value: 12, unit: "px"}}
              hover:
                border-color: {$type: "color", $value: "{accent.hover}"}
            toggle-button:
              selected:
                fill: {$type: "color", $value: "{accent.default}"}
                text: {$type: "color", $value: "{text.on-accent}"}
            field:
              rest:
                fill: {$type: "color", $value: "{surface.panel}"}
                border-color: {$type: "color", $value: "{border.strong}"}
                radius: {$type: "dimension", $value: {value: 0, unit: "px"}}
              focus:
                border-color: {$type: "color", $value: "{accent.default}"}
                border-width: {$type: "dimension", $value: {value: 2, unit: "px"}}
            scroll-bar:
              width: {$type: "dimension", $value: {value: 6, unit: "px"}}
              margin: {$type: "dimension", $value: {value: 4, unit: "px"}}
              visibility: {$type: "string", $value: "always"}
              track-fill: {$type: "color", $value: "{transparent}"}
              rest:
                fill: {$type: "color", $value: "{border.strong}"}
                radius: {$type: "dimension", $value: {value: 999, unit: "px"}}
              hover:
                fill: {$type: "color", $value: "{text.muted}"}
              active:
                fill: {$type: "color", $value: "{text.default}"}
            "#,
        )
        .expect("the test theme reads");
    let mut h = harness(options("pop-dark", Density::Comfortable, "scoped"));
    h.run();
    let tokens = Tokens::new(themes.resolve(&egui_pop::tokens::ThemeSpec::new("kit-scoped")));
    egui_pop::core::theme::install_tokens(&h.ctx, tokens);
    let kit = egui_pop::core::kit::Kit::of(&h.ctx);
    // Only the text fields and the scroll bars outside a scope, which
    // follow `button`.
    assert_eq!(kit.problems.len(), 2, "{:?}", kit.problems);
    assert!(kit.problems[0].starts_with("field.rest:"));
    assert!(kit.problems[1].starts_with("scroll-bar:"));
    h.run();
    // The plain field with the keyboard: its focus outline.
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "Plain")
        .click();
    h.run();
    h.remove_cursor();
    h.run();
    h.snapshot("scoped-kit-restyled");
}

#[test]
fn a_folded_side_area_shows_its_strip_and_a_click_unfolds_it() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "workbench"));
    h.run();
    h.get_by_label("Open the demonstration panels").click();
    h.run();
    let app = h.state().app.handle().clone();
    app.fold_area(egui_pop::model::panels::Area::StartSide, true);
    h.run();
    h.run();
    assert!(
        app.layout()
            .folded
            .contains(&egui_pop::model::panels::Area::StartSide)
    );
    h.snapshot("workbench-folded-pop-dark");
    h.get_by_label("Unfold the start side bar").click();
    h.run();
    h.run();
    assert!(app.layout().folded.is_empty());
}

#[test]
fn a_folded_strip_opens_a_flyout_and_shows_the_outlines_new_headings() {
    use egui_pop::api::workbench;
    use egui_pop::model::panels::Area;
    let mut h = harness(options("pop-dark", Density::Comfortable, "workbench"));
    h.run();
    h.get_by_label("Open the demonstration panels").click();
    h.run();
    h.get_by_label("Add a heading to the outline").click();
    h.run();
    h.get_by_label("Add a heading to the outline").click();
    h.run();
    // The page's strip controls sit above its long tables: in view.
    let app = h.state().app.handle().clone();
    let button = h.get_by_label("Start side: fold").rect();
    let centre = app.area_rect(Area::Center).unwrap();
    assert!(
        centre.contains_rect(button),
        "the button {button:?} is out of the centre {centre:?}"
    );
    h.get_by_label("Start side: fold").click();
    h.run();
    h.run();
    let steps: Vec<String> = app
        .step_traces()
        .into_iter()
        .filter(|t| t.step.as_str().starts_with("workbench."))
        .map(|t| format!("{} {} {:?} {:?}", t.step, t.args, t.vetoed_by, t.result))
        .collect();
    assert!(
        app.layout().folded.contains(&Area::StartSide),
        "not folded; the workbench's steps: {steps:#?}"
    );
    // The outline's icon carries its two new headings.
    let outline = workbench::strip(&app)
        .into_iter()
        .find(|i| i.panel.as_str() == "gallery.outline")
        .expect("the outline has an icon in the strip");
    assert_eq!(outline.icon.as_deref(), Some("file"));
    assert!(outline.marked, "{outline:?}");
    assert_eq!(outline.indicator.and_then(|s| s.count), Some(2));
    // Its flyout over the centre; the layout does not move.
    let centre = app.area_rect(Area::Center).unwrap();
    let id = egui_pop::model::ids::PanelId::new("gallery.outline").unwrap();
    assert!(app.open_flyout(&id, true).unwrap().ok());
    h.run();
    h.run();
    assert!(h.query_by_label("Sections of the document.").is_some());
    assert!(h.query_by_label("Pin Outline").is_some());
    assert_eq!(app.area_rect(Area::Center).unwrap(), centre);
    egui_pop_test::assert_all_named(&h);
    h.key_press(egui::Key::Escape);
    h.run();
    h.run();
    assert!(app.flyout().is_none());
    assert!(h.query_by_label("Sections of the document.").is_none());
}

#[test]
fn a_side_area_lies_over_the_centre_with_its_shadow() {
    use egui_pop::model::layout::Placement;
    use egui_pop::model::panels::Area;
    let mut h = harness(options("pop-dark", Density::Comfortable, "workbench"));
    h.run();
    h.get_by_label("Open the demonstration panels").click();
    h.run();
    let app = h.state().app.handle().clone();
    let centre = app.area_rect(Area::Center).unwrap();
    assert!(
        app.place_area(Area::StartSide, Placement::Over)
            .unwrap()
            .ok()
    );
    for _ in 0..3 {
        h.run();
    }
    // The centre keeps the room the side bar gave up; the area lies over it.
    let wide = app.area_rect(Area::Center).unwrap();
    assert!(wide.left() < centre.left() - 100.0, "{wide:?} {centre:?}");
    assert!(app.area_rect(Area::StartSide).unwrap().intersects(wide));
    assert!(h.query_by_label("Fold the start side bar").is_some());
    h.snapshot("workbench-over-pop-dark");
}

#[test]
fn the_preview_page_opens_an_image_viewport_in_a_custom_dialog() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "preview"));
    run_page(&mut h, "preview");
    h.get_by_label("Preview the export").click();
    for _ in 0..10 {
        let _ = h.run_ok();
    }
    assert!(egui_pop::core::overlay::modal_open(&h.ctx));
    assert!(h.query_by_label("Export preview").is_some());
    assert!(h.query_by_label("Image behind the preview").is_some());
    egui_pop_test::assert_all_named(&h);
    // The body is mounted in the custom dialog, sRGB first.
    let mounts = h.state().app.handle().mounts();
    let m = mounts
        .iter()
        .find(|m| m.component == "viewport")
        .expect("the preview's viewport is mounted");
    assert_eq!(m.host, "dialog.custom");
    assert_eq!(m.fit.cell(), "if c3 c9");
    h.snapshot("preview-dialog-pop-dark");
    // Enter exports, with the view the preview was left at.
    h.key_press(egui::Key::Enter);
    let _ = h.run_ok();
    let _ = h.run_ok();
    assert!(!egui_pop::core::overlay::modal_open(&h.ctx));
    let sent = context_key(&h, "galleryPreviewSent");
    assert!(sent.contains(r#""format":"png""#), "{sent}");
    assert!(sent.contains(r#""view""#), "{sent}");
}

/// Every slot holding one value per instance (a widget, a window), with
/// how many it holds, from the debug bridge's `state` collector.
fn instance_entries(h: &Harness<'_, Gallery>) -> std::collections::BTreeMap<String, u64> {
    let snap = h.state().app.handle().debug_snapshot();
    snap["state"]["slots"]
        .as_array()
        .expect("the state collector lists slots")
        .iter()
        .filter(|s| s["scope"] == "instance")
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["entries"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn the_state_kept_per_instance_stays_bounded_across_tours_of_the_pages() {
    let mut h = harness(options("pop-dark", Density::Comfortable, PAGES[0]));
    // A tour: every page, a few frames each, then the next one.
    let tour = |h: &mut Harness<'_, Gallery>| {
        for _ in 0..PAGES.len() {
            for _ in 0..3 {
                h.step();
            }
            let f = h.state_mut().app.debug_call(
                "commands",
                "run",
                &serde_json::json!({ "id": "gallery.page.next" }),
            );
            assert!(f.ok, "{f:?}");
        }
    };
    tour(&mut h);
    let first = instance_entries(&h);
    assert!(!first.is_empty(), "the pages keep state per widget");
    // Two more tours, about 260 frames: the same pages keep no more.
    for _ in 0..2 {
        tour(&mut h);
        let now = instance_entries(&h);
        let grown: Vec<String> = now
            .iter()
            .filter(|(name, n)| **n > first.get(*name).copied().unwrap_or(0))
            .map(|(name, n)| format!("{name}: {} -> {n}", first.get(name).copied().unwrap_or(0)))
            .collect();
        assert!(grown.is_empty(), "state grows: {grown:?}");
    }
}

#[test]
fn the_pointer_page_switches_profile_and_lists_its_gestures() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "pointer"));
    h.run();
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("pointer-pop-dark"));
    let app = h.state().app.handle().clone();
    assert_eq!(egui_pop::api::pointer::profile_id(&app), "document");
    h.get_by_label("darktable").click();
    h.run();
    h.run();
    assert_eq!(egui_pop::api::pointer::profile_id(&app), "darktable");
    let map = egui_pop::core::pointer::map(&h.ctx);
    assert_eq!(map.profile, "darktable");
    assert!(h.query_by_label("MiddleClick").is_some());
}

#[test]
fn the_shortcuts_editor_adds_a_gesture_done_on_its_pad() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "settings"));
    h.run();
    h.get_by_label("Open the settings window (Ctrl+,)").click();
    h.run();
    h.get_by_label("Keyboard shortcuts").click();
    h.run();
    assert!(h.query_by_label("Pointer profile: document").is_none());
    assert!(h.query_by_label("Pointer profile: Document").is_some());
    h.get_by_label("Pointer gestures only").click();
    h.run();
    assert!(h.query_by_label("Gallery: Next page").is_none());
    let command = "Pointer: Image: zoom at the pointer";
    h.get_by_label(&format!("Add a gesture to {command}"))
        .click();
    h.run();
    let pad = h.get_by_label("Gesture pad").rect().center();
    h.hover_at(pad);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        modifiers: egui::Modifiers::ALT,
        phase: egui::TouchPhase::Move,
    });
    h.run();
    h.get_by_label("Use Alt+Wheel").click();
    h.run();
    h.run();
    let app = h.state().app.handle().clone();
    let zoom = egui_pop::model::pointer::gesture::IMAGE_ZOOM;
    let keys = app.keys_for(&zoom);
    assert!(keys.contains(&"Alt+Wheel".to_string()), "{keys:?}");
    assert!(keys.contains(&"Ctrl+Wheel".to_string()), "{keys:?}");
    assert!(
        h.query_by_label("Pointer profile: Document (1 changed)")
            .is_some()
    );
    // The fourth key wraps the row, below the window's fold: scrolled
    // into view before the click.
    let remove = format!("Remove Alt+Wheel from {command}");
    h.get_by_label(&remove).scroll_to_me();
    h.run();
    h.get_by_label(&remove).click();
    h.run();
    h.run();
    assert!(!app.keys_for(&zoom).contains(&"Alt+Wheel".to_string()));
}

#[test]
fn the_font_pairs_page_compares_each_pair_and_measures_their_digits() {
    use egui_pop::core::fonts::{self, Weight};
    use egui_pop_gallery::pages::fonts::{CANDIDATES, all_equal, digit_advances};
    let mut h = harness(options("pop-dark", Density::Comfortable, "fonts"));
    h.run();
    for c in &CANDIDATES {
        assert!(h.query_by_label(c.title).is_some(), "{}", c.title);
        // Within the budget for the pair bundled.
        assert!(c.bytes() < 1_000_000, "{}: {} bytes", c.title, c.bytes());
    }
    let report = fonts::report(&h.ctx).unwrap();
    assert!(
        !report.problems.iter().any(|p| p.starts_with("specimen")),
        "{:?}",
        report.problems
    );
    let family = |name: &str| fonts::specimen(&h.ctx, name, Weight::REGULAR).unwrap();
    for c in &CANDIDATES {
        // Every monospace face: one advance for digits, `i` and `W`.
        let mono = family(c.mono);
        assert!(
            all_equal(&digit_advances(&h.ctx, mono.clone(), 13.0)),
            "{}",
            c.mono
        );
        let id = egui::FontId::new(13.0, mono);
        let (i, w) = h
            .ctx
            .fonts_mut(|f| (f.glyph_width(&id, 'i'), f.glyph_width(&id, 'W')));
        assert!((i - w).abs() < 0.01, "{}: i {i}, W {w}", c.mono);
    }
    // What the study says of the proportional faces: Noto Sans's default
    // digits are tabular, Inter's are not.
    assert!(all_equal(&digit_advances(
        &h.ctx,
        family("Noto Sans"),
        13.0
    )));
    assert!(!all_equal(&digit_advances(&h.ctx, family("Inter"), 13.0)));
    h.snapshot("fonts-pop-dark");
}

/// Zen mode entered from the View menu: its way out is in sight, and
/// Escape leaves it.
#[test]
fn zen_mode_from_the_view_menu_shows_its_way_out_and_escape_leaves_it() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "overview"));
    h.run();
    h.get_by_label("View").click();
    h.run();
    h.query_all_by_label("Zen mode").last().unwrap().click();
    h.run();
    let handle = h.state().app.handle().clone();
    assert!(handle.zen());
    assert!(h.query_by_label("View").is_none(), "the menu bar is hidden");
    // The strip shows it (the live region keeps it too, as a `Status`).
    assert!(zen_hint_on_screen(&h));
    assert!(h.query_by_label("Leave zen mode").is_some());
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!handle.zen());
    assert!(h.query_by_label("View").is_some());
    assert!(!zen_hint_on_screen(&h));
}

/// Whether zen mode's strip shows its hint: the polite live region keeps
/// the last announcement as its label, a `Status` node left out here.
fn zen_hint_on_screen(h: &Harness<'_, Gallery>) -> bool {
    h.query_all_by_label("Zen mode: Escape to leave")
        .any(|n| n.accesskit_node().role() != egui::accesskit::Role::Status)
}

/// In a text field, the first Escape leaves the field, the second zen
/// mode; Shift+F11 toggles it even while typing.
#[test]
fn escape_leaves_a_text_field_then_zen_mode() {
    let mut h = harness(options("pop-dark", Density::Comfortable, "inputs"));
    h.run();
    text_input(&h, "Search").focus();
    h.run();
    assert!(text_input(&h, "Search").is_focused());
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::F11);
    h.run();
    let handle = h.state().app.handle().clone();
    assert!(handle.zen(), "Shift+F11 while typing");
    assert!(text_input(&h, "Search").is_focused(), "the field kept it");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(handle.zen(), "the first Escape leaves the field");
    assert!(!text_input(&h, "Search").is_focused());
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!handle.zen(), "the second leaves zen mode");
    // Shift+F11 toggles it on and off.
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::F11);
    h.run();
    assert!(handle.zen());
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::F11);
    h.run();
    assert!(!handle.zen());
}
