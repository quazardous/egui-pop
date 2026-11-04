//! Devtools' inspect mode in the gallery: a click picks an element
//! without reaching it, the dialog names it, Copy puts it on the
//! clipboard; in a detached panel too; by the keyboard; Escape leaves.

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::NodeT as _;
use egui_kittest::kittest::Queryable as _;
use egui_pop::api::AppHandle;
use egui_pop::api::devtools::inspect;
use egui_pop::model::clipboard::Slot;
use egui_pop::theme::Density;
use egui_pop_gallery::{Gallery, Options};
use egui_pop_test::SharedGpu as _;

fn harness(page: &str) -> Harness<'static, Gallery> {
    let options = Options {
        theme: "pop-dark".to_string(),
        density: Density::Comfortable,
        page: page.to_string(),
        ..Options::default()
    };
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .shared_wgpu()
        .build_eframe(move |cc| {
            egui_pop::core::motion::freeze(&cc.egui_ctx, 0.0);
            Gallery::from_creation_context(cc, options).expect("the gallery builds")
        });
    h.run();
    // On from the start in the gallery.
    assert!(egui_pop::api::devtools::enabled(&handle(&h)));
    h
}

fn handle(h: &Harness<'_, Gallery>) -> AppHandle {
    h.state().app.handle().clone()
}

/// Ctrl+Shift+F12 (Ctrl+Shift+C is the browsers' own picker).
fn chord(h: &mut Harness<'_, Gallery>) {
    h.key_press_modifiers(
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        egui::Key::F12,
    );
    h.run();
    h.run();
}

fn copied(app: &AppHandle) -> String {
    egui_pop::api::clipboard::with_owner(app, |c| c.get(&Slot::User))
        .flatten()
        .expect("copied into the user's slot")
        .fallbacks
        .iter()
        .find(|r| r.format == egui_pop::model::clipboard::TEXT)
        .and_then(|r| r.data.as_str().map(String::from))
        .expect("a text fallback")
}

fn line<'a>(text: &'a str, name: &str) -> &'a str {
    text.lines()
        .find_map(|l| l.strip_prefix(&format!("{name}: ")))
        .unwrap_or_else(|| panic!("no `{name}` line in\n{text}"))
}

#[test]
fn a_click_names_the_button_without_pressing_it_and_copy_holds_it() {
    let mut h = harness("debug");
    let app = handle(&h);
    chord(&mut h);
    assert!(inspect::active(&app));
    h.get_by_label("Announce \"Saved\"").click();
    h.run();
    h.run();
    // Not pressed: nothing announced.
    assert!(egui_pop::core::a11y::announcements(&h.ctx).is_empty());
    assert!(!inspect::active(&app));
    h.get_by_label("Inspected element");
    h.get_by_label("Button \"Announce \\\"Saved\\\"\" (widget)");
    let mut results = egui_kittest::SnapshotResults::new();
    results.add(h.try_snapshot("inspect-dialog-pop-dark"));
    h.get_by_role_and_label(Role::Button, "Copy").click();
    h.run();
    h.get_by_label("Copied.");
    let text = copied(&app);
    assert!(
        text.starts_with("Element: Button \"Announce \\\"Saved\\\"\" (widget)\n"),
        "{text}"
    );
    assert_eq!(line(&text, "Role"), "Button");
    assert_eq!(line(&text, "Name"), "Announce \"Saved\"");
    assert_eq!(line(&text, "Window"), "main");
    assert_eq!(line(&text, "Zone"), "app.inspector");
    assert_eq!(line(&text, "Block"), "gallery.playground");
    assert_eq!(line(&text, "Panel"), "app.inspector");
    assert_eq!(line(&text, "Story plugin"), "gallery.debug");
    assert_eq!(
        line(&text, "Theme"),
        "pop-dark (dark), kit built-in, density comfortable"
    );
    assert_eq!(line(&text, "Enabled"), "yes");
    assert_eq!(line(&text, "Page"), "Debug bridge (Framework)");
    assert_eq!(line(&text, "Story"), "debug");
    assert_eq!(line(&text, "In"), "its playground");
    assert_eq!(line(&text, "Picked by"), "pointer");
    // The JSON form beside it, and the collector.
    let snapshot = app.debug_snapshot();
    let last = &snapshot["inspect"]["last"];
    assert_eq!(last["text"], text.as_str());
    assert_eq!(last["json"]["element"]["role"], "Button");
    assert_eq!(last["json"]["app"]["Story"], "debug");
    // Report…: a new issue on the gallery's repository, its form filled
    // with the page and this pick.
    let rec = egui_pop::api::links::RecordingOpener::default();
    egui_pop::api::links::links(&app)
        .unwrap()
        .set_opener(std::sync::Arc::new(rec.clone()));
    h.get_by_role_and_label(Role::Button, "Report…").click();
    h.run();
    h.run();
    let records = rec.records();
    assert_eq!(records.len(), 1, "{records:?}");
    let url = &records[0].target;
    assert!(
        url.starts_with(
            "https://github.com/quazardous/egui-pop/issues/new?template=bug.yml&title=Problem%20in%20Debug%20bridge%20%28story%20debug%29&version=egui-pop%20gallery%20"
        ),
        "{url}"
    );
    assert!(
        url.contains("&page=Debug%20bridge%20%28story%20debug%29"),
        "{url}"
    );
    assert!(
        url.contains(
            "&pick=Element%3A%20Button%20%22Announce%20%5C%22Saved%5C%22%22%20%28widget%29%0A"
        ),
        "{url}"
    );
    // Pick another: the dialog goes, the mode is back.
    h.get_by_role_and_label(Role::Button, "Pick another")
        .click();
    h.run();
    h.run();
    assert!(inspect::active(&app));
    assert!(h.query_by_label("Inspected element").is_none());
}

#[test]
fn a_detached_panel_is_inspected_in_its_window() {
    let mut h = harness("workbench");
    let app = handle(&h);
    h.get_by_label("Open the demonstration panels").click();
    h.run();
    h.get_by_label("Detach Notes into a window").click();
    h.run();
    h.run();
    assert_eq!(app.layout().auxiliary.len(), 1);
    chord(&mut h);
    assert!(inspect::active(&app));
    h.get_by_label("Clear the notes…").click();
    h.run();
    h.run();
    // Not pressed: no question about clearing.
    assert!(h.query_by_label("Clear the notes?").is_none());
    h.get_by_label("Inspected element");
    h.get_by_role_and_label(Role::Button, "Copy").click();
    h.run();
    let text = copied(&app);
    assert_eq!(line(&text, "Name"), "Clear the notes…");
    assert_eq!(line(&text, "Panel"), "gallery.notes");
    assert_eq!(line(&text, "Window"), "panel gallery.notes");
    assert_eq!(line(&text, "Plugin"), "gallery.workbench");
}

#[test]
fn the_keyboard_moves_between_widgets_and_enter_picks() {
    let mut h = harness("debug");
    let app = handle(&h);
    chord(&mut h);
    h.key_press(egui::Key::Tab);
    h.run();
    h.key_press(egui::Key::Tab);
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    assert!(!inspect::active(&app));
    let second = inspect::last(&app).expect("picked");
    assert_eq!(second.element.how, "keyboard");
    // One Tab less is another widget.
    h.get_by_role_and_label(Role::Button, "Pick another")
        .click();
    h.run();
    h.run();
    h.key_press(egui::Key::Tab);
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    let first = inspect::last(&app).expect("picked");
    assert_ne!(first.element.id, second.element.id);
}

#[test]
fn escape_leaves_and_a_click_then_reaches_the_button() {
    let mut h = harness("debug");
    let app = handle(&h);
    chord(&mut h);
    assert!(inspect::active(&app));
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!inspect::active(&app));
    h.get_by_label("Announce \"Saved\"").click();
    h.run();
    assert!(h.query_by_label("Inspected element").is_none());
    assert_eq!(egui_pop::core::a11y::announcements(&h.ctx).len(), 1);
    // A second press of the chord leaves too.
    chord(&mut h);
    assert!(inspect::active(&app));
    chord(&mut h);
    assert!(!inspect::active(&app));
}

#[test]
fn the_menu_bar_picker_toggles_the_mode_and_the_pointer_is_a_crosshair() {
    let mut h = harness("debug");
    let app = handle(&h);
    // At the menu bar's end, its key in the tooltip, not pressed.
    let picker = h.get_by_role_and_label(Role::Button, "Inspect an element");
    assert_eq!(picker.accesskit_node().toggled(), Some(Toggled::False));
    assert_eq!(
        app.keys_for(&inspect::CMD_INSPECT)
            .first()
            .map(String::as_str),
        Some("Ctrl+Shift+F12")
    );
    // The debug page's own button says the same: the tooltip is a second.
    let tip = "Inspect an element (Ctrl+Shift+F12)";
    assert_eq!(h.query_all_by_label(tip).count(), 1);
    picker.hover();
    h.run();
    assert_eq!(h.query_all_by_label(tip).count(), 2);
    h.get_by_role_and_label(Role::Button, "Inspect an element")
        .click();
    h.run();
    h.run();
    assert!(inspect::active(&app));
    assert_eq!(app.context().get(inspect::CONTEXT_INSPECTING), "true");
    let picker = h.get_by_role_and_label(Role::Button, "Inspect an element");
    assert_eq!(picker.accesskit_node().toggled(), Some(Toggled::True));
    // Over the page: a crosshair.
    h.get_by_label("Announce \"Saved\"").hover();
    h.run();
    assert_eq!(
        h.output().platform_output.cursor_icon,
        egui::CursorIcon::Crosshair
    );
    // Pressed again, it leaves the mode instead of being picked.
    h.get_by_role_and_label(Role::Button, "Inspect an element")
        .click();
    h.run();
    h.run();
    assert!(!inspect::active(&app));
    assert!(h.query_by_label("Inspected element").is_none());
    assert_eq!(app.context().get(inspect::CONTEXT_INSPECTING), "false");
    h.get_by_label("Announce \"Saved\"").hover();
    h.run();
    assert_ne!(
        h.output().platform_output.cursor_icon,
        egui::CursorIcon::Crosshair
    );
    // Off with the devtools setting.
    {
        use egui_pop::model::commands::Origin;
        use egui_pop::model::values::{Change, Set, Value};
        egui_pop::core::lock(&app.store())
            .set(Set::new(
                &format!("settings/{}", egui_pop::api::devtools::SETTING_DEVTOOLS),
                Change::To(Value::Bool(false)),
                Origin::User,
            ))
            .unwrap();
    }
    h.run();
    h.run();
    assert!(
        h.query_by_role_and_label(Role::Button, "Inspect an element")
            .is_none()
    );
}
