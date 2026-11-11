# 0025: Application windows are deferred viewports with an `Rc` shim; the main window is always the first

Date: 2026-11-11. Status: accepted (2026-11-11, by the lead
maintainer's decision under GOVERNANCE, without the comment period).

## Context

An application wants several OS windows that behave almost as separate
applications: each with its own content, title, keys and dialogs, none
blocking the others, an idle one costing nothing while another animates.
egui-pop's application (`AppHandle`) is `Rc`, one per process; its
detached panels are immediate viewports, repainted with their parent.

A spike (`crates/egui-pop-gallery/examples/multi_window_spike.rs`, run in
wbox on labwc, headless mutter and Xwayland) measured eframe 0.37:

- deferred viewports repaint on their own, and their `Send + Sync`
  callbacks run on the main thread, so a thread-local registry keyed by
  viewport reaches `Rc` state from them;
- winit 0.30 cannot hide a window on Wayland, so a hidden coordinator
  root is impossible there;
- `ViewportInfo`'s rects stay empty for deferred windows on Wayland, and
  no position is ever known there;
- on headless mutter a FIFO present stalls every window (one UI thread
  waits on one surface); `AutoNoVsync` (Mailbox) does not, and stays
  paced at the display rate by winit's frame callbacks.

## Decision

- `AppHandle::open_window(WindowSpec, content)` opens a **deferred
  viewport**, declared from the main window's pass. The content is a
  closure (`FnMut(&mut Ui, &mut WindowUi)`) that may hold `Rc` state;
  the callback egui keeps holds only the viewport id, and finds the
  window in egui-pop's **thread-local registry**, with a debug assertion
  that it runs on the UI thread.
- **The main window is always the first window**, never hidden. Closing
  it while other windows are open cancels the close and moves the window
  worked in last into it (content, title, id); closing the last window
  quits.
- **Titles come from the content** (`WindowUi::set_title`), sent to the
  system as soon as they change; there is no generic word.
- **Geometry** is read from each window's own pass and kept per window id
  in `windows.json` under the persistence directory; the position only
  where the platform gives it.
- Per window: keys are dispatched in its own pass (`appWindow` names it in
  `when` clauses); a modal open in an application window blocks that
  window only.
- `AppBuilder::multi_window(true)` makes `run_native` use `AutoNoVsync`
  on Wayland (`WAYLAND_DISPLAY` set) and `AutoVsync` elsewhere, unless the
  application chose a mode.

## Consequences

- Applications keep `Rc` state; nothing moves to `Arc<Mutex>`. Should
  eframe ever run deferred callbacks off the main thread, the debug
  assertion and the empty registry say so at once.
- Commands a window's keys start run before the main window's next pass
  (one pass later, in the window's context).
- Raising a window is best effort on Wayland (no activation token for an
  existing window in winit 0.30).
- `egui_pop_test::windows::MultiWindow` drives such an application
  headless, each window's pass on its own.
