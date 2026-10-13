# Extensions

An extension is a crate that adds to egui-pop through its public API: a
plugin registering commands, panels, settings, steps and stories, and
usually a component drawn from tokens. This page lists the known ones,
says how to get listed and what a listed extension must hold to.

To write one, start from the [extension template](../templates/extension)
(a plugin, a component with its tokens and strings in English and
French, a gallery story, a step, an agent command, headless tests):

```sh
cargo generate --git https://github.com/quazardous/egui-pop templates/extension
```

How egui-pop treats extensions across versions (what counts as a
breaking change for them, deprecation windows) is in
[GOVERNANCE.md](../GOVERNANCE.md).

## Tiers

- **Official**: in this repository, released with egui-pop, gated by
  its CI on every commit. Optional crates behind a feature of the
  `egui-pop` facade are official extensions.
- **Community**: in their authors' repositories, listed here after the
  checks below. Their authors release them; egui-pop does not build them.
- **Featured**: community extensions shown first and named in the
  release notes. Promotion is asked for in a pull request moving the
  entry, once the extension has followed two egui-pop minors (two egui
  bumps) within a month of each release, has users other than its
  author, and passes the checks. An extension left behind by a minor for
  more than three months goes back to community; one left behind by two
  minors is moved to the unmaintained list, and back when it catches up.
- An official extension may move out of the repository (to its own, as
  a community one) or a featured one into it, by a decision recorded as
  an ADR.

## Entry format

One entry per extension, under its tier, in this shape:

```markdown
### tally

- **Crate**: [`egui-pop-extension-template`](../templates/extension)
- **Adds**: a counter component, the `tally.change` step, `tally.set` for agents
- **egui-pop**: 0.1 (egui 0.36)
- **Licence**: MIT
- **Maintainer**: the egui-pop maintainers
```

- **Crate**: its name on crates.io, linked to its page or repository.
- **Adds**: one line: the components, plugins, steps and commands it
  brings.
- **egui-pop**: the egui-pop minors it builds against, with the egui
  minor each one uses (one egui-pop minor per egui minor).
- **Licence**: its SPDX expression.
- **Maintainer**: a person or group, with a way to reach them (the
  repository's issues are enough).

Entries are sorted by name within a tier.

## Getting listed

Open a pull request adding the entry under **Community**, with a link to
a release on crates.io. A maintainer runs the checks below and merges,
or says what is missing. The crate should also carry the `egui-pop`
keyword on crates.io, so a search finds it.

An entry is updated by a pull request too: a new egui-pop minor
supported, a new maintainer. An author may ask for their entry to be
removed at any time.

## Checks an entry must pass

- **Licence**: an OSI-approved licence that lets applications combine
  it with egui-pop (MIT). Permissive licences (MIT, Apache-2.0, BSD,
  Zlib) fit everywhere; a copyleft one (GPL, LGPL, MPL) is accepted and
  said in the entry, since it binds the applications using it. Its
  dependencies' licences say the same.
- **Builds against the stated versions**: `cargo test` passes against
  each egui-pop minor in the entry, with the egui of that minor and no
  second copy of egui in its dependency tree (`cargo tree -d -i egui`
  is empty).
- **Plugin rules**: its ids start with its plugin's id; it registers
  through `Ctx` only, so disconnecting its plugin leaves nothing behind
  (ADR 0004); nothing fails silently (ADR 0003); what changes state runs
  as a step (ADR 0011); its commands are declared once, with their
  arguments described when agents may run them.
- **Token contract**: colours, sizes and durations come from tokens,
  never literals; its component tokens are read by path with a system
  token as fallback (`tally.background`, else `surface.raised`) and
  named by role ([Design tokens](tokens.md#naming-rules)); text over its
  backgrounds keeps 4.5:1 (7:1 in the high-contrast themes) and a
  control's boundary 3:1 in every built-in theme, which a test of its
  own checks.
- **Accessibility**: every interactive widget has a role, a name and a
  value, reachable with the keyboard; its tests find widgets by role and
  label and run `egui_pop_test::assert_all_named`.
- **Strings**: every text shown has a key and an English fallback; its
  catalogues carry its plugin's id (`I18n::add_owned` refuses others).

The template passes all of them; an extension that keeps its tests'
shape passes most of them by construction.

## Official

### i18n

- **Crate**: [`egui-pop-i18n`](../crates/egui-pop-i18n) (feature `i18n`)
- **Adds**: Fluent catalogues, the language setting, locale formatting, the `i18n.resolve` step, the key extractor and checks
- **egui-pop**: 0.1 (egui 0.36)
- **Licence**: MIT
- **Maintainer**: the egui-pop maintainers

### imaging

- **Crate**: [`egui-pop-imaging`](../crates/egui-pop-imaging) (feature `imaging`)
- **Adds**: the image tools' steps (`histogram.drag`, `curve.change`, `wheel.change`, `zoom.change`, `cursor.resolve`, `graph.*`) and the node graph editor in an application
- **egui-pop**: 0.1 (egui 0.36)
- **Licence**: MIT
- **Maintainer**: the egui-pop maintainers

### platform

- **Crate**: [`egui-pop-platform`](../crates/egui-pop-platform) (features `platform-*`)
- **Adds**: a single instance and deep links, notifications, a tray icon, global shortcuts, logs and crash reports, the focus mode, each an opt-in plugin
- **egui-pop**: 0.1 (egui 0.36)
- **Licence**: MIT
- **Maintainer**: the egui-pop maintainers

### tally (the template)

- **Crate**: [`egui-pop-extension-template`](../templates/extension) (not published: copy it, or `cargo generate`)
- **Adds**: a counter component with its tokens and strings, a gallery story, the `tally.change` step, `tally.increment` and `tally.set` for agents
- **egui-pop**: 0.1 (egui 0.36), and every commit of `main`
- **Licence**: MIT
- **Maintainer**: the egui-pop maintainers

### views and editors

- **Crate**: [`egui-pop-views`](../crates/egui-pop-views) (feature `views`), [`egui-pop-editors`](../crates/egui-pop-editors) (feature `editors`)
- **Adds**: a live tree, lists and tables, the photo library's widgets; text and code editors and a terminal; every moment a step, agent commands
- **egui-pop**: 0.1 (egui 0.36)
- **Licence**: MIT
- **Maintainer**: the egui-pop maintainers

### viewport

- **Crate**: [`egui-pop-viewport`](../crates/egui-pop-viewport) (feature `viewport`)
- **Adds**: the image viewport (tiles from the application's source, wide gamut and HDR), thumbnails, the histogram, curves, colour wheels, the zoom control and navigator, the node graph editor
- **egui-pop**: 0.1 (egui 0.36)
- **Licence**: MIT
- **Maintainer**: the egui-pop maintainers

## Featured

None yet.

## Community

None yet: be the first, with a pull request.

## Unmaintained

None.

## Built with egui-pop

Applications, for reference and ideas; a pull request adds one (name,
link, licence, one line on what it shows of egui-pop).

- [Helios](https://github.com/quazardous/helios) (GPL-3.0): a
  client/server image processing framework whose first application is
  photo editing. The flagship: an editor over a kernel of its own, which
  egui-pop's host contract, image tools, workbench and library widgets
  are shaped for.
