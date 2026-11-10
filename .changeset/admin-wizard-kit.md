---
egui-pop-api: minor
egui-pop-model: minor
egui-pop-i18n: patch
---

The Settings app gains the wizard kit: a multi-step flow declared in a
manifest (`Manifest::wizard`, `Wizard::step`), each step built as a page
is, its draft fields (`Field::draft`) with their own default, choices,
range and `required`. Next is greyed with the step's first blocking
problem (the field's own checks, its `validate` hook, the step's), shown
under its field and beside Next; a step may be conditional (`when`) or
optional (Skip); a summary lists every value with "Change"; Finish runs
the wizard's command with the draft. Cancel and Escape ask first once
something was entered. The stepper says "Step 2 of 4, …" to assistive
technology; Enter (nothing focused) and Alt+N go next, Alt+B back. Agents
drive it with `admin.wizard.open`, `set`, `next`, `back`, `skip`, `go`,
`finish` and `cancel`, and `admin.open {"wizard"}`; the `admin`
collector shows the flow. The gallery's settings page has a greeting
wizard.
