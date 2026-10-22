# Fonts in egui-pop: proportional and monospace, done properly

Status: design note, **not decided** (2026-10-12). Built so far
(2026-10-18): families from the tokens at four weights (a variable face through its
`wght` axis, set per face with `FontTweak::coords`), from
registered faces, the system's (fontdb behind `system-fonts`, not
fontique: no per-script fallback yet) and egui's bundled ones, in that
order (2026-10-22), reported
problems (`egui_pop_core::fonts`); a size scale (2026-10-18), below;
font files a user adds, kept as paths (2026-10-20), below; the text size
setting, aligned numbers, the `fonts` debug collector, the settings
window's Fonts page (2026-10-21), below. The bundled pair is not
chosen yet (the gallery's Font pairs page is where it is chosen); the
rest is still a design.

## The size scale (built)

`font.size.xs`, `s`, `m`, `l` and `xl` are the theme's text sizes, in
`px` (points). egui's five text styles each alias a step, and so may any
component token:

| Text style token | egui | Step | pop-dark | high contrast |
|---|---|---|---|---|
| `font.text-style.small` | `Small` | `xs` | 10 | 12 |
| `font.text-style.monospace` | `Monospace` | `s` | 12.5 | 14 |
| `font.text-style.body` | `Body` | `m` | 13 | 15 |
| `font.text-style.button` | `Button` | `m` | 13 | 15 |
| `font.text-style.heading` | `Heading` | `xl` | 18 | 18 |

`l` (15, 16 in high contrast) has no text style of its own. A theme
moves a step and every style on it follows, or re-aliases a style onto
another step. The former `font.size.small`, `body`, `heading` and `mono`
are no longer read (see the changelog's migration table). In code, `fonts::font_at(tokens, role,
weight, step)` is the `FontId` of a step (`fonts::Step`), next to
`fonts::font_id(role, weight, size)`; `Tokens::font.scale` holds the
sizes. The user's text size (below) multiplies the scale.

## Text size (built)

The text size is a user setting, `appearance.text_size` in Settings ›
Appearance › Fonts, one of seven steps: 85 %, 100 %, 115 %, 130 %, 150 %, 175 %
and 200 % (`egui_pop_tokens::TEXT_SIZES`). Quantised, so the font atlas
holds few sizes.

- `ThemeSpec::text_size(factor)` (the nearest step) multiplies each step
  of `font.size.*` when the theme resolves, rounded to half a point;
  the text styles and every component token aliasing a step follow.
  Spacing does not move: it is not the interface zoom, which scales
  everything (`appearance.interface_zoom`).
- `size.control.height` grows to at least `ceil(body × 1.6)` when the
  text size is not 100 %, so a larger label is not clipped in a button.
- In an application, `egui_pop_api::text` reads and writes the setting
  (`text_size`, `set_text_size`); the theme is installed again at the
  next frame. Changing theme keeps it: it is the user's, not the
  theme's.

The ask: "good font management, fixed-width and variable-width". Concretely,
egui-pop should load real families for interface text (proportional) and
for values, code and debug output (monospace), let a theme and a user pick
them, size them from one scale tied to an accessibility setting, and make
columns of numbers line up.

When this note was written, the tokens `font.family.ui` (`["Ubuntu", "sans-serif"]`) and
`font.family.mono` (`["Hack", "monospace"]`) are parsed and typed
(`egui_pop_core::theme::Font::family_ui` / `family_mono`) but not applied:
the text is drawn by egui's bundled fonts. Only `font.size.{small, body,
heading, mono}` reach egui's `TextStyle`s. The icon font (Phosphor) is
already installed in a family of its own with `ctx.add_font`
(`egui_pop_core::icon::install`); that is the pattern to generalise.

## A user's font files (built)

A user adds a font file (`.ttf`, `.otf`, the first face of a `.ttc`)
rather than the application embedding it:

- `fonts::FontFile::read(path)` checks the file parses (with skrifa,
  the font reader egui itself uses: egui panics on a file it cannot
  parse, so nothing unchecked reaches it) and reads its family (the
  typographic family, else the family name) and weight from its tables.
- A `FontFile` is a path, a family and a weight, serialisable: what
  settings keep, never the bytes. `fonts::register_files(ctx, &files)`
  reads each file again and registers its face (`fonts::register`);
  a file that cannot be read or parsed is reported, and the face it had
  stays. The faces are installed with the next theme install, or at
  once with `fonts::reinstall` when the families did not change (one
  slow frame: the atlas is rebuilt).
- `fonts::installed_families` gives the families installed last.

The theme editor's **Fonts** tab ([The theme editor](theme-editor.md))
edits them: each role's families in priority order (move up, move down,
remove, add a name), the font files (added through the host's file
chooser), a preview of the fonts edited, and one explicit "Apply the
fonts" (or "Cancel the font edits") that registers the files, writes `font.family.ui` and
`font.family.mono` into the theme being edited as one change, and
applies it. Saving the files is the host's: `ThemeEditor::font_files`
gives them, `ThemeEditor::set_font_files` registers them at the next
start. A theme naming the user's font files is saved with them, as a
folder or a `.zip` archive holding the token file and the faces by
relative path ([Design tokens](tokens.md#a-theme-with-its-fonts)), and
opening one makes its faces the user's font files again
(`ThemeEditor::set_font_dir` says where an archive's faces are kept).

## Settings › Appearance › Fonts (built)

The settings window has a Fonts page under Appearance (section 3's
"Text" page, in part), holding egui-pop's text settings
(`egui_pop_api::text`, `settings_window::FONTS`):

- **Text size**, above.
- **Interface font** and **Monospace font** (`appearance.font_ui`,
  `appearance.font_mono`; empty: the theme's): a family picked among
  `fonts::families` (egui's, the application's, the user's font files,
  the system's with `system-fonts`), only monospace ones for numbers and
  code, each drawn in its own face (as a specimen) but the system's.
  The family is put first in the theme's list (`ThemeSpec::font_ui`,
  `font_mono`), the theme's families stay its fallbacks; changing theme
  keeps it. Each has its reset.
- **Preview**: a heading, the theme editor's sample line per role
  (`theme_editor::font_sample`, shared with its Fonts tab), and numbers
  right-aligned in the numeric font. Resting the pointer on a family in
  a picker draws the heading and the sample in it before it is picked
  (the preview families, below).

Not built: the weight of body text, the hinting choice, the per-script
lines and the missing glyphs outlined.

## Aligned numbers (built)

egui applies no OpenType feature, so `tnum` cannot be asked: digits
line up only in a face whose default digits are tabular. As section 3
recommends, values are drawn by `font.numeric` (a theme token):

- `"mono"` (the default): in the monospace family, at the size whose
  x-height matches the text around (`size × font.text-style.monospace /
  font.text-style.body`), lowered to the nearest size whose advance is a
  whole number of physical pixels at the window's scale: with a
  fractional advance, glyphs snap to pixels differently from row to row
  and a column drifts by a pixel.
- `"ui"`: in the interface family. When its regular face's default
  digits are tabular (measured at install, `Report::tabular_digits`),
  as they are; otherwise each digit is laid in a cell as wide as the
  face's widest digit, right-aligned in it (one layout section per
  digit, with its `leading_space`), the workaround of section 4.

`fonts::number_font(ctx, size)` says which, `fonts::number_job` and
`fonts::number_galley` lay a value out. egui-pop's slider draws its
value this way, so it no longer jitters while dragged; an application's
readouts and tables use the same functions. Tests: a column of numbers
in the default has every digit at the same distance from the right
edge; Inter's digits in cells take the same width whatever they are.

## The `fonts` debug collector (built)

Among the core collectors of the debug bridge (section 3 below was its
design):

- snapshot: the families the theme asks for (`requested`), the faces
  registered (family, weight, `origin`: registered or file, bytes,
  tabular digits, monospace), each egui family with the face drawn first
  and its fallback chain, the specimens, whether each role's face has
  tabular digits, `numeric`, the text size, the sizes of the scale and
  of egui's text styles, `pixels_per_point`, the atlas's size and fill
  ratio, and the install's problems;
- action `check { text, family }`: the chars of a text no face of a
  family covers (`ui`, `mono` or a family's name), drawn as a box.

Not built: the `coverage` action over the string catalogue, and the
warning when the atlas is rebuilt often.

## Families by name, and the families there are (built)

- `fonts::specimen(ctx, name, weight)` gives an egui family drawing a
  family by its name, outside the theme's two roles: a picker drawing
  each name in its own face, a page comparing fonts. The first ask
  installs the fonts again (one slow frame, from the next pass) and
  returns `None` meanwhile; a name found nowhere draws egui's bundled
  fonts and is said in the report.
- `fonts::families(ctx)` lists the families a theme or a user may name:
  egui's bundled ones (`Ubuntu`, `Hack`), the faces registered (the
  application's, then the user's files), then the system's (feature
  `system-fonts`), each with its `Origin` and whether it is monospace.
- The install's `Report` also gives each egui family's fallback chain,
  the specimens, and per role whether its regular face has tabular
  digits by default (`Report::tabular_digits`), read from the face's
  advances with skrifa.

## Previewing a face before it is applied (built)

egui's fonts are global, and egui panics on a family it has no fonts
for, so a face is previewed through families every install defines:

- `fonts::set_preview(ctx, role, Some(names))` makes the role's preview
  families (`pop.preview.ui.400`, `.500`, `.600`, `.700`, and the
  `mono` ones) draw the faces found for `names` in priority order, the
  role's families as installed behind them: a name found nowhere draws
  the role's families, never an empty family. `None` makes them draw
  the role's families alone.
  The theme's own families are untouched. A change installs the fonts
  again (one slow frame, from the next pass).
- Every install defines the preview families, whatever it was asked
  for: a theme switched while a preview is shown keeps drawing it, and
  no frame asks for a family egui has not got.
- `fonts::with_preview(ui, add)` draws `add` in them: its text styles
  draw the interface preview (the monospace style the monospace one),
  and inside it the text roles (`UiPop::section_title`, ...) and
  `fonts::shown_font_id` give the preview families too. What draws
  with an explicit `FontId` keeps the theme's fonts.
- `fonts::preview_family(ctx, role, weight)` is `None` until egui's
  fonts in use have it (before the first install's pass), as
  `fonts::specimen` is until egui has the specimen.

The theme editor's Fonts tab and Settings › Appearance › Fonts preview
a face this way ([The theme editor](theme-editor.md#what-it-shows)).

## The order a name is looked up in (built)

Each name of `font.family.ui` and `font.family.mono` (and a specimen's
name) is looked for in this order, the order a platform resolves a font
stack in (as GPUI and browsers do):

1. the faces the application registered, and the user's font files;
2. the system's fonts, by family name (below);
3. egui's bundled faces by name (`Ubuntu` is egui's Ubuntu-Light,
   `Hack` its Hack).

A bundled face is reached by its name only when the system has no
family of that name: a theme naming `Ubuntu` draws the system's Ubuntu
where it is installed, egui's Ubuntu-Light elsewhere, so a stack copied
from a platform (`Helvetica, Segoe UI, Ubuntu, Adwaita Sans, ...`)
draws what the platform draws. Until 2026-10-22 egui's bundled faces
came before the system's, and shadowed the system's Ubuntu.
`fonts::origin_of` and `fonts::families` follow the same order: a
family both on the system and bundled is listed once, as the system's.

The install's report says where each name was found
(`Report::resolved`, `Report::resolved(list, name)`), at the regular
weight, in list order: its list (`ui`, `mono` or `specimen`), its
origin (`registered`, `file`, `system`, `bundled`, or none for a name
found nowhere) and the face drawn (egui's key for a bundled face, the
PostScript name of a system one, else its family and weight). Shown, it
reads `Ubuntu → bundled Ubuntu-Light`, `Adwaita Sans → system
AdwaitaSans-Regular`, `Helvetica → missing`; the `fonts` collector gives
the same under `resolved`, each with its `says` line.

## The system's fonts, or faces given in their place (built)

With `system-fonts`, the system's fonts are fontdb's (MIT, read once per
process). `fonts::set_system_faces(ctx, Some(faces))` gives a context's
system fonts in their place: tests that must not depend on the fonts of
the machine they run on, or an application finding fonts its own way.
`None` goes back to fontdb's. The faces are listed by
`fonts::families` with `Origin::System` and found by the install as the
system's would be.

A theme names a system face by its family name: the next names of its
list are the fallback where the face is missing.
`fonts::origin_of(ctx, name)` says where a name would be found, `None`
when it is missing on this machine (the install's report says so too),
and `fonts::is_generic` names the generic families (`sans-serif`,
`monospace`, ...), always found. A theme package carries a face's file
instead when the file is one of the user's font files.

## Choosing the bundled pair: the gallery's Font pairs page (built)

The gallery's `fonts` page draws candidate pairs side by side, egui's
own as the baseline: a heading, a paragraph, buttons and a checkbox, the
four weights, a slider with its value, a column of numbers right-aligned
in the proportional and in the monospace face, code, and a few scripts;
each pair says its licence, the size of its files and whether its digits
are tabular, measured from the glyphs. The pair bundled is chosen on that
page: **not decided**.

| Pair | Files carried | Licence | Digits |
|---|---|---|---|
| Ubuntu Light + Hack (egui's) | none (in egui) | Ubuntu Font Licence; MIT + Bitstream Vera | Measured on the page |
| Inter + JetBrains Mono NL | 416 + 201 KB (617 KB) | OFL 1.1 | Inter proportional |
| Noto Sans + Noto Sans Mono | 453 + 381 KB (834 KB) | OFL 1.1 | Noto Sans tabular |
| IBM Plex Sans + Plex Mono | 537 + 273 KB (810 KB) | OFL 1.1, RFN "Plex" | Measured on the page |

The files are gallery assets, in `crates/egui-pop-gallery/assets/fonts`
with each licence and source; no published crate includes them. Inter
and Noto are subset to Latin, Greek and Cyrillic with their other axes
pinned (full files: 877 KB, 2049 KB, 1708 KB); JetBrains Mono NL (no
ligatures, no variable build) is subset, regular and bold; IBM Plex is
as published, since a modified build must be renamed (subset and renamed
it would be 347 + 98 KB). The page's test checks every monospace face
has one advance for digits, `i` and `W`, and that Noto Sans's digits are
tabular and Inter's are not.

## 1. What egui 0.36 / epaint can do

All of this was checked in the epaint 0.36.2 source (`src/text/*.rs`) and
the egui release notes.

### Pipeline

| Stage | Crate in 0.36 | Notes |
|---|---|---|
| Font parsing, charmap, metrics, outlines, hinting | `skrifa` 0.44 (Google Fonts / Linebender, MIT or Apache-2.0) | Replaced `ab_glyph`. TrueType instructions or the auto-hinter, with `HintingTarget` (Mono, Smooth with light/normal, symmetric rendering, linear metrics). |
| Shaping | `harfrust` 0.12 (Rust port of HarfBuzz) | Since 0.35: "Use `harfrust` for better kerning and ligatures". Called with `ShapeOptions::new()`: the font's **default features** apply (`kern`, `liga`, `calt`, `mark`, ...). No API to pass features. |
| Rasterisation | `vello_cpu` 0.1 | Outlines only (an `OutlinePen`). CPU, so output depends only on font bytes and options, not on the GPU. |
| Atlas | epaint `TextureAtlas` | An RGBA `ColorImage` (4 bytes per pixel). |

### Families and fallback

- `FontDefinitions { font_data: name -> Arc<FontData>, families:
  FontFamily -> Vec<name> }`. The first name is the primary, the others
  are fallbacks in order.
- `FontFamily::{Proportional, Monospace, Name(Arc<str>)}`. A `Name` family
  is any number of extra families (egui-pop's icons already use one).
- Fallback is **per grapheme**: `segment_into_runs` resolves the face of
  each grapheme's base char through the chain and shapes each run with its
  own face. A char in no face is drawn as `◻` (or `?` if no face has `◻`).
  `Fonts::has_glyph(font_id, c)` tells whether a family covers a char.
- `ctx.set_fonts(defs)` replaces everything, `ctx.add_font(FontInsert)`
  adds one face to families at a priority. Both take effect **at the next
  pass**. Loading at runtime is therefore fine: read a file, `FontData::
  from_owned(bytes)`, insert, done. A change of `FontDefinitions` or of
  `TextOptions` rebuilds the atlas and the galley cache (one slow frame).
- `FontData { font, index, tweak }`: `index` picks a face in a collection
  (`.ttc`/`.otc`, needed for Noto CJK).

### FontTweak (per face)

`scale`, `y_offset_factor`, `y_offset` (visual only, layout unchanged),
`hinting: Option<bool>`, `hinting_target`, `subpixel_binning:
Option<bool>`, `coords: VariationCoords` (default axis values, e.g.
`wght`), `thin_space_width` (for `1 234 567` grouping). `scale` and the
offsets are how a fallback face is matched to the primary's x-height and
baseline; egui does it for its emoji fonts (`scale: 0.81`).

### Weights, italics, variable fonts

- **No `bold`.** `FontId` is `{ size, family }`; a comment in `fonts.rs`
  still says "TODO: weight (bold), italics". `RichText::strong()` only
  changes the **colour**.
- **Variable fonts work.** Since 0.34 ("Add font variations API"), a
  `TextFormat` carries `coords: VariationCoords`, and
  `RichText::variation(b"wght", 600.0)` / `variations(..)` set them per
  run. `FontData::variation_axes()` lists a font's axes with their ranges.
  So with a variable font, real weights (and `wdth`, `opsz`) are
  available per run without extra files.
- With static fonts, a weight is a separate face in its own
  `FontFamily::Name` ("ui-semibold").
- `italics: true` is a **fake slant**: the glyph quad is sheared by a
  quarter of its height. A real italic needs its own face (or a `slnt` /
  `ital` axis).

### OpenType features

The shaper applies only the default features. Consequences:

- **No `tnum`**: tabular figures come only from fonts whose default digits
  are already tabular (many UI fonts are proportional by default: Inter
  is; Noto Sans and Roboto have tabular default digits; check each with a
  test, see section 5).
- **No way to turn off `calt`/`liga`**: JetBrains Mono, Fira Code and
  Cascadia Code ligate `!=`, `->`, `>=` by default. For values and debug
  output pick a font without programming ligatures (JetBrains Mono **NL**,
  IBM Plex Mono, Hack, Noto Sans Mono, Cascadia Mono).
- No `ss01`/`cv**` (slashed or dotted zero, single-storey a), no `zero`.
  The default glyph of the chosen mono font must be the wanted one.

### Scripts

- CJK: needs a font that covers it. Sub-pixel binning is **disabled** for
  CJK glyphs (too many unique glyphs for the atlas); CJK line-break rules
  exist (`is_cjk_break_allowed`).
- RTL: harfrust shapes Arabic and Hebrew runs (joining forms), but layout
  is left-to-right: there is no bidi reordering (`// TODO(emilk): heed
  bidi characters`; bidi control chars are made invisible). Arabic text
  is shaped but drawn in the wrong visual order for mixed or RTL lines.
- Emoji: outlines only. Colour emoji (COLR, bitmap) are not drawn in
  0.36; egui bundles monochrome Noto Emoji and an emoji icon font.

### Hinting, sub-pixel, HiDPI, memory

- `TextOptions { max_texture_side, font_hinting: true, subpixel_binning:
  true, .. }` are global, overridable per face by the tweak. egui's
  settings UI calls hinting "sharper text" and binning "more even
  kerning" (but blurrier).
- There is no LCD (RGB) sub-pixel anti-aliasing: grey-scale only.
- Glyphs are rasterised at physical pixels: `pixels_per_point` (native
  scale times `zoom_factor`) is part of the glyph key. Text is crisp at
  any scale, but every distinct (face, size, ppp, variation location,
  sub-pixel bin up to 4) is a separate bitmap.
- The atlas is `max_texture_side` wide (2048 default, capped at 16384; the
  backends raise it to the GPU limit), starts 32 rows high and grows.
  When it is more than 80 % full, `Fonts::begin_pass` **throws everything
  away and rebuilds** (one slow frame, re-rasterisation). Memory is
  `width x height x 4` bytes: a 4096 x 1024 atlas is 16 MiB of CPU RAM
  plus the GPU copy. What fills it: many sizes, many weights (each `wght`
  value is a new location), CJK (thousands of glyphs, no binning saves
  them a little), HiDPI (4x the pixels at ppp 2).
- Rule for egui-pop: **few sizes, few weights**. A size scale of five or
  six steps and two or three weights keeps the atlas small; a continuous
  "text size" slider is quantised (section 3).

### What is coming upstream (not in 0.36.2)

A stack of PRs merged in September 2026 (into a feature branch, then
completed by #8512), expected in egui 0.37:

- [#8491](https://github.com/emilk/egui/pull/8491) "Render any glyph on
  native": a `FontProvider` trait asked once per (family, char) that no
  configured face covers; `egui_system_fonts::SystemFontProvider`, backed
  by **fontique**, asks the OS fallback (CoreText, DirectWrite,
  fontconfig, Android `fonts.xml`) and memory-maps the file. eframe
  feature `system_fonts`, on by default, `NativeOptions::
  system_font_fallback` to turn it off, locale through
  `SystemFontProvider::with_locale`. Closes
  [#5233](https://github.com/emilk/egui/issues/5233).
- [#8492](https://github.com/emilk/egui/pull/8492): colour emoji
  (epaint `color_fonts` feature).
- [#8493](https://github.com/emilk/egui/pull/8493): bundled monochrome
  emoji fonts become opt-in (`monochrome_emoji_fonts`).

egui-pop's `docs/upstream.md` already expects 0.37 to fix missing
symbols through #8491. The design below takes it as the system fallback
mechanism and does not build its own before 0.37.

## 2. Families: bundle or ask the system

### Bundled candidates (all redistributable with MIT and GPLv3 software)

| Family | Licence | Kind | Notes |
|---|---|---|---|
| [Inter](https://github.com/rsms/inter) | OFL 1.1 | variable `wght`, `opsz` | Designed for UI at small sizes; tall x-height. Default digits proportional (`tnum` unreachable in 0.36). Full file about 0.8 MB, much less subset to Latin, Greek, Cyrillic. |
| [Noto Sans](https://github.com/notofonts/latin-greek-cyrillic) | OFL 1.1 | variable `wght`, `wdth` | Default digits tabular. Same design family as Noto Sans CJK, Arabic, Hebrew, so fallbacks match. |
| [IBM Plex Sans / Mono](https://github.com/IBM/plex) | OFL 1.1, Reserved Font Name "Plex" | static, variable for Sans | A modified (subset) build must be renamed. Mono has no programming ligatures. |
| [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) | OFL 1.1 | variable `wght` | Ligatures on by default: use the **NL** cut for values. |
| [Noto Sans Mono](https://github.com/notofonts/latin-greek-cyrillic) | OFL 1.1 | variable `wght`, `wdth` | Wide coverage, plain, no ligatures. |
| [Hack](https://github.com/source-foundry/Hack) | MIT + Bitstream Vera | static Regular in egui | Already in egui's `default_fonts`. |
| [Ubuntu](https://design.ubuntu.com/font) (Light, in egui) | [Ubuntu Font Licence 1.0](https://ubuntu.com/legal/font-licence) | static | Free to redistribute, also embedded; modified versions must be renamed; Debian treats it as free. Already shipped by egui's `default_fonts`, so no new obligation. Light at 13 px is thin on dark themes. |
| [Fira Sans / Mono / Code](https://github.com/mozilla/Fira) | OFL 1.1, RFN "Fira" | static | Code ligates. |
| [Atkinson Hyperlegible Next / Mono](https://www.brailleinstitute.org/freefont/) | OFL 1.1 | variable | Made for low vision: a natural "legibility" choice in the picker. |
| [Noto Sans CJK](https://github.com/notofonts/noto-cjk) | OFL 1.1 | `.otc` collections | 15 to 20 MB per weight; regional subsets (SC, TC, JP, KR) about 5 to 10 MB each. Never bundled by default. |

OFL fonts may be bundled, embedded and sold with any software; only the
fonts themselves stay under OFL, and a modified font cannot use a
Reserved Font Name. Carry each `OFL.txt` next to the file and list the
fonts in the crate's licence notes (`cargo deny` does not see font
files). GPLv3 Helios linking MIT egui-pop that embeds OFL fonts is fine:
the OFL explicitly permits bundling.

### System fonts

| Crate | Licence | Backends | Use |
|---|---|---|---|
| [fontique](https://crates.io/crates/fontique) (Linebender, part of parley) | Apache-2.0 OR MIT | fontconfig (optionally `dlopen`), DirectWrite, CoreText, Android | Enumeration, CSS-like matching (family, weight, width, style), **fallback by script and locale**. What egui 0.37 uses. Built on `read-fonts`/`skrifa`, the same stack as epaint. |
| [fontdb](https://crates.io/crates/fontdb) | MIT | scans directories, parses fontconfig's XML (no libfontconfig) | Enumeration and CSS-like queries; no per-script fallback. Used by resvg and cosmic-text. Mature. |
| [font-kit](https://crates.io/crates/font-kit) (servo) | MIT OR Apache-2.0 | fontconfig + FreeType, DirectWrite, CoreText | Heavier (FreeType); maintenance slower. Not needed. |
| `fontconfig` crates directly | MIT | Linux only | Too low level. |

fontique is the choice: same stack as epaint, used upstream, does
per-script fallback, and asks the OS for "sans-serif" / "monospace"
(generic families) the way the token value's last entry already does.

### Recommendation

1. **Ship one proportional and one monospace family, in a feature
   (`bundled-fonts`, on by default), budget about 1 MB:**
   - UI: **Inter** (variable, roman, subset to Latin + Latin Extended +
     Greek + Cyrillic). Alternative if tabular digits in proportional
     text matter more than small-size polish: **Noto Sans** (tabular by
     default, same family as the CJK and Arabic fallbacks). A side by side
     page in the gallery decides; mark it "not decided".
   - Mono: **JetBrains Mono NL** or **Noto Sans Mono** (no ligatures,
     slashed or dotted zero by default, variable `wght`).
   - Keep egui's Noto Emoji and Ubuntu-Light only as last fallbacks while
     `default_fonts` stays on (it is enabled by egui-pop crates today and
     features are additive, so turning it off saves about 1.4 MB only if
     every crate in the build agrees).
2. **Never bundle CJK, Arabic, Devanagari** by default. Take them from the
   system: with egui 0.37, `SystemFontProvider` with the UI locale; before
   that, an opt-in `system-fonts` feature that, at install, asks fontique
   for a fallback per script of the active locale and adds the faces
   behind the bundled ones. An application that ships a locale ships or
   downloads its fonts (Helios: an optional CJK font pack).
3. **A theme or a user picks families by name.** `font.family.ui` is a
   list resolved in order: a bundled family name (`"Inter"`), an
   installed system family (`"Cantarell"`), a font file shipped with a
   theme (`"file:fonts/MyFace.ttf"`, relative to the theme file), and the
   generic names `sans-serif`, `monospace`, `system-ui` (asked from the
   OS). The first one found is the primary, the next ones are fallbacks,
   the bundled family is always appended last so text never vanishes.
   Every name not found is a reported problem (the theme's
   `problems()`), not a silent fallback.
4. Tests and snapshots never look at the system (section 5).

## 3. Tokens and settings

### Tokens

Families (type `fontFamily`, a list):

| Token | Default | Use |
|---|---|---|
| `font.family.ui` | `["Inter", "system-ui", "sans-serif"]` | Interface text, `FontFamily::Proportional`. |
| `font.family.mono` | `["JetBrains Mono NL", "monospace"]` | Values, code, logs, the debug pages, `FontFamily::Monospace`. |
| `font.family.display` (optional) | `{font.family.ui}` | Headings, if a theme wants a distinct face. |

Weights (type `fontWeight`, DTCG's own type, 1 to 1000, or keywords):

| Token | Default | Use |
|---|---|---|
| `font.weight.regular` | 400 | Body. |
| `font.weight.medium` | 500 | Buttons, labels of active items. |
| `font.weight.strong` | 600 | Headings, emphasis. |

Applied through `wght` when the face has the axis; with a static face
the nearest installed static weight family; otherwise ignored and
reported once ("Hack has no weight axis, strong drawn regular").
`RichText::strong()` keeps egui's colour meaning; egui-pop's own
`strong` helper sets both the colour and `font.weight.strong`.

Sizes: today four `px` values. Proposed scale, in `rem` (the DTCG
`dimension` reader already accepts `rem` at 16 px; egui-pop rem = the
body size, not 16 px, see below):

| Token | Comfortable | Use (egui `TextStyle`) |
|---|---|---|
| `font.size.caption` | 0.77 (10 px) | Captions, small print (`Small`). |
| `font.size.small` | 0.85 (11 px) | Secondary text, hints. |
| `font.size.body` | 1 (13 px) | Body, buttons (`Body`, `Button`). |
| `font.size.mono` | 0.96 (12.5 px) | Values and code (`Monospace`), so mono and proportional x-heights match. |
| `font.size.heading.3` / `.2` / `.1` | 1.08 / 1.23 / 1.38 | Section, panel, page headings (`Heading` = `.2`, others as `TextStyle::Name`). |
| `font.line-height` | 1.3 (a `number`) | Row height multiple, applied as `TextFormat::line_height` where egui-pop lays out text. |

Keep `font.size.small/body/heading/mono` as aliases for compatibility.

How a size is computed:

```
pt = round_to_half(token_rem * base(density) * text_scale)
```

- `base(density)`: 13 px comfortable, 12 px compact (density overlays set
  `font.size.base`).
- `text_scale`: the user's **text size** setting (accessibility), steps
  85 %, 100 %, 115 %, 130 %, 150 %, 175 %, 200 %. Quantised so the atlas
  holds a small set of sizes; rounding to half points keeps hinting
  steady.
- Text size is not `zoom_factor`. `zoom_factor` (interface scale) scales
  everything, spacing included, and stays a separate setting (Helios
  already lists "font size, interface scale" as settings outside the
  theme). Text size scales only text, and the control height token grows
  with it (`size.control.height = max(token, ceil(body * 1.6))`), so
  larger text never clips in a button.

Values and numbers:

- `font.numeric`: `"mono"` (default) or `"ui"`. Value displays (slider
  readouts, drag values, tables, histograms' readouts) use the mono family
  unless the theme says the UI face has tabular digits.
- egui-pop's value widgets right-align numbers and pad to a fixed number
  of digits per parameter (from the parameter's range and step), so the
  label does not jitter while dragging.

### Settings page "Text"

- **Interface font**: a picker listing bundled families first, then
  system families (fontique enumeration, proportional only), each name
  drawn in its own face; a "theme default" entry.
- **Monospace font**: the same list filtered to fixed-pitch faces
  (`post.isFixedPitch`, or all digits and `i`/`W` of equal advance).
- **Text size**: a stepped slider (the steps above) with the percentage.
- **Weight of body text**: regular or medium (low-vision aid on dark
  themes), only shown when the face has a `wght` axis.
- **Hinting**: auto (on below ppp 1.5, off above), on, off.
- **Preview**: a panel with a heading, a paragraph, a labelled slider
  with its value, a column of numbers (`0.00` to `-1234.56`), a code line
  (`0O1lI |{}[] != ->`), and a line per script of the shipped locales;
  missing glyphs are outlined in the warning colour.
- These are user settings, not theme tokens: changing theme keeps them.
  The theme provides the defaults; the settings override the families and
  scale at install.

### Debug bridge

A `fonts` core collector:

- snapshot: for each family (`ui`, `mono`, `display`, icons), the
  requested list, the resolved faces (name, source: bundled, system path
  hashed or file, theme file; axes and the weight used), the problems
  (names not found), `pixels_per_point`, `TextOptions`, the atlas size
  and `font_atlas_fill_ratio`, the computed point sizes per text style.
- action `check { text, family }`: the chars no face covers (`has_glyph`
  false) and which face draws each run.
- action `coverage { locale }`: runs `check` over every string of the
  string catalogue (`egui_pop_core::strings`) for that locale and lists
  missing chars with the keys that use them. The same function backs a
  test.
- A warning diagnostic when the atlas was rebuilt because it was full
  more than once in a minute (a sign of too many sizes or weights).

## 4. Upstream gaps (for `docs/upstream.md`)

Each entry follows the register's format (compensated, upstream,
workaround, remove when). Issues to open or find:

1. **No OpenType features per text run** (`tnum`, `zero`, `calt` off,
   `ss01`). harfrust takes features in `ShapeOptions`; epaint passes
   none. Upstream: add `features` to `TextFormat`, next to `coords`
   (small, symmetric with variations; file a PR). Workarounds: pick fonts
   whose defaults are right (no-ligature mono, tabular digits); for
   numbers in a proportional face, egui-pop's `tabular` text helper lays
   each digit in a cell as wide as the widest digit (`glyph_width` of
   `0`..`9`), painting digits one by one; or freeze `tnum` into a derived
   font offline (fontTools `pyftfeatfreeze`, allowed by OFL when there is
   no Reserved Font Name, renamed anyway). Remove when features land.
2. **No weight or style in `FontId`**, `strong` is colour only, italics
   are a shear. Workaround: variable fonts and `variation(b"wght", ..)`,
   static weights as `FontFamily::Name`, real italics as their own
   family. Remove when egui gets `RichText::bold` / a weight in `FontId`
   (listed as planned in the 0.34 notes).
3. **No bidi**: RTL lines are drawn in logical order. Workaround: none
   good; do not ship RTL locales until fixed (or reorder with
   `unicode-bidi` before layout for single-direction labels only).
   Track the upstream issue.
4. **No system font fallback in 0.36.** Fixed by #8491 (0.37). Until
   then: bundled faces, plus an opt-in fontique fallback per locale
   script at install.
5. **No colour emoji in 0.36.** Fixed by #8492 (0.37). Workaround: icons
   by role (already done), no emoji in UI strings.
6. **Atlas thrown away when 80 % full.** Workaround: quantised sizes and
   weights, a larger `max_texture_side` on native. Possible upstream
   proposal: evict least-used glyphs instead of rebuilding.
7. **No callback when a replacement glyph is drawn.** Workaround: the
   `check`/`coverage` actions above, run over the string catalogue.

## 5. Tests

- **Determinism.** Tests install only bundled faces: a
  `FontSource::BundledOnly` mode in `egui_pop_test` that disables the
  system provider (and, from egui 0.37, `system_font_fallback`), so a
  snapshot never depends on the machine's fonts. Rasterisation is CPU
  (`vello_cpu`), so with the same bytes and options glyphs are identical;
  the existing wgpu/lavapipe snapshot threshold covers the compositing.
  Pin hinting and sub-pixel options in the test harness.
- **Resolution.** Unit tests on the resolver (no egui): a list with an
  unknown name, a bundled name, a generic name; the problems reported;
  the bundled family always last.
- **Fallback.** A test font fixture without `é` (or a missing-glyph
  string): `check` reports it; a chain with a second face that has it
  reports nothing and the run uses the second face.
- **Monospace alignment.** Lay out `0123456789`, `-1234.56`, `iiiii` and
  `WWWWW` in the mono family: equal advances per char, and a column of
  numbers right-aligned has digit `x` positions equal across rows
  (compare `Galley` glyph positions, not pixels).
- **Tabular digits** of the UI face: all ten digits have the same advance,
  or the test documents that they do not and `font.numeric` stays
  `"mono"`.
- **Size scaling.** For each text-size step and both densities: the
  computed point sizes, monotonic, rounded to half points; the control
  height grows so a `Body` button label is not clipped (galley height <=
  inner height).
- **Weights.** A variable face at `wght` 400 and 600: the 600 galley is
  wider; a static face reports the problem once.
- **Coverage.** For every shipped locale, `coverage(locale)` over the
  catalogue returns nothing missing (pseudo-locale included).
- **Snapshots.** The gallery's new "Text" page (families, scale, weights,
  number column, scripts) in each theme and density.

## 6. Implementation plan (egui-pop commits)

1. **Docs: fonts design and upstream entries.** This note condensed into
   `docs/fonts.md`; entries 1, 2, 3 and 6 in `docs/upstream.md`.
2. **Tokens: font scale, weights, line height.** `font.size.*` scale
   with aliases, `font.size.base` in density overlays, `font.weight.*`
   (`fontWeight` type in the parser), `font.line-height`,
   `font.numeric`. Contract and docs updated. No egui change.
3. **Bundled fonts.** A `bundled-fonts` feature in `egui-pop-core` with
   the UI and mono faces (subset, licence files, a `FONTS.md` with
   sources and versions), registered by name. Tests: faces parse, axes
   listed.
4. **Family resolver and install.** `egui_pop_core::fonts`: resolve
   `font.family.*` lists (bundled, theme file, generic) into
   `FontDefinitions` with the icon family kept, report unknown names as
   theme problems, called by `theme::install`. Snapshots regenerated
   (the visible change of the series).
5. **Text size setting.** `text_scale` in `ThemeSpec` (quantised steps),
   sizes computed in points, control height follows. Tests on the scale.
6. **Weights and strong.** Weight tokens applied through `wght`; a
   `strong` helper; headings at `font.weight.strong`.
7. **Tabular values.** Value widgets use `font.numeric`; the `tabular`
   helper for proportional digits; the alignment tests.
8. **Debug collector `fonts`.** Snapshot, `check`, `coverage`; the
   coverage test over the English catalogue.
9. **System fonts (opt-in).** `system-fonts` feature: fontique
   enumeration for the pickers and per-script fallback for the locale.
   Off in tests.
10. **Settings page "Text" and gallery page.** Pickers, size, weight,
    hinting, preview; snapshots.
11. **On the egui 0.37 bump.** Switch the fallback to
    `SystemFontProvider` (with the UI locale), drop commit 9's fallback
    part, remove the symbol workaround, consider colour emoji; review
    `docs/upstream.md`.

## What Helios then uses

- Parameter values, slider readouts, coordinates, histogram and scope
  readouts (`L 52.3  a -4.1  b 12.0`), the node graph's numeric ports,
  the log and the agent/debug pages: `font.family.mono` at
  `font.size.mono`, right-aligned, fixed digits per parameter. Columns of
  values in the module panel line up while dragging.
- Module labels, menus, dialogs, hints: `font.family.ui`; panel headings
  at `font.weight.strong`.
- The "Text" settings page as is, with the Helios defaults in the Helios
  themes; font size and interface scale stay ordinary settings outside
  the theme (as `docs/gui.md` decides).
- Locales: ship Latin, Greek, Cyrillic in the bundle; CJK through the
  system (0.37) or an optional font pack; Arabic and Hebrew not shipped
  until bidi exists. Screenshot tests per script and density, as the
  platform pitfalls note asks (darktable showed per-OS font regressions,
  e.g. [darktable#13084](https://github.com/darktable-org/darktable/issues/13084)).

## Sources

- epaint 0.36.2 source: `src/text/fonts.rs` (`FontDefinitions`,
  `FontTweak`, `HintingTarget`, `FontData::variation_axes`, default
  fonts, `begin_pass` atlas rebuild), `src/text/font.rs` (skrifa,
  vello_cpu, sub-pixel bins, CJK, invisible bidi chars),
  `src/text/text_layout.rs` (`segment_into_runs`, `shape_text` with
  `ShapeOptions::new()`, italics shear), `src/text/mod.rs`
  (`TextOptions`), `Cargo.toml` (skrifa 0.44, harfrust 0.12, vello_cpu
  0.1). egui 0.36.2 `src/widget_text.rs` (`variation`, `strong`).
- harfrust 0.12 `ShapeOptions` (has a `features` slice).
- egui releases: <https://github.com/emilk/egui/releases> (0.34 font
  variations, 0.35 harfrust and hinting target).
- egui PRs #8491, #8492, #8493, #8512; issue #5233.
- fontique: <https://crates.io/crates/fontique>; fontdb:
  <https://crates.io/crates/fontdb>; font-kit:
  <https://crates.io/crates/font-kit>.
- SIL Open Font License FAQ: <https://openfontlicense.org/ofl-faq/>;
  Ubuntu Font Licence: <https://ubuntu.com/legal/font-licence>.
- Font projects as linked in the table. File sizes are approximate and
  must be measured when the subset is built.
