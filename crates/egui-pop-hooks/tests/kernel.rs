//! The kernel's behaviours: ordering, both emits, the error boundary,
//! panics, deadlines, failure sinks, introspection and removal.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use egui_pop_hooks::{
    DEFAULT_DEADLINE, FailureKind, Kernel, ListenOpts, PLUGIN_DEADLINE, Point, PointSpec, Refused,
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
