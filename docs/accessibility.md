# Accessibility

The contract of the family "Accessibility"
([ADR 0027](adr/0027-component-families.md)), and the rule every other
family follows ([CREDO.md](../CREDO.md), point 7):
every interactive widget has a name and a role that assistive
technologies read, through egui's AccessKit tree.

| Module | What it does |
|---|---|
| `egui_pop_core::a11y` | Names and roles for custom widgets; the announcer (AccessKit live regions) |
| `egui_pop_test` (the checks) | "Every interactive node has a name", the accessibility tree as text |

**Owns**: the names of egui-pop's custom widgets, the announcer, the
checks that keep both true.

**Refuses**: a screen reader of its own; platform bridges (AccessKit is
the bridge, through eframe and winit).

## Names for custom widgets

A widget drawn from `ui.allocate_response` has no role or name for
AccessKit. `egui_pop_core::a11y::name` gives it both:

```rust
use egui::accesskit::Role;
use egui_pop_core::a11y;

fn swatch(ui: &mut egui::Ui) -> egui::Response {
    let response = ui.allocate_response(egui::vec2(16.0, 16.0), egui::Sense::click());
    a11y::name(&response, Role::Button, "Red label");
    response
}
```

egui-pop's own widgets name themselves: a compact cell by its name, a
late value by its value's name with its state as value, a thumbnail by
its label, a slider by its label with its value, a modal's grip as a
splitter. A widget whose label is an icon takes the name its caller
gives (a band button, a toolbar command by its command's label).

## The announcer

What changes without the focus moving (a toast, a status line, a file
picker's count) is said through `egui_pop_core::a11y::announce`, polite
(read when the reader is idle) or assertive (at once):

```rust
use egui_pop_core::a11y::{self, Politeness};

let ctx = egui::Context::default();
a11y::announce(&ctx, "Saved", Politeness::Polite);
assert_eq!(a11y::announcements(&ctx)[0].text, "Saved");
```

`egui_pop_core::a11y::live_regions` draws the two live regions (one
polite, one assertive) holding the latest message of each, invisible;
the application's shell draws them each pass, so a plugin only
announces.
The last `KEPT` (32) announcements are kept for tests and agents
(`announcements`).

## States and failure

- A region holds the latest message of its politeness: the same text
  announced twice in a row leaves the region unchanged, so a reader may
  say it once.
- Without the application (a plain egui program), `announce` still
  queues; nothing is read until something draws `live_regions`.
- Busy states are said with AccessKit's `busy` flag: a skeleton
  (`egui_pop_core::placeholder::Skeleton`), a late value while its work
  runs, a command-bound control waiting for the host.

## Checks

The test kit fails a test when an interactive node has no name
(`egui_pop_test::assert_all_named`, `unnamed_interactive`) and prints
the tree for a review (`egui_pop_test::a11y_tree`). A keyboard walk over
the gallery (`crates/egui-pop-gallery/tests/it/keyboard_walk.rs`) reaches
every page by keys alone.

A pass with a real screen reader (Orca on Linux) is not run yet; it is
planned.

## Tier

From [API stability](stability.md): **Supported** ("Accessibility
helpers (`egui_pop_core::a11y`): names for custom widgets and the
announcer"); the test kit's checks are **Supported** with the test kit.

## Tests

- The announcer and live regions: `crates/egui-pop-core/tests/core.rs`.
- Names across components: `crates/egui-pop-views/tests/views.rs`,
  `crates/egui-pop-api/tests/it/files.rs`, `status_items.rs`, `zen.rs`;
  focus rings at several scales: `crates/egui-pop-test/tests/focus_ring.rs`;
  the gallery's pages: `crates/egui-pop-gallery/tests/it/inspect.rs`,
  `keyboard_walk.rs`.
