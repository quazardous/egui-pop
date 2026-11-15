//! The late-value cell through its four states: the same room pending,
//! fresh and refreshing (nothing moves when the value lands), its
//! accessible name and value per state, the delayed spinner, and Retry.

use egui::Rect;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::late::{Late, LateValue, Waiting};
use egui_pop_core::motion;
use egui_pop_core::theme::{self, ThemeSpec};
use egui_pop_model::clock::ManualClock;

struct Cell {
    late: Late<String>,
    waiting: Waiting,
    since: Option<f64>,
    rect: Option<Rect>,
    after: Option<Rect>,
    retries: u32,
}

fn harness(
    late: Late<String>,
    waiting: Waiting,
    since: Option<f64>,
) -> (Harness<'static, Cell>, ManualClock) {
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 120.0))
        .build_ui_state(
            |ui, s: &mut Cell| {
                ui.horizontal(|ui| {
                    let mut cell = LateValue::new("Free space", s.late.clone())
                        .sample("000.0 GB")
                        .waiting(s.waiting);
                    if let Some(at) = s.since {
                        cell = cell.since(at);
                    }
                    let out = cell.show(ui);
                    s.rect = Some(out.response.rect);
                    if out.retry {
                        s.retries += 1;
                    }
                    s.after = Some(ui.label("after").rect);
                });
            },
            Cell {
                late,
                waiting,
                since,
                rect: None,
                after: None,
                retries: 0,
            },
        );
    theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    let clock = motion::freeze(&h.ctx, 10.0);
    h.run();
    (h, clock)
}

fn value(h: &Harness<'static, Cell>) -> Option<String> {
    h.get_by_label("Free space").accesskit_node().value()
}

fn busy(h: &Harness<'static, Cell>) -> bool {
    h.get_by_label("Free space").accesskit_node().is_busy()
}

#[egui_pop_test_macros::critical]
#[test]
fn the_cell_keeps_its_room_from_pending_to_fresh_to_refreshing() {
    let (mut h, _clock) = harness(Late::Pending, Waiting::Skeleton, None);
    let pending = h.state().rect.unwrap();
    let after = h.state().after.unwrap();
    assert!(
        h.query_by_label("Loading Free space").is_some(),
        "the skeleton is named"
    );

    h.state_mut().late = Late::Fresh("12.4 GB".into());
    h.run();
    assert_eq!(h.state().rect.unwrap(), pending, "fresh: same rect");
    assert_eq!(h.state().after.unwrap(), after, "what follows did not move");

    h.state_mut().late = Late::Refreshing("12.4 GB".into());
    h.run();
    assert_eq!(h.state().rect.unwrap(), pending, "refreshing: same rect");
    assert_eq!(h.state().after.unwrap(), after);
}

#[test]
fn the_cell_says_its_state_to_assistive_technologies() {
    let (mut h, _clock) = harness(Late::Pending, Waiting::Skeleton, None);
    assert_eq!(value(&h).as_deref(), Some("Loading"));
    assert!(busy(&h));

    h.state_mut().late = Late::Fresh("12.4 GB".into());
    h.run();
    assert_eq!(value(&h).as_deref(), Some("12.4 GB"));
    assert!(!busy(&h));
    assert!(h.query_by_label("12.4 GB").is_some(), "the value is drawn");

    h.state_mut().late = Late::Refreshing("12.4 GB".into());
    h.run();
    assert_eq!(value(&h).as_deref(), Some("12.4 GB"));
    assert!(busy(&h));
    assert!(
        h.query_by_label("Updating Free space").is_some(),
        "the small spinner is named"
    );

    h.state_mut().late = Late::Failed("The disk is not mounted".into());
    h.run();
    assert_eq!(value(&h).as_deref(), Some("The disk is not mounted"));
    assert!(!busy(&h));
}

#[test]
fn a_delayed_spinner_waits_before_showing_and_the_room_is_kept() {
    let (mut h, clock) = harness(Late::Pending, Waiting::Spinner, Some(10.0));
    let rect = h.state().rect.unwrap();
    assert!(
        h.query_by_label("Loading Free space").is_none(),
        "not shown before the delay"
    );
    clock.advance(5.0);
    h.run();
    assert!(
        h.query_by_label("Loading Free space").is_some(),
        "shown after the delay"
    );
    assert_eq!(h.state().rect.unwrap(), rect);
    h.state_mut().late = Late::Fresh("1.0 GB".into());
    h.run();
    assert_eq!(h.state().rect.unwrap(), rect);
}

#[test]
fn retry_is_named_after_the_value_and_reported_once_clicked() {
    let (mut h, _clock) = harness(Late::Failed("Timed out".into()), Waiting::Skeleton, None);
    h.get_by_label("Retry Free space").click();
    h.run();
    assert_eq!(h.state().retries, 1);
}
