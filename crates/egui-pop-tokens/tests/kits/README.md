# Kit fixtures

- `brand/`: a clean kit folder: `kit.yaml` with a manifest, a second
  token file (in JSON, read too), a face and its licence. `fonts/InterVariable-subset.ttf` is
  [Inter](https://github.com/rsms/inter) (SIL Open Font License 1.1, the
  kit's `LICENSE`), the subset `egui-pop-core`'s font tests use.
- `broken/`: one problem of each kind a folder can have (its `kit.json`
  is JSON, `trap.tokens.yaml` holds an unquoted colour).
- `not-a-kit/`: no kit file, skipped by discovery.
