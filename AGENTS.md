# Working on egui-pop (for agents and newcomers)

## Where things are

| Path | What |
|---|---|
| `crates/egui-pop-tokens` | L0, design tokens (DTCG parser, themes, contrast). No egui. |
| `crates/egui-pop-win32` | L0, the frame's Windows hit-test layer (`SetWindowSubclass`, `WM_NCHITTEST`); no egui, the only crate allowed `unsafe`, in its `sys` module. |
| `crates/egui-pop-model` | L1, the logic without egui: ids, diagnostics, `when` clauses, commands, keymap, menus, panels, layouts, value store, documents and migrations, plugin ordering. |
| `crates/egui-pop-core` | L2, egui: theme, focus scopes, key dispatcher, overlays, placement, accessibility, strings, icons, persistence, fault boundary, value binding, debug bridge. |
| `crates/egui-pop-api` | L3, the application API: `Plugin`, `AppBuilder`, `App`, `Ctx`, zones and blocks, hooks, services, stories. |
| `crates/egui-pop-viewport` | L3, a component on the egui layer: the image viewport (view model and mapping, tile source and cache, GPU path, display transform, overlays, hooks, debug collector), thumbnails, the histogram; `examples/viewer.rs` implements a tile source. |
| `crates/egui-pop-i18n` | L4, a plugin on the application API: translations over Fluent (catalogues and their fallback chains, the language setting, locale formatting, the `i18n.resolve` step, the key extractor and checks); egui-pop's own catalogues in `locales/`. |
| `crates/egui-pop-platform` | L4, plugins on the application API: platform services (a single instance and deep links, notifications, a tray icon, global shortcuts, logs and crash reports, the focus mode read in-process), each a Cargo feature and an opt-in plugin. |
| `crates/egui-pop-views` | L4, a component on the application API: data views (a live tree over a lazy, asynchronous source; a list; a data table wrapping `egui_table`), virtualised, keyboard-driven, every moment a step. |
| `crates/egui-pop-editors` | L4, a component on the application API: text and code editors (a line buffer, virtualised drawing, search and replace, a gutter of markers, folding, a pluggable highlighter with syntect by default, code assist providers), every edit a step, driven by commands; a terminal (vt100's emulator over a source: a PTY behind the `pty` feature, or a read-only log console). |
| `crates/egui-pop-test` | L4, test kit: themed harness, "every interactive node has a name", accessibility tree dump. |
| `crates/egui-pop` | L5, the facade users depend on. Holds the layering test. |
| `crates/egui-pop-gallery` | The catalogue app, a plugin app; its pages are the snapshot fixtures. |
| `crates/consumer-smoke` | Compiles the README and guide snippets (not published). |
| `docs/adr/` | Architecture decisions. Read them before changing a registry. |
| `docs/tokens.md` | The token tiers and how to add a theme. |
| `template/` | The `cargo generate` application template, identical to `crates/egui-pop/examples/minimal.rs` (a test checks it). |
| `tools/wbox/` | Running the gallery offscreen in a nested compositor. |

Principles: [CREDO.md](CREDO.md). Rules for contributions:
[CONTRIBUTING.md](CONTRIBUTING.md).

## The gate

Every commit passes:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Snapshots: `UPDATE_SNAPSHOTS=1 cargo test -p egui-pop-gallery`, then look
at the images. Layering is checked by `cargo test -p egui-pop --test
layering`.

## Rules

- English everywhere; MIT; no code copied from GPL, LGPL or MPL projects.
- One purpose per commit, signed off (`git commit -s`), CHANGELOG entry
  for what users notice.
- GUI runs only headless (kittest, or wbox); never open a window on a
  desktop session from automation.
- A new crate goes into the layering table of
  `crates/egui-pop/tests/layering.rs` and of ADR 0001.
