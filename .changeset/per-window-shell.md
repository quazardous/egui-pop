---
egui-pop-core: minor
egui-pop-api: minor
egui-pop-agent: minor
egui-pop: minor
---

Each application window's shell is its own (ADR 0025). Toasts, dialogs
and the command palette belong to the window they were opened from (the
window being drawn, the one whose command opened them, the one an agent
names, else the active one): each window stacks its own toasts
(`Toasts::notify_in`), queues its own dialogs (one shown at a time in
each, blocking only that window) and opens its own palette, offering and
running that window's commands with the global ones. `modalOpen` is per
window group (`overlay::modal_open`, `overlay::set_separate_windows`,
`AppHandle::window_group`): a modal in an application window no longer
makes `!modalOpen` commands refuse in the main window. The agent socket's
`call` and `snapshot` take `window` (the active window without one), and
`describe` lists the open windows. `window.pick` (Window > Switch to
window…) is a quick pick of the open windows by title. A window moved
into the main window keeps the geometry saved under its own id; the main
window's stays under `main`.
