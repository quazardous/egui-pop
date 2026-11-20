---
egui-pop-api: minor
egui-pop-i18n: minor
egui-pop: minor
---

Report a problem can attach what the application just did.
`AboutPlugin::journal` records the last minutes of the journal (5 by
default) in a time window of its own, so a rare entry is not pushed out
by frequent ones; kinds that come at every pass are skipped
(`RecorderConfig::skip`). Each entry is redacted as it is kept: a
private command's data reads `<private>`, the home folder is written
`~`, every other absolute path keeps only its root and its extension,
and the environment's secrets are blanked (`about::journal::Redactor`).
`AboutPlugin::context` adds the application's own context (its
library's shape, its layout, its settings; `about::changed_settings`
gives the settings that differ from their defaults). `help.report` then shows
the report in a dialog before anything leaves the machine: its button
copies it and opens the issue, to paste it there. Agents read it with
`help.report.bundle`.
