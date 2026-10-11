# Changelog

All notable changes to the egui-pop crates. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/), with one egui-pop minor per
egui minor.

## Unreleased

### Added

- `egui-pop-model` (no egui): typed ids serialized as strings and checked
  at compile time for literals; diagnostics that log and collect what
  registration ignored or changed; owner-tagged registries removable by
  owner; `when` clauses with focus depth; commands declared once (keys,
  menus, palette, agent surface) with a bounded run history and a ranked
  palette search; a layered keymap with chords, focus-depth precedence,
  command removal (`"-id"`), key unbinding (`null`) and a resolution
  trace; declared menus and panels; layouts and their repair; a value
  store (descriptors, soft and hard ranges, steps, gestures, `if_seq`,
  undo, change listeners, a journal, settings export and import with
  values kept for paths declared later) and its `value.*` commands;
  versioned documents with migrations (`layout` v2 renames the areas to
  start and end); plugin ordering with typed errors.
- `egui-pop-core` (egui): focus scopes and a focus chain rebuilt each
  pass, giving context keys their depth; a key dispatcher (chords with a
  timeout, keys consumed when they resolve, the last traces kept); an
  overlay stack (Escape closes the top only, outside clicks stop at a
  modal, closing closes what is above, focus restored) and overlays drawn
  as modals or placed popovers; placement with flip, shift and shrink;
  accessible names and an announcer with live regions; a `Strings`
  provider with English fallbacks; icons by role and tone (glyphs,
  images, painted shapes); a persistence driver (atomic writes, one
  backup, migrations on load); fault boundaries showing an error card
  with Retry; plain, bound and watch-only value binding.
- A layering test reading `cargo metadata`: dependencies between
  workspace crates point down the layers of ADR 0001.
- `egui-pop-tokens`: a DTCG 2025.10 token parser and resolver with no egui
  dependency: groups, inherited `$type`, `$description` and `$extensions`
  kept, whole-value aliases, `$extends` between theme files, colours in
  sRGB, OKLCH and OKLab. Alias and `$extends` cycles, unknown aliases,
  wrong types and missing contract tokens are reported and fall back to
  `pop-dark`; resolving never panics.
- Three tiers of tokens (palette, system roles, component tokens) and a
  contract of the system and component tokens every theme provides.
- Built-in themes `pop-dark`, `pop-light` and `pop-high-contrast`;
  densities `comfortable` and `compact`; accents `gold`, `blue`, `green`,
  `amber` and `red`; reduced motion; per-token overrides, composed by a
  `ThemeSpec`.
- WCAG 2.x contrast checks over an explicit list of token pairs, holding
  every built-in theme, density and accent to 4.5:1 for text and 3:1 for
  interface parts (7:1 for text in high contrast).
- `egui-pop`: `theme::install` turns resolved tokens into egui's `Visuals`
  and `Style` and stores a typed `Tokens` set in the context
  (`Tokens::of`); `egui` and `egui-pop-tokens` re-exported.
- `egui-pop-gallery` (not published): a catalogue application with theme,
  density, accent and motion switchers, pages for the theme overview, the
  token table and egui's built-in widgets, and options for scripted runs;
  it builds for the web too.
- Snapshot tests of the gallery per theme (egui_kittest on wgpu), a wbox
  workspace to run the gallery offscreen, and CI: format, clippy, tests on
  lavapipe, cargo-deny, the web build and semver checks.

### Changed

- `theme` moved to `egui-pop-core`; `egui-pop` is now the facade and
  re-exports it at the same path, with `model` and `core`.

- The repository is a Cargo workspace; the `egui-pop` crate moved to
  `crates/egui-pop`.
