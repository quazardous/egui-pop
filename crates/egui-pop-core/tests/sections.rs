//! A stack of collapsible sections in a panel: the heights it shows when
//! the panel changes, what a drag leaves, and what it tells the debug
//! bridge.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::sections::{Layout, Section, Sections, laid_out};

const NAME: &str = "test.stack";

fn stack(ui: &mut egui::Ui) {
    Sections::new(NAME).show(ui, |stack| {
        for (key, title) in [("a", "Aa"), ("b", "Bb"), ("c", "Cc")] {
            stack.section(Section::new(key, title).min_height(40.0), move |ui| {
                ui.label(format!("{title} row"));
            });
        }
    });
}

fn harness(height: f32) -> Harness<'static> {
    Harness::builder()
        .with_size(egui::vec2(300.0, height))
        .build_ui(stack)
}

fn heights(h: &Harness<'_>) -> Vec<f32> {
    laid_out(&h.ctx, NAME).iter().map(|s| s.height).collect()
}

fn hands(h: &Harness<'_>) -> Vec<bool> {
    laid_out(&h.ctx, NAME).iter().map(|s| s.by_hand).collect()
}

/// Keeps a layout, then shows the stack in a new height.
fn set_layout(h: &mut Harness<'_>, layout: &Layout) {
    layout.store(&h.ctx, NAME);
    h.run();
}

/// Gives the stack `room` points, whatever margin the harness's panel
/// takes off the window.
fn resize(h: &mut Harness<'_>, room: f32) {
    let laid = laid_out(&h.ctx, NAME);
    let top = laid[0].band.top();
    let shown = laid[2].body.unwrap().bottom() - top;
    let margin = h.ctx.content_rect().height() - shown;
    h.set_size(egui::vec2(300.0, room + margin));
    h.run();
}

fn drag(h: &mut Harness<'_>, name: &str, by: f32) {
    let a = h
        .get_by_role_and_label(Role::Splitter, name)
        .rect()
        .center();
    h.drag_at(a);
    h.run();
    for i in 1..=4 {
        h.hover_at(a + egui::vec2(0.0, by * i as f32 / 4.0));
        h.run();
    }
    h.drop_at(a + egui::vec2(0.0, by));
    h.run();
}

fn close(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1.0)
}

#[test]
fn sections_sized_by_the_list_share_the_room_and_the_last_takes_the_rest() {
    let mut h = harness(600.0);
    h.run();
    let shown = heights(&h);
    assert!((shown[0] - shown[2]).abs() < 1.0, "{shown:?}");
    assert_eq!(hands(&h), [false, false, false]);
    let mut layout = Layout::default();
    layout.set_height("a", Some(200.0));
    set_layout(&mut h, &layout);
    let shown = heights(&h);
    assert!((shown[0] - 200.0).abs() < 0.1, "{shown:?}");
    assert!((shown[1] - shown[2]).abs() < 1.0, "{shown:?}");
    assert_eq!(hands(&h), [true, false, false]);
    // Even a height kept for the last open section is not its own.
    layout.set_height("c", Some(50.0));
    set_layout(&mut h, &layout);
    assert_eq!(hands(&h), [true, false, false]);
    assert!(heights(&h)[2] > 50.0);
}

#[test]
fn a_short_panel_takes_from_the_list_then_the_heights_set_by_hand() {
    let band = |h: &Harness<'_>| laid_out(&h.ctx, NAME)[0].band.height();
    let mut h = harness(600.0);
    h.run();
    let bands = 3.0 * band(&h);
    let mut layout = Layout::default();
    layout.set_height("a", Some(200.0));
    set_layout(&mut h, &layout);
    // Room for 200 and the two least heights: the list gives first.
    resize(&mut h, bands + 280.0);
    assert!(
        close(&heights(&h), &[200.0, 40.0, 40.0]),
        "{:?}",
        heights(&h)
    );
    // Shorter: then the height set by hand; the stored one stays.
    resize(&mut h, bands + 200.0);
    assert!(
        close(&heights(&h), &[120.0, 40.0, 40.0]),
        "{:?}",
        heights(&h)
    );
    assert_eq!(Layout::load(&h.ctx, NAME).unwrap().height("a"), Some(200.0));
    // The room back, the height back.
    resize(&mut h, 500.0);
    assert!((heights(&h)[0] - 200.0).abs() < 0.1, "{:?}", heights(&h));
    // Short again, and a drag: the stored height aligns on what shows,
    // the border starts from there.
    resize(&mut h, bands + 200.0);
    drag(&mut h, "Resize Aa", -20.0);
    assert!(
        close(&heights(&h), &[100.0, 60.0, 40.0]),
        "{:?}",
        heights(&h)
    );
    let stored = Layout::load(&h.ctx, NAME).unwrap().height("a").unwrap();
    assert!((stored - 100.0).abs() < 1.0, "{stored}");
}

#[test]
fn only_a_real_overflow_makes_the_stack_taller_than_its_panel() {
    let mut h = harness(600.0);
    h.run();
    let tall = |h: &Harness<'_>| {
        let laid = laid_out(&h.ctx, NAME);
        laid[2].body.unwrap().bottom() - laid[0].band.top()
    };
    let band = laid_out(&h.ctx, NAME)[0].band.height();
    // Exactly the least heights: the stack fits.
    resize(&mut h, 3.0 * band + 120.0);
    assert!((tall(&h) - 3.0 * band - 120.0).abs() < 1.0, "{}", tall(&h));
    // Shorter: each at its least, and the stack goes on under the panel.
    resize(&mut h, 3.0 * band + 60.0);
    assert!(
        close(&heights(&h), &[40.0, 40.0, 40.0]),
        "{:?}",
        heights(&h)
    );
    assert!((tall(&h) - 3.0 * band - 120.0).abs() < 1.0, "{}", tall(&h));
    assert!(h.ctx.content_rect().height() < tall(&h), "it overflows");
    // A drag stops at once: nothing can give, nothing moves.
    let before = heights(&h);
    drag(&mut h, "Resize Aa", 30.0);
    assert_eq!(heights(&h), before);
    // The wheel scrolls it.
    let top = laid_out(&h.ctx, NAME)[0].band.top();
    h.hover_at(egui::pos2(100.0, 40.0));
    h.run();
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -30.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    assert!(laid_out(&h.ctx, NAME)[0].band.top() < top);
}

#[test]
fn a_drag_keeps_a_section_it_does_not_touch_in_the_list_and_a_double_click_drops_the_rest() {
    let mut h = harness(600.0);
    h.run();
    let before = heights(&h);
    // Between the first two: the last is not touched.
    drag(&mut h, "Resize Aa", 30.0);
    let after = heights(&h);
    assert!(
        close(&after, &[before[0] + 30.0, before[1] - 30.0, before[2]]),
        "{after:?}"
    );
    assert_eq!(hands(&h), [true, true, false]);
    // A double click on the boundary drops them.
    let line = h.get_by_role_and_label(Role::Splitter, "Resize Aa").rect();
    h.hover_at(line.center());
    h.run();
    for pressed in [true, false, true, false] {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: line.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.step();
    h.run();
    assert_eq!(hands(&h), [false, false, false]);
    assert!(close(&heights(&h), &before), "{:?}", heights(&h));
}

#[test]
fn a_band_is_clicked_right_up_to_its_boundary_and_the_boundary_wins_at_the_line() {
    let mut h = harness(600.0);
    h.run();
    let laid = laid_out(&h.ctx, NAME);
    let line = laid[1].band.top();
    let x = laid[1].band.center().x;
    let a = egui::pos2(x, line);
    // At the line, the boundary: a drag moves it.
    h.drag_at(a);
    h.run();
    h.hover_at(a + egui::vec2(0.0, 20.0));
    h.run();
    h.drop_at(a + egui::vec2(0.0, 20.0));
    h.run();
    assert!(
        laid_out(&h.ctx, NAME)[0].by_hand,
        "{:?}",
        laid_out(&h.ctx, NAME)
    );
    assert!(laid_out(&h.ctx, NAME)[1].open);
    // In the middle of the band, a click folds it.
    let band = laid_out(&h.ctx, NAME)[1].band.center();
    h.drag_at(band);
    h.drop_at(band);
    h.run();
    assert!(!laid_out(&h.ctx, NAME)[1].open);
}

#[test]
fn a_boundary_under_a_folded_section_resizes_the_first_open_one_above() {
    // A open, B and C folded, D open.
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 600.0))
        .build_ui(|ui| {
            Sections::new(NAME).show(ui, |stack| {
                stack.section(Section::new("a", "Aa").min_height(40.0), |_| {});
                stack.section(Section::new("b", "Bb").open(false), |_| {});
                stack.section(Section::new("c", "Cc").open(false), |_| {});
                stack.section(Section::new("d", "Dd").min_height(40.0), |_| {});
            });
        });
    h.run();
    let start = laid_out(&h.ctx, NAME);
    // Three lines, all named after the section they resize, one a stop.
    let lines = h
        .get_all_by_role_and_label(Role::Splitter, "Resize Aa")
        .count();
    assert_eq!(lines, 3);
    for (at, line) in [(1, 30.0), (2, -30.0), (3, 30.0), (3, -30.0)] {
        // Each line: drag it by this much, and it stays under the pointer.
        let before = laid_out(&h.ctx, NAME);
        let a = egui::pos2(before[at].band.center().x, before[at].band.top());
        h.drag_at(a);
        h.run();
        h.hover_at(a + egui::vec2(0.0, line));
        h.run();
        let now = laid_out(&h.ctx, NAME);
        assert!(
            (now[0].height - before[0].height - line).abs() < 1.0,
            "{at} {now:?}"
        );
        assert!(
            (now[3].height - before[3].height + line).abs() < 1.0,
            "{at} {now:?}"
        );
        assert!(
            (now[at].band.top() - before[at].band.top() - line).abs() < 1.0,
            "{at} {now:?}"
        );
        // No section goes up as the line goes down.
        if line > 0.0 {
            for (n, b) in now.iter().zip(&before) {
                assert!(n.band.top() >= b.band.top() - 0.1, "{at} {now:?}");
            }
        }
        h.drop_at(a + egui::vec2(0.0, line));
        h.run();
    }
    assert_ne!(start, laid_out(&h.ctx, NAME));
}

#[test]
fn with_nothing_open_above_a_line_is_no_boundary() {
    // A folded, B open: the line above B moves nothing; it folds nothing.
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 600.0))
        .build_ui(|ui| {
            Sections::new(NAME).show(ui, |stack| {
                stack.section(Section::new("a", "Aa").open(false), |_| {});
                stack.section(Section::new("b", "Bb"), |_| {});
                stack.section(Section::new("c", "Cc"), |_| {});
            });
        });
    h.run();
    assert_eq!(
        h.query_all_by_role_and_label(Role::Splitter, "Resize Aa")
            .count(),
        0
    );
    let before = laid_out(&h.ctx, NAME);
    let a = egui::pos2(before[1].band.center().x, before[1].band.top());
    h.drag_at(a);
    h.run();
    h.hover_at(a + egui::vec2(0.0, 40.0));
    h.run();
    h.drop_at(a + egui::vec2(0.0, 40.0));
    h.run();
    let after = laid_out(&h.ctx, NAME);
    assert_eq!(after[1].height, before[1].height);
    assert_eq!(after[1].band, before[1].band);
}

#[test]
fn the_debug_bridge_says_each_height_and_whether_it_is_set_by_hand() {
    let mut h = harness(600.0);
    let mut layout = Layout::default();
    layout.set_height("a", Some(200.0));
    set_layout(&mut h, &layout);
    let report = egui_pop_core::state::report(&h.ctx, 8);
    let laid = report
        .iter()
        .find(|r| r.name == "core.sections.laid")
        .expect("the slot is listed");
    let text = serde_json::to_string(&laid).unwrap();
    assert!(text.contains("a 200 by hand"), "{text}");
    assert!(text.contains("b "), "{text}");
    // Laid itself carries them for an agent.
    let json = serde_json::to_value(laid_out(&h.ctx, NAME)).unwrap();
    assert_eq!(json[0]["height"], 200.0);
    assert_eq!(json[0]["by_hand"], true);
    assert_eq!(json[2]["by_hand"], false);
}

const ROW: f32 = 20.0;

/// A row exactly `ROW` tall, named by its number.
fn row(ui: &mut egui::Ui, name: &str, i: usize) {
    let width = ui.available_width();
    ui.add_sized([width, ROW], egui::Label::new(format!("{name} {i}")));
}

/// A stack of one windowed section of `count` rows and one other, its
/// count read each pass, and every window its closure was given.
fn windowed(
    count: Rc<Cell<usize>>,
    windows: Rc<RefCell<Vec<Range<usize>>>>,
) -> impl FnMut(&mut egui::Ui) {
    move |ui| {
        let (count, windows) = (count.get(), windows.clone());
        Sections::new(NAME).show(ui, |stack| {
            stack.rows(
                Section::new("log", "Log").row_height(ROW),
                count,
                move |ui, range| {
                    windows.borrow_mut().push(range.clone());
                    for i in range {
                        row(ui, "Line", i);
                    }
                },
            );
            stack.section(Section::new("other", "Other").min_height(40.0), |ui| {
                ui.label("Other row");
            });
        });
    }
}

/// Over `at`, a wheel's notches down (negative) or up. In one gesture, as
/// a touchpad's, so that egui does not smooth it over many passes.
fn wheel(h: &mut Harness<'_>, at: egui::Pos2, notches: f32) {
    let event = |delta, phase| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, delta),
        modifiers: egui::Modifiers::NONE,
        phase,
    };
    h.hover_at(at);
    h.run();
    h.event(event(0.0, egui::TouchPhase::Start));
    h.event(event(notches, egui::TouchPhase::Move));
    h.run();
    h.event(event(0.0, egui::TouchPhase::End));
    h.run();
}

fn laid(h: &Harness<'_>, key: &str) -> egui_pop_core::sections::Laid {
    laid_out(&h.ctx, NAME)
        .into_iter()
        .find(|s| s.key == key)
        .unwrap()
}

/// How many rows a body of this height shows, the partial ones counted.
fn in_view(body: egui::Rect) -> usize {
    (body.height() / ROW).ceil() as usize
}

/// Every row in view is in the window: no blank row is left.
fn assert_covers(h: &Harness<'_>) {
    let log = laid(h, "log");
    let window = log.window.clone().unwrap();
    assert!(window.start as f32 * ROW <= log.scroll, "{log:?}");
    let bottom = log.scroll + log.body.unwrap().height();
    let drawn = window.end as f32 * ROW;
    assert!(
        drawn >= bottom,
        "{log:?}: rows to {drawn}, in view to {bottom}"
    );
}

#[test]
fn a_section_of_thousands_of_rows_only_draws_those_in_view() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    let count = Rc::new(Cell::new(100_000));
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 400.0))
        .build_ui(windowed(count, windows.clone()));
    h.run();
    let log = laid(&h, "log");
    let most = in_view(log.body.unwrap()) + 1;
    assert_eq!(log.window, Some(0..most), "{log:?}");
    assert!(h.query_by_label("Line 0").is_some());
    assert!(h.query_by_label(&format!("Line {}", most + 1)).is_none());
    // Scrolled far down, the window moved and is as small.
    let inside = log.body.unwrap().center();
    for _ in 0..4 {
        wheel(&mut h, inside, -20.0);
    }
    let log = laid(&h, "log");
    assert!(log.scroll > 500.0, "{log:?}");
    assert_covers(&h);
    let window = log.window.clone().unwrap();
    assert!(window.start > 10, "{log:?}");
    assert!(h.query_by_label("Line 0").is_none());
    assert!(
        h.query_by_label(&format!("Line {}", window.start))
            .is_some()
    );
    // Never, on any pass, more than that.
    for drawn in windows.borrow().iter() {
        assert!(drawn.len() <= most + 1, "{drawn:?}");
    }
}

#[test]
fn the_window_follows_the_sections_size_so_no_row_is_left_blank() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    let count = Rc::new(Cell::new(5_000));
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 300.0))
        .build_ui(windowed(count.clone(), windows));
    h.run();
    let before = laid(&h, "log").window.unwrap();
    h.set_size(egui::vec2(300.0, 700.0));
    h.run();
    let after = laid(&h, "log").window.unwrap();
    assert!(after.len() > before.len(), "{before:?} then {after:?}");
    assert_covers(&h);

    // At the very end of the scroll, a section grown has fewer rows to
    // show than room: the scroll comes back by itself, with another pass.
    let inside = laid(&h, "log").body.unwrap().center();
    wheel(&mut h, inside, -10_000.0);
    let log = laid(&h, "log");
    assert_eq!(log.window.as_ref().unwrap().end, 5_000, "{log:?}");
    assert!(h.query_by_label("Line 4999").is_some());

    // The rows going away with no input at all: the pass that sees it asks
    // for the next, which draws the rows under the new scroll.
    count.set(30);
    h.step();
    assert!(
        h.ctx.has_requested_repaint(),
        "the scroll moved after drawing"
    );
    h.run();
    let log = laid(&h, "log");
    let content = 30.0 * ROW;
    let lowest = (content - log.body.unwrap().height()).max(0.0);
    assert!((log.scroll - lowest).abs() < 1.0, "{log:?}");
    assert_eq!(log.window.as_ref().unwrap().end, 30, "{log:?}");
    assert!(h.query_by_label("Line 29").is_some());
}

#[test]
fn a_folded_windowed_section_draws_no_row() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 400.0))
        .build_ui(windowed(Rc::new(Cell::new(5_000)), windows.clone()));
    h.run();
    h.get_by_label("Log").click();
    h.run();
    windows.borrow_mut().clear();
    h.run();
    let log = laid(&h, "log");
    assert!(!log.open && log.window.is_none(), "{log:?}");
    assert!(windows.borrow().is_empty());
    assert!(h.query_by_label("Line 0").is_none());
}

#[test]
fn a_windowed_section_keeps_two_rows_or_all_of_them_when_fewer() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 100.0))
        .build_ui(|ui| {
            Sections::new(NAME).show(ui, |stack| {
                for (key, count) in [("none", 0), ("one", 1), ("two", 2), ("many", 5_000)] {
                    let section = Section::new(key, key).row_height(ROW);
                    stack.rows(section, count, |ui, range| {
                        for i in range {
                            row(ui, "Row", i);
                        }
                    });
                }
                // The least height asked by name still wins.
                let section = Section::new("named", "named")
                    .row_height(ROW)
                    .min_height(7.0);
                stack.rows(section, 5_000, |_, _| {});
            });
        });
    h.run();
    let two = 2.0 * ROW;
    let least = [0.0, ROW, two, two, 7.0];
    let shown = heights(&h);
    assert!(close(&shown, &least), "{shown:?}");
    // The one row fits: it has nowhere to scroll.
    assert_eq!(laid(&h, "one").window, Some(0..1));
    assert_eq!(laid(&h, "none").window, Some(0..0));
}

#[test]
fn a_row_height_defaults_to_the_theme() {
    let mut seen = 0.0;
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 100.0))
        .build_ui(|ui| {
            seen = egui_pop_core::sections::row_height(ui);
            let spacing = ui.spacing_mut();
            spacing.interact_size.y = 31.0;
            let tall = egui_pop_core::sections::row_height(ui);
            assert_eq!(tall, 31.0);
            Sections::new(NAME).show(ui, |stack| {
                stack.rows(Section::new("one", "One"), 1, |_, _| {});
                stack.rows(Section::new("two", "Two"), 5, |_, _| {});
            });
        });
    h.run();
    let shown = heights(&h);
    assert!(close(&shown, &[31.0, 62.0]), "{shown:?}");
    drop(h);
    assert!(seen > 0.0);
}

/// The stack's top, the body's scroll and a parent's: what must not move.
fn places(h: &Harness<'_>, key: &str) -> (f32, f32) {
    let s = laid(h, key);
    (laid(h, "first").band.top(), s.scroll)
}

/// A stack too tall for its 120 points of panel, inside a parent's scroll
/// of its own, which reports how far it is scrolled.
fn nested(parent: Rc<Cell<f32>>) -> impl FnMut(&mut egui::Ui) {
    move |ui| {
        let out = egui::ScrollArea::vertical()
            .id_salt("test.parent")
            .show(ui, |ui| {
                ui.allocate_ui(egui::vec2(280.0, 120.0), |ui| {
                    ui.set_min_size(egui::vec2(280.0, 120.0));
                    ui.set_max_size(egui::vec2(280.0, 120.0));
                    Sections::new(NAME).show(ui, |stack| {
                        let first = Section::new("first", "First").min_height(60.0);
                        stack.section(first, |ui| {
                            for i in 0..30 {
                                ui.label(format!("Long {i}"));
                            }
                        });
                        let short = Section::new("short", "Short").min_height(60.0);
                        stack.section(short, |ui| {
                            ui.label("Short row");
                        });
                        let rows = Section::new("rows", "Rows")
                            .row_height(ROW)
                            .min_height(60.0);
                        stack.rows(rows, 1_000, |ui, range| {
                            for i in range {
                                row(ui, "Row", i);
                            }
                        });
                        let bare = Section::new("bare", "Bare").min_height(60.0);
                        stack.bare(bare, |ui| {
                            ui.label("Bare row");
                        });
                    });
                });
                ui.add_space(2_000.0);
            });
        parent.set(out.state.offset.y);
    }
}

#[test]
fn a_wheel_past_the_end_of_a_bodys_scroll_stops_there() {
    let parent = Rc::new(Cell::new(0.0));
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 400.0))
        .build_ui(nested(parent.clone()));
    h.run();
    // Scrolled down to its end: first the body moves, alone.
    let body = laid(&h, "first").body.unwrap().center();
    let top = laid(&h, "first").band.top();
    wheel(&mut h, body, -2.0);
    let (stack, scroll) = places(&h, "first");
    assert!(scroll > 0.0, "the body scrolls");
    assert_eq!((stack, parent.get()), (top, 0.0));
    for _ in 0..40 {
        wheel(&mut h, body, -4.0);
    }
    let (stack, end) = places(&h, "first");
    assert!(end > 400.0, "{end}");
    assert_eq!((stack, parent.get()), (top, 0.0));
    // Past its end, nothing moves, however much it is asked.
    for _ in 0..4 {
        wheel(&mut h, body, -4.0);
        assert_eq!(places(&h, "first"), (top, end));
        assert_eq!(parent.get(), 0.0);
    }
    // And back at its start, up: nothing but itself either.
    for _ in 0..40 {
        wheel(&mut h, body, 4.0);
    }
    assert_eq!(places(&h, "first"), (top, 0.0));
    assert_eq!(parent.get(), 0.0);
}

#[test]
fn a_wheel_over_windowed_rows_stops_at_their_end_too() {
    let parent = Rc::new(Cell::new(0.0));
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 400.0))
        .build_ui(nested(parent.clone()));
    h.run();
    // The stack scrolled to bring the section in view by its band.
    let band = laid(&h, "short").band.center();
    wheel(&mut h, band, -8.0);
    let top = laid(&h, "first").band.top();
    let body = laid(&h, "rows").body.unwrap();
    let rows_at = body.center();
    let frame = h.ctx.content_rect();
    assert!(
        frame.contains(rows_at) && rows_at.y < 120.0 + 60.0,
        "{rows_at:?}"
    );
    wheel(&mut h, rows_at, 8.0);
    assert_eq!(laid(&h, "first").band.top(), top, "scrolled at its start");
    wheel(&mut h, rows_at, -2.0);
    assert!(laid(&h, "rows").scroll > 0.0);
    assert_eq!(laid(&h, "first").band.top(), top);
    assert_eq!(parent.get(), 0.0);
}

#[test]
fn a_wheel_over_a_band_or_a_body_with_nothing_to_scroll_scrolls_the_stack() {
    let parent = Rc::new(Cell::new(0.0));
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 400.0))
        .build_ui(nested(parent.clone()));
    h.run();
    let top = laid(&h, "first").band.top();
    // Over a band.
    let band = laid(&h, "first").band.center();
    wheel(&mut h, band, -1.0);
    let moved = laid(&h, "first").band.top();
    assert!(moved < top, "{moved} from {top}");
    assert_eq!(parent.get(), 0.0, "the stack had somewhere to go");
    // Over a body that fits.
    let body = laid(&h, "short").body.unwrap().center();
    wheel(&mut h, body, 1.0);
    assert!(laid(&h, "first").band.top() > moved);
    assert_eq!(parent.get(), 0.0);
    // At the stack's own end, the band hands on to the parent: that is no
    // section's scroll.
    let band = laid(&h, "first").band.center();
    wheel(&mut h, band, 8.0);
    assert_eq!(laid(&h, "first").band.top(), top);
    h.hover_at(band);
    wheel(&mut h, band, -80.0);
    let band = laid(&h, "bare").band.center();
    wheel(&mut h, band, -80.0);
    assert!(parent.get() > 0.0, "{}", parent.get());
}

#[test]
fn a_bare_body_has_no_scroll_of_ours_and_keeps_the_wheel() {
    let parent = Rc::new(Cell::new(0.0));
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 400.0))
        .build_ui(nested(parent.clone()));
    h.run();
    let band = laid(&h, "short").band.center();
    wheel(&mut h, band, -80.0);
    let top = laid(&h, "first").band.top();
    let bare = laid(&h, "bare");
    assert_eq!(bare.scroll, 0.0);
    assert_eq!(bare.window, None);
    wheel(&mut h, bare.body.unwrap().center(), 80.0);
    assert_eq!(laid(&h, "first").band.top(), top, "the stack stays");
    assert_eq!(parent.get(), 0.0);
}

#[test]
fn a_bare_body_is_given_the_whole_body_for_its_own_scroll() {
    let seen = Rc::new(Cell::new((egui::Rect::NOTHING, 0.0_f32)));
    let inner = seen.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 300.0))
        .build_ui(move |ui| {
            let inner = inner.clone();
            Sections::new(NAME).show(ui, |stack| {
                stack.bare(Section::new("view", "View"), move |ui| {
                    let rect = ui.max_rect();
                    let out = egui::ScrollArea::vertical()
                        .id_salt("test.own")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for i in 0..100 {
                                ui.label(format!("Own {i}"));
                            }
                        });
                    inner.set((rect, out.state.offset.y));
                });
            });
        });
    h.run();
    let view = laid(&h, "view");
    let body = view.body.unwrap();
    let (rect, _) = seen.get();
    assert_eq!(rect, body);
    wheel(&mut h, body.center(), -4.0);
    assert!(seen.get().1 > 0.0, "the view's own scroll moves");
    assert_eq!(laid(&h, "view").scroll, 0.0);
}
