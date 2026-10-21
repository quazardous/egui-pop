//! Device tiles (feature `wgpu`): a tile that already lives on the
//! render device, handed to the viewport with no copy through the CPU.
//!
//! A source that renders on the GPU (an engine running on the device
//! eframe opened) answers a request with a [`DeviceTile`] through
//! [`TileSink::deliver_device`](crate::source::TileSink::deliver_device):
//!
//! - **a texture** ([`DeviceTile::texture`]): any 2D float-sampled format
//!   (`Rgba16Float`, `Rgba32Float`, `Rgba8Unorm`, `Rgba8UnormSrgb`...),
//!   with `TEXTURE_BINDING` usage, read texel by texel;
//! - **a storage buffer** ([`DeviceTile::buffer`]): `f32` values, one to
//!   four channels a pixel, rows `row_floats` apart ([`BufferLayout`]),
//!   with `STORAGE` usage, read in place (the GPU path binds the rows the
//!   tile covers, nothing is copied).
//!
//! Its values are scene-referred ([`Encoding::Scene`], as
//! [`Pixels::Scene`]: the display stage runs on them in the shader, as for
//! uploaded tiles) or display-ready ([`Encoding::Display`], as
//! [`Pixels::Display`]: sRGB-encoded, shown as they are). Alpha is not
//! drawn: the image is opaque, as on the GPU path's uploaded tiles.
//!
//! The resource must have been created on the render state's device (the
//! one [`gpu::install`](crate::gpu::install) was given), and the work
//! writing it submitted to that device's queue before the hand-over. A
//! tile from another device, or one that does not fit, is refused with a
//! [`DeviceTileError`]: the source's fallback ([`DeviceTile::fallback`]),
//! when it offers one, then gives CPU pixels that take the usual path.
//! The viewport's upload counters
//! ([`ViewportState::transfers`](crate::ViewportState::transfers)) count
//! device tiles as bound, with no byte copied.

use std::fmt;

use egui_wgpu::wgpu;

use crate::source::Pixels;

/// What a device tile's values are, as [`Pixels`] says for CPU tiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Encoding {
    /// Linear values in the source's working space: the viewport's
    /// display stage (its matrix, clamp and transfer) runs on them.
    #[default]
    Scene,
    /// sRGB-encoded values, the display transform already applied: shown
    /// as they are (an `…Srgb` texture format is read back to its
    /// encoded values).
    Display,
}

impl Encoding {
    pub fn name(self) -> &'static str {
        match self {
            Encoding::Scene => "scene",
            Encoding::Display => "display",
        }
    }
}

/// Where a tile's pixels are in a storage buffer of `f32` values: pixel
/// `(x, y)` of the image starts at float
/// `offset / 4 + y * row_floats + x * channels`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferLayout {
    /// Bytes from the buffer's start to pixel `(0, 0)`; a multiple of 4.
    pub offset: u64,
    /// Floats from one row to the next.
    pub row_floats: u32,
    /// Floats a pixel: 1 (grey), 2 (grey, alpha), 3 (RGB) or 4 (RGBA).
    pub channels: u32,
}

impl BufferLayout {
    /// Rows of `width` pixels of `channels` floats, packed, from the
    /// buffer's start.
    pub fn packed(width: u32, channels: u32) -> BufferLayout {
        BufferLayout {
            offset: 0,
            row_floats: width.saturating_mul(channels),
            channels,
        }
    }

    /// The bytes to bind for the `size` pixels from `origin`, with
    /// bindings aligned to `align` bytes: `(offset, size, base)`, `base`
    /// being the tile's first float within the binding.
    pub(crate) fn binding(&self, origin: [u32; 2], size: [u32; 2], align: u64) -> (u64, u64, u64) {
        let row = u64::from(self.row_floats);
        let ch = u64::from(self.channels);
        let first = self.offset / 4 + u64::from(origin[1]) * row + u64::from(origin[0]) * ch;
        let end = self.offset / 4
            + (u64::from(origin[1]) + u64::from(size[1]).max(1) - 1) * row
            + (u64::from(origin[0]) + u64::from(size[0])) * ch;
        let align = align.max(4);
        let start = first * 4 / align * align;
        (start, end * 4 - start, first - start / 4)
    }
}

/// The resource a device tile reads.
#[derive(Clone, Debug)]
pub enum Resource {
    /// A 2D texture of a float-sampled format, `TEXTURE_BINDING` usage.
    Texture(wgpu::Texture),
    /// A storage buffer of `f32` values, `STORAGE` usage.
    Buffer {
        buffer: wgpu::Buffer,
        layout: BufferLayout,
    },
}

impl Resource {
    pub fn name(&self) -> &'static str {
        match self {
            Resource::Texture(_) => "texture",
            Resource::Buffer { .. } => "buffer",
        }
    }
}

/// CPU pixels for a device tile the viewport cannot draw.
type Fallback = Box<dyn FnOnce() -> Option<Pixels> + Send>;

/// A tile on the render device: the resource, the device it was created
/// on, what its values are, and where the tile starts in it.
pub struct DeviceTile {
    pub(crate) device: wgpu::Device,
    pub(crate) resource: Resource,
    pub(crate) encoding: Encoding,
    pub(crate) origin: [u32; 2],
    pub(crate) fallback: Option<Fallback>,
}

impl fmt::Debug for DeviceTile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceTile")
            .field("resource", &self.resource)
            .field("encoding", &self.encoding)
            .field("origin", &self.origin)
            .field("fallback", &self.fallback.is_some())
            .finish()
    }
}

impl DeviceTile {
    /// A tile read from `texture`, created on `device`; scene-referred
    /// values, from the texture's top left.
    pub fn texture(device: &wgpu::Device, texture: wgpu::Texture) -> DeviceTile {
        DeviceTile::new(device, Resource::Texture(texture))
    }

    /// A tile read from `buffer`, created on `device`, laid out as
    /// `layout` says; scene-referred values, from pixel `(0, 0)`.
    pub fn buffer(device: &wgpu::Device, buffer: wgpu::Buffer, layout: BufferLayout) -> DeviceTile {
        DeviceTile::new(device, Resource::Buffer { buffer, layout })
    }

    fn new(device: &wgpu::Device, resource: Resource) -> DeviceTile {
        DeviceTile {
            device: device.clone(),
            resource,
            encoding: Encoding::Scene,
            origin: [0, 0],
            fallback: None,
        }
    }

    /// What the values are ([`Encoding::Scene`] by default).
    pub fn encoding(mut self, encoding: Encoding) -> DeviceTile {
        self.encoding = encoding;
        self
    }

    /// The tile starts at pixel `origin` of the resource (a tile cut
    /// from a larger image): its `request.size` pixels from there.
    pub fn origin(mut self, origin: [u32; 2]) -> DeviceTile {
        self.origin = origin;
        self
    }

    /// CPU pixels, made only when the viewport refuses the tile (another
    /// device, no GPU path, a resource that does not fit): they then
    /// take the CPU tiles' path. Called once, on the thread that
    /// delivers the tile, or on the interface thread when the refusal
    /// happens there; `None` gives up (the request fails).
    pub fn fallback(
        mut self,
        pixels: impl FnOnce() -> Option<Pixels> + Send + 'static,
    ) -> DeviceTile {
        self.fallback = Some(Box::new(pixels));
        self
    }

    /// The device the resource was created on.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn resource(&self) -> &Resource {
        &self.resource
    }

    pub fn has_fallback(&self) -> bool {
        self.fallback.is_some()
    }

    /// Bytes of GPU memory the tile's pixels take in its resource.
    pub(crate) fn footprint(&self, size: [u32; 2]) -> usize {
        let n = size[0] as usize * size[1] as usize;
        match &self.resource {
            Resource::Texture(t) => n * t.format().block_copy_size(None).unwrap_or(8) as usize,
            Resource::Buffer { layout, .. } => n * layout.channels as usize * 4,
        }
    }

    /// Whether the viewport can draw it for a tile of `size` pixels on
    /// `render` (`None`: no GPU path installed).
    pub fn check(
        &self,
        render: Option<&RenderDevice>,
        size: [u32; 2],
    ) -> Result<(), DeviceTileError> {
        let Some(render) = render else {
            return Err(DeviceTileError::NoGpuPath);
        };
        if self.device != render.device {
            return Err(DeviceTileError::OtherDevice);
        }
        let [ox, oy] = self.origin;
        let unfit = |why: String| Err(DeviceTileError::Unfit(why));
        match &self.resource {
            Resource::Texture(t) => {
                if !t.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING) {
                    return unfit("the texture lacks TEXTURE_BINDING usage".into());
                }
                if t.dimension() != wgpu::TextureDimension::D2
                    || t.depth_or_array_layers() != 1
                    || t.sample_count() != 1
                {
                    return unfit(
                        "the texture is not a single-sampled 2D texture of one layer".into(),
                    );
                }
                if !matches!(
                    t.format().sample_type(None, None),
                    Some(wgpu::TextureSampleType::Float { .. })
                ) {
                    return unfit(format!("{:?} is not a float-sampled format", t.format()));
                }
                let need = [
                    u64::from(ox) + u64::from(size[0]),
                    u64::from(oy) + u64::from(size[1]),
                ];
                if u64::from(t.width()) < need[0] || u64::from(t.height()) < need[1] {
                    return unfit(format!(
                        "the texture is {}×{}, the tile needs {}×{} from ({ox}, {oy})",
                        t.width(),
                        t.height(),
                        size[0],
                        size[1]
                    ));
                }
            }
            Resource::Buffer { buffer, layout } => {
                if !render.buffers {
                    return Err(DeviceTileError::BuffersUnsupported);
                }
                if !buffer.usage().contains(wgpu::BufferUsages::STORAGE) {
                    return unfit("the buffer lacks STORAGE usage".into());
                }
                if !(1..=4).contains(&layout.channels) {
                    return unfit(format!("{} channels (1 to 4 are drawn)", layout.channels));
                }
                if layout.offset % 4 != 0 {
                    return unfit(format!("offset {} is not a multiple of 4", layout.offset));
                }
                let row = (u64::from(ox) + u64::from(size[0])) * u64::from(layout.channels);
                if u64::from(layout.row_floats) < row {
                    return unfit(format!(
                        "rows of {} floats, the tile needs {row}",
                        layout.row_floats
                    ));
                }
                let (start, bytes, _) = layout.binding(self.origin, size, render.storage_align);
                if start + bytes > buffer.size() {
                    return unfit(format!(
                        "the buffer holds {} bytes, the tile reads up to {}",
                        buffer.size(),
                        start + bytes
                    ));
                }
                if bytes > render.max_binding {
                    return unfit(format!(
                        "the tile's rows take {bytes} bytes, over the device's binding limit of {}",
                        render.max_binding
                    ));
                }
            }
        }
        Ok(())
    }
}

/// The render device the GPU path draws with, as device tiles are
/// checked against it.
#[derive(Clone, Debug)]
pub struct RenderDevice {
    pub device: wgpu::Device,
    /// Storage buffers can be read by the tile shader on this adapter.
    pub buffers: bool,
    /// The device's `min_storage_buffer_offset_alignment`.
    pub storage_align: u64,
    /// The device's `max_storage_buffer_binding_size`.
    pub max_binding: u64,
}

/// Why a device tile was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceTileError {
    /// No GPU path is installed in the context: nothing can bind it.
    NoGpuPath,
    /// The resource was created on another device than the render
    /// state's.
    OtherDevice,
    /// This adapter cannot read storage buffers in a fragment shader.
    BuffersUnsupported,
    /// The resource does not fit the tile (usage, format, size), said.
    Unfit(String),
}

impl fmt::Display for DeviceTileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceTileError::NoGpuPath => {
                f.write_str("no GPU path is installed (gpu::install): device tiles cannot be drawn")
            }
            DeviceTileError::OtherDevice => f.write_str(
                "the tile's resource was created on another wgpu device than the render state's; \
                 create it on the device gpu::install was given",
            ),
            DeviceTileError::BuffersUnsupported => f.write_str(
                "this adapter cannot read storage buffers in a fragment shader: deliver a texture",
            ),
            DeviceTileError::Unfit(why) => write!(f, "the resource does not fit the tile: {why}"),
        }
    }
}

impl std::error::Error for DeviceTileError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packed_buffer_binds_the_rows_a_tile_covers() {
        // A 1000-pixel wide RGBA image, the tile at (256, 512), 256 × 256.
        let layout = BufferLayout::packed(1000, 4);
        let (start, bytes, base) = layout.binding([256, 512], [256, 256], 256);
        let first = 512 * 4000 + 256 * 4;
        let end = (512 + 255) * 4000 + (256 + 256) * 4;
        assert_eq!(start % 256, 0);
        assert_eq!(start / 4 + base, first);
        assert_eq!(start + bytes, end * 4);
        assert!(base * 4 < 256);
    }

    #[test]
    fn an_offset_and_three_channels_are_counted_in_floats() {
        let layout = BufferLayout {
            offset: 64,
            row_floats: 30,
            channels: 3,
        };
        let (start, bytes, base) = layout.binding([2, 1], [4, 2], 4);
        assert_eq!(start / 4 + base, 16 + 30 + 6);
        assert_eq!((start + bytes) / 4, 16 + 2 * 30 + 6 * 3);
    }
}
