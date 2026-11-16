---
egui-pop-api: patch
---

An agent's dry run of an undo, a redo or a jump over the value store's history (`history.undo`, `history.redo`, `history.jump` with `dry_run`) now answers as the request itself would: refused when a step it would walk changed a value the agent may not write, instead of saying it would go through.
