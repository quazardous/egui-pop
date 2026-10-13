//! Dialogs: confirmations (verbs, danger), forms from descriptors with
//! the in-app path picker, choices, answers from an agent, the close
//! button.

use std::cell::RefCell;
use std::rc::Rc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::dialogs::{
    self, Choose, ChooseOption, Confirm, DialogSpec, Form, Info, SETTING_CLOSABLE, STEP_ANSWER,
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
