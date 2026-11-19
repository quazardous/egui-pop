//! One journal per process (ADR 0030, slice 3): when the application's
//! bus offers the contract's journal, egui-pop's journal is that one.
//! Its events and the application's journaled facts share one numbered
//! sequence, agents' `wait` reads it, and egui-pop's application bus
//! still keeps the contract (bus-kernel's conformance suite).

#[path = "support/memory.rs"]
mod memory;

use std::sync::Arc;
use std::time::Duration;

use bus_kernel::conformance;
use egui_pop_api::about::{AboutPlugin, AppInfo};
use egui_pop_api::bus::bus_contract::{Bus, Point, PointDecl, PointId, PointKind, declare_all};
use egui_pop_api::bus::bus_contract_journal::{BusJournal, JOURNALED};
use egui_pop_api::bus::egui_pop_hooks::Kernel;
use egui_pop_api::host::{Dispatch, Host, Journal as HostJournal, Reply, Waker};
use egui_pop_api::wait::{WaitFor, Waited};
use egui_pop_api::{App, DefaultPlugins};
use serde_json::{Value as Json, json};

/// The application's own fact, journaled on its bus.
struct Imported;
impl Point for Imported {
    const ID: PointId = PointId::new("library.imported");
    const KIND: PointKind = PointKind::Event;
    type Input = String;
    type Value = ();
}

/// A bus with a journal, the application's fact declared on it.
fn journaled_bus() -> Arc<dyn Bus> {
    let bus: Arc<dyn Bus> = memory::full();
    assert!(
        declare_all(
            &*bus,
            &[PointDecl::of::<Imported>("A photo was imported.").facets(&[JOURNALED])]
        )
        .is_empty()
    );
    bus
}

fn app_on(bus: Arc<dyn Bus>) -> App {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(AboutPlugin::new(AppInfo::new("Demo", "1.0.0")))
        .bus(bus)
        .build(&egui::Context::default())
        .unwrap()
}

fn fact(bus: &Arc<dyn Bus>, path: &str) -> u64 {
    bus.journal()
        .expect("a journal")
        .publish::<Imported>(path.into())
        .unwrap()
}

#[test]
fn one_numbered_sequence_carries_egui_pops_events_and_the_apps_facts() {
    let bus = journaled_bus();
    let mut app = app_on(bus.clone());
    let h = app.handle().clone();
    let start = h.last_event();
    assert_eq!(start, bus.journal().unwrap().last_seq());

    let a = fact(&bus, "/a.raw");
    h.publish("test", None, "test.said", json!({ "n": 1 }));
    let f = app.debug_call("commands", "run", &json!({ "id": "about.read" }));
    assert!(f.ok, "{}", f.message);
    let b = fact(&bus, "/b.raw");

    let events = h.events_since(start);
    let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    let expected: Vec<u64> = (start + 1..=b).collect();
    assert_eq!(seqs, expected, "one sequence, no hole, no repeat");
    assert_eq!(h.last_event(), b);

    let first = &events[0];
    assert_eq!((first.seq, first.name.as_str()), (a, "library.imported"));
    assert_eq!(first.source, "bus");
    let said = events.iter().find(|e| e.name == "test.said").unwrap();
    assert_eq!(
        (said.source.as_str(), &said.data),
        ("test", &json!({ "n": 1 }))
    );
    let dispatched = events
        .iter()
        .find(|e| e.name == "command.dispatched")
        .unwrap();
    let done = events.iter().find(|e| e.name == "command.done").unwrap();
    assert_eq!(
        done.cause,
        Some(dispatched.seq),
        "a cause is a number of the same sequence"
    );
    assert!(a < said.seq && said.seq < done.seq && done.seq < b);

    // The bus's journal holds egui-pop's events as `pop.journal` entries.
    let kinds: Vec<String> = bus
        .journal()
        .unwrap()
        .since(start, usize::MAX)
        .entries
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(kinds.first().map(String::as_str), Some("library.imported"));
    assert_eq!(kinds.last().map(String::as_str), Some("library.imported"));
    assert!(kinds[1..kinds.len() - 1].iter().all(|k| k == "pop.journal"));
    assert!(bus.journal().unwrap().flush(Duration::from_secs(5)));
}

#[test]
fn wait_returns_at_the_right_entry_among_the_apps_facts() {
    let bus = journaled_bus();
    let mut app = app_on(bus.clone());
    let since = app.handle().last_event();
    fact(&bus, "/a.raw");
    let f = app.debug_call("commands", "run", &json!({ "id": "about.read" }));
    assert!(f.ok, "{}", f.message);
    fact(&bus, "/b.raw");

    // A command's result: the `command.done` entry, not a fact around it.
    let h = app.handle().clone();
    let Some(Waited::Event(done)) = h.wait_met(&WaitFor::event("command.done"), since) else {
        panic!("no command.done");
    };
    assert_eq!(done.data["command"], "about.read");
    let expected = h
        .events_since(since)
        .into_iter()
        .find(|e| e.name == "command.done")
        .unwrap();
    assert_eq!(done.seq, expected.seq);

    // An application fact is waited on by its point's id.
    let Some(Waited::Event(imported)) = h.wait_met(&WaitFor::event("library.imported"), done.seq)
    else {
        panic!("no fact after the command");
    };
    assert_eq!(imported.seq, h.last_event());

    // Through the bridge, as an agent asks it.
    let f = app.debug_call(
        "bridge",
        "wait",
        &json!({ "event": "command.done", "since": since }),
    );
    assert_eq!(f.data["met"], true, "{}", f.message);
    assert_eq!(f.data["event"]["seq"], done.seq);
    let f = app.debug_call(
        "bridge",
        "wait",
        &json!({ "event": "command.done", "since": h.last_event() }),
    );
    assert_eq!(f.data["met"], false, "nothing after the last entry");
}

/// A host keeping its own journal keeps it: egui-pop does not move its
/// events to the bus's.
#[derive(Default)]
struct OwnJournal {
    journal: HostJournal,
    store: egui_pop_core::binding::SharedStore,
}

impl Host for OwnJournal {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        Reply::Done(d.run_local().unwrap_or(Ok(Json::Null)))
    }
    fn journal(&self) -> Option<&HostJournal> {
        Some(&self.journal)
    }
    fn journal_mut(&mut self) -> Option<&mut HostJournal> {
        Some(&mut self.journal)
    }
    fn store(&self) -> egui_pop_core::binding::SharedStore {
        self.store.clone()
    }
    fn set_waker(&mut self, _: Waker) {}
}

#[test]
fn a_host_keeping_its_own_journal_keeps_it() {
    let bus = journaled_bus();
    let app = App::builder()
        .host(OwnJournal::default())
        .bus(bus.clone())
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle().clone();
    h.publish("test", None, "test.said", Json::Null);
    assert_eq!(bus.journal().unwrap().last_seq(), 0);
    assert_eq!(h.events_since(0).last().unwrap().name, "test.said");
}

#[test]
fn without_a_bus_journal_egui_pop_keeps_its_own() {
    let app = App::builder()
        .bus(Arc::new(Kernel::new()))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle().clone();
    h.publish("test", None, "test.said", Json::Null);
    assert_eq!(h.events_since(0).last().unwrap().name, "test.said");
    assert!(h.bus().points().iter().all(|p| p.id != "pop.journal"));
}

// --- bus-kernel's conformance suite on egui-pop's application bus ---

/// The bus of a full egui-pop application, built on `bus`; the
/// application is kept for the rest of the run (its listeners stay on
/// the bus, as in a running application).
fn app_bus(bus: Arc<dyn Bus>) -> Arc<dyn Bus> {
    std::mem::forget(app_on(bus.clone()));
    bus
}

#[test]
fn egui_pops_application_bus_keeps_the_contract() {
    conformance::run(|| app_bus(Arc::new(Kernel::new())));
}

#[test]
fn egui_pops_application_bus_with_a_journal_keeps_the_contract() {
    let failed: Vec<String> = conformance::report(|| app_bus(memory::full()))
        .into_iter()
        .filter_map(|(name, outcome)| match outcome {
            conformance::Outcome::Failed(why) => Some(format!("- {name}: {why}")),
            _ => None,
        })
        .collect();
    assert!(failed.is_empty(), "conformance:\n{}", failed.join("\n"));
}
