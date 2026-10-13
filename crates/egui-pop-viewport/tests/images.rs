//! Thumbnails and the histogram: named for screen readers, selectable,
//! virtualised, drawn from supplied data.

use egui::{Color32, Event, Key, Modifiers, PointerButton, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_viewport::change::{Change, Verdict};
use egui_pop_viewport::histogram::{Channel, Clipping, Drag, Histogram};
use egui_pop_viewport::thumbnail::{Thumbnail, fitted, thumbnail_grid};

#[test]
fn a_thumbnail_is_named_selectable_and_clicked() {
    let mut h = Harness::builder()
        .with_size(vec2(300.0, 120.0))
        .build_ui_state(
            |ui, selected: &mut usize| {
                ui.horizontal(|ui| {
                    for i in 0..2 {
                        let t = Thumbnail::new(format!("Image {i}"))
                            .placeholder(Color32::from_gray(100))
                            .selected(*selected == i)
                            .size(vec2(96.0, 72.0));
                        if ui.add(t).clicked() {
                            *selected = i;
                        }
                    }
                });
            },
            0,
        );
    h.run();
    h.get_by_label("Image 1").click();
    h.run();
    assert_eq!(*h.state(), 1);
    assert_eq!(
        h.get_by_label("Image 1").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    egui_pop_test::assert_all_named(&h);
}

#[test]
fn the_grid_builds_only_the_rows_in_view() {
    let mut h = Harness::builder()
        .with_size(vec2(420.0, 300.0))
        .build_ui(|ui| {
            thumbnail_grid(ui, 10_000, vec2(96.0, 72.0), |ui, i| {
                ui.add(Thumbnail::new(format!("Image {i}")).placeholder(Color32::DARK_GRAY));
            });
        });
    h.run();
    assert!(h.query_by_label("Image 0").is_some());
    assert!(h.query_by_label("Image 9999").is_none());
    let built = h.query_all_by_label_contains("Image ").count();
    assert!(built < 60, "{built} built");
}

#[test]
fn an_image_keeps_its_aspect() {
    let outer = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(100.0, 100.0));
    let r = fitted(outer, [400, 200]);
    assert_eq!(r.size(), vec2(100.0, 50.0));
    assert_eq!(r.center(), outer.center());
}

#[test]
fn the_histogram_says_what_it_shows() {
    let bins: Vec<f32> = (0..64).map(|i| (i * (63 - i)) as f32).collect();
    let mut h = Harness::builder()
        .with_size(vec2(320.0, 140.0))
        .build_ui(move |ui| {
            ui.add(
                Histogram::new([Channel::new("Luminance", &bins, Color32::GRAY)])
                    .log(true)
                    .size(vec2(300.0, 100.0)),
            );
        });
    h.run();
    let node = h.get_by_label("Histogram");
    assert_eq!(
        node.accesskit_node().value().as_deref(),
        Some("Luminance (64 bins, logarithmic)")
    );
}

/// A draggable histogram adjusting an exposure, four stops per width;
/// `veto` refuses every finished gesture.
fn exposure_histogram(veto: bool) -> Harness<'static, f64> {
    let bins: Vec<f32> = (0..64).map(|i| (i * (63 - i)) as f32).collect();
    let mut h = Harness::builder()
        .with_size(vec2(420.0, 160.0))
        .build_ui_state(
            move |ui, ev: &mut f64| {
                let out = Histogram::new([Channel::new("Luminance", &bins, Color32::GRAY)])
                    .clipping(Clipping {
                        shadows: 0.0,
                        highlights: 0.012,
                    })
                    .size(vec2(400.0, 120.0))
                    .on_change(move |c: &Change<Drag>| {
                        if veto && c.done {
                            Verdict::Veto
                        } else {
                            Verdict::Allow
                        }
                    })
                    .show(ui);
                *ev += f64::from(out.moved()) * 4.0;
            },
            0.0,
        );
    h.run();
    h
}

fn drag_by(h: &mut Harness<'_, f64>, from: egui::Pos2, dx: f32) {
    h.event(Event::PointerMoved(from));
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    for i in 1..=4 {
        h.event(Event::PointerMoved(from + vec2(dx * i as f32 / 4.0, 0.0)));
        h.step();
    }
    h.event(Event::PointerButton {
        pos: from + vec2(dx, 0.0),
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
}

#[test]
fn a_histogram_drag_is_handed_back_and_a_veto_gives_it_back() {
    let mut h = exposure_histogram(false);
    let node = h.get_by_label("Histogram");
    assert_eq!(node.accesskit_node().role(), egui::accesskit::Role::Slider);
    assert_eq!(
        node.accesskit_node().value().as_deref(),
        Some("Luminance (64 bins), highlights clipped: 1.2 %")
    );
    let rect = node.rect();
    drag_by(&mut h, rect.center(), rect.width() / 4.0);
    assert!((*h.state() - 1.0).abs() < 0.05, "a quarter: {}", h.state());
    // Focused by the press: the arrows step by a hundredth of a width.
    h.key_press(Key::ArrowLeft);
    h.run();
    assert!((*h.state() - 0.96).abs() < 0.05, "{}", h.state());

    let mut h = exposure_histogram(true);
    let rect = h.get_by_label("Histogram").rect();
    drag_by(&mut h, rect.center(), rect.width() / 4.0);
    assert!(h.state().abs() < 1e-6, "vetoed at the end: {}", h.state());
}
