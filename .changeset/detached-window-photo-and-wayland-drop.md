---
egui-pop-api: patch
egui-pop-model: minor
egui-pop-test: minor
---

A panel detached into a window of its own stays there when the place shown changes only by its payload (another image in the same view): the context stack keeps a place's layout under `State::layout_key`, which leaves a payload out unless its frame chooses its arrangement from it (`FrameSpec::arrangement_for`). Before, each payload was a place of its own, starting from the place's arrangement, and the panel docked back as a tab.

A tab dragged from the main window and let go over a detached panel's window joins that window's tabs where the system hides window positions (Wayland): let go outside the main window, it waits a few passes for the window that then sees the pointer. The test kit's `MultiWindow::hide_positions` gives the windows no position, as Wayland does.
