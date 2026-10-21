//! Device tiles: a source that creates its tiles on the harness's wgpu
//! device (textures of several formats, storage buffers packed or cut
//! from a larger image) is drawn as the CPU path draws the same image,
//! with no tile byte uploaded by the viewport; the loupe and a mask's
//! tint draw over them as over uploaded tiles; a tile from another
//! device, or without a GPU path, is refused with the reason and falls
//! back to the source's pixels. Every test needs a wgpu adapter (lavapipe
//! is enough).

use std::future::Future as _;

use egui::{Color32, Pos2, vec2};
use egui_kittest::Harness;
use egui_pop_test::viewport::TestSource;
use egui_pop_viewport::device::{BufferLayout, DeviceTile, Encoding};
use egui_pop_viewport::gpu::egui_wgpu::{self, wgpu};
use egui_pop_viewport::source::{Pixels, TileRequest, TileSink, TileSource};
use egui_pop_viewport::{Mapping, View, Viewport, ViewportState};

const IMAGE: [u32; 2] = [1000, 800];

/// What the device source hands over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Scene floats in an `Rgba32Float` texture.
    SceneTexture,
    /// Display bytes in an `Rgba8UnormSrgb` texture.
    DisplaySrgbTexture,
    /// Display bytes in an `Rgba8Unorm` texture.
    DisplayTexture,
    /// Scene floats in a packed RGBA storage buffer.
    PackedBuffer,
    /// Scene floats, RGB, cut from a larger buffer at an offset.
    CutBuffer,
}

/// The image of `TestSource` (`TestSource::pixel`), rendered by the
/// source into resources on `device`, as soon as asked.
struct DeviceSource {
    sink: TileSink,
    device: wgpu::Device,
    queue: wgpu::Queue,
    kind: Kind,
    /// Offers CPU pixels when refused.
    fallback: bool,
}

/// The display bytes of a tile, as `TestSource` serves them.
fn display_bytes(r: &TileRequest) -> Vec<u8> {
    let [w, h] = r.size;
    let mut bytes = Vec::with_capacity(w as usize * h as usize * 4);
    for j in 0..h {
        for i in 0..w {
            let p = r.image_point(i, j);
            bytes.extend(TestSource::pixel(
                p[0].floor() as i64,
                p[1].floor() as i64,
                0,
            ));
        }
    }
    bytes
}

/// The scene floats of a tile, as `TestSource::scene` serves them.
fn scene_floats(r: &TileRequest) -> Vec<f32> {
    display_bytes(r)
        .chunks_exact(4)
        .flat_map(|c| {
            let lin = egui::ecolor::linear_f32_from_gamma_u8;
            [lin(c[0]), lin(c[1]), lin(c[2]), f32::from(c[3]) / 255.0]
        })
        .collect()
}

fn floats_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

impl DeviceSource {
    fn texture(
        &self,
        r: &TileRequest,
        format: wgpu::TextureFormat,
        bytes: &[u8],
        bpp: u32,
    ) -> wgpu::Texture {
        let extent = wgpu::Extent3d {
            width: r.size[0],
            height: r.size[1],
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("device-tiles.test"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // The source's own upload: it stands for the engine's render.
        self.queue.write_texture(
            texture.as_image_copy(),
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(r.size[0] * bpp),
                rows_per_image: Some(r.size[1]),
            },
            extent,
        );
        texture
    }

    fn buffer(&self, bytes: &[u8]) -> wgpu::Buffer {
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("device-tiles.test"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buffer, 0, bytes);
        buffer
    }

    fn tile(&self, r: &TileRequest) -> DeviceTile {
        let [w, h] = r.size;
        match self.kind {
            Kind::SceneTexture => {
                let t = self.texture(
                    r,
                    wgpu::TextureFormat::Rgba32Float,
                    &floats_bytes(&scene_floats(r)),
                    16,
                );
                DeviceTile::texture(&self.device, t)
            }
            Kind::DisplaySrgbTexture | Kind::DisplayTexture => {
                let format = if self.kind == Kind::DisplaySrgbTexture {
                    wgpu::TextureFormat::Rgba8UnormSrgb
                } else {
                    wgpu::TextureFormat::Rgba8Unorm
                };
                let t = self.texture(r, format, &display_bytes(r), 4);
                DeviceTile::texture(&self.device, t).encoding(Encoding::Display)
            }
            Kind::PackedBuffer => {
                let b = self.buffer(&floats_bytes(&scene_floats(r)));
                DeviceTile::buffer(&self.device, b, BufferLayout::packed(w, 4))
            }
            Kind::CutBuffer => {
                // The tile at (3, 2) of a larger RGB image, 16 bytes into
                // the buffer, rows padded.
                let (ox, oy, pad_w, pad_h) = (3u32, 2u32, w + 5, h + 3);
                let layout = BufferLayout {
                    offset: 16,
                    row_floats: pad_w * 3 + 2,
                    channels: 3,
                };
                let floats = layout.offset as usize / 4 + (pad_h * layout.row_floats) as usize;
                let mut data = vec![-1.0f32; floats];
                let scene = scene_floats(r);
                for j in 0..h {
                    for i in 0..w {
                        let at = layout.offset as usize / 4
                            + ((oy + j) * layout.row_floats) as usize
                            + ((ox + i) * 3) as usize;
                        let from = ((j * w + i) * 4) as usize;
                        data[at..at + 3].copy_from_slice(&scene[from..from + 3]);
                    }
                }
                let b = self.buffer(&floats_bytes(&data));
                DeviceTile::buffer(&self.device, b, layout).origin([ox, oy])
            }
        }
    }
}

impl TileSource for DeviceSource {
    fn size(&self) -> Option<[u32; 2]> {
        Some(IMAGE)
    }

    fn request(&self, requests: Vec<TileRequest>) {
        for r in requests {
            let mut tile = self.tile(&r);
            if self.fallback {
                let req = r.clone();
                tile = tile.fallback(move || Some(Pixels::Scene(scene_floats(&req))));
            }
            self.sink.deliver_device(&r, tile);
        }
    }
}

struct Scene {
    state: ViewportState,
    view: View,
    source: Box<dyn TileSource>,
}

/// The shared device and its queue (see `egui_pop_test::shared_setup`).
fn shared() -> (wgpu::Device, wgpu::Queue) {
    let setup = egui_pop_test::shared_setup();
    (setup.device, setup.queue)
}

fn state() -> ViewportState {
    let mut state = ViewportState::new();
    state.set_settle(0.0);
    state
}

/// A scene drawn from device tiles made on `device`.
fn device_scene(
    view: View,
    kind: Kind,
    device: (wgpu::Device, wgpu::Queue),
    fallback: bool,
) -> Scene {
    let state = state();
    let source = DeviceSource {
        sink: state.sink(),
        device: device.0,
        queue: device.1,
        kind,
        fallback,
    };
    Scene {
        state,
        view,
        source: Box::new(source),
    }
}

/// The same image from the CPU path's test source (scene pixels).
fn cpu_scene(view: View) -> Scene {
    let state = state();
    let source = TestSource::immediate(state.sink(), IMAGE).scene();
    Scene {
        state,
        view,
        source: Box::new(source),
    }
}

/// A harness rendering with the process's device; the GPU path installed
/// when `gpu`.
fn harness(s: Scene, ppp: f32, gpu: bool) -> Harness<'static, Scene> {
    let rs = {
        let setup = egui_pop_test::shared_setup();
        let _guard = egui_pop_test::gpu_lock();
        egui_kittest::wgpu::create_render_state(
            egui_wgpu::WgpuSetup::Existing(setup),
            egui_wgpu::RendererOptions::PREDICTABLE,
        )
    };
    let renderer = egui_kittest::wgpu::WgpuTestRenderer::from_render_state(rs.clone());
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .with_pixels_per_point(ppp)
        .renderer(renderer)
        .build_ui_state(
            |ui, s: &mut Scene| {
                Viewport::new("t")
                    .source(s.source.as_ref())
                    .pixel_grid(false)
                    .show(ui, &mut s.state, &mut s.view);
            },
            s,
        );
    if gpu {
        egui_pop_viewport::gpu::install(&h.ctx, &rs);
    }
    // Stepped, not run: a source whose tiles keep failing keeps asking
    // for frames.
    h.run_steps(2);
    h
}

/// Steps until every wanted tile is on screen, then renders once more
/// so a whole frame and its paint ran on the tiles held.
fn settle(h: &mut Harness<'_, Scene>) {
    for _ in 0..20 {
        h.run();
        if h.state().state.tiles_complete() {
            let _ = h.render().expect("render");
            h.run();
            return;
        }
    }
    panic!(
        "the tiles never completed: {:?}",
        h.state().state.last_error()
    );
}

/// Each physical pixel of a row through the view shows its image pixel.
fn assert_one_to_one(h: &mut Harness<'_, Scene>, ppp: f32, tolerance: i32, what: &str) {
    let image = h.render().expect("render");
    let m = h.state().state.mapping().unwrap();
    let mut checked = 0;
    for y in [
        ((m.rect_px[1] + m.rect_px[3]) / 2.0) as u32,
        m.rect_px[1] as u32 + 3,
    ] {
        for x in (m.rect_px[0] as u32 + 2)..(m.rect_px[2] as u32 - 2) {
            let pos = Pos2::new((x as f32 + 0.5) / ppp, (y as f32 + 0.5) / ppp);
            let Some([ix, iy]) = m.pixel_at(pos) else {
                continue;
            };
            let want = TestSource::pixel(ix, iy, 0);
            let got = image.get_pixel(x, y).0;
            for c in 0..3 {
                assert!(
                    (i32::from(got[c]) - i32::from(want[c])).abs() <= tolerance,
                    "{what}: physical ({x}, {y}) shows {got:?}, image pixel ({ix}, {iy}) is {want:?}"
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 200, "{what}");
}

/// Nothing uploaded by the viewport, every tile bound in place: in the
/// state's counters and in the debug bridge's snapshot.
fn assert_zero_upload(h: &mut Harness<'_, Scene>, what: &str) {
    // A few more frames, the view held.
    for _ in 0..3 {
        h.run();
        let f = h.state().state.frame_transfers();
        assert!(f.is_zero_copy(), "{what}: the last frame copied {f:?}");
    }
    let st = &h.state().state;
    let total = st.transfers();
    assert_eq!((total.bytes, total.tiles), (0, 0), "{what}: {total:?}");
    assert!(total.bound > 0, "{what}: no device tile bound");
    let stats: Vec<_> = st.stats().snapshot().to_vec();
    assert!(stats.contains(&("refused", 0)), "{what}: {stats:?}");
    assert!(st.last_error().is_none(), "{what}: {:?}", st.last_error());
    let snap = egui_pop_viewport::debug::snapshot(&h.ctx);
    let uploads = &snap["viewports"]["t"]["tiles"]["uploads"];
    assert_eq!(uploads["total"]["bytes"], 0, "{what}: {uploads}");
    assert_eq!(uploads["frame"]["bytes"], 0, "{what}: {uploads}");
}

#[test]
fn device_textures_draw_as_the_cpu_path_with_nothing_uploaded() {
    for kind in [
        Kind::SceneTexture,
        Kind::DisplaySrgbTexture,
        Kind::DisplayTexture,
    ] {
        for (view, ppp) in [
            (View::actual([300.3, 200.7]), 1.0),
            (View::actual([300.3, 200.7]), 1.5),
            (View::at(2.0, [300.0, 200.0]), 1.0),
        ] {
            let what = format!("{kind:?} at {view:?}, {ppp}");
            let mut h = harness(device_scene(view, kind, shared(), false), ppp, true);
            settle(&mut h);
            // Float textures and the shader's encoding: within one level.
            assert_one_to_one(&mut h, ppp, 1, &what);
            assert_zero_upload(&mut h, &what);
        }
    }
}

#[test]
fn device_buffers_draw_as_the_cpu_path_with_nothing_uploaded() {
    for kind in [Kind::PackedBuffer, Kind::CutBuffer] {
        // Below 100 % the tiles are at the view's scale: the picture is
        // checked at 100 % and above, the counters everywhere.
        for (view, ppp, picture) in [
            (View::actual([300.3, 200.7]), 1.0, true),
            (View::at(0.5, [500.0, 400.0]), 1.0, false),
            (View::at(2.0, [300.0, 200.0]), 2.0, true),
        ] {
            let what = format!("{kind:?} at {view:?}, {ppp}");
            let mut h = harness(device_scene(view, kind, shared(), false), ppp, true);
            if egui_pop_viewport::gpu::render_device(&h.ctx).is_some_and(|r| !r.buffers) {
                eprintln!("skipped: this adapter cannot read storage buffers in a fragment shader");
                return;
            }
            settle(&mut h);
            if picture {
                assert_one_to_one(&mut h, ppp, 1, &what);
            }
            assert_zero_upload(&mut h, &what);
        }
    }
}

/// A second device on the shared adapter.
fn other_device() -> (wgpu::Device, wgpu::Queue) {
    let setup = egui_pop_test::shared_setup();
    let _guard = egui_pop_test::gpu_lock();
    let future = setup.adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("device-tiles.other"),
        ..Default::default()
    });
    // wgpu's native futures are ready at once.
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(r) = future.as_mut().poll(&mut cx) {
            return r.expect("a second device");
        }
        std::thread::yield_now();
    }
}

#[test]
fn a_tile_from_another_device_is_refused_and_falls_back_to_its_pixels() {
    let view = View::actual([300.3, 200.7]);
    // With a fallback: the picture is right, through uploads.
    let mut h = harness(
        device_scene(view, Kind::SceneTexture, other_device(), true),
        1.0,
        true,
    );
    settle(&mut h);
    assert_one_to_one(&mut h, 1.0, 1, "fallback");
    let st = &h.state().state;
    let error = st.last_error().expect("the refusal is kept");
    assert!(error.contains("another wgpu device"), "{error}");
    assert!(
        st.stats()
            .snapshot()
            .iter()
            .any(|(k, v)| *k == "refused" && *v > 0)
    );
    assert!(
        st.transfers().bytes > 0,
        "the fallback pixels were uploaded"
    );
    assert_eq!(st.transfers().bound, 0);
    // Without: the requests fail, with the same reason.
    let mut h = harness(
        device_scene(view, Kind::SceneTexture, other_device(), false),
        1.0,
        true,
    );
    h.run_steps(4);
    let st = &h.state().state;
    assert!(
        st.last_error()
            .is_some_and(|e| e.contains("another wgpu device"))
    );
    assert!(
        st.stats()
            .snapshot()
            .iter()
            .any(|(k, v)| *k == "failed" && *v > 0)
    );
    assert_eq!(st.tile_count(), 0);
}

#[test]
fn without_the_gpu_path_device_tiles_fall_back_to_the_cpu_path() {
    let view = View::actual([300.3, 200.7]);
    let mut h = harness(
        device_scene(view, Kind::SceneTexture, shared(), true),
        1.0,
        false,
    );
    settle(&mut h);
    // The CPU path's display transform: within one level.
    assert_one_to_one(&mut h, 1.0, 1, "cpu fallback");
    let st = &h.state().state;
    assert!(
        st.last_error().is_some_and(|e| e.contains("no GPU path")),
        "{:?}",
        st.last_error()
    );
    assert!(st.transfers().tiles > 0);
}

#[test]
fn the_loupe_redraws_device_tiles_without_uploading_them() {
    let view = View::at(2.0, [300.0, 200.0]);
    let mut s = device_scene(view, Kind::PackedBuffer, shared(), false);
    s.state.loupe_mut().on = true;
    let mut h = harness(s, 1.0, true);
    if egui_pop_viewport::gpu::render_device(&h.ctx).is_some_and(|r| !r.buffers) {
        eprintln!("skipped: this adapter cannot read storage buffers in a fragment shader");
        return;
    }
    let at = Pos2::new(120.3, 210.6);
    h.hover_at(at);
    settle(&mut h);
    h.run();
    let st = &h.state().state;
    let f = st.loupe_frame().expect("the loupe shows").clone();
    assert!(f.gpu, "drawn from the GPU path's tiles");
    assert!(f.tiles > 0);
    let pixel = f.sampling.pixel.expect("over the image");
    let block = f.sampling.center_block();
    let image = h.render().expect("render");
    let x = ((block[0] + block[2]) / 2.0) as u32;
    let y = ((block[1] + block[3]) / 2.0) as u32;
    let got = image.get_pixel(x, y).0;
    let want = TestSource::pixel(pixel[0], pixel[1], 0);
    for c in 0..3 {
        assert!(
            (i32::from(got[c]) - i32::from(want[c])).abs() <= 1,
            "the loupe's centre shows {got:?}, pixel {pixel:?} is {want:?}"
        );
    }
    assert_zero_upload(&mut h, "loupe");
}

/// Paints a half-transparent tint over the whole image under the
/// overlays, as a mask's display does.
fn tint(state: &mut ViewportState) {
    state.hooks_mut().under_overlays = Some(Box::new(|painter: &egui::Painter, m: &Mapping| {
        let r = m.image_rect_to_screen([0.0, 0.0, m.image[0], m.image[1]]);
        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(220, 20, 60, 128));
    }));
}

#[test]
fn a_mask_tint_composites_over_device_tiles_as_over_cpu_tiles() {
    let view = View::actual([300.3, 200.7]);
    let mut cpu = cpu_scene(view);
    tint(&mut cpu.state);
    let mut h = harness(cpu, 1.0, false);
    settle(&mut h);
    let want = h.render().expect("render");
    for kind in [Kind::SceneTexture, Kind::CutBuffer] {
        let mut s = device_scene(view, kind, shared(), false);
        tint(&mut s.state);
        let mut h = harness(s, 1.0, true);
        if kind == Kind::CutBuffer
            && egui_pop_viewport::gpu::render_device(&h.ctx).is_some_and(|r| !r.buffers)
        {
            continue;
        }
        settle(&mut h);
        let got = h.render().expect("render");
        let m = h.state().state.mapping().unwrap();
        let mut checked = 0;
        for y in (m.rect_px[1] as u32 + 2..m.rect_px[3] as u32 - 2).step_by(7) {
            for x in (m.rect_px[0] as u32 + 2..m.rect_px[2] as u32 - 2).step_by(5) {
                let (a, b) = (got.get_pixel(x, y).0, want.get_pixel(x, y).0);
                for c in 0..3 {
                    assert!(
                        (i32::from(a[c]) - i32::from(b[c])).abs() <= 2,
                        "{kind:?}: ({x}, {y}) is {a:?} over device tiles, {b:?} over CPU tiles"
                    );
                }
                checked += 1;
            }
        }
        assert!(checked > 500);
        assert_zero_upload(&mut h, &format!("{kind:?} tinted"));
    }
}
