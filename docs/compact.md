# Compact controls

Part of the family "Controls" ([ADR 0027](adr/0027-component-families.md)):
controls for dense rows, where a control has a slot of a given size and
no label beside it (a HUD's parts, a property sheet, a card's fold).

| Item | What it is |
|---|---|
| `egui_pop_core::compact::ChoiceCell` | One of a few words in a slot: egui-pop's `combo_box` sized to it |
| `egui_pop_core::compact::MenuRow` | A row the width of its room in the look of a menu's items, with a muted note at its end |
| `egui_pop_core::compact::RowButton` | A button small enough for a list's row: the kit's `button` around its words, no taller than a row |
| `egui_pop_core::slider::Slider::scrub` | The slider as a scrub field: label and value, no fill, no thumb |
| `egui_pop_core::input::Input::cell` | The decorated input sized exactly to a slot |

**Owns**: these five looks, and the rule that a compact control keeps
every behaviour of its full-size version (keys, typed entry, history,
name).

**Refuses**: a second set of widgets: a compact control is a size of an
existing one, never a new behaviour; layout of the row itself (the
caller's, or [list items and property rows](list-items.md)).

## The API

```rust
use egui_pop_core::compact::{ChoiceCell, MenuRow, RowButton};
use egui_pop_core::slider::Slider;

fn row(ui: &mut egui::Ui, mode: &mut usize, exposure: &mut f64) {
    let size = egui::vec2(120.0, 22.0);
    ChoiceCell::new("Blend mode", mode, &["Normal", "Multiply", "Screen"])
        .size(size)
        .show(ui);
    ui.add(Slider::new(exposure, "Exposure", -3.0..=3.0).scrub());
    if ui.add(MenuRow::new("Show Contrast").note("hidden")).clicked() {
        // reveal it
    }
    if ui.add(RowButton::new("Cancel")).clicked() {
        // cancel the row's job
    }
}
```

- `ChoiceCell::new(name, &mut selected, choices)`: `size` (default: the
  available width and a row's height), `id_salt` for cells of the same
  name in one place; `show` marks the response changed when the choice
  changed.
- `MenuRow::new(text)`: `note` (a count, a key), `height` (default: the
  kit's `menu-item.height`, at least a row).
- `RowButton::new(text)`: `tone` (the kit's `button.tone.*`), `height`
  (default: its words with `space.xs` above and below, no taller than
  `size.row.height`); its words are centred with `space.s` at each side.
- `Slider::scrub`: a drag along the field scrubs over the soft range
  whatever the field's width; too narrow for its label, it shows the
  value alone. Horizontal only: a vertical slider keeps its track.
- `Input::cell(size)`: exactly `size` with its attached buttons, the
  small padding step, none of the room an input keeps for its text.

## States and keys

- `ChoiceCell`: while focused with its list closed, Up and Down choose
  the previous and next word, Home and End the first and last (as a
  native drop-down); Space or Enter opens the list. Its look in each
  state is the combo box's, from the kit.
- `MenuRow`: focused by Tab, pressed with Space or Enter; hover and
  focus from the kit's `menu-item`.
- `RowButton`: a button named by its words, focused by Tab, pressed with
  Space or Enter; its look in each state is the kit's `button`'s.
- The scrub field and the cell keep their full-size versions' states,
  keys, menus and history.

## Accessibility

`ChoiceCell` is named by its name, which also shows in its tooltip (no
label beside it); its value is the word chosen. `MenuRow` is a button
named by its text, its note read after it. The scrub field and the cell
keep their full-size names.

## Tier

From [API stability](stability.md): **Experimental** ("Compact controls
for dense rows (`egui_pop_core::compact`, `Slider::scrub`): new, not yet
released").

## Tests

- `crates/egui-pop-gallery/tests/it/gallery.rs`
  (`compact_controls_scrub_choose_mark_and_reveal_from_their_slots`)
  drives the gallery's compact page; its snapshots are
  `compact-pop-dark.png` and `compact-pop-light.png`.
- `crates/egui-pop-core/tests/compact.rs`: the choice cell keeps its
  slot whatever the word, its name and value, the arrows, Home and End
  while closed, Space opening the list, no words; the menu row's name
  and note, its width and height, Tab, Space and Enter, disabled.
