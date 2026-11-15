//! `hooks`' contract (docs/viewport.md, "Hooks"): the hooks set, by
//! name; the causes' names; the trace kept to its bound; the
//! `before_compare` hook over the "before" key, asked once per press
//! (and release after a press it let through), its veto and redirect
//! traced. `before_view`,
//! `before_handle`, `on_double_click`, `under_overlays`, `after_paint`
//! and `before_display` are driven in `view.rs`, `overlays.rs` and
//! `pointer.rs`.

use std::cell::Cell;
use std::rc::Rc;

use egui::{Key, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_viewport::hooks::TRACE_KEPT;
use egui_pop_viewport::{
    Cause, Compare, Hooks, Phase, Signal, Verdict, View, ViewChange, Viewport, ViewportState,
};

#[derive(Default)]
struct Scene {
    state: ViewportState,
    view: View,
    compare: Compare,
    signals: Vec<Signal>,
}

fn harness(state: ViewportState) -> Harness<'static, Scene> {
    Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui_state(
            |ui, s: &mut Scene| {
                let out = Viewport::new("v")
                    .image_size([1000, 800])
                    .compare(s.compare)
                    .show(ui, &mut s.state, &mut s.view);
                for signal in out.signals {
                    // The application keeps the comparison it is told,
                    // not the quasimode's ("before" while the key is
                    // held).
                    if let Signal::Compare { compare, phase } = signal
                        && phase != Phase::Begin
                        && compare != Compare::Before
                    {
                        s.compare = compare;
                    }
                    s.signals.push(signal);
                }
            },
            Scene {
                state,
                view: View::actual([500.0, 400.0]),
                ..Scene::default()
            },
        )
}

fn compares(s: &Scene) -> Vec<(Compare, Phase)> {
    s.signals
        .iter()
        .filter_map(|s| match s {
            Signal::Compare { compare, phase } => Some((*compare, *phase)),
            _ => None,
        })
        .collect()
}

/// Hovers the viewport, holds the "before" key a few frames, lets go.
fn hold_before(h: &mut Harness<'_, Scene>) {
    let c = h.get_by_label("Image viewport").rect().center();
    h.hover_at(c);
    h.run();
    h.key_down(Key::Backslash);
    for _ in 0..4 {
        h.step();
    }
    h.key_up(Key::Backslash);
    h.run();
}

#[test]
fn the_hooks_set_are_named_in_their_order() {
    let mut hooks = Hooks::default();
    assert!(hooks.names().is_empty());
    assert_eq!(
        format!("{hooks:?}"),
        "Hooks { before_view: false, on_double_click: false, before_handle: false, \
         before_compare: false, on_readout: false, under_overlays: false, \
         after_paint: false, before_display: false }"
    );
    hooks.after_paint = Some(Box::new(|_, _| {}));
    hooks.before_view = Some(Box::new(|_, _| Verdict::Allow));
    hooks.on_readout = Some(Box::new(|_| String::new()));
    assert_eq!(hooks.names(), ["before_view", "on_readout", "after_paint"]);
    hooks.on_double_click = Some(Box::new(|_, _, default| default()));
    hooks.before_handle = Some(Box::new(|_, _| Verdict::Allow));
    hooks.before_compare = Some(Box::new(|_, _| Verdict::Allow));
    hooks.under_overlays = Some(Box::new(|_, _| {}));
    hooks.before_display = Some(Box::new(|_| Verdict::Allow));
    assert_eq!(
        hooks.names(),
        [
            "before_view",
            "on_double_click",
            "before_handle",
            "before_compare",
            "on_readout",
            "under_overlays",
            "after_paint",
            "before_display",
        ]
    );
}

#[test]
fn each_cause_has_its_name() {
    let names: Vec<_> = [
        Cause::Pan,
        Cause::Wheel,
        Cause::Pinch,
        Cause::Key,
        Cause::DoubleClick,
        Cause::Click,
        Cause::Command,
    ]
    .map(Cause::name)
    .into();
    assert_eq!(
        names,
        [
            "pan",
            "wheel",
            "pinch",
            "key",
            "double-click",
            "click",
            "command"
        ]
    );
}

#[test]
fn the_trace_keeps_the_latest_decisions_up_to_its_bound() {
    let mut state = ViewportState::new();
    state.hooks_mut().before_view = Some(Box::new(|c: &ViewChange, _| {
        assert_eq!(c.cause, Cause::Key);
        Verdict::Veto
    }));
    let mut h = harness(state);
    h.run();
    h.get_by_label("Image viewport").click();
    h.run();
    for i in 0..TRACE_KEPT + 6 {
        h.key_press(if i % 2 == 0 { Key::Num2 } else { Key::Num0 });
        h.run();
    }
    let trace: Vec<_> = h.state().state.trace().cloned().collect();
    assert_eq!(trace.len(), TRACE_KEPT);
    assert!(
        trace.iter().all(|t| t.hook == "before_view"
            && t.verdict == "veto"
            && t.detail.starts_with("key to ")),
        "{trace:?}"
    );
    assert!(
        trace.windows(2).all(|w| w[0].time <= w[1].time),
        "oldest first"
    );
    assert_eq!(
        h.state().view,
        View::actual([500.0, 400.0]),
        "every key vetoed"
    );
}

#[test]
fn the_before_key_shows_before_while_held_without_a_hook() {
    let mut h = harness(ViewportState::new());
    hold_before(&mut h);
    assert_eq!(
        compares(h.state()),
        [(Compare::Before, Phase::Begin), (Compare::Off, Phase::End)],
        "the End carries the application's comparison"
    );
    assert_eq!(h.state().state.trace().count(), 0, "no hook, no trace");
}

#[test]
fn a_vetoed_comparison_is_asked_once_per_change_and_traced() {
    let asked = Rc::new(Cell::new(0));
    let a = asked.clone();
    let mut state = ViewportState::new();
    state.hooks_mut().before_compare = Some(Box::new(move |_, _| {
        a.set(a.get() + 1);
        Verdict::Veto
    }));
    let mut h = harness(state);
    hold_before(&mut h);
    assert!(compares(h.state()).is_empty(), "{:?}", compares(h.state()));
    assert_eq!(h.state().compare, Compare::Off);
    // Once on the press; the release changes nothing it let through.
    assert_eq!(asked.get(), 1);
    let trace: Vec<_> = h
        .state()
        .state
        .trace()
        .map(|t| (t.hook, t.verdict))
        .collect();
    assert_eq!(trace, [("before_compare", "veto")]);
}

#[test]
fn a_redirected_comparison_is_signalled_as_given_and_traced() {
    let mut state = ViewportState::new();
    state.hooks_mut().before_compare = Some(Box::new(|_, proposed| {
        if *proposed == Compare::Before {
            Verdict::Redirect(Compare::Split(0.25))
        } else {
            Verdict::Allow
        }
    }));
    let mut h = harness(state);
    hold_before(&mut h);
    assert_eq!(
        compares(h.state()),
        [
            (Compare::Split(0.25), Phase::Begin),
            (Compare::Off, Phase::End),
        ]
    );
    let trace: Vec<_> = h
        .state()
        .state
        .trace()
        .map(|t| (t.hook, t.verdict))
        .collect();
    assert_eq!(trace, [("before_compare", "redirect")]);
}
