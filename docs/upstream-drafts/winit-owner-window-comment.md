# Comment on rust-windowing/winit#4256 ("Add the capability of transient windows")

> **Before submitting (delete this block).** This is not a new issue: winit
> already has <https://github.com/rust-windowing/winit/issues/4256> (open),
> and a comment there already names Wayland's `xdg_toplevel.set_parent`. So
> this is a comment that adds a use case and offers help. winit's
> CONTRIBUTING has no AI policy, and it asks for larger API proposals to go
> to an issue first, which this is. Check the platform notes below before
> posting.

Another use case: tool palettes in an image editor. A palette (brushes,
colours, a navigator) is a small utility window that should:

- stay above its application's main window, but not above other
  applications;
- be minimised and restored with that window;
- stay out of the taskbar and the window switcher.

Today only Windows can do this, through `with_owner_window`. On the other
platforms, a palette in its own window either floats above everything
(always-on-top) or falls behind the main window. So an application keeps
its palettes drawn inside the main window, and loses the multi-monitor
layout that utility windows give.

A cross-platform owner on `WindowAttributes` (say
`with_owner(&impl HasWindowHandle)`) would cover it. As far as I can tell,
it maps to:

- Windows: the existing owner (`GWLP_HWNDPARENT` at creation);
- macOS: `addChildWindow:ordered:` on the owner's `NSWindow`;
- X11: `WM_TRANSIENT_FOR` (`XSetTransientForHint`), with the utility window
  type;
- Wayland: `xdg_toplevel.set_parent`.

An owner that is not supported, or a platform without the concept (web,
mobile), could be ignored with a warning, as other attributes are.

We would be glad to help with a PR if the API shape suits you.
