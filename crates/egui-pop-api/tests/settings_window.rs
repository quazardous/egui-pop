//! The settings window: search, sections, live form with resets, and the
//! shortcuts editor (capture, conflicts, reset, the layer keys go to and
//! where each comes from).

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::app::{CMD_QUIT, CMD_UNDO};
use egui_pop_api::settings_window::{self, SHORTCUTS};
use egui_pop_api::{App, DefaultPlugins};

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn open(h: &mut Harness<'_, App>) {
    // The settings window itself (`Ctrl+,` opens the Settings app).
    let _ = h.state_mut().run(
        &egui_pop_api::settings_window::CMD_SETTINGS_WINDOW,
        serde_json::Value::Null,
        egui_pop_model::commands::Origin::User,
    );
    h.run();
    assert!(settings_window::is_open(h.state().handle()));
}

/// What the search field holds.
fn search_text(h: &Harness<'_, App>) -> String {
    h.get_by_role_and_label(Role::TextInput, "Settings")
        .value()
        .unwrap_or_default()
}

#[test]
fn the_search_takes_text_typed_with_an_accesskit_focus() {
    let mut h = harness();
    open(&mut h);
    // A screen reader or an agent focuses the field and types in the
    // same frame: the text reaches the field.
    let (target_node, target_tree) = h
        .get_by_role_and_label(Role::TextInput, "Settings")
        .accesskit_node()
        .locate();
    let events = &mut h.input_mut().events;
    events.push(egui::Event::AccessKitActionRequest(
        egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Focus,
            target_node,
            target_tree,
            data: None,
        },
    ));
    events.push(egui::Event::Text("focus".into()));
    h.step();
    h.run();
    assert_eq!(search_text(&h), "focus");
    assert!(h.query_by_label("Keyboard focus").is_some());
    assert!(h.query_all_by_label("Interface zoom").next().is_none());
    // Later keys reach it too.
    h.get_by_role_and_label(Role::TextInput, "Settings")
        .type_text(" x");
    h.run();
    assert_eq!(search_text(&h), "focus x");
}

#[test]
fn search_narrows_the_form_and_a_reset_takes_a_setting_back() {
    let mut h = harness();
    open(&mut h);
    assert!(h.query_all_by_label("Appearance").count() >= 1);
    // The search field (labelled by the window's title), clicked first
    // as a person would.
    h.get_by_role_and_label(Role::TextInput, "Settings").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Settings")
        .type_text("focus");
    h.run();
    assert_eq!(search_text(&h), "focus");
    assert!(h.query_by_label("Keyboard focus").is_some());
    // A setting of the same section that does not match goes (an empty
    // search shows it with the first section). The search is fuzzy:
    // "Light or dark" stays, its description holding f…o…c…u…s.
    assert!(h.query_all_by_label("Interface zoom").next().is_none());
    // The shortcuts matching the search are listed too, nothing else.
    assert!(h.query_by_label("Edit: Undo").is_none());
    h.get_by_label("Keyboard focus").click();
    h.run();
    h.get_by_label("hover").click();
    h.run();
    let path = format!("settings/{}", egui_pop_api::SETTING_FOCUS);
    let value = |h: &Harness<'_, App>| {
        egui_pop_core::lock(&h.state().handle().store())
            .get(&path)
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(
        value(&h),
        egui_pop_model::values::Value::Choice("hover".into())
    );
    h.get_by_label("Reset Keyboard focus").click();
    h.run();
    assert_eq!(
        value(&h),
        egui_pop_model::values::Value::Choice("system".into())
    );
    // Escape closes the window.
    h.key_press(Key::Escape);
    h.run();
    assert!(!settings_window::is_open(h.state().handle()));
}

#[test]
fn new_keys_are_captured_conflicts_said_and_a_reset_brings_the_defaults_back() {
    let mut h = harness();
    open(&mut h);
    h.get_by_label(SHORTCUTS).click();
    h.run();
    h.run();
    h.get_by_label("Change the keys of Edit: Undo").click();
    h.run();
    // Captured keys never run a command (Ctrl+Q would quit).
    h.key_press_modifiers(Modifiers::COMMAND, Key::Q);
    h.run();
    h.run();
    let handle = h.state().handle().clone();
    assert_eq!(handle.keys_for(&CMD_UNDO), ["Ctrl+Q"]);
    assert!(handle.history().iter().all(|r| r.id != CMD_QUIT));
    assert!(
        h.query_by_label_contains("Ctrl+Q goes to").is_some()
            || h.query_by_label_contains("Ctrl+Q also bound to").is_some()
    );
    // Escape during a capture cancels it, the window stays.
    h.get_by_label("Change the keys of Edit: Undo").click();
    h.run();
    h.key_press(Key::Escape);
    h.run();
    assert!(settings_window::is_open(&handle));
    assert_eq!(handle.keys_for(&CMD_UNDO), ["Ctrl+Q"]);
    h.get_by_label("Default keys of Edit: Undo").click();
    h.run();
    h.run();
    assert_eq!(handle.keys_for(&CMD_UNDO), ["Ctrl+Z"]);
}

/// A plugin with a command bound in the system scope, and the state of
/// its registration when `support` is given.
struct Player(Option<egui_pop_api::system_keys::Support>);

impl egui_pop_api::Plugin for Player {
    fn id(&self) -> egui_pop_model::ids::PluginId {
        egui_pop_model::ids::PluginId::from_static("player")
    }

    fn connect(
        &mut self,
        ctx: &mut egui_pop_api::Ctx<'_>,
    ) -> Result<(), egui_pop_api::PluginError> {
        use egui_pop_api::system_keys::{KeyStatus, SYSTEM_KEYS, SystemKeys};
        use egui_pop_model::commands::CommandSpec;
        use egui_pop_model::ids::CommandId;
        use egui_pop_model::keymap::BindOptions;
        ctx.command(
            CommandSpec::new(
                CommandId::from_static("player.toggle"),
                "Play or pause",
                "Player",
            )
            .key_with("Ctrl+Alt+P", BindOptions::system()),
            |_| Ok(serde_json::Value::Null),
        )?;
        if let Some(support) = self.0.clone() {
            let keys = SystemKeys::default();
            keys.set_support(support);
            keys.set_status(
                "Ctrl+Alt+P",
                KeyStatus::Failed("taken by another application".into()),
            );
            ctx.service(&SYSTEM_KEYS, keys)?;
        }
        Ok(())
    }
}

#[test]
fn the_editor_says_where_system_shortcuts_work() {
    use egui_pop_api::system_keys::Support;
    let cases = [
        (
            None,
            "Ctrl+Alt+P works only in the window: no plugin registers global shortcuts",
        ),
        (
            Some(Support::Unavailable {
                reason: "the compositor offers no global shortcuts".into(),
            }),
            "Ctrl+Alt+P works only in the window: the compositor offers no global shortcuts",
        ),
        (
            Some(Support::Available {
                backend: "x11".into(),
            }),
            "Ctrl+Alt+P does not work outside the window: taken by another application",
        ),
    ];
    for (support, said) in cases {
        let app = App::builder()
            .plugins(DefaultPlugins)
            .plugin(Player(support))
            .build(&egui::Context::default())
            .unwrap();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 700.0))
            .build_ui_state(|ui, app: &mut App| app.show(ui), app);
        h.run();
        open(&mut h);
        h.get_by_label(SHORTCUTS).click();
        h.run();
        h.run();
        assert!(h.query_by_label(said).is_some(), "{said}");
    }
}

#[test]
fn keys_go_to_the_workspace_when_asked_and_each_says_where_it_comes_from() {
    use egui_pop_model::keymap::Layer;
    let mut h = harness();
    let handle = h.state().handle().clone();
    // Not offered: no choice, keys go to the user.
    open(&mut h);
    h.get_by_label(SHORTCUTS).click();
    h.run();
    h.run();
    assert!(h.query_by_label("this workspace").is_none());
    handle.offer_workspace_keys(true);
    handle.name_key_sources(|b| (b.layer == Layer::Workspace).then(|| "the template's".into()));
    h.run();
    h.get_by_label("this workspace").click();
    h.run();
    h.get_by_label("Change the keys of Edit: Undo").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::U);
    h.run();
    h.run();
    assert_eq!(handle.keys_for(&CMD_UNDO), ["Ctrl+U"]);
    assert!(handle.rebound_in(Layer::Workspace, &CMD_UNDO));
    assert!(!handle.rebound(&CMD_UNDO));
    // The host's name for it.
    assert!(h.query_by_label("Ctrl+U (the template's)").is_some());
    // Back to the user's: changed there, said so, and its reset.
    h.get_by_label("every workspace (yours)").click();
    h.run();
    h.get_by_label("Change the keys of Edit: Undo").click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::K);
    h.run();
    h.run();
    assert!(h.query_by_label("Ctrl+K (user)").is_some());
    h.get_by_label("Default keys of Edit: Undo").click();
    h.run();
    h.run();
    assert_eq!(handle.keys_for(&CMD_UNDO), ["Ctrl+U"]);
}

/// A plugin with settings documented at length, for the search's tests.
struct Notes;

impl egui_pop_api::Plugin for Notes {
    fn id(&self) -> egui_pop_model::ids::PluginId {
        egui_pop_model::ids::PluginId::from_static("notes")
    }

    fn title(&self) -> String {
        "Notes".into()
    }

    fn connect(
        &mut self,
        ctx: &mut egui_pop_api::Ctx<'_>,
    ) -> Result<(), egui_pop_api::PluginError> {
        use egui_pop_model::ids::SettingId;
        use egui_pop_model::values::Descriptor;
        // Its description holds s…p…e…c…i…m…e…n scattered over its words.
        ctx.setting(
            SettingId::from_static("notes.wrap"),
            Descriptor::bool("Soft wrap", true).description(
                "Soft wrap: lines break at the pane's edge, so a paragraph reads without \
                 scrolling; the file itself remains unchanged on disk.",
            ),
        )?;
        // Declared first, found by a word of its description only.
        ctx.setting(
            SettingId::from_static("notes.ruler"),
            Descriptor::bool("Ruler", false)
                .description("A line drawn down the column the zoom keeps in view."),
        )?;
        ctx.setting(
            SettingId::from_static("notes.zoom"),
            Descriptor::int("Text zoom", 50, 400, 100).description("The text's size, in percent."),
        )?;
        Ok(())
    }
}

fn notes_harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Notes)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 1400.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

/// Types in the search field, clicked first as a person would.
fn type_search(h: &mut Harness<'_, App>, text: &str) {
    h.get_by_role_and_label(Role::TextInput, "Settings").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Settings")
        .type_text(text);
    h.run();
    h.run();
}

#[test]
fn a_long_description_does_not_match_a_word_as_scattered_letters() {
    let mut h = notes_harness();
    open(&mut h);
    type_search(&mut h, "specimen");
    // The letters are all there in order, spread over the description's
    // words: no match. Nor do the built-in settings documented at length
    // (the accent colour, the pointer profile) hold the word.
    assert!(h.query_by_label("Reset Soft wrap").is_none());
    assert!(h.query_by_label("Reset Accent colour").is_none());
    assert!(h.query_by_label("Reset Pointer profile").is_none());
}

#[test]
fn a_description_matches_by_words_and_word_prefixes() {
    let mut h = notes_harness();
    open(&mut h);
    // A word's start in the description finds it.
    type_search(&mut h, "parag");
    assert!(h.query_by_label("Reset Soft wrap").is_some());
    assert!(h.query_by_label("Reset Ruler").is_none());
}

#[test]
fn a_label_still_matches_fuzzily() {
    let mut h = notes_harness();
    open(&mut h);
    // "swrp": s…w…r…p in the label, Soft wrap.
    type_search(&mut h, "swrp");
    assert!(h.query_by_label("Reset Soft wrap").is_some());
    assert!(h.query_by_label("Reset Text zoom").is_none());
}

#[test]
fn a_label_match_comes_before_a_description_match() {
    let mut h = notes_harness();
    open(&mut h);
    type_search(&mut h, "zoom");
    // Ruler is declared first, but "zoom" is only in its description.
    let top = |h: &Harness<'_, App>, label: &str| h.get_by_label(label).rect().min.y;
    assert!(
        top(&h, "Reset Text zoom") < top(&h, "Reset Ruler"),
        "the label match is listed first"
    );
}
