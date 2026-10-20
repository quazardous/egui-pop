# Comment on rust-windowing/winit#3788 ("Feature Request: Get Cursor Icon Size")

> **Before submitting (delete this block).** Not a new issue: winit already
> has <https://github.com/rust-windowing/winit/issues/3788> (open), so this
> is a comment for it, adding a use case and per-platform sources. winit's
> CONTRIBUTING has no AI policy; larger API proposals go to an issue first,
> which this is. Checked against `master` at 1893565: winit has no cursor
> size query (no `XCURSOR_SIZE`, `SM_CXCURSOR` or `Xcursor.size` anywhere).
> Please double-check the platform notes below before posting.

Another use case, where the hotspot does not matter: an application with
its own bitmap cursors (tool cursors in an image editor, set through
`CustomCursor`) wants them the same size as the system's, so they do not
look tiny next to the arrow for users who enlarged their cursor. Knowing
our own custom cursor's size does not help; we need the system's to choose
it.

A best-effort `Option` would already cover that, from what each platform
offers:

- Windows: `GetSystemMetrics(SM_CXCURSOR)` for the nominal size, and the
  accessibility size in `HKCU\Control Panel\Cursors\CursorBaseSize`.
- X11: `XcursorGetDefaultSize` (the `Xcursor.size` resource, then
  `XCURSOR_SIZE`).
- Wayland: no protocol for it; `XCURSOR_SIZE` is the common convention.
- macOS, Web: `None`.

Today we read `XCURSOR_SIZE` ourselves and fall back to 24 points, which is
wrong on Windows and on desktops that only keep the size in their own
settings.
