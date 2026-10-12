//! Dialogs: confirmations (verbs, danger), forms from descriptors with
//! the in-app path picker, choices, answers from an agent.

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::dialogs::{self, Choose, Confirm, Form, Info};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use egui_pop_model::values::{Descriptor, PathKind};
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
    h.run();
    // Folders first, then the files the field accepts.
    assert!(h.query_by_label("sub/").is_some());
    assert!(h.query_by_label("photo.png").is_none());
    h.get_by_label("notes.txt").click();
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
    assert_eq!(snapshot["dialogs"]["current"]["options"][1], "Right");
    let feedback = h.state_mut().debug_call(
        "dialogs",
        "answer",
        &json!({ "answer": "choose", "index": 1 }),
    );
    assert!(feedback.ok, "{}", feedback.message);
    h.run();
    h.run();
    let (args, origin) = runs.borrow()[0].clone();
    assert_eq!(args, json!({ "choice": 1, "label": "Right" }));
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
