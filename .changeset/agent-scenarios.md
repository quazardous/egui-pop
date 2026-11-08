---
egui-pop-test: minor
---

Scenarios: tests written as RON data and replayed headless
(`egui_pop_test::scenario`, `docs/scenarios.md`). A scenario calls the
capabilities `bridge.describe` lists, waits on the journal or for the
application to settle, clicks, types, presses keys and scrolls through
egui_kittest, checks the tree, a collector's value or the journal, and
keeps shots; `test.*` fixtures set up its world. After every step the
runner checks invariants (`NoErrorToast`, `NoFailedCommand`,
`NoUnknownCommand`, `NoPanic`, `StableWindows`, `FrameBudget`,
`NoMissingText`, and the application's own), which a scenario lifts by
name. A failure names the step, what broke, the journal's last events
and the accessibility tree. `scenario::run_dir(build, "tests/scenarios")`
runs a folder.
