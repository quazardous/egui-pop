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
use egui_wgpu::wgpu;

/// egui-wgpu, the version the GPU path is built with.
pub use egui_wgpu;

use crate::display::Stage;
use crate::tiles::{Device, GpuQueue};

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

const SHADER: &str = r#"
struct Draw {
    dst: vec4<f32>,
    sizes: vec4<f32>,
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> d: Draw;
@group(1) @binding(0) var tex: texture_2d<f32>;

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

fn encode(v: f32, code: u32) -> f32 {
    if code == 1u {
        return pow(max(v, 0.0), 1.0 / 2.2);
    }
    if code == 2u {
        return v;
    }
    return srgb_encode(v);
}

@fragment
fn fs(v: V) -> @location(0) vec4<f32> {
    let size = d.sizes.zw;
    let t = (v.pos.xy - d.dst.xy) / (d.dst.zw - d.dst.xy) * size;
    let texel = clamp(vec2<i32>(floor(t)), vec2<i32>(0, 0), vec2<i32>(size) - vec2<i32>(1, 1));
    let s = textureLoad(tex, texel, 0);
    var rgb = vec3<f32>(dot(d.m0.xyz, s.rgb), dot(d.m1.xyz, s.rgb), dot(d.m2.xyz, s.rgb));
    rgb = clamp(rgb, vec3<f32>(d.params.x), vec3<f32>(d.params.y));
    let code = u32(d.params.z);
    var out = vec3<f32>(encode(rgb.r, code), encode(rgb.g, code), encode(rgb.b, code));
    if d.params.w > 0.5 {
        // The surface encodes sRGB itself: hand it what decodes to `out`.
        out = vec3<f32>(srgb_decode(out.r), srgb_decode(out.g), srgb_decode(out.b));
    }
    return vec4<f32>(out, 1.0);
}
"#;

/// Bytes of one draw's uniform.
const UNIFORM_SIZE: u64 = 6 * 16;

struct Slot {
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
    capacity: usize,
}

struct Resources {
    pipeline: wgpu::RenderPipeline,
    uniform_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    stride: u64,
    format: wgpu::TextureFormat,
    target_is_srgb: bool,
    textures: HashMap<u64, wgpu::BindGroup>,
    slots: HashMap<u64, Slot>,
}

fn device_id() -> egui::Id {
    egui::Id::new("egui_pop_viewport::gpu::device")
}

/// The GPU path's device, when installed in `ctx`.
pub(crate) fn installed(ctx: &Context) -> Option<Device> {
    ctx.data(|d| d.get_temp::<Device>(device_id()))
}

/// Installs the GPU path on a render state; viewports in `ctx` use it
/// from the next frame.
pub fn install(ctx: &Context, rs: &egui_wgpu::RenderState) -> GpuInfo {
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
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
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
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("egui-pop-viewport"),
        bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
        ..Default::default()
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("egui-pop-viewport"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: rs.target_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let align = u64::from(device.limits().min_uniform_buffer_offset_alignment);
    let stride = UNIFORM_SIZE.div_ceil(align) * align;
    let max_texture_side = device.limits().max_texture_dimension_2d;
    let target_is_srgb = rs.target_format.is_srgb();
    rs.renderer.write().callback_resources.insert(Resources {
        pipeline,
        uniform_layout,
        texture_layout,
        stride,
        format,
        target_is_srgb,
        textures: HashMap::new(),
        slots: HashMap::new(),
    });
    ctx.data_mut(|d| {
        d.insert_temp(
            device_id(),
            Device {
                max_texture_side,
                gpu: true,
            },
        );
    });
    GpuInfo {
        max_texture_side,
        half_float: half,
        target_is_srgb,
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

fn texture_bytes(format: wgpu::TextureFormat, data: &[f32]) -> Vec<u8> {
    if format == wgpu::TextureFormat::Rgba16Float {
        data.iter()
            .flat_map(|v| half::f16::from_f32(*v).to_bits().to_le_bytes())
            .collect()
    } else {
        data.iter().flat_map(|v| v.to_le_bytes()).collect()
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
        let (uploads, frees) = {
            let mut q = self
                .queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (std::mem::take(&mut q.uploads), std::mem::take(&mut q.frees))
        };
        for id in frees {
            r.textures.remove(&id);
        }
        for (id, size, data) in uploads {
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
            let bpp = if r.format == wgpu::TextureFormat::Rgba16Float {
                8
            } else {
                16
            };
            queue.write_texture(
                texture.as_image_copy(),
                &texture_bytes(r.format, &data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size[0] * bpp),
                    rows_per_image: Some(size[1]),
                },
                extent,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("egui-pop-viewport.tile"),
                layout: &r.texture_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            });
            r.textures.insert(id, group);
        }
        if self.draws.is_empty() {
            return Vec::new();
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
            return Vec::new();
        };
        let mut bytes = vec![0u8; stride as usize * needed];
        let [sw, sh] = screen.size_in_pixels;
        let m = &self.stage.matrix;
        for (i, d) in self.draws.iter().enumerate() {
            let values: [f32; 24] = [
                d.dst[0],
                d.dst[1],
                d.dst[2],
                d.dst[3],
                sw as f32,
                sh as f32,
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
                if r.target_is_srgb { 1.0 } else { 0.0 },
            ];
            let at = i * stride as usize;
            for (k, v) in values.iter().enumerate() {
                bytes[at + k * 4..at + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        queue.write_buffer(&slot.buffer, 0, &bytes);
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
        let Some(slot) = r.slots.get(&self.slot) else {
            return;
        };
        // Positions are in window pixels; the scissor keeps the clip.
        let [sw, sh] = info.screen_size_px;
        pass.set_viewport(0.0, 0.0, sw as f32, sh as f32, 0.0, 1.0);
        pass.set_pipeline(&r.pipeline);
        for (i, d) in self.draws.iter().enumerate() {
            let Some(texture) = r.textures.get(&d.texture) else {
                continue;
            };
            pass.set_bind_group(0, &slot.group, &[(i as u64 * r.stride) as u32]);
            pass.set_bind_group(1, texture, &[]);
            pass.draw(0..6, 0..1);
        }
    }
}
