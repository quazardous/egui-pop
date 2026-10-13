# Wide gamut on Wayland: tagging the image

Research note, 2026-10-13. Versions: egui and eframe 0.36, egui-wgpu
0.36.2, wgpu 30.0.1, winit 0.30.13, wayland-client 0.31,
wayland-protocols 0.32.13 (`wp_color_management_v1`, staging, features
`client` and `staging`), Mesa main (26.x). "Unverified" marks what was
inferred and not run on a machine.

## The problem

An application asks the viewport for a wide target, for example BT.2020
primaries with a 2.2 power (`bt2020` + `gamma22`, an encoding every
compositor with colour management can map to its screen). Today the
screen stays sRGB, for three reasons:

1. `ColorSpace` mirrors wgpu's surface colour spaces, which have no
   BT.2020 with gamma 2.2; `DisplayTarget::is_supported` refuses it and
   `effective()` draws sRGB.
2. Nothing tags a surface: eframe configures every window 8-bit sRGB,
   winit binds no colour-management object, egui-wgpu takes no colour
   space.
3. The output's preferred image description is never read.

Two paths can tell the compositor how the image is encoded.

## (a) A subsurface for the image, tagged through the protocol

The viewport's image goes to a `wl_subsurface` of the window, with its
own wgpu surface; the subsurface is tagged with
`wp_color_management_surface_v1.set_image_description`. The interface
stays on the window's surface, sRGB, untouched.

- **A wgpu surface on a `wl_surface` we own: yes.** eframe hands out
  the window's raw handles (`CreationContext` implements
  `HasWindowHandle` and `HasDisplayHandle`): winit's `wl_display` and
  `wl_surface`. winit links libwayland (`wayland-backend`'s
  `client_system`), so `Backend::from_foreign_display` gives a
  wayland-client connection sharing winit's socket, and
  `ObjectId::from_ptr` a proxy for winit's surface. On a private event
  queue, our events never reach winit. We create a `wl_surface`, make
  it a subsurface of winit's, and give wgpu a handle pointing at it
  (winit's display, our surface): `Instance::create_surface` with
  `RenderState::instance`, the device eframe opened. The Vulkan WSI only
  needs a `wl_surface`, no shell role. Creating a handle from raw
  pointers is `unsafe`, so it lives in a crate of its own, as the Win32
  layer does.
- **Position and size.** The subsurface sits **below** the window's
  surface (`place_below(parent)`), and the viewport punches a hole in the
  interface where the image is: its paint callback writes transparent
  black over the image rectangle. egui's overlays, menus and tooltips are
  drawn after, over the hole, so they stay above the image. Below has a
  second gain: the subsurface may be larger than the hole, so its
  rectangle is snapped outwards to whole logical pixels
  (`set_position` takes integers) and the overflow hides under the
  interface. The buffer is in physical pixels; `wp_viewporter` maps it to
  the logical size (fractional scales), else `set_buffer_scale` for an
  integer scale. Both are double-buffered state applied with the
  subsurface's commit (the WSI's present) and, in synchronised mode,
  with the parent's.
  The hole needs a window with an alpha channel: egui-wgpu configures a
  premultiplied surface only when the viewport is built transparent
  (`ViewportBuilder::with_transparent(true)`), and winit then sets no
  opaque region. On an opaque window the hole would show black, so the
  path refuses it and says why.
- **Input.** The subsurface gets an empty input region: pointer and
  touch go to winit's surface wherever the image is. (The parent's input
  region covers its whole surface, transparent pixels included, so the
  hole does not leak input either.)
- **Frame synchronisation.** Subsurfaces start synchronised: their
  commits are cached until the parent commits. egui-wgpu calls the paint
  callbacks' `prepare` before it acquires the window's texture; the
  viewport renders the image there, submits, flushes its own requests
  (position, scale, image description) and presents the subsurface, then
  egui presents the window: both land in the same compositor frame. The
  subsurface's swapchain uses `Mailbox` (else `Immediate`), not `Fifo`: a
  synchronised subsurface gets its frame callbacks only once the parent
  commits, and a FIFO present waiting for them could stall the frame
  (unverified, avoided rather than measured).
- **The collision.** The protocol allows **one**
  `wp_color_management_surface_v1` per `wl_surface`; a second is the
  fatal `surface_exists` error. Mesa's Wayland WSI (25.1 and later) makes
  one on every swapchain's surface it decides to colour-manage
  (`wsi_common_wayland.c`, `needs_color_surface`): for any colour space
  but pass-through, and for `SRGB_NONLINEAR` too when the compositor
  announces the `srgb` primaries **and** the `srgb` transfer function to
  a version 1 client (Mesa binds the manager at version 1). It then sets
  no description for sRGB, but the slot is taken. wgpu exposes no
  pass-through colour space. So:
  - on KWin, sway (wlroots) and Weston, which do not announce the `srgb`
    transfer, the image surface configured `Srgb` through wgpu is ours
    to tag;
  - on GNOME (Mutter) and Hyprland, which announce it to version 1
    clients, Mesa takes the slot: tagging ourselves would kill the
    connection. There the image surface uses a colour space the WSI
    declares instead (Display P3 on both, scRGB on Hyprland), through
    wgpu's capabilities of the image surface.
  The rule is read from the compositor's own announcements (we bind at
  version 1 too), whatever the driver: a driver that does not tag
  (NVIDIA's, older Mesa) loses nothing but the self-tagging on GNOME and
  Hyprland. An environment variable turns self-tagging off for a driver
  that tags beyond Mesa's rule.
- **The output's description.** `get_surface_feedback` on winit's
  surface is not exclusive (it lives beside Mesa's colour surface);
  `get_preferred` and `get_information` give the primaries, transfer and
  luminances the compositor prefers for the window, and
  `preferred_changed` says when it moves to another screen.

Cost: a Linux-only module (the connection, the subsurface lifecycle, the
description, the feedback), a second pipeline for the image surface's
format and a hole pipeline, and a transparent window. No upstream change.

## (b) The swapchain's colour space

wgpu 30 configures a surface in a colour space
(`SurfaceConfiguration::color_space`), and Mesa maps it to the protocol.

- **Mesa's mapping** (`colorspace_mapping[]`): `SRGB_NONLINEAR`
  (srgb + **srgb** TF), `DISPLAY_P3_NONLINEAR` (display_p3 + srgb TF),
  `EXTENDED_SRGB_LINEAR` (srgb + ext_linear, needs
  `extended_target_volume`), `DISPLAY_P3_LINEAR`, `BT709_LINEAR`,
  `BT709_NONLINEAR` (bt1886), `BT2020_LINEAR` (bt2020 + ext_linear),
  `HDR10_ST2084` (pq), `HDR10_HLG`, `ADOBERGB_LINEAR`; a colour space is
  offered only when the compositor announces its primaries and TF. Always
  the perceptual intent, no luminances.
- **No BT.2020 with gamma 2.2**: Vulkan has no such colour space. The
  nearest, `BT2020_LINEAR`, is dropped by wgpu-hal (its conversion keeps
  sRGB, extended sRGB linear and nonlinear, Display P3, HDR10 PQ and
  HLG). And the `srgb` TF Mesa asks for is not announced by KWin, sway or
  Weston, so Display P3 is offered only on GNOME and Hyprland.
- **It tags the whole window, interface included.** egui-wgpu picks the
  format itself (`Rgba8Unorm` or `Bgra8Unorm`) and configures wgpu's
  default colour space; `WgpuConfiguration` has no field for either, so a
  library over eframe cannot choose. Were it chosen, egui's renderer
  writes sRGB-encoded values to every non-`*Srgb` format
  (`fs_main_gamma_framebuffer`), so the interface would be wrong on any
  colour space but sRGB and extended sRGB. Raw access through wgpu-hal
  does not help: the swapchain is egui-wgpu's.

## Recommendation: (a)

1. It needs no upstream change, where (b) needs egui-wgpu to take a
   format and colour space and to encode the interface for them, a change
   a library cannot ship on eframe.
2. It carries the encoding asked for (BT.2020, gamma 2.2) on KWin, sway
   and Weston, which no Vulkan colour space can, and falls back to what
   the WSI offers on the image surface elsewhere (Display P3 on GNOME and
   Hyprland).
3. Only the image is tagged; the interface stays the sRGB egui draws
   faithfully.

What it costs: a transparent window; one image surface per viewport, on
the main window only (its handles come from eframe's creation context);
a guess at the driver's rule for the colour surface, conservative and
switchable off. The swapchain path stays the long-term one for HDR of the
whole window, once egui-wgpu takes a colour space (see
[upstream.md](upstream.md)).

## The negotiation

`egui_pop_viewport::image_surface::negotiate` decides, from plain data,
what the image surface gets, so it is tested headless with fake
capabilities:

| Situation | Image drawn in | Reason given |
|---|---|---|
| not Wayland (X11, Windows, macOS) | sRGB, on the window | not a Wayland window |
| no `wp_color_manager_v1`, or no subcompositor | sRGB, on the window | the compositor has no colour management (or no subsurfaces) |
| an opaque window | sRGB, on the window | the image surface needs a transparent window |
| sRGB wanted | sRGB, on the window | none |
| `parametric`, the wanted primaries and TF announced, and no `srgb` + `srgb` | wanted, tagged by us (`TargetSource::Tagged`) | none |
| otherwise, a WSI colour space of the image surface carries more than sRGB | it (`TargetSource::Wsi`) | what it lacks of the wanted, if anything |
| otherwise | sRGB, on the window | why each step failed |

The protocol names a target by named primaries (`srgb`, `display_p3`,
`bt2020`) and a named TF (`srgb`, `gamma22`, `ext_linear`, `st2084_pq`,
`hlg`); custom primaries are not tagged.

## Checking it on a screen

No wide-gamut Wayland session is available to automation: the
negotiation is tested headless, the rest is checked by hand on a
colour-managed compositor, as
[viewport.md](viewport.md#wide-gamut-on-wayland) describes (the gallery
with `--wide-gamut`, `WAYLAND_DEBUG`, `helios doctor display`).

## Sources

- Protocol: wayland-protocols `staging/color-management/color-management-v1.xml`
  (<https://gitlab.freedesktop.org/wayland/wayland-protocols>), as bound
  by the `wayland-protocols` crate 0.32.
- Mesa: `src/vulkan/wsi/wsi_common_wayland.c` on main
  (<https://gitlab.freedesktop.org/mesa/mesa>): `colorspace_mapping`,
  `wsi_wl_display_determine_colorspaces`, `needs_color_surface`,
  `wsi_wl_swapchain_update_colorspace`, the manager bound at version 1.
- wgpu-hal 30.0.1 `vulkan/conv.rs` (the colour spaces it keeps);
  egui-wgpu 0.36.2 `lib.rs`, `winit.rs` (format, alpha mode, the order of
  `prepare` and the window's present), `renderer.rs` (output encoding).
- Compositors' announcements: KWin `src/wayland/colormanagement_v1.cpp`,
  Mutter `src/wayland/meta-wayland-color-management.c`, wlroots
  `types/wlr_color_management_v1.c`, Hyprland
  `src/protocols/ColorManagement.cpp`, Weston
  `libweston/color-lcms/color-lcms.c`.
- darktable's Wayland preview (its pull request 22491, read only): a
  subsurface below a toolkit surface with a hole, presented from shared
  memory, tagged BT.2020 + gamma 2.2.
