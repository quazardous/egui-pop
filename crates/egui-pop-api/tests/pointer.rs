//! Pointer profiles in an application: the default and the
//! application's pick, the setting switching it, the options, the
//! user's changes over the profile and the question when the profile
//! changes under them.

use egui_kittest::Harness;
use egui_pop_api::dialogs::{self, Answer};
use egui_pop_api::pointer::{self, CMD_OVERRIDES, SETTING_SLIDER_WHEEL};
use egui_pop_api::{App, DefaultPlugins};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::OwnerId;
use egui_pop_model::keymap::{Chord, Layer};
use egui_pop_model::pointer::{SCOPE_IMAGE, SCOPE_VALUE, SliderWheel, gesture};
use egui_pop_model::values::{Change, Set, Value};

fn chord(text: &str) -> Chord {
    Chord::parse(text).unwrap()
}

fn passes(app: &mut App, ctx: &egui::Context, n: usize) {
    for _ in 0..n {
        app.logic(ctx);
    }
}

#[test]
fn the_default_profile_is_document_and_the_context_gets_its_map() {
    let ctx = egui::Context::default();
    let mut app = App::builder().build(&ctx).unwrap();
    passes(&mut app, &ctx, 2);
    let map = egui_pop_core::pointer::map(&ctx);
    assert_eq!(map.profile, "document");
    assert_eq!(
        map.command(SCOPE_IMAGE, &chord("Wheel")),
        Some(&gesture::IMAGE_PAN)
    );
    assert_eq!(
        map.command(SCOPE_IMAGE, &chord("Ctrl+Wheel")),
        Some(&gesture::IMAGE_ZOOM)
    );
    // The gestures are commands, kept out of the palette, with their
    // pointer chords as keys.
    let h = app.handle();
    let spec = h.command(&gesture::IMAGE_ZOOM).unwrap();
    assert!(!spec.palette);
    assert!(
        h.keys_for(&gesture::IMAGE_ZOOM)
            .contains(&"Ctrl+Wheel".to_string())
    );
    assert_eq!(pointer::profile_id(h), "document");
}

#[test]
fn an_application_picks_its_profile_and_the_setting_switches_it() {
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .pointer_profile("darktable")
        .build(&ctx)
        .unwrap();
    passes(&mut app, &ctx, 2);
    let map = egui_pop_core::pointer::map(&ctx);
    assert_eq!(map.profile, "darktable");
    assert_eq!(
        map.command(SCOPE_IMAGE, &chord("Wheel")),
        Some(&gesture::IMAGE_ZOOM)
    );
    assert_eq!(map.options.slider_wheel, SliderWheel::Value);
    assert!(egui_pop_core::slider::gestures(&ctx).wheel_steps);
    pointer::set_profile(app.handle(), "trackpad").unwrap();
    passes(&mut app, &ctx, 2);
    let map = egui_pop_core::pointer::map(&ctx);
    assert_eq!(map.profile, "trackpad");
    assert_eq!(
        map.command(SCOPE_IMAGE, &chord("Wheel")),
        Some(&gesture::IMAGE_PAN)
    );
    // The darktable bindings are gone, not underneath.
    assert_eq!(map.command(SCOPE_IMAGE, &chord("MiddleClick")), None);
    assert!(!egui_pop_core::slider::gestures(&ctx).wheel_steps);
    assert!(pointer::set_profile(app.handle(), "nonesuch").is_err());
}

#[test]
fn the_retired_adobe_id_gives_lightroom() {
    let ctx = egui::Context::default();
    let mut app = App::builder().pointer_profile("adobe").build(&ctx).unwrap();
    passes(&mut app, &ctx, 2);
    assert_eq!(pointer::profile_id(app.handle()), "lightroom");
    assert_eq!(egui_pop_core::pointer::map(&ctx).profile, "lightroom");
    pointer::set_profile(app.handle(), "photoshop").unwrap();
    passes(&mut app, &ctx, 2);
    assert_eq!(egui_pop_core::pointer::map(&ctx).profile, "photoshop");
    // Asked by its old id, the setting stores the profile it names.
    pointer::set_profile(app.handle(), "adobe").unwrap();
    passes(&mut app, &ctx, 2);
    assert_eq!(pointer::profile_id(app.handle()), "lightroom");
    let stored = egui_pop_core::lock(&app.handle().store())
        .get(&format!("settings/{}", pointer::SETTING_PROFILE))
        .and_then(|e| e.value.as_choice().map(str::to_string));
    assert_eq!(stored.as_deref(), Some("lightroom"));
}

#[test]
fn an_option_set_overrides_the_profiles_default() {
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .pointer_profile("darktable")
        .build(&ctx)
        .unwrap();
    passes(&mut app, &ctx, 2);
    egui_pop_core::lock(&app.handle().store())
        .set(
            Set::new(
                &format!("settings/{SETTING_SLIDER_WHEEL}"),
                Change::To(Value::Choice("guarded".into())),
                Origin::User,
            )
            .by(&OwnerId::FRAMEWORK),
        )
        .unwrap();
    passes(&mut app, &ctx, 1);
    let map = egui_pop_core::pointer::map(&ctx);
    assert_eq!(map.options.slider_wheel, SliderWheel::Guarded);
    let g = egui_pop_core::slider::gestures(&ctx);
    assert!(!g.wheel_steps);
    // darktable's value modifiers: Ctrl finer, Shift coarser, Alt steps,
    // Ctrl+Alt scrolls the panel.
    assert_eq!(g.wheel, egui::Modifiers::ALT);
    assert_eq!(g.fine, egui::Modifiers::COMMAND);
    assert_eq!(g.coarse, egui::Modifiers::SHIFT);
    assert_eq!(g.panel, egui::Modifiers::COMMAND | egui::Modifiers::ALT);
    assert!(map.scope(SCOPE_VALUE).count() >= 4);
}

#[test]
fn a_gesture_the_user_changed_stays_over_the_profile() {
    let ctx = egui::Context::default();
    let mut app = App::builder().build(&ctx).unwrap();
    passes(&mut app, &ctx, 1);
    app.handle()
        .rebind_gestures(Layer::User, gesture::IMAGE_ZOOM, &["Alt+Wheel", "Pinch"]);
    passes(&mut app, &ctx, 1);
    let map = egui_pop_core::pointer::map(&ctx);
    assert_eq!(
        map.command(SCOPE_IMAGE, &chord("Alt+Wheel")),
        Some(&gesture::IMAGE_ZOOM)
    );
    assert_eq!(map.command(SCOPE_IMAGE, &chord("Ctrl+Wheel")), None);
    assert_eq!(pointer::overrides(app.handle()), [gesture::IMAGE_ZOOM]);
    // Another profile: the change stays (until the user drops it).
    pointer::set_profile(app.handle(), "blender").unwrap();
    passes(&mut app, &ctx, 2);
    let map = egui_pop_core::pointer::map(&ctx);
    assert_eq!(map.profile, "blender");
    assert_eq!(
        map.command(SCOPE_IMAGE, &chord("Alt+Wheel")),
        Some(&gesture::IMAGE_ZOOM)
    );
    assert_eq!(
        map.command(SCOPE_IMAGE, &chord("Wheel")),
        None,
        "Blender's wheel zoom is replaced by the user's"
    );
}

#[test]
fn a_change_of_profile_asks_whether_to_keep_the_users_changes() {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .build(&egui::Context::default())
        .unwrap();
    let d = (*app.handle().service(&dialogs::DIALOGS).unwrap()).clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    // No change of the user's: no question.
    pointer::set_profile(h.state().handle(), "photoshop").unwrap();
    h.run();
    assert!(d.current().is_none());
    h.state()
        .handle()
        .rebind(gesture::IMAGE_PAN, Some("MiddleDrag".into()));
    h.run();
    pointer::set_profile(h.state().handle(), "darktable").unwrap();
    h.run();
    let current = d.describe();
    assert_eq!(current["kind"], "choose", "{current}");
    assert_eq!(current["options"][0]["label"], "Keep my changes");
    assert_eq!(current["options"][1]["label"], "Drop my changes");
    d.answer(
        Answer::Value {
            value: serde_json::json!("drop"),
        },
        Origin::User,
    )
    .unwrap();
    h.run();
    h.run();
    assert!(pointer::overrides(h.state().handle()).is_empty());
    let map = egui_pop_core::pointer::map(&h.ctx);
    assert_eq!(
        map.command(SCOPE_IMAGE, &chord("PrimaryDrag")),
        Some(&gesture::IMAGE_PAN),
        "darktable's pan is back"
    );
    // The command runs on its own too.
    let out = h.state_mut().run(
        &CMD_OVERRIDES,
        serde_json::json!({ "value": "keep" }),
        Origin::User,
    );
    assert_eq!(out.now(), Some(Ok(serde_json::json!({ "kept": 0 }))));
}
