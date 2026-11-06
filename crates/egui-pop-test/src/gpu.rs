//! One wgpu device per test process (feature `wgpu`).
//!
//! Creating or enumerating wgpu instances and devices on parallel test
//! threads crashes some Vulkan loaders (a use-after-free in the Khronos
//! Vulkan loader 1.4.341, fixed in 1.4.350; see `docs/testing.md`).
//! Harnesses built with [`SharedGpu::shared_wgpu`] render through one
//! device opened once per process; a test that must open its own takes
//! [`gpu_lock`] around it.

use std::sync::{Mutex, MutexGuard};

use egui_kittest::HarnessBuilder;
use egui_kittest::wgpu::{WgpuTestRenderer, create_render_state, default_wgpu_setup};
use egui_wgpu::{RendererOptions, WgpuSetup, WgpuSetupExisting};

static LOCK: Mutex<()> = Mutex::new(());
static SHARED: Mutex<Option<Result<WgpuSetupExisting, String>>> = Mutex::new(None);

/// The process-wide lock around opening a wgpu device: take it when a
/// test must open one of its own.
pub fn gpu_lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// The process's device (opened on first use, under [`gpu_lock`]).
///
/// # Panics
///
/// When no wgpu adapter can be opened ([`try_shared_setup`] says why).
pub fn shared_setup() -> WgpuSetupExisting {
    try_shared_setup().unwrap_or_else(|e| panic!("{e}"))
}

/// The process's device, or why none opens (no adapter: no GPU driver,
/// no lavapipe). Tried once per process.
pub fn try_shared_setup() -> Result<WgpuSetupExisting, String> {
    let _guard = gpu_lock();
    let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    shared
        .get_or_insert_with(|| {
            // Vulkan alone where there is a Vulkan adapter (lavapipe): an
            // instance that also enumerates OpenGL makes an EGL context
            // current on whichever test thread asks, and the shared
            // device would then be used from another.
            let vulkan = std::panic::catch_unwind(|| {
                let mut setup = default_wgpu_setup();
                if let WgpuSetup::CreateNew(new) = &mut setup {
                    new.instance_descriptor.backends = egui_wgpu::wgpu::Backends::VULKAN;
                }
                create_render_state(setup, RendererOptions::PREDICTABLE)
            });
            let rs = vulkan.or_else(|_| {
                std::panic::catch_unwind(|| {
                    create_render_state(default_wgpu_setup(), RendererOptions::PREDICTABLE)
                })
            });
            match rs {
                Ok(rs) => Ok(WgpuSetupExisting {
                    instance: rs.instance.clone(),
                    adapter: rs.adapter.clone(),
                    device: rs.device.clone(),
                    queue: rs.queue.clone(),
                }),
                Err(_) => Err("no wgpu adapter opens here (no GPU driver, no lavapipe)".into()),
            }
        })
        .clone()
}

/// Set to compare snapshots on any adapter, not only a software one.
pub const ANY_GPU: &str = "EGUI_POP_SNAPSHOT_ANY_GPU";

/// Why snapshots are not compared in this process, if they are not:
/// no adapter opens, or the one that opens is a GPU rather than a
/// software rasterizer (lavapipe, Mesa's software Vulkan, which renders
/// the pictures kept in the repositories and in CI; a GPU draws edges a
/// little differently, past the tolerances). `None` with a software
/// adapter, when [`ANY_GPU`] is set, or when the pictures are being
/// written (`UPDATE_SNAPSHOTS`).
pub fn snapshot_skip_reason() -> Option<String> {
    let setup = match try_shared_setup() {
        Ok(setup) => setup,
        Err(e) => {
            return Some(format!(
                "{e}; install lavapipe (Mesa's software Vulkan) to compare snapshots"
            ));
        }
    };
    let set = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty() && v != "0");
    if set(ANY_GPU) || set("UPDATE_SNAPSHOTS") {
        return None;
    }
    let info = setup.adapter.get_info();
    (info.device_type != egui_wgpu::wgpu::DeviceType::Cpu).then(|| {
        format!(
            "the adapter is `{}`, not a software one: snapshots are compared on lavapipe \
             (Mesa's software Vulkan), as CI renders them; install it, or set {ANY_GPU}=1 \
             to compare on this GPU",
            info.name
        )
    })
}

/// A renderer drawing with the process's device (a renderer of its own,
/// the device shared).
pub fn shared_renderer() -> WgpuTestRenderer {
    let setup = shared_setup();
    let _guard = gpu_lock();
    WgpuTestRenderer::from_setup(WgpuSetup::Existing(setup))
}

/// Renders a harness with the process's wgpu device.
pub trait SharedGpu {
    /// As `HarnessBuilder::wgpu`, through the shared device.
    fn shared_wgpu(self) -> Self;
}

impl<S> SharedGpu for HarnessBuilder<S> {
    fn shared_wgpu(self) -> Self {
        self.renderer(shared_renderer())
    }
}
