//! Dialogs: confirmations (verbs, danger), forms from descriptors with
//! the in-app path picker, choices, answers from an agent, the close
//! button, movable dialogs (drag, clamp, recentre, keyboard move, kept
//! places) and their backdrops.

use std::cell::RefCell;
use std::rc::Rc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::dialogs::{
    self, Backdrop, Choose, ChooseOption, Confirm, DialogSpec, Form, Info, SETTING_CLOSABLE,
    SETTING_MOVABLE, STEP_ANSWER, STEP_MOVE,
};
use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use egui_pop_model::values::{Change, Descriptor, PathKind, Set, Value};
use serde_json::{Value as Json, json};

const GO: CommandId = CommandId::from_static("doc.go");

type Runs = Rc<RefCell<Vec<(Json, Origin)>>>;

struct Doc(Runs);

impl Plugin for Doc {
    fn id(&self) -> PluginId {
        PluginId::from_static("doc")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let runs = self.0.clone();
        ctx.command(CommandSpec::new(GO, "Go", "Doc"), move |call| {
            runs.borrow_mut()
                .push((call.args.clone(), call.origin.clone()));
            Ok(Json::Null)
        })?;
        Ok(())
    }
}

fn harness() -> (Harness<'static, App>, Runs, dialogs::Dialogs) {
    let runs: Runs = Rc::default();
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Doc(runs.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let d = app.handle().service(&dialogs::DIALOGS).unwrap();
    let h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    (h, runs, (*d).clone())
}

#[test]
fn a_confirmation_names_its_action_and_enter_runs_it() {
    let (mut h, runs, d) = harness();
    d.open(Confirm::new("Export the page?", "Export", GO).args(json!({ "n": 1 })));
    h.run();
    assert!(h.query_by_label("Export the page?").is_some());
    assert!(h.get_by_label("Export").is_focused());
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(runs.borrow().len(), 1);
    assert_eq!(runs.borrow()[0].0, json!({ "n": 1 }));
    assert!(d.is_empty());
}

#[test]
fn a_dangerous_confirmation_starts_on_cancel_and_escape_cancels() {
    let (mut h, runs, d) = harness();
    d.open(
        Confirm::new("Delete 3 images?", "Delete 3 images", GO)
            .detail("They go to the trash.")
            .danger(),
    );
    h.run();
    assert!(h.get_by_label("Cancel").is_focused());
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(d.is_empty());
    assert!(runs.borrow().is_empty());
    assert_eq!(d.answered()[0].1, dialogs::Answer::Cancel);
    // Dialogs queue: the second shows when the first is answered.
    d.open(Info::new("First", "One."));
    d.open(Info::new("Second", "Two."));
    h.run();
    assert!(h.query_by_label("First").is_some());
    h.get_by_label("Close").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Second").is_some());
}

#[test]
fn a_form_edits_typed_fields_and_picks_a_path_in_the_app() {
    let dir = std::env::temp_dir().join(format!("egui-pop-dialogs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    std::fs::write(dir.join("photo.png"), "x").unwrap();
    let (mut h, runs, d) = harness();
    d.open(
        Form::new("Export", "Export", GO)
            .field(
                "format",
                Descriptor::choice("Format", &["png", "jpeg"], "png"),
            )
            .field("quality", Descriptor::int("Quality", 1, 100, 90))
            .field_at(
                "notes",
                Descriptor::path("Notes", PathKind::File, &["txt"]),
                egui_pop_model::values::Value::Text(dir.join("none").display().to_string()),
            ),
    );
    h.run();
    h.get_by_label("Browse…").click();
    // The dialog grows with the picker and is placed again: a few frames.
    for _ in 0..3 {
        h.run();
    }
    // Folders first, then the files the field accepts.
    assert!(h.query_by_label("sub/").is_some());
    assert!(h.query_by_label("photo.png").is_none());
    // The file chooser's in-app picker: a click highlights, Open chooses.
    h.get_by_label("notes.txt").click();
    h.run();
    h.get_by_label("Open").click();
    h.run();
    h.run();
    assert!(h.query_by_label("notes.txt").is_none(), "the picker closed");
    h.query_all_by_label("Export").last().unwrap().click();
    h.run();
    h.run();
    let (args, origin) = runs.borrow()[0].clone();
    assert_eq!(origin, Origin::User);
    assert_eq!(args["format"], "png");
    assert_eq!(args["quality"], 90);
    assert_eq!(
        args["notes"],
        json!(dir.join("notes.txt").display().to_string())
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_agent_sees_and_answers_what_a_person_would() {
    let (mut h, runs, d) = harness();
    d.open(Choose::new("Which side?", &["Left", "Right"], GO));
    h.run();
    let snapshot = h.state().handle().debug_snapshot();
    assert_eq!(snapshot["dialogs"]["current"]["kind"], "choose");
    assert_eq!(
        snapshot["dialogs"]["current"]["options"][1]["label"],
        "Right"
    );
    let feedback = h.state_mut().debug_call(
        "dialogs",
        "answer",
        &json!({ "answer": "choose", "index": 1 }),
    );
    assert!(feedback.ok, "{}", feedback.message);
    h.run();
    h.run();
    let (args, origin) = runs.borrow()[0].clone();
    assert_eq!(
        args,
        json!({ "choice": 1, "label": "Right", "value": "Right" })
    );
    assert_eq!(origin, Origin::Agent("debug-bridge".into()));
    // A wrong answer is refused, the dialog stays.
    d.open(Confirm::new("Sure?", "Go", GO));
    h.run();
    let feedback = h
        .state_mut()
        .debug_call("dialogs", "answer", &json!({ "answer": "maybe" }));
    assert!(!feedback.ok);
    assert_eq!(d.len(), 1);
}

#[test]
fn a_choice_answers_its_option_s_value_and_its_filter_narrows_the_list() {
    let (mut h, runs, d) = harness();
    d.open(
        Choose::with_options(
            "Open a profile",
            vec![
                ChooseOption::new("Studio", json!("/p/studio")).hint("studio.helios-profile"),
                ChooseOption::new("Field", json!("/p/field")),
                ChooseOption::new("Night", json!("/p/night")),
            ],
            GO,
        )
        .value_key("path"),
    );
    h.run();
    let before = h.state().handle().last_event();
    // The filter has the focus: typing narrows the list.
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("fld");
    h.run();
    assert!(h.query_by_label("Field").is_some());
    assert!(h.query_by_label("Night").is_none());
    assert!(h.query_by_label_contains("Studio").is_none());
    let current = d.describe();
    assert_eq!(current["filter"], "fld");
    assert_eq!(current["shown"], json!([1]));
    // Enter takes the first shown; the command gets its value.
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    assert_eq!(
        runs.borrow()[0].0,
        json!({ "choice": 1, "label": "Field", "path": "/p/field" })
    );
    let events = h.state().handle().events_since(before);
    let names: Vec<&str> = events.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"dialog.filtered"), "{names:?}");
    let chosen = events.iter().find(|e| e.name == "dialog.chosen").unwrap();
    assert_eq!(chosen.data["value"], "/p/field");
    // Arrows move through what is shown.
    d.open(Choose::new("Which side?", &["Left", "Right"], GO));
    h.run();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    assert_eq!(runs.borrow()[1].0["value"], "Right");
}

#[test]
fn an_agent_answers_by_value_and_a_form_shows_its_values_plainly() {
    let (mut h, runs, d) = harness();
    d.open(
        Form::new("Export", "Export", GO)
            .field("quality", Descriptor::int("Quality", 1, 100, 90))
            .field(
                "format",
                Descriptor::choice("Format", &["png", "jpeg"], "png"),
            ),
    );
    h.run();
    let current = d.describe();
    assert_eq!(current["values"], json!({ "quality": 90, "format": "png" }));
    assert!(
        current["fields"][0]["value"].is_object(),
        "typed values stay"
    );
    d.answer(dialogs::Answer::Cancel, Origin::User).unwrap();
    h.run();
    d.open(Choose::new("Which side?", &["Left", "Right"], GO));
    h.run();
    let feedback = h.state_mut().debug_call(
        "dialogs",
        "answer",
        &json!({ "answer": "choose", "value": "Left" }),
    );
    assert!(feedback.ok, "{}", feedback.message);
    h.run();
    h.run();
    assert_eq!(runs.borrow()[0].0["choice"], 0);
}

struct Gate;

impl Plugin for Gate {
    fn id(&self) -> PluginId {
        PluginId::from_static("gate")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_ANSWER, |_, args| {
            match args["answer"]["answer"].as_str() {
                // A cancel becomes the first option; an acceptance waits.
                Some("cancel") if args["dialog"]["kind"] == "choose" => {
                    let mut a = args.clone();
                    a["answer"] = json!({ "answer": "choose", "index": 0 });
                    Before::Redirect(a)
                }
                Some("accept") => Before::Veto("not before saving".into()),
                _ => Before::Go,
            }
        });
        Ok(())
    }
}

#[test]
fn hooks_veto_or_redirect_an_answer() {
    let runs: Runs = Rc::default();
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Doc(runs.clone()))
        .plugin(Gate)
        .build(&egui::Context::default())
        .unwrap();
    let d = (*app.handle().service(&dialogs::DIALOGS).unwrap()).clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    d.open(Confirm::new("Export?", "Export", GO));
    h.run();
    h.get_by_label("Export").click();
    h.run();
    // Vetoed: the dialog stays, saying why.
    assert_eq!(d.len(), 1);
    assert!(h.query_by_label("not before saving").is_some());
    assert!(runs.borrow().is_empty());
    d.answer(dialogs::Answer::Cancel, Origin::User).unwrap();
    h.run();
    d.open(Choose::new("Which side?", &["Left", "Right"], GO));
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    h.run();
    assert_eq!(
        runs.borrow()[0].0["value"],
        "Left",
        "the cancel was redirected"
    );
}

/// Refuses every cancel: a dialog that must be answered.
struct NoCancel;

impl Plugin for NoCancel {
    fn id(&self) -> PluginId {
        PluginId::from_static("no_cancel")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_ANSWER, |_, args| {
            if args["answer"]["answer"] == "cancel" {
                Before::Veto("answer it first".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

/// What dismissing a closable confirmation did, by the ✕ or by Escape:
/// the dialogs left, the answers recorded, the commands run, whether the
/// refusal shows.
fn dismiss(refuse: bool, by_close: bool) -> (usize, String, usize, bool) {
    let runs: Runs = Rc::default();
    let mut builder = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Doc(runs.clone()));
    if refuse {
        builder = builder.plugin(NoCancel);
    }
    let app = builder.build(&egui::Context::default()).unwrap();
    let d = (*app.handle().service(&dialogs::DIALOGS).unwrap()).clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    d.open(DialogSpec::new(Confirm::new("Export?", "Export", GO)).closable(true));
    h.run();
    assert!(
        !h.get_by_role_and_label(Role::Button, "Close").is_focused(),
        "the close button is never focused when the dialog opens"
    );
    assert!(h.get_by_label("Export").is_focused());
    if by_close {
        h.get_by_role_and_label(Role::Button, "Close").click();
    } else {
        h.key_press(egui::Key::Escape);
    }
    h.run();
    h.run();
    let refused = h.query_by_label("answer it first").is_some();
    let answered = format!("{:?}", d.answered());
    let ran = runs.borrow().len();
    (d.len(), answered, ran, refused)
}

#[test]
fn the_close_button_is_escape_even_when_refused() {
    for refuse in [false, true] {
        let by_escape = dismiss(refuse, false);
        let by_close = dismiss(refuse, true);
        assert_eq!(by_close, by_escape, "refused: {refuse}");
        if refuse {
            assert_eq!(by_close.0, 1, "the dialog stays");
            assert!(by_close.3, "saying why");
        } else {
            assert_eq!(by_close.0, 0, "cancelled");
            assert!(by_close.1.contains("Cancel"));
        }
        assert_eq!(by_close.2, 0, "nothing ran");
    }
}

#[test]
fn the_close_button_is_opt_in_or_set_for_every_dialog() {
    let (mut h, _, d) = harness();
    d.open(Confirm::new("Export?", "Export", GO));
    h.run();
    assert!(
        h.query_by_role_and_label(Role::Button, "Close").is_none(),
        "no close button by default"
    );
    assert_eq!(d.describe()["closable"], false);
    h.key_press(egui::Key::Escape);
    h.run();
    let store = h.state().handle().store();
    egui_pop_core::lock(&store)
        .set(Set::new(
            &format!("settings/{SETTING_CLOSABLE}"),
            Change::To(Value::Bool(true)),
            Origin::User,
        ))
        .unwrap();
    d.open(Confirm::new("Export?", "Export", GO));
    h.run();
    assert!(h.query_by_role_and_label(Role::Button, "Close").is_some());
}

/// A movable confirmation.
fn movable(title: &str) -> DialogSpec {
    DialogSpec::new(Confirm::new(title, "Export", GO)).movable(true)
}

/// Drags from a point by an amount, in four moves.
fn drag_by(h: &mut Harness<'static, App>, from: egui::Pos2, by: egui::Vec2) {
    h.drag_at(from);
    h.run();
    for i in 1..=4 {
        h.hover_at(from + by * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(from + by);
    h.run();
}

/// A double click in one frame.
fn double_click_at(h: &mut Harness<'static, App>, pos: egui::Pos2) {
    let events = &mut h.input_mut().events;
    events.push(egui::Event::PointerMoved(pos));
    for _ in 0..2 {
        for pressed in [true, false] {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    h.run();
}

fn offset(d: &dialogs::Dialogs) -> egui::Vec2 {
    d.offset().expect("a dialog is shown")
}

fn inside(h: &Harness<'static, App>, label: &str, size: egui::Vec2) -> bool {
    let r = h.get_by_label(label).rect();
    r.min.x >= 0.0 && r.min.y >= 0.0 && r.max.x <= size.x && r.max.y <= size.y
}

#[test]
fn a_movable_dialog_is_dragged_by_its_band_and_stays_inside_the_window() {
    let (mut h, runs, d) = harness();
    d.open(Confirm::new("Plain?", "Export", GO));
    h.run();
    // Not movable: dragging its title moves nothing.
    let title = h.get_by_label("Plain?").rect().center();
    drag_by(&mut h, title, egui::vec2(80.0, 40.0));
    assert_eq!(offset(&d), egui::Vec2::ZERO);
    assert_eq!(d.describe()["movable"], false);
    d.answer(dialogs::Answer::Cancel, Origin::User).unwrap();
    h.run();

    d.open(movable("Export?"));
    h.run();
    assert_eq!(d.describe()["movable"], true);
    let before = h.state().handle().last_event();
    let title = h.get_by_label("Export?").rect().center();
    drag_by(&mut h, title, egui::vec2(80.0, 40.0));
    let moved = offset(&d);
    assert!(moved.x > 40.0 && moved.y > 20.0, "{moved:?}");
    let after = h.get_by_label("Export?").rect().center();
    assert!(
        (after - title - moved).length() < 1.0,
        "the dialog followed"
    );
    let events = h.state().handle().events_since(before);
    let names: Vec<&str> = events.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"dialog.move"), "{names:?}");
    let done = events.iter().find(|e| e.name == "dialog.moved").unwrap();
    assert_eq!(done.data["how"], "drag");
    // Far away: kept inside the window.
    let size = egui::vec2(800.0, 600.0);
    let title = h.get_by_label("Export?").rect().center();
    drag_by(&mut h, title, egui::vec2(2000.0, 2000.0));
    h.run();
    assert!(inside(&h, "Export", size), "clamped");
    assert!(inside(&h, "Export?", size));
    // Again when the window shrinks.
    let small = egui::vec2(560.0, 400.0);
    h.set_size(small);
    h.run();
    h.run();
    assert!(inside(&h, "Export", small), "clamped again");
    // Moving answered nothing.
    assert_eq!(d.len(), 1);
    assert!(runs.borrow().is_empty());
}

#[test]
fn a_double_click_on_the_band_centres_the_dialog_again() {
    let (mut h, _, d) = harness();
    d.open(movable("Export?"));
    h.run();
    let centred = h.get_by_label("Export?").rect().center();
    drag_by(&mut h, centred, egui::vec2(-120.0, 60.0));
    assert_ne!(offset(&d), egui::Vec2::ZERO);
    let title = h.get_by_label("Export?").rect().center();
    let before = h.state().handle().last_event();
    double_click_at(&mut h, title);
    h.run();
    assert_eq!(offset(&d), egui::Vec2::ZERO);
    assert!((h.get_by_label("Export?").rect().center() - centred).length() < 1.0);
    let events = h.state().handle().events_since(before);
    let done = events.iter().find(|e| e.name == "dialog.moved").unwrap();
    assert_eq!(done.data["how"], "recentre");
}

#[test]
fn the_keyboard_moves_a_dialog_enter_keeps_the_place_escape_puts_it_back() {
    let (mut h, runs, d) = harness();
    d.open(movable("Export?"));
    h.run();
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::F7);
    h.run();
    assert_eq!(d.describe()["moving"], true);
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    h.key_press(egui::Key::ArrowUp);
    h.run();
    let step = egui_pop_core::theme::Tokens::of(&h.ctx).space.m;
    assert_eq!(offset(&d), egui::vec2(2.0 * step, -step));
    // Enter keeps the place: nothing is accepted.
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(d.describe()["moving"], false);
    assert_eq!(d.len(), 1);
    assert!(runs.borrow().is_empty());
    let kept = offset(&d);
    // Shift moves farther; Escape puts it back, the dialog stays.
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::F7);
    h.run();
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowDown);
    h.run();
    assert!(offset(&d).y > kept.y + step);
    h.key_press(egui::Key::Escape);
    h.run();
    h.run();
    assert_eq!(offset(&d), kept);
    assert_eq!(d.len(), 1, "Escape ended the move, not the dialog");
    // Escape again cancels the dialog.
    h.key_press(egui::Key::Escape);
    h.run();
    h.run();
    assert!(d.is_empty());
    // A dialog that is not movable is not moved by the keys.
    d.open(Confirm::new("Plain?", "Export", GO));
    h.run();
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::F7);
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert_eq!(offset(&d), egui::Vec2::ZERO);
    assert_eq!(d.describe()["moving"], false);
}

#[test]
fn a_dialog_reopens_centred_unless_it_keeps_its_place() {
    let (mut h, _, d) = harness();
    let open = |d: &dialogs::Dialogs, remember: bool| {
        let spec = movable("Levels");
        d.open(if remember {
            spec.remember_position("levels")
        } else {
            spec
        });
    };
    open(&d, true);
    h.run();
    let title = h.get_by_label("Levels").rect().center();
    drag_by(&mut h, title, egui::vec2(100.0, -60.0));
    let left = offset(&d);
    assert_ne!(left, egui::Vec2::ZERO);
    h.key_press(egui::Key::Escape);
    h.run();
    h.run();
    assert!(d.is_empty());
    // Kept in the host's store, by key.
    let store = h.state().handle().store();
    let kept = egui_pop_core::lock(&store)
        .get("ui/dialogs/levels")
        .map(|e| e.value.clone());
    assert_eq!(
        kept,
        Some(Value::Floats(vec![f64::from(left.x), f64::from(left.y)]))
    );
    open(&d, true);
    h.run();
    h.run();
    assert_eq!(offset(&d), left, "where it was left");
    assert_eq!(d.describe()["remember"], "levels");
    h.key_press(egui::Key::Escape);
    h.run();
    h.run();
    open(&d, false);
    h.run();
    assert_eq!(offset(&d), egui::Vec2::ZERO, "centred");
}

/// The dim the dialog shown asks of the overlay layer.
fn dim(h: &Harness<'static, App>) -> f32 {
    egui_pop_core::overlay::stack(&h.ctx)
        .last()
        .map(|s| s.dim)
        .expect("an overlay is open")
}

#[test]
fn a_backdrop_dims_or_stays_clear_and_a_click_on_it_never_closes() {
    let (mut h, runs, d) = harness();
    d.open(movable("Export?"));
    h.run();
    assert_eq!(dim(&h), 1.0, "dimmed by default");
    assert_eq!(d.describe()["backdrop"], "dim");
    // The dim fades while the dialog is dragged.
    let title = h.get_by_label("Export?").rect().center();
    h.drag_at(title);
    h.run();
    for i in 1..=3 {
        h.hover_at(title + egui::vec2(20.0 * i as f32, 0.0));
        h.run();
    }
    assert!(dim(&h) < 1.0, "faded: {}", dim(&h));
    h.drop_at(title + egui::vec2(60.0, 0.0));
    h.run();
    h.run();
    assert_eq!(dim(&h), 1.0, "back once dropped");
    // A click on the backdrop leaves the dialog open.
    h.event(egui::Event::PointerMoved(egui::pos2(8.0, 590.0)));
    h.event(egui::Event::PointerButton {
        pos: egui::pos2(8.0, 590.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.event(egui::Event::PointerButton {
        pos: egui::pos2(8.0, 590.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    h.run();
    assert_eq!(d.len(), 1);
    d.answer(dialogs::Answer::Cancel, Origin::User).unwrap();
    h.run();
    // Clear: modal still, nothing dimmed.
    d.open(DialogSpec::new(Confirm::new("Preview?", "Apply", GO)).backdrop(Backdrop::Clear));
    h.run();
    assert_eq!(dim(&h), 0.0);
    assert_eq!(d.describe()["backdrop"], "clear");
    assert!(egui_pop_core::overlay::modal_open(&h.ctx));
    h.event(egui::Event::PointerButton {
        pos: egui::pos2(8.0, 590.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.event(egui::Event::PointerButton {
        pos: egui::pos2(8.0, 590.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    h.run();
    assert_eq!(d.len(), 1, "a clear backdrop does not close it either");
    assert!(runs.borrow().is_empty());
}

#[test]
fn every_dialog_is_movable_with_the_setting() {
    let (mut h, _, d) = harness();
    let store = h.state().handle().store();
    egui_pop_core::lock(&store)
        .set(Set::new(
            &format!("settings/{SETTING_MOVABLE}"),
            Change::To(Value::Bool(true)),
            Origin::User,
        ))
        .unwrap();
    d.open(Confirm::new("Export?", "Export", GO));
    h.run();
    let title = h.get_by_label("Export?").rect().center();
    drag_by(&mut h, title, egui::vec2(80.0, 40.0));
    assert_ne!(offset(&d), egui::Vec2::ZERO);
}

/// Refuses moves by the keys, sends every drop to one place.
struct Pin;

impl Plugin for Pin {
    fn id(&self) -> PluginId {
        PluginId::from_static("pin")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_MOVE, |_, args| match args["how"].as_str() {
            Some("keys") => Before::Veto("not by the keys".into()),
            Some("drag") => {
                let mut a = args.clone();
                a["to"] = json!([10.0, 20.0]);
                Before::Redirect(a)
            }
            _ => Before::Go,
        });
        Ok(())
    }
}

#[test]
fn hooks_veto_or_redirect_a_move() {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Pin)
        .build(&egui::Context::default())
        .unwrap();
    let d = (*app.handle().service(&dialogs::DIALOGS).unwrap()).clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    d.open(movable("Export?"));
    h.run();
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::F7);
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert_eq!(offset(&d), egui::Vec2::ZERO, "vetoed");
    h.key_press(egui::Key::Enter);
    h.run();
    let title = h.get_by_label("Export?").rect().center();
    drag_by(&mut h, title, egui::vec2(150.0, 90.0));
    h.run();
    assert_eq!(offset(&d), egui::vec2(10.0, 20.0), "redirected");
}

/// Where every line of each text starting with one of `labels` begins,
/// as drawn in the last pass.
fn line_starts(h: &Harness<'_, App>, labels: &[&str]) -> Vec<(String, f32)> {
    fn walk(shape: &egui::Shape, labels: &[&str], out: &mut Vec<(String, f32)>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, labels, out)),
            egui::Shape::Text(t) => {
                let text = t.galley.text();
                if labels.iter().any(|l| text.starts_with(l)) {
                    for row in &t.galley.rows {
                        out.push((text.to_string(), t.pos.x + row.rect().min.x));
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in &h.output().shapes {
        walk(&c.shape, labels, &mut out);
    }
    out
}

#[test]
fn a_choice_s_rows_keep_their_size_under_the_pointer_and_line_up() {
    let (mut h, _, d) = harness();
    let long = "a hint long enough to wrap onto a second line inside the dialog, \
                and even onto a third one when the dialog is narrow";
    let labels = ["Studio", "Field", "Night", "Archive"];
    d.open(Choose::with_options(
        "Open a profile",
        vec![
            ChooseOption::new("Studio", json!(0)).hint(long),
            ChooseOption::new("Field", json!(1)).hint(long),
            ChooseOption::new("Night", json!(2)),
            ChooseOption::new("Archive", json!(3)).hint("short"),
        ],
        GO,
    ));
    for _ in 0..3 {
        h.run();
    }
    let dialog = |h: &Harness<'_, App>| {
        let rects = egui_pop_core::overlay::rects(&h.ctx);
        rects.last().map(|(_, r)| *r).unwrap()
    };
    let rows = |h: &Harness<'_, App>| -> Vec<egui::Rect> {
        labels
            .iter()
            .map(|l| h.get_by_label_contains(l).rect())
            .collect()
    };
    let rect = dialog(&h);
    let sizes: Vec<egui::Vec2> = rows(&h).iter().map(|r| r.size()).collect();
    assert!(sizes[1].y > sizes[2].y, "the long hint wraps: {sizes:?}");
    // Each row's text starts at the same x, wrapped lines included.
    let starts = line_starts(&h, &labels);
    assert!(starts.len() > labels.len(), "{starts:?}");
    let x = starts[0].1;
    for (text, sx) in &starts {
        assert!((sx - x).abs() < 0.5, "{text:?} starts at {sx}, not {x}");
    }
    // Hovering a row changes neither the dialog nor any row's size.
    for n in 0..labels.len() {
        let center = rows(&h)[n].center();
        h.hover_at(center);
        h.run();
        h.run();
        assert_eq!(dialog(&h), rect, "the dialog moved hovering {}", labels[n]);
        let now: Vec<egui::Vec2> = rows(&h).iter().map(|r| r.size()).collect();
        assert_eq!(now, sizes, "rows resized hovering {}", labels[n]);
        for (text, sx) in line_starts(&h, &labels) {
            assert!((sx - x).abs() < 0.5, "{text:?} starts at {sx}, not {x}");
        }
    }
}
