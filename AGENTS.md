# Working on egui-pop (for agents and newcomers)

## Where things are

| Path | What |
|---|---|
| `crates/egui-pop-tokens` | L0, design tokens (DTCG parser, themes, contrast). No egui. |
| `crates/egui-pop-win32` | L0, the frame's Windows hit-test layer (`SetWindowSubclass`, `WM_NCHITTEST`); no egui, `unsafe` allowed only in its `sys` module (as in `egui-pop-wayland`). |
| `crates/egui-pop-wayland` | L0, the image viewport's Wayland layer: a subsurface under a window, tagged through `wp_color_management_v1`, and the compositor's preferred image description; no egui, `unsafe` allowed only in its `sys` module (the window's raw handles). |
| `crates/ortho-router` | L0, a standalone crate without dependencies: orthogonal connector routing around rectangles (a Hanan grid and A* with a bend cost), published on its own for any project; the node graph's orthogonal wires. |
| `crates/egui-pop-model` | L1, the logic without egui: ids, diagnostics, `when` clauses, commands, keymap, menus, panels, layouts, value store, documents and migrations, plugin ordering. |
| `crates/egui-pop-core` | L2, egui: theme, focus scopes, key dispatcher, overlays, placement, accessibility, strings, icons, persistence, fault boundary, value binding, thumbnails and their grid, debug bridge. |
| `crates/egui-pop-api` | L3, the application API: `Plugin`, `AppBuilder`, `App`, `Ctx`, zones and blocks, hooks, services, stories. |
| `crates/egui-pop-viewport` | L3, a component on the egui layer: the image viewport (view model and mapping, tile source and cache, GPU path, display transform, overlays, hooks, debug collector), thumbnails (re-exported from the egui layer), the histogram, the curve editor, colour wheels, the zoom control and navigator; `examples/viewer.rs` implements a tile source. |
| `crates/egui-pop-i18n` | L4, a plugin on the application API: translations over Fluent (catalogues and their fallback chains, the language setting, locale formatting, the `i18n.resolve` step, the key extractor and checks); egui-pop's own catalogues in `locales/`. |
| `crates/egui-pop-platform` | L4, plugins on the application API: platform services (a single instance and deep links, notifications, a tray icon, global shortcuts, logs and crash reports, the focus mode read in-process), each a Cargo feature and an opt-in plugin. |
| `crates/egui-pop-views` | L4, a component on the application API: data views (a live tree over a lazy, asynchronous source; a list; a data table wrapping `egui_table`), virtualised, keyboard-driven, every moment a step. |
| `crates/egui-pop-editors` | L4, a component on the application API: text and code editors (a line buffer, virtualised drawing, search and replace, a gutter of markers, folding, a pluggable highlighter with syntect by default, code assist providers), every edit a step, driven by commands; a terminal (vt100's emulator over a source: a PTY behind the `pty` feature, or a read-only log console). |
| `crates/egui-pop-imaging` | L4, the image tools in an application: each finished change of the viewport crate's histogram drag, curve editor, colour wheels, zoom control and navigator runs as a step (`histogram.drag`, `curve.change`, `wheel.change`, `zoom.change`), declared by `ImagingPlugin`. |
| `crates/egui-pop-agent` | L4, opt-in: the debug bridge served out of the process, JSON-RPC 2.0 (`describe`, `snapshot`, `call`, `wait`) over a Unix socket in the runtime directory (mode 0600) or a Windows named pipe; requests drained on the UI thread at each pass, `wait` held by the transport (`docs/agent-socket.md`). |
| `crates/egui-pop-test` | L4, test kit: themed harness, "every interactive node has a name", accessibility tree dump. |
| `crates/egui-pop-test-macros` | L0, the test kit's attributes (`critical`), re-exported by `egui-pop-test`; a procedural macro crate without dependencies. |
| `crates/egui-pop` | L5, the facade users depend on. Holds the layering test. |
| `crates/egui-pop-gallery` | The catalogue app, a plugin app; its pages are the snapshot fixtures. |
| `crates/consumer-smoke` | Compiles the README and guide snippets (not published). |
| `docs/adr/` | Architecture decisions. Read them before changing a registry. |
| `.changeset/`, `knope.toml` | Pending changesets and the release configuration: knope raises the versions, `cargo xtask release-notes` writes `CHANGELOG.md` (`docs/releasing.md`). |
| `docs/tokens.md` | The token tiers and how to add a theme. |
| `templates/app` | The application template: the end state of `docs/first-app.md` (a plugin with panels, a setting, commands for people and agents, undo, a kit, kittest tests and a snapshot), a workspace member so the gate keeps it building; `cargo generate` writes its `Cargo.toml.liquid`. |
| `templates/extension` | The extension template: a third-party crate (a plugin, a component with tokens and strings, a story, a step, an agent command, kittest tests), a workspace member so the gate keeps it building; `cargo generate` writes its `Cargo.toml.liquid`. |
| `book.toml`, `docs/SUMMARY.md`, `tools/book/` | The docs site: `docs/` as an mdBook under `/docs/` on GitHub Pages, the workspace's rustdoc (every feature) under `/api/`; `tools/book/links.py` turns links out of `docs/` into GitHub links and fails on a broken one; `tools/book/summary.py` checks that `docs/SUMMARY.md` lists every page `docs/README.md` links. |
| `tools/site/` | The site's landing page (`index.html`, static, at the root of GitHub Pages, beside the gallery under `/gallery/`), and `links.py`, which checks its links in the assembled site. |
| `tools/wbox/` | Running the gallery offscreen in a nested compositor. |
| `tools/web-smoke.mjs` | The web smoke test: loads a Trunk build of the gallery in headless Chrome, fails on a panic or without the start line (CI's wasm job, the Pages workflow). |

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
- One purpose per commit, signed off (`git commit -s`), its subject in
  Conventional Commits (`feat(views): ...`, `fix(core)!: ...`; types and
  scopes in CONTRIBUTING.md), and so is the pull request's title.
- What users notice comes with a changeset in `.changeset/`: the crates,
  each one's bump (`patch`, `minor`, `major`) and the entry's text. A
  break is `major` (before 1.0 it makes a new minor, 0.2 to 0.3), and
  every published crate depending on the broken one takes `major` too.
  Never edit `CHANGELOG.md` by hand; a change users do not notice inside
  a published crate is labelled `internal`. Details in
  `docs/releasing.md`; `cargo xtask changeset-check --base origin/main`
  checks locally.
- Never publish or tag: `knope release` prepares a release as a pull
  request; publishing and tagging are the owner's.
- GUI runs only headless (kittest, or wbox); never open a window on a
  desktop session from automation.
- A new crate goes into the layering table of
  `crates/egui-pop/tests/layering.rs` and of ADR 0001.
