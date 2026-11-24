# Testing

How egui-pop is tested, and what to know when a test misbehaves.

## Goal: time that does not grow with the tests

The suite's wall time must not grow linearly with the number of tests.
Two levers:

- **Parallel runs**: every test binary runs side by side, every test in a
  process of its own (cargo-nextest, below);
- **Selective runs**: a change runs the tests it can affect, not the
  whole suite ([Selection](#selection)); `main` always runs everything.

The integration tests are split into small binaries
(`crates/*/tests/*.rs`, one per feature) where a binary links little: a
binary is a unit of selection, and editing one test file relinks one
small binary. Where every binary links the whole application stack, the
links cost more than the selection saves: those crates have one test
executable, `it`, and each former binary is a module of it.

### One test executable: the gallery and the application layer

`egui-pop-gallery`'s and `egui-pop-api`'s integration tests are each one
executable, `it` (`crates/<crate>/tests/it/main.rs`, one module per
feature: `gallery`, `keyboard_walk`, `mounts`... in the gallery,
`windows`, `dialogs`, `files`... in `egui-pop-api`). Each former binary
linked egui, eframe and the whole application layer: fifteen copies of
the stack for the gallery, 115 for `egui-pop-api` (135 MB each, 15 GB in
all), and as many links per build. A module is still a unit of selection
([Selection](#selection) names it `it::mounts` and runs it alone by a
test name filter); editing one relinks the one executable. Helpers
shared by several modules live in `tests/it/support/`, a module of the
executable declared in its `main.rs`.

```sh
cargo nextest run -p egui-pop-gallery --test it -E 'test(/^mounts::/)'   # one module
cargo test -p egui-pop-gallery --test it keyboard_walk:: -- --ignored     # the same, cargo test
cargo test -p egui-pop-api --test it windows::                            # one module of egui-pop-api
```

The test names carry their module (`gallery::snapshots_per_theme`).
Under nextest every test still has a process of its own. Under plain
`cargo test`, every module's tests now share one process, where before
each former binary had its own: a test leaning on process-wide state
(an environment variable, a global installed once) may meet one from
another module. A global installed once goes in `tests/it/support/` and
is shared (the log capture of `egui-pop-api`'s `removal` and
`window_invariants`: `log::set_logger` succeeds once per process).
Every gallery module passed in one `cargo test` process when the change
was made (the `egui-pop-test-1p` box job, `test=it`).

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
  visible there, and `cargo xtask shards refresh` reads them to split the
  shards ([Shards by duration](#shards-by-duration)).
- **The `critical` profile** runs the critical tests alone (below).

One process per test means one wgpu device per test on lavapipe. The
GitHub runner has four cores, as many as nextest runs tests at once; no
test group caps the rendering tests yet. If a machine with many cores
runs out of memory, cap them with `--test-threads`, or add a test group
to the configuration.

## The next branch

Pull requests target `next`, a buffer branch between them and `main`:

- a contributor's pull request into `next` (from a fork) runs the quick
  checks only (format, clippy, the guides' documentation, licences,
  changesets and title), in minutes; it merges as soon as they pass;
- the maintainers' own pull requests (branches of this repository) are
  tested on the bench box, and on GitHub run one check, `pr-lint`
  (`.github/workflows/pr-lint.yml`), on every push, in a few minutes and
  without building a test: the title, `cargo fmt --check`, the changeset
  check, `cargo xtask ui-index --check`, `cargo deny check` and `cargo doc
  --workspace --no-deps --locked` without a warning. **Such a pull
  request merges when its box jobs are green and its `pr-lint` check is
  green** (since 2026-11-15). Look at the check once, when the box jobs
  are done: `gh pr checks <n> --watch --fail-fast` (it returns when the
  checks end), or a single `gh pr checks <n>`; nothing else of CI is
  watched. **The box jobs of a branch** are narrow: the `egui-pop-test`
  runs of the crates and test targets the change touches, plus
  `egui-pop-snapshots` when something visible or listed changes (a page
  drawn differently, a page or a story added, a snapshot's subject
  moved); `next`'s CI runs the rest. The box's `egui-pop-gate` job (fmt,
  clippy, every test, the doctests and the documentation build) is for
  releases or a by-hand check, not for each branch; it runs every test
  and every step even after a failure, so one run shows all that fails;
- every push to `next` runs everything; a run is never cancelled: a
  newer push waits behind the running one (GitHub keeps only the newest
  waiting), so a batch of merges is checked once and `main` is promoted
  even while merges keep coming;
- once that run passes, `main` fast-forwards to the commit it checked
  (`.github/workflows/promote.yml`), and the site is deployed from `main`.
  `main` only ever holds commits that passed everything;
- when `next` fails, the merge that broke it is fixed on `next` or
  reverted (its pull request reopened); `main` waits until `next` is green.

Releases, publishing and tags happen from `main`. A release pull request
goes through `next` like any other.

## When Ubuntu's archive is down

GitHub's Ubuntu runners install packages from Ubuntu's archive
(`archive.ubuntu.com`, its Azure mirror, `security.ubuntu.com`). When
Canonical has an outage, every `apt-get` step fails ("Temporary failure
resolving", "Unable to fetch some archives") through no fault of the
code, `next` stays red and `main` is not promoted. One repository
variable, `UBUNTU_ARCHIVE_DOWN`, tells the workflows:

```sh
gh variable set UBUNTU_ARCHIVE_DOWN --body true -R quazardous/egui-pop   # on
gh variable delete UBUNTU_ARCHIVE_DOWN -R quazardous/egui-pop            # off
```

While it is `true`, what can only work with the archive is skipped, not
failed, what has a fallback without it takes the fallback, and every run
says so: a warning and a line in the summary, from the job "Ubuntu's
archive down" and from the steps that changed.

| Workflow | Job or step | With the switch on |
|---|---|---|
| CI | Tests (lavapipe 1/3 to 3/3) | lavapipe from a pinned Mesa build of gfx-rs/ci-build (checked by SHA-256, `.github/actions/software-vulkan`), on `ubuntu-26.04`, whose LLVM it links; a cache of its own |
| CI | Viewer's video (GStreamer) | skipped |
| CI | Viewer's web view (WebKitGTK) | skipped |
| CI | Semver checks | no GStreamer or WebKitGTK packages: every crate as usual but `egui-pop` and `egui-pop-views`, checked with all their features but the video and webview ones |
| Gallery and docs on the web | Build the API documentation | every feature but the video and webview ones |

Unset or `false`, the workflows behave as they always did. A skipped job
counts as passed, for the `CI` job and for the run's result, so a green
run on `next` still promotes `main`. The Platforms workflow (Windows,
macOS) uses no apt and is not affected.

The Linux tests still run on GitHub (lavapipe from the fallback Mesa);
no full gate is run on the bench box in their place. The video and
webview builds wait until the switch is off, or run where GStreamer's
and WebKitGTK's development
packages are installed. Delete the variable once `apt-get update` works
on the runners again.

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
| a module beside a binary's `main.rs` (`crates/egui-pop-gallery/tests/it/mounts.rs`, `crates/egui-pop-api/tests/it/windows.rs`) | that module alone (`it::mounts`, a test name filter on the binary `it`) |
| a snapshot (`crates/<crate>/tests/snapshots/...`) | the crate's tests, not its dependents |
| a crate's `README.md` | as its sources, and `egui-pop::readme_links` |
| documentation (`docs/**`, `*.md` at the root) | the guides' doctests (`consumer-smoke`), `egui-pop::readme_links`, `egui-pop-gallery::it::gallery_doc` and `it::hosts_doc` |
| the docs site (`book.toml`, `tools/book/**`) | none (the `book` job checks it) |
| `examples/**` (example kits) | `egui-pop-tokens::kits`, `egui-pop-gallery::it::gallery` |
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
| `alone` (`tools/check-alone.sh`: every published crate's `cargo check -p`, with and without its default features, outside the workspace's feature unification) | as `clippy` |
| `doc` (the guides' `cargo doc`) | documentation changed and `clippy` does not run |
| `book` (the docs site: `mdbook build`, `tools/book/summary.py`, `tools/book/links.py`) | a file under `docs/`, `book.toml` or `tools/book/` changed |
| `test` | always: the selection above, critical tests at least, in three shards ([by duration](#shards-by-duration)) |
| `deny` | a `Cargo.toml`, `Cargo.lock` or `deny.toml` changed |
| `wasm` | a crate the gallery builds from changed (its tests and examples aside) |
| `package` | a manifest changed, or a file was added to or removed from a crate |
| `semver` | a crate `egui-pop` builds from changed (its tests and examples aside), or a changeset |

A change under `.github/`, `.config/`, `.cargo/`, `xtask/`, to the
workspace files, the test kit or the token files, or to any path no rule
names runs every job. So do every push to `main` and a pull request
labelled `ci:full`.

A push to `next` runs every job too, but `semver`: it runs when what
changed since `main` (the last commit that passed every job) selects it
by the rule above. Comparing with `main` rather than with the previous
push keeps the commits of a run that never finished (a newer push
waiting replaces an older one). The job is its own workflow,
`.github/workflows/semver.yml`, that `ci.yml` calls; it also runs every
night on `next` (03:17 UTC) when `next` moved since the last nightly
run, and by hand (`workflow_dispatch`). A failed nightly run is red on
the Actions page, like CI. The nightly run starts with a `gate` job that
takes seconds (no checkout, no toolchain): it reads `next`'s head and
looks for the cache entry `semver-tested-<sha>`, which a nightly run
saves once it has checked that commit, pass or fail. Found, the checks
are skipped, the run stays green and its summary says `skipped: next
unchanged since <sha>`. A cache entry records which `next` was checked
(a scheduled run's own head is `main`'s). Any API or cache error, or an
evicted entry, runs the checks (the gate fails open). A run from
`ci.yml` or by hand always checks. `cargo xtask publish` runs the checks again before uploading
anything ([Changesets and releases](releasing.md#semver-checks)).

A skipped job reports nothing, and a required check that never reports
blocks a pull request: the last job, `CI`, needs every other one and
fails when one of them failed. It is the only check to require.

### Shards by duration

The `test` job runs in three shards side by side (Tests (lavapipe 1/3 to
3/3)), split by each test's measured time rather than by count or name,
so the slowest shard stays close to the average:

- `ci/test-durations.json` holds every test's time on CI, per nextest
  binary id, taken from the `junit-*` artifacts of full runs on `next`.
- `cargo xtask shards` computes the split from that file alone, so the
  three jobs compute the same one: the tests are grouped into units (a
  whole binary; a heavy binary cut into its modules, and a heavy module
  into its tests, until a unit weighs less than a twelfth of a shard),
  and the units go, longest first, to the lightest shard. Shard 1 starts
  with `first_shard_seconds` of its own (the doctests, the icon set's SVG
  tests and the shard check, which run there alone), counted over
  `threads` (the runner's four cores).
- The lightest shard at the end runs every test the two others do not
  (`not (...)`): a test the file does not know yet runs there, and no
  test can be dropped. On a full run, shard 1 checks it anyway: `cargo
  xtask shards check` lists each shard's tests with nextest and fails
  unless every test of the workspace is in exactly one shard.
- `cargo xtask affected --run --shard I/N` runs shard I: each nextest run
  is narrowed by the shard's filter (joined with its own `-E`), the
  doctests run on shard 1 only.

```sh
cargo xtask shards                        # the split: each shard's expected time and filter
cargo xtask shards --filter 2             # shard 2's nextest filter alone
cargo xtask shards refresh --run RUN_ID   # rewrite the durations from a CI run's JUnit reports
cargo xtask shards refresh DIR...         # or from reports on disk (target/nextest/ci/)
```

Refresh the file by hand when a shard drifts from the others (each run's
summary shows the shards' times), from one or more green full runs of
`next` (`gh run list --branch next --workflow ci.yml`): a test in several
runs gets its mean time. Nothing refreshes it on a schedule yet. A test
added since only lands on the rest shard, so the split degrades slowly,
never by losing a test. Three shards, because each one pays about seven
minutes before its first test (the runner's setup and the build), against
about five of tests: a fourth shard would save about a minute of wall time
for seven more runner minutes.

Measured on 2026-11-19 (two full runs of `next`): with nextest's hash
partition the shards' test steps took 4 min 10 s to 6 min (plus the
doctests on every shard, 40 s each); the durations split expects about
5 min 10 s for each, the doctests and the check counted on shard 1.

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
`--quit-after 360 --detach gallery.notes --page workbench
--focus-detached --type "smoke 42" --resize-to 800x600`. The gallery
opens its window and detaches the notes panel into a second one. A
quarter of the way through it asks the system to focus that window and
gives the window's field the keys (the field shows only in this run),
then types the text into that window's input once the window says it
has the focus; it asks for a new size halfway and closes by itself.
The script checks its JSON lines (the first line of each counts) and
prints one line per check:

| Check | What it proves |
|---|---|
| detach | `ready` says the panel is in its own window |
| focus | the detached window got the system's focus (`focus`) |
| keys | the text typed into that window reached its field (`windows`, `field`): egui-pop's per-window key routing ([upstream.md](upstream.md), the focus plugin's workaround) |
| scale | each window has its own pixels per point, the system's times the zoom; `--scale main=1.5` checks a factor |
| resize | the new size was asked (`resize`) |
| quit | `quit` after every frame, exit code 0, within five minutes |

The size the window had when it closed is printed, not checked: a
window manager may refuse it. On GitHub's runners nothing outside the
gallery can type, so the gallery types into its own window's input; on
Linux the bench box's `egui-pop-window-smoke` job runs the same checks
in a headless labwc (`tools/wbox/window_smoke.py`), as a Wayland client
and as an X11 one through Xwayland, with the keys typed by the
compositor and a scale set by the system (the output's scale 2 changed
while the gallery runs on Wayland, `WINIT_X11_SCALE_FACTOR=1.5` on
X11). That job is a pre-release check, run by hand, not on CI
([releasing](releasing.md)).

Each job's summary says whether the tests and the smoke run passed, and
names the failed tests; a failed job also leaves an error annotation on
the run's page, since the run itself stays green. The JUnit report and
any snapshot differences are kept as artifacts.

Known failures and differences (2026-11-11): none fail. What is left
out, by platform:

- **Windows and macOS**: the gallery's `files-breadcrumbs-pop-dark` and
  `files-replace-pop-dark` snapshots are compared on Linux only. They
  show the demo folder's path, in the temporary folder: `/tmp` where the
  pictures are made, `C:\Users\…\Temp` on Windows, `/var/folders/…` on
  macOS. The tests' other checks run everywhere.
- **Windows**: the viewer test of an image address no parser can join
  (`//[bad…`) checks a drive's path written as is instead: `//[bad`
  before a drive makes no path at all, and the image fails unread.
- **Windows**: paths built by the file picker carry `\`, the tests'
  paths `/`; the tests compare paths, not text (`assert_path` in
  `egui-pop-api`'s `tests/it/files.rs`), and the root of `/top` is named
  `\`.

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
pace (and reached the CI timeout as pages were added). Every test that
walks the gallery's pages is now a test per page (since 2026-11-18): in
`egui-pop-gallery`'s `it`, `gallery::every_page_renders::<page>` (every
theme and density), `gallery::every_interactive_node_has_a_name::<page>`,
`gallery::every_input_brick_has_a_name::<page>`,
`gallery::every_page_ends_in_a_short_window::<page>`,
`gallery::every_playground_ends_in_a_short_window::<page>` and
`default_fonts::every_page_draws_with_eguis_own_fonts::<page>`. They run
side by side, none grows with the gallery, and a slow page shows by name.
The gallery's build script writes the page list as a macro
(`for_each_page!`, from `PAGES` in `src/lib.rs`) and the tests' `per_page!`
makes a module of tests from it: a page added to `PAGES` gets its tests,
and `gallery::every_page_has_its_generated_tests` fails if the list read
differs from `PAGES`. The JUnit reports of each run (the `junit`
artifact) list every test's time.

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
  egui-pop-gallery` and look at the images in the diff. Each is cut to
  what it tests ([below](#snapshots-cut-to-what-they-test)).

## Snapshots cut to what they test

A picture of the whole gallery window holds the top bar, the side bar
and the status bar around the page. Compared whole, every picture
changes when a bar does: a label added to the top bar would rewrite
every page's snapshot, and the diff would hide what the change did to
the pages. So no test of a page calls kittest's `snapshot`; the gallery's
tests cut the picture (`crates/egui-pop-gallery/tests/it/shot.rs`):

| Call | What the picture holds | For |
|---|---|---|
| `h.page_shot(name)`, `h.try_page_shot(name)` | the page's region: the main zone's block, heading included, and the playground beside it (`Nav::region`, recorded by the gallery's shell as it draws) | a page's own content: nearly every snapshot |
| `h.body_shot(name)`, `h.try_body_shot(name)` | everything under the top bar | a page that drives the side bar, the bottom area or the status bar (`workbench-*`, `arrangements-*`, `strips-*`, `contexts-*`, `jobs-*`) |
| `h.window_shot(name)`, `h.try_window_shot(name)` | the whole window | what fills the window or draws its frame (`admin-*`, `frame-own-*`, `frame-full-screen-*`), and `chrome-window-*` |
| `h.try_shot_of(name, rect)` | a rectangle the test names | the bars (`chrome-*`), the side bar with its list (`pages-*`) |

What floats over the window (a dialog, a file chooser, a menu, a tip, a
toast, a palette) widens a page's or a body's cut when it reaches out of
it, so the picture holds all of it; a sheet over the whole window (a
modal's dimming) does not. The cut is rounded to whole pixels and depends
on the layout alone, so a picture has the same size on every run.

The bars have a suite of their own, `gallery::the_bars_have_their_own_snapshots`:
`chrome-top-bar-*`, `chrome-side-bar-*` and `chrome-status-bar-*` per
theme and in the compact density, and `chrome-window-pop-dark`, the one
picture of a whole window. So:

- a change to the top bar changes the `chrome-top-bar-*` pictures and
  `chrome-window-pop-dark`, plus the few whole-window pictures above and
  any whose floating part reaches over the bar;
- a change to the side bar or the status bar changes its `chrome-*`
  pictures, the body shots, and the pictures a dialog widens over it;
- a change to a page changes that page's pictures only.

A shot that reaches over the side bar still veils the page list
(`Nav::veil_list`): the list is laid out and answers as usual, but it is
drawn over with the side bar's fill, so a page added to the gallery
changes only the list's own shots (`pages-*`, which lift the veil).

The Rerun-like example (`rerun-like-*`) is an application of its own,
without the gallery's bars: its pictures show its whole window.

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

`crates/egui-pop-gallery/tests/it/keyboard_walk.rs` walks the gallery for the
[keyboard survey](keyboard-survey.md): what holds is pinned, and a gap is an
ignored test asserting what the survey asks, its reason naming the survey's
item, so that

```sh
cargo test -p egui-pop-gallery --test it keyboard_walk:: -- --ignored
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
