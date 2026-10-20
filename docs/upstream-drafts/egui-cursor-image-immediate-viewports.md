# `Context::set_cursor_image` is dropped in immediate viewports

> **Before submitting (delete this block).** Repository: emilk/egui, "Bug
> report" template (label `bug`), or "Feature request" if you prefer to
> present it as a missing piece; fill the OS / version fields. egui's
> CONTRIBUTING ("Agents & LLM policy"): the human filing is responsible for
> the issue; no AI disclosure is required. No duplicate found (searched
> issues and PRs for set_cursor_image / custom cursor / immediate viewport
> cursor, 2026-10-20). The web side is already tracked by #3848 ("Custom
> pointer icons", open; a comment there asks for CSS cursors), so it is only
> referenced here, not filed. Checked against `main` at db63b6d.

**Describe the bug**

`Context::set_cursor_image` (added in #8155) shows the bitmap in eframe's
root and deferred viewports, but not in immediate viewports
(`show_viewport_immediate`): there the image is silently dropped and the
plain `cursor_icon` is shown.

eframe renders immediate viewports with
`egui_winit::State::handle_platform_output`, which has no `ActiveEventLoop`
and so cannot create a `winit::CustomCursor`:

- wgpu: <https://github.com/emilk/egui/blob/db63b6d1765552b345491ae4ee5894de8440cd44/crates/eframe/src/native/wgpu_integration.rs#L1319>
- glow: <https://github.com/emilk/egui/blob/db63b6d1765552b345491ae4ee5894de8440cd44/crates/eframe/src/native/glow_integration.rs#L1855>
- the drop itself: <https://github.com/emilk/egui/blob/db63b6d1765552b345491ae4ee5894de8440cd44/crates/egui-winit/src/lib.rs#L1345-L1349>

#8155 lists this as a known limitation; this issue is to track it. On the
web the image is ignored too
(`crates/eframe/src/web/app_runner.rs`, `TODO(alextournai)`), tracked by
#3848.

Found while building a desktop framework on egui
(<https://github.com/quazardous/egui-pop>), whose tool cursors are bitmaps.

**To Reproduce**

```rust
use std::sync::Arc;

fn main() -> eframe::Result {
    // 16x16 opaque red square, hotspot in the middle.
    let image = egui::CustomCursorImage {
        rgba: Arc::from(vec![255u8, 0, 0, 255].repeat(16 * 16)),
        size: [16, 16],
        hotspot: [8, 8],
    };
    eframe::run_ui_native("cursor", Default::default(), move |ui, _frame| {
        ui.label("Root viewport: red square cursor here.");
        ui.ctx().set_cursor_image(Some(image.clone()));
        let image = image.clone();
        ui.ctx().show_viewport_immediate(
            egui::ViewportId::from_hash_of("immediate"),
            egui::ViewportBuilder::default().with_title("immediate"),
            move |ui, _class| {
                ui.label("Immediate viewport: system arrow here.");
                ui.ctx().set_cursor_image(Some(image.clone()));
            },
        );
    })
}
```

Move the pointer over each window: the root shows the red square, the
immediate viewport shows the default arrow.

**Expected behavior**

The bitmap is shown in every native viewport, immediate ones included.

**Suggested fix (not tried)**

`egui_winit::State` could keep the last `cursor_image` it could not apply
for lack of an event loop, and apply it the next time the integration has
one for that window (eframe's window-event or `about_to_wait` handling);
or eframe could create the `CustomCursor` for immediate viewports from its
own event-loop callbacks and hand it to the state. Either way the
`CustomCursor` cache already keyed by `Arc::as_ptr` keeps it from being
re-created every frame.
