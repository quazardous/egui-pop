//! Toasts: shown without the focus, an action runs its command, they
//! expire by level, the queue is data.

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::toasts::{self, Toast};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::{Value as Json, json};

const REVEAL: CommandId = CommandId::from_static("files.reveal");

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
    // Closed by its button, or by an agent through the bridge.
    h.get_by_label("Close: Disk full").click();
    h.run();
    assert!(!on_screen(&h, "Disk full"));
    assert!(on_screen(&h, "Note 3"), "its turn came");
    let id = t.shown()[0].id;
    let f = h
        .state_mut()
        .debug_call("toasts", "dismiss", &json!({ "id": id }));
    assert!(f.ok, "{}", f.message);
    assert!(!t.shown().iter().any(|s| s.id == id));
    let snapshot = h.state().handle().debug_snapshot();
    assert_eq!(snapshot["toasts"]["history"].as_array().unwrap().len(), 3);
}
