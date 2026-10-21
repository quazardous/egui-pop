//! The loupe: its sampling (the pixel under the pointer at its centre,
//! crisp blocks), its place beside the pointer, its readout, and the
//! viewport drawing it.

use egui::{Key, Pos2, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_pop_test::viewport::TestSource;
use egui_pop_viewport::display::{DisplayTarget, DisplayTransform, LinearSrgb};
use egui_pop_viewport::loupe::{self, Loupe, Sampling, Values};
use egui_pop_viewport::source::{Readout, Side};
use egui_pop_viewport::{Mapping, Signal, View, Viewport, ViewportState};

fn mapping(zoom: f64, ppp: f32) -> Mapping {
    let view = View::at(zoom, [500.0, 400.0]);
    Mapping::new(
        &view,
        [1000.0, 800.0],
        Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0)),
        ppp,
    )
}

#[test]
fn above_100_percent_the_image_pixel_under_the_pointer_fills_the_centre_block() {
    for (zoom, ppp) in [(1.0, 1.0), (3.0, 1.0), (4.0, 1.5), (2.5, 2.0)] {
        let m = mapping(zoom, ppp);
        let pointer = pos2(123.4, 87.9);
        let s = Sampling::new(&m, pointer, 8, pos2(300.0, 60.0));
        let pixel = m.pixel_at(pointer).unwrap();
        assert_eq!(s.pixel, Some(pixel));
        assert_eq!(s.texel, zoom);
        // The block is whole physical pixels: 8 points.
        assert_eq!(s.block, (8.0 * f64::from(ppp)).round());
        // The pixel's square lands on the centre block, on whole pixels.
        let corner = s.map(m.image_to_px([pixel[0] as f64, pixel[1] as f64]));
        let block = s.center_block();
        assert!((corner[0] - block[0]).abs() < 1e-9, "{zoom} {ppp}");
        assert!((corner[1] - block[1]).abs() < 1e-9);
        assert_eq!(block[0], block[0].round());
        assert_eq!(block[1], block[1].round());
        // The next pixel is the next block.
        let next = s.map(m.image_to_px([pixel[0] as f64 + 1.0, pixel[1] as f64]));
        assert!((next[0] - (block[0] + s.block)).abs() < 1e-9);
    }
}

#[test]
fn below_100_percent_each_screen_pixel_becomes_a_block() {
    let m = mapping(0.25, 1.0);
    let pointer = pos2(200.7, 150.2);
    let s = Sampling::new(&m, pointer, 6, pos2(80.0, 80.0));
    assert_eq!(s.texel, 1.0);
    assert_eq!(s.center, [200.5, 150.5]);
    assert_eq!(s.scale(), 6.0);
    let block = s.center_block();
    assert_eq!(s.map([200.0, 150.0]), [block[0], block[1]]);
    assert_eq!(s.map([201.0, 151.0]), [block[2], block[3]]);
}

#[test]
fn the_grid_falls_between_the_blocks() {
    let m = mapping(2.0, 1.0);
    let s = Sampling::new(&m, pos2(100.0, 100.0), 10, pos2(60.0, 60.0));
    let block = s.center_block();
    let (xs, ys) = s.grid_lines([
        block[0] - 25.0,
        block[1] - 25.0,
        block[2] + 25.0,
        block[3] + 25.0,
    ]);
    assert!(xs.contains(&block[0]) && xs.contains(&block[2]));
    assert!(ys.contains(&block[1]) && ys.contains(&block[3]));
    for w in xs.windows(2) {
        assert_eq!(w[1] - w[0], s.block);
    }
}

#[test]
fn the_loupe_sits_above_right_and_flips_at_the_edges() {
    let bounds = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
    let size = vec2(100.0, 100.0);
    let r = loupe::place(pos2(150.0, 200.0), size, bounds, 20.0);
    assert_eq!(r.min, pos2(170.0, 80.0));
    // Near the right edge: to the left.
    let r = loupe::place(pos2(350.0, 200.0), size, bounds, 20.0);
    assert_eq!(r.max.x, 330.0);
    // Near the top: below.
    let r = loupe::place(pos2(150.0, 50.0), size, bounds, 20.0);
    assert_eq!(r.min.y, 70.0);
    // Too small a viewport: kept inside as much as it can.
    let small = Rect::from_min_size(pos2(0.0, 0.0), vec2(120.0, 120.0));
    let r = loupe::place(pos2(60.0, 60.0), size, small, 20.0);
    assert!(small.contains_rect(r));
}

#[test]
fn the_readout_shows_displayed_values_or_the_sources() {
    let readout = Readout {
        side: Side::After,
        pixel: [12, 34],
        values: vec![("R".into(), 0.18), ("G".into(), 0.18), ("B".into(), 0.18)],
    };
    let stage = LinearSrgb.stage(&DisplayTarget::srgb());
    let labels = ["display", "scene", "no value"];
    let lines = loupe::readout_lines(
        Some([12, 34]),
        Some(&readout),
        Values::Display,
        Some(&stage),
        labels,
    );
    assert_eq!(lines[0], "12, 34  display");
    assert_eq!(lines[1], "RGB 118 118 118");
    assert!(
        lines[2].starts_with("Lab 49 0 0") || lines[2].starts_with("Lab 50 0 0"),
        "{lines:?}"
    );
    let lines = loupe::readout_lines(Some([12, 34]), Some(&readout), Values::Scene, None, labels);
    assert_eq!(lines[1..], ["R 0.180", "G 0.180", "B 0.180"]);
    assert!(!loupe::stale(Some([12, 34]), Some(&readout)));
    assert!(loupe::readout_lines(Some([1, 1]), None, Values::Off, None, labels).is_empty());
}

#[test]
fn a_lagging_readout_keeps_its_values_and_is_stale() {
    let readout = Readout {
        side: Side::After,
        pixel: [12, 34],
        values: vec![("R".into(), 0.18), ("G".into(), 0.18), ("B".into(), 0.18)],
    };
    let stage = LinearSrgb.stage(&DisplayTarget::srgb());
    let labels = ["display", "scene", "no value"];
    let fresh = loupe::readout_lines(
        Some([12, 34]),
        Some(&readout),
        Values::Display,
        Some(&stage),
        labels,
    );
    // The pointer moved on: the position follows it, the values stay.
    let lagging = loupe::readout_lines(
        Some([13, 34]),
        Some(&readout),
        Values::Display,
        Some(&stage),
        labels,
    );
    assert_eq!(lagging.len(), 3, "{lagging:?}");
    assert_eq!(lagging[0], "13, 34  display");
    assert_eq!(lagging[1..], fresh[1..]);
    assert!(loupe::stale(Some([13, 34]), Some(&readout)));
    // Off the image: one line, nothing stale.
    let off = loupe::readout_lines(None, Some(&readout), Values::Display, Some(&stage), labels);
    assert_eq!(off, ["no value"]);
    assert!(!loupe::stale(None, Some(&readout)));
    // No readout yet: the values say so.
    let none = loupe::readout_lines(Some([1, 1]), None, Values::Scene, None, labels);
    assert_eq!(none, ["1, 1  scene", "no value"]);
    assert!(!loupe::stale(Some([1, 1]), None));
}

#[test]
fn the_readout_templates_hold_the_widest_values_line_by_line() {
    let labels = ["display", "scene", "no value"];
    assert!(loupe::readout_templates(Values::Off, labels, &[]).is_empty());
    let t = loupe::readout_templates(Values::Display, labels, &[]);
    assert_eq!(
        t,
        [
            "-00000, -00000  display",
            "RGB 255 255 255",
            "Lab 100 -128 -128"
        ]
    );
    let names: Vec<String> = ["R", "G", "B"].map(String::from).to_vec();
    let t = loupe::readout_templates(Values::Scene, labels, &names);
    assert_eq!(t.len(), 4, "the position and one line per value");
    assert_eq!(t[0], "-00000, -00000  scene");
    assert_eq!(t[1], "R -0000.000");
    // Before any readout: one value line, as wide as the none label.
    let long = ["display", "scene", "nothing to show here"];
    let t = loupe::readout_templates(Values::Scene, long, &[]);
    assert_eq!(t.len(), 2);
    assert_eq!(t[1], "nothing to show here");
    // Every line fits what can be shown in it.
    let stage = LinearSrgb.stage(&DisplayTarget::srgb());
    let readout = Readout {
        side: Side::After,
        pixel: [-12345, 99999],
        values: vec![("R".into(), 1.0), ("G".into(), 0.0), ("B".into(), 0.0)],
    };
    let t = loupe::readout_templates(Values::Display, labels, &[]);
    let lines = loupe::readout_lines(
        Some(readout.pixel),
        Some(&readout),
        Values::Display,
        Some(&stage),
        labels,
    );
    assert_eq!(lines.len(), t.len());
    for (l, w) in lines.iter().zip(&t) {
        assert!(l.chars().count() <= w.chars().count(), "{l:?} {w:?}");
    }
}

#[test]
fn lab_matches_reference_values() {
    let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.2);
    assert!(close(
        loupe::lab_from_srgb8([255, 255, 255]),
        [100.0, 0.0, 0.0]
    ));
    assert!(close(loupe::lab_from_srgb8([0, 0, 0]), [0.0, 0.0, 0.0]));
    assert!(close(
        loupe::lab_from_srgb8([255, 0, 0]),
        [53.24, 80.09, 67.2]
    ));
    assert!(close(
        loupe::lab_from_srgb8([119, 119, 119]),
        [50.0, 0.0, 0.0]
    ));
}

struct Scene {
    state: ViewportState,
    view: View,
    source: TestSource,
    signals: Vec<Signal>,
}

fn scene() -> Scene {
    let mut state = ViewportState::new();
    state.set_settle(0.0);
    let source = TestSource::immediate(state.sink(), [1000, 800]);
    Scene {
        state,
        view: View::at(2.0, [500.0, 400.0]),
        source,
        signals: Vec::new(),
    }
}

fn harness(s: Scene) -> Harness<'static, Scene> {
    Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui_state(
            |ui, s: &mut Scene| {
                let out = Viewport::new("l")
                    .source(&s.source)
                    .show(ui, &mut s.state, &mut s.view);
                s.signals.extend(out.signals);
            },
            s,
        )
}

#[test]
fn a_key_turns_the_loupe_on_and_it_redraws_the_tiles_under_the_pointer() {
    let mut s = scene();
    *s.state.loupe_mut() = Loupe {
        key: Some(Key::L),
        values: Values::Scene,
        ..Loupe::default()
    };
    let mut h = harness(s);
    h.run();
    let at = Pos2::new(100.0, 250.0);
    h.hover_at(at);
    h.run();
    assert!(h.state().state.loupe_frame().is_none(), "off at first");
    h.key_press(Key::L);
    h.run();
    assert!(h.state().signals.contains(&Signal::Loupe { on: true }));
    h.hover_at(at + vec2(1.0, 0.0));
    h.run();
    let st = &h.state().state;
    let f = st.loupe_frame().expect("the loupe shows");
    let m = st.mapping().unwrap();
    assert_eq!(f.sampling.pixel, m.pixel_at(at + vec2(1.0, 0.0)));
    assert!(f.tiles > 0, "tiles drawn again");
    assert!(!f.gpu, "no GPU path installed here");
    // Above and to the right of the pointer, inside the viewport.
    assert!(f.rect.left() > at.x && f.rect.bottom() < at.y);
    assert!(m.rect.contains_rect(f.rect));
    // The source's values for that pixel, once they arrived.
    h.run();
    let f = h.state().state.loupe_frame().unwrap();
    assert!(f.lines[0].ends_with("scene"), "{:?}", f.lines);
    // The same key turns it off.
    h.key_press(Key::L);
    h.run();
    assert!(h.state().state.loupe_frame().is_none());
    assert!(!h.state().state.loupe().on);
}

#[test]
fn near_the_right_edge_the_loupe_flips_left() {
    let mut s = scene();
    s.state.loupe_mut().on = true;
    let mut h = harness(s);
    h.run();
    let at = Pos2::new(380.0, 250.0);
    h.hover_at(at);
    h.run();
    let f = h.state().state.loupe_frame().expect("shown").clone();
    assert!(f.rect.right() < at.x, "{:?}", f.rect);
    assert_eq!(f.sampling.block, 8.0);
}

#[test]
fn the_loupe_keeps_its_size_while_the_pointer_crosses_pixels() {
    for values in [Values::Display, Values::Scene] {
        let mut s = scene();
        *s.state.loupe_mut() = Loupe {
            on: true,
            values,
            ..Loupe::default()
        };
        let mut h = harness(s);
        h.run();
        // Low, so the loupe goes above the pointer: a change of height
        // would move it.
        let start = Pos2::new(150.0, 285.0);
        h.hover_at(start);
        h.run();
        h.run();
        let first = h.state().state.loupe_frame().expect("shown").clone();
        assert!(!first.stale, "{:?}", first.lines);
        let mut saw_stale = false;
        // At 200 %, each step of 6 points is another pixel, and the walk
        // crosses image x 512, where red (x % 256) drops from 255 to 0.
        for k in 1..24 {
            let at = start + vec2(6.0 * k as f32, 0.0);
            h.hover_at(at);
            // The frame the pointer moves in: the values are the last
            // pixel's, then this one's once the source answered.
            for settle in [false, true] {
                if settle {
                    h.run();
                } else {
                    h.step();
                }
                let f = h.state().state.loupe_frame().expect("shown").clone();
                saw_stale |= f.stale;
                assert_eq!(
                    f.rect.size(),
                    first.rect.size(),
                    "{values:?} {k} {:?}",
                    f.lines
                );
                assert_eq!(f.lines.len(), first.lines.len(), "{values:?} {:?}", f.lines);
                assert_eq!(f.rect.bottom(), at.y - 24.0, "it sits above the pointer");
            }
        }
        assert!(saw_stale, "the readout lags the pointer for a frame");
    }
}
