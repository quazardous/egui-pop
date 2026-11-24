//! Escape goes to the innermost gesture first (issue #651): a drag on a
//! slider inside a dismissable host (the Settings app, a dialog) is
//! cancelled by Escape, its value store gesture rolled back, and the host
//! stays open; only the next Escape dismisses the host. A host closed
//! while a gesture is open (its Close button, mid-drag) leaves no open
//! gesture behind.

use std::sync::{Arc, Mutex};

use egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::admin::model::{FORMAT, Manifest};
use egui_pop_api::admin::{self, ADMIN, AdminPlugin, CMD_OPEN};
use egui_pop_api::dialogs::{self, Custom, DialogSpec};
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::binding::{self, Binding, SharedStore};
use egui_pop_core::slider::Slider;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, OwnerId, PluginId};
use egui_pop_model::values::{Descriptor, Store};
use serde_json::{Value as Json, json};

const GAIN: &str = "plugin/gain";
const GO: CommandId = CommandId::from_static("gain.go");

fn store() -> SharedStore {
    let store = Arc::new(Mutex::new(Store::new()));
    store
        .lock()
        .unwrap()
        .declare(
            &OwnerId::FRAMEWORK,
            GAIN,
            Descriptor::float("Gain", 0.0, 10.0, 5.0),
        )
        .unwrap();
    store
}

fn gain(store: &SharedStore) -> f64 {
    store
        .lock()
        .unwrap()
        .get(GAIN)
        .unwrap()
        .value
        .as_f64()
        .unwrap()
}

/// Whether a gesture still holds the value (one is open on it).
fn held(store: &SharedStore) -> bool {
    store.lock().unwrap().held_by(GAIN).is_some()
}

/// The slider bound to the store, as a plugin's control draws it.
fn slider(ui: &mut egui::Ui, store: &SharedStore) {
    binding::edit(ui, Binding::bound(store, GAIN), |ui, v: &mut f64| {
        ui.add(Slider::new(v, "Gain", 0.0..=10.0).size(egui::vec2(300.0, 24.0)))
    });
}

/// A plugin putting the slider in a Settings page, as an admin widget,
/// and declaring a command for the dialog.
struct Gain(SharedStore);

impl Plugin for Gain {
    fn id(&self) -> PluginId {
        PluginId::from_static("gain")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![PluginId::from_static("egui_pop.admin")]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(CommandSpec::new(GO, "Go", "Gain"), |_| Ok(Json::Null))?;
        let admin = ctx.app().service(&ADMIN).expect("the admin service");
        let store = self.0.clone();
        admin.widget_kind("gain.slider", move |ui, _, _| slider(ui, &store));
        admin.manifest(
            Manifest::new(FORMAT)
                .scope("plugins", "Plugins", "plug")
                .section("plugins/gain", "Gain", |s| {
                    s.widget("gain", "gain.slider", json!({}));
                }),
        );
        Ok(())
    }
}

fn harness(store: &SharedStore) -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(AdminPlugin)
        .plugin(Gain(store.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn open_settings(h: &mut Harness<'static, App>) {
    h.state_mut()
        .run(&CMD_OPEN, json!({ "path": "plugins/gain" }), Origin::User)
        .now()
        .unwrap()
        .unwrap();
    h.run();
    h.run();
    assert!(admin::is_open(h.state().handle()));
}

/// Presses on the slider and drags it right, keeping the button down.
fn press_and_drag(h: &mut Harness<'static, App>) {
    let at = h
        .get_by_role_and_label(egui::accesskit::Role::Slider, "Gain")
        .rect()
        .center();
    h.drag_at(at);
    h.run();
    for i in 1..=4 {
        h.hover_at(at + egui::vec2(20.0 * i as f32, 0.0));
        h.run();
    }
}

/// Lets go of the pointer where it is.
fn release(h: &mut Harness<'static, App>) {
    let at = h.ctx.input(|i| i.pointer.latest_pos()).unwrap_or_default();
    h.drop_at(at);
    h.run();
}

#[test]
fn escape_mid_drag_in_settings_cancels_the_drag_and_keeps_settings_open() {
    let store = store();
    let mut h = harness(&store);
    open_settings(&mut h);
    press_and_drag(&mut h);
    assert!(gain(&store) > 5.0, "the drag moves the value");
    assert!(held(&store), "the drag is a gesture");
    // Escape is the drag's: the value goes back, the gesture closes, and
    // Settings stays open.
    h.key_press(Key::Escape);
    h.run();
    h.run();
    assert_eq!(gain(&store), 5.0);
    assert!(!held(&store));
    assert!(admin::is_open(h.state().handle()));
    release(&mut h);
    assert_eq!(
        gain(&store),
        5.0,
        "letting go after a cancel writes nothing"
    );
    // No gesture left: the next Escape closes Settings.
    h.key_press(Key::Escape);
    h.run();
    h.run();
    assert!(!admin::is_open(h.state().handle()));
}

#[test]
fn escape_mid_drag_in_a_dialog_cancels_the_drag_and_keeps_the_dialog_open() {
    let store = store();
    let mut h = harness(&store);
    let app: AppHandle = h.state().handle().clone();
    let d = (*app.service(&dialogs::DIALOGS).unwrap()).clone();
    let s2 = store.clone();
    d.open_custom(
        &app,
        DialogSpec::new(Custom::new("Gain", "Apply", GO)),
        move |ui, _, _| slider(ui, &s2),
    )
    .unwrap();
    h.run();
    h.run();
    assert_eq!(d.len(), 1);
    press_and_drag(&mut h);
    assert!(gain(&store) > 5.0);
    assert!(held(&store));
    h.key_press(Key::Escape);
    h.run();
    h.run();
    assert_eq!(gain(&store), 5.0);
    assert!(!held(&store));
    assert_eq!(d.len(), 1, "the dialog stays open");
    release(&mut h);
    h.key_press(Key::Escape);
    h.run();
    h.run();
    assert_eq!(d.len(), 0, "the second Escape cancels the dialog");
}

#[test]
fn a_host_closed_by_its_button_mid_drag_leaves_no_open_gesture() {
    let store = store();
    let mut h = harness(&store);
    open_settings(&mut h);
    press_and_drag(&mut h);
    assert!(held(&store));
    // The page's Close, through AccessKit: the pointer stays down on the
    // slider.
    h.get_by_label("Close").click_accesskit();
    h.run();
    h.run();
    assert!(!admin::is_open(h.state().handle()));
    assert!(!held(&store), "the gesture is cancelled with its host");
    assert_eq!(gain(&store), 5.0, "and its value put back");
    release(&mut h);
    assert_eq!(gain(&store), 5.0);
}
