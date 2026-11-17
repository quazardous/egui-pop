//! The comparison's zoom and pan driven as a user would (kittest): the
//! wheel does what it does in the image viewport (one wheel path, the
//! pointer profile's), and synced panes move together, the image point
//! under the pointer staying under it in every pane, whatever the
//! photos' shapes and the panes' sizes.

use egui::{Event, Modifiers, MouseWheelUnit, Pos2, Rect, TextureId, Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::model::pointer::{self, PointerMap};
// The image viewport's own zoom factor per wheel motion.
use egui_pop_core::pointer::view::wheel_factor;
use egui_pop_views::library::compare::{CompareState, CompareView, Pane};
use egui_pop_views::library::grid::{Thumb, ThumbSource};

/// Photos of these sizes, ready, each its own texture.
struct Photos(Vec<[u32; 2]>);

impl ThumbSource<u64> for Photos {
    fn thumb(&self, key: &u64, _: u32) -> Thumb {
        Thumb::Ready {
            texture: TextureId::User(*key),
            size: self.0[*key as usize],
        }
    }
}

struct Bench {
    state: CompareState,
    photos: Vec<[u32; 2]>,
}

fn harness(photos: Vec<[u32; 2]>, size: Vec2) -> Harness<'static, Bench> {
    let mut h = Harness::builder().with_size(size).build_ui_state(
        |ui, b: &mut Bench| {
            let panes: Vec<Pane<u64>> = (0..b.photos.len())
                .map(|i| Pane::new(i as u64, format!("P{i}")))
                .collect();
            let _ = CompareView::new("compare").show(
                ui,
                &mut b.state,
                &panes,
                &Photos(b.photos.clone()),
            );
        },
        Bench {
            state: CompareState::new(),
            photos,
        },
    );
    h.run();
    h
}

fn use_profile(h: &Harness<'_, Bench>, id: &str) {
    let p = pointer::profile(id).unwrap();
    egui_pop_core::pointer::set_map(&h.ctx, PointerMap::from_profile(p));
}

fn pane(h: &Harness<'_, Bench>, i: usize) -> Rect {
    h.get_by_label(&format!("P{i}")).rect()
}

/// Where pane `i`'s photo is drawn.
fn image(h: &Harness<'_, Bench>, i: usize) -> Rect {
    h.output()
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::Shape::Mesh(m) if m.texture_id == TextureId::User(i as u64) => {
                Some(m.calc_bounds())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("pane {i}'s image is drawn"))
}

fn wheel(h: &mut Harness<'_, Bench>, at: Pos2, lines: f32, modifiers: Modifiers) {
    h.event(Event::PointerMoved(at));
    h.run();
    h.event(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, lines),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers,
    });
    h.run();
}

/// The points a wheel of `lines` moves by, as the views read it.
fn points(h: &Harness<'_, Bench>, lines: f32) -> f32 {
    lines * h.ctx.options(|o| o.input_options.line_scroll_speed)
}

#[test]
fn the_wheel_zooms_a_pane_as_it_zooms_the_image_viewport() {
    let mut h = harness(vec![[3000, 2000], [3000, 2000]], vec2(800.0, 400.0));
    // darktable's profile: the wheel alone zooms the image canvas.
    use_profile(&h, "darktable");
    h.run();
    let at = pane(&h, 0).center() + vec2(30.0, 10.0);
    wheel(&mut h, at, 2.0, Modifiers::NONE);
    let want = wheel_factor(points(&h, 2.0)) as f32;
    let got = h.state().state.view(0, true).zoom;
    assert!(
        (got - want).abs() < 1e-4,
        "the wheel alone zooms by the viewport's factor: {got} != {want}"
    );

    // The default profile: Ctrl and the wheel zoom, the wheel alone pans.
    let mut h = harness(vec![[3000, 2000], [3000, 2000]], vec2(800.0, 400.0));
    use_profile(&h, pointer::DEFAULT_PROFILE);
    h.run();
    wheel(&mut h, at, 2.0, Modifiers::NONE);
    assert_eq!(
        h.state().state.view(0, true).zoom,
        1.0,
        "the wheel alone pans"
    );
    wheel(&mut h, at, 2.0, Modifiers::COMMAND);
    let got = h.state().state.view(0, true).zoom;
    assert!((got - want).abs() < 1e-4, "Ctrl+wheel: {got} != {want}");
}

/// Zooms in, drags `steps` times over pane 0, and checks that in every
/// pane the image point under the pointer (at the same place in each
/// pane) stayed under it within half a point, and that photos of the same
/// shape show the same point.
fn synced_drag_keeps_every_pane_under_the_pointer(photos: Vec<[u32; 2]>, size: Vec2) {
    let n = photos.len();
    let same: Vec<[u32; 2]> = photos.clone();
    let mut h = harness(photos, size);
    use_profile(&h, pointer::DEFAULT_PROFILE);
    h.run();
    let a0 = pane(&h, 0);
    let p = a0.center() + vec2(a0.width() * 0.2, -a0.height() * 0.15);
    wheel(&mut h, p, 6.0, Modifiers::COMMAND);
    assert!(h.state().state.view(0, true).zoom > 1.2, "zoomed in");

    // The image point (0..1 across the photo) under `at`'s place in pane
    // `j`.
    let under = |h: &Harness<'_, Bench>, j: usize, at: Pos2| {
        let q = pane(h, j).min + (at - pane(h, 0).min);
        let r = image(h, j);
        ((q - r.min) / r.size(), r.size())
    };
    let before: Vec<Vec2> = (0..n).map(|j| under(&h, j, p).0).collect();

    let step = vec2(6.7, -4.3);
    let steps = 15;
    h.event(Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run();
    let mut at = p;
    for _ in 0..steps {
        at += step;
        h.event(Event::PointerMoved(at));
        h.run();
    }
    h.event(Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    h.run();

    for j in 0..n {
        let (uv, img) = under(&h, j, at);
        let off = ((uv - before[j]) * img).length();
        assert!(
            off < 0.5,
            "pane {j} ({:?}) drifted {off} pt from the pointer after {steps} drags",
            same[j]
        );
    }
    for j in 1..n {
        if same[j] == same[0] {
            let (a, img) = under(&h, 0, at);
            let (b, _) = under(&h, j, at);
            let off = ((a - b) * img).length();
            assert!(
                off < 0.5,
                "panes 0 and {j} show the same point: {off} pt apart"
            );
        }
    }
}

#[test]
fn synced_drags_do_not_drift_four_photos_of_different_shapes() {
    synced_drag_keeps_every_pane_under_the_pointer(
        vec![[3000, 2000], [3000, 2000], [2000, 3000], [1000, 1000]],
        vec2(800.0, 600.0),
    );
}

#[test]
fn synced_drags_do_not_drift_in_other_pane_sizes() {
    // Three panes in a row, then two, in another window: other pane sizes.
    synced_drag_keeps_every_pane_under_the_pointer(
        vec![[4000, 3000], [4000, 3000], [3000, 4500]],
        vec2(900.0, 360.0),
    );
    synced_drag_keeps_every_pane_under_the_pointer(
        vec![[6000, 4000], [5472, 3648]],
        vec2(700.0, 500.0),
    );
}

#[test]
fn a_drag_follows_the_pointer_from_the_press() {
    // Nothing of the motion is lost to the drag threshold.
    let mut h = harness(vec![[3000, 2000], [3000, 2000]], vec2(800.0, 400.0));
    let p = pane(&h, 0).center();
    let r0 = image(&h, 0);
    h.event(Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run();
    let to = p + vec2(40.0, 0.0);
    h.event(Event::PointerMoved(to));
    h.run();
    h.run();
    let moved = image(&h, 0).center() - r0.center();
    assert!(
        (moved - vec2(40.0, 0.0)).length() < 0.5,
        "the image moved {moved:?} for a 40 pt drag"
    );
}
