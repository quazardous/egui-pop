---
egui-pop-model: patch
---

The keymap finds the bindings in force in one pass: the removals (`-command` and keys bound to nothing) are indexed once instead of being searched again for every binding. Rebuilding an application's key tables (each command's keys and sources, the dormant bindings) no longer grows with the square of the bindings for every command, which made large keymaps slow to load and to switch profiles.
