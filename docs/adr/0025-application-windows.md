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
  Raising the main window (`raise_window(&WindowId::main())`,
  `window.switch { window: "main" }`) while a moved content holds it
  sends that content back to a window of its own (its id, title, saved
  geometry and keys) and draws the shell again (2026-11-12).
  `AppHandle::quit` (used by `app.quit`) closes everything instead of
  moving a window into the main one.
- **Titles come from the content** (`WindowUi::set_title`), sent to the
  system as soon as they change; there is no generic word.
- **Geometry** is read from each window's own pass and kept per window id
  in `windows.json` under the persistence directory; the position only
  where the platform gives it.
- Per window: keys are dispatched in its own pass (`appWindow` names it in
  `when` clauses); a modal open in an application window blocks that
  window only.
- **Each window's shell is its own** (2026-11-11). A *window group* is
  an application window, or the main window with its detached panels
  (`AppHandle::window_group`). What opens belongs to the group it was
  opened from: the window being drawn, the window whose command opened
  it (its `appWindow`), the window an agent's call names, else the
  active one.
  - **Toasts:** each group stacks its own (`Toasts::notify_in` names
    one); a closed window's go to the main window.
  - **Dialogs:** one queue per group, one dialog shown in each at a
    time; a dialog blocks its group only. `modalOpen` is per group
    (`overlay::modal_open`, with `overlay::set_separate_windows`): a
    modal in an application window no longer makes `!modalOpen`
    commands refuse in the main window. A closed (or moved) window's
    dialogs go to the main window's queue.
  - **The palette:** one per group, opened where it is asked, offering
    and running in that window's context (its commands and the global
    ones), Escape closing that one only.
  - **Agents:** `call` and `snapshot` take `window` (a window id, `main`
    for the main window; the active window without one), and
    `describe` lists the open windows (`id`, `title`, `main`,
    `active`).
  - **Quick pick:** `window.pick` (Window > Switch to window…) lists the
    open windows by title in a choice with a filter, in the window it is
    asked in.
  - **The shell's bars** (2026-11-13): a window opened with
    `WindowSpec::shell_bars(true)` shows the main window's bars, as VS
    Code's windows do: the title bar's start (the `CAPTION_START` zone:
    the menu button, the name or logo) and the main menu (`app.main`,
    when the application has a menu bar), evaluated and run in that
    window's context. A command whose `when` holds only in another open
    window (`appWindow == 'main'`) is greyed there, its tooltip naming
    that window ("Only in the Demo window"). A menu opened as a popup
    (`menus::popup_at`) is drawn in the window it was opened in.
- **Geometry when a window moves into the main window:** the main
  window keeps its own geometry under its own id (`main`), and the
  moved content keeps the one saved under its id; the main window's size
  is never written under the content's id.
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
- A `when` context asked for another group than the window being drawn
  (an agent naming a window from the main window's pass) has that
  group's `appWindow`, `window` and `modalOpen`; its focus-chain keys
  stay those of the window being drawn.
- Not yet per window: the workbench layout. An application window's
  content is a closure, not a workbench; when it moves into the main
  window it replaces the shell there (the shell's detached panels are
  not drawn meanwhile). A workbench per window (its own layout, its
  detached panels following it) is left for a later slice.
