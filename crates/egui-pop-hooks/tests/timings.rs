//! The `Timings` capability: per-listener and per-pass times, the reset,
//! budgets reported (never failed), queued and deferred passes, timing
//! turned off. Lower bounds only: a loaded machine makes runs longer,
//! never shorter.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use egui_pop_hooks::bus_contract::Bus;
use egui_pop_hooks::{
    Capability, Kernel, ListenOpts, OverBudget, Point, PointDecl, PointKind, declare_all, offers,
};

struct Trail;
impl Point for Trail {
    const ID: &'static str = "test.trail";
    type Input = ();
    type Value = Vec<String>;
}

struct Ask;
impl Point for Ask {
    const ID: &'static str = "test.ask";
    type Input = ();
    type Value = Vec<String>;
    const KIND: PointKind = PointKind::Request;
}

const MS: Duration = Duration::from_millis(1);

/// A listener of `Trail` that sleeps `ms` and says its id.
fn sleeper(k: &Kernel, id: &'static str, ms: u64) {
    k.listen::<Trail>(id, ListenOpts::new().owner("test"), move |_, v| {
        thread::sleep(MS * ms as u32);
        v.push(id.to_owned());
        Ok(())
    })
    .unwrap();
}

/// Every pass over budget, as the sink heard it.
fn heard(k: &Kernel) -> Arc<Mutex<Vec<OverBudget>>> {
    let heard: Arc<Mutex<Vec<OverBudget>>> = Arc::default();
    let h = heard.clone();
    let timings = Bus::timings(k).expect("the kernel offers Timings");
    timings.on_over_budget(move |o| h.lock().unwrap().push(o.clone()));
    heard
}

#[test]
fn the_kernel_offers_timings_and_shows_budgets() {
    let k = Kernel::new();
    assert!(offers(&k, Capability::Timings));
    const TABLE: &[PointDecl] = &[PointDecl::of::<Trail>("A trail.").budget(MS)];
    assert!(declare_all(&k, TABLE).is_empty());
    let info = k.points();
    assert_eq!(info[0].budget, Some(MS));
    // Nothing ran yet: no timings.
    assert!(k.point_timings(Trail::ID).is_none());
    assert!(k.timings().is_empty());
}

#[test]
fn each_listener_and_each_pass_is_timed_until_the_reset() {
    let k = Kernel::new();
    sleeper(&k, "slow", 4);
    sleeper(&k, "quick", 0);
    for _ in 0..3 {
        k.alter::<Trail>((), Vec::new());
    }
    let t = k.point_timings(Trail::ID).unwrap();
    assert_eq!(t.point, Trail::ID);
    assert_eq!(t.budget, None);
    assert_eq!(t.passes.count, 3);
    assert!(t.passes.total >= MS * 12);
    assert!(t.passes.max >= MS * 4);
    assert!(t.passes.p95 <= t.passes.max);
    let ids: Vec<&str> = t.listeners.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, ["slow", "quick"], "in the order they run");
    assert_eq!(t.listeners[0].owner.as_deref(), Some("test"));
    assert_eq!(t.listeners[0].stats.count, 3);
    assert!(t.listeners[0].stats.p50 >= MS * 4 / 2, "within a bucket");
    assert_eq!(t.slowest()[0].id, "slow");
    assert_eq!(t.over_budget, 0);
    // Through the capability, the same.
    assert_eq!(Bus::timings(&k).unwrap().points(), vec![t]);

    k.reset_timings();
    assert!(k.point_timings(Trail::ID).is_none());
    k.alter::<Trail>((), Vec::new());
    assert_eq!(k.point_timings(Trail::ID).unwrap().passes.count, 1);
}

#[test]
fn a_pass_over_budget_is_reported_and_its_results_kept() {
    let k = Kernel::new();
    k.declare::<Trail>(egui_pop_hooks::PointSpec::new("A trail.").budget(MS))
        .unwrap();
    sleeper(&k, "quick", 0);
    sleeper(&k, "slow", 5);
    let heard = heard(&k);
    let out = k.alter::<Trail>((), Vec::new());
    assert!(out.failures.is_empty(), "a budget is not a deadline");
    assert_eq!(out.value, ["quick", "slow"]);
    let heard = heard.lock().unwrap();
    assert_eq!(heard.len(), 1);
    let o = &heard[0];
    assert_eq!(o.point, Trail::ID);
    assert_eq!(o.budget, MS);
    assert!(o.took >= MS * 5);
    assert_eq!(o.listeners.len(), 2);
    assert_eq!(o.slowest().unwrap().0, "slow");
    assert!(o.to_string().contains("slowest `slow`"));
    assert_eq!(k.point_timings(Trail::ID).unwrap().over_budget, 1);
}

#[test]
fn a_pass_within_budget_is_not_reported() {
    let k = Kernel::new();
    k.declare::<Trail>(egui_pop_hooks::PointSpec::new("A trail.").budget(Duration::from_secs(60)))
        .unwrap();
    sleeper(&k, "quick", 0);
    let heard = heard(&k);
    k.alter::<Trail>((), Vec::new());
    assert!(heard.lock().unwrap().is_empty());
    assert_eq!(k.point_timings(Trail::ID).unwrap().over_budget, 0);
}

#[test]
fn a_sink_that_panics_is_ignored() {
    let k = Kernel::new();
    k.declare::<Trail>(egui_pop_hooks::PointSpec::new("A trail.").budget(Duration::ZERO))
        .unwrap();
    sleeper(&k, "slow", 1);
    k.on_over_budget(|_| panic!("a broken sink"));
    let heard = heard(&k);
    let out = k.alter::<Trail>((), Vec::new());
    assert_eq!(out.value, ["slow"]);
    assert_eq!(
        heard.lock().unwrap().len(),
        1,
        "the next sink still hears it"
    );
}

#[test]
fn collects_queued_runs_and_drained_asks_are_passes_too() {
    let k = Arc::new(Kernel::new());
    k.declare::<Ask>(egui_pop_hooks::PointSpec::new("An ask.").budget(Duration::ZERO))
        .unwrap();
    k.listen_local::<Ask>("here", ListenOpts::new(), |_, v| {
        thread::sleep(MS);
        v.push("here".into());
        Ok(())
    })
    .unwrap();
    let heard = heard(&k);

    // A drained ask: one pass on this thread.
    k.ask::<Ask>((), Vec::new());
    assert_eq!(k.drain().len(), 1);
    assert_eq!(k.point_timings(Ask::ID).unwrap().passes.count, 1);

    // An emit on another thread: no listener ran there, no pass; the
    // queued run here is one.
    let other = k.clone();
    thread::spawn(move || {
        other.alter::<Ask>((), Vec::new());
    })
    .join()
    .unwrap();
    assert_eq!(k.point_timings(Ask::ID).unwrap().passes.count, 1);
    assert_eq!(k.run_pending().len(), 1);
    let t = k.point_timings(Ask::ID).unwrap();
    assert_eq!(t.passes.count, 2);
    assert_eq!(t.over_budget, 2);
    assert_eq!(heard.lock().unwrap().len(), 2);

    // A collect is timed like an alter.
    struct Parts;
    impl Point for Parts {
        const ID: &'static str = "test.parts";
        type Input = ();
        type Value = Vec<u8>;
    }
    k.listen::<Parts>("one", ListenOpts::new(), |_, v| {
        v.push(1);
        Ok(())
    })
    .unwrap();
    k.collect::<Parts>(());
    assert_eq!(
        k.point_timings(Parts::ID).unwrap().listeners[0].stats.count,
        1
    );
}

#[test]
fn a_threaded_listener_counts_the_time_the_emit_waited() {
    let k = Kernel::new();
    k.listen::<Trail>(
        "stuck",
        ListenOpts::new().threaded().deadline(MS * 20),
        |_, _| {
            thread::sleep(Duration::from_millis(300));
            Ok(())
        },
    )
    .unwrap();
    let out = k.alter::<Trail>((), Vec::new());
    assert_eq!(out.failures.len(), 1, "late");
    let t = k.point_timings(Trail::ID).unwrap();
    assert!(t.listeners[0].stats.max >= MS * 20);
}

#[test]
fn timing_off_counts_nothing() {
    let k = Kernel::new();
    k.declare::<Trail>(egui_pop_hooks::PointSpec::new("A trail.").budget(Duration::ZERO))
        .unwrap();
    sleeper(&k, "slow", 1);
    let heard = heard(&k);
    k.set_timing(false);
    assert!(!k.timing());
    let out = k.alter::<Trail>((), Vec::new());
    assert_eq!(out.value, ["slow"]);
    assert!(k.point_timings(Trail::ID).is_none());
    assert!(heard.lock().unwrap().is_empty());
    k.set_timing(true);
    k.alter::<Trail>((), Vec::new());
    assert_eq!(k.point_timings(Trail::ID).unwrap().passes.count, 1);
}
