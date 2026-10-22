//! The GPU path (feature `wgpu`): scene tiles as half-float textures,
//! drawn by an `egui_wgpu` paint callback whose shader runs the display
//! transform's [`Stage`].
//!
//! Call [`install`] once with the render state eframe (or a test
//! harness) opened; from then on, viewports in that context send scene
//! tiles here. Without it, everything takes the CPU path.
//!
//! - Textures are `Rgba16Float` where the adapter can sample it (else
//!   `Rgba32Float`), one per tile, a tile never larger than the device's
//!   `max_texture_dimension_2d`.
//! - Texels are read with `textureLoad` (nearest): below 100 % the
//!   source renders at the view's scale, one texel per physical pixel;
//!   above, each image pixel is a crisp square.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::Context;
use egui_pop_core::state;
use egui_wgpu::wgpu;

/// egui-wgpu, the version the GPU path is built with.
pub use egui_wgpu;

use crate::device::{Encoding, RenderDevice, Resource};
use crate::display::{self, ColorSpace, DisplayTarget, Stage, SurfaceFormat, TargetSource};
use crate::tiles::{Device, GpuData, GpuQueue};

/// What [`install`] found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuInfo {
    /// The largest texture side the device takes.
    pub max_texture_side: u32,
    /// Tiles are half floats (else 32-bit floats).
    pub half_float: bool,
    /// The surface encodes sRGB on write (the shader then writes linear).
    pub target_is_srgb: bool,
}

/// What every tile pipeline shares: the draw's uniform, the vertex
/// stage, the transfers and the display stage.
const COMMON: &str = r#"
struct Draw {
    dst: vec4<f32>,
    sizes: vec4<f32>,
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    params: vec4<f32>,
    extra: vec4<f32>,
    // Uploaded tiles and textures: the tile's texel origin, 0, the mode;
    // buffers: the tile's first float, floats a row, channels, the mode.
    tile_layout: vec4<u32>,
};

@group(0) @binding(0) var<uniform> d: Draw;

struct V {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> V {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let p = mix(d.dst.xy, d.dst.zw, corners[i]);
    var out: V;
    out.pos = vec4<f32>(p.x / d.sizes.x * 2.0 - 1.0, 1.0 - p.y / d.sizes.y * 2.0, 0.0, 1.0);
    return out;
}

fn srgb_encode(v: f32) -> f32 {
    let x = max(v, 0.0);
    if x <= 0.0031308 {
        return x * 12.92;
    }
    return 1.055 * pow(x, 1.0 / 2.4) - 0.055;
}

fn srgb_decode(v: f32) -> f32 {
    if v <= 0.04045 {
        return v / 12.92;
    }
    return pow((v + 0.055) / 1.055, 2.4);
}

fn pq_encode(nits: f32) -> f32 {
    let m1 = 2610.0 / 16384.0;
    let m2 = 2523.0 / 4096.0 * 128.0;
    let c1 = 3424.0 / 4096.0;
    let c2 = 2413.0 / 4096.0 * 32.0;
    let c3 = 2392.0 / 4096.0 * 32.0;
    let y = pow(clamp(nits / 10000.0, 0.0, 1.0), m1);
    return pow((c1 + c2 * y) / (1.0 + c3 * y), m2);
}

fn hlg_oetf(e: f32) -> f32 {
    let x = clamp(e, 0.0, 1.0);
    if x <= 1.0 / 12.0 {
        return sqrt(3.0 * x);
    }
    let a = 0.17883277;
    return a * log(12.0 * x - (1.0 - 4.0 * a)) + 0.5599107;
}

// The transfers of `display::Transfer`, by code; `white` is the nits of
// SDR white (PQ is absolute).
fn encode(v: f32, code: u32, white: f32) -> f32 {
    if code == 1u {
        return pow(max(v, 0.0), 1.0 / 2.2);
    }
    if code == 2u {
        return v;
    }
    if code == 3u {
        return pq_encode(max(v, 0.0) * white);
    }
    if code == 4u {
        return hlg_oetf(max(v, 0.0) * 0.26496256);
    }
    if code == 5u {
        return sign(v) * srgb_encode(abs(v));
    }
    return srgb_encode(v);
}

// The tile's texel under a fragment, from the tile's top left.
fn texel_at(pos: vec2<f32>) -> vec2<i32> {
    let size = d.sizes.zw;
    let t = (pos - d.dst.xy) / (d.dst.zw - d.dst.xy) * size;
    return clamp(vec2<i32>(floor(t)), vec2<i32>(0, 0), vec2<i32>(size) - vec2<i32>(1, 1));
}

// A value read to the colour written. Mode 0: scene-referred, through
// the display stage; 1: display-encoded, as it is; 2: display-encoded
// in an sRGB texture, whose read decoded it: encoded again.
fn shade(s: vec3<f32>) -> vec4<f32> {
    let mode = d.tile_layout.w;
    var out = s;
    if mode == 0u {
        var rgb = vec3<f32>(dot(d.m0.xyz, s), dot(d.m1.xyz, s), dot(d.m2.xyz, s));
        rgb = clamp(rgb, vec3<f32>(d.params.x), vec3<f32>(d.params.y));
        let code = u32(d.params.z);
        let white = d.extra.x;
        out = vec3<f32>(encode(rgb.r, code, white), encode(rgb.g, code, white), encode(rgb.b, code, white));
    } else if mode == 2u {
        out = vec3<f32>(srgb_encode(s.r), srgb_encode(s.g), srgb_encode(s.b));
    }
    if d.params.w > 0.5 {
        // The surface encodes sRGB itself: hand it what decodes to `out`.
        out = vec3<f32>(srgb_decode(out.r), srgb_decode(out.g), srgb_decode(out.b));
    }
    return vec4<f32>(out, 1.0);
}
"#;

/// Tiles in textures: uploaded ones and device textures; and the hole
/// an image surface shows through.
const TEXTURE: &str = r#"
@group(1) @binding(0) var tex: texture_2d<f32>;

@fragment
fn fs(v: V) -> @location(0) vec4<f32> {
    let texel = texel_at(v.pos.xy) + vec2<i32>(d.tile_layout.xy);
    return shade(textureLoad(tex, texel, 0).rgb);
}

// A triangle over the whole target, for the hole an image surface below
// the window shows through (the scissor keeps it to the image).
@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> V {
    var corners = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var out: V;
    out.pos = vec4<f32>(corners[i], 0.0, 1.0);
    return out;
}

@fragment
fn fs_hole() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
"#;

/// Device tiles in storage buffers of `f32`, read in place.
const BUFFER: &str = r#"
@group(1) @binding(0) var<storage, read> buf: array<f32>;

@fragment
fn fs(v: V) -> @location(0) vec4<f32> {
    let t = vec2<u32>(texel_at(v.pos.xy));
    let channels = d.tile_layout.z;
    let i = d.tile_layout.x + t.y * d.tile_layout.y + t.x * channels;
    var s = vec3<f32>(buf[i]);
    if channels >= 3u {
        s = vec3<f32>(buf[i], buf[i + 1u], buf[i + 2u]);
    }
    return shade(s);
}
"#;

/// Bytes of one draw's uniform.
const UNIFORM_SIZE: u64 = 8 * 16;

struct Slot {
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
    capacity: usize,
}

/// The tile pipelines for one target format: textures, and storage
/// buffers where the adapter reads them in a fragment shader.
pub(crate) struct Pipelines {
    texture: wgpu::RenderPipeline,
    buffer: Option<wgpu::RenderPipeline>,
}

/// A tile bound for drawing: its bind group, which pipeline reads it,
/// and its uniform's `layout`.
struct Bound {
    group: wgpu::BindGroup,
    buffer: bool,
    layout: [u32; 4],
}

/// The shader modules and pipeline layouts the tile pipelines are built
/// from.
struct Modules {
    texture: (wgpu::ShaderModule, wgpu::PipelineLayout),
    buffer: Option<(wgpu::ShaderModule, wgpu::PipelineLayout)>,
}

impl Modules {
    fn pipelines(&self, device: &wgpu::Device, format: wgpu::TextureFormat) -> Pipelines {
        let (shader, layout) = &self.texture;
        Pipelines {
            texture: render_pipeline(device, shader, layout, ["vs", "fs"], format),
            buffer: self.buffer.as_ref().map(|(shader, layout)| {
                render_pipeline(device, shader, layout, ["vs", "fs"], format)
            }),
        }
    }
}

pub(crate) struct Resources {
    pipelines: Pipelines,
    // What an image surface (`wayland`) draws with: its hole, and on Linux
    // the tile pipeline per surface format, built from the same shader and
    // layout.
    /// Writes transparent black over the image, in the window's format.
    #[cfg(feature = "wayland")]
    hole: wgpu::RenderPipeline,
    modules: Modules,
    /// The tile pipelines for each image surface format.
    #[cfg(all(feature = "wayland", target_os = "linux"))]
    pub(crate) image_pipelines: HashMap<wgpu::TextureFormat, Pipelines>,
    uniform_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    buffer_layout: wgpu::BindGroupLayout,
    stride: u64,
    format: wgpu::TextureFormat,
    target_is_srgb: bool,
    /// The device's storage binding offset alignment.
    storage_align: u64,
    textures: HashMap<u64, Bound>,
    slots: HashMap<u64, Slot>,
}

/// The GPU path's device, when installed.
const DEVICE: state::Slot<Device> = state::Slot::opaque("viewport.gpu.device");
/// The GPU path takes half-float texels.
const HALF_FLOAT: state::Slot<bool> = state::Slot::new("viewport.gpu.half_float");
/// The render device, for device tiles.
const RENDER: state::Slot<RenderDevice> = state::Slot::opaque("viewport.gpu.render");

/// The render device of the GPU path installed in `ctx`: the device a
/// [`DeviceTile`](crate::device::DeviceTile) must be created on, and what
/// it can read. `None` without a GPU path.
pub fn render_device(ctx: &Context) -> Option<RenderDevice> {
    RENDER.get(ctx)
}

/// The GPU path's device, when installed in `ctx`.
pub(crate) fn installed(ctx: &Context) -> Option<Device> {
    DEVICE.get(ctx)
}

/// Whether the GPU path installed in `ctx` takes half-float texels.
pub(crate) fn half_float(ctx: &Context) -> bool {
    HALF_FLOAT.get(ctx).unwrap_or(false)
}

/// The display stage's surface format for a wgpu one.
pub fn surface_format(format: wgpu::TextureFormat) -> SurfaceFormat {
    use wgpu::TextureFormat as F;
    match format {
        F::Rgba8UnormSrgb | F::Bgra8UnormSrgb => SurfaceFormat::Rgba8UnormSrgb,
        F::Rgba8Unorm | F::Bgra8Unorm => SurfaceFormat::Rgba8Unorm,
        F::Rgb10a2Unorm => SurfaceFormat::Rgb10a2Unorm,
        F::Rgba16Float => SurfaceFormat::Rgba16Float,
        _ => SurfaceFormat::Other,
    }
}

/// The display stage's colour space for a wgpu one; `Auto` as wgpu
/// resolves it (extended linear on half floats, sRGB otherwise).
pub fn color_space(space: wgpu::SurfaceColorSpace, format: wgpu::TextureFormat) -> ColorSpace {
    use wgpu::SurfaceColorSpace as S;
    match space {
        S::Auto if format == wgpu::TextureFormat::Rgba16Float => ColorSpace::ExtendedSrgbLinear,
        S::ExtendedSrgbLinear => ColorSpace::ExtendedSrgbLinear,
        S::ExtendedSrgb => ColorSpace::ExtendedSrgb,
        S::DisplayP3 => ColorSpace::DisplayP3,
        S::ExtendedDisplayP3 => ColorSpace::ExtendedDisplayP3,
        S::Bt2100Pq => ColorSpace::Bt2100Pq,
        S::Bt2100Hlg => ColorSpace::Bt2100Hlg,
        _ => ColorSpace::Srgb,
    }
}

pub(crate) fn to_wgpu(space: ColorSpace) -> wgpu::SurfaceColorSpace {
    use wgpu::SurfaceColorSpace as S;
    match space {
        ColorSpace::Srgb => S::Srgb,
        ColorSpace::ExtendedSrgbLinear => S::ExtendedSrgbLinear,
        ColorSpace::ExtendedSrgb => S::ExtendedSrgb,
        ColorSpace::DisplayP3 => S::DisplayP3,
        ColorSpace::ExtendedDisplayP3 => S::ExtendedDisplayP3,
        ColorSpace::Bt2100Pq => S::Bt2100Pq,
        ColorSpace::Bt2100Hlg => S::Bt2100Hlg,
    }
}

/// What a surface configured with `format` and `space` expects, as
/// declared by wgpu's window system integration; `headroom` is the
/// display's (wgpu's `DisplayHdrInfo`), 1.0 when unknown.
pub fn surface_target(
    format: wgpu::TextureFormat,
    space: wgpu::SurfaceColorSpace,
    headroom: f32,
) -> DisplayTarget {
    DisplayTarget::for_color_space(
        color_space(space, format),
        surface_format(format),
        headroom,
        TargetSource::Wsi,
    )
}

/// Which surfaces a host may configure for a wanted target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfacePolicy {
    /// Only colour spaces egui's renderer draws the interface faithfully
    /// on (sRGB, extended sRGB): the window holds egui's interface.
    EguiCompatible,
    /// Any colour space: the host composes the interface itself (or the
    /// surface holds only the image).
    Any,
}

/// The surface configuration path for a host that configures its own
/// surface (eframe 0.36 configures 8-bit sRGB and offers no choice):
/// the format and colour space to configure for `wanted`, among what the
/// surface's capabilities offer, and what the viewport will draw for
/// once configured. Prefers what carries the wanted gamut and headroom,
/// then fewer bits.
pub fn choose_surface(
    caps: &wgpu::SurfaceCapabilities,
    wanted: &DisplayTarget,
    policy: SurfacePolicy,
    headroom: f32,
) -> Option<(wgpu::TextureFormat, wgpu::SurfaceColorSpace, DisplayTarget)> {
    let mut best: Option<(u8, wgpu::TextureFormat, ColorSpace)> = None;
    for &format in &caps.formats {
        let supported = caps.color_spaces(format);
        for space in ColorSpace::ALL {
            if policy == SurfacePolicy::EguiCompatible && !space.egui_draws_faithfully() {
                continue;
            }
            if !supported.contains(color_space_flag(space)) {
                continue;
            }
            let target = DisplayTarget::for_color_space(
                space,
                surface_format(format),
                headroom,
                TargetSource::Wsi,
            );
            if !target.is_supported() {
                continue;
            }
            // Carrying what is wanted first, then the narrowest format.
            let score = if target.missing(wanted).is_none() {
                100
            } else {
                0
            } + (if space == ColorSpace::Srgb { 10 } else { 0 })
                + (32 - surface_format(format).bits().min(32) / 2);
            if best.is_none_or(|b| score > b.0) {
                best = Some((score, format, space));
            }
        }
    }
    best.map(|(_, format, space)| {
        let target = DisplayTarget::for_color_space(
            space,
            surface_format(format),
            headroom,
            TargetSource::Wsi,
        );
        (format, to_wgpu(space), target)
    })
}

pub(crate) fn color_space_flag(space: ColorSpace) -> wgpu::SurfaceColorSpaces {
    use wgpu::SurfaceColorSpaces as F;
    match space {
        ColorSpace::Srgb => F::SRGB,
        ColorSpace::ExtendedSrgbLinear => F::EXTENDED_SRGB_LINEAR,
        ColorSpace::ExtendedSrgb => F::EXTENDED_SRGB,
        ColorSpace::DisplayP3 => F::DISPLAY_P3,
        ColorSpace::ExtendedDisplayP3 => F::EXTENDED_DISPLAY_P3,
        ColorSpace::Bt2100Pq => F::BT2100_PQ,
        ColorSpace::Bt2100Hlg => F::BT2100_HLG,
    }
}

/// Installs the GPU path on a render state; viewports in `ctx` use it
/// from the next frame. Records what the surface expects for every
/// window: eframe 0.36 configures `rs.target_format` with wgpu's default
/// colour space (8-bit sRGB in practice). A host that configured its
/// surface itself calls [`install_for_surface`].
pub fn install(ctx: &Context, rs: &egui_wgpu::RenderState) -> GpuInfo {
    install_for_surface(ctx, rs, wgpu::SurfaceColorSpace::Auto, 1.0)
}

/// [`install`], for a surface configured with `space` (and a display of
/// `headroom`, 1.0 when unknown).
pub fn install_for_surface(
    ctx: &Context,
    rs: &egui_wgpu::RenderState,
    space: wgpu::SurfaceColorSpace,
    headroom: f32,
) -> GpuInfo {
    display::set_default_target(ctx, surface_target(rs.target_format, space, headroom));
    let device = &rs.device;
    let half = rs
        .adapter
        .get_texture_format_features(wgpu::TextureFormat::Rgba16Float)
        .allowed_usages
        .contains(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
    let format = if half {
        wgpu::TextureFormat::Rgba16Float
    } else {
        wgpu::TextureFormat::Rgba32Float
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("egui-pop-viewport"),
        source: wgpu::ShaderSource::Wgsl(format!("{COMMON}{TEXTURE}").into()),
    });
    // Storage buffers read in a fragment shader: not on every adapter
    // (WebGL 2, some downlevel ones).
    let buffers = rs
        .adapter
        .get_downlevel_capabilities()
        .flags
        .contains(wgpu::DownlevelFlags::FRAGMENT_STORAGE)
        && device.limits().max_storage_buffers_per_shader_stage > 0;
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("egui-pop-viewport.draw"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(UNIFORM_SIZE),
            },
            count: None,
        }],
    });
    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("egui-pop-viewport.tile"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let buffer_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("egui-pop-viewport.device-buffer"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("egui-pop-viewport"),
        bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
        ..Default::default()
    });
    let buffer_module = buffers.then(|| {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("egui-pop-viewport.device-buffer"),
            source: wgpu::ShaderSource::Wgsl(format!("{COMMON}{BUFFER}").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("egui-pop-viewport.device-buffer"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&buffer_layout)],
            ..Default::default()
        });
        (shader, layout)
    });
    #[cfg(feature = "wayland")]
    let empty = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("egui-pop-viewport.hole"),
        ..Default::default()
    });
    #[cfg(feature = "wayland")]
    let hole = render_pipeline(
        device,
        &shader,
        &empty,
        ["vs_full", "fs_hole"],
        rs.target_format,
    );
    let align = u64::from(device.limits().min_uniform_buffer_offset_alignment);
    let stride = UNIFORM_SIZE.div_ceil(align) * align;
    let max_texture_side = device.limits().max_texture_dimension_2d;
    let target_is_srgb = rs.target_format.is_srgb();
    let storage_align = u64::from(device.limits().min_storage_buffer_offset_alignment);
    let modules = Modules {
        texture: (shader, layout),
        buffer: buffer_module,
    };
    let pipelines = modules.pipelines(device, rs.target_format);
    rs.renderer.write().callback_resources.insert(Resources {
        pipelines,
        #[cfg(feature = "wayland")]
        hole,
        modules,
        #[cfg(all(feature = "wayland", target_os = "linux"))]
        image_pipelines: HashMap::new(),
        uniform_layout,
        texture_layout,
        buffer_layout,
        stride,
        format,
        target_is_srgb,
        storage_align,
        textures: HashMap::new(),
        slots: HashMap::new(),
    });
    RENDER.set(
        ctx,
        RenderDevice {
            device: device.clone(),
            buffers,
            storage_align,
            max_binding: device.limits().max_storage_buffer_binding_size,
        },
    );
    DEVICE.set(
        ctx,
        Device {
            max_texture_side,
            gpu: true,
        },
    );
    HALF_FLOAT.set(ctx, half);
    GpuInfo {
        max_texture_side,
        half_float: half,
        target_is_srgb,
    }
}

/// A pipeline of the viewport's shader, its `[vertex, fragment]` entry
/// points, writing `format` without blending.
fn render_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    [vertex, fragment]: [&str; 2],
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("egui-pop-viewport"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl Resources {
    /// A tile texture's bind group (`layout`: its uniform's).
    fn bind_tile(&self, device: &wgpu::Device, texture: &wgpu::Texture, layout: [u32; 4]) -> Bound {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("egui-pop-viewport.tile"),
            layout: &self.texture_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        Bound {
            group,
            buffer: false,
            layout,
        }
    }

    /// A device tile's binding: its texture, or the rows of its buffer
    /// the tile covers. `None` when it cannot be bound here (checked
    /// before: another device, or buffers unreadable).
    fn bind_device(
        &self,
        device: &wgpu::Device,
        tile: &crate::device::DeviceTile,
        size: [u32; 2],
    ) -> Option<Bound> {
        if tile.device() != device {
            return None;
        }
        let display = tile.encoding == Encoding::Display;
        match tile.resource() {
            Resource::Texture(texture) => {
                let mode = match (display, texture.format().is_srgb()) {
                    (false, _) => 0,
                    (true, false) => 1,
                    (true, true) => 2,
                };
                let [ox, oy] = tile.origin;
                Some(self.bind_tile(device, texture, [ox, oy, 0, mode]))
            }
            Resource::Buffer { buffer, layout } => {
                self.modules.buffer.as_ref()?;
                let (offset, bytes, base) = layout.binding(tile.origin, size, self.storage_align);
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("egui-pop-viewport.device-buffer"),
                    layout: &self.buffer_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer,
                            offset,
                            size: wgpu::BufferSize::new(bytes),
                        }),
                    }],
                });
                Some(Bound {
                    group,
                    buffer: true,
                    layout: [
                        u32::try_from(base).ok()?,
                        layout.row_floats,
                        layout.channels,
                        u32::from(display),
                    ],
                })
            }
        }
    }

    /// The tile pipelines for an image surface's format, built on first
    /// use.
    #[cfg(all(feature = "wayland", target_os = "linux"))]
    pub(crate) fn ensure_image_pipelines(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) {
        let modules = &self.modules;
        self.image_pipelines
            .entry(format)
            .or_insert_with(|| modules.pipelines(device, format));
    }

    /// The window's tile pipelines.
    pub(crate) fn pipelines(&self) -> &Pipelines {
        &self.pipelines
    }

    /// Draws the hole an image surface below the window shows through.
    #[cfg(feature = "wayland")]
    pub(crate) fn draw_hole(&self, pass: &mut wgpu::RenderPass<'_>, screen: [u32; 2]) {
        pass.set_viewport(0.0, 0.0, screen[0] as f32, screen[1] as f32, 0.0, 1.0);
        pass.set_pipeline(&self.hole);
        pass.draw(0..3, 0..1);
    }
}

/// One tile drawn by the GPU path.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GpuDraw {
    pub texture: u64,
    /// Where, in physical pixels of the window.
    pub dst: [f32; 4],
    pub size: [u32; 2],
}

/// One viewport side's GPU tiles for a frame.
pub(crate) struct TilePaint {
    pub slot: u64,
    pub queue: Arc<Mutex<GpuQueue>>,
    pub draws: Vec<GpuDraw>,
    pub stage: Stage,
}

impl TilePaint {
    /// Takes in the tiles uploaded and freed since the last frame.
    pub(crate) fn sync_textures(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        r: &mut Resources,
    ) {
        // On the render thread: one texture per tile uploaded, and the
        // f16 conversion of tiles not prepared by the delivering thread.
        profiling::scope!("pop.tiles.gpu_prepare");
        let (uploads, frees) = {
            let mut q = self
                .queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (std::mem::take(&mut q.uploads), std::mem::take(&mut q.frees))
        };
        if uploads.is_empty() && frees.is_empty() {
            return;
        }
        for id in frees {
            r.textures.remove(&id);
        }
        let mut copied = crate::tiles::Transfers::default();
        for (id, size, data) in uploads {
            let half = r.format == wgpu::TextureFormat::Rgba16Float;
            let bpp: u32 = if half { 8 } else { 16 };
            let bytes = match data {
                GpuData::Scene(data) => crate::source::texels(&data, half),
                GpuData::Texels(bytes) => bytes,
                GpuData::Device(tile) => {
                    // On the render device: bound as it is.
                    if let Some(bound) = r.bind_device(device, &tile, size) {
                        r.textures.insert(id, bound);
                        copied.bound += 1;
                    }
                    continue;
                }
            };
            // Prepared for another format (cannot happen while the
            // device stays): left out, the tile is not drawn.
            if bytes.len() != size[0] as usize * size[1] as usize * bpp as usize {
                continue;
            }
            let extent = wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("egui-pop-viewport.tile"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: r.format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                texture.as_image_copy(),
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size[0] * bpp),
                    rows_per_image: Some(size[1]),
                },
                extent,
            );
            copied.bytes += bytes.len() as u64;
            copied.tiles += 1;
            let bound = r.bind_tile(device, &texture, [0; 4]);
            r.textures.insert(id, bound);
        }
        let mut q = self
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        q.copied.bytes += copied.bytes;
        q.copied.tiles += copied.tiles;
        q.copied.bound += copied.bound;
    }

    /// Writes the draws' uniforms for a target of `size` pixels whose
    /// top left is `origin` in window pixels; `false` when there is
    /// nothing to draw.
    pub(crate) fn write_uniforms(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        r: &mut Resources,
        size: [f32; 2],
        origin: [f32; 2],
        target_is_srgb: bool,
    ) -> bool {
        if self.draws.is_empty() {
            return false;
        }
        let stride = r.stride;
        let needed = self.draws.len();
        let grow = r.slots.get(&self.slot).is_none_or(|s| s.capacity < needed);
        if grow {
            let capacity = needed.next_power_of_two().max(16);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("egui-pop-viewport.draws"),
                size: stride * capacity as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("egui-pop-viewport.draws"),
                layout: &r.uniform_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(UNIFORM_SIZE),
                    }),
                }],
            });
            r.slots.insert(
                self.slot,
                Slot {
                    buffer,
                    group,
                    capacity,
                },
            );
        }
        let Some(slot) = r.slots.get(&self.slot) else {
            return false;
        };
        let r = &*r;
        let mut bytes = vec![0u8; stride as usize * needed];
        let [sw, sh] = size;
        let m = &self.stage.matrix;
        for (i, d) in self.draws.iter().enumerate() {
            let layout = r.textures.get(&d.texture).map_or([0; 4], |b| b.layout);
            let values: [f32; 28] = [
                d.dst[0] - origin[0],
                d.dst[1] - origin[1],
                d.dst[2] - origin[0],
                d.dst[3] - origin[1],
                sw,
                sh,
                d.size[0] as f32,
                d.size[1] as f32,
                m[0][0],
                m[0][1],
                m[0][2],
                0.0,
                m[1][0],
                m[1][1],
                m[1][2],
                0.0,
                m[2][0],
                m[2][1],
                m[2][2],
                0.0,
                self.stage.clamp[0],
                self.stage.clamp[1],
                self.stage.transfer.code() as f32,
                if target_is_srgb { 1.0 } else { 0.0 },
                self.stage.white_nits,
                0.0,
                0.0,
                0.0,
            ];
            let at = i * stride as usize;
            for (k, v) in values.iter().enumerate() {
                bytes[at + k * 4..at + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
            let at = at + values.len() * 4;
            for (k, v) in layout.iter().enumerate() {
                bytes[at + k * 4..at + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        queue.write_buffer(&slot.buffer, 0, &bytes);
        true
    }

    /// Records the draws with `pipelines` (the viewport set by the
    /// caller): textures and buffers, each with its pipeline.
    pub(crate) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        r: &Resources,
        pipelines: &Pipelines,
    ) {
        let Some(slot) = r.slots.get(&self.slot) else {
            return;
        };
        let mut current = None;
        for (i, d) in self.draws.iter().enumerate() {
            let Some(bound) = r.textures.get(&d.texture) else {
                continue;
            };
            let pipeline = if bound.buffer {
                match &pipelines.buffer {
                    Some(p) => p,
                    None => continue,
                }
            } else {
                &pipelines.texture
            };
            if current != Some(bound.buffer) {
                pass.set_pipeline(pipeline);
                current = Some(bound.buffer);
            }
            pass.set_bind_group(0, &slot.group, &[(i as u64 * r.stride) as u32]);
            pass.set_bind_group(1, &bound.group, &[]);
            pass.draw(0..6, 0..1);
        }
    }
}

impl egui_wgpu::CallbackTrait for TilePaint {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(r) = resources.get_mut::<Resources>() else {
            return Vec::new();
        };
        self.sync_textures(device, queue, r);
        let [w, h] = screen.size_in_pixels;
        let srgb = r.target_is_srgb;
        self.write_uniforms(device, queue, r, [w as f32, h as f32], [0.0, 0.0], srgb);
        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(r) = resources.get::<Resources>() else {
            return;
        };
        // Positions are in window pixels; the scissor keeps the clip.
        let [sw, sh] = info.screen_size_px;
        pass.set_viewport(0.0, 0.0, sw as f32, sh as f32, 0.0, 1.0);
        self.draw(pass, r, r.pipelines());
    }
}

#[cfg(test)]
mod tests {
    use super::{BUFFER, COMMON, TEXTURE};

    /// Each tile module, as `install` composes it, parses and validates
    /// with naga (what wgpu runs at shader creation), without a GPU.
    #[test]
    fn the_tile_shaders_parse_and_validate() {
        for (name, part) in [("texture", TEXTURE), ("buffer", BUFFER)] {
            let source = format!("{COMMON}{part}");
            let module = naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&source)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&source)));
        }
    }
}
