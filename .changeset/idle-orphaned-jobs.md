---
egui-pop-model: minor
egui-pop-api: minor
egui-pop-agent: minor
---

A wait for the application to settle no longer hangs on a job left orphaned by its disconnected plugin: nothing owns it and it may never end, so `Jobs::idle` (and `AppHandle::settled`, `WaitFor::Settled`) leave it out. It is said apart instead: `Jobs::orphans` and `JobsService::orphans` list it, `AppHandle::orphaned_jobs` counts it, and the `jobs` collector and the `bridge.wait` answer (in-process and through `egui-pop-agent`, MCP `wait` included) carry `orphaned`, so an agent sees it without waiting on it. With only orphaned jobs going on, the status bar's ring no longer spins, so the application settles. A wait reads the jobs up to the last `job.*` event, not the last pass's. The count is read from the jobs fold of the journal's `job.*` events (`job.orphaned`, `job.adopted`).
