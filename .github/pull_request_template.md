<!--
Title: Conventional Commits, e.g. `feat(views): the list sorts by any column`
or `fix(core)!: ...` for a breaking change (CONTRIBUTING.md, "Commit subjects").
-->

## What and why



## Checklist

- [ ] One purpose; every commit builds, passes its tests and is signed off (`git commit -s`).
- [ ] A changeset in `.changeset/` names each crate users will notice a change in, with its
      bump: `patch` (a fix), `minor` (an addition), `major` (a break; before 1.0 it makes a
      new minor, 0.2 to 0.3), and every published crate depending on a broken one takes
      `major` too. Or nothing users notice changed, and the pull request is labelled
      `internal` (or touches no published crate). See `docs/releasing.md`.
- [ ] A new component comes with interaction tests, a gallery page and snapshots.
