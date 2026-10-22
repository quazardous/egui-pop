# Issue for emilk/egui: "ViewportBuilder: an owner viewport, for utility windows"

> **Before submitting (delete this block).** File this once winit has a
> cross-platform owner (rust-windowing/winit#4256); until then only the
> Windows part can be built. egui's rule is that a human files issues and
> PRs: the maintainer posts this. I searched egui's issues for "viewport
> parent window", "owner window viewport", "transient" and "with_parent",
> and found no existing request.

**Is your feature request related to a problem?**

An application with tool palettes (an image editor's brushes, colours, a
navigator) would like each palette in its own native utility window. The
window should stay above its application's main window but not above
other applications, follow the main window when it is minimised, and stay
out of the taskbar and the window switcher.

`ViewportBuilder` has no way to name an owner, so an immediate or deferred
viewport is always an independent top-level window. With
`with_always_on_top` it floats above every application; without it, it
falls behind the main window as soon as that window is clicked.

**Describe the solution you'd like**

A `ViewportBuilder::with_owner(ViewportId)` (or `with_parent`) that
egui-winit passes to winit when it creates the window:

- on Windows today, through `WindowAttributesExtWindows::with_owner_window`;
- elsewhere, through winit's cross-platform owner once it exists
  (rust-windowing/winit#4256: X11 `WM_TRANSIENT_FOR`, macOS child windows,
  Wayland `xdg_toplevel.set_parent`).

The owner must already exist. If it closes, the owned viewport closes too,
as the platforms do.

**Describe alternatives you've considered**

- Drawing the palettes inside the main window. This is what we do today,
  and it loses multi-monitor layouts.
- Always-on-top while the application has the focus, turned off when it
  loses it. This flickers, and it is not what the platforms' own utility
  windows do.

**Additional context**

The use case comes from egui-pop, an application framework on egui, which
keeps the limit listed as "no owner window for egui viewports". We would be
glad to help with a PR once the winit side lands.
