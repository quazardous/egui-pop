# Inputs, field states and Tab order

egui-pop's inputs are one component, the **decorated input**
(`egui_pop_core::input`), around a text, number, path or search field;
one **field state** model (`egui_pop_model::field`) that every input
shows the same way, the sliders included; and a **Tab order** decided
per focus scope (`egui_pop_core::tab`), with F6 between panels.

![the inputs page](../crates/egui-pop-gallery/tests/snapshots/inputs-pop-dark.png)

## The decorated input

```rust
use egui_pop::egui;
use egui_pop::core::icon::Icon;
use egui_pop::core::input::{Attached, Input};
use egui_pop::model::field::FieldState;

fn export_row(ui: &mut egui::Ui, folder: &mut String, size: &mut f64, name: &mut String) {
    let out = Input::path(folder, "Folder")
        .leading_icon(Icon::Folder)
        .button(Attached::text("Browse…", "Browse for a folder"))
        .state(FieldState::new().required())
        .show(ui);
    if out.button == Some(0) { /* open the file chooser */ }

    Input::number(size, "Long edge")      // a number or an expression
        .unit("px")
        .choices(["1024", "2048", "4096"]) // a dropdown chevron
        .show(ui);

    let out = Input::text(name, "Export name")
        .placeholder("File name")
        .clearable(true)
        .button(Attached::icon(Icon::Save, "Export").menu(["Export as…", "Export settings…"]))
        .show(ui);
    if let Some((_, item)) = out.menu { let _ = item; /* a split button's item */ }
}
```

| Decorator | How | What it does |
|---|---|---|
| Placeholder | `.placeholder(text)` | Shown while the field is empty, in `field.placeholder`; assistive technologies read it as the placeholder |
| Leading, trailing slots | `.leading_icon`, `.prefix`, `.suffix`, `.unit`, `.trailing_icon` | Icons and texts inside the field, before or after the value, never edited |
| Clear ✕ | `.clearable(true)` (on for `Input::search`) | Shown while there is a value and the field can be edited; Tab reaches it, Enter or Space presses it, the focus goes back to the field. One change: one undo step when the field is bound to a store, one step in the field's own Ctrl+Z history |
| Reset | from the state | Shown while the [state](#field-states) offers "reset to default"; puts back `.default_value(text)` |
| Attached buttons | `.button(Attached::text(..))`, `Attached::icon(..)` | Joined to the field's end, each a button of its own; its click is `InputOutput::button` |
| Split button | `Attached::..(..).menu(items)` | The action and a chevron opening a menu (`InputOutput::menu`); `input::split_button` draws one on its own |
| Dropdown chevron | `.choices(items)` | Opens a list of values; choosing one sets the field (`InputOutput::choice`) |

The field is egui's `TextEdit`, so it follows the focus rules of every
text input: while it has the focus, Escape and Enter are its own
(Escape leaves the field, Enter commits it) and no key binding fires on
them. A number field (`Input::number`) reads what is typed as a slider's
typed entry does (`+ - * / ^`, parentheses, either decimal separator,
a unit ignored) and commits it when it loses the focus: Enter, Tab, a
click elsewhere. Escape leaves it with nothing: what was typed is
dropped and the value stays.

Every part has an accessible name: the field its label, the ✕ "Clear
*label*", the reset "Reset *label* to default", the chevron "*label*,
choices" (a list box popup, expanded or not), a split button's chevron
"*label*, more" (a menu popup).

## Field states

```rust
use egui_pop::model::field::{Baseline, FieldState};

let saved = "Sunset".to_string();
let title = "Golden hour".to_string();
let state = FieldState::new()
    .compare(&title, Some(&saved), &"Untitled".to_string()) // modified against saved
    .required()
    .validated(None);                                      // or Some(message)
assert!(state.is_editable());

let off = FieldState::new().disabled("Needs an open image");
assert!(!off.is_focusable());
```

| State | Builder | Edge | Marker | Assistive technologies |
|---|---|---|---|---|
| Disabled | `.disabled(reason)` | muted | an "i"; the **reason** is the tooltip | disabled, the reason as the description |
| Invalid | `.invalid(message)`, `.validated(Option)` | `field.invalid`, and an outline | a crossed circle; the message is the tooltip | invalid, the message as the description |
| Pending | `.pending()` | `field.pending`, and an outline | a clock | busy |
| Modified | `.modified(Baseline::Saved / Default)`, `.compare(value, saved, default)` | `field.modified` | a dot; a reset arrow when it differs from its default | "changed since saved" or "from the default" in the description |
| Required | `.required()` | none | an asterisk (`field.required`) | required |
| Read-only | `.read_only()` | none, and the `field.background-read-only` fill | a lock | read-only |

The edge is a strip along the field's leading side in the colour of its
first state, in that order of precedence; every state also has its
marker, so none rests on colour alone. A disabled field cannot take the
focus and is greyed; a **read-only** field takes the focus, its text can
be selected and copied, never changed. The colours are the `field.*`
component tokens, each an alias of a system role (`danger`, `info`,
`accent.default`, `warning`), checked for contrast over the field.

**Sliders** take the same states: `Slider::new(..).state(state)` and
`RangeSlider::new(..).state(state)` draw the edge on the bar and the
markers after the label, and give the state to assistive technologies.
A read-only slider takes the focus and refuses every change (drag,
keys, wheel, typed entry, reset); a disabled one is greyed, skipped by
Tab and says its reason on hover. Without a state, a slider is drawn as
before.

**Agents** read every field drawn in the last pass through the debug
bridge's `fields` collector (and `egui_pop_core::field::describe`): its
label, kind (`text`, `number`, `path`, `search`, `slider`, `range`), its
marks, whether it can be focused and edited, the disabled reason and the
invalid message. The same snapshot lists the Tab scopes and their
stops.

Any widget can show a state: `field::paint_edge`,
`field::paint_markers`, `field::hover` (the tooltips), `field::access`
and `field::record`.

## Tab order

```rust
use egui_pop::egui;
use egui_pop::core::input::Input;
use egui_pop::core::tab::{self, ScopeKind, TabStop};

fn options(ui: &mut egui::Ui, tags: &mut String, quality: &mut String) {
    tab::scope(ui, "export-options", ScopeKind::Panel, |ui| {
        Input::text(tags, "Tags").tab(TabStop::new().group("export")).show(ui);
        Input::text(quality, "Quality").tab(TabStop::new().group("export")).show(ui);
        let r = ui.button("Export");
        tab::stop(ui, &r, TabStop::new()); // any widget joins the order
    });
}
```

- **Reading order** by default inside a scope: rows from top to bottom,
  in a row from left to right, whatever order the widgets were drawn in
  (two columns drawn one after the other still read row by row).
- **Explicit order**: stops with `TabStop::index(n)` come first, by
  index; **groups** (`TabStop::group(name)`) keep their stops together,
  at the place of the group's first stop. The parts of one input (the
  field, its ✕, its buttons) are a group of their own.
- **Disabled fields are skipped**, read-only ones are not.
- **Focus scopes**: in a `ScopeKind::Modal` scope Tab cycles (after the
  last stop comes the first); in a `ScopeKind::Panel` scope, Tab past
  its ends goes on in egui's order. **F6** and Shift+F6 move to the next
  or previous panel scope, to the stop focused there last (its first
  the first time); F6 does nothing while a modal is open.

| Key | What it does |
|---|---|
| Tab, Shift+Tab | The next or previous stop of the scope |
| F6, Shift+F6 | The next or previous panel |
| Enter, Space on a ✕ or a button | Presses it |

The order is computed from the stops drawn in the previous pass
(`egui_pop_model::tab::order`, pure and tested), and egui-pop moves the
focus before anything is drawn. The focus on a widget that is not a
stop is left to egui, whose Tab order is the order of drawing; a
focused widget that takes the keys itself (a terminal) keeps Tab and
F6.

## Not yet

- A **formal form layout** (labels in a column, sections, help text,
  inline errors and an error summary, built from value descriptors) and
  the `Form` dialog on it come next, with the decision on how
  components are mounted in hosts.
- The application shell's own panels are not Tab scopes yet: an
  application wraps a panel's content in `tab::scope` for F6 to reach
  it.
## Why its own

egui's `TextEdit` is the field (with its hint text); nothing in the
egui ecosystem draws decorated inputs from tokens, carries a shared
field state, or orders Tab per scope. See
[the survey](widgets.md#inputs-and-forms-what-was-checked-2026-10-14).
