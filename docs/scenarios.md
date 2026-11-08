# Scenarios

A scenario is a test written as data: a RON file of steps that call the
application's capabilities, act on its interface, wait and check. The
test kit replays it headless, in an egui_kittest harness, and checks a
set of **invariants** after every step, so a scenario catches more than
it asserts. Agents write scenarios from what they did in a running
application; from then on they are regression tests. Discovery and
waiting come from the bridge ([Agents](agents.md)).

## Running them

```rust,ignore
// tests/scenarios.rs
#[test]
fn scenarios() {
    egui_pop_test::scenario::run_dir(|| my_app::builder(), "tests/scenarios");
}
```

`run_dir` builds a fresh application for each `*.ron` file of the folder
(not its subfolders), runs it, and panics with every failure's report.
For more, `egui_pop_test::scenario::Runner`:

| Call | What it does |
|---|---|
| `Runner::new(build)` | a runner building each scenario's application with `build` |
| `.size(v)`, `.frame_budget(d)`, `.deadline(d)` | the window (1100 × 720), the longest frame (2 s), how long a `Wait` waits (120 frames or 10 s) |
| `.invariant(name, check)` | an application's own invariant, a predicate on the harness, the step and its journal events |
| `.snapshots(true)` | renders with wgpu (feature `wgpu`): a `Shot` is a snapshot image |
| `.run(&scenario)`, `.run_file(path)` | one scenario: `Ok(Passed)` or `Err(Failure)` |
| `.run_dir(dir)`, `.assert_dir(dir)` | a folder, its results, or a panic on any failure |
| `Runner::load(path)`, `scenario::parse(text)` | reads a scenario |

## The format

```ron
Scenario(
    name: "toasts",
    description: "A person asks for toasts.",
    fixtures: [Fixture("test.library", { "photos": 3 })],
    allow: ["NoErrorToast"],
    steps: [
        Call("gallery.page", { "page": "toasts" }),
        Wait(Event("command.done")),
        Ui(Click(role: "Button", label: "Information")),
        Expect(Value("toasts.shown.0.level", "info")),
        Wait(Settled),
        Shot("after"),
    ],
)
```

Arguments are JSON values written in RON: `{ "key": value }` maps,
lists, strings, numbers, `true`, `false`, and `()` for none. Fields
that may be left out are written bare (`label: "Save"`, not
`Some("Save")`).

**`fixtures`** are `test.*` capabilities called before the steps, to set
up the world without Rust code (a library, a folder of photos, a fake
card). An application registers them as collector actions or commands
in its test builds, under a feature of its own.

**`allow`** lifts invariants by name, for a scenario that expects what
they forbid.

### Steps

| Step | What it does |
|---|---|
| `Call(name, args)` | calls a capability that `bridge.describe` lists, as the agent: a collector action (`journal.since`) or a command (`about.read`, run at once) |
| `Wait(Settled)` | steps frames until no command is queued or running, every job is idle and no repaint is asked for; then checks `StableWindows` |
| `Wait(Event(name))` | steps frames until a journal event of that name follows the last `Call` or `Ui` step |
| `Wait(Any([..]))` | the first of several |
| `Ui(Click(label: .., role: ..))` | clicks a node of the accessibility tree, by label, role (`Button`, `Window`, `CheckBox`..., any case) or both; it must be the only one |
| `Ui(Type(label: .., role: .., text: ".."))` | focuses a node and types into it |
| `Ui(Key("Ctrl+K"))` | presses a key, with `Ctrl`, `Shift`, `Alt`, `Cmd` |
| `Ui(Scroll(label: .., role: .., up: true))` | scrolls a node, down unless `up` |
| `Expect(NoError)` | no error toast and no failed command since the start, whatever `allow` lifts |
| `Expect(Tree(has: (..), lacks: (..)))` | the tree has, or lacks, a node |
| `Expect(Value("collector.key.0", v))` | a collector's snapshot value (`AppHandle::debug_snapshot`); the collector's name may hold dots |
| `Expect(Journal(has: "name", data: {..}))` | an event since the start, its data holding the keys given |
| `Shot(name)` | a snapshot image `<scenario>-<name>` when the runner renders, else the accessibility tree, kept in `Passed::trees` |

## Invariants

Checked after every step, in this order, unless `allow` lifts them:

| Invariant | Breaks when |
|---|---|
| `NoErrorToast` | an error toast shows |
| `NoFailedCommand` | `command.failed` is on the journal during the step |
| `NoUnknownCommand` | a `Call` names a capability, or `commands.run` a command, that is not registered |
| `NoPanic` | the step panics |
| `StableWindows` | an area (window, popup, modal) moves or resizes in the frames after a `Wait(Settled)` |
| `FrameBudget` | a frame takes longer than the budget (generous: a guard against loops) |
| `NoMissingText` | the `i18n` collector reports a key shown untranslated during the step in a language other than English (a key without an English entry shows its English text; keys already untranslated before the first step are left out) |

An application adds its own with `Runner::invariant`.

## A failure

A failure names the scenario, the step as written, what broke (an
invariant, an expectation, a call refused, a wait past its deadline...),
why, the journal's last events and the accessibility tree:

```text
scenario `unknown-page` failed
  step 2: Call("gallery.page", {"page":"no-such-page"})
  broken: invariant NoFailedCommand
  why: command.failed: [...]
  journal (last 12):
    #41 command.dispatched [agent:debug-bridge] {...}
    #42 command.failed [...] {...}
  tree:
    Window "egui-pop gallery"
    ...
```

The gallery's scenarios are in `crates/egui-pop-gallery/tests/scenarios`;
the broken ones, under `broken/`, fail on purpose and
`tests/scenarios.rs` checks their reports.

## Live runs

The same files run against a running application over its [agent
socket](agent-socket.md), as an agent drives it, with
`egui_pop_test::scenario::live::LiveRunner` (Unix) or its command line:

```console
$ egui-pop-scenario --socket "$XDG_RUNTIME_DIR/egui-pop-agent-my-app.sock" \
    --var folder=/tmp/photos --out /tmp/out \
    --shot my.screenshot '{"path": "$path"}' tests/scenarios
```

- **Steps.** `Call`, `Wait`, `Expect(NoError)`, `Expect(Value)`,
  `Expect(Journal)` and `Shot` work as in-process. `Ui` and
  `Expect(Tree)` need the accessibility tree: they fail live, until the
  `ui.*` side over eframe's `inspection` exists.
- **Variables.** `--var name=value` (`LiveRunner::var`) replaces `$name`
  in every string argument of the steps and fixtures: a folder the run
  made, a file.
- **Invariants.** `NoErrorToast`, `NoFailedCommand`, `NoUnknownCommand`
  and `NoMissingText` as in-process; `NoPanic` breaks when the
  application is gone (it crashed or exited). `StableWindows` and
  `FrameBudget` need the frames: in-process only.
- **Shots.** A shot keeps every collector's snapshot as
  `<scenario>-<name>.json` in `--out`. With `--shot CAPABILITY ARGS`
  (`LiveRunner::shot`), it also calls the application's own screenshot
  capability, `$path` in its arguments replaced by
  `<scenario>-<name>.png`, and waits for the file.
- **Output.** One JSON line per scenario on stdout; with `--out`, each
  passed scenario's journal as `<scenario>-journal.json` and a
  failure's report as `<scenario>-failure.txt`. The exit status is 1
  when one fails.

The application must let the agent write (`BridgeMode::On`): a socket
turned on without a mode is read-only, and a scenario that changes
anything is refused.

## Not yet

- `Ui` steps live, and the recording of an agent's session as a
  scenario.
