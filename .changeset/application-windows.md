---
egui-pop-model: minor
egui-pop-core: minor
egui-pop-api: minor
egui-pop-test: minor
egui-pop: minor
---

Application windows: several OS windows in one process, each almost an
application of its own. `AppHandle::open_window(WindowSpec, content)`
opens a window drawn by its own pass (an egui deferred viewport: an idle
window sleeps while another animates); its content may hold `Rc` state,
reached through a thread-local registry. The content names its window
(`WindowUi::set_title`, sent to the system at once) and may keep it open
on a close request. Closing the main window while others are open moves
the one worked in last into it; closing the last quits. Each window's
size (and position, where the platform tells it) is kept per window id
in `windows.json`. Keys are dispatched in the window's own pass, with
`appWindow` naming it in `when` clauses; a modal open in an application
window blocks only that window (`overlay::block_window_among`).
`close_window`, `raise_window`, `windows`, `window_at`,
`active_app_window` and `window_geometry` complete the API, the
`windows` collector lists them (`app_windows`), an opt-in
`WindowsPlugin` adds a Window menu listing the open windows by title
(`window.switch`, `window.close`, offered to agents), and
`AppBuilder::multi_window` sets wgpu's present mode for several windows
(`AutoNoVsync` on Wayland, where a FIFO present can stall them all).
`egui_pop_test::windows::MultiWindow` drives such an application
headless, each window's pass on its own. `Keymap` is `Clone`.
