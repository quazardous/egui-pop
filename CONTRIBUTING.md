# Contributing to egui-pop

Thank you for helping. A few rules keep the library easy to review and to
keep up with egui.

## Licence and sign-off

egui-pop is MIT licensed, and contributions come in under the same licence
(inbound = outbound): by contributing you agree that your work is published
under the MIT licence of this repository.

Every commit carries a Developer Certificate of Origin sign-off
(<https://developercertificate.org/>), a line stating you have the right to
submit it:

```
Signed-off-by: Your Name <you@example.org>
```

`git commit -s` adds it. Pull requests with unsigned commits cannot be
merged.

Do not copy code from projects under other licences (GPL, LGPL, MPL...);
behaviour seen elsewhere is reimplemented from its description. New
dependencies must be under a permissive licence accepted by `deny.toml`.

## One purpose per pull request

A pull request does one thing: a component, a fix, a theme, a refactoring.
Keep each commit building and passing its tests, with a message saying what
changed and why. Add an entry to the `Unreleased` section of
[CHANGELOG.md](CHANGELOG.md) for anything a user of the crates would notice.

## Tic and toc

Work alternates. A **tic** is an active refactor: it moves fast and may
leave bridges so that everything keeps working meanwhile (an old token
name kept as an alias in `RENAMED`, a typed shim next to a new
vocabulary, an adapter that loses part of what it translates). A
**toc** is the consolidation that follows: it regularises the notation
and removes the bridges. A tic records each bridge it leaves in the
current toc issue (labelled `toc`), so the toc has its list; a bridge
nobody wrote down is a bridge nobody removes.

## Components come with tests and a gallery page

Every component arrives with:

- **interaction tests** with egui_kittest, finding widgets by role and
  label (never by position), including the keyboard;
- **a gallery page** in `crates/egui-pop-gallery`: the component in its
  variants and states (rest, hover, focus, disabled), with a playground of
  its options. Pages are plain functions reused by the tests;
- **snapshots** of that page per theme (and density where it matters),
  under `crates/egui-pop-gallery/tests/snapshots`;
- **tokens**: colours, sizes and durations come from component tokens
  aliasing system tokens, never from literals; new contrast pairs go into
  `contrast::PAIRS`;
- accessibility: a role, a name, a value and actions on every interactive
  widget.

## Checks

Before pushing:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Snapshots render on wgpu; CI uses Mesa's software Vulkan driver
(lavapipe), and so do the tests locally when it is installed
(`mesa-vulkan-drivers` on Debian and Ubuntu). After a deliberate visual
change, regenerate them with
`UPDATE_SNAPSHOTS=1 cargo test -p egui-pop-gallery` and review the images
in the diff. Thresholds per operating system are in `kittest.toml`.

How tests are organised, and why every rendering test shares one GPU
device per process (a Vulkan loader crash), is in
[docs/testing.md](docs/testing.md).

CI also runs `cargo deny check`, builds the gallery for
`wasm32-unknown-unknown` and runs `cargo semver-checks` on `egui-pop`.

## Screenshots

The images in `docs/screenshots/` (shown in the README and the guides
in `docs/`) are the gallery's real window, captured offscreen in a nested
compositor with [wbox](tools/wbox). Regenerate them whenever the gallery
changes visibly, with one command:

```sh
cargo xtask screenshots              # every image
cargo xtask screenshots palette      # only the shots whose name contains "palette"
cargo xtask screenshots --list       # the shots' names
```

It needs wbox installed (`wbox-mcp` on `PATH`, or `WBOX_PYTHON` set to
the Python that imports it) and replaces the manual steps: it builds the
gallery in release mode, then for each shot starts the compositor
(headless, 1280x800 unless the shot says otherwise, scale 1) with the
gallery at the shot's page, theme and density and its animations off
(`--reduced-motion`), clicks an inert spot to focus it, plays the shot's
keys and clicks, moves the pointer off the window, captures without the
cursor and crops to the window (title bar included). The shots, their
steps and their coordinates are in `xtask/src/main.rs`; the wbox side is
`tools/wbox/shoot.py`.

It writes:

- `docs/screenshots/<name>.png`: the full images;
- `docs/screenshots/thumbs/<name>.png`: the README's thumbnails, 400 px
  wide (shown at 200);
- `docs/screenshots/themes/<theme>.png`: the overview in every built-in
  theme, at full size and never reduced, in a larger window (1280x800
  points in a 1440x900 output, `GALLERY_SIZE`) so its widgets read at
  1:1 (`docs/themes.md`);
- `docs/screenshots/themes.png`: the editor-inspired themes on one sheet
  (`docs/tokens.md`), rebuilt from the above.

PNGs are indexed when they have 256 colours or fewer, and quantised to
256 only when a file would exceed 200 KB. Look at the images in the diff
before committing them; a shot whose button moved needs its coordinates
updated.

## Releasing

Publishing is the owner's call. Each crate has its own version
([API stability](docs/stability.md#versions-of-the-crates)): raise the
version of each crate that changed since it was last published (and of
the crates above it when the change breaks), raise a requirement in
`[workspace.dependencies]` when a crate needs the newer version, and
date `CHANGELOG.md`'s entries as a release listing those versions.
Then:

```sh
cargo xtask publish --dry-run     # the plan: each crate, published or not
cargo xtask publish               # publish what is not on crates.io yet
```

The crates come from `cargo metadata` (every package not marked
`publish = false`), ordered by their dependencies; a version already on
crates.io is skipped. crates.io limits how many new crates an account
publishes at once: a refused crate is tried again after `--interval`
seconds (600 by default), and running the task again resumes where it
stopped. Each crate it publishes gets a local tag, `<crate>-vX.Y.Z`, on
the commit it was published from; push them (`git push --tags`).

## Upstream limits

How upstream limits are handled, from the workaround to its removal, is
in [docs/upstream.md](docs/upstream.md#how-a-limit-is-handled). In
short: when you hit a limit of egui (or of another dependency), work around it
in egui-pop, file a focused upstream issue with a minimal reproduction
after checking that none exists, and record it in
[docs/upstream.md](docs/upstream.md) with an `// upstream:` comment at the
code site. The register is reviewed at every egui bump.

## Extensions

An extension lives in its own repository, under its own licence; you do
not need to contribute it here to use or publish it.

- Start from the [extension template](templates/extension): copy it, or
  `cargo generate --git https://github.com/quazardous/egui-pop templates/extension`.
  Rename its ids after your plugin's id first.
- Depend on the `egui-pop` facade alone, and use `egui_pop::egui`: one
  copy of egui in the tree.
- Follow the rules components follow here: tokens, never literals (your
  component tokens read by path, a system token as fallback); a role, a
  name and a value on every interactive widget; text by key with an
  English fallback, in catalogues carrying your plugin's id; state
  changes as steps; tests with egui_kittest finding widgets by role and
  label.
- To be listed, open a pull request adding your entry to
  [docs/extensions.md](docs/extensions.md); the checks it must pass are
  there.
- What egui-pop treats as breaking for you, and how long a deprecated
  item keeps working, is in [GOVERNANCE.md](GOVERNANCE.md). A change you
  need in egui-pop's API is proposed as an ADR (a request for comments).
- A limit of egui you work around in your extension is worth an entry
  in [docs/upstream.md](docs/upstream.md) when egui-pop could compensate
  for everyone.

## Reporting issues

Give the egui-pop and egui versions, the operating system and the GPU (or
software renderer), and a minimal way to reproduce. Component proposals
describe the keyboard map and the tokens the component needs.
