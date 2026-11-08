---
egui-pop-api: patch
egui-pop: patch
---

The shell's zones that hold no dock (the title, top and status bars, the
activity bar) refuse a dragged tab: released there, it goes back where it
was, with no drop preview, instead of landing where the last area it
crossed would have put it.
