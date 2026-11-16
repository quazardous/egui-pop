---
egui-pop-viewport: patch
---

The viewport's `before_compare` hook is asked once when the "before" key goes down and once when it comes up (only after a press it let through), instead of on every frame while a vetoed key stays held, and its vetoes and redirects are kept in the state's trace as the other hooks' are.
