//! A host other than the default: commands answered later (from another
//! thread), menu items that wait or fail, the journal read since a
//! sequence number, the interface woken from another thread.

use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::app::MENU_FILE;
use egui_pop_api::host::{self, Dispatch, Host, Journal, Reply, Resolver, Waker};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::binding::SharedStore;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::{Value as Json, json};

const EXPORT: CommandId = CommandId::from_static("doc.export");
const LOCAL: CommandId = CommandId::from_static("doc.local");

type Waiting = Arc<Mutex<Vec<(CommandId, Json, Resolver)>>>;

/// Runs declared commands later: their resolvers wait in `waiting`.
/// In-process handlers still run now.
#[derive(Default)]
struct Later {
    journal: Journal,
    store: SharedStore,
    waker: Arc<Mutex<Waker>>,
    waiting: Waiting,
}

impl Host for Later {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        if let Some(r) = d.run_local() {
            return Reply::Done(r);
        }
        let (pending, resolver) = host::pending();
        self.waiting
            .lock()
            .unwrap()
            .push((d.command.clone(), d.args.clone(), resolver));
        self.journal
            .publish("host", Some(d.seq), "export.started", Json::Null);
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

    fn set_waker(&mut self, waker: Waker) {
        *self.waker.lock().unwrap() = waker;
    }
}

struct Doc;

impl Plugin for Doc {
    fn id(&self) -> PluginId {
        PluginId::from_static("doc")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.declare(CommandSpec::new(EXPORT, "Export", "File").menu(MENU_FILE, "5_doc", 0))?;
        ctx.command(CommandSpec::new(LOCAL, "Local", "File"), |_| {
            Ok(json!("here"))
        })?;
        Ok(())
    }
}

fn harness() -> (Harness<'static, App>, Waiting, Arc<Mutex<Waker>>) {
    let later = Later::default();
    let (waiting, waker) = (later.waiting.clone(), later.waker.clone());
    let app = App::builder()
        .host(later)
        .plugin(Doc)
        .build(&egui::Context::default())
        .unwrap();
    let h = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    (h, waiting, waker)
}

fn export_item<'a>(h: &'a Harness<'_, App>) -> egui_kittest::Node<'a> {
    h.query_all_by_label("Export").last().unwrap()
}

#[test]
fn a_command_answered_later_greys_its_menu_item_until_it_fails() {
    let (mut h, waiting, waker) = harness();
    h.run();
    h.get_by_label("File").click();
    h.run();
    export_item(&h).click();
    h.run();
    assert!(h.state().handle().running(&EXPORT));
    assert_eq!(waiting.lock().unwrap().len(), 1);
    h.get_by_label("File").click();
    h.run();
    assert!(
        export_item(&h).accesskit_node().is_disabled(),
        "running: greyed"
    );
    // The answer comes from another thread, which wakes the interface.
    let (_, _, resolver) = waiting.lock().unwrap().pop().unwrap();
    let wake = waker.lock().unwrap().clone();
    h.ctx.request_repaint(); // settle any earlier request
    h.run();
    std::thread::spawn(move || {
        resolver.resolve(Err("disk full".into()));
        wake.wake();
    })
    .join()
    .unwrap();
    assert!(h.ctx.has_requested_repaint(), "woken");
    h.run();
    let handle = h.state().handle().clone();
    assert!(!handle.running(&EXPORT));
    assert_eq!(
        handle.history().last().unwrap().result,
        Err("disk full".to_string())
    );
    // The menu, still open, offers it again.
    assert!(!export_item(&h).accesskit_node().is_disabled());
}

#[test]
fn the_journal_tells_what_happened_in_order_with_causes() {
    let (mut h, waiting, _) = harness();
    h.run();
    let handle = h.state().handle().clone();
    let start = handle.last_event();
    assert_eq!(
        h.state_mut().run(&LOCAL, Json::Null, Origin::User).now(),
        Some(Ok(json!("here")))
    );
    let reply = h
        .state_mut()
        .run(&EXPORT, json!({ "to": "a.png" }), Origin::Agent("t".into()));
    assert!(reply.now().is_none());
    let (_, args, resolver) = waiting.lock().unwrap().pop().unwrap();
    assert_eq!(args, json!({ "to": "a.png" }));
    resolver.resolve(Ok(json!({ "bytes": 3 })));
    assert_eq!(h.state_mut().settle(), 1);
    let events = handle.events_since(start);
    let names: Vec<(&str, &str)> = events
        .iter()
        .map(|e| (e.name.as_str(), e.source.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("command.dispatched", "user"),
            ("command.done", "user"),
            ("command.dispatched", "agent:t"),
            ("export.started", "host"),
            ("command.done", "agent:t"),
        ]
    );
    // A result names its dispatch as its cause.
    assert_eq!(events[1].cause, Some(events[0].seq));
    assert_eq!(events[4].cause, Some(events[2].seq));
    assert_eq!(events[4].data["result"], json!({ "bytes": 3 }));
    // Read since a sequence number, as tests and agents do.
    assert_eq!(handle.events_since(events[3].seq).len(), 1);
    let snapshot = handle.debug_snapshot();
    assert_eq!(snapshot["journal"]["last"], json!(events[4].seq));
}

#[test]
fn the_default_host_runs_declared_commands_nowhere_and_says_so() {
    let mut app = App::builder()
        .plugin(Doc)
        .build(&egui::Context::default())
        .unwrap();
    let out = app.run(&EXPORT, Json::Null, Origin::User).now();
    assert!(out.is_some_and(|r| r.is_err_and(|e| e.contains("no handler"))));
    assert_eq!(
        app.run(&LOCAL, Json::Null, Origin::User).now(),
        Some(Ok(json!("here")))
    );
}

/// A host journaling on its own bus: an event list it numbers itself,
/// from 100.
#[derive(Default)]
struct Bus {
    events: Arc<Mutex<Vec<host::Event>>>,
    store: SharedStore,
}

impl Host for Bus {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        Reply::Done(d.run_local().unwrap_or(Err("no handler".into())))
    }
    fn publish(&mut self, source: &str, cause: Option<u64>, name: &str, data: Json) -> Option<u64> {
        let mut e = self.events.lock().unwrap();
        let seq = 100 + e.len() as u64;
        e.push(host::Event {
            seq,
            source: source.into(),
            cause,
            name: name.into(),
            data,
        });
        Some(seq)
    }
    fn since(&self, seq: u64) -> Option<Vec<host::Event>> {
        Some(
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.seq > seq)
                .cloned()
                .collect(),
        )
    }
    fn last_seq(&self) -> Option<u64> {
        Some(self.events.lock().unwrap().last().map_or(0, |e| e.seq))
    }
    fn store(&self) -> SharedStore {
        self.store.clone()
    }
    fn set_waker(&mut self, _: Waker) {}
}

/// A host keeping no journal at all.
#[derive(Default)]
struct Bare {
    store: SharedStore,
}

impl Host for Bare {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        Reply::Done(d.run_local().unwrap_or(Err("no handler".into())))
    }
    fn store(&self) -> SharedStore {
        self.store.clone()
    }
    fn set_waker(&mut self, _: Waker) {}
}

#[test]
fn a_host_on_its_own_bus_answers_the_journal_and_a_bare_host_gets_one() {
    let bus = Bus::default();
    let events = bus.events.clone();
    let mut app = App::builder()
        .host(bus)
        .plugin(Doc)
        .build(&egui::Context::default())
        .unwrap();
    app.run(&LOCAL, Json::Null, Origin::User);
    let h = app.handle();
    // The bus numbers them; egui-pop reads it back through the host.
    assert_eq!(events.lock().unwrap().len(), 2);
    assert_eq!(h.last_event(), 101);
    let back = h.events_since(100);
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].name, "command.done");
    assert_eq!(back[0].cause, Some(100));

    let mut app = App::builder()
        .host(Bare::default())
        .plugin(Doc)
        .build(&egui::Context::default())
        .unwrap();
    app.run(&LOCAL, Json::Null, Origin::User);
    let h = app.handle();
    assert_eq!(h.last_event(), 2);
    let names: Vec<String> = h.events_since(0).into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["command.dispatched", "command.done"]);
}
