---
egui-pop-api: minor
egui-pop-core: minor
egui-pop-viewport: minor
egui-pop-winit: minor
egui-pop: minor
---

A detached panel's window can get its own image surface. The runner now pairs immediate viewports' windows too (a detached panel's window, painted inside its parent's pass), so `runner::window` answers for them; a host that runs the winit event loop itself pairs every window by calling `runner::host`'s hooks from its handler (`created`, `resumed`, `window_event`, `redraw`, `detach`; feature `host-loop`, implied by `create-native`). `AppBuilder::transparent_panel_windows` builds detached panels' windows transparent on Wayland, where the image surface can show through, and opaque elsewhere; each window says how it was built and why (`egui_pop_core::window_surface::Transparency`), and `windows.list` shows it with whether the window's handles are known (`WindowRecord::transparency`, `handles`). The image surface reads that: an opaque window gets none, with its builder's reason. `runner::windowing` (`window::Windowing`) tells the windowing system; `egui_pop_winit::Display::from_raw` makes a display from its raw handle.
