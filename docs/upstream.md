# Upstream workarounds

The register of what egui-pop compensates for in its dependencies. Each
entry says what is compensated, where it is tracked upstream, how
egui-pop works around it (the code site carries an `// upstream:`
comment pointing here) and when the workaround can go.

Reviewed at every egui bump: remove what upstream fixed, update the rest.

## How a limit is handled

1. **Work around it in egui-pop**, at the narrowest place, with an
   `// upstream:` comment at the code site. An application or an
   extension never has to carry the workaround itself.
2. **Search upstream** (egui's issues and pull requests, or the
   dependency's) for the same limit. If none exists, **file a focused
   issue** with a minimal reproduction, written against upstream's code
   alone; a fix, when it is small and clear, goes with it as a pull
   request.
3. **Record it here**: what is compensated, where it is tracked
   upstream ("not filed yet" says it must be), the workaround, and when
   it can go.
4. **At every egui bump**, review the register: remove the workarounds
   upstream fixed (and their entries), update the rest.

An extension author who meets a limit egui-pop could compensate for
everyone opens an issue here, or a pull request adding the entry and
its workaround. Workarounds are not part of the extension contract
([GOVERNANCE.md](../GOVERNANCE.md)): they go when upstream fixes the
limit.

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

## egui: a selectable button changes size under the pointer

- **Compensated**: `Button::selectable` draws no frame at rest (neither
  selected nor hovered), and its frameless margin is the button padding
  plus the state's expansion minus its stroke width, where the framed
  margin comes to the padding alone. A theme with a stroke at rest
  (egui-pop's 1 px border) makes the button 2 px smaller at rest than
  hovered: a list of them changes height under the pointer, and a
  wrapped label can wrap differently.
- **Upstream**: <https://github.com/emilk/egui/issues/8125> (open). Fixed
  on egui's main and in 0.37.0: the default style hides the inactive frame
  with `Frame::invisible`, keeping its stroke and margins
  (`theme/default_style.rs`).
- **Workaround**: egui-pop's selectable rows (`widgets::SelectableRow`)
  keep egui's sizes, and `SelectableRow::steady` keeps the framed size
  in every state: the rows of a `Choose` dialog are steady
  (`dialogs::choice_row`, `// upstream:`).
- **Remove when**: egui-pop moves to egui 0.37 (check that a selectable
  row keeps its size under the pointer).

## egui: input in the pass of an AccessKit focus is lost

- **Compensated**: an AccessKit `Focus` action on a text field (from a
  screen reader, an agent, or a test) is only noted when egui reads the
  pass's input (`Focus::begin_pass`); the focus is handed over when the
  field registers its interest in it, as it is allocated. A `TextEdit`
  reads its events earlier, while its text is laid out, so the keys and
  text that arrive in the same pass as the action are dropped: the field
  gets the keyboard, not what was typed with it. Later passes are fine,
  which is why a kittest test (one queued event per step) does not see
  it; a platform batching the action with the keys does.
- **Upstream**: fixed in egui 0.37.0: `Focus::begin_pass` focuses the
  widget the action names at once, looked up among the focusable widgets
  of the previous pass (`memory/mod.rs`), before anything reads the
  input. No issue to file.
- **Workaround**: the focus plugin (`egui_pop_core::focus::install`,
  which an `App` installs) holds back the keyboard input that follows an
  AccessKit `Focus` action in the same pass and replays it, in order, at
  the start of the next pass, when the field has the focus
  (`focus::hold_after_accesskit_focus`, `// upstream:`). It covers every
  text field in the context, egui-pop's and an application's own.
- **Remove when**: egui-pop moves to egui 0.37 (check that text typed in
  the pass of an AccessKit focus reaches the field without the plugin).

## egui: popups built from the context's style, not the opening `Ui`'s

- **Compensated**: egui builds the `Ui` of every `Area` from the
  context's style (`Ui::new` falls back to `ctx.global_style()`; `Area`
  has no style of its own), and every popup is an `Area`: a combo box's
  list, a tooltip (`on_hover_text`, `on_hover_ui`), a context menu, a
  menu (`menu_button`, `MenuBar`), egui's `Popup` and `Modal`, windows.
  A style set on one `Ui` (a theme drawn in one part of a window, egui-pop's
  `ScopedTheme`) does not reach the popups opened inside it. The hooks
  that exist (`Popup::style`, `ComboBox::popup_style`,
  `MenuConfig::style`, all a `StyleModifier` applied over the context's
  style) only help at call sites one owns: `Response::on_hover_text`
  and `Response::context_menu` take none, and a widget of another crate
  opening a popup cannot be reached. The same in egui 0.37.0 (`ui.rs`,
  `containers/popup.rs`); its experimental style providers see a
  popup's `UiStack`, which does not lead back to the `Ui` that opened it.
- **Upstream**: <https://github.com/emilk/egui/issues/4692> (tooltips
  should inherit the parent's style, open; `ComboBox::popup_style`,
  <https://github.com/emilk/egui/pull/7360>, covers combo boxes at the
  call site only). A comment widening it to every popup, with a
  proposed API, is not posted yet (draft:
  [upstream-drafts/egui-popups-inherit-ui-style.md](upstream-drafts/egui-popups-inherit-ui-style.md)).
- **Workaround**: while a `ScopedTheme` draws, egui's context styles
  (dark and light) are the scoped theme's, made from the installed
  style and kept per scope, and the installed ones are put back when
  the scope ends, even after a panic (`ScopedTheme::within`,
  `// upstream:` in `theme.rs`). egui 0.36 draws its popups while the
  widget that opens them is drawn, so they follow the scope. A popup
  egui-pop draws later, out of the scope (`menus::popup_at`, drawn by
  the shell), records the scope it was opened in and is drawn within
  it. Limits: egui's light or dark mode (`ctx.theme()`) stays the
  installed one, and a change to the context's style made while a
  scope draws is undone when it ends (themes are installed between
  frames).
- **Remove when**: egui's popups take the style of the `Ui` they are
  opened from; `ScopedTheme::within` then only sets egui-pop's tokens
  and kit (still needed for a popup drawn after its scope).

## egui: AccessKit bounds ignore layer transforms

- **Compensated**: a widget in a transformed layer (egui's `Scene`)
  reports its rectangle to AccessKit in the layer's coordinates, before
  the transform: screen readers (and kittest, which clicks at a node's
  bounds) place it where it is not.
- **Upstream**: not filed yet (draft:
  [upstream-drafts/egui-accesskit-bounds-layer-transform.md](upstream-drafts/egui-accesskit-bounds-layer-transform.md)).
- **Workaround**: the node graph editor sets the bounds of its nodes,
  ports, chevrons, groups, stacks and sliders to their rectangles on
  screen (`graph::Cx::bounds`, `// upstream:`).
- **Remove when**: egui applies the layer's transform to the bounds it
  reports.

## egui: text blurs in a Scene zoomed past 100 %

- **Compensated**: egui draws text at its size and scales the layer, so
  a `Scene` zoomed in shows blurred glyphs.
- **Upstream**: <https://github.com/emilk/egui/issues/4813>
- **Workaround**: the node graph editor stops at 150 % by default
  (`GraphEditor::zoom_range`), and its overview draws titles at a few
  larger sizes rather than scaling small ones.
- **Remove when**: egui lays text out at the layer's scale.

## eframe: no custom cursor on the web or in immediate viewports

- **Compensated**: `Context::set_cursor_image` reaches winit only where
  egui-winit has the event loop (eframe's root and deferred viewports);
  immediate viewports and the web drop the bitmap.
- **Upstream**: web: <https://github.com/emilk/egui/issues/3848> (custom
  pointer icons, open; the web integration carries a TODO for CSS
  `url(...)` cursors). Immediate viewports: a known limitation of
  <https://github.com/emilk/egui/pull/8155>, not filed yet (draft:
  [upstream-drafts/egui-cursor-image-immediate-viewports.md](upstream-drafts/egui-cursor-image-immediate-viewports.md)).
- **Workaround**: every bitmap cursor is shown with a system cursor as
  its fallback (`cursor::apply`, `// upstream:`): the tool cursor's
  nearest system cursor, or a registered image's own fallback.
- **Remove when**: the bitmap reaches the system everywhere.

## winit: no system cursor size

- **Compensated**: winit does not say how large the system's cursors
  are, so tool cursors could not match them.
- **Upstream**: <https://github.com/rust-windowing/winit/issues/3788>
  (open); a comment with this use case is not posted yet (draft:
  [upstream-drafts/winit-cursor-size-comment.md](upstream-drafts/winit-cursor-size-comment.md)).
- **Workaround**: `cursor::system_size` reads `XCURSOR_SIZE` (X11 and
  Wayland desktops), else 24 points; `CursorSettings::size` overrides
  it.
- **Remove when**: winit reports the cursor size (Windows'
  `SM_CXCURSOR`, the desktop's cursor theme size).

## egui_table: unnamed resize handles, rows drawn per region

- **Compensated**: the column resize handles are interactive widgets
  without an accessible name; `TableDelegate::row_ui` and
  `header_cell_ui` are called once per region (sticky and scrolled
  columns), also for the sticky region when there are no sticky columns.
- **Upstream**: egui_table lives in rerun-io/egui_crates now. Not filed
  yet (drafts:
  [upstream-drafts/egui_crates-table-resize-handle-name.md](upstream-drafts/egui_crates-table-resize-handle-name.md)
  and
  [upstream-drafts/egui_crates-table-row-ui-empty-sticky-region.md](upstream-drafts/egui_crates-table-row-ui-empty-sticky-region.md)).
  On egui_table's main the header is drawn once, so only `row_ui` is still
  called for the empty region.
- **Workaround**: `egui-pop-views`' data table names each handle after
  its column once the table is drawn, and skips the rows and header
  cells of an empty region (`table.rs`, `// upstream:`).
- **Remove when**: egui_table names its handles and skips empty regions.

## egui_kittest: no way to move a window to a screen of another scale

- **Compensated**: `Harness::set_pixels_per_point` changes egui's zoom
  factor, not the window's native scale, so a test cannot simulate a
  window moved to a screen of another scale (winit's scale change).
- **Upstream**: not filed yet (draft:
  [upstream-drafts/egui-kittest-native-pixels-per-point.md](upstream-drafts/egui-kittest-native-pixels-per-point.md),
  proposing `Harness::set_native_pixels_per_point`).
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
  `egui-pop-win32` (`SetWindowSubclass`, `unsafe` in one module,
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
- **Workaround**: none possible inside eframe for the window's own
  surface. The viewport's display stage implements every transfer and
  the P3 and BT.2020 primaries (CPU and shader); it reports truthfully
  what an eframe window draws (sRGB, with the reason,
  `display::Resolution`); a host with its own surface configures it with
  `egui_pop_viewport::gpu::choose_surface` and declares it
  (`gpu::install_for_surface`, `display::set_target`); on Wayland the
  image gets a surface of its own (next entry). The issue text is
  drafted below, under the Wayland entry.
- **Remove when**: egui-wgpu takes a surface format and colour space;
  then `gpu::install` reads them from the render state.

## egui-winit: copy, cut and paste arrive as events, their keys swallowed

- **Compensated**: egui-winit turns the platform's copy, cut and paste
  chords (Command or Ctrl with C, X, V; Ctrl+Insert, Shift+Insert,
  Shift+Delete on Windows) into `Event::Copy`, `Event::Cut` and
  `Event::Paste(text)` and sends no key event: a command bound to them
  in the keymap never runs. A paste is read synchronously, as text
  only, and nothing reads the primary selection.
- **Upstream**: not filed yet.
- **Workaround**: while a clipboard endpoint has the focus (and no text
  field does), the clipboard plugin takes the three events and runs
  `edit.copy`, `edit.cut`, `edit.paste` (Shift held: paste special)
  with the pasted text (`clipboard::pass`, `// upstream:`); the keymap
  binds the same keys for every other backend (kittest, the web).
- **Remove when**: egui-winit sends the key events (or lets a keymap
  claim them) and offers a pull model for richer formats.

## Wayland: no colour management through winit

- **Compensated**: on Wayland, colour management is the
  `wp_color_management_v1` protocol (staging, in wayland-protocols since
  1.41): a surface gets an image description (primaries, transfer,
  luminances), and the compositor gives each surface's preferred one.
  winit 0.30 binds none of it (no colour-management object, no event for
  the preferred description), eframe configures every window 8-bit sRGB
  (above), and wgpu's Vulkan path relies on the driver: Mesa's Wayland
  window system integration (25.1 and later) maps some
  `VkColorSpaceKHR` to the protocol, none of them BT.2020 with a 2.2
  power, and holds the colour-management object of every swapchain's
  surface when the compositor announces the `srgb` transfer to it
  (research: [wayland-colour.md](wayland-colour.md)).
- **Upstream**: winit's
  <https://github.com/rust-windowing/winit/issues/4131> (colour
  management getters and events, open); the wgpu, egui-wgpu and Mesa
  requests below are not filed yet.
- **Workaround**: the image gets a surface of its own
  (`egui_pop_viewport::image_surface`, `// upstream:` in
  `image_surface.rs`): `egui-pop-wayland` joins winit's connection from
  the window's raw handles, makes a subsurface below the window, tags it
  through the protocol (or lets the driver declare a colour space where
  it holds the surface), and reads the preferred description through
  `get_surface_feedback`; the viewport renders into it with wgpu and
  punches a hole in the interface. It needs a transparent window and
  serves the main window only.
- **Remove when**: egui-wgpu takes a surface colour space and wgpu
  offers one for the wanted encoding (or a pass-through one an
  application can tag), and winit exposes the preferred description;
  then the window's own surface carries the image, and the interface is
  encoded for it.

### Drafted requests (not filed)

**wgpu: a pass-through surface colour space, and BT.2020 linear.**

> Title: `SurfaceColorSpace::PassThrough` and `Bt2020Linear` on Vulkan
>
> wgpu 30 configures a surface's colour space (`SurfaceConfiguration::color_space`,
> #9658), mapping `SRGB_NONLINEAR`, `EXTENDED_SRGB_LINEAR`,
> `EXTENDED_SRGB_NONLINEAR`, `DISPLAY_P3_NONLINEAR`, `HDR10_ST2084` and
> `HDR10_HLG` (`wgpu-hal/src/vulkan/conv.rs`); the others a driver
> reports are dropped (`unknown_color_spaces_are_dropped`). Two of them
> matter on Wayland:
>
> - `VK_COLOR_SPACE_PASS_THROUGH_EXT`: Mesa's Wayland WSI then leaves
>   the surface's `wp_color_management_surface_v1` alone. With any other
>   colour space, Mesa creates that object (one is allowed per
>   `wl_surface`) on compositors announcing the `srgb` transfer, even for
>   plain sRGB, so an application that describes its surface itself
>   through the protocol (an ICC profile, BT.2020 with a 2.2 power,
>   custom primaries) cannot use a wgpu swapchain there: tagging it is a
>   fatal `surface_exists` error.
> - `VK_COLOR_SPACE_BT2020_LINEAR_EXT`: Mesa maps it to `bt2020` +
>   `ext_linear` wherever the compositor announces both (GNOME and sway
>   included, which offer no scRGB), a wide SDR encoding.
>
> Proposal: `SurfaceColorSpace::PassThrough` (documented as "the
> presentation engine applies no colour management; the application
> describes the surface by other means") and `Bt2020Linear`, with their
> `SurfaceColorSpaces` flags, mapped on Vulkan only and reported from
> `vkGetPhysicalDeviceSurfaceFormatsKHR` as the others are.

PR idea: two variants in `wgpu-types/src/surface.rs` and their flags,
the two arms in `wgpu-hal/src/vulkan/conv.rs` both ways, a line in the
`03_hdr_surface` example's list.

**egui-wgpu / eframe: a surface colour space** (see also the entry
above).

> Title: Let `WgpuConfiguration` choose the surface format and colour space
>
> egui-wgpu picks the surface format (`preferred_framebuffer_format`) and
> configures wgpu's default colour space (`configure_surface` builds on
> `get_default_config`); an application cannot ask for a wide-gamut or
> HDR surface, which wgpu 30 supports (`SurfaceConfiguration::color_space`,
> `SurfaceCapabilities::format_capabilities`). The renderer also picks
> its output encoding from `format.is_srgb()` (`fs_main_linear_framebuffer`
> or `fs_main_gamma_framebuffer`), so on a linear half-float surface
> (scRGB) the interface would be written gamma-encoded.
>
> Proposal: `WgpuConfiguration::surface_format` and `surface_color_space`
> as callbacks over the surface's capabilities (`Fn(&SurfaceCapabilities)
> -> Option<(TextureFormat, SurfaceColorSpace)>`, default: today's
> choice), used by `RenderState::create` and `configure_surface`; the
> chosen colour space exposed in `RenderState`; the renderer's output
> transfer chosen from it (linear for scRGB and the linear spaces,
> sRGB-encoded otherwise), with a hook for a matrix and transfer
> (Display P3, PQ) later.

PR idea: the two fields and their default in `egui-wgpu/src/lib.rs`,
their use in `winit.rs` (`configure_surface`) and `RenderState::create`,
and a `Renderer` option for the output transfer replacing the
`is_srgb()` test; eframe passes them through `NativeOptions::wgpu_options`.

**winit: the preferred image description** (a comment on
rust-windowing/winit#4131 rather than a new issue).

> On Wayland, `wp_color_manager_v1.get_surface_feedback` gives a
> window's preferred image description (primaries, transfer,
> luminances, the display's target volume) and `preferred_changed` its
> changes, which is what an application needs to choose an encoding and
> an HDR headroom per window. A `Window::preferred_color_description()`
> (a plain struct of those values) and a `WindowEvent::ColorDescriptionChanged`
> would cover it; binding the manager in winit's Wayland backend and one
> feedback object per window is enough. Today an application binds it on
> winit's connection through the raw handles (as egui-pop does).

**Mesa (Wayland WSI): leave sRGB surfaces untouched** (for the record;
Mesa's call).

> `needs_color_surface()` creates a `wp_color_management_surface_v1`
> for `VK_COLOR_SPACE_SRGB_NONLINEAR_KHR` when the compositor announces
> the `srgb` primaries and transfer, but `wsi_wl_swapchain_update_colorspace`
> never sets a description for it. The object only takes the surface's
> single slot, so a client describing the surface itself (a subsurface
> with an ICC profile, for one) cannot. Creating it lazily, when a
> description is set, would leave sRGB swapchains untouched. Separately,
> `SRGB_NONLINEAR` and `DISPLAY_P3_NONLINEAR` map to the `srgb` transfer,
> deprecated in version 2 and not announced by KWin, wlroots or Weston,
> so Display P3 is offered only on GNOME and Hyprland; `gamma22` (or
> `compound_power_2_4` on version 2) would offer it everywhere.
