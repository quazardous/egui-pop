---
egui-pop-api: minor
egui-pop-agent: minor
egui-pop: patch
---

Security: a private command's data no longer reaches an agent whose grant withholds private data (`Grant::private` off). Its arguments, result and error stayed in clear in the journal (`command.dispatched`, `command.done`, `command.failed`) and in the `commands` collector's run `history`, readable through `snapshot`, `journal.since` and `wait` whatever the grant. The journal now marks those events `"private": true` in their data and the run history marks those runs the same way, and the agent socket redacts them at read time, per reader: every `snapshot`, `call` and `wait` answer goes through `egui_pop_agent::grants::redacted_for`, which puts `"<private>"` in their place for such an agent (`egui_pop_api::host::redact_private`, `host::REDACTED`). An agent shown private data still reads them whole, so `egui-pop-agent-mcp` still answers a granted agent a queued private command's result. In process, the application's own reads are unchanged.
