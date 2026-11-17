//! The kernel's behaviours: ordering, both emits, the error boundary,
//! panics, deadlines, failure sinks, introspection and removal;
//! refusals, fail-closed requests and deferred requests.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use egui_pop_hooks::bus_contract::Bus;
use egui_pop_hooks::{
    Capability, DEFAULT_DEADLINE, FailureKind, HookError, Kernel, ListenOpts, PLUGIN_DEADLINE,
    Point, PointDecl, PointSpec, Refused, declare_all, offers,
};

struct Trail;
impl Point for Trail {
    const ID: &'static str = "test.trail";
    type Input = ();
    type Value = Vec<String>;
}

struct Parts;
impl Point for Parts {
    const ID: &'static str = "test.parts";
    type Input = u32;
    type Value = Vec<u32>;
}

/// Same id as `Trail`, another type.
struct Impostor;
impl Point for Impostor {
    const ID: &'static str = "test.trail";
    type Input = ();
    type Value = u8;
}

fn push(kernel: &Kernel, id: &'static str, opts: ListenOpts) {
    kernel
        .listen::<Trail>(id, opts, move |_, v| {
            v.push(id.to_owned());
            Ok(())
        })
        .unwrap();
}

#[test]
fn listeners_run_by_constraints_then_priority_then_registration() {
    let k = Kernel::new();
    push(&k, "late", ListenOpts::new().after("core"));
    push(&k, "core", ListenOpts::new());
    push(&k, "first", ListenOpts::new().priority(5));
    push(&k, "guard", ListenOpts::new().before("core").priority(-9));
    push(&k, "tail", ListenOpts::new());
    let out = k.alter::<Trail>((), Vec::new());
    assert!(out.failures.is_empty());
    // `tail` outranks `guard`, which only has to precede `core`.
    assert_eq!(out.value, ["first", "tail", "guard", "core", "late"]);
    // Introspection shows the same order.
    let ids: Vec<_> = k
        .listeners(Trail::ID)
        .unwrap()
        .into_iter()
        .map(|l| l.id)
        .collect();
    assert_eq!(ids, out.value);
}

#[test]
fn a_cycle_is_refused_and_changes_nothing() {
    let k = Kernel::new();
    push(&k, "a", ListenOpts::new().after("b"));
    let refused = k
        .listen::<Trail>("b", ListenOpts::new().after("a"), |_, _| Ok(()))
        .unwrap_err();
    assert!(matches!(&refused, Refused::Cycle { path, .. } if path.len() == 3));
    assert!(refused.to_string().contains("cycle"));
    assert_eq!(k.listeners(Trail::ID).unwrap().len(), 1);
    // Itself as a target is a cycle too.
    assert!(matches!(
        k.listen::<Trail>("me", ListenOpts::new().before("me"), |_, _| Ok(())),
        Err(Refused::Cycle { .. })
    ));
}

#[test]
fn a_missing_target_is_reported_and_ignored() {
    let k = Kernel::new();
    push(&k, "a", ListenOpts::new().after("ghost").owner("m"));
    let out = k.alter::<Trail>((), Vec::new());
    assert_eq!(out.value, ["a"]);
    assert_eq!(out.failures.len(), 1);
    assert_eq!(
        out.failures[0].kind,
        FailureKind::MissingTarget("ghost".into())
    );
    assert_eq!(out.failures[0].owner.as_deref(), Some("m"));
    assert!(!out.failures[0].dropped());
    assert_eq!(k.listeners(Trail::ID).unwrap()[0].missing, ["ghost"]);
    // Once the target registers, the constraint holds.
    push(&k, "ghost", ListenOpts::new().priority(-1));
    let out = k.alter::<Trail>((), Vec::new());
    assert!(out.failures.is_empty());
    assert_eq!(out.value, ["ghost", "a"]);
}

#[test]
fn duplicate_ids_and_mismatched_types_are_refused() {
    let k = Kernel::new();
    push(&k, "a", ListenOpts::new());
    assert!(matches!(
        k.listen::<Trail>("a", ListenOpts::new(), |_, _| Ok(())),
        Err(Refused::Duplicate { .. })
    ));
    assert_eq!(
        k.listen::<Impostor>("x", ListenOpts::new(), |_, _| Ok(())),
        Err(Refused::TypeMismatch {
            point: "test.trail"
        })
    );
    assert_eq!(
        k.declare::<Impostor>(PointSpec::new("no")),
        Err(Refused::TypeMismatch {
            point: "test.trail"
        })
    );
    // Emitting with the wrong type runs nothing and says why.
    let out = k.alter::<Impostor>((), 7);
    assert_eq!(out.value, 7);
    assert_eq!(out.failures[0].kind, FailureKind::TypeMismatch);
    k.declare::<Trail>(PointSpec::new("trail")).unwrap();
    assert_eq!(
        k.declare::<Trail>(PointSpec::new("again")),
        Err(Refused::AlreadyDeclared {
            point: "test.trail"
        })
    );
}

#[test]
fn alter_keeps_only_the_edits_of_listeners_that_succeed() {
    let k = Kernel::new();
    push(&k, "one", ListenOpts::new());
    k.listen::<Trail>("fails", ListenOpts::new(), |_, v| {
        v.push("half".into());
        Err("refused".into())
    })
    .unwrap();
    k.listen::<Trail>("panics", ListenOpts::new(), |_, v| {
        v.clear();
        panic!("boom {}", 42);
    })
    .unwrap();
    push(&k, "two", ListenOpts::new());
    let out = k.alter::<Trail>((), vec!["start".into()]);
    assert_eq!(out.value, ["start", "one", "two"]);
    let kinds: Vec<_> = out.failures.iter().map(|f| f.kind.clone()).collect();
    assert_eq!(
        kinds,
        [
            FailureKind::Error("refused".into()),
            FailureKind::Panic("boom 42".into())
        ]
    );
    assert!(out.failures.iter().all(|f| f.dropped()));
}

#[test]
fn collect_gathers_each_successful_listener_part_in_order() {
    let k = Kernel::new();
    k.listen::<Parts>("double", ListenOpts::new(), |n, out| {
        out.push(n * 2);
        Ok(())
    })
    .unwrap();
    k.listen::<Parts>("partial", ListenOpts::new().priority(1), |n, out| {
        out.push(*n);
        Err("incomplete".into())
    })
    .unwrap();
    k.listen::<Parts>("panics", ListenOpts::new(), |_, _| panic!("no"))
        .unwrap();
    k.listen::<Parts>("square", ListenOpts::new(), |n, out| {
        // Each listener starts from a fresh value.
        assert!(out.is_empty());
        out.extend([n * n, 1]);
        Ok(())
    })
    .unwrap();
    let out = k.collect::<Parts>(3);
    assert_eq!(
        out.parts,
        [
            ("double".to_string(), vec![6]),
            ("square".to_string(), vec![9, 1])
        ]
    );
    let failed: Vec<_> = out.failures.iter().map(|f| f.listener.as_str()).collect();
    assert_eq!(failed, ["partial", "panics"]);
}

#[test]
fn an_inline_listener_past_its_deadline_counts_as_failed() {
    let k = Kernel::new();
    k.listen::<Trail>(
        "slow",
        ListenOpts::new().deadline(Duration::from_millis(5)),
        |_, v| {
            thread::sleep(Duration::from_millis(30));
            v.push("slow".into());
            Ok(())
        },
    )
    .unwrap();
    push(&k, "fast", ListenOpts::new());
    let out = k.alter::<Trail>((), Vec::new());
    assert_eq!(out.value, ["fast"]);
    match &out.failures[0].kind {
        FailureKind::Late {
            deadline,
            took: Some(took),
        } => {
            assert_eq!(*deadline, Duration::from_millis(5));
            assert!(*took >= Duration::from_millis(30));
        }
        other => panic!("not late: {other:?}"),
    }
}

#[test]
fn a_threaded_listener_is_abandoned_at_its_deadline() {
    let k = Kernel::new();
    k.declare::<Parts>(PointSpec::new("parts").deadline(Duration::from_millis(50)))
        .unwrap();
    k.listen::<Parts>("stuck", ListenOpts::new().threaded(), |_, out| {
        thread::sleep(Duration::from_secs(5));
        out.push(0);
        Ok(())
    })
    .unwrap();
    k.listen::<Parts>("quick", ListenOpts::new().threaded(), |n, out| {
        out.push(*n);
        Ok(())
    })
    .unwrap();
    k.listen::<Parts>("crash", ListenOpts::new().threaded(), |_, _| {
        panic!("in thread")
    })
    .unwrap();
    let start = Instant::now();
    let out = k.collect::<Parts>(4);
    // The emit waited for the point's deadline, not for the stuck thread.
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_eq!(out.parts, [("quick".to_string(), vec![4])]);
    assert_eq!(
        out.failures[0].kind,
        FailureKind::Late {
            deadline: Duration::from_millis(50),
            took: None
        }
    );
    assert_eq!(out.failures[1].kind, FailureKind::Panic("in thread".into()));
}

#[test]
fn deadlines_resolve_listener_then_point_then_kernel() {
    let k = Kernel::new();
    push(&k, "default", ListenOpts::new());
    push(&k, "plugin", ListenOpts::new().deadline(PLUGIN_DEADLINE));
    let ls = k.listeners(Trail::ID).unwrap();
    assert_eq!(ls[0].deadline, DEFAULT_DEADLINE);
    assert_eq!(ls[1].deadline, PLUGIN_DEADLINE);

    let k = Kernel::with_default_deadline(Duration::from_millis(200));
    k.declare::<Parts>(PointSpec::new("p").deadline(Duration::from_millis(300)))
        .unwrap();
    k.listen::<Parts>("a", ListenOpts::new(), |_, _| Ok(()))
        .unwrap();
    push(&k, "b", ListenOpts::new());
    assert_eq!(
        k.listeners(Parts::ID).unwrap()[0].deadline,
        Duration::from_millis(300)
    );
    assert_eq!(
        k.listeners(Trail::ID).unwrap()[0].deadline,
        Duration::from_millis(200)
    );
}

#[test]
fn sinks_receive_dropped_contributions_only() {
    let k = Kernel::new();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    k.on_failure(move |f| sink.lock().unwrap().push(f.to_string()));
    // A sink that panics does not stop the others or the emit.
    k.on_failure(|_| panic!("bad sink"));
    push(&k, "ok", ListenOpts::new().after("ghost"));
    k.listen::<Trail>("bad", ListenOpts::new().owner("plugin.x"), |_, _| {
        Err("no disk".into())
    })
    .unwrap();
    let out = k.alter::<Trail>((), Vec::new());
    assert_eq!(out.failures.len(), 2);
    assert_eq!(
        *seen.lock().unwrap(),
        ["hook `test.trail`, listener `bad`: failed: no disk"]
    );
}

#[test]
fn introspection_lists_points_listeners_and_counters() {
    let k = Kernel::new();
    k.declare::<Parts>(PointSpec::new("Numbers, one list per listener"))
        .unwrap();
    k.listen::<Parts>(
        "flaky",
        ListenOpts::new().owner("mod.a").before("other").priority(3),
        |n, _| if *n == 0 { Err("zero".into()) } else { Ok(()) },
    )
    .unwrap();
    push(&k, "only", ListenOpts::new());
    k.collect::<Parts>(1);
    k.collect::<Parts>(0);

    let points = k.points();
    let ids: Vec<_> = points.iter().map(|p| p.id).collect();
    assert_eq!(ids, ["test.parts", "test.trail"]);
    let parts = &points[0];
    assert!(parts.declared);
    assert_eq!(parts.description, "Numbers, one list per listener");
    let flaky = &parts.listeners[0];
    assert_eq!(flaky.id, "flaky");
    assert_eq!(flaky.owner.as_deref(), Some("mod.a"));
    assert_eq!(flaky.before, ["other"]);
    assert_eq!(flaky.priority, 3);
    assert_eq!(flaky.missing, ["other"]);
    assert_eq!((flaky.runs, flaky.failures), (2, 1));
    assert!(flaky.last_failure.as_deref().unwrap().contains("zero"));
    assert!(!points[1].declared);
    assert!(k.listeners("nobody").is_none());
}

#[test]
fn forget_and_unlisten_remove_listeners() {
    let k = Kernel::new();
    push(&k, "a", ListenOpts::new().owner("plugin"));
    push(&k, "b", ListenOpts::new());
    k.listen::<Parts>("c", ListenOpts::new().owner("plugin"), |_, _| Ok(()))
        .unwrap();
    assert_eq!(k.forget("plugin"), 2);
    assert!(k.unlisten(Trail::ID, "b"));
    assert!(!k.unlisten(Trail::ID, "b"));
    assert!(k.alter::<Trail>((), Vec::new()).value.is_empty());
    assert!(k.collect::<Parts>(1).parts.is_empty());
}

#[test]
fn listeners_may_reenter_the_kernel_and_emits_cross_threads() {
    let k = Arc::new(Kernel::new());
    k.listen::<Parts>("n", ListenOpts::new(), |n, out| {
        out.push(*n);
        Ok(())
    })
    .unwrap();
    let inner = k.clone();
    k.listen::<Trail>("reenter", ListenOpts::new(), move |_, v| {
        // Emit and listen from inside a listener: no lock is held.
        let got = inner.collect::<Parts>(9);
        v.push(format!("{:?}", got.parts[0].1));
        inner
            .listen::<Parts>("added", ListenOpts::new(), |_, _| Ok(()))
            .map_err(|e| e.to_string().into())
    })
    .unwrap();
    let out = k.alter::<Trail>((), Vec::new());
    assert!(out.failures.is_empty(), "{:?}", out.failures);
    assert_eq!(out.value, ["[9]"]);

    let handles: Vec<_> = (0..4)
        .map(|i| {
            let k = k.clone();
            thread::spawn(move || k.collect::<Parts>(i).parts.len())
        })
        .collect();
    for h in handles {
        assert_eq!(h.join().unwrap(), 2);
    }
}

#[test]
fn a_last_listener_runs_after_every_other_one_and_is_alone() {
    let kernel = Kernel::new();
    push(&kernel, "check", ListenOpts::new().last());
    push(&kernel, "early", ListenOpts::new().priority(10));
    // Registered after it, ordered against nothing: still before it.
    push(
        &kernel,
        "late",
        ListenOpts::new().after("early").priority(-10),
    );
    let out = kernel.alter::<Trail>((), Vec::new());
    assert_eq!(out.value, ["early", "late", "check"]);
    assert!(out.failures.is_empty(), "LAST is no missing target");
    let listeners = kernel.listeners(Trail::ID).unwrap();
    assert!(listeners[2].after.iter().any(|a| a == egui_pop_hooks::LAST));
    assert!(ListenOpts::new().last().last().is_last());
    // A second last one: each would run after the other.
    let refused = kernel
        .listen::<Trail>("check-too", ListenOpts::new().last(), |_, _| Ok(()))
        .unwrap_err();
    assert!(matches!(refused, Refused::Cycle { .. }), "{refused:?}");
    assert_eq!(kernel.listeners(Trail::ID).unwrap().len(), 3);
}

/// The kernel as an application hands it to a library: an
/// `Arc<dyn Bus>` (ADR 0030). Threaded deadlines, the error boundary and
/// type checks hold through the contract's erased methods.
#[test]
fn the_kernel_through_the_contract() {
    use egui_pop_hooks::bus_contract::{Bus, PointKind};
    let bus: Arc<dyn Bus> = Arc::new(Kernel::new());
    bus.declare::<Trail>(PointSpec::new("a trail")).unwrap();
    bus.listen::<Trail>("b", ListenOpts::new().after("a"), |_, v| {
        v.push("b".into());
        Ok(())
    })
    .unwrap();
    bus.listen::<Trail>("a", ListenOpts::new(), |_, v| {
        v.push("a".into());
        Ok(())
    })
    .unwrap();
    bus.listen::<Trail>(
        "slow",
        ListenOpts::new()
            .last()
            .threaded()
            .deadline(Duration::from_millis(20)),
        |_, v| {
            thread::sleep(Duration::from_millis(500));
            v.push("slow".into());
            Ok(())
        },
    )
    .unwrap();
    let out = bus.alter::<Trail>((), vec![]);
    assert_eq!(out.value, ["a", "b"]);
    assert!(matches!(
        out.failures[0].kind,
        FailureKind::Late { took: None, .. }
    ));
    assert_eq!(bus.points()[0].kind, PointKind::Alter);
    assert!(matches!(
        bus.listen::<Impostor>("x", ListenOpts::new(), |_, _| Ok(())),
        Err(Refused::TypeMismatch { .. })
    ));
    bus.listen::<Parts>("double", ListenOpts::new(), |n, v| {
        v.push(n * 2);
        Ok(())
    })
    .unwrap();
    assert_eq!(
        bus.collect::<Parts>(4).parts,
        [("double".to_string(), vec![8])]
    );
}

/// A local listener (state that is not `Send`) takes its place among the
/// others: run inline on its thread, queued by an emit from another
/// thread and run in its order at the thread's `run_pending`.
#[test]
fn local_listeners_run_in_order_on_their_thread() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let k = Arc::new(Kernel::new());
    let seen: Rc<RefCell<Vec<Vec<String>>>> = Rc::default();
    push(&k, "core", ListenOpts::new());
    let mine = seen.clone();
    k.listen_local::<Trail>(
        "ui.panel",
        ListenOpts::new().owner("ui").after("core"),
        move |_, v| {
            mine.borrow_mut().push(v.clone());
            v.push("ui.panel".into());
            Ok(())
        },
    )
    .unwrap();
    push(&k, "tail", ListenOpts::new().after("ui.panel"));
    let woken = Arc::new(Mutex::new(Vec::new()));
    let w = woken.clone();
    k.on_pending(move |t| w.lock().unwrap().push(t));

    let out = k.alter::<Trail>((), Vec::new());
    assert_eq!(out.value, ["core", "ui.panel", "tail"]);
    assert!(out.deferred.is_empty());
    assert!(k.run_pending().is_empty());

    let other = k.clone();
    let out = thread::spawn(move || other.alter::<Trail>((), Vec::new()))
        .join()
        .unwrap();
    assert_eq!(out.value, ["core", "tail"]);
    assert_eq!(out.deferred, ["ui.panel"]);
    assert_eq!(*woken.lock().unwrap(), [thread::current().id()]);
    assert_eq!(seen.borrow().len(), 1);

    let ran = k.run_pending();
    assert_eq!(ran.len(), 1);
    assert_eq!(ran[0].ran, ["ui.panel"]);
    assert!(ran[0].failures.is_empty());
    assert_eq!(seen.borrow()[1], ["core", "tail"]);
    assert!(k.run_pending().is_empty());

    let infos = k.listeners(Trail::ID).unwrap();
    assert!(infos[1].local && infos[1].runs == 2);
    assert!(
        egui_pop_hooks::order_dump(&*k, &[Trail::ID])
            .contains("ui.panel (owner ui): local; after core")
    );
    assert_eq!(
        k.listen_local::<Trail>("ui.slow", ListenOpts::new().threaded(), |_, _| Ok(())),
        Err(Refused::Threaded {
            point: Trail::ID,
            listener: "ui.slow".into()
        })
    );
}

/// A failing or panicking local listener is dropped like any other, on
/// its thread and when run from the queue.
#[test]
fn local_listeners_keep_the_error_boundary() {
    let k = Arc::new(Kernel::new());
    k.listen_local::<Trail>("ui.broken", ListenOpts::new(), |_, v| {
        v.push("half".into());
        panic!("boom")
    })
    .unwrap();
    push(&k, "core", ListenOpts::new());
    let out = k.alter::<Trail>((), Vec::new());
    assert_eq!(out.value, ["core"]);
    assert!(matches!(&out.failures[0].kind, FailureKind::Panic(m) if m == "boom"));

    let other = k.clone();
    thread::spawn(move || other.alter::<Trail>((), Vec::new()))
        .join()
        .unwrap();
    let ran = k.run_pending();
    assert_eq!(ran[0].ran, ["ui.broken"]);
    assert_eq!(ran[0].failures.len(), 1);
}

/// `forget`, from any thread, removes local listeners, queued runs
/// included; their closures are dropped on their thread.
#[test]
fn forget_removes_local_listeners_from_any_thread() {
    use std::rc::Rc;

    let k = Arc::new(Kernel::new());
    let held = Rc::new(());
    let mine = held.clone();
    k.listen_local::<Trail>("ui.panel", ListenOpts::new().owner("ui"), move |_, v| {
        let _ = &mine;
        v.push("ui.panel".into());
        Ok(())
    })
    .unwrap();
    assert_eq!(Rc::strong_count(&held), 2);
    let other = k.clone();
    let (deferred, forgot) = thread::spawn(move || {
        let out = other.alter::<Trail>((), Vec::new());
        (out.deferred, other.forget("ui"))
    })
    .join()
    .unwrap();
    assert_eq!(deferred, ["ui.panel"]);
    assert_eq!(forgot, 1);
    let ran = k.run_pending();
    assert!(ran[0].ran.is_empty(), "removed since the emit: not run");
    assert_eq!(Rc::strong_count(&held), 1, "its closure dropped here");
    assert!(k.listeners(Trail::ID).unwrap().is_empty());

    // Removed on its own thread: dropped at once.
    let mine = held.clone();
    k.listen_local::<Trail>("ui.again", ListenOpts::new(), move |_, _| {
        let _ = &mine;
        Ok(())
    })
    .unwrap();
    assert!(k.unlisten(Trail::ID, "ui.again"));
    assert_eq!(Rc::strong_count(&held), 1);
}

/// A collect from another thread: each queued local listener fills a
/// fresh value on its thread.
#[test]
fn a_deferred_collect_fills_fresh_values() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let k = Arc::new(Kernel::new());
    let got: Rc<RefCell<Vec<Vec<u32>>>> = Rc::default();
    let mine = got.clone();
    k.listen_local::<Parts>("ui.badge", ListenOpts::new(), move |n, out| {
        out.push(*n);
        mine.borrow_mut().push(out.clone());
        Ok(())
    })
    .unwrap();
    k.listen::<Parts>("disk", ListenOpts::new(), |n, out| {
        out.push(n + 1);
        Ok(())
    })
    .unwrap();
    let other = k.clone();
    let out = thread::spawn(move || other.collect::<Parts>(7))
        .join()
        .unwrap();
    assert_eq!(out.parts, [("disk".to_string(), vec![8])]);
    assert_eq!(out.deferred, ["ui.badge"]);
    assert_eq!(k.run_pending()[0].ran, ["ui.badge"]);
    assert_eq!(*got.borrow(), [vec![7]]);
}

/// A request whose listeners may refuse it.
struct Run;
impl Point for Run {
    const ID: &'static str = "test.run";
    const KIND: egui_pop_hooks::PointKind = egui_pop_hooks::PointKind::Request;
    type Input = String;
    type Value = Vec<String>;
}

/// `policy` (answering with `policy`), then `handler`.
fn guarded(k: &Kernel, policy: impl Fn(&str) -> Result<(), HookError> + Send + Sync + 'static) {
    k.listen::<Run>(
        "policy",
        ListenOpts::new().owner("policy"),
        move |cmd, did| {
            did.push("policy".into());
            policy(cmd)
        },
    )
    .unwrap();
    k.listen::<Run>("handler", ListenOpts::new().after("policy"), |cmd, did| {
        did.push(format!("ran {cmd}"));
        Ok(())
    })
    .unwrap();
}

#[test]
fn a_refusal_stops_a_request_and_is_not_a_failure() {
    let k = Kernel::new();
    guarded(&k, |cmd| {
        if cmd == "delete" {
            Err(HookError::refuse("agents may not delete"))
        } else {
            Ok(())
        }
    });
    let sunk = Arc::new(Mutex::new(0));
    let s = sunk.clone();
    k.on_failure(move |_| *s.lock().unwrap() += 1);

    assert_eq!(
        k.alter::<Run>("open".into(), vec![]).value,
        ["policy", "ran open"]
    );
    let out = k.alter::<Run>("delete".into(), vec![]);
    assert!(out.value.is_empty(), "the refusing edit is dropped too");
    assert!(out.failures.is_empty());
    let refusal = out.refused.unwrap();
    assert_eq!(
        (refusal.listener.as_str(), refusal.owner.as_deref()),
        ("policy", Some("policy"))
    );
    assert_eq!(
        refusal.to_string(),
        "hook `test.run`: refused by `policy`: agents may not delete"
    );
    assert_eq!(*sunk.lock().unwrap(), 0);
    let info = k.listeners("test.run").unwrap();
    assert_eq!((info[0].runs, info[0].failures), (2, 0));
    assert_eq!(info[1].runs, 1, "the handler ran once, not for the refusal");

    // A collect on a request stops the same way.
    let out = k.collect::<Run>("delete".into());
    assert!(out.parts.is_empty());
    assert_eq!(out.refused.unwrap().reason, "agents may not delete");
}

#[test]
fn a_refusal_off_a_request_counts_as_a_failure() {
    let k = Kernel::new();
    k.listen::<Trail>("picky", ListenOpts::new(), |_, _| {
        Err(HookError::refuse("no"))
    })
    .unwrap();
    push(&k, "after", ListenOpts::new().after("picky"));
    let out = k.alter::<Trail>((), vec![]);
    assert_eq!(out.value, ["after"]);
    assert!(out.refused.is_none());
    assert_eq!(out.failures[0].kind, FailureKind::Error("no".into()));
}

#[test]
fn a_fail_closed_request_refuses_on_an_error_a_panic_or_a_missed_deadline() {
    // Open: the failing policy is dropped and the handler runs.
    let k = Kernel::new();
    guarded(&k, |_| Err("policy table unreadable".into()));
    let out = k.alter::<Run>("delete".into(), vec![]);
    assert_eq!(out.value, ["ran delete"]);
    assert!(out.refused.is_none());

    // Closed: an error.
    let k = Kernel::new();
    k.declare::<Run>(PointSpec::new("runs").fail_closed())
        .unwrap();
    guarded(&k, |_| Err("policy table unreadable".into()));
    let out = k.alter::<Run>("delete".into(), vec![]);
    assert!(out.value.is_empty(), "the handler did not run");
    assert_eq!(out.failures.len(), 1, "the failure is still reported");
    let refusal = out.refused.unwrap();
    assert!(refusal.failed);
    assert_eq!(refusal.reason, "policy table unreadable");
    assert!(k.points()[0].fail_closed);

    // Closed: a panic.
    let k = Kernel::new();
    k.declare::<Run>(PointSpec::new("runs").fail_closed())
        .unwrap();
    guarded(&k, |_| panic!("policy bug"));
    let out = k.alter::<Run>("x".into(), vec![]);
    assert!(out.value.is_empty());
    assert_eq!(out.refused.unwrap().reason, "policy bug");

    // Closed: a threaded listener past its deadline.
    let k = Kernel::new();
    k.declare::<Run>(PointSpec::new("runs").fail_closed())
        .unwrap();
    k.listen::<Run>(
        "policy",
        ListenOpts::new()
            .threaded()
            .deadline(Duration::from_millis(20)),
        |_, _| {
            thread::sleep(Duration::from_secs(2));
            Ok(())
        },
    )
    .unwrap();
    k.listen::<Run>("handler", ListenOpts::new().after("policy"), |_, did| {
        did.push("ran".into());
        Ok(())
    })
    .unwrap();
    let start = Instant::now();
    let out = k.alter::<Run>("x".into(), vec![]);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(out.value.is_empty());
    let refusal = out.refused.unwrap();
    assert!(refusal.failed);
    assert!(matches!(
        out.failures[0].kind,
        FailureKind::Late { took: None, .. }
    ));
}

#[test]
fn a_refused_request_queues_nothing_and_a_queued_refusal_stops_the_rest() {
    let k = Arc::new(Kernel::new());
    k.listen_local::<Run>("ui.confirm", ListenOpts::new(), |cmd, did| {
        if cmd == "delete" {
            return Err(HookError::refuse("the user said no"));
        }
        did.push("confirmed".into());
        Ok(())
    })
    .unwrap();
    k.listen_local::<Run>(
        "ui.after",
        ListenOpts::new().after("ui.confirm"),
        |_, did| {
            did.push("after".into());
            Ok(())
        },
    )
    .unwrap();
    k.listen::<Run>("gate", ListenOpts::new().after("ui.after"), |cmd, _| {
        if cmd == "format" {
            Err(HookError::refuse("never"))
        } else {
            Ok(())
        }
    })
    .unwrap();

    let other = k.clone();
    let out = thread::spawn(move || other.alter::<Run>("format".into(), vec![]))
        .join()
        .unwrap();
    assert_eq!(out.refused.unwrap().listener, "gate");
    assert!(out.deferred.is_empty());
    assert!(k.run_pending().is_empty());

    let other = k.clone();
    let out = thread::spawn(move || other.alter::<Run>("delete".into(), vec![]))
        .join()
        .unwrap();
    assert_eq!(out.deferred, ["ui.confirm", "ui.after"]);
    let ran = k.run_pending();
    assert_eq!(ran[0].ran, ["ui.confirm"]);
    assert_eq!(ran[0].refused.as_ref().unwrap().reason, "the user said no");
    assert_eq!(ran[0].ask, None);
}

#[test]
fn the_kernel_offers_defer() {
    let k = Kernel::new();
    let bus: &dyn Bus = &k;
    assert!(offers(bus, Capability::Defer));
    const NEEDS: &[PointDecl] = &[PointDecl::of::<Run>("runs").needs(&[Capability::Defer])];
    assert!(declare_all(bus, NEEDS).is_empty());
}

#[test]
fn asks_run_at_the_drain_in_order_and_an_ask_while_draining_waits() {
    let k = Arc::new(Kernel::new());
    k.declare::<Run>(PointSpec::new("writes")).unwrap();
    let heard: Arc<Mutex<Vec<String>>> = Arc::default();
    let h = heard.clone();
    let again = Arc::downgrade(&k);
    k.listen::<Run>("persist", ListenOpts::new(), move |what, did| {
        match what.as_str() {
            "locked" => return Err(HookError::refuse("read-only")),
            "broken" => return Err("disk full".into()),
            _ => {}
        }
        h.lock().unwrap().push(what.clone());
        did.push(format!("wrote {what}"));
        if what == "rename"
            && let Some(k) = again.upgrade()
        {
            k.ask::<Run>("reindex".into(), vec![]);
        }
        Ok(())
    })
    .unwrap();
    let woken = Arc::new(Mutex::new(Vec::new()));
    let w = woken.clone();
    k.on_pending(move |t| w.lock().unwrap().push(t));

    let first = k.ask::<Run>("rename".into(), vec![]);
    let second = k.ask::<Run>("locked".into(), vec![]);
    k.ask::<Run>("broken".into(), vec![]);
    assert!(second > first);
    assert_eq!(woken.lock().unwrap().len(), 3);
    assert_eq!(woken.lock().unwrap()[0], thread::current().id());
    assert_eq!(k.waiting(), 3);
    assert!(heard.lock().unwrap().is_empty());
    assert!(k.run_pending().is_empty(), "asks wait for the drain");

    // An immediate emit after the asks runs first.
    k.alter::<Run>("now".into(), vec![]);

    let ran = k.drain();
    assert_eq!(ran.len(), 3);
    assert_eq!(
        (ran[0].ask, ran[0].ran.as_slice()),
        (Some(first), &["persist".to_string()][..])
    );
    assert_eq!(ran[1].refused.as_ref().unwrap().reason, "read-only");
    assert_eq!(
        ran[2].failures[0].kind,
        FailureKind::Error("disk full".into())
    );
    assert_eq!(*heard.lock().unwrap(), ["now", "rename"]);

    assert_eq!(k.waiting(), 1, "the ask made while draining waits");
    let bus: &dyn Bus = &*k;
    assert_eq!(bus.defer().unwrap().drain().len(), 1);
    assert_eq!(*heard.lock().unwrap(), ["now", "rename", "reindex"]);
    assert!(k.drain().is_empty());
}

#[test]
fn asks_belong_to_the_asking_thread() {
    let k = Arc::new(Kernel::new());
    let heard: Arc<Mutex<Vec<String>>> = Arc::default();
    let h = heard.clone();
    k.listen::<Run>("persist", ListenOpts::new(), move |what, _| {
        h.lock().unwrap().push(what.clone());
        Ok(())
    })
    .unwrap();
    k.ask::<Run>("here".into(), vec![]);
    let other = k.clone();
    let there = thread::spawn(move || {
        other.ask::<Run>("there".into(), vec![]);
        (other.waiting(), other.drain().len())
    })
    .join()
    .unwrap();
    assert_eq!(there, (1, 1));
    assert_eq!(*heard.lock().unwrap(), ["there"]);
    assert_eq!(k.drain().len(), 1);
    assert_eq!(*heard.lock().unwrap(), ["there", "here"]);
}
