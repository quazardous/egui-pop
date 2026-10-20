# Design tokens

egui-pop's look comes from design tokens: named, typed values in YAML files
(JSON is read too) in the format of the W3C Design Tokens Community Group (DTCG 2025.10). No
widget hard-codes a colour or a size; it reads a token.

The files live in [`crates/egui-pop-tokens/themes`](../crates/egui-pop-tokens/themes).
`egui-pop-tokens` reads and resolves them (it has no egui dependency);
`egui_pop::theme` turns the result into egui's style.

## The format, as read here

```yaml
$description: "A theme"
$extends: "pop-dark"
gray:
  $type: "color"
  "2":
    $value: {colorSpace: "oklch", components: [0.215, 0.006, 75], hex: "#1b1916"}
surface:
  $type: "color"
  panel:
    $value: "{gray.2}"
    $description: "Panels and side bars."
radius:
  widget: {$type: "dimension", $value: {value: 4, unit: "px"}}
```

- A **token** is an object with `$value`; anything else not starting with
  `$` is a **group**. A token's name is its dotted path (`surface.panel`).
- `$type` is given on the token or inherited from an enclosing group, or
  taken from the token an alias points to.
- An **alias** is a whole value of the form `"{path}"`. Aliases chain;
  a cycle or a reference to nothing is reported, never followed forever.
- `$description` and `$extensions` are kept (tools may store their own data
  under `$extensions`, keyed by a reverse domain name).
- At the root, `$extends` names the theme a file builds on: every token
  not given comes from it, and chains of `$extends` are followed.
- Types read: `color` (colour objects in `srgb`, `srgb-linear`, `oklch` or
  `oklab`, other colour spaces through their `hex` fallback, and
  `#rrggbb`/`#rrggbbaa` strings), `dimension` (`px`, or `rem` at 16 px),
  `number`, `duration` (`ms` or `s`) and `fontFamily`. Other types are
  kept as raw JSON.

Not read yet: references inside composite values, JSON Pointer `$ref`,
`$deprecated`, group-level `$extends`.

### A colour at another opacity

DTCG has no way to say "this colour, translucent". egui-pop reads one
modifier, in the spirit of Tokens Studio's `modify`, kept under
`$extensions` so the file stays plain DTCG (another tool reads the
alias and ignores the modifier):

```yaml
pill:
  rest:
    fill:
      $type: "color"
      $value: "{surface.overlay}"
      # the overlay at 85 %: its alpha multiplied by 0.85
      $extensions: {org.egui-pop.alpha: 0.85}
```

`org.egui-pop.alpha` (`egui_pop_tokens::ALPHA_EXTENSION`) is a number
from 0 to 1 the colour's alpha is multiplied by, once its alias is
followed. A token aliasing a modified token gets the modified colour,
and its own modifier multiplies again. A value outside 0 to 1, or a
modifier on a token that is not a colour, is a reported problem, and the
token is treated as broken. The modifier belongs to the value it is
written with: a theme giving the token again, an override and the theme
editor's export replace both, and `to_dtcg` writes the resulting colour
without it.

## Three tiers

1. **Reference**: the palette, named by what a value is. Each base theme
   has a warm neutral scale `gray.0` to `gray.12` and five hue scales,
   `gold`, `amber`, `red`, `green` and `blue`, steps `1` to `12` plus
   `contrast` (the text colour that reads on step 9). Scale steps mean the
   same in every mode:

   | Steps | Use |
   |---|---|
   | gray 0 to 2 | the deepest backgrounds (in light mode, white paper) |
   | 3 to 5 | subtle fills (hue scales: quiet accent backgrounds, selection) |
   | 6 to 7 | borders |
   | 8 | a solid fill being pressed |
   | 9 | the solid fill (the accent) |
   | 10 | the solid fill under the pointer |
   | 11 | low-contrast text (status colours) |
   | 12 | high-contrast text |

   The light theme redefines the scales (light backgrounds, dark text) and
   keeps the same step meanings, so the system tier mostly aliases the same
   steps in both modes.

   A theme may add hue scales of its own with the same steps (`1` to
   `12` and `contrast`): any such scale is offered as an accent
   (`Themes::accents`). The Catppuccin-inspired themes add `purple`
   (Catppuccin's mauve) and use it as their accent.

2. **System**: roles, what a value is for. These are the tokens widgets
   read, listed in `contract()`:
   `surface.{canvas, panel, raised, hover, active, overlay, sunken}`,
   `text.{default, muted, strong, on-accent}`, `border.{default, strong}`,
   `accent.{default, hover, active, subtle}`, `danger`, `warning`,
   `success`, `info`, `focus.ring`, `selection`, `radius.{widget, window}`,
   `shadow.{popup, window, side}.{offset-x, offset-y, blur, spread, color}`
   (egui's popup and window shadows, and a side area lying over the
   centre; their colours alias the `shade.10` and `shade.38` palette
   tokens, black at 10 % and 38 %),
   `space.{xs, s, m, l, xl}` (the spacing scale),
   `space.{inline, group, section, page}` (spacing by intent, below),
   `text-role.{title, section, body, muted, meta}.{size, color, weight}`
   (text by role, below), `size.control.height`,
   `size.icon.{inline, toolbar, panel}`, `size.dialog.width`, `size.magnet`,
   `size.toast.width`, `font.size.{xs, s, m, l, xl}` (the size scale)
   and `font.text-style.{small, body, button, heading, monospace}`
   (egui's text styles, each an alias of a step),
   `font.family.{ui, mono}`, `motion.duration`,
   `motion.spinner.{delay, min, turn}` (300 ms before a busy indicator
   shows, 500 ms at least once shown, a spinner's turn),
   `motion.progress.travel`, `motion.skeleton.shimmer`, `opacity.scrim`.

3. **Component**: one component's tokens, each an alias of a system token
   (or, for a component based on another, of its base's token under the
   same term: [kit.md](kit.md#components-based-on-another)). The kit's
   components are `component.state.term` ([kit.md](kit.md)): `button.*`
   (with tones), `card.*` (an application's card, by tone),
   the sliders' (`slider.*`, `slider-fill.*`...), `field.*` (input
   fields, their states as tones: read-only and the placeholder `muted`,
   modified `accent`, required `warning`, invalid `danger`, pending
   `info`), the bars, rings, meters and spinners (`progress-bar.*`,
   `progress-fill.*`, `progress-ring.*`, `spinner.*`...), `indicator-dot.*`, `indicator-badge.*`, `indicator-flag.*`,
   `indicator-swatch.*`, `indicator-outline.*` and `indicator-button.*`
   (micro-indicators, see [indicators](indicators.md)),
   `skeleton.*` and `skeleton-shimmer.*`, `dialog.*`, `palette-window.*`,
   `toast.*`. Content drawn in code keeps flat names: `editor.*` and `syntax.*` (one colour per kind of code:
   keywords on the accent, strings on `success`, numbers on `warning`,
   comments muted...), `terminal.*` (its text, cursor, selection, links,
   and `terminal.ansi.0` to `terminal.ansi.15`, the 16 colours programs
   ask for by number: 0 is the colour furthest from the text, 7 the
   text's own, 1 to 6 the status roles and the accent, 8 to 15 their
   stronger forms). A theme may re-alias a component token (high contrast
   outlines every button and field: `button.rest.border-color` and
   `field.rest.border-color` are `{border.strong}`). A component's text
   size aliases a step of the size scale (`"{font.size.l}"`), never a
   number of its own.

### Spacing and text by intent

Layout code says why there is space, or what a piece of text is, rather
than how many points: the tokens below are system roles over the
scales, read through `Tokens` (`t.space.group`, `t.text_role.section`)
or the `UiPop` helpers ([Getting started](getting-started.md#laying-out-by-role)).

| Token | Alias of | Use |
|---|---|---|
| `space.inline` | `{space.s}` | between items on one line |
| `space.group` | `{space.m}` | between related groups of items |
| `space.section` | `{space.l}` | before a section of a panel or a page |
| `space.page` | `{space.xl}` | around a page's content, between its main blocks |

They are aliases, so a density overlay, which moves the steps, moves
them too; a theme may re-alias one onto another step.

A text role is three tokens, `text-role.<role>.size` (a step of the
size scale), `.color` (a text colour) and `.weight` (a `number`, 100 to
900; 400 is regular). The role names say what the text is; the colours
stay `text.*`, which the roles alias, so `text.muted` is still the one
muted colour:

| Role | Size | Colour | Weight | Use |
|---|---|---|---|---|
| `title` | `{font.size.xl}` | `{text.strong}` | 600 | a page's or a dialog's title |
| `section` | `{font.size.m}` | `{text.strong}` | 600 | a section's title in a panel |
| `body` | `{font.size.m}` | `{text.default}` | 400 | ordinary text |
| `muted` | `{font.size.m}` | `{text.muted}` | 400 | hints, descriptions, an empty list's message |
| `meta` | `{font.size.xs}` | `{text.muted}` | 400 | a date, a size, a count |

A weight other than 400 is drawn with the theme's interface font at
that weight (`fonts::family`), once a theme has installed the fonts;
egui's bundled font has one weight, which draws them all. The role
colours are in the contrast pairs, over panels and the raised surface.

## Naming rules

- Name by **role**, never by hue, outside the reference tier:
  `text.muted`, not `text.gray`; `danger`, not `red`.
- Path segments are lowercase, words joined by hyphens:
  `background-hover`, `on-accent`. Dots separate groups, so a segment never
  contains `.`, `{` or `}`.
- States are suffixes of the thing they modify (`button.hover.fill`, the kit's grid: `docs/kit.md`),
  sizes are `xs`, `s`, `m`, `l`, `xl`.
- A component token points at a system token, a system token at the
  palette or another role, never the other way round (a test checks it).

## The editor-inspired themes

Twelve built-in themes take their colours from popular, permissively
licensed editor themes (their workbench colours, not syntax): five
families with a light and a dark member, and GitHub's high-contrast pair.
Each extends `pop-light`, `pop-dark` or `pop-high-contrast` and gives
its palette and the roles that differ; where an original colour missed a
contrast pair, only its OKLCH lightness was moved. Ids keep the family
names; the labels shown to people say "inspired" (`theme_label`).
Sources and licences are in [NOTICE](../NOTICE).

| id | label | extends |
|---|---|---|
| `github-light`, `github-dark` | GitHub-inspired Light, Dark | `pop-light`, `pop-dark` |
| `github-light-high-contrast`, `github-dark-high-contrast` | GitHub-inspired Light, Dark High Contrast | `pop-light`, `pop-high-contrast` (7:1 text) |
| `catppuccin-latte`, `catppuccin-mocha` | Catppuccin-inspired Latte, Mocha | `pop-light`, `pop-dark` (+ `purple`) |
| `one-light`, `one-dark-pro` | One Light-, One Dark Pro-inspired | `pop-light`, `pop-dark` |
| `solarized-light`, `solarized-dark` | Solarized-inspired Light, Dark | `pop-light`, `pop-dark` |
| `gruvbox-light`, `gruvbox-dark` | Gruvbox-inspired Light, Dark | `pop-light`, `pop-dark` |

[![the editor-inspired themes](screenshots/themes.png)](themes.md#the-built-in-themes)

Each one at full size: [the built-in themes](themes.md#the-built-in-themes).

## Composing a theme

The three base themes, `pop-dark`, `pop-light` and `pop-high-contrast`,
are shown at full size with the others in
[the built-in themes](themes.md#the-built-in-themes) (captured headless;
see CONTRIBUTING.md to regenerate).

The compact density changes spacing and sizes only:

![egui's widgets in pop-dark, compact](screenshots/builtins-pop-dark-compact.png)

A theme is resolved from a `ThemeSpec`, in this order:

1. the **base theme** (mode and contrast: `pop-dark`, `pop-light`,
   `pop-high-contrast`) with everything it `$extends`;
2. a **density** overlay, `density-comfortable` or `density-compact`, which
   touches `space.*` and `size.*` only (a test checks it);
3. an **accent**: a colour scale whose steps become `accent.default` (9),
   `accent.hover` (10), `accent.active` (8), `accent.subtle` (3),
   `selection` (5) and `text.on-accent` (`contrast`); anything aliasing
   the accent (`focus.ring`, `slider-fill.rest.fill`) follows;
4. **reduced motion**: `motion.duration` becomes 0 (egui's animation time,
   cursor blink and scroll animation follow);
5. **components based on another** (`<component>.based-on`) receive each
   token of their base they do not give, as an alias of it
   ([kit.md](kit.md#components-based-on-another));
6. the user's **overrides**, raw values by path (aliases allowed); an
   override of a base reaches the components based on it.

```rust
use egui_pop::tokens::{Density, ThemeSpec, Themes};
use serde_json::json;

let spec = ThemeSpec::new("pop-light")
    .density(Density::Compact)
    .accent("blue")
    .reduced_motion(true)
    .override_token("radius.widget", json!({ "value": 2, "unit": "px" }));
let tokens = Themes::default().resolve(&spec);
```

Resolving never panics. Problems (an unknown theme, an `$extends` cycle,
an alias cycle, an unknown alias, a wrong type, an unreadable value, a
modifier that cannot apply, a missing contract token, an unknown override or accent, a
component based on an unknown one or components based on each other in a
circle) are listed in
`tokens.problems()`, and every contract token with a problem takes the
built-in `pop-dark` value.

## Contrast

`egui_pop_tokens::contrast` lists, by hand, the token pairs that must stay
legible (`PAIRS`): text over every surface it is drawn on, status colours
used as text, labels on accent fills, component text, and the things that
must be seen (strong borders, the focus ring, slider fill and thumb on the
track). The tests check every built-in theme in every density against WCAG
2.x: 4.5:1 for text, 3:1 for interface parts, 7:1 for text in high
contrast; and every accent in every theme at 4.5:1 and 3:1. Decorative
borders (`border.default`) are not checked. The gallery's overview page
shows the same table live.

## YAML or JSON

A token file is the DTCG tree, written in YAML (`.tokens.yaml`, the
shipped files) or in JSON (`.tokens.json`): `TokenFile::parse` reads JSON
when the text starts with `{`, YAML otherwise, into the same tree. The
shipped files are compiled to JSON when egui-pop is built
(`egui-pop-tokens/build.rs`): reading YAML costs about seven times
JSON's, and the built-in themes load at every start (24 ms in YAML, 3 ms
in JSON, release build). A trap below in a shipped file fails the build,
with its file and line. Files known only at run time, a user's kits and themes
(`KitData::load`, `Themes::add_path`), are compiled into a cache folder
instead, never next to their YAML: see
[the cache of compiled files](kit-packages.md#the-cache-of-compiled-files).
`Themes::add_path` runs the loading rule below on what it reads, so a
theme's warnings are the same on a hit. YAML
is there for comments. JSON stays the exchange format: what egui-pop
writes (the theme editor's export, `to_dtcg`) is DTCG JSON, which other
token tools read.

In YAML, **every string is quoted**, colours and aliases first. Left
bare, YAML reads them as something else, and a parser would take them
silently, so egui-pop refuses them, with the line:

| Written | YAML reads | Write |
|---|---|---|
| `$value: #0a0a0a` | a comment: the value is lost | `$value: "#0a0a0a"` |
| `$value: {surface.panel}` | a map with one key | `$value: "{surface.panel}"` |

(The same rule as every YAML colour format, Asciidoctor's themes or
DESIGN.md's tokens among them.) Numbers stay bare
(`$value: {value: 1, unit: "px"}`), and DTCG's colour objects need no
quotes but their strings':

```yaml
accent:
  default:
    $type: "color"
    # gold, kept in OKLCH so the light theme can derive its shades
    $value: {colorSpace: "oklch", components: [0.832, 0.126, 83], hex: "#f0c060"}
```

A colour is `"#rgb"`, `"#rrggbb"`, `"#rrggbbaa"`, or a DTCG object
(`colorSpace` `srgb`, `srgb-linear`, `oklch` or `oklab`; another space
is read from its `hex` fallback).

## A file's version

A token file may say which format and which contract it was written
for, with a stamp in the root's `$extensions`:

```json
{
  "$extends": "pop-light",
  "$extensions": { "org.egui-pop.format": { "version": 1, "contract": 1 } }
}
```

- `version` is the file format's (`FORMAT_VERSION`): how a file is laid
  out, apart from which tokens it gives.
- `contract` is the contract's (`CONTRACT_VERSION`): which tokens exist,
  under which names. It is a number of its own, not a crate's version:
  it starts at 1 and goes up by one in a release that renames or removes
  a token of the contract. Adding a token does not raise it, since an
  older file still loads.
- A file without a stamp is read as current, so a file written by hand
  needs none. The theme editor's export writes it
  (`TokenFile::stamp`, `FormatStamp::CURRENT`).
- One stamp per file: each file a theme is made of says what it was
  written for, whoever wrote it, and a file keeps its meaning when it
  travels alone.

A file stamped with a newer format or contract than this egui-pop knows
loads all the same, best-effort, with a warning saying so.

**One loading rule.** A theme file loads over its base theme (its
`$extends`, else `pop-dark`) the same way wherever it comes from:
`Themes::add_text` (an application's own themes, a user's), the theme
editor's import (`ThemeDraft::import`), and `Themes::check_file` for a
file already parsed. Only a file that cannot be read at all (not JSON or
YAML, not a tree of tokens) is an error. Past that, nothing fails: each
of these is a `Warning`, with the token's path and why.

- A token whose value does not load (an unreadable value, an alias to
  nothing or in a cycle, another type than the base's or the
  contract's) is left out: the base theme's value stays.
- A token the base does not have is kept as the file's own (a palette
  scale, an extension's component); under a group of the contract
  (`surface`, `button`) with a term no token of the base uses, it is
  reported as a likely typo. The theme editor, which changes its base's
  tokens only, leaves it out.
- A token renamed since the file's contract is read under its new name.
- A file stamped newer than this egui-pop is read best-effort.
- A base not known yet (themes added in another order) leaves the
  tokens unchecked; resolving the theme reports what is wrong.

`Themes::add` takes a parsed file as it is, past this rule.

**Renames.** A release renaming a token raises `CONTRACT_VERSION` and
adds an entry to `egui_pop_tokens::RENAMED`: the former name, the new
one, and the contract that dropped the former name. A file stamped with
an older contract has each former name read under its new one (aliases
naming it too), with a warning: "renamed, update your file"
(`TokenFile::upgrade`). The table is empty while the contract is at its
first version.

## Adding a theme

1. Write `my-theme.tokens.yaml`. The shortest way is to extend a built-in
   theme and change what differs:

   ```yaml
   $extends: "pop-dark"
   $description: "Cooler greys"
   gray:
     $type: "color"
     "2": {$value: "#171a1f"}   # a key that is a number is quoted too
   surface:
     $type: "color"
     canvas: {$value: "#000000"}
   ```

   A theme may also stand alone (no `$extends`): it must then give every
   contract token, or the missing ones fall back to `pop-dark`.

2. Register it and resolve it:

   ```rust
   let mut themes = egui_pop::tokens::Themes::default();
   // What was read otherwise than written: a token left out, renamed...
   for warning in themes.add_text("my-theme", include_str!("my-theme.tokens.yaml"))? {
       eprintln!("my-theme: {warning}");
   }
   let tokens = egui_pop::theme::install_with(ctx, &themes, &ThemeSpec::new("my-theme"));
   assert!(tokens.resolved().problems().is_empty());
   ```

3. Check its contrast: `contrast::check(tokens.resolved(), Level::Normal)`
   returns the failing pairs.

For a new **built-in** theme: add the file under `themes/`, list it in
`BUILT_IN_FILES` and `BUILT_IN_THEMES`, and the contrast and contract tests
cover it at once. The default kit's component tokens are not in
`pop-dark.tokens.yaml`: they live in `themes/kit/<component>.tokens.yaml`
and are layered into `pop-dark` at load (`docs/kit.md`). Add its
snapshots to the gallery tests
(`UPDATE_SNAPSHOTS=1 cargo test -p egui-pop-gallery`).

## Into egui

`egui_pop::theme::install` sets egui's styles from the system and
component tiers (see the module documentation for the mapping) and stores
a typed `Tokens` set in the context, read with `Tokens::of(ctx)`. A theme
alone fills both of egui's styles, dark and light; a theme with a
counterpart (`org.egui-pop.variant-of`, [Themes](themes.md#light-and-dark-together))
fills each with its own variant, and `Tokens::of` gives the one of egui's
theme of the moment. Install between frames; never change the global
style mid-frame to theme one widget.
