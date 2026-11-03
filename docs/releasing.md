# Changesets and releases

How a change reaches the changelog and a version number, and how a
release is made. Versions and changelog entries are mechanical: each
pull request says what it changes in a **changeset**, and
[knope](https://knope.tech) turns the pending changesets into versions
at release time. What a version promises is in
[API stability](stability.md).

## Changesets

A change users of the crates notice comes with a changeset: a Markdown
file in `.changeset/`, named after the change (`toast-corner.md`). Its
front matter names each crate the change touches and the bump it takes;
the text below is the changelog entry, written for users.

```markdown
---
egui-pop-core: major
egui-pop-api: major
egui-pop: major
---

The debug bridge has a mode: `AppBuilder::debug_bridge` takes a
`BridgeMode` (`On`, the default as before, `ReadOnly` or `Off`). To move:
replace `debug_bridge(true)` with `debug_bridge(BridgeMode::On)`.
```

`knope document-change` asks for the crates, the bumps and a summary
and writes the file; writing it by hand is as good. The names are the
crates' package names; the gallery, the template and the tools are not
published and take no changeset.

### Which bump

| Bump | For | Before 1.0 | From 1.0 |
|---|---|---|---|
| `patch` | a fix | 0.2.3 to 0.2.4 | 1.2.3 to 1.2.4 |
| `minor` | an addition, or a change that breaks nothing | 0.2.3 to 0.2.4 | 1.2.3 to 1.3.0 |
| `major` | a breaking change | 0.2.3 to **0.3.0** | 1.2.3 to 2.0.0 |

Before 1.0, a breaking change takes a new **minor** version (0.2 to
0.3), and additions and fixes share the patch number. A changeset still
says `major` for a break: knope applies it to the middle number while
the version starts with 0, and the changelog lists it under "Breaking
changes". Declare what the change is, not the number you expect.

What breaks is the Rust API (`cargo semver-checks` tells), and also the
extension contract: tokens, steps, commands, strings and ids, the debug
bridge ([GOVERNANCE.md](../GOVERNANCE.md#what-breaks-an-extension)).

**A break travels up.** The crates above a broken one re-export its
types, so each published crate depending on a crate that takes a
`major` takes a `major` too, up to the facade. The check below names the
missing ones.

One changeset per change; a pull request doing one thing has one. A
changeset may name several crates with different bumps; its entry goes
under its highest bump and names every crate it lists.

### When none is needed

Tests, examples, benchmarks, the documentation, the gallery, CI and the
tools change no published crate. A change inside a crate that users do
not notice (a refactoring, a comment, an internal rename) needs none
either: label the pull request **`internal`**.

### The check

CI's **Changesets** job (`cargo xtask changeset-check`) fails when:

- a changeset names a crate that is not published, or a bump other
  than `patch`, `minor` or `major`;
- a crate takes a `major` and a published crate depending on it does
  not;
- a pull request changes the shipped files of a published crate (its
  `src/`, its manifest, its README; not `tests/`, `examples/` or
  `benches/`) and adds no changeset, unless it is labelled `internal`.
  Adding the label runs CI again.

A release pull request, which deletes changesets and adds none, passes.
Locally:

```sh
cargo xtask changeset-check --base origin/main
```

## Commit subjects

Commit subjects, and pull request titles (a squash merge makes the
title the commit's subject), follow
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

```
<type>(<scope>)!: <what changed>
```

- **type**: `feat` (an addition), `fix`, `perf`, `refactor`, `docs`,
  `test`, `build`, `ci`, `chore` (`chore(release)` for a release),
  `style` or `revert`;
- **scope**, optional: the crate without its `egui-pop-` prefix
  (`core`, `api`, `views`), or the area (`gallery`, `xtask`, `ci`,
  `deps`);
- **`!`** marks a breaking change, as `major` does in its changeset.

```
feat(views): the list sorts by any column
fix(core)!: a focus scope restores the focus it took
docs: how a release is made
```

knope reads the changesets only, not the commits
(`ignore_conventional_commits` in `knope.toml`): a change described by
both would be counted twice. The subject format keeps the history
readable and says at a glance what a commit is. CI's **Pull request
title** workflow checks the title, and runs again when it is edited.

## Releasing

Publishing is the owner's call, and stays a manual step.

1. **Prepare**, on a branch from an up-to-date `main`:

   ```sh
   knope release --dry-run   # what would change
   knope release
   ```

   knope raises each crate's version from the pending changesets (its
   manifest, its requirement in `[workspace.dependencies]`, `Cargo.lock`)
   and deletes them; then `cargo xtask release-notes` writes the release
   into `CHANGELOG.md` and commits everything, signed off, as
   `chore(release): <the facade's version>` (or the crates and their
   versions when the facade does not move). The section lists the
   crates it publishes and groups the entries under "Breaking changes",
   "Features" and "Fixes", each entry naming its crates.

2. **Review**: push the branch, open a pull request titled like the
   commit, read the changelog and the versions. CI's semver job checks
   the new versions against crates.io. Merge it.

3. **Publish**, from the merged commit on `main`, by hand:

   ```sh
   cargo xtask publish --dry-run   # the plan, and crates changed but not raised
   cargo xtask publish
   ```

   It publishes the crates whose version is not on crates.io yet, in
   the order of their dependencies.

4. **Tag** a minor of the facade, `vX.Y.0`, on the commit it was
   published from; patches and sub-crates are not tagged:

   ```sh
   cargo xtask tag                 # refuses until crates.io has the version
   git push origin v0.3.0
   ```

knope is a development tool, not a dependency:
`cargo install knope --locked` (CI pins its version, see
`.github/workflows/ci.yml`).

### Semver checks

CI's **Semver checks** job blocks a merge. It first runs
`knope pending-versions`, which writes in its checkout the versions the
pending changesets lead to, then
`cargo semver-checks check-release --workspace`, which compares every
published crate with its latest release on crates.io (the baseline is
what users have). A breaking change passes when its changeset makes
the crate's version a new minor (before 1.0) and fails otherwise,
naming what broke.

## The switch to changesets

Changesets started after 0.2.0. The changes merged before them are in
`CHANGELOG.md`'s `Unreleased` section, written by hand; one changeset
without text, `.changeset/since-0.2.0.md`, declares their bumps. The
0.3.0 release moves that section under its heading: `release-notes`
puts any `Unreleased` section it finds into the release it writes.
From then on, entries are written as changesets only.
