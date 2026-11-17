---
egui-pop-model: major
egui-pop-api: minor
egui-pop-i18n: patch
---

A plugin's jobs and the keys bound to its commands have an outcome when it goes (ADR 0028, whose nine gaps are now closed). `job.started` carries an `owner` (the plugin the job runs for) and a `command` (the one that started it), kept as `Job::owner` and `Job::command`. When the plugin is disconnected, the framework asks the host to cancel its jobs that can be (`job.cancel` with `{ "id", "reason" }`, from `script:egui_pop`; the job keeps the reason, `Job::reason`), and publishes `job.orphaned` for the others, which go on marked (`Job::orphaned`): the jobs centre says "its plugin x is off" and lets them be dismissed, and `job.adopted` takes the mark back when the plugin reconnects (`Jobs::left_by`, `Jobs::adopt`); `plugin.disconnected` counts them (`jobs_cancelled`, `jobs_orphaned`). A key that the user, the workspace or another plugin bound to one of its commands is dormant: it stays in its layer and is saved, but no longer takes its keys, which go to the next binding of them instead of running a command that is refused (`Keymap::set_dormant`, `Keymap::dormant`, `keymap::DORMANT` in the key trace). The shortcuts editor lists such keys under "Dormant keys" with the plugin's name (`AppHandle::dormant_keys`, `DormantKey`), and they work again when the plugin reconnects.

**Breaking:** `Job` has new public fields (`owner`, `command`, `orphaned`, `reason`): a struct literal gives them (`None` keeps the old meaning); a pattern naming every field adds them or ends with `..`.
