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

CI also runs `cargo deny check`, builds the gallery for
`wasm32-unknown-unknown` and runs `cargo semver-checks` on `egui-pop`.

## Screenshots

The images in `docs/screenshots/` (shown in the README and the tokens
guide) are the gallery's real window, captured offscreen in a nested
compositor with [wbox](tools/wbox). Regenerate them whenever the gallery
changes visibly:

```sh
cargo build --release -p egui-pop-gallery
GALLERY_THEME=pop-light GALLERY_DENSITY=comfortable GALLERY_PAGE=overview \
  <driver> up tools/wbox/config.yaml
<driver> click tools/wbox/config.yaml 1070 700   # focus, away from tooltips
<driver> key tools/wbox/config.yaml ctrl+shift+p # optional: keys to press
<driver> shot tools/wbox/config.yaml --name overview-pop-light
<driver> down tools/wbox/config.yaml
```

`<driver>` is any wbox driver (the `wbox-mcp` server's tools, or a
script calling the wbox library); none is shipped here. Crop the shot to
the window and keep each PNG small (under 200 KB). The current set:
`overview-pop-dark`, `overview-pop-light`, `overview-pop-high-contrast`,
`builtins-pop-dark-compact`, `zones-pop-dark`, `zones-pop-light-off`
(after `ctrl+e`), `debug-pop-dark` (after `alt+up alt+down`), `palette-pop-dark` (after
`ctrl+shift+p`), `values-pop-dark`, `dialogs-pop-dark` (after a click on
"Export settings"), `toasts-pop-light` (after Warning, Error and
"Success with an action"), `settings-window-pop-dark` (after `ctrl+comma`)
and `shortcuts-pop-dark` (then "Keyboard shortcuts"). The window is
decorated in the compositor: crop to it (`89,26 1102x748` at the default
size).

## Upstream limits

When you hit a limit of egui (or of another dependency), work around it
in egui-pop, file a focused upstream issue with a minimal reproduction
after checking that none exists, and record it in
[docs/upstream.md](docs/upstream.md) with an `// upstream:` comment at the
code site. The register is reviewed at every egui bump.

## Reporting issues

Give the egui-pop and egui versions, the operating system and the GPU (or
software renderer), and a minimal way to reproduce. Component proposals
describe the keyboard map and the tokens the component needs.
