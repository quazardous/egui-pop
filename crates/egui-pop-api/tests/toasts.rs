//! Toasts: shown without the focus, an action runs its command, they
//! expire by level, the queue is data; they slide in and out (not under
//! reduced motion), their time left stops under the pointer, they show
//! in the corner chosen and cast the overlays' shadow.

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::app::OverlayKind;
use egui_pop_api::core::theme::ThemeSpec;
use egui_pop_api::dialogs;
use egui_pop_api::toasts::{self, Corner, Phase, Toast};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::motion;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use egui_pop_model::values::{Change, Set, Value};
use serde_json::{Value as Json, json};

const REVEAL: CommandId = CommandId::from_static("files.show");

struct Files(Rc<RefCell<Vec<Json>>>);

impl Plugin for Files {
    fn id(&self) -> PluginId {
        PluginId::from_static("files")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let runs = self.0.clone();
        ctx.command(CommandSpec::new(REVEAL, "Reveal", "Files"), move |call| {
            runs.borrow_mut().push(call.args.clone());
            Ok(Json::Null)
        })?;
        Ok(())
    }
}

fn harness() -> (
    Harness<'static, App>,
    Rc<RefCell<Vec<Json>>>,
    toasts::Toasts,
) {
    let runs = Rc::default();
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Files(Rc::clone(&runs)))
        .build(&egui::Context::default())
        .unwrap();
    let t = (*app.handle().service(&toasts::TOASTS).unwrap()).clone();
    let h = Harness::builder()
        .with_size(egui::vec2(800.0, 500.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    (h, runs, t)
}

/// Whether a toast shows this text (the live region also says it).
fn on_screen(h: &Harness<'_, App>, text: &str) -> bool {
    h.query_all_by_label(text)
        .any(|n| n.accesskit_node().role() != egui::accesskit::Role::Status)
}

#[test]
fn an_action_runs_its_command_and_closes_the_toast() {
    let (mut h, runs, t) = harness();
    h.run();
    let id = t.notify(Toast::success("Exported 3 images").action(
        "Show",
        REVEAL,
        json!({ "path": "out" }),
    ));
    h.run();
    assert!(on_screen(&h, "Exported 3 images"));
    assert!(
        egui_pop_core::focus::focused(&h.ctx).is_none(),
        "no focus taken"
    );
    h.get_by_label("Show").click();
    h.run();
    h.run();
    assert_eq!(*runs.borrow(), [json!({ "path": "out" })]);
    assert!(t.shown().is_empty());
    assert_eq!(t.history()[0].id, id);
    assert_eq!(t.history()[0].ended, Some("action"));
    let journal = h.state().handle().events_since(0);
    assert!(journal.iter().any(|e| e.name == "toast.shown"));
}

#[test]
fn information_expires_errors_stay_and_the_queue_waits() {
    let (mut h, _, t) = harness();
    h.run();
    t.notify(Toast::info("Saved").lifetime(0.5));
    h.run();
    for _ in 0..4 {
        h.step();
    }
    h.run();
    assert!(!on_screen(&h, "Saved"), "expired");
    assert_eq!(t.history()[0].ended, Some("expired"));
    t.notify(Toast::error("Disk full"));
    for n in 0..4 {
        t.notify(Toast::info(&format!("Note {n}")).sticky());
    }
    h.run();
    // Four on screen, the others wait.
    assert!(on_screen(&h, "Note 2"));
    assert!(!on_screen(&h, "Note 3"));
    for _ in 0..40 {
        h.step();
    }
    assert!(on_screen(&h, "Disk full"), "an error stays");
    // Closed by its button, or through the bridge by the user (devtools;
    // an agent may not unless the application offers it).
    h.get_by_label("Close: Disk full").click();
    h.run();
    assert!(!on_screen(&h, "Disk full"));
    assert!(on_screen(&h, "Note 3"), "its turn came");
    let id = t.shown()[0].id;
    let f = h
        .state_mut()
        .debug_call_as("toasts", "dismiss", &json!({ "id": id }), Origin::User);
    assert!(f.ok, "{}", f.message);
    assert!(!t.shown().iter().any(|s| s.id == id));
    let snapshot = h.state().handle().debug_snapshot();
    assert_eq!(snapshot["toasts"]["history"].as_array().unwrap().len(), 3);
}

#[test]
fn long_text_wraps_before_the_close_button_and_overlays_are_reported() {
    let (mut h, _, t) = harness();
    h.run();
    let long =
        "The export of 3 images failed: there is no space left on the destination disk you chose.";
    t.notify(Toast::error(long).action("Retry", REVEAL, json!(null)));
    h.run();
    let text = h
        .query_all_by_label(long)
        .find(|n| n.accesskit_node().role() != egui::accesskit::Role::Status)
        .unwrap()
        .rect();
    let close = h.get_by_label(&format!("Close: {long}")).rect();
    let retry = h.get_by_label("Retry").rect();
    assert!(text.right() <= retry.left(), "{text:?} then {retry:?}");
    assert!(
        retry.right() <= close.left() + 0.5,
        "{retry:?} then {close:?}"
    );
    assert!(text.height() > close.height(), "it wrapped: {text:?}");
    let app = h.state().handle().clone();
    let toasts = app
        .overlay_rects()
        .into_iter()
        .find(|o| o.kind == OverlayKind::Toasts)
        .expect("the toasts are reported");
    assert!(toasts.rect.contains_rect(close));
    // A dialog over them: reported as the dialog, by its id.
    let d = app.service(&dialogs::DIALOGS).unwrap();
    let id = d.open(dialogs::Info::new("About", "A note."));
    h.run();
    let rects = app.overlay_rects();
    let dialog = rects
        .iter()
        .find(|o| o.kind == OverlayKind::Dialog)
        .expect("the dialog is reported");
    assert_eq!(dialog.id, format!("dialog:{id}"));
    assert!(dialog.rect.contains_rect(h.get_by_label("A note.").rect()));
    let snapshot = app.debug_snapshot();
    let kinds: Vec<&str> = snapshot["overlay_rects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"dialog") && kinds.contains(&"toasts"),
        "{kinds:?}"
    );
}

/// A harness on a frozen motion clock at 10 s, built as `build` says.
fn frozen(
    build: impl FnOnce(egui_pop_api::AppBuilder) -> egui_pop_api::AppBuilder,
) -> (
    Harness<'static, App>,
    toasts::Toasts,
    egui_pop_model::clock::ManualClock,
) {
    let app = build(App::builder().plugins(DefaultPlugins))
        .build(&egui::Context::default())
        .unwrap();
    let t = (*app.handle().service(&toasts::TOASTS).unwrap()).clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 500.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    let clock = motion::freeze(&h.ctx, 10.0);
    h.run();
    (h, t, clock)
}

/// Where a toast's text is drawn.
fn text_rect(h: &Harness<'_, App>, text: &str) -> egui::Rect {
    h.query_all_by_label(text)
        .find(|n| n.accesskit_node().role() != egui::accesskit::Role::Status)
        .unwrap_or_else(|| panic!("{text} is not drawn"))
        .rect()
}

fn slide(h: &Harness<'_, App>) -> f64 {
    f64::from(egui_pop_core::theme::Tokens::of(&h.ctx).motion.toast_slide)
}

#[test]
fn a_toast_slides_in_from_its_corner_s_side_and_out_again() {
    let (mut h, t, clock) = frozen(|b| b);
    let slide = slide(&h);
    assert!(slide > 0.0, "motion.toast.slide");
    let id = t.notify(Toast::info("Saved").sticky());
    h.run();
    let mut xs = Vec::new();
    for k in 0..=4 {
        clock.set(10.0 + slide * f64::from(k) / 4.0);
        h.run();
        xs.push(text_rect(&h, "Saved").left());
        let phase = t.shown()[0].phase;
        assert_eq!(
            phase,
            if k < 4 { Phase::Appear } else { Phase::Idle },
            "{k}"
        );
    }
    assert!(xs[0] >= 800.0, "out of sight at first: {xs:?}");
    assert!(xs.windows(2).all(|w| w[1] < w[0]), "comes in: {xs:?}");
    // Eased: more than half the way in the first quarter of the time.
    assert!(xs[0] - xs[1] > (xs[0] - xs[4]) / 2.0, "{xs:?}");
    clock.set(11.0);
    h.run();
    assert_eq!(text_rect(&h, "Saved").left(), xs[4], "in place");
    // Out again: gone from the queue at once, drawn while it slides,
    // with nothing left for a screen reader or the pointer.
    assert!(t.dismiss(id));
    h.run();
    assert!(t.shown().is_empty());
    let leaving = t.leaving();
    assert_eq!(leaving.len(), 1);
    assert_eq!(leaving[0].phase, Phase::Disappear);
    assert!(!on_screen(&h, "Saved"));
    assert!(h.query_by_label("Close: Saved").is_none());
    clock.set(11.0 + slide / 2.0);
    h.run();
    assert_eq!(t.leaving().len(), 1, "still sliding");
    clock.set(11.0 + slide);
    h.run();
    assert!(t.leaving().is_empty());
    assert_eq!(t.history()[0].ended, Some("dismissed"));
}

#[test]
fn under_reduced_motion_a_toast_appears_and_goes_at_once() {
    let (mut h, t, clock) = frozen(|b| b.theme(ThemeSpec::new("pop-dark").reduced_motion(true)));
    let id = t.notify(Toast::info("Saved").sticky());
    h.run();
    assert_eq!(t.shown()[0].phase, Phase::Idle);
    let x = text_rect(&h, "Saved").left();
    assert!(x < 800.0, "in place at once: {x}");
    clock.set(12.0);
    h.run();
    assert_eq!(text_rect(&h, "Saved").left(), x, "it never moved");
    t.dismiss(id);
    h.run();
    assert!(t.leaving().is_empty(), "gone at once");
    assert!(!on_screen(&h, "Saved"));
}

#[test]
fn the_time_left_empties_and_stops_while_the_pointer_holds_the_toast() {
    let (mut h, t, clock) = frozen(|b| b);
    t.notify(Toast::info("Saved").lifetime(4.0));
    h.run();
    let left = |t: &toasts::Toasts| t.shown()[0].left.unwrap();
    assert!((left(&t) - 1.0).abs() < 1e-3, "full at first");
    clock.set(11.0);
    h.run();
    assert!((left(&t) - 0.75).abs() < 1e-3, "{}", left(&t));
    // Held: the time stops.
    h.get_by_label("Close: Saved").hover();
    h.run();
    assert_eq!(t.shown()[0].held, Some(11.0));
    clock.set(13.0);
    h.run();
    assert!((left(&t) - 0.75).abs() < 1e-3, "stopped: {}", left(&t));
    // Let go: it goes on from where it stopped.
    h.hover_at(egui::pos2(10.0, 10.0));
    h.run();
    assert_eq!(t.shown()[0].held, None);
    assert!((left(&t) - 0.75).abs() < 1e-3, "{}", left(&t));
    clock.set(14.0);
    h.run();
    assert!((left(&t) - 0.5).abs() < 1e-3, "{}", left(&t));
    clock.set(16.01);
    h.run();
    assert!(t.shown().is_empty(), "expired two seconds late");
    assert_eq!(t.history()[0].ended, Some("expired"));
    // A toast that stays has no bar.
    t.notify(Toast::error("Disk full"));
    h.run();
    assert_eq!(t.shown()[0].left, None);
}

#[test]
fn each_corner_shows_the_newest_nearest_the_edge() {
    for corner in Corner::ALL {
        let (mut h, t, clock) = frozen(|b| b.toast_corner(corner));
        assert_eq!(toasts::corner(h.state().handle()), corner);
        t.notify(Toast::info("First").sticky());
        t.notify(Toast::info("Second").sticky());
        h.run();
        clock.set(11.0);
        h.run();
        let first = h.get_by_label("Close: First").rect();
        let second = h.get_by_label("Close: Second").rect();
        let name = corner.as_str();
        if corner.top() {
            assert!(second.top() < first.top(), "{name}: the newest on top");
            assert!(second.top() < 100.0, "{name}: {second:?}");
        } else {
            assert!(
                second.top() > first.top(),
                "{name}: the newest at the bottom"
            );
            assert!(second.bottom() > 400.0, "{name}: {second:?}");
        }
        let text = text_rect(&h, "First");
        match corner {
            Corner::TopLeft | Corner::BottomLeft => {
                assert!(first.right() < 400.0, "{name}: {first:?}");
            }
            Corner::TopRight | Corner::BottomRight => {
                assert!(text.left() > 400.0, "{name}: {text:?}");
            }
            Corner::TopCenter | Corner::BottomCenter => {
                assert!(
                    text.left() < 400.0 && first.right() > 400.0,
                    "{name}: {text:?} {first:?}"
                );
            }
        }
    }
}

#[test]
fn the_corner_is_a_setting_under_appearance_and_it_persists() {
    let dir = std::env::temp_dir().join(format!("egui-pop-toast-corner-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let build = || {
        App::builder()
            .plugins(DefaultPlugins)
            .persist(&dir)
            .toast_corner(Corner::TopRight)
            .build(&egui::Context::default())
            .unwrap()
    };
    let app = build();
    let setting = app
        .handle()
        .settings()
        .into_iter()
        .find(|s| s.id == toasts::SETTING_CORNER)
        .unwrap();
    assert_eq!(setting.section, "Appearance");
    assert_eq!(
        setting.descriptor.default,
        Value::Choice("top-right".into()),
        "the application's default"
    );
    egui_pop_core::lock(&app.handle().store())
        .set(Set::new(
            &format!("settings/{}", toasts::SETTING_CORNER),
            Change::To(Value::Choice("bottom-center".into())),
            Origin::User,
        ))
        .unwrap();
    assert_eq!(toasts::corner(app.handle()), Corner::BottomCenter);
    app.save().unwrap();
    drop(app);
    let again = build();
    assert_eq!(toasts::corner(again.handle()), Corner::BottomCenter);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_toast_casts_the_shadow_overlays_cast() {
    use egui_pop_core::kit::Kit;
    use egui_pop_core::theme::Tokens;
    for spec in [
        ThemeSpec::new("pop-dark"),
        ThemeSpec::new("pop-light"),
        ThemeSpec::new("pop-dark")
            .override_token("shadow.popup.blur", json!({ "value": 30, "unit": "px" })),
    ] {
        let ctx = egui::Context::default();
        let _app = App::builder()
            .plugins(DefaultPlugins)
            .theme(spec.clone())
            .build(&ctx)
            .unwrap();
        let shadow = Kit::of(&ctx).toast.data.shadow;
        assert_ne!(shadow, egui::Shadow::NONE, "{}", spec.label());
        assert_eq!(shadow, Tokens::of(&ctx).shadow.popup, "{}", spec.label());
    }
    let ctx = egui::Context::default();
    let _app = App::builder()
        .theme(
            ThemeSpec::new("pop-dark")
                .override_token("shadow.popup.blur", json!({ "value": 30, "unit": "px" })),
        )
        .build(&ctx)
        .unwrap();
    assert_eq!(Kit::of(&ctx).toast.data.shadow.blur, 30);
}
