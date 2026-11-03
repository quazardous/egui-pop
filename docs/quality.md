# How quality is held

egui-pop is developed with AI assistance: most of its code, tests and
documentation are written by AI coding agents, working to the plan and
the design decisions of the maintainer. That is why it moves fast. This
page says what every change must pass before it reaches `main`, and,
as plainly, what those checks do not cover.

## Who writes, who decides

- **The maintainer decides** what is built and how: the
  [credo](../CREDO.md), the [architecture decisions](adr/README.md)
  (written before the code they shape), what each release promises
  ([API stability](stability.md)) and [governance](../GOVERNANCE.md).
- **Agents work under written rules**: [AGENTS.md](../AGENTS.md) and
  [CONTRIBUTING.md](../CONTRIBUTING.md). One purpose per commit, every
  commit building and passing its tests, signed off, with a Conventional
  Commits subject and a changeset for what users notice; a component comes with interaction
  tests, a gallery page and snapshots; an existing crate is surveyed
  before writing our own.
- **Changes reach `main` through pull requests**, and a pull request
  merges only when the required check passes. There is no second human
  reviewer today: a pull request is merged by the maintainer, or by an
  agent acting for them, once its checks are green. The checks below are
  what stands between a change and `main`, and they are meant to be
  strict.

## The gate on every commit

Every commit passes, locally, before it is pushed:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## What CI runs on a pull request

The workflow is [`.github/workflows/ci.yml`](../.github/workflows/ci.yml).
Every build there treats warnings as errors (`RUSTFLAGS` and
`RUSTDOCFLAGS` set to `-D warnings`), and every job but the semver
checks uses the locked dependencies and the minimum supported Rust
version. One check, `CI`, is required by the
branch protection of `main`: it fails when any job below failed. A
separate workflow, `Pull request title`, checks that the title follows
Conventional Commits; it runs again when the title is edited.

| Job | What it checks | Blocks a merge |
|---|---|---|
| Format | `cargo fmt --all --check` | yes |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo doc` of the workspace | yes |
| Guides' documentation | the README and the guides compiled as documentation (`consumer-smoke`), when only they changed | yes |
| Docs site | the mdBook builds, `docs/SUMMARY.md` lists every page of the index, no link is broken | yes |
| Tests (lavapipe) | unit and integration tests with cargo-nextest, and doctests (below) | yes |
| Viewer's video | the one build with GStreamer: clippy, docs and the video tests | yes |
| Viewer's web view | the one build with WebKitGTK: clippy, docs and the YouTube page's tests | yes |
| Licences and advisories | `cargo deny check`: every dependency's licence accepted by `deny.toml`, security advisories, banned crates, sources | yes |
| Gallery for the web | the gallery built for `wasm32-unknown-unknown` with Trunk, then started in headless Chrome: it fails on a panic or without the start line | yes |
| Packaging | `cargo publish --workspace --dry-run`: every published crate packages with its metadata | yes |
| Changesets | `cargo xtask changeset-check`: a pull request changing a published crate adds a changeset (or is labelled `internal`); changesets name published crates and known bumps; a break travels up to the dependents | yes |
| Semver checks | `cargo semver-checks` on every published crate against its latest release on crates.io, with the versions the pending changesets lead to: a break needs a `major` changeset ([Changesets and releases](releasing.md#semver-checks)) | yes |

The tests job runs, among others:

- **UI tests** with [egui_kittest](https://github.com/emilk/egui/tree/main/crates/egui_kittest),
  finding widgets by role and name, never by position, keyboard
  included; **keyboard walks** that press only keys and check where the
  focus goes ([Testing](testing.md#keyboard-walks)).
- **Snapshots**: the gallery's pages rendered with wgpu on Mesa's
  software Vulkan driver, per theme and density, compared with the
  images in the repository.
- **Accessibility**: every interactive node has a name; contrast pairs
  pass in every built-in theme and density.
- **The layering test**: no crate depends on a crate of a higher layer
  ([ADR 0001](adr/0001-layering.md)).
- **Translations**: the English catalogue is generated from the code,
  every locale translates every key and nothing else.
- **Saved data**: old documents and layouts still load, migrated.
- **Doctests** of the crates, of the README and of the guides; the
  application template matches the minimal example; the extension
  template builds as a workspace member; README links are absolute, so
  they work on crates.io.

`unsafe` is forbidden in the whole workspace by its lints, except in
the two crates that call the operating system
([Unsafe code](https://github.com/quazardous/egui-pop#unsafe-code)).

### Selection, and what `main` runs

A pull request runs the jobs and the tests its change can affect,
chosen by `cargo xtask affected`, plus the critical tests that guard
rules any change can break ([Testing](testing.md#selection)). Selection
can miss a dependency it cannot see, so every push to `main` runs every
job and every test, and a pull request labelled `ci:full` does too. A
miss is then caught on `main`, after the merge, not before.

After a merge, the Pages workflow builds the docs site, the API
documentation with every feature, and the web gallery with its smoke
test again, then publishes them.

## What is not covered

- **Linux only.** CI runs on Ubuntu. Windows and macOS are neither
  built nor tested there; the Windows layer of the window frame
  (`egui-pop-win32`) compiles only its stubs in CI.
- **No real window.** Rendering tests draw offscreen through wgpu on a
  software renderer. A real compositor, a real GPU, the other wgpu
  backends (Metal, Direct3D, OpenGL) and display scaling as a desktop
  applies it are not exercised in CI. The screenshots are the gallery's
  real window in a nested compositor, but they are taken by hand
  (`cargo xtask screenshots`), not in CI.
- **Platform services** (single instance, notifications, tray icon,
  global shortcuts, crash reports, the Wayland colour-managed
  subsurface) are tested through their logic, not against a running
  desktop session.
- **Accessibility** is checked on the accessibility tree egui builds; no
  screen reader is run.
- **The web build** is checked to start; its interactions are not
  tested in a browser.
- **Snapshots** catch a change in the drawing, within thresholds per
  operating system (`kittest.toml`); when they are regenerated, only the
  person or agent looking at the diff judges whether the new images are
  right.
- **API compatibility** is checked on Rust signatures only:
  `cargo semver-checks` does not see the extension contract's other
  parts (tokens, steps, commands, strings), which the changeset's author
  judges ([GOVERNANCE.md](../GOVERNANCE.md#what-breaks-an-extension)).
- **No performance or memory gate**, no fuzzing.
