//! Controls bound to a command the host answers later: the button and
//! the menu item grey and say they are busy until the result comes, a
//! failure shows on the button; the status bar's jobs item sums the
//! jobs up.

use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::app::MENU_FILE;
use egui_pop_api::busy::CommandButton;
use egui_pop_api::host::{self, Dispatch, Host, Journal, Reply, Resolver, Waker};
use egui_pop_api::jobs::JobsPlugin;
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::binding::SharedStore;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, PluginId};
use serde_json::{Value as Json, json};

const EXPORT: CommandId = CommandId::from_static("doc.export");

type Waiting = Arc<Mutex<Vec<Resolver>>>;

/// Answers every command later.
#[derive(Default)]
struct Later {
    journal: Journal,
    store: SharedStore,
    waiting: Waiting,
}

impl Host for Later {
    fn dispatch(&mut self, _: &Dispatch<'_>) -> Reply {
        let (pending, resolver) = host::pending();
        self.waiting.lock().unwrap().push(resolver);
        Reply::Later(pending)
    }

    fn journal(&self) -> Option<&Journal> {
        Some(&self.journal)
    }

    fn journal_mut(&mut self) -> Option<&mut Journal> {
        Some(&mut self.journal)
    }

    fn store(&self) -> SharedStore {
        self.store.clone()
    }

    fn set_waker(&mut self, _: Waker) {}
}

struct Doc;

impl Plugin for Doc {
    fn id(&self) -> PluginId {
        PluginId::from_static("doc")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.declare(CommandSpec::new(EXPORT, "Export", "File").menu(MENU_FILE, "5_doc", 0))?;
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("doc.toolbar"), 0, |ui, app| {
                ui.add(CommandButton::new(app, EXPORT).label("Export now"));
            }),
        );
        Ok(())
    }
}

fn harness() -> (Harness<'static, App>, Waiting) {
    let later = Later::default();
    let waiting = later.waiting.clone();
    let app = App::builder()
        .host(later)
        .plugin(Doc)
        .plugin(JobsPlugin::default())
        .build(&egui::Context::default())
        .unwrap();
    let h = Harness::builder()
        .with_size(egui::vec2(700.0, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    (h, waiting)
}

fn answer(waiting: &Waiting, result: Result<Json, String>) {
    for r in waiting.lock().unwrap().drain(..) {
        r.resolve(result.clone());
    }
}

#[test]
fn a_button_stays_busy_until_the_host_answers() {
    let (mut h, waiting) = harness();
    let clock = egui_pop_core::motion::freeze(&h.ctx, 0.0);
    h.run();
    h.get_by_label("Export now").click();
    h.run();
    assert!(h.state().handle().running(&EXPORT));
    assert_eq!(h.state().handle().running_since(&EXPORT), Some(0.0));
    let button = h.get_by_label("Export now");
    assert!(button.accesskit_node().is_disabled());
    assert!(button.accesskit_node().is_busy());
    clock.set(0.4);
    h.run();
    // Its name is kept while the spinner shows.
    assert!(h.query_by_label("Export now").is_some());
    answer(&waiting, Err("disk full".into()));
    h.run();
    assert!(!h.state().handle().running(&EXPORT));
    assert_eq!(h.state().handle().running_since(&EXPORT), None);
    let button = h.get_by_label("Export now");
    assert!(!button.accesskit_node().is_busy());
    assert!(!button.accesskit_node().is_disabled());
}

#[test]
fn a_menu_item_says_it_is_busy() {
    let (mut h, waiting) = harness();
    egui_pop_core::motion::freeze(&h.ctx, 0.0);
    h.run();
    h.get_by_label("File").click();
    h.run();
    h.query_all_by_label("Export").last().unwrap().click();
    h.run();
    h.get_by_label("File").click();
    h.run();
    let item = h.query_all_by_label("Export").last().unwrap();
    assert!(item.accesskit_node().is_busy());
    answer(&waiting, Ok(json!(null)));
    h.run();
    let item = h.query_all_by_label("Export").last().unwrap();
    assert!(!item.accesskit_node().is_busy());
}

#[test]
fn the_status_bar_sums_the_jobs_up() {
    let (mut h, _) = harness();
    h.run();
    assert!(
        h.query_by_label_contains("Jobs:").is_none(),
        "nothing to say"
    );
    let app = h.state().handle().clone();
    app.publish(
        "host",
        None,
        "job.started",
        json!({ "id": "a", "total": 4 }),
    );
    app.publish(
        "host",
        None,
        "job.progress",
        json!({ "id": "a", "done": 1 }),
    );
    h.run();
    assert!(h.query_by_label("Jobs: 1 (25%)").is_some());
    app.publish("host", None, "job.started", json!({ "id": "b" }));
    h.run();
    // One cannot estimate: no percentage.
    assert!(h.query_by_label("Jobs: 2").is_some());
    app.publish(
        "host",
        None,
        "job.failed",
        json!({ "id": "b", "error": "no" }),
    );
    app.publish("host", None, "job.finished", json!({ "id": "a" }));
    h.run();
    assert!(h.query_by_label("Failed: 1").is_some());
    assert!(h.query_by_label_contains("Jobs:").is_none());
}
