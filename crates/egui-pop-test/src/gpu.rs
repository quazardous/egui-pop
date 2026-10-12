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
static SHARED: Mutex<Option<WgpuSetupExisting>> = Mutex::new(None);

/// The process-wide lock around opening a wgpu device: take it when a
/// test must open one of its own.
pub fn gpu_lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// The process's device (opened on first use, under [`gpu_lock`]).
pub fn shared_setup() -> WgpuSetupExisting {
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
            let rs = vulkan.unwrap_or_else(|_| {
                create_render_state(default_wgpu_setup(), RendererOptions::PREDICTABLE)
            });
            WgpuSetupExisting {
                instance: rs.instance.clone(),
                adapter: rs.adapter.clone(),
                device: rs.device.clone(),
                queue: rs.queue.clone(),
            }
        })
        .clone()
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
