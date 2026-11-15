---
egui-pop-api: patch
---

The native runner paces each window's paints to the fastest screen's refresh: a window that animates draws at most about one frame per refresh period even where the present does not wait for the screen (an X server without DRI3, Mesa's software rendering), where it used to draw hundreds of frames a second and spend several cores on them. A present paced by the screen (vsync, Wayland's frame callbacks) is not held back.
