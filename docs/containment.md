# Containment: fault boundaries and the paint guard

The contract of the family "Containment"
([ADR 0027](adr/0027-component-families.md)): a part of the window that
fails stays in its own rectangle, says so, and leaves the rest of the
window working; a window never stays empty without a word. It applies
[ADR 0003](adr/0003-no-silent-no-op.md) (no silent no-op) to drawing.

| Module | What it does |
|---|---|
| `egui_pop_core::fault` | Fault boundaries: a panic while drawing a region becomes an error card in that region |
| `egui_pop_api::paint_guard` | The main window's paint guard: a focus screen without its page, a window that painted nothing |

**Owns**: the boundaries around panels, blocks, stories, custom dialog
bodies and settings pages; the main window's paint guard.

**Refuses**: recovering the application's state after a panic (its
kernel's job); crash reporting (the platform services' logs and crash
reports, [Logs and crash reports](logs.md)).

## Fault boundaries

`egui_pop_core::fault::boundary` draws a closure inside a boundary. A
panic in it is caught (`catch_unwind`), logged on the `egui_pop` target,
and kept: the region shows an error card (the kit's `fault-card`) with
the boundary's name, the panic's message and a Retry button, until the
user retries. The boundary returns `None` while the card is shown.

```rust
use egui_pop_core::fault;

fn inspector(ui: &mut egui::Ui) {
    let drawn = fault::boundary(ui, "inspector", "Inspector", |ui| {
        ui.label("Drawn by code that might panic");
    });
    if drawn.is_none() {
        // The error card is shown in place of the inspector.
    }
}
```

`egui_pop_core::fault::boundary_reporting` hands the caught `Fault` (its
name and message) to a
callback, once per failure: the application uses it to log the panel's id and owner and to
publish `panel.fault` on the journal. `egui_pop_core::fault::faults`
lists the faults shown, for the debug bridge.

The application draws inside a boundary: every declared panel, every
block of a zone, every gallery story, a custom dialog's body and a
custom settings page. Code drawn outside these (an application's own
`egui::Area`, a `wrap` around the shell) is not contained unless it
calls `boundary` itself.

## The paint guard

At the end of each pass of the main window, the application checks two
holes:

- **A focus screen without its page.** A wizard run as a focus screen
  hides the shell under its page. When the wizard is stored but no page
  of it was drawn in the pass, the shell is drawn again from the next
  pass on, a warning is logged once, and `window.focus_hole` is
  published. `AppHandle::focus_screen_hole` reads it.
- **A window that painted nothing.** When the window added nothing but
  backgrounds and lines (no text, no image, no overlay held) for longer
  than a delay (`egui_pop_api::paint_guard::DEFAULT_DELAY`, two seconds),
  an error says once what holds the screen (the place stack, the
  Settings app and its wizard, the overlays, the layout's areas),
  `window.painted_nothing` is published, and a fallback page offers to
  reset the arrangement and go back to the first place. The page leaves
  as soon as the window paints again. `AppHandle::empty_window` reads it,
  `AppHandle::screen_state` gives what the log says.

`AppHandle::set_empty_window_delay` changes the delay, or turns the
second watch off with `None` (an application that paints a blank canvas
on purpose).

```rust
use std::time::Duration;
use egui_pop_api::AppHandle;

fn setup(app: &AppHandle) {
    app.set_empty_window_delay(Some(Duration::from_secs(5)));
}
```

The cost is a counter and a scan of the pass's shapes that stops at the
first text or image.

[ADR 0029](adr/0029-not-on-the-bus.md) makes the guard's findings
events of the window lifecycle (`window.painted`, `window.invariant`)
read from one window registry, instead of watches of its own.

## Failure behaviour

- Where panics abort (`panic = "abort"`, WebAssembly without unwinding)
  nothing is caught: the boundary changes nothing there.
- A boundary never retries by itself: Retry forgets the fault and draws
  the closure again; a new panic shows a new card and is reported
  again.
- The paint guard says each hole once per occurrence (log, journal
  event), not each pass.

## Accessibility

The error card is text (title, sentence, message) and a named Retry
button, reachable by Tab. The fallback page is an ordinary page with
named buttons.

## Tier

From [API stability](stability.md): fault boundaries are **Supported**
("a panel that panics is replaced by its fallback, the rest of the
window goes on"); the paint guard is **Experimental** ("The main
window's paint guard (`egui_pop_api::paint_guard`): new, not yet
released").

## Tests

- Boundaries: `crates/egui-pop-core/tests/core.rs`; a panel that panics
  is contained, named and logged:
  `crates/egui-pop-api/tests/it/paint_guard.rs`
  (`a_panel_that_panics_is_contained_and_named`); mounted components in
  boundaries: `crates/egui-pop-gallery/tests/it/mounts.rs`.
- The paint guard: `crates/egui-pop-api/tests/it/paint_guard.rs` (a focus
  screen with and without its page, a window that paints nothing, with
  and without decorations, the watch turned off).
