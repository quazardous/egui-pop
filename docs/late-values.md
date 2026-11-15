# Late values and latest-wins loads

Two parts of the family "Values and binding"
([ADR 0027](adr/0027-component-families.md)) for values that arrive
after asynchronous work: how a cell shows one while it comes, and how a
panel following a selection loads only what is still wanted.

| Module | What it does |
|---|---|
| `egui_pop_core::late` | A cell for a value that arrives late, in four states, keeping its size |
| `egui_pop_core::latest` | A slow load following a key: the latest asked wins, one load at a time |

**Owns**: the four states of a late value and their drawing, the
latest-wins loading of one value per key.

**Refuses**: running the work (the application does, on its thread or
through the [read kernel](runner.md#reads)); reactive signals or a
general state library.

## Late values

A `Late<T>` says where the value stands; a `LateValue` draws it:

- **pending**: a skeleton line, or a delayed spinner (`Waiting::Spinner`),
  in a box sized like the expected value (`LateValue::sample`,
  `LateValue::size`), so nothing moves when the value lands;
- **fresh**: the value, as text in a `TextRole` or drawn by a closure
  (`LateValue::show_with`);
- **refreshing**: the last value, a small spinner beside it;
- **failed**: an error icon, the reason and a Retry button.

```rust
use egui_pop_core::late::{Late, LateValue};

fn ui(ui: &mut egui::Ui, free: &Late<String>) {
    let cell = LateValue::new("Free space", free.clone())
        .sample("000.0 GB")
        .show(ui);
    if cell.retry {
        // Run the work again.
    }
}
```

The cell keeps the spinner's room in every state, so its size is the
same pending, fresh and refreshing. Retry is the application's to act
on (`LateResponse::retry`); the cell never runs the work itself.
`LateValue::since` gives when the work started, so a delayed spinner
waits from then rather than from the first draw.

## Latest-wins loads

A panel previewing what is selected elsewhere asks for the current key
each time it draws (`Latest::want`). A key that changed starts a load on
a worker thread; a load asked before it and not started yet is dropped,
one running is told through its `Cancel` and its result is dropped. One
load runs at a time, so going down a list with an arrow key does not
start one thread per row.

```rust
use egui_pop_core::latest::{Latest, Loaded};

fn preview(ui: &mut egui::Ui, latest: &mut Latest<String, String>, selected: Option<&String>) {
    let shown = latest.want(ui.ctx(), selected, |key, cancel| {
        // On a worker thread; a long load looks at `cancel` now and then.
        if cancel.cancelled() {
            return String::new();
        }
        format!("the preview of {key}")
    });
    match shown {
        Loaded::Nothing => ui.weak("Nothing selected."),
        Loaded::Loading { .. } => ui.weak("Loading…"),
        Loaded::Ready(text) => ui.label(text.as_str()),
        Loaded::Failed(reason) => ui.label(reason),
    };
}
```

Nothing runs behind a hidden panel: only `want` starts a load. A load
running when the panel closes goes on to its end, and the panel keeps
its result if the key is still the one selected when it comes back.
[Linked panels](linked-panels.md#slow-previews) shows it in a recipe.

## States and failure

- A late value's states mirror the read kernel's (`Pending`, `Fresh`,
  `Refreshing`, `Failed`), with one difference: `Late::Failed` holds the
  reason only, where a failed read also keeps the value before it.
  `Late::map` and `Late::as_ref` keep the state while changing the value.
- A latest-wins load that panics is `Loaded::Failed` with the reason;
  the next key asked starts afresh.
- On the web there are no threads: the load runs in the frame that asks
  for it.

## Accessibility

A late value's accessible node is a status named after the value ("Free
space"); its value says the state (the value, "Loading", the error), and
it is `busy` while the work runs. Retry is named after the value. A
latest-wins load draws nothing itself: the panel says what it shows.

## Tier

From [API stability](stability.md): **Experimental** ("Values worked out
late (`egui_pop_core::late`, new, not yet released) and slow loads where
the latest asked wins (`egui_pop_core::latest`): no application has
adopted them yet").

## Tests

- Late values: `crates/egui-pop-core/tests/late.rs` (the room kept from
  pending to fresh to refreshing, the state said to assistive
  technologies, the delayed spinner, Retry named and reported once).
- Latest-wins loads: unit tests in `crates/egui-pop-core/src/latest.rs`;
  the gallery's linked panels page uses it. No frame test of its own
  yet.
