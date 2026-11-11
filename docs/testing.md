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

## The next branch

Pull requests target `next`, a buffer branch between them and `main`:

- a pull request into `next` runs the quick checks only (format, clippy,
  the guides' documentation, licences, changesets and title), in minutes;
  it merges as soon as they pass;
- every push to `next` runs everything; a newer push cancels the running
  run, so a batch of merges is checked once;
- once that run passes, `main` fast-forwards to the commit it checked
  (`.github/workflows/promote.yml`), and the site is deployed from `main`.
  `main` only ever holds commits that passed everything;
- when `next` fails, the merge that broke it is fixed on `next` or
  reverted (its pull request reopened); `main` waits until `next` is green.

Releases, publishing and tags happen from `main`. A release pull request
goes through `next` like any other.

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
| the docs site (`book.toml`, `tools/book/**`) | none (the `book` job checks it) |
| `examples/**` (example kits) | `egui-pop-tokens::kits`, `egui-pop-gallery::gallery` |
| the workspace (`Cargo.toml`, `Cargo.lock`, `rust-toolchain*`, any root file but Markdown and `book.toml`), `.github/`, `.config/`, `.cargo/`, `xtask/`, the test kit (`egui-pop-test`, `egui-pop-test-macros`), the token files (`crates/egui-pop-tokens/themes/`) | everything |
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
| `book` (the docs site: `mdbook build`, `tools/book/summary.py`, `tools/book/links.py`) | a file under `docs/`, `book.toml` or `tools/book/` changed |
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

## Other platforms

`.github/workflows/platforms.yml` runs the workspace on Windows and macOS,
on every push to `next` and by hand (`workflow_dispatch`). A run is never
cancelled: a newer push waits for the running one. It is **not required**: it is
a workflow of its own, so `main`'s promotion, which waits on CI, does not
wait on it, and its jobs are `continue-on-error`. A red job there is a
platform bug to look at, not a broken `next`.

| | Windows (`windows-latest`) | macOS (`macos-latest`, Apple silicon) |
|---|---|---|
| Build (`cargo build --workspace --all-targets`) | yes | yes |
| Tests (`cargo nextest run --workspace`) | yes, rendering on WARP (DirectX 12 in software, `WGPU_BACKEND=dx12`) | yes, rendering on Metal (the virtual machine's paravirtualised GPU, `WGPU_BACKEND=metal`) |
| Snapshots | compared with the thresholds of `kittest.toml`'s `[windows]` | compared with `[mac]` |
| Real window | `tools/window-smoke.py` | `tools/window-smoke.py` |

The window's smoke run starts the debug gallery with a scripted run:
`--quit-after 240 --detach gallery.notes --resize-to 800x600`. The
gallery opens its window, detaches the notes panel into a second one,
asks for a new size halfway, and closes by itself; the script checks its
JSON lines (`ready` with the panel detached, `resize`, `quit` after every
frame; the first line of each counts) and its exit code, within four minutes. The size the window had
when it closed is printed, not checked: a window manager may refuse it.

Each job's summary says whether the tests and the smoke run passed; the
JUnit report and any snapshot differences are kept as artifacts.

Known failures, from the first runs:

- none recorded yet.

## Time budget

The budget (decided 2026-10-20): a pull request's CI under 10 minutes on
GitHub's standard runner (four cores), a docs-only one under 5. Measured on 2026-10-20,
wall time of the whole workflow, with a warm cache:

| Change | Before (every job, `cargo test`) | After (selection, nextest) |
|---|---|---|
| everything (`main`, `ci:full`, CI files) | about 6 min (tests 5 min 45 s) | about 7 min 20 s (tests 6 min 40 s) |
| documentation only | about 6 min | about 4 min (tests 3 min 30 s; fmt, clippy, deny, wasm, package and semver skipped) |
| one crate's source (`egui-pop-views`) | about 6 min | about 6 min (6 crates of 20 tested, deny and package skipped) |

A full run was not faster: one test, rendering every gallery page under
every theme and density, ran for nearly 4 minutes on lavapipe and set the
pace (and reached the CI timeout as pages were added). It is split in
`egui-pop-gallery::gallery` into a test per theme and density
(`every_page_renders_in_<theme>_<density>`), run side by side; a further
test checks every theme and density has its own. The JUnit reports of
each run (the `junit` artifact) list every test's time.

## Layers

- **Model** (`egui-pop-model`): plain unit tests, no egui, milliseconds.
- **Widgets and applications**: egui_kittest harnesses, finding widgets
  by role and label (never by position), including the keyboard. The
  test kit (`egui-pop-test`) has harnesses for a whole application
  (`app`, `harness_for`), at a given scale (`app_at`, `SCALES`,
  `at_scales`, `set_native_scale`), and checks (`assert_all_named`,
  `assert_keymap_clean`, `assert_writes_declared`, `a11y_tree`).
- **Keyboard walks** ([below](#keyboard-walks)): real key presses, and
  what holds the focus after each.
- **Snapshots** (`crates/egui-pop-gallery/tests/snapshots`): the gallery's
  pages rendered with wgpu, per theme; thresholds per operating system in
  `kittest.toml`. Regenerate with `UPDATE_SNAPSHOTS=1 cargo test -p
  egui-pop-gallery` and look at the images in the diff. A page's shot
  veils the side bar's page list (`Nav::veil_list`): the list is laid
  out and answers as usual, but it is drawn over with the side bar's
  fill, so a page added to the gallery changes only the list's own
  shots (`pages-*`, which lift the veil), not every page's.

## Keyboard walks

Putting the focus on a widget through AccessKit (`.focus()`) shows what
the widget does once focused, not that the keyboard gets there. A walk
(`egui_pop_test::walk`) only presses keys, through the harness, and looks
at egui's focus after each:

```rust,ignore
use egui_pop_test::walk::{self, End, ESCAPE, SHIFT_TAB, TAB};

let walked = walk::walk(&mut h, TAB, 200); // until it loops or is lost
assert_eq!(walked.end, End::Looped { at: 0 });
assert!(walked.stops.iter().all(|s| s.visible), "{walked}");

let stop = walk::reach(&mut h, SHIFT_TAB, "Save", 40).expect("the keyboard gets there");
assert_eq!(walk::press(&mut h, ESCAPE), None); // nothing holds the focus
```

- **`press`** presses one key (`TAB`, `SHIFT_TAB`, `F6`, `SHIFT_F6`,
  `ESCAPE`, or any `Press`), runs until the application settles and
  returns the `Stop` holding the focus, if any; **`focused`** only looks.
- **`Stop`** says what holds it: `role`, `name`, `rect`, `visible` (any of
  it on screen, not scrolled or clipped away; a layer drawn over it does
  not count) and egui's `id`. A stop with no role and no name is a focusable
  widget that declares nothing.
- **`walk`** presses one key until the focus comes back to a stop seen, or
  nothing holds it, or a limit: `End::Looped { at }`, `End::Lost`,
  `End::Limit`. A window has no outside: egui wraps Tab, so a walk ends
  looped unless a key drops the focus. `at` being the last stop means the
  key does not move it (a terminal keeps Tab).
- **`hold`** puts a key down and leaves it there, each further call being
  a repeat, and **`release`** lets go: what a held key does, where `press`
  goes down and up in one frame.
- **`reach`** presses a key until a stop of that name holds the focus: the
  way to a widget in a test about the keyboard.

`crates/egui-pop-gallery/tests/keyboard_walk.rs` walks the gallery for the
[keyboard survey](keyboard-survey.md): what holds is pinned, and a gap is an
ignored test asserting what the survey asks, its reason naming the survey's
item, so that

```sh
cargo test -p egui-pop-gallery --test keyboard_walk -- --ignored
```

lists what is still open, each failing. The change closing an item removes
its `ignore`. A page's stops come last in the window's order, so these tests
get there with Shift+Tab: each press is a few passes of the whole gallery.

## Waiting on the application

A test that starts something asynchronous (a command whose host answers
later, a job) waits with `egui_pop_test::wait::wait_until`: it steps the
harness's frames until a journal event matches or the application
settles, the conditions of the bridge's `bridge.wait`
([Agents](agents.md#wait)), and fails past a deadline (120 frames or 10
seconds by default) with the condition and the journal's last events.

```rust,ignore
use egui_pop::api::wait::WaitFor;
use egui_pop_test::wait::{Deadline, settle, wait_until};

let since = h.state().handle().last_event();
// ... start something ...
wait_until(&mut h, &WaitFor::event("command.done"), since, Deadline::default())?;
settle(&mut h, Deadline::default())?;
```

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

## Performance

The gallery's `perf` example measures its heavy pages headless:
`node-graph`, `viewport`, `file-browser`, `table`, `text-log` and
`sections`. Each page runs on a fresh context for 60 frames of warm-up and
300 measured frames, with a fixed clock (1/60 s a frame) and fixed input:
the pointer sweeps the window on a set path and the wheel scrolls down
then up every four seconds. Per page, one JSON line:

- `build_p50_ms`, `build_p95_ms`, `build_max_ms`: the frame's build
  (`run_ui`, egui-pop's logic and drawing); `tessellate_p50_ms`,
  `tessellate_p95_ms`: its tessellation;
- `shapes`, `vertices`: what the last frame draws;
- `rss_start_mb`, `rss_peak_mb`, `rss_end_mb`, `rss_growth_mb`: the
  process's resident memory after the warm-up, its peak while the page
  runs, after the last frame, and what the measured frames added (Linux,
  `/proc/self/status`; none elsewhere).

The counts (shapes, vertices) are the same on every run; the times and,
less so, the memory vary with the machine and its load. Rendering is not measured: there
is no GPU, only what the interface thread does before egui-wgpu.

```sh
cargo run --release -p egui-pop-gallery --example perf
cargo run --release -p egui-pop-gallery --example perf -- node-graph table
cargo run --release -p egui-pop-gallery --example perf -- --baseline last.jsonl
```

With `--baseline` (the JSON lines of an earlier run), a figure that grew
beyond its threshold is reported as a `warning:` line; the exit code stays
0. The thresholds, in percent: build p50 25, build p95 40, tessellation
p50 25, vertices 10, resident peak 15, resident growth 50 (a figure under
a tenth of a millisecond or a megabyte is noise, and is not compared).

The gate runs on the maintainers' bench box, the machine alone (a timing
job, `egui-pop-perf`): every full run is kept per branch, and compared
with the last run of `next`, which is the baseline. It is run on `next`
after merges that may move the figures, and on a branch before merging
one that may. It warns; it never blocks a merge.

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
