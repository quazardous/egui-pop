---
egui-pop-api: patch
---

A listener registered for a window (`AppHandle::listen_local_in`) whose window is closed or drawn in the main one no longer asks for a second main pass when the main window's pass runs it from the queue another thread filled: that pass draws what it changed. Run between passes, it still wakes the main window.
