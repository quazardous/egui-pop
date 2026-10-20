# The theme editor

A theme's tokens edited live, in any `Ui` of an application: grouped by
role, each colour with its picker, the contrast of every pair a token
takes part in, undo and redo, export and import as a token file. The
gallery's `theme-editor` page shows it over the gallery's own theme.

![The theme editor in pop-dark](../crates/egui-pop-gallery/tests/snapshots/theme-editor-pop-dark.png)

## In an application

```rust
use egui_pop::api::App;
use egui_pop::api::theme_editor::{self, ThemeEditorPlugin};

// The plugin: the editor's state, its steps and agent commands.
let app = App::builder()
    .plugin(ThemeEditorPlugin)
    .build(&egui_pop::egui::Context::default())
    .unwrap();

// The view, wherever the application wants it: a panel, a window, a
// page of its settings.
fn appearance(ui: &mut egui_pop::egui::Ui, app: &egui_pop::api::AppHandle) {
    theme_editor::show(ui, app);
}

// Optional: the host's file chooser, here the files plugin's.
let handle = app.handle();
let editor = handle.service(&theme_editor::THEME_EDITOR).unwrap();
editor.set_file_dialog(Some(std::sync::Arc::new(|app: &egui_pop::api::AppHandle, request| {
    app.service(&egui_pop::api::files::FILES)
        .expect("the files plugin")
        .choose(app, request)
})));
# let _ = appearance;
```

The editor works on the application's theme (the theme service): each
change becomes the spec's base and overrides, installed before the next
frame, so every widget follows at once. When the theme changes elsewhere
(a theme switcher, `AppHandle::theme().set`), the editor starts again
from it, its history emptied. A theme with a light and a dark variant
([Themes](themes.md#light-and-dark-together)) is edited in the variant
shown, and switching between light and dark starts the editor again
from the other one; the first change makes the variant edited the
theme, alone (its overrides are not its counterpart's), so it stays
shown whatever the light or dark preference.

**Live, or in a preview only.** By default each change is installed at
once, as above. With "Preview only" ticked
(`ThemeEditor::set_preview_only`), changes stay in the draft: the
preview pane draws it (a `theme::ScopedTheme`, so `Tokens::of` and
`Kit::of` give the draft's inside it), while the editor and the rest
of the window keep the theme installed, so a draft with unreadable text
or a broken control cannot make the editor itself hard to use. "Apply"
(`ThemeEditor::apply`) installs the draft app-wide; "Discard"
(`ThemeEditor::discard`) goes back to the installed theme; going back
to live editing applies it. The draft's fonts are not previewed: fonts
are global, see the Fonts tab.

The model is `egui_pop_tokens::ThemeDraft`, with no egui in it: a base
theme, the tokens changed over it (raw DTCG values, aliases kept as
aliases) and their history. Applications that want their own view use
it directly, or the service (`THEME_EDITOR`: `set`, `undo`, `redo`,
`start`, `export`, `import`, `read`, `generate`).

## What it shows

- **Tokens** and **Fonts**: two tabs (`ThemeEditor::set_tab`). The
  Fonts tab edits `font.family.ui` and `font.family.mono` in priority
  order (move a family up or down, remove it, add one by name) and the
  user's font files (added through the file chooser, read to take
  their family and weight, kept as paths: `ThemeEditor::font_files`,
  for the host to save, and `set_font_files` at the next start), shows
  a sample line per role in the fonts installed, and applies once with
  "Apply the fonts" (`ThemeEditor::apply_fonts`): the files are
  registered, the families written as one change, the draft applied,
  the atlas rebuilt once. A file that cannot be read is reported and
  its previous face stays. Fonts are global, so they are never
  previewed in the pane (see [Fonts](fonts.md#a-users-font-files-built)).
- **Preview**: a pane drawing the draft in its own theme: text by
  role, the status colours, buttons in their tones and states, a
  checkbox, options, a slider, a field, a selected row and a progress
  bar (`theme_editor::sample`). An application shows its own widgets
  instead with `ThemeEditor::set_sampler` (the gallery passes its
  component sampler). "Preview only", "Apply" and "Discard" sit above
  it.
- **Start from**: any built-in theme; the draft starts again from it.
- **Undo** and **Redo**, naming the token they would change.
- **The tokens, by role**: surfaces, text, accent and focus, states,
  borders, shape, space, type and motion (the system tier), the
  components' tokens, then the palette (the reference tier). A filter
  narrows them; changed tokens are in bold, marked "changed".
- **The token picked**: its description and type, whether it is changed
  here or as in the base theme, what it aliases; its value with the
  widget of its type (egui's colour picker with alpha and, beside it,
  a hex field, "Hex of `<token>`", a number field
  for dimensions, numbers and durations) and a text field taking a
  value or an alias (`{gray.3}`, checked: same type, no loop); reset to
  the base theme's value.
- **Generate this scale from a colour** (a palette token picked, of a
  scale with a step 9: `blue.3`, `gray.7`): a seed picker with its hex
  field, the twelve steps it gives for the variant edited, light or
  dark (`egui_pop_tokens::palette`, [Design tokens](tokens.md#a-scale-from-one-colour)),
  fitted to the theme's surfaces and text, and "Generate", which
  writes them and the scale's `contrast` over the scale; one undo takes
  them all back. A seed too dark, too light or too vivid is clamped,
  and said so. Only the variant shown is written: its light or dark
  counterpart keeps its own scale (generate it again with the other
  mode shown; editing both variants of a pair at once is not done
  yet). An accent for both variants is the theme service's
  `set_accent_color` (the `appearance.accent_color` setting).
- **Contrast**: every pair of `contrast::PAIRS` the token takes part in,
  itself or through the tokens aliasing it, with its ratio, AA (4.5:1
  for text, 3:1 for interface parts), AAA (7:1 for text; interface parts
  have no AAA) and what the theme needs: AAA text for a high-contrast
  theme (7:1), AA otherwise.
- **Export and import**: the draft as a `.tokens.json` file, or a file
  pasted and loaded; "Open a file" and "Save as a file" through a file
  chooser. The chooser is the host's (`ThemeEditor::set_file_dialog`, a
  function starting a choice and answering later, as
  `files::Files::choose` does), or the `files` plugin's when it is
  connected, so the editor brings no file-dialog dependency; without
  either, files are pasted as text only. Tokens the base theme does not have, or whose value
  is not of their type, are left out and listed, with what else the
  file loaded with (a token renamed, a file newer than this egui-pop):
  the file loads over its base as any theme file does
  ([Design tokens](tokens.md#a-files-version)).
- **Problems**: what the theme's resolution reports.

## The file

A draft is saved as an ordinary token file (see [Design tokens](tokens.md)):
`$extends` names the base theme, the file gives only the tokens changed,
each with its `$type`, and the root's `$extensions` carries the format
stamp, `"org.egui-pop.format": { "version": 1, "contract": 1 }`
([Design tokens](tokens.md#a-files-version)). Files the editor wrote
before carry `"org.egui-pop.theme-editor": { "version": 1 }` instead,
still read. A file stamped newer than this egui-pop knows loads
best-effort, with a warning. Any such file
loads as a theme (`Themes::add_text`) and imports back as a draft. The
editor writes DTCG JSON, the exchange format; a YAML file imports too.

```json
{
  "$description": "Edited from pop-light",
  "$extends": "pop-light",
  "$extensions": { "org.egui-pop.format": { "version": 1, "contract": 1 } },
  "accent": { "default": { "$type": "color", "$value": "#2060c0" } },
  "button": { "rest": { "border-color": { "$type": "color", "$value": "{border.strong}" } } }
}
```

## Steps and commands

| Step | Arguments | Default |
|---|---|---|
| `theme.token` | `path`, `from`, `to` (raw DTCG values, `null` for the base theme's), `cause` (`edit`, `reset`, `undo`, `redo`, `agent`) | writes `to` |
| `theme.load` | `base`, `overrides`, `cause` (`start`, `import`) | loads them, the history emptied |

A before-hook vetoes a change (the token keeps `from`) or redirects it
(another `to`, checked again); a run-hook's result is the value written.
Undo and redo go through `theme.token` too, so a plugin can keep a token
from moving whichever way it is asked. A colour dragged in its picker is
shown live and becomes one change when the pointer is released; a
number moved with the keys, or a value typed in, becomes one when its
field lets the focus go.

Commands for agents (hidden from the palette): `theme.read` (the draft:
base, overrides, history, failing pairs and problems; with `path`, one
token, its value, alias and contrast readouts), `theme.set` (`path`,
`value`, `null` for the base theme's), `theme.undo`, `theme.redo`,
`theme.start` (`base`), `theme.export` (`name`, the file's text back),
`theme.import` (`text`; the tokens left out back as `skipped`,
and what the file loaded with as `warnings`) and `theme.generate`
(`scale`, `seed`: a scale written from a seed colour, through
`theme.token` for each of its tokens, undone as one change).

## Keyboard and screen readers

Every control is reached with Tab and used with Enter or Space: the
tabs, the theme picker, undo and redo, "Preview only", "Apply" and
"Discard", the filter, each group and token, the value and hex fields,
reset, the seed picker's hex field and "Generate", export and import,
the Fonts tab's moves, removals, additions and "Apply the fonts". egui's colour picker is drawn
for the pointer; its keyboard path is the value field beside it (a hex
colour or an alias). The colour button is a colour well named "Adjust
`<token>`", the field "Value of `<token>`"; the contrast table is text.

## Survey (2026-10-13)

| Crate | Licence | What it is | Decision |
|---|---|---|---|
| egui's `color_picker` (`color_edit_button_srgba`) | MIT OR Apache-2.0 | An HSV picker with alpha in a popup | **Adopt**: the colour widget |
| egui_colors 0.13 | MIT | 12-step palettes generated from a seed colour (Radix-like), with a clamped picker; "for exploring styling options" | **Own, after it** (2026-10-20): its approach (OkHsl, hue shifts, saturation limits per hue band, APCA) re-derived in `egui_pop_tokens::palette` with documented targets and contrast tests; it depends on egui and its constants are undocumented |
| egui_thematic | MIT | A theme editor over egui's `Visuals`, with presets and JSON save | **Own**: it edits egui's style, not DTCG tokens in tiers with aliases and contrast pairs |
| egui-themes, zeus-theme | MIT | Ready-made egui styles | Not relevant: egui-pop's themes are token files |

Not done yet: renaming a theme and adding it to the themes the
application offers (the editor works as overrides over a known base),
a face picked from the system's fonts in a list (the Fonts tab takes
a family by name), a sample drawn in a face before it is applied, and
exporting a user's font files with a theme.
