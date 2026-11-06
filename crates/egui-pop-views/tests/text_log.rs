//! The text log view: its filters, the tail it follows, the time cursor
//! it highlights and moves, multi-line bodies, and a large log built a
//! screenful at a time.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_views::text_log::{Level, LogEntry, TextLog, TextLogState};
use egui_pop_views::timeline::{TimeScale, TimelineState};

const ENTITIES: [&str; 3] = ["/robot", "/robot/arm", "/camera"];

/// Entry `i`: logged at second `i`, its level and entity in turn, its
/// body `line 00000i` (every 50th on three lines).
fn entry(i: usize) -> LogEntry {
    let body = if i % 50 == 7 {
        format!("line {i:06}\nsecond line\nthird line")
    } else {
        format!("line {i:06}")
    };
    LogEntry::new(i as f64, ENTITIES[i % 3], Level::ALL[i % 6], body)
}

fn entries(n: usize) -> Vec<LogEntry> {
    (0..n).map(entry).collect()
}

/// A row's accessible name: its time, level, entity and first line.
fn row_name(i: usize) -> String {
    format!(
        "{i} {} {} line {i:06}",
        Level::ALL[i % 6].name(),
        ENTITIES[i % 3]
    )
}

struct Log {
    state: TextLogState,
    entries: Vec<LogEntry>,
    cursor: Option<f64>,
    timeline: Option<TimelineState>,
    following: bool,
    picked: Option<usize>,
}

fn harness(n: usize, cursor: Option<f64>, size: egui::Vec2) -> Harness<'static, Log> {
    Harness::builder().with_size(size).build_ui_state(
        |ui, l: &mut Log| {
            let mut log = TextLog::new("log", &l.entries);
            log = match l.timeline.as_mut() {
                Some(t) => log.timeline(t),
                None => log.time_cursor(l.cursor),
            };
            let out = log.show(ui, &mut l.state);
            if let Some(t) = out.cursor_moved {
                l.cursor = Some(t);
            }
            if out.picked.is_some() {
                l.picked = out.picked;
            }
            l.following = out.following;
        },
        Log {
            state: TextLogState::new(),
            entries: entries(n),
            cursor,
            timeline: None,
            following: false,
            picked: None,
        },
    )
}

/// Whether line `i` is built (in view).
fn in_view(h: &Harness<'_, Log>, i: usize) -> bool {
    h.query_all_by_label_contains(&format!("line {i:06}"))
        .next()
        .is_some()
}

/// Over the table, a wheel's lines down (negative: up), in one gesture.
fn wheel(h: &mut Harness<'_, Log>, lines: f32) {
    let at = h.get_by_label("Text log").rect().center();
    let event = |delta, phase| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, delta),
        modifiers: Modifiers::NONE,
        phase,
        source: egui::MouseWheelSource::Wheel,
    };
    h.hover_at(at);
    h.run();
    h.event(event(0.0, egui::TouchPhase::Start));
    h.event(event(-lines, egui::TouchPhase::Move));
    h.run();
    h.event(event(0.0, egui::TouchPhase::End));
    h.run();
}

fn append(h: &mut Harness<'_, Log>, n: usize) {
    let from = h.state().entries.len();
    h.state_mut().entries.extend((from..from + n).map(entry));
    h.run();
    h.run();
}

#[test]
fn levels_and_text_filter_the_lines() {
    let mut h = harness(30, None, egui::vec2(900.0, 700.0));
    h.run();
    egui_pop_test::assert_all_named(&h);
    assert_eq!(h.state().state.shown().len(), 30);
    h.get_by_label("30 of 30 lines");
    assert_eq!(h.state().state.count(Level::Error), 5);

    // The error chip off: no error line left.
    h.get_by_label("Show ERROR lines").click();
    h.run();
    h.run();
    assert!(!h.state().state.level_shown(Level::Error));
    let shown = h.state().state.shown().to_vec();
    assert_eq!(shown.len(), 25);
    assert!(shown.iter().all(|&i| Level::ALL[i % 6] != Level::Error));
    h.get_by_label("25 of 30 lines");
    // Its count stays: what the chip would bring back.
    assert_eq!(h.state().state.count(Level::Error), 5);

    // The text filter, case aside, on the body.
    h.get_by_label("Filter").click();
    h.run();
    h.get_by_label("Filter").type_text("LINE 00001");
    h.run();
    h.run();
    let expected: Vec<usize> = (10..20)
        .filter(|i| Level::ALL[i % 6] != Level::Error)
        .collect();
    assert_eq!(h.state().state.shown(), expected.as_slice());
    assert!(in_view(&h, 11) && !in_view(&h, 2));

    // And on the entity path.
    h.state_mut().state.set_filter("/robot/arm");
    h.state_mut().state.set_level_shown(Level::Error, true);
    h.run();
    let shown = h.state().state.shown().to_vec();
    assert_eq!(shown.len(), 10);
    assert!(shown.iter().all(|i| i % 3 == 1));
}

#[test]
fn the_tail_is_followed_until_the_user_scrolls_up_and_again_at_the_end() {
    let mut h = harness(200, None, egui::vec2(800.0, 300.0));
    h.run();
    h.run();
    assert!(in_view(&h, 199), "the tail in view from the start");
    assert!(!in_view(&h, 0));
    append(&mut h, 5);
    assert!(in_view(&h, 204), "new lines followed");
    assert!(h.state().following);

    // Scrolled up: the view stays where it is as lines come in.
    wheel(&mut h, -20.0);
    append(&mut h, 5);
    assert!(!in_view(&h, 209));
    assert!(!h.state().following);
    assert!(h.state().state.follow(), "Follow stays on");

    // Back at the end: followed again.
    for _ in 0..10 {
        wheel(&mut h, 40.0);
    }
    assert!(in_view(&h, 209));
    append(&mut h, 5);
    assert!(in_view(&h, 214), "followed again");
    assert!(h.state().following);

    // Follow off: new lines are not followed even at the end.
    h.get_by_label("Follow").click();
    h.run();
    assert!(!h.state().state.follow());
    append(&mut h, 30);
    assert!(!in_view(&h, 244));
    // On again: the end comes into view.
    h.get_by_label("Follow").click();
    h.run();
    h.run();
    h.run();
    assert!(in_view(&h, 244));
}

#[test]
fn the_rows_at_the_time_cursor_are_selected_and_a_click_moves_the_cursor() {
    let mut h = harness(40, Some(5.5), egui::vec2(900.0, 700.0));
    // Two lines at second 20: both at the cursor once it is there.
    h.state_mut().entries[21].time = 20.0;
    h.run();
    h.run();
    // The latest line not after the cursor.
    assert_eq!(h.state().state.table().selection().keys(), &[5]);

    // A click on a row moves the cursor to its time.
    h.get_by_role_and_label(egui::accesskit::Role::Button, &row_name(8))
        .click();
    h.run();
    h.run();
    assert_eq!(h.state().cursor, Some(8.0));
    assert_eq!(h.state().picked, Some(8));
    assert_eq!(h.state().state.table().selection().keys(), &[8]);
    // The arrow keys too: the table has the focus.
    h.key_press(Key::ArrowDown);
    h.run();
    h.run();
    assert_eq!(h.state().cursor, Some(9.0));

    // Every line at the cursor's time.
    h.state_mut().cursor = Some(20.5);
    h.run();
    assert_eq!(h.state().state.table().selection().keys(), &[20, 21]);
    // Before the first line: none.
    h.state_mut().cursor = Some(-1.0);
    h.run();
    assert!(h.state().state.table().selection().is_empty());
}

#[test]
fn a_bound_timeline_s_playhead_is_the_cursor_and_moves_with_a_click() {
    let mut h = harness(40, None, egui::vec2(900.0, 700.0));
    let mut timeline = TimelineState::new(TimeScale::Seconds, 60.0);
    timeline.playhead = 3.0;
    h.state_mut().timeline = Some(timeline);
    h.run();
    h.run();
    assert_eq!(h.state().state.table().selection().keys(), &[3]);
    // The times in the timeline's scale.
    let name = format!(
        "0:12.000 {} {} line {:06}",
        Level::ALL[0].name(),
        ENTITIES[0],
        12
    );
    h.get_by_role_and_label(egui::accesskit::Role::Button, &name)
        .click();
    h.run();
    h.run();
    let playhead = h.state().timeline.as_ref().map(|t| t.playhead);
    assert_eq!(playhead, Some(12.0));
    assert_eq!(h.state().state.table().selection().keys(), &[12]);
}

#[test]
fn a_multi_line_body_expands_to_all_its_lines() {
    let mut h = harness(20, None, egui::vec2(900.0, 700.0));
    h.run();
    h.run();
    assert!(h.query_by_label_contains("+2 lines").is_some());
    h.get_by_label("Show all 3 lines").click();
    h.run();
    h.run();
    assert!(h.state().state.is_expanded(7));
    h.get_by_label("Show the first line").click();
    h.run();
    h.run();
    assert!(!h.state().state.is_expanded(7));
}

#[test]
fn a_hundred_thousand_lines_build_a_screenful() {
    let mut h = harness(100_000, None, egui::vec2(900.0, 500.0));
    h.run();
    h.run();
    let built = h.query_all_by_label_contains("line 0").count();
    assert!(built < 200, "{built} lines built");
    assert!(in_view(&h, 99_999), "the tail");
    // The cursor moved elsewhere: its row comes into view.
    h.state_mut().cursor = Some(50_000.0);
    h.run();
    h.run();
    h.run();
    assert!(in_view(&h, 50_000));
    assert_eq!(h.state().state.table().selection().keys(), &[50_000]);
    // A filter over all of them; a line more that does not match.
    h.state_mut().state.set_filter("line 0999");
    h.run();
    assert_eq!(h.state().state.shown().len(), 100);
    append(&mut h, 1);
    assert_eq!(h.state().state.shown().len(), 100);
}
