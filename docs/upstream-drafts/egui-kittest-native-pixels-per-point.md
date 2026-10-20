# egui_kittest: `Harness::set_native_pixels_per_point` to simulate a window moving to a screen of another scale

> **Before submitting (delete this block).** Repository: emilk/egui
> (egui_kittest lives there), "Feature request" template (label
> `feature-request`). egui's CONTRIBUTING ("Agents & LLM policy"): the human
> filing is responsible for the issue; no AI disclosure is required. A PR is
> welcome for small things, opened as a draft until a human reviewed it. No
> duplicate found (searched issues and PRs for kittest + pixels_per_point /
> native_pixels_per_point / scale / dpi, 2026-10-20). Checked against `main`
> at db63b6d.

**Is your feature request related to a problem? Please describe.**

I want to test how an app reacts when its window moves to a monitor with
another scale factor (winit's `ScaleFactorChanged`), for example that it
keeps its layout or re-rasterises its icons. A harness has no way to do
that:

- `HarnessBuilder::with_pixels_per_point` sets the root viewport's
  `native_pixels_per_point`, but only when the harness is built:
  <https://github.com/emilk/egui/blob/db63b6d1765552b345491ae4ee5894de8440cd44/crates/egui_kittest/src/lib.rs#L242-L243>
- `Harness::set_pixels_per_point` calls `Context::set_pixels_per_point`,
  which changes the zoom factor, not the native scale:
  <https://github.com/emilk/egui/blob/db63b6d1765552b345491ae4ee5894de8440cd44/crates/egui_kittest/src/lib.rs#L359-L366>
  and
  <https://github.com/emilk/egui/blob/db63b6d1765552b345491ae4ee5894de8440cd44/crates/egui/src/context.rs#L2720-L2724>

So `ctx.native_pixels_per_point()` and `ctx.zoom_factor()` in a test never
look like they do after a real monitor change.

Found while building a desktop framework on egui
(<https://github.com/quazardous/egui-pop>).

**Describe the solution you'd like**

```rust
impl<State> Harness<'_, State> {
    /// Simulate the window moving to a screen with another scale factor,
    /// as egui-winit reports it on `ScaleFactorChanged`.
    /// The zoom factor is unchanged.
    pub fn set_native_pixels_per_point(&mut self, native_pixels_per_point: f32) -> &mut Self {
        self.input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(native_pixels_per_point);
        self
    }
}
```

and perhaps a doc line on `set_pixels_per_point` saying it changes the zoom
factor.

**Describe alternatives you've considered**

Writing the field through `Harness::input_mut()` before the next step,
which works but needs knowing egui-winit's internals:

```rust
harness
    .input_mut()
    .viewports
    .entry(egui::ViewportId::ROOT)
    .or_default()
    .native_pixels_per_point = Some(2.0);
harness.run();
assert_eq!(harness.ctx.native_pixels_per_point(), Some(2.0));
```

**Additional context**

Happy to open a small PR with the setter and a test if that is welcome.
