# Upstream workarounds

The register of what egui-pop compensates for in its dependencies. Each
entry says what is compensated, where it is tracked upstream, how
egui-pop works around it (the code site carries an `// upstream:`
comment pointing here) and when the workaround can go.

Reviewed at every egui bump: remove what upstream fixed, update the rest.

## egui_kittest: no double click from queued events

- **Compensated**: a double click made of queued events never registers,
  because each queued event advances the clock by `step_dt` (0.25 s),
  beyond egui's 0.3 s double-click delay.
- **Upstream**: <https://github.com/emilk/egui/issues/8697>
- **Workaround**: `egui_pop_test::double_click(harness, label)` (and
  `double_click_at`) put the four button events in one frame, so egui
  sees both clicks within its delay instead of `step_dt` apart.
- **Remove when**: kittest ships a `double_click` helper or a per-step
  time.

## egui_table: unnamed resize handles, rows drawn per region

- **Compensated**: the column resize handles are interactive widgets
  without an accessible name; `TableDelegate::row_ui` and
  `header_cell_ui` are called once per region (sticky and scrolled
  columns), also for the sticky region when there are no sticky columns.
- **Upstream**: not filed yet (a name for the handles, and no call for
  an empty region, would do).
- **Workaround**: `egui-pop-views`' data table names each handle after
  its column once the table is drawn, and skips the rows and header
  cells of an empty region (`table.rs`, `// upstream:`).
- **Remove when**: egui_table names its handles and skips empty regions.

## egui_kittest: no way to move a window to a screen of another scale

- **Compensated**: `Harness::set_pixels_per_point` changes egui's zoom
  factor, not the window's native scale, so a test cannot simulate a
  window moved to a screen of another scale (winit's scale change).
- **Upstream**: not filed yet (a `Harness::set_native_pixels_per_point`
  would do).
- **Workaround**: `egui_pop_test::set_native_scale` writes the root
  viewport's `native_pixels_per_point` in the harness's input before the
  next step, as egui-winit does on a scale change.
- **Remove when**: kittest has such a setter.

## egui: keyboard zoom on by default

- **Compensated**: `Options::zoom_with_keyboard` is on by default: Ctrl
  and `+`, `-`, `0` change the zoom factor of every window, the keys an
  application's content zoom (an image viewer's) uses too.
- **Upstream**: a default, not a defect; nothing to file.
- **Workaround**: egui-pop's application turns it off and owns the
  interface zoom (`egui_pop_api::zoom`): a setting in fixed steps, and
  commands bound by default to `Ctrl+Alt+=`, `Ctrl+Alt+-` and
  `Ctrl+Alt+0`, rebindable like any command.
- **Remove when**: never; this is egui-pop's design.

## egui_dock: no per-tab, per-destination drop veto

- **Compensated**: egui_dock has no hook to refuse dropping a given tab
  in a given place (only `allowed_in_windows` and the global
  `AllowedSplits`), while egui-pop panels declare the areas they may live
  in.
- **Upstream**: <https://github.com/anhosh/egui_dock/issues/351> (related
  to #163 there).
- **Workaround**: each area is a dock of its own, its tabs never allowed
  into egui_dock's windows; a tab dropped over another area is moved
  there by the workbench (`egui_pop_api::workbench`) when the panel's
  rules allow it, and refused otherwise, with the drop hint saying why
  and the refusal reported (no silent no-op, ADR 0003). `Layout::repair`
  moves panels found in an area they may not be in.
- **Remove when**: the hook lands in egui_dock.

## egui: symbols missing from the default fonts

- **Compensated**: symbols such as ✓ ▸ ◆ ● render as missing glyphs with
  egui's default fonts.
- **Upstream**: fixed by <https://github.com/emilk/egui/pull/8491>
  ("Render any glyph on native", merged, expected in egui 0.37).
- **Workaround**: icons are named by role (`egui_pop_core::icon::Icon`)
  and drawn from the Phosphor icon font (`phosphor` feature), installed
  in a family of its own, or painted with lines and fills; widgets never
  put symbols in text. egui's `SubMenuButton` arrow (`⏵`) is replaced
  by a painted chevron in egui-pop's menus.
- **Remove when**: egui-pop moves to egui 0.37.

## eframe: no list of screens when a window is placed

- **Compensated**: a window's place is restored before the window
  exists, when eframe gives no list of the screens (only the current
  monitor's size afterwards, with no origin), so a place on a screen that
  is gone cannot be told from one on a second screen.
- **Upstream**: related to <https://github.com/emilk/egui/issues/4073>
  (choose the monitor at launch); a request for the screens' rectangles
  in `ViewportInfo` is not filed yet.
- **Workaround**: `egui_pop_api::window::WindowState::repair` takes the
  screens when they are known (tested with sample layouts); at start-up
  only the size is bounded and a position far away is dropped. Sizes are
  kept under 8192 points per side, past which the surface cannot be
  configured (<https://github.com/emilk/egui/issues/8361>).
- **Remove when**: eframe tells the screens before the window is placed.

## winit: no hit-test hook for a custom title bar on Windows

- **Compensated**: Windows 11 shows its Snap Layouts flyout only when the
  maximize button's area answers `HTMAXBUTTON` to `WM_NCHITTEST`; winit
  has no hook to answer it, so a title bar drawn by the application loses
  Snap Layouts.
- **Upstream**: <https://github.com/rust-windowing/winit/issues/3884>
  (open since 2024), <https://github.com/rust-windowing/winit/issues/2413>
  (a `HitTest` event) and its pull request
  <https://github.com/rust-windowing/winit/pull/3207>, not updated since
  2024. Maintainers point to subclassing the window outside winit; a 2026
  comment on the issue notes that subclassing does not help when a child
  window covers the client area.
- **Workaround**: egui-pop's frame (`egui_pop_core::frame`) publishes
  each window's hit regions every frame, in physical client pixels
  (`HitTable`, `HitRegions::hit` mapping a point to caption, minimize,
  maximize, close or a resize edge). On Windows, `FramePlugin` (when the
  application draws the frame) subclasses the main window through
  `egui-pop-win32` (`SetWindowSubclass`, the one crate allowed `unsafe`,
  with no egui dependency) and answers `WM_NCHITTEST` from them:
  `HTCAPTION` (Windows moves the window, with Aero Snap), `HTMINBUTTON`,
  `HTMAXBUTTON` (Snap Layouts) and `HTCLOSE`, the resize codes, else
  `HTCLIENT`. The buttons' non-client clicks never reach winit, so the
  layer swallows them and hands them back with hover, as are a double
  click and a right click on the caption: the `frame.*` steps still run.
  The layer is installed and removed through the `frame.hit_test` step.
  Limits: only the main window (its handle comes from eframe's creation
  context; auxiliary windows keep egui's own hit handling), and a child
  window covering the client area would take the hit tests first (the
  2026 comment above).
- **Remove when**: winit can answer a hit test for a point of the window.

## egui-wgpu: no surface format or colour space to choose

- **Compensated**: wgpu 30 configures surfaces in any colour space the
  platform offers (`SurfaceConfiguration::color_space`: extended sRGB,
  scRGB, Display P3, BT.2100 PQ and HLG) and reports them per format
  (`SurfaceCapabilities::color_spaces`) with the display's headroom
  (`Surface::display_hdr_info`). eframe 0.36 does not let an application
  use any of it: egui-wgpu picks the format itself
  (`preferred_framebuffer_format`: `Rgba8Unorm` or `Bgra8Unorm`), and
  configures the surface with wgpu's default colour space
  (`configure_surface` builds on `get_default_config`); `WgpuConfiguration`
  has no field for either. Its renderer also writes sRGB-encoded values
  to every format that is not `*Srgb` (`fs_main_gamma_framebuffer`), so
  the interface is right only on sRGB and extended sRGB surfaces.
- **Upstream**: not filed yet. What would plug in: a preferred format
  and colour space in `egui_wgpu::WgpuConfiguration` (used by
  `RenderState::create` and `configure_surface`), and the renderer's
  fragment entry chosen by the colour space's transfer (linear for scRGB,
  an encoding pass for PQ and HLG) rather than by `is_srgb()`.
- **Workaround**: none possible inside eframe. The viewport's display
  stage implements every transfer and the P3 and BT.2020 primaries (CPU
  and shader); it reports truthfully what an eframe window draws (sRGB,
  with the reason, `display::Resolution`), and a host with its own
  surface configures it with `egui_pop_viewport::gpu::choose_surface`
  and declares it (`gpu::install_for_surface`, `display::set_target`).
- **Remove when**: egui-wgpu takes a surface format and colour space;
  then `gpu::install` reads them from the render state.

## Wayland: no colour management through winit

- **Compensated**: on Wayland, colour management is the
  `wp_color_management_v1` protocol (staging, in wayland-protocols since
  1.41): a surface gets an image description (primaries, transfer,
  luminances), and the compositor gives each output's preferred one.
  winit 0.30 binds none of it (no colour-management object, no event for
  the output's description), and wgpu has no Wayland-specific path: on
  Vulkan it relies on the driver's window system integration exposing
  extra colour spaces through `VK_EXT_swapchain_colorspace` (recent Mesa
  drivers do so on compositors with the protocol), and on GL it offers
  sRGB only.
- **Upstream**: winit has no issue for `wp_color_management_v1` that
  egui-pop could find; it would sit with the other Wayland protocol
  bindings of winit's Wayland backend (`platform_impl/linux/wayland`).
- **Missing, and where it would plug in**:
  - the surface's encoding: through wgpu's Vulkan colour spaces where
    the driver offers them, then through egui-wgpu as above;
    `gpu::install_for_surface` already takes the colour space;
  - the output's preferred description (its primaries, its luminances
    and so the headroom): winit would need to expose it per window and
    signal its changes; egui-pop would turn it into
    `DisplayTarget::sdr_white_nits` and `headroom` with
    `TargetSource::Os`, set with `display::set_target`;
  - without a Vulkan driver offering the colour spaces (GL, older
    Mesa), an application would have to tag the surface itself through
    the protocol (`TargetSource::Tagged`), which needs the
    `wl_surface` winit gives and a protocol binding of its own.
- **Workaround**: none: Wayland windows draw sRGB, and the viewport says
  so (`display::Resolution::reason`).
- **Remove when**: winit exposes the output's image description and
  egui-wgpu takes a colour space.
