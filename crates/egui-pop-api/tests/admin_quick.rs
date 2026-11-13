//! Quick settings (`admin.quick`): the settings marked quick, then those
//! changed lately, which age out; the "Set for" choice only on a setting
//! with a host level; a change saved through the host's store, which
//! another client reads; opened in the window it is asked in.

use egui::{Key, Modifiers, ViewportId};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::admin::model::{Field, Manifest};
use egui_pop_api::admin::quick::{CMD_QUICK, RECENT_SECONDS, now};
use egui_pop_api::admin::store::MemoryStore;
use egui_pop_api::admin::{ADMIN, AdminPlugin, CMD_SET, ExtraScope};
use egui_pop_api::windows::{WindowSpec, WindowsPlugin};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::strings::StrKey;
use egui_pop_model::admin::ValueScope;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{PluginId, SettingId};
use egui_pop_model::values::Descriptor;
use egui_pop_test::windows::MultiWindow;
use serde_json::{Value as Json, json};

const DARK: SettingId = SettingId::from_static("look.dark");
const SIZE: SettingId = SettingId::from_static("look.size");
const PREVIEW: SettingId = SettingId::from_static("look.preview");
const OTHER: SettingId = SettingId::from_static("look.other");

const LIBRARY: ExtraScope = ExtraScope::new(
    "library",
    StrKey::new("test-library", "This library"),
    StrKey::new("test-library-about", "while this library is open"),
);

struct Look;

impl Plugin for Look {
    fn id(&self) -> PluginId {
        PluginId::from_static("look")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.setting(DARK, Descriptor::bool("Dark look", false))?;
        ctx.setting(SIZE, Descriptor::int("Reading size", 8, 32, 12))?;
        ctx.setting(PREVIEW, Descriptor::bool("Fast preview", false))?;
        ctx.setting(OTHER, Descriptor::int("Other thing", 0, 10, 1))?;
        let admin = ctx.app().service(&ADMIN).unwrap();
        admin.declare_scope(LIBRARY);
        admin.manifest(
            Manifest::new(1)
                .scope("look", "Look", "settings")
                .section("look/main", "Main", |s| {
                    s.field(Field::setting("look.dark"));
                    s.field(Field::setting("look.size"));
                    s.field(Field::setting("look.preview").scopes(&[LIBRARY.scope]));
                    s.field(Field::setting("look.other"));
                })
                .quick(&["look.dark", "look.preview", "look.missing"]),
        );
        Ok(())
    }
}

fn app() -> App {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(AdminPlugin)
        .plugin(Look)
        .build(&egui::Context::default())
        .unwrap()
}

fn harness() -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app());
    h.run();
    h
}

fn run(h: &mut Harness<'_, App>, cmd: &egui_pop_model::ids::CommandId, args: Json) -> Json {
    let out = h.state_mut().run(cmd, args, Origin::User).now().unwrap();
    h.run();
    h.run();
    out.unwrap()
}

fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&Harness<'_, App>) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !done(h) {
        assert!(std::time::Instant::now() < deadline, "timed out: {what}");
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.run();
    }
}

#[test]
fn the_quick_settings_show_the_marked_ones_with_their_editors() {
    let mut h = harness();
    let a = h.state().handle().service(&ADMIN).unwrap();
    assert_eq!(
        a.quick_settings(h.state().handle()),
        ["look.dark", "look.preview"],
        "in order, an unknown id left out"
    );
    let out = run(&mut h, &CMD_QUICK, json!({}));
    assert_eq!(out["open"], json!(true));
    assert!(h.query_all_by_label("Quick settings").count() > 0);
    h.get_by_label("Often changed");
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Dark look");
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Fast preview");
    assert!(
        h.query_by_label("Reading size").is_none(),
        "not marked quick"
    );
    h.get_by_label("Open Dark look in Settings");
    egui_pop_test::assert_all_named(&h);
    // Typing searches every setting.
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("reading");
    h.run();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Reading size");
    assert!(h.query_by_label("Often changed").is_none());
    // Escape closes.
    h.key_press(Key::Escape);
    h.run();
    h.run();
    assert!(!a.quick_open_in(ViewportId::ROOT));
}

#[test]
fn a_setting_changed_lately_shows_then_ages_out() {
    let store = MemoryStore::new(&[ValueScope::App]);
    let mut h = harness();
    let a = h.state().handle().service(&ADMIN).unwrap();
    a.attach_store(store.clone());
    h.run();
    assert!(a.recent_settings(h.state().handle(), now()).is_empty());
    run(
        &mut h,
        &CMD_SET,
        json!({ "key": "look.other", "scope": "app", "value": 4 }),
    );
    until(&mut h, "the change saved", |_| {
        store.values(ValueScope::App).get("look.other") == Some(&json!(4))
    });
    assert_eq!(a.recent_settings(h.state().handle(), now()), ["look.other"]);
    run(&mut h, &CMD_QUICK, json!({}));
    h.get_by_label("Changed lately");
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Other thing, modified");
    // A quick one changed is not listed twice.
    run(
        &mut h,
        &CMD_SET,
        json!({ "key": "look.dark", "scope": "app", "value": true }),
    );
    until(&mut h, "the second change saved", |_| {
        store.values(ValueScope::App).get("look.dark") == Some(&json!(true))
    });
    assert_eq!(a.recent_settings(h.state().handle(), now()), ["look.other"]);
    // An hour later, it has aged out.
    assert!(
        a.recent_settings(h.state().handle(), now() + RECENT_SECONDS + 1.0)
            .is_empty()
    );
    // A change made elsewhere is not the person's: not listed.
    store.change(
        ValueScope::App,
        [("look.size".into(), Some(json!(20)))].into(),
    );
    h.run();
    h.run();
    assert_eq!(a.recent_settings(h.state().handle(), now()), ["look.other"]);
}

#[test]
fn set_for_shows_only_on_a_setting_with_a_library_level() {
    let mut h = harness();
    let a = h.state().handle().service(&ADMIN).unwrap();
    a.offer(LIBRARY.scope, true);
    run(&mut h, &CMD_QUICK, json!({}));
    h.get_by_label("Where Fast preview is set");
    assert!(h.query_by_label("Where Dark look is set").is_none());
}

#[test]
fn a_change_in_the_quick_settings_is_saved_and_another_client_reads_it() {
    let store = MemoryStore::new(&[ValueScope::App]);
    let mut h = harness();
    let a = h.state().handle().service(&ADMIN).unwrap();
    a.attach_store(store.clone());
    h.run();
    run(&mut h, &CMD_QUICK, json!({}));
    // The row's own editor: the checkbox of "Dark look".
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Dark look")
        .click();
    h.run();
    until(&mut h, "saved", |_| {
        store.values(ValueScope::App).get("look.dark") == Some(&json!(true))
    });
    // Another client of the same store reads it.
    let mut other = harness();
    let b = other.state().handle().service(&ADMIN).unwrap();
    b.attach_store(store.clone());
    other.run();
    assert!(other.state().handle().setting::<bool>(&DARK).unwrap());
}

#[test]
fn opened_from_a_window_it_shows_there() {
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .plugin(WindowsPlugin)
            .plugin(AdminPlugin)
            .plugin(Look)
            .build(ctx)
            .unwrap()
    });
    m.app()
        .open_window(WindowSpec::new("import", "Import"), |ui, _| {
            ui.label("IMPORT");
        });
    m.root_pass();
    let vi = m.app().window_viewport(&"import".into()).unwrap();
    m.window_pass(vi);
    m.focus(vi);
    m.press(vi, Key::Q, Modifiers::COMMAND | Modifiers::SHIFT);
    m.window_pass(vi);
    m.root_pass();
    m.window_pass(vi);
    let a = m.app().service(&ADMIN).unwrap();
    assert!(a.quick_open_in(vi), "open in the import window");
    assert!(!a.quick_open_in(ViewportId::ROOT), "not in the main one");
    assert_eq!(a.quick_window_in(vi), Some(vi));
}
