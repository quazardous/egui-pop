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
from it, its history emptied.

**A light and dark pair is edited as a pair.** A theme with a light
and a dark variant ([Themes](themes.md#light-and-dark-together)) keeps
both in the draft, each with its own changes, history and generated
scales, and both stay installed: the variant edited's changes are its
own (`ThemeSpec::counterpart_overrides` for the counterpart's). The
variant edited is the one shown. The editor's "Variant edited" switch
(Dark, Light; `ThemeEditor::switch_variant`) edits the other one and
shows it, the preference becoming its mode as the application's
light or dark toggle does; that toggle, or the system's mode while
the preference follows it, switches the variant edited too. Undo and
redo act on the variant edited. A theme alone (`pop-high-contrast`)
has no switch.

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
  for the host to save, and `set_font_files` at the next start). It
  lists the **families installed** (`ThemeEditor::installed_families`:
  egui's, the application's, the user's font files and the system's
  with `system-fonts`), with a search and the role they are for
  (monospace ones only for monospace); resting the pointer on one
  previews it, picking one puts it first in the role's families
  (`ThemeEditor::choose_face`), the others kept as its fallbacks and a
  generic family (`sans-serif`, `monospace`) added at the end when the
  list has none. A family missing on this machine is marked "Not
  installed here". The tab previews the families edited before they
  are applied: its own
  preview pane draws the sampler and a sample line per role in the
  draft's theme and in the fonts edited (`ThemeEditor::font_preview`,
  through the preview families of
  [Fonts](fonts.md#previewing-a-face-before-it-is-applied-built)),
  the rest of the application keeping the fonts installed. Resting the
  pointer on a family previews it first (`ThemeEditor::preview_face`).
  "Apply the fonts" applies once (`ThemeEditor::apply_fonts`): the
  files are registered, the families written as one change, the draft
  applied, the atlas rebuilt once. "Cancel the font edits"
  (`ThemeEditor::cancel_fonts`) drops them. A file that cannot be read
  is reported and its previous face stays. A file added is drawn in
  the preview once applied. A theme names a system face by its family
  name, so it travels as a token file: where the face is missing, the
  next family of its list draws, and opening or importing the theme
  says so (`Warning::FontMissing`, the family drawing instead). To
  carry the face itself, add its file as a font file: the theme is
  then saved with it (below).
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
  them all back. The seed is recorded, and the file saved gives the
  scale as its seed rather than twelve values (see [The file](#the-file)). A seed too dark, too light or too vivid is clamped,
  and said so. Only the variant edited is written: the other keeps its
  own scale (switch to it and generate it there, or generate the other
  variant, below). An accent for both variants is the theme service's
  `set_accent_color` (the `appearance.accent_color` setting).
- **Generate the other variant** (a pair): the other variant computed
  from the one edited (`ThemeEditor::propose_counterpart`,
  `egui_pop_tokens::ThemeDraft::generate_counterpart`), offered, never
  applied silently: the changes it would make are listed (each token,
  its colour now and proposed), the preview pane draws the proposal,
  and "Apply to `<theme>`" writes it (`ThemeEditor::apply_counterpart`,
  each token through `theme.token`, one undo in that variant), or
  "Cancel" drops it. The rule, deterministic: a colour is mirrored
  (`palette::mirror`: the same OkHsl hue and saturation, the lightness
  `1 - l`; black and white swap), an alias (`{blue.9}`) and any other
  value (dimensions, numbers, durations, font families) are carried as
  they are, a scale generated from a seed is generated again from it
  for the other scheme (fitted to the other variant as generated), and
  the other variant's own changes not among these are taken back.
  Tokens its base lacks, or whose value does not check there, are
  listed as left out.
- **Contrast**: every pair of `contrast::PAIRS` the token takes part in,
  itself or through the tokens aliasing it, with its ratio, AA (4.5:1
  for text, 3:1 for interface parts), AAA (7:1 for text; interface parts
  have no AAA) and what the theme needs: AAA text for a high-contrast
  theme (7:1), AA otherwise; and, for information, the pair's APCA
  lightness contrast (Lc, `egui_pop_tokens::apca`): the theme's
  contract stays on WCAG.
- **Export and import**: the draft as a `.tokens.json` file (a pair as
  one file, both variants), or a file pasted and loaded; "Open a file" and "Save as a file" through a file
  chooser. A theme naming the user's font files is saved with them:
  as a folder (`<name>/<name>.tokens.json` and `fonts/`) beside the
  path chosen, or as a `.zip` archive when the path ends in `.zip`
  (`ThemeEditor::save_file`, `ThemeEditor::package`). "Open a file"
  takes a token file (a package's faces found beside it) or an archive
  (`ThemeEditor::open_file`); the faces become the user's font files,
  registered at once, copied first to the host's font folder when it
  gives one (`ThemeEditor::set_font_dir`, else an archive's faces are
  registered for this run only, with a warning). The chooser is the host's (`ThemeEditor::set_file_dialog`, a
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
([Design tokens](tokens.md#a-files-version)). A scale generated from
a colour is saved as its seed, under `"org.egui-pop.seeds"`
(`{ "blue": { "seed": "#8e44ad", "scheme": "light" } }`), and generated
again when the file is loaded
([Design tokens](tokens.md#a-scale-from-one-colour)): the steps still
at the value the seed gives are left out of the file, a step changed
since is written as it is, and a seed whose scale was undone or no
longer matches is not saved. Files the editor wrote
before carry `"org.egui-pop.theme-editor": { "version": 1 }` instead,
still read. A file stamped newer than this egui-pop knows loads
best-effort, with a warning. Any such file
loads as a theme (`Themes::add_text`) and imports back as a draft. The
editor writes DTCG JSON, the exchange format; a YAML file imports too. A theme
saved with the user's font files lists them under `"org.egui-pop.fonts"`
(`[{ "file": "fonts/Inter-Regular.ttf", "family": "Inter", "weight": 400 }]`),
by their path in the package ([Design tokens](tokens.md#a-theme-with-its-fonts)).

A pair is saved as a new pair, both variants under the name given: the
file is the theme named first's (named `<name>`), and carries its
counterpart's, named `<name>-light` or `<name>-dark`, under
`"org.egui-pop.counterpart"`, each naming the other
([Design tokens](tokens.md#a-light-and-dark-pair-in-one-file)). Saved,
exported or packaged, it is one file; imported or opened, it is the
pair again, the first variant edited; added to an application's themes
(`Themes::add_text`, `add_path`), it is two themes, a pair.

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
| `theme.load` | `base`, `overrides`, `counterpart` (`{base, overrides}`, `null` for a theme alone), `cause` (`start`, `import`) | loads them, the history emptied |

A before-hook vetoes a change (the token keeps `from`) or redirects it
(another `to`, checked again); a run-hook's result is the value written.
Undo and redo go through `theme.token` too, so a plugin can keep a token
from moving whichever way it is asked. A colour dragged in its picker is
shown live and becomes one change when the pointer is released; a
number moved with the keys, or a value typed in, becomes one when its
field lets the focus go.

Commands for agents (hidden from the palette): `theme.read` (the draft:
base, `variant` (`dark` or `light`), overrides, history, failing pairs
and problems, and the pair's `counterpart` (its base, overrides,
whether it can undo; `null` for a theme alone); with `path`, one
token, its value, alias and contrast readouts), `theme.set` (`path`,
`value`, `null` for the base theme's), `theme.undo`, `theme.redo`,
`theme.start` (`base`), `theme.export` (`name`, the file's text back),
`theme.import` (`text`; the tokens left out back as `skipped`,
and what the file loaded with as `warnings`) and `theme.generate`
(`scale`, `seed`: a scale written from a seed colour, through
`theme.token` for each of its tokens, undone as one change),
`theme.switch` (the pair's other variant edited, and shown) and
`theme.counterpart` (the other variant generated from the one edited:
its `edits`, `seeds` and `skipped` proposed, or, with `apply: true`,
written as one change, `changed` back).

## Keyboard and screen readers

Every control is reached with Tab and used with Enter or Space: the
tabs, the theme picker, undo and redo, the variant switch (Dark,
Light), "Generate the other variant" with "Apply to `<theme>`" and
"Cancel", "Preview only", "Apply" and
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
a family by name), and a sample drawn in a face before it is applied.
