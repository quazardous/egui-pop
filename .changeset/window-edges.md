---
egui-pop-tokens: minor
egui-pop-core: minor
egui-pop-api: minor
egui-pop: minor
---

Every window egui-pop decorates (its own frame: the main window, a detached panel's window, an application window) has a 1 px line around it, so a window over another can be told apart: the kit's `window.rest.border-width` is now 1 px in `border.default`, dimmer while another window has the focus (`window.inactive.border-color`), kept when maximized, none in full screen. Its corners round (`window.radius`, `radius.window` by default, 0 for square) while the window floats free, and stay square when maximized, in full screen or on a side against a screen's edge (`egui_pop_core::frame::{Placement, Stuck, corner_radius}`); rounded corners need a transparent window, so `AppBuilder::rounded_windows` builds the decorated windows transparent, by default on Wayland only (X11 when asked; Windows and macOS round their own). What egui paints at a rounded corner is rounded at the end of the pass (`frame::paint_window_edge`, `frame::window_edge`, `frame::set_rounding`); `Kit::window_edge` carries the inactive line and the radius. Wayland does not tell egui a tiled window: one there keeps its rounded corners. The gallery's window frame page toggles them (`--rounded-windows`).
