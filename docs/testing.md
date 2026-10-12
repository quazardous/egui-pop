# Testing

How egui-pop is tested, and what to know when a test misbehaves.

## Layers

- **Model** (`egui-pop-model`): plain unit tests, no egui, milliseconds.
- **Widgets and applications**: egui_kittest harnesses, finding widgets
  by role and label (never by position), including the keyboard. The
  test kit (`egui-pop-test`) has harnesses for a whole application
  (`app`, `harness_for`), at a given scale (`app_at`, `SCALES`,
  `at_scales`, `set_native_scale`), and checks (`assert_all_named`,
  `assert_keymap_clean`, `a11y_tree`).
- **Snapshots** (`crates/egui-pop-gallery/tests/snapshots`): the gallery's
  pages rendered with wgpu, per theme; thresholds per operating system in
  `kittest.toml`. Regenerate with `UPDATE_SNAPSHOTS=1 cargo test -p
  egui-pop-gallery` and look at the images in the diff.

## One GPU device per test process

Rendering tests run on a software Vulkan driver (lavapipe) in CI and
locally when it is installed. Creating or enumerating wgpu instances and
devices on parallel test threads crashes the Khronos Vulkan loader
1.4.341 (a use-after-free while debug labels are set; Fedora ships it):
the test binary dies with `SIGSEGV`, on any test, at random. It is fixed
upstream in loader 1.4.350 (Vulkan-Loader pull request 1866).

So a test process opens **one** wgpu device and every rendering harness
shares it:

```rust,ignore
use egui_pop_test::SharedGpu as _;

let h = egui_kittest::Harness::builder()
    .shared_wgpu() // instead of .wgpu()
    .build_ui(|ui| { ui.label("hi"); });
```

The shared instance asks for Vulkan alone when a Vulkan adapter exists
(lavapipe): an instance that also enumerates OpenGL makes an EGL context
current on whichever test thread asks, and a shared device would then
be used from another thread. Without Vulkan it falls back to wgpu's
default backends. `egui_pop_test::app_wgpu` uses the shared device. A test that must open a device of its
own holds `egui_pop_test::gpu_lock()` while it does. A harness built
without a renderer opens a device of its own the first time it renders
(kittest's lazy renderer): give it `.shared_wgpu()` when it renders.

Until the loader is updated everywhere, a crashing GPU test binary is
rerun with `--test-threads=1` before concluding anything.
