---
egui-pop-test: minor
---

Scenarios run live, against a running application over its agent
socket: `egui_pop_test::scenario::live::LiveRunner` and the
`egui-pop-scenario` command line (Unix). Calls, waits, value and journal
expectations and shots work as in-process; `$name` variables fill the
arguments; a shot keeps every collector's snapshot as JSON and, given
the application's screenshot capability, a picture. The invariants
`NoErrorToast`, `NoFailedCommand`, `NoUnknownCommand`, `NoMissingText`
and `NoPanic` (the application is gone) are checked after every step.
`Ui` steps stay in-process for now.
