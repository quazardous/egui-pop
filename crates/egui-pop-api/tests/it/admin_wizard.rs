//! The wizard kit: a wizard declared in a manifest, driven by its
//! commands as an agent drives it (validation blocking Next with its
//! reason, a conditional step, an optional step skipped, the summary, the
//! draft handed to a command), and through the accessibility tree and the
//! keys as a person does (Next greyed, the stepper's words, Cancel asking
//! first once something was entered).

use std::sync::{Arc, Mutex};

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::admin::model::hooks::{Hook, Problem};
use egui_pop_api::admin::model::{FORMAT, Field, FieldKind, Manifest, Tone, Wizard};
use egui_pop_api::admin::{
    self, ADMIN, AdminPlugin, CMD_WIZARD_BACK, CMD_WIZARD_CANCEL, CMD_WIZARD_FINISH, CMD_WIZARD_GO,
    CMD_WIZARD_NEXT, CMD_WIZARD_OPEN, CMD_WIZARD_SET, CMD_WIZARD_SKIP,
};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::{Value as Json, json};

const BOOK: CommandId = CommandId::from_static("trip.book");

/// What `trip.book` was given.
type Booked = Arc<Mutex<Vec<Json>>>;

struct Trip(Booked);

impl Plugin for Trip {
    fn id(&self) -> PluginId {
        PluginId::from_static("trip")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![PluginId::from_static("egui_pop.admin")]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let booked = self.0.clone();
        ctx.command(CommandSpec::new(BOOK, "Book a trip", "Trip"), move |call| {
            booked.lock().unwrap().push(call.args.clone());
            Ok(json!({ "booked": true }))
        })?;
        // A status bar block: the shell's, hidden by a focus screen.
        ctx.block(
            egui_pop_api::zones::STATUS_BAR,
            egui_pop_api::zones::Block::add(
                egui_pop_model::ids::BlockId::from_static("trip.status"),
                0,
                |ui, _| {
                    ui.label("Trip status");
                },
            ),
        );
        let admin = ctx.app().service(&ADMIN).expect("the admin service");
        admin.hook(
            "trip.long",
            Hook::Validate(Box::new(|v| {
                let n = v.get("nights").and_then(Json::as_f64).unwrap_or(0.0);
                if n > 14.0 {
                    vec![Problem {
                        field: String::new(),
                        tone: Tone::Warning,
                        message: "A long trip: check the dates.".into(),
                    }]
                } else {
                    Vec::new()
                }
            })),
        );
        let field = |key: &str, title: &str, kind| Field::draft("trip", key, title, kind);
        admin.manifest(
            Manifest::new(FORMAT).wizard(
                Wizard::new("trip", "Plan a trip", "trip.book")
                    .description("Where, how long, and the room.")
                    .step("where", "Where", |s| {
                        s.field(field("city", "City", FieldKind::Text).required())
                            .field(
                                field("abroad", "Abroad", FieldKind::Toggle)
                                    .default_value(json!(false)),
                            );
                    })
                    .step("passport", "Passport", |s| {
                        s.field(
                            field("passport", "Passport number", FieldKind::Text).required(),
                        );
                    })
                    .when("form.abroad")
                    .step("nights", "Nights", |s| {
                        s.field(
                            field("nights", "Nights", FieldKind::Number)
                                .range(1.0, 30.0)
                                .default_value(json!(2)),
                        );
                    })
                    .validate("trip.long")
                    .step("room", "Room", |s| {
                        s.field(
                            field("room", "Room", FieldKind::Choice)
                                .choice(json!("single"), "Single room")
                                .choice(json!("double"), "Double room")
                                .default_value(json!("single")),
                        );
                    })
                    .optional()
                    .args(json!({ "to": "${city}", "nights": "${nights}", "room": "${room}", "passport": "${passport}" }))
                    .verb("Book the trip"),
            ),
        );
        Ok(())
    }
}

fn app(booked: &Booked) -> App {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(AdminPlugin)
        .plugin(Trip(booked.clone()))
        .build(&egui::Context::default())
        .unwrap()
}

fn harness(booked: &Booked) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 640.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app(booked));
    h.run();
    h
}

fn run(h: &mut Harness<'_, App>, cmd: &CommandId, args: Json) -> Result<Json, String> {
    let out = h.state_mut().run(cmd, args, Origin::User).now().unwrap();
    h.run();
    h.run();
    out
}

#[test]
fn an_agent_drives_a_wizard_by_its_commands_and_the_draft_reaches_the_command() {
    let booked = Booked::default();
    let mut h = harness(&booked);
    let s = run(&mut h, &CMD_WIZARD_OPEN, json!({ "id": "trip" })).unwrap();
    assert!(
        admin::is_open(h.state().handle()),
        "the Settings app opens with it"
    );
    assert_eq!(s["step"], "where");
    assert_eq!(s["count"], 3, "no passport step while not abroad");
    assert_eq!(s["blocked"], "City: required.");
    // Next is refused with the reason.
    let e = run(&mut h, &CMD_WIZARD_NEXT, json!({})).unwrap_err();
    assert_eq!(e, "City: required.");
    assert!(
        run(
            &mut h,
            &CMD_WIZARD_SET,
            json!({ "key": "nowhere", "value": 1 })
        )
        .is_err()
    );
    let s = run(
        &mut h,
        &CMD_WIZARD_SET,
        json!({ "values": { "city": "Oslo", "abroad": true } }),
    )
    .unwrap();
    assert_eq!(s["count"], 4, "abroad: the passport step comes");
    assert_eq!(s["dirty"], true);
    run(&mut h, &CMD_WIZARD_NEXT, json!({})).unwrap();
    // Going ahead lands on the first invalid step and says why.
    let e = run(&mut h, &CMD_WIZARD_GO, json!({ "step": "summary" })).unwrap_err();
    assert_eq!(e, "Passport number: required.");
    let a = h.state().handle().service(&ADMIN).unwrap();
    assert_eq!(a.wizard_flow().unwrap().at, "passport");
    run(
        &mut h,
        &CMD_WIZARD_SET,
        json!({ "key": "passport", "value": "X123" }),
    )
    .unwrap();
    run(&mut h, &CMD_WIZARD_NEXT, json!({})).unwrap();
    // Out of range blocks; a warning lets through.
    run(
        &mut h,
        &CMD_WIZARD_SET,
        json!({ "key": "nights", "value": 40 }),
    )
    .unwrap();
    assert_eq!(
        run(&mut h, &CMD_WIZARD_NEXT, json!({})).unwrap_err(),
        "Nights: at most 30."
    );
    let s = run(
        &mut h,
        &CMD_WIZARD_SET,
        json!({ "key": "nights", "value": 20 }),
    )
    .unwrap();
    assert_eq!(s["problems"][0]["tone"], "warning");
    assert!(s["blocked"].is_null());
    run(&mut h, &CMD_WIZARD_NEXT, json!({})).unwrap();
    // An optional step skipped: its values back to their defaults.
    run(
        &mut h,
        &CMD_WIZARD_SET,
        json!({ "key": "room", "value": "double" }),
    )
    .unwrap();
    let s = run(&mut h, &CMD_WIZARD_SKIP, json!({})).unwrap();
    assert_eq!(s["step"], "summary");
    assert_eq!(s["values"]["room"], "single");
    assert_eq!(s["canFinish"], true);
    // Back and forth.
    let s = run(&mut h, &CMD_WIZARD_BACK, json!({})).unwrap();
    assert_eq!(s["step"], "room");
    run(&mut h, &CMD_WIZARD_GO, json!({ "step": "summary" })).unwrap();
    let done = run(&mut h, &CMD_WIZARD_FINISH, json!({})).unwrap();
    assert_eq!(done["command"], "trip.book");
    h.run();
    assert_eq!(
        *booked.lock().unwrap(),
        [json!({ "to": "Oslo", "nights": 20, "room": "single", "passport": "X123" })]
    );
    assert!(a.wizard_flow().is_none());
    assert!(
        !admin::is_open(h.state().handle()),
        "opened by the wizard, closed with it"
    );
    assert_eq!(a.last_finished().unwrap()["command"], "trip.book");
}

#[test]
fn a_person_fills_it_through_the_accessibility_tree_and_the_keys() {
    let booked = Booked::default();
    let mut h = harness(&booked);
    run(&mut h, &CMD_WIZARD_OPEN, json!({ "id": "trip" })).unwrap();
    egui_pop_test::assert_all_named(&h);
    // Next is greyed until the city is typed.
    assert!(h.get_by_label("Next").accesskit_node().is_disabled());
    assert_eq!(
        h.get_all_by_label("City: required.").count(),
        2,
        "under the field and beside Next"
    );
    h.get_by_role_and_label(Role::ListItem, "Step 1 of 4, Where");
    h.get_by_role_and_label(Role::ListItem, "Step 4 of 4, Summary, not reachable yet");
    h.get_by_role_and_label(Role::TextInput, "City").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "City")
        .type_text("Lyon");
    h.run();
    h.run();
    assert!(!h.get_by_label("Next").accesskit_node().is_disabled());
    // Alt+N goes next.
    h.key_press_modifiers(Modifiers::ALT, Key::N);
    h.run();
    h.run();
    h.get_by_role_and_label(Role::ListItem, "Step 1 of 4, Where, done");
    h.get_by_role_and_label(Role::SpinButton, "Nights");
    egui_pop_test::assert_all_named(&h);
    // Enter, with nothing focused, goes next too: the optional room.
    h.ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    h.run();
    h.key_press(Key::Enter);
    h.run();
    h.run();
    h.get_by_label("optional");
    h.get_by_label("Skip");
    h.get_by_label("Next").click();
    h.run();
    h.run();
    // The summary, each value as shown.
    h.get_by_role_and_label(Role::Group, "City: Lyon");
    h.get_by_role_and_label(Role::Group, "Abroad: No");
    h.get_by_role_and_label(Role::Group, "Room: Single room");
    egui_pop_test::assert_all_named(&h);
    // Change goes back to its step.
    h.get_by_label("Change Where").click();
    h.run();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "City");
    // Escape: something was entered, so it asks first.
    h.key_press(Key::Escape);
    h.run();
    h.run();
    h.get_by_label("Cancel Plan a trip?");
    assert!(admin::is_open(h.state().handle()));
    h.get_by_label("Discard").click();
    h.run();
    h.run();
    h.run();
    assert!(!admin::is_open(h.state().handle()));
    assert!(booked.lock().unwrap().is_empty(), "nothing booked");
}

#[test]
fn cancelling_with_nothing_entered_closes_at_once_and_back_to_the_page_left() {
    let booked = Booked::default();
    let mut h = harness(&booked);
    // Opened from the Settings app: it goes back there.
    run(&mut h, &admin::CMD_OPEN, json!({})).unwrap();
    let s = run(&mut h, &admin::CMD_OPEN, json!({ "wizard": "trip" })).unwrap();
    assert_eq!(s["wizard"], "trip");
    let out = run(&mut h, &CMD_WIZARD_CANCEL, json!({})).unwrap();
    assert_eq!(out["cancelled"], "trip");
    assert!(admin::is_open(h.state().handle()));
    // Opened alone, values given at the start are no entry.
    run(&mut h, &admin::CMD_CLOSE, json!({})).unwrap();
    run(
        &mut h,
        &CMD_WIZARD_OPEN,
        json!({ "id": "trip", "values": { "city": "Rome" }, "step": "nights" }),
    )
    .unwrap();
    let a = h.state().handle().service(&ADMIN).unwrap();
    assert_eq!(a.wizard_flow().unwrap().at, "nights");
    run(&mut h, &CMD_WIZARD_CANCEL, json!({})).unwrap();
    assert!(!admin::is_open(h.state().handle()));
    assert!(
        run(&mut h, &CMD_WIZARD_NEXT, json!({})).is_err(),
        "no wizard open"
    );
}

/// A wizard opened as a focus screen is drawn over the main window
/// whatever the window setting, the shell's bars hidden under it; the
/// setting and the shell come back when it ends.
#[test]
fn a_focus_wizard_covers_the_main_window_whatever_the_window_setting() {
    let booked = Booked::default();
    let mut h = harness(&booked);
    let a = h.state().handle().service(&ADMIN).unwrap();
    let app = h.state().handle().clone();
    a.set_in(
        &app,
        egui_pop_model::admin::layers::ValueScope::App,
        &admin::WINDOW,
        json!(true),
    )
    .unwrap();
    h.run();
    assert!(admin::in_window(&app), "the Settings app in its own window");
    assert!(h.query_by_label("Trip status").is_some(), "the shell drawn");
    let s = run(
        &mut h,
        &CMD_WIZARD_OPEN,
        json!({ "id": "trip", "focus": true }),
    )
    .unwrap();
    assert_eq!(s["focus"], true);
    assert!(a.focus_screen());
    assert!(!admin::in_window(&app), "drawn in the main window");
    assert!(
        h.query_by_label("Trip status").is_none(),
        "nothing of the shell under it"
    );
    assert!(
        h.query_by_label("Where").is_some(),
        "the wizard's step drawn"
    );
    run(&mut h, &CMD_WIZARD_CANCEL, json!({})).unwrap();
    assert!(!a.focus_screen());
    assert!(admin::in_window(&app), "the window setting holds again");
    assert!(h.query_by_label("Trip status").is_some(), "the shell back");
}

/// A toast that stays (an error) sits in the corner the footer's Finish
/// is in: the toasts keep off the footer, so the click reaches Finish.
#[test]
fn finish_is_clicked_with_a_toast_shown() {
    use egui_pop_api::toasts::{TOASTS, Toast};
    let booked = Booked::default();
    let mut h = harness(&booked);
    run(
        &mut h,
        &CMD_WIZARD_OPEN,
        json!({ "id": "trip", "values": { "city": "Lyon" } }),
    )
    .unwrap();
    run(&mut h, &CMD_WIZARD_GO, json!({ "step": "summary" })).unwrap();
    let toasts = h.state().handle().service(&TOASTS).unwrap();
    toasts.notify(Toast::error("Something failed elsewhere"));
    for _ in 0..4 {
        h.run();
    }
    let finish = h.get_by_label("Book the trip");
    let b = finish.accesskit_node().bounding_box().unwrap();
    let centre = egui::pos2(
        f64::midpoint(b.x0, b.x1) as f32,
        f64::midpoint(b.y0, b.y1) as f32,
    );
    assert_ne!(
        h.ctx.layer_id_at(centre).map(|l| l.id),
        Some(egui::Id::unique("egui_pop.toasts")),
        "a toast covers Finish"
    );
    finish.click();
    h.run();
    h.run();
    let booked = booked.lock().unwrap();
    assert_eq!(booked.len(), 1, "the click booked the trip");
    assert_eq!(booked[0]["to"], "Lyon");
}
