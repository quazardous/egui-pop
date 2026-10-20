# Testing

How egui-pop is tested, and what to know when a test misbehaves.

## Goal: time that does not grow with the tests

The suite's wall time must not grow linearly with the number of tests.
Two levers:

- **Parallel runs**: every test binary runs side by side, every test in a
  process of its own (cargo-nextest, below);
- **Selective runs**: a change runs the tests it can affect, not the
  whole suite ([Selection](#selection)); `main` always runs everything.

The integration tests are split into many small binaries
(`crates/*/tests/*.rs`, one per feature) for that reason: a binary is a
unit of selection, and editing one test file relinks one small binary.
Do not merge them back into one.

## The runner: cargo-nextest

CI runs the tests with [cargo-nextest](https://nexte.st), configured in
`.config/nextest.toml`. Plain `cargo test --workspace` still works and
remains the reference: every commit passes it.

```sh
cargo install cargo-nextest --locked     # once
cargo nextest run --workspace            # every test, binaries side by side
cargo test --workspace --doc             # doctests (nextest does not run them)
cargo nextest run -p egui-pop-viewport   # one crate
```

What the configuration sets:

- **Retries for tests that may render.** A crash of the Vulkan loader
  (below) kills the process of one test instead of a whole binary, and
  that test is tried again twice before it counts as a failure; nextest
  lists it as flaky. The crates whose tests never render
  (`egui-pop-tokens`, `egui-pop-model`, the platform layers, `xtask`)
  get no retry. A flaky test in the report is still a bug to look at.
- **Slow tests are reported** after 60 seconds (10 for critical tests).
  The `ci` profile stops a test after 4 minutes and writes a JUnit report
  (`target/nextest/ci/junit.xml`, renamed `junit-<n>.xml` after each run
  by `cargo xtask affected --run`), which CI keeps: the slowest tests are
  visible there.
- **The `critical` profile** runs the critical tests alone (below).

One process per test means one wgpu device per test on lavapipe. The
GitHub runner has four cores, as many as nextest runs tests at once; no
test group caps the rendering tests yet. If a machine with many cores
runs out of memory, cap them with `--test-threads`, or add a test group
to the configuration.

## Selection

`cargo xtask affected` reads the files a change touches (against the
merge base with `origin/main`, the working tree and untracked files
included) and the workspace's dependency graph (`cargo metadata
--no-deps`, nothing is built), and says which tests and which CI jobs
the change needs, and why.

```sh
cargo xtask affected                      # the report: jobs, crates, skipped crates
cargo xtask affected --plan               # the cargo commands it would run
cargo xtask affected --run                # run them (what CI's test job runs)
cargo xtask affected --base upstream/main # another base
cargo xtask affected --files docs/kit.md  # a change given by hand
cargo xtask affected --full --run         # everything, as on main
```

The rules, per changed file:

| Changed | Tests that run |
|---|---|
| a crate's sources, manifest, build script, data (`crates/<crate>/...`) | every test of the crate, and of every workspace crate depending on it (dev-dependencies included), doctests too |
| a test binary's file (`crates/<crate>/tests/<binary>.rs`) | that binary alone, unless another test file includes it as a module |
| a snapshot (`crates/<crate>/tests/snapshots/...`) | the crate's tests, not its dependents |
| a crate's `README.md` | as its sources, and `egui-pop::readme_links` |
| documentation (`docs/**`, `*.md` at the root) | the guides' doctests (`consumer-smoke`), `egui-pop::readme_links`, `egui-pop-gallery::gallery_doc` and `hosts_doc` |
| `template/**` | `egui-pop::minimal` |
| `examples/**` (example kits) | `egui-pop-tokens::kits`, `egui-pop-gallery::gallery` |
| the workspace (`Cargo.toml`, `Cargo.lock`, `rust-toolchain*`, any root file but Markdown), `.github/`, `.config/`, `.cargo/`, `xtask/`, the test kit (`egui-pop-test`, `egui-pop-test-macros`), the token files (`crates/egui-pop-tokens/themes/`) | everything |
| any path no rule names | everything |

**When in doubt, run.** A missing rule costs time, never safety. Every
selection adds the [critical tests](#critical-tests) it does not already
hold (a docs-only change: those that need no GPU). Selection can still
miss a dependency (a file read at run time from another crate's folder),
so `main` runs everything, and a pull request labelled `ci:full` too.

What the selection runs, with `--run`: one `cargo nextest run` over the
whole crates (`-p` each, so only their tests are built), `cargo test
--doc` for their doctests, and one `cargo nextest run` per crate with
single binaries or critical tests (`--test` each, filtered with `-E`).
Arguments after `--` go to every nextest run (`-- --profile ci`).

## CI jobs

The jobs of `.github/workflows/ci.yml` are selected the same way. A
first job, `changes`, runs `cargo xtask affected --github`: it writes
which jobs the change needs as outputs, and the report (why, the crates
selected and skipped, the critical tests added) in the run's summary.

| Job | Runs when |
|---|---|
| `fmt` | a `.rs` file changed |
| `clippy` (and `cargo doc`) | a crate's file changed, other than a snapshot |
| `doc` (the guides' `cargo doc`) | documentation changed and `clippy` does not run |
| `test` | always: the selection above, critical tests at least |
| `deny` | a `Cargo.toml`, `Cargo.lock` or `deny.toml` changed |
| `wasm` | a crate the gallery builds from changed (its tests and examples aside) |
| `package` | a manifest changed, or a file was added to or removed from a crate |
| `semver` | a crate `egui-pop` builds from changed (its tests and examples aside) |

A change under `.github/`, `.config/`, `.cargo/`, `xtask/`, to the
workspace files, the test kit or the token files, or to any path no rule
names runs every job. So do every push to `main` and a pull request
labelled `ci:full`.

A skipped job reports nothing, and a required check that never reports
blocks a pull request: the last job, `CI`, needs every other one and
fails when one of them failed. It is the only check to require.

To run locally what the `test` job runs on a pull request:

```sh
git fetch origin
cargo xtask affected --run                # the selection against origin/main
cargo xtask affected --full --run         # what main runs
```

## Time budget

The target: a pull request's CI under 10 minutes on GitHub's standard
runner (four cores), a docs-only one under 5. Measured on 2026-10-20,
wall time of the whole workflow, with a warm cache:

| Change | Before (every job, `cargo test`) | After (selection, nextest) |
|---|---|---|
| everything (`main`, `ci:full`, CI files) | about 6 min (tests 5 min 45 s) | about 7 min 20 s (tests 6 min 40 s) |
| documentation only | about 6 min | about 4 min (tests 3 min 30 s; fmt, clippy, deny, wasm, package and semver skipped) |
| one crate's source (`egui-pop-views`) | about 6 min | about 6 min (6 crates of 20 tested, deny and package skipped) |

A full run is not faster yet: one test,
`egui-pop-gallery::gallery every_page_renders_under_every_theme_and_density`,
runs for more than 3 minutes and sets the pace; splitting it is the next
lever. The JUnit reports of each run (the `junit` artifact) list every
test's time.

## Layers

- **Model** (`egui-pop-model`): plain unit tests, no egui, milliseconds.
- **Widgets and applications**: egui_kittest harnesses, finding widgets
  by role and label (never by position), including the keyboard. The
  test kit (`egui-pop-test`) has harnesses for a whole application
  (`app`, `harness_for`), at a given scale (`app_at`, `SCALES`,
  `at_scales`, `set_native_scale`), and checks (`assert_all_named`,
  `assert_keymap_clean`, `assert_writes_declared`, `a11y_tree`).
- **Snapshots** (`crates/egui-pop-gallery/tests/snapshots`): the gallery's
  pages rendered with wgpu, per theme; thresholds per operating system in
  `kittest.toml`. Regenerate with `UPDATE_SNAPSHOTS=1 cargo test -p
  egui-pop-gallery` and look at the images in the diff.

## One GPU device per test process

Rendering tests run on a software Vulkan driver (lavapipe) in CI and
locally when it is installed. Creating or enumerating wgpu instances and
devices on parallel test threads crashes the Khronos Vulkan loader
1.4.341 (a use-after-free while debug labels are set; Fedora ships it):
the test binary dies with `SIGSEGV`, on any test, at random. It is fixed
upstream in loader 1.4.350 (Vulkan-Loader pull request 1866).

So a test process opens **one** wgpu device and every rendering harness
shares it:

```rust,ignore
use egui_pop_test::SharedGpu as _;

let h = egui_kittest::Harness::builder()
    .shared_wgpu() // instead of .wgpu()
    .build_ui(|ui| { ui.label("hi"); });
```

The shared instance asks for Vulkan alone when a Vulkan adapter exists
(lavapipe): an instance that also enumerates OpenGL makes an EGL context
current on whichever test thread asks, and a shared device would then
be used from another thread. Without Vulkan it falls back to wgpu's
default backends. `egui_pop_test::app_wgpu` uses the shared device. A test that must open a device of its
own holds `egui_pop_test::gpu_lock()` while it does. A harness built
without a renderer opens a device of its own the first time it renders
(kittest's lazy renderer): give it `.shared_wgpu()` when it renders.

Until the loader is updated everywhere, a crashing GPU test binary is
rerun with `--test-threads=1` before concluding anything. Under nextest
every test is a process of its own, so the parallel threads that trigger
the crash are rarer, and a test that crashes anyway is retried.

## Critical tests

Selective runs (below) skip what a change cannot affect. Some tests
guard rules that any change can break, so they run on every change,
whatever the selection says. They carry a mark from the test kit:

```rust,ignore
#[egui_pop_test::critical]
#[test]
fn every_interactive_node_has_a_name() { /* ... */ }
```

The mark changes nothing in how the test runs: it renames the function
with the prefix `critical_` (`critical_gpu_` with `#[critical(gpu)]`,
for a test that renders), and the runners select by name. Crates whose
tests do not use the test kit (`egui-pop-tokens`, `egui-pop-model`,
`egui-pop-core`, `egui-pop-i18n`) use `#[egui_pop_test_macros::critical]`,
the same attribute, through a path-only dev-dependency.

```sh
cargo nextest run --workspace --profile critical   # every critical test
cargo test --workspace -- critical_                # the same, plain cargo
```

Rules for the mark:

- **Critical tests stay fast.** They run on every pull request; one that
  becomes slow loses its mark or is split (nextest reports it as slow
  after 10 seconds).
- **The list is short and written down here**, each test with the rule
  it guards. A mark is a decision: added or removed in a commit that
  says why.
- **A docs-only change runs the critical tests that need no GPU.** The
  ones marked `gpu` wait for a code change, or for `main`.

| Test | Rule it guards |
|---|---|
| `egui-pop::layering` `critical_dependencies_point_down_the_layers` | crate layers, ADR 0001 |
| `egui-pop-tokens::themes` `critical_built_in_themes_answer_the_whole_contract_cleanly` | every theme gives every required token |
| `egui-pop-tokens::themes` `critical_system_and_component_tokens_are_aliases_of_the_tier_below` | component tokens alias the tier below |
| `egui-pop-tokens::contrast` `critical_every_built_in_theme_and_density_passes_its_contrast_pairs` | contrast of every built-in theme and density |
| `egui-pop-tokens::kits` `critical_the_default_kit_is_a_kit_folder` | the default kit is a clean kit folder |
| `egui-pop-i18n::catalogues` `critical_the_english_catalogue_is_generated_from_the_code` | no text missing from the English catalogue |
| `egui-pop-i18n::catalogues` `critical_every_locale_translates_everything_and_nothing_else` | no missing key, no unstamped translation |
| `egui-pop-core::core` `critical_documents_are_written_atomically_with_a_backup_and_migrated_on_load` | an old saved document still loads |
| `egui-pop-model::shell` `critical_a_version_1_layout_is_migrated_to_start_and_end_areas` | an old layout still loads |
| `egui-pop-gallery::gallery` `critical_gpu_every_interactive_node_of_every_page_has_a_name` (GPU) | every interactive node of the gallery has a name (over 30 s on CI: to split, or the mark goes) |
