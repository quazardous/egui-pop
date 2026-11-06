//! The wheel at a scroll area's end: chained by default, as egui does, and kept by
//! what asks to keep it (`UiPop::scroll_area_contained`, `ui::contain_wheel`).

use std::cell::Cell;
use std::rc::Rc;

use egui::{Pos2, ScrollArea, Ui, pos2, vec2};
use egui_kittest::Harness;
use egui_pop_core::thumbnail::ThumbnailGrid;
use egui_pop_core::ui::UiPop as _;

const OVER: Pos2 = pos2(100.0, 50.0);

/// Over `at`, a wheel by `delta` lines. In one gesture, as a touchpad's, so that
/// egui does not smooth it over many passes.
fn wheel(h: &mut Harness<'_>, at: Pos2, delta: egui::Vec2) {
    let event = |delta, phase| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta,
        modifiers: egui::Modifiers::NONE,
        phase,
        source: egui::MouseWheelSource::Wheel,
    };
    h.hover_at(at);
    h.run();
    h.event(event(egui::Vec2::ZERO, egui::TouchPhase::Start));
    h.event(event(delta, egui::TouchPhase::Move));
    h.run();
    h.event(event(egui::Vec2::ZERO, egui::TouchPhase::End));
    h.run();
}

/// `inner` above a tall page, in a scroll area on both axes whose place goes to
/// `outer`.
fn page<'a>(outer: Rc<Cell<egui::Vec2>>, inner: impl FnMut(&mut Ui) + 'a) -> Harness<'a> {
    let mut inner = inner;
    Harness::builder()
        .with_size(vec2(300.0, 300.0))
        .build_ui(move |ui| {
            let out = ScrollArea::both().id_salt("outer").show(ui, |ui| {
                ui.set_max_width(280.0);
                inner(ui);
                ui.allocate_space(vec2(2000.0, 2000.0));
            });
            outer.set(out.state.clamped_offset());
        })
}

fn tall(height: f32) -> ScrollArea {
    ScrollArea::vertical()
        .id_salt("inner")
        .max_height(height)
        .auto_shrink([false, true])
}

fn content(ui: &mut Ui, height: f32) {
    ui.allocate_space(vec2(100.0, height));
}

#[test]
fn a_plain_area_hands_the_wheel_on_at_its_end() {
    let outer = Rc::new(Cell::new(egui::Vec2::ZERO));
    let mut h = page(outer.clone(), |ui| {
        ui.scroll_area(tall(100.0), |ui| content(ui, 150.0));
    });
    h.run();
    for _ in 0..4 {
        wheel(&mut h, OVER, vec2(0.0, -4.0));
    }
    assert!(outer.get().y > 0.0, "{:?}", outer.get());
}

#[test]
fn a_contained_area_keeps_the_wheel_at_its_end() {
    let outer = Rc::new(Cell::new(egui::Vec2::ZERO));
    let inner = Rc::new(Cell::new(0.0));
    let seen = inner.clone();
    let mut h = page(outer.clone(), move |ui| {
        let out = ui.scroll_area_contained(tall(100.0), |ui| content(ui, 150.0));
        seen.set(out.state.clamped_offset().y);
    });
    h.run();
    for _ in 0..4 {
        wheel(&mut h, OVER, vec2(0.0, -4.0));
    }
    assert!(inner.get() > 0.0, "it moved itself");
    assert_eq!(outer.get(), egui::Vec2::ZERO);
    for _ in 0..4 {
        wheel(&mut h, OVER, vec2(0.0, 4.0));
    }
    assert_eq!(inner.get(), 0.0, "and back");
    assert_eq!(outer.get(), egui::Vec2::ZERO);
}

#[test]
fn a_contained_area_with_nothing_to_scroll_hands_the_wheel_on() {
    let outer = Rc::new(Cell::new(egui::Vec2::ZERO));
    let mut h = page(outer.clone(), |ui| {
        ui.scroll_area_contained(tall(100.0), |ui| content(ui, 150.0));
        ui.scroll_area_contained(tall(100.0).id_salt("short"), |ui| content(ui, 50.0));
    });
    h.run();
    // Below the first, over the second: it is shrunk to nothing but its content.
    wheel(&mut h, pos2(100.0, 125.0), vec2(0.0, -4.0));
    assert!(outer.get().y > 0.0, "{:?}", outer.get());
}

#[test]
fn a_contained_area_keeps_only_the_way_it_scrolls() {
    let outer = Rc::new(Cell::new(egui::Vec2::ZERO));
    let mut h = page(outer.clone(), |ui| {
        ui.scroll_area_contained(tall(100.0), |ui| content(ui, 150.0));
    });
    h.run();
    wheel(&mut h, OVER, vec2(-4.0, 0.0));
    assert!(outer.get().x > 0.0, "{:?}", outer.get());
    assert_eq!(outer.get().y, 0.0);
}

#[test]
fn a_bar_beside_the_content_keeps_the_wheel_too() {
    let outer = Rc::new(Cell::new(egui::Vec2::ZERO));
    let bar = Rc::new(Cell::new(0.0));
    let right = bar.clone();
    let mut h = page(outer.clone(), move |ui| {
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        let out = ui.scroll_area_contained(tall(100.0), |ui| content(ui, 150.0));
        right.set(out.inner_rect.right());
    });
    h.run();
    h.run();
    let over_the_bar = pos2(bar.get() + 5.0, 50.0);
    for _ in 0..4 {
        wheel(&mut h, over_the_bar, vec2(0.0, -4.0));
    }
    assert_eq!(outer.get(), egui::Vec2::ZERO);
}

#[test]
fn a_thumbnail_grid_keeps_the_wheel_at_its_end() {
    let outer = Rc::new(Cell::new(egui::Vec2::ZERO));
    let mut h = page(outer.clone(), |ui| {
        ThumbnailGrid::new(12, vec2(100.0, 40.0))
            .id_salt("grid")
            .max_height(100.0)
            .show(ui, |ui, _| {
                ui.label("cell");
            });
    });
    h.run();
    for _ in 0..8 {
        wheel(&mut h, OVER, vec2(0.0, -4.0));
    }
    assert_eq!(outer.get(), egui::Vec2::ZERO);
}

/// A popup is a layer of its own: what scrolls under it does not see the wheel,
/// so a popup or a dialog has no use for containment.
#[test]
fn a_wheel_over_a_popup_stays_out_of_what_is_under_it() {
    let outer = Rc::new(Cell::new(egui::Vec2::ZERO));
    let mut h = page(outer.clone(), |ui| {
        egui::Area::new(egui::Id::unique("popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(10.0, 10.0))
            .show(ui.ctx(), |ui| {
                ui.set_min_size(vec2(200.0, 100.0));
                ScrollArea::vertical()
                    .id_salt("in the popup")
                    .max_height(100.0)
                    .show(ui, |ui| content(ui, 150.0));
            });
    });
    h.run();
    h.run();
    for _ in 0..4 {
        wheel(&mut h, OVER, vec2(0.0, -4.0));
    }
    assert_eq!(outer.get(), egui::Vec2::ZERO);
}

/// A strip scrolling only sideways in a tall page, its place and the page's: under
/// egui-pop's theme when `themed`, else egui's own style.
fn strip(themed: bool, offsets: Rc<Cell<(f32, f32)>>) -> Harness<'static> {
    let h = Harness::builder()
        .with_size(vec2(300.0, 300.0))
        .build_ui(move |ui| {
            let mut strip = 0.0;
            let page = ScrollArea::vertical().id_salt("page").show(ui, |ui| {
                let area = ScrollArea::horizontal().id_salt("strip");
                let out = ui.scroll_area(area, |ui| {
                    ui.horizontal(|ui| {
                        for i in 0..40 {
                            ui.label(format!("Tab {i}"));
                        }
                    });
                });
                strip = out.state.clamped_offset().x;
                ui.allocate_space(vec2(100.0, 2000.0));
            });
            offsets.set((strip, page.state.clamped_offset().y));
        });
    if themed {
        egui_pop_core::theme::install(&h.ctx, &egui_pop_core::theme::ThemeSpec::new("pop-dark"));
    }
    h
}

#[test]
fn a_strip_scrolling_only_sideways_takes_the_plain_wheel() {
    let offsets = Rc::new(Cell::new((0.0, 0.0)));
    let mut h = strip(true, offsets.clone());
    h.run();
    wheel(&mut h, pos2(100.0, 10.0), vec2(0.0, -2.0));
    let (strip, page) = offsets.get();
    assert!(strip > 0.0, "the strip moved: {strip}");
    assert_eq!(page, 0.0, "and the page did not");
    // At its end, the wheel goes on to the page, as for any area.
    for _ in 0..40 {
        wheel(&mut h, pos2(100.0, 10.0), vec2(0.0, -4.0));
    }
    assert!(offsets.get().1 > 0.0, "{:?}", offsets.get());
}

/// What the theme changes: egui's own style wants Shift for a sideways strip.
#[test]
fn without_the_theme_a_sideways_strip_wants_shift() {
    let offsets = Rc::new(Cell::new((0.0, 0.0)));
    let mut h = strip(false, offsets.clone());
    h.run();
    wheel(&mut h, pos2(100.0, 10.0), vec2(0.0, -2.0));
    let (strip, page) = offsets.get();
    assert_eq!(strip, 0.0);
    assert!(page > 0.0, "{page}");
}

/// egui 0.37 lets a trackpad at an area's end go on to an area around it
/// (`has_scrolling_ancestor`), rather than stretching it: containment still keeps
/// it there.
#[test]
fn a_contained_area_keeps_a_trackpad_at_its_end_too() {
    let outer = Rc::new(Cell::new(egui::Vec2::ZERO));
    let inner = Rc::new(Cell::new(0.0));
    let seen = inner.clone();
    let mut h = page(outer.clone(), move |ui| {
        let out = ui.scroll_area_contained(tall(100.0), |ui| content(ui, 150.0));
        seen.set(out.state.clamped_offset().y);
    });
    h.run();
    let event = |delta, phase| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta,
        modifiers: egui::Modifiers::NONE,
        phase,
        source: egui::MouseWheelSource::Trackpad,
    };
    h.hover_at(OVER);
    h.run();
    h.event(event(egui::Vec2::ZERO, egui::TouchPhase::Start));
    for _ in 0..8 {
        h.event(event(vec2(0.0, -30.0), egui::TouchPhase::Move));
        h.step();
    }
    h.event(event(egui::Vec2::ZERO, egui::TouchPhase::End));
    h.run_steps(30);
    assert!(inner.get() > 0.0, "it moved itself");
    assert_eq!(outer.get(), egui::Vec2::ZERO);
}
