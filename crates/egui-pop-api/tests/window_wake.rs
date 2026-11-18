//! A local listener registered for a window (`AppHandle::listen_local_in`)
//! wakes that window when it runs: a fact heard for a detached window
//! draws it within one frame, without the main window redrawing it by
//! hand. The main window is still woken for what it runs (the local
//! listeners queued by other threads, the requests asked for after the
//! drawing): that guarantee is kept.
//!
//! kittest embeds the windows in the main one, so these tests drive an
//! egui context with real viewports, as eframe does
//! (`egui_pop_test::windows::MultiWindow`), and run the passes the
//! application asks for, as an idle event loop does.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui::ViewportId;
use egui_pop_api::App;
use egui_pop_api::bus::bus_contract::{self, ListenOpts, Point, PointDecl, PointKind};
use egui_pop_api::windows::{WindowId, WindowSpec};
use egui_pop_test::windows::MultiWindow;

/// A fact of the test's own: something a window shows changed.
#[derive(Debug)]
struct Changed;
impl Point for Changed {
    const KIND: PointKind = PointKind::Alter;
    const ID: &'static str = "test.changed";
    type Input = String;
    type Value = ();
}

/// A request of the test's own, asked for after the drawing.
#[derive(Debug)]
struct Note;
impl Point for Note {
    const KIND: PointKind = PointKind::Request;
    const ID: &'static str = "test.note";
    type Input = String;
    type Value = Vec<String>;
}

type Heard = Rc<RefCell<Vec<String>>>;

/// The idle loop's longest run: more means a window wakes itself.
const SETTLE: usize = 40;

struct Fixture {
    m: MultiWindow,
    /// The window `w`'s passes.
    passes: Rc<Cell<u32>>,
    /// The window `w` asks `test.note` in its next pass.
    asking: Rc<Cell<bool>>,
    heard: Heard,
}

impl Fixture {
    fn w() -> WindowId {
        WindowId::new("w")
    }

    fn viewport(&self) -> ViewportId {
        self.m.app().window_viewport(&Self::w()).expect("w is open")
    }

    /// Runs what the application asks for until it is idle.
    fn settle(&mut self) -> Vec<ViewportId> {
        let ran = self.m.run_requested(SETTLE);
        assert!(ran.len() < SETTLE, "the loop never idles: {ran:?}");
        ran
    }

    /// A listener of `P` for `window` noting what it heard.
    fn listen_in<P: Point<Input = String>>(&self, window: &WindowId) {
        let heard = self.heard.clone();
        self.m
            .app()
            .listen_local_in::<P>(
                window,
                format!("test.in.{window}"),
                ListenOpts::new().owner("test"),
                move |input, _| {
                    heard.borrow_mut().push(input.clone());
                    Ok(())
                },
            )
            .unwrap();
    }

    /// Emits `test.changed` from a worker thread: the local listeners
    /// are queued for the interface's thread.
    fn changed_from_a_worker(&self, what: &str) -> Vec<String> {
        let bus = self.m.app().bus();
        let what = what.to_string();
        std::thread::spawn(move || bus.alter::<Changed>(what, ()).deferred)
            .join()
            .unwrap()
    }
}

/// The main window, a window `w` counting its passes and asking
/// `test.note` when told, and nothing left to run.
fn fixture() -> Fixture {
    let m = MultiWindow::new(|ctx| App::builder().build(ctx).unwrap());
    let refused = bus_contract::declare_all(
        &*m.app().bus(),
        &[
            PointDecl::of::<Changed>("Something a window shows changed."),
            PointDecl::of::<Note>("A note asked for after the drawing."),
        ],
    );
    assert!(refused.is_empty(), "{refused:?}");
    let passes = Rc::new(Cell::new(0));
    let asking = Rc::new(Cell::new(false));
    let (p, a, app) = (passes.clone(), asking.clone(), m.app().clone());
    m.app().open_window(
        WindowSpec::new("w", "W").size([300.0, 200.0]),
        move |ui, _| {
            p.set(p.get() + 1);
            ui.label("window w");
            if a.replace(false) {
                assert!(app.ask::<Note>("from w".into(), Vec::new()).is_some());
            }
        },
    );
    let mut f = Fixture {
        m,
        passes,
        asking,
        heard: Heard::default(),
    };
    f.m.root_pass();
    // The system opens the window: its first pass.
    let v = f.viewport();
    f.m.window_pass(v);
    f.settle();
    assert!(f.passes.get() > 0, "w is drawn");
    f.m.clear_repaints();
    f
}

#[test]
fn a_fact_from_another_thread_heard_for_a_window_draws_that_window() {
    let mut f = fixture();
    f.listen_in::<Changed>(&Fixture::w());
    let before = f.passes.get();

    assert_eq!(f.changed_from_a_worker("photo 1"), ["test.in.w"]);
    let ran = f.settle();

    assert_eq!(*f.heard.borrow(), ["photo 1"]);
    assert_eq!(
        ran.first(),
        Some(&ViewportId::ROOT),
        "the main window runs what was queued: {ran:?}"
    );
    assert!(ran.contains(&f.viewport()), "then w is drawn: {ran:?}");
    assert!(f.passes.get() > before, "w drew what it heard");
}

#[test]
fn a_fact_on_the_interface_thread_heard_for_a_window_draws_it() {
    let mut f = fixture();
    f.listen_in::<Changed>(&Fixture::w());
    let before = f.passes.get();

    let out = f.m.app().bus().alter::<Changed>("photo 2".into(), ());
    assert!(out.deferred.is_empty(), "run inline");
    assert_eq!(*f.heard.borrow(), ["photo 2"]);
    assert!(
        f.m.repaints_due().contains(&f.viewport()),
        "w's pass is asked for"
    );
    let ran = f.settle();
    assert!(ran.contains(&f.viewport()), "{ran:?}");
    assert!(f.passes.get() > before);
}

#[test]
fn a_listener_of_the_main_window_wakes_the_main_window_alone() {
    let mut f = fixture();
    f.listen_in::<Changed>(&WindowId::main());
    let bus = f.m.app().bus();
    let heard = f.heard.clone();
    bus.listen_local::<Changed>(
        "test.plain",
        ListenOpts::new().owner("test"),
        move |i, _| {
            heard.borrow_mut().push(format!("plain {i}"));
            Ok(())
        },
    )
    .unwrap();
    let before = f.passes.get();

    assert_eq!(
        f.changed_from_a_worker("photo 3"),
        ["test.in.main", "test.plain"]
    );
    let ran = f.settle();

    assert_eq!(*f.heard.borrow(), ["photo 3", "plain photo 3"]);
    assert!(
        !ran.is_empty() && ran.iter().all(|v| *v == ViewportId::ROOT),
        "the main window alone: {ran:?}"
    );
    assert_eq!(f.passes.get(), before, "w stays idle");
}

#[test]
fn an_ask_made_in_a_window_is_drained_by_the_main_window_and_draws_the_window() {
    let mut f = fixture();
    f.listen_in::<Note>(&Fixture::w());
    let v = f.viewport();

    f.asking.set(true);
    f.m.window_pass(v);
    let after_ask = f.passes.get();
    assert_eq!(f.m.app().asks_waiting(), 1, "waits for the main window");
    assert!(f.heard.borrow().is_empty());

    let ran = f.settle();
    assert_eq!(*f.heard.borrow(), ["from w"], "drained");
    assert_eq!(f.m.app().asks_waiting(), 0);
    let root = ran.iter().position(|r| *r == ViewportId::ROOT);
    let w = ran.iter().rposition(|r| *r == v);
    assert!(
        matches!((root, w), (Some(r), Some(w)) if r < w),
        "the main window drains, then w draws what it heard: {ran:?}"
    );
    assert!(f.passes.get() > after_ask);
}

#[test]
fn a_listener_of_a_closed_window_wakes_the_main_window() {
    let mut f = fixture();
    f.listen_in::<Changed>(&Fixture::w());
    f.m.app().close_window(&Fixture::w());
    f.settle();
    assert!(f.m.app().window_viewport(&Fixture::w()).is_none());
    f.m.clear_repaints();
    let before = f.passes.get();

    assert_eq!(f.changed_from_a_worker("photo 4"), ["test.in.w"]);
    let ran = f.settle();

    assert_eq!(*f.heard.borrow(), ["photo 4"]);
    assert!(
        !ran.is_empty() && ran.iter().all(|v| *v == ViewportId::ROOT),
        "the main window alone: {ran:?}"
    );
    assert_eq!(f.passes.get(), before);
}

#[test]
fn a_window_drawn_in_the_main_one_wakes_the_main_window() {
    let mut f = fixture();
    f.listen_in::<Changed>(&Fixture::w());
    f.m.ctx().set_embed_viewports(true);
    f.m.clear_repaints();

    let _ = f.m.app().bus().alter::<Changed>("photo 5".into(), ());
    assert_eq!(*f.heard.borrow(), ["photo 5"]);
    let due = f.m.repaints_due();
    assert!(due.contains(&ViewportId::ROOT), "{due:?}");
    assert!(!due.contains(&f.viewport()), "{due:?}");
}
