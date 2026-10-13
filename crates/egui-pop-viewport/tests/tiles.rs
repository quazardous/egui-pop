//! Tiles through the widget: proxy then exact, cancellation, live
//! changes, the budget, the upload budget and the conversion on the
//! delivering thread, the texture limit, and pixels drawn 1:1 on the
//! CPU and the GPU paths. Nothing sleeps: the test source serves when
//! told, and the procedural source is waited on through its own signal.

use egui::{Key, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_test::viewport::TestSource;
use egui_pop_viewport::source::Quality;
use egui_pop_viewport::tiles::UploadBudget;
use egui_pop_viewport::{Signal, View, Viewport, ViewportState};

struct Scene {
    state: ViewportState,
    view: View,
    source: TestSource,
    signals: Vec<Signal>,
}

fn scene(view: View, manual: bool, scene_pixels: bool) -> Scene {
    let mut state = ViewportState::new();
    state.set_settle(0.0);
    let sink = state.sink();
    let mut source = if manual {
        TestSource::manual(sink, [1000, 800])
    } else {
        TestSource::immediate(sink, [1000, 800])
    };
    if scene_pixels {
        source = source.scene();
    }
    Scene {
        state,
        view,
        source,
        signals: Vec::new(),
    }
}

fn draw(ui: &mut egui::Ui, s: &mut Scene) {
    let out =
        Viewport::new("t")
            .source(&s.source)
            .pixel_grid(false)
            .show(ui, &mut s.state, &mut s.view);
    s.signals.extend(out.signals);
}

fn harness(s: Scene, ppp: f32) -> Harness<'static, Scene> {
    Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .with_pixels_per_point(ppp)
        .build_ui_state(draw, s)
}

fn wgpu_harness(s: Scene, ppp: f32, gpu: bool) -> Harness<'static, Scene> {
    // The process's one device (see egui_pop_test::shared_setup).
    let rs = {
        let setup = egui_pop_test::shared_setup();
        let _guard = egui_pop_test::gpu_lock();
        egui_kittest::wgpu::create_render_state(
            egui_pop_viewport::gpu::egui_wgpu::WgpuSetup::Existing(setup),
            egui_pop_viewport::gpu::egui_wgpu::RendererOptions::PREDICTABLE,
        )
    };
    let renderer = egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs.clone());
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .with_pixels_per_point(ppp)
        .renderer(renderer)
        .build_ui_state(draw, s);
    if gpu {
        egui_pop_viewport::gpu::install(&h.ctx, &rs);
    }
    h.run();
    h
}

/// Steps until every wanted tile is on screen (the test source serving
/// what is pending each time).
fn settle(h: &mut Harness<'_, Scene>) {
    for _ in 0..20 {
        h.run();
        if h.state().state.tiles_complete() {
            return;
        }
        h.state().source.deliver_all();
    }
    panic!("the tiles never completed");
}

#[test]
fn the_proxy_is_asked_for_first_then_the_exact_tiles() {
    let mut h = harness(scene(View::at(0.5, [500.0, 400.0]), true, false), 1.0);
    h.run();
    let pending = h.state().source.pending();
    assert!(!pending.is_empty());
    let first_exact = pending.iter().position(|r| r.quality == Quality::Exact);
    let last_proxy = pending.iter().rposition(|r| r.quality == Quality::Proxy);
    assert!(last_proxy.unwrap() < first_exact.unwrap(), "proxy first");
    // The exact tiles are at the view's scale (one texel per physical
    // pixel), the proxy at a power of two at or below fit.
    assert!(
        pending
            .iter()
            .any(|r| r.quality == Quality::Exact && r.scale() == 0.5)
    );
    assert!(
        pending
            .iter()
            .any(|r| r.quality == Quality::Proxy && r.scale() == 0.25)
    );
    settle(&mut h);
    let st = h.state().state.stats().snapshot();
    assert_eq!(st[2], ("dropped", 0));
}

#[test]
fn panning_on_cancels_the_requests_no_longer_wanted() {
    let mut h = harness(scene(View::at(2.0, [100.0, 100.0]), true, false), 1.0);
    h.run();
    let first = h.state().source.pending();
    assert!(first.iter().any(|r| r.quality == Quality::Exact));
    h.get_by_label("Image viewport").click();
    h.run();
    // Far away: none of the first exact tiles is wanted any more.
    for _ in 0..6 {
        h.key_press_modifiers(egui::Modifiers::SHIFT, Key::ArrowLeft);
        h.run();
    }
    assert!(
        first
            .iter()
            .filter(|r| r.quality == Quality::Exact)
            .all(|r| r.is_cancelled())
    );
    h.state().source.deliver_all();
    assert!(h.state().source.cancelled() > 0);
    settle(&mut h);
    let st = h.state().state.stats();
    assert!(st.snapshot()[3].1 > 0, "{:?}", st.snapshot());
    assert!(
        h.state()
            .signals
            .iter()
            .any(|s| matches!(s, Signal::Tiles { cancelled, .. } if *cancelled > 0))
    );
}

#[test]
fn a_change_asks_again_for_its_region_only_and_keeps_the_old_tiles_meanwhile() {
    let mut h = harness(scene(View::actual([500.0, 400.0]), true, false), 1.0);
    settle(&mut h);
    let held = h.state().state.tile_count();
    let before = h.state().source.requested();
    h.state().source.change(Some([0.0, 0.0, 10.0, 10.0]));
    h.run();
    // Only the tiles over the changed corner are asked for again...
    let again = h.state().source.requested() - before;
    assert!((1..=2).contains(&again), "{again}");
    // ... and nothing was thrown away while waiting.
    assert_eq!(h.state().state.tile_count(), held);
    assert!(!h.state().state.tiles_complete());
    settle(&mut h);
}

#[test]
fn the_budget_evicts_tiles_not_drawn() {
    let mut s = scene(View::actual([128.0, 128.0]), false, false);
    // About four 256-pixel tiles.
    s.state.set_texture_budget(4 * 256 * 256 * 4);
    let mut h = harness(s, 1.0);
    settle(&mut h);
    h.get_by_label("Image viewport").click();
    for _ in 0..8 {
        h.key_press_modifiers(egui::Modifiers::SHIFT, Key::ArrowLeft);
        h.key_press_modifiers(egui::Modifiers::SHIFT, Key::ArrowUp);
        settle(&mut h);
    }
    let st = h.state().state.stats().snapshot();
    assert!(st[5].1 > 0, "{st:?}");
}

#[test]
fn leftover_tiles_survive_a_pan() {
    let mut s = scene(View::at(1.0, [200.0, 150.0]), true, true);
    s.state.set_upload_budget(UploadBudget::tiles(1));
    s.state.set_prepare_on_worker(true);
    let mut h = harness(s, 1.0);
    h.step();
    let asked = h.state().source.pending().len();
    assert!(asked >= 3, "{asked}");
    // Everything arrives at once (converted on this, the delivering,
    // thread); one tile a frame goes up.
    assert_eq!(h.state().source.deliver_all(), asked);
    h.step();
    let report = h.state().state.upload_report();
    assert_eq!((report.tiles, report.waiting), (1, asked - 1));
    // Far away: none of the waiting exact tiles is visible any more.
    h.state_mut().view = View::at(1.0, [800.0, 650.0]);
    for _ in 0..asked {
        h.step();
        if h.state().state.upload_report().waiting == 0 {
            break;
        }
    }
    assert_eq!(h.state().state.upload_report().waiting, 0);
    // None was dropped: the old view's tiles went up after the pan.
    let st = h.state().state.stats().snapshot();
    assert_eq!(st[1], ("delivered", asked as u64), "{st:?}");
    assert_eq!(st[2], ("dropped", 0), "{st:?}");
    assert!(h.state().state.tile_count() >= asked);
    // The new view's tiles, asked for meanwhile, come in turn.
    for _ in 0..20 {
        h.state().source.deliver_all();
        h.step();
        if h.state().state.tiles_complete() {
            break;
        }
    }
    assert!(h.state().state.tiles_complete());
}

#[test]
fn tiles_converted_on_the_delivering_thread_draw_the_same_pixels() {
    for scene_pixels in [false, true] {
        let mut s = scene(View::actual([300.3, 200.7]), false, scene_pixels);
        s.state.set_prepare_on_worker(true);
        // About two tiles a frame.
        s.state.set_upload_budget(UploadBudget::bytes(600_000));
        let mut h = wgpu_harness(s, 1.0, false);
        settle(&mut h);
        assert_one_to_one(&mut h, 1.0, i32::from(scene_pixels));
    }
}

#[test]
fn tiles_never_exceed_the_largest_texture_side() {
    let mut s = scene(View::actual([3000.0, 2500.0]), true, false);
    s.source = TestSource::manual(s.state.sink(), [6000, 5000]);
    s.state.set_tile_size(8192);
    let mut h = harness(s, 1.0);
    h.input_mut().max_texture_side = Some(2048);
    h.run();
    h.run();
    let pending = h.state().source.pending();
    assert!(!pending.is_empty());
    assert!(
        pending
            .iter()
            .all(|r| r.size[0] <= 2048 && r.size[1] <= 2048)
    );
}

/// Walks the physical pixels of a row through the middle of the view and
/// checks each shows the colour of the image pixel the mapping says.
fn assert_one_to_one(h: &mut Harness<'_, Scene>, ppp: f32, tolerance: i32) {
    let image = h.render().expect("render");
    let m = h.state().state.mapping().unwrap();
    let y = ((m.rect_px[1] + m.rect_px[3]) / 2.0) as u32;
    let mut checked = 0;
    for x in (m.rect_px[0] as u32 + 2)..(m.rect_px[2] as u32 - 2) {
        let pos = pos2((x as f32 + 0.5) / ppp, (y as f32 + 0.5) / ppp);
        let Some([ix, iy]) = m.pixel_at(pos) else {
            continue;
        };
        let want = TestSource::pixel(ix, iy, 0);
        let got = image.get_pixel(x, y).0;
        for c in 0..3 {
            assert!(
                (i32::from(got[c]) - i32::from(want[c])).abs() <= tolerance,
                "physical ({x}, {y}) shows {got:?}, image pixel ({ix}, {iy}) is {want:?}"
            );
        }
        checked += 1;
    }
    assert!(checked > 100);
}

#[test]
fn at_100_percent_every_physical_pixel_shows_its_image_pixel_cpu_path() {
    for ppp in [1.0, 2.0] {
        let mut h = wgpu_harness(
            scene(View::actual([300.3, 200.7]), false, false),
            ppp,
            false,
        );
        settle(&mut h);
        assert_one_to_one(&mut h, ppp, 0);
    }
}

#[test]
fn at_100_percent_every_physical_pixel_shows_its_image_pixel_gpu_path() {
    for ppp in [1.0, 1.5] {
        let mut h = wgpu_harness(scene(View::actual([300.3, 200.7]), false, true), ppp, true);
        settle(&mut h);
        // Half floats and the shader's encoding: within one level.
        assert_one_to_one(&mut h, ppp, 1);
    }
}

#[test]
fn at_200_percent_each_image_pixel_is_a_crisp_square() {
    let mut h = wgpu_harness(
        scene(View::at(2.0, [300.0, 200.0]), false, false),
        1.0,
        false,
    );
    settle(&mut h);
    assert_one_to_one(&mut h, 1.0, 0);
}

#[test]
fn the_procedural_source_is_waited_on_without_sleeping() {
    use egui_pop_viewport::demo::Procedural;
    let mut state = ViewportState::new();
    state.set_settle(0.0);
    let source = Procedural::new(state.sink(), [2000, 1500], 2);
    let mut view = View::fit();
    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let seen = done.clone();
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui(move |ui| {
            Viewport::new("p")
                .source(&source)
                .show(ui, &mut state, &mut view);
            seen.set(state.tiles_complete());
            if !state.tiles_complete() {
                // The source's own signal: every request taken answered.
                source.wait_idle();
                ui.ctx().request_repaint();
            }
        });
    h.run();
    assert!(done.get());
}
