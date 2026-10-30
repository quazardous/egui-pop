//! A stack of collapsible sections in a panel: the heights it shows when
//! the panel changes, what a drag leaves, and what it tells the debug
//! bridge.

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
