# Changelog

All notable changes to the egui-pop crates. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/), with one egui-pop minor per
egui minor.

## Unreleased

### Added

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

- The repository is a Cargo workspace; the `egui-pop` crate moved to
  `crates/egui-pop`.
