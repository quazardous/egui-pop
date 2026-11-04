---
egui-pop-api: minor
egui-pop-i18n: patch
---

Reporting a problem opens a prefilled issue. With `AppInfo::issues` set, a
`help.report` command (Help > Report a problem, the palette, a Report…
button beside Copy in the inspect dialog, the About dialog's link) opens
`<issues>/new` in the browser through the links service, with a title,
the version info, the platform, the page shown (`AboutPlugin::page`) and
the last inspect pick: one parameter per field of the issue form named by
`AppInfo::issue_form` (`template=bug.yml`), or a Markdown body without
one. Values are percent-encoded and the URL stays under 8000 bytes, the
pick trimmed first. Without an issues URL the command is not declared.
On the web, the platform line names the browser and its system, read
from the user agent (`web (wasm32), Firefox 132 on Linux`). The gallery
reports to egui-pop's repository, which now has a bug form and a
suggestion form. New strings `command-help-report`,
`egui-pop-inspect-report` and `egui-pop-inspect-report-hover`, translated
in French.
