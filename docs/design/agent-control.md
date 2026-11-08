# Agent control: transports, scenarios and invariants

Status: **design note, proposed** (2026-11-08). Built since: schemas
and `bridge.wait`, the in-process scenario runner, the socket and MCP
transports, and live scenario runs over the socket (`Ui` steps aside;
see [Scenarios](../scenarios.md)). It answers an application's need (Helios): agents drive a running
application, turn what they find into tests, and run those tests
without a person.

## Goal

An agent can:

1. drive an egui-pop application from outside its process: run its
   commands, read its state, see its screen, click and type;
2. write what it did as a **scenario**, a data file of steps and checks;
3. replay scenarios headless, as regression tests, with **invariants**
   checked at every step, so a scenario catches more than it asserts.

The framework is agnostic. The application codes against egui-pop's
bridge only. Transports, protocols and outside tools (egui's
`egui_inspection`, Rerun's `egui_mcp`, an MCP SDK) sit behind adapters
that can be swapped or dropped, the way egui_stack sits behind its host
contract. It grows by registering more, never by changing the core.

## 1. What exists

Most of the core is already here ([Agents and the debug
bridge](../agents.md)).

| Need | Exists | Where |
|---|---|---|
| A registry of capabilities | yes: collectors and their actions, and every command | `DebugBridge`, `debug_describe` |
| Discovery | yes, as JSON: names, descriptions, `mutates`, `agent`, an example of the arguments | `debug_describe`, the `commands` snapshot |
| Running as the agent | yes: `Origin::Agent`, the `agent` flags, the bridge modes (on, read-only, off), the `debug.call` step and its vetoes | ADR 0018 to 0024 |
| Reading state | yes: about 50 collectors' snapshots | `debug_snapshot` |
| The journal | yes: `journal.since`, `command.done` and `command.failed` with their cause | `journal` collector |
| Tree, screenshots, input in a live window | yes, outside the bridge: eframe's `inspection` feature and `egui_mcp` | [inspection.md](../inspection.md), the gallery only |
| Tree, screenshots, input in tests | yes: egui_kittest, `egui_pop_test::a11y_tree` | [Testing](../testing.md) |
| A headless real window | yes: wbox | `tools/wbox` |

**Missing:**
- a transport that carries the bridge out of the process;
- schemas for the arguments (today an example, free JSON or a schema);
- a blocking wait;
- the scenario format, its runner and its invariants;
- fixtures;
- an application-side switch (Helios has none of this wired).

## 2. The capability model

A capability is a bridge entry, either a collector action or a command,
described by:

| Field | Today | Proposed |
|---|---|---|
| `name` | `collector.action`, or a command `id` | unchanged |
| `description` | yes | unchanged |
| `mutates` | yes | unchanged |
| `agent` | yes | unchanged |
| `args` | an example, free JSON or a schema | **a JSON Schema when the author gives one**; the example stays as `example` |
| `result` | none | an optional JSON Schema |

`debug_describe` gains a `protocol` version, and the commands' entries
are listed beside the collectors' actions, as one list a client reads.
Adding a capability never breaks a client: a client calls only names
that `describe` lists.

**The generic capabilities**, by egui-pop:
- the existing collectors;
- `commands.run`;
- `journal.since`;
- a new `bridge.wait`. It replies when a journal event matches (`name`,
  `cause`), or when the jobs are idle and the frame settled, or at a
  deadline. It is the blocking wait the known gaps list as missing, held
  by the transport, never by the UI thread.

**An application's capabilities** are its commands and its collectors,
as today. Helios already declares its shell commands; it adds
collectors where an agent needs state (the library's counts, the import
window's sources, the jobs).

## 3. Transports, as adapters

One dispatcher, the bridge, does the checks once, for every transport:
- the mode;
- the `agent` flags;
- the `debug.call` step;
- the origin;
- the journal.

A transport only carries requests to the UI thread and replies back.

| Adapter | Carries | Crate or feature | State |
|---|---|---|---|
| In-process | Rust calls | the bridge itself | exists |
| Local socket | JSON-RPC 2.0: `describe`, `snapshot`, `call`, `wait` | `egui-pop-agent` (new, opt-in) | built |
| MCP | the same, as MCP tools generated from `describe`: a new capability becomes a tool with no code | `egui-pop-agent`, feature `mcp`, on an MCP SDK (below) | built: `egui-pop-agent-mcp` |
| UI: tree, screenshot, input | egui's `egui_inspection` protocol | eframe's `inspection` feature, as the gallery does | exists; to document as the `ui.*` side |

The UI side stays egui's protocol, with `egui_mcp` or our own client. An
agent holds two connections: the bridge for meaning, `inspection` for
pixels and pointer. A later step may join them in one MCP server, which
proxies `inspection`'s requests as `ui.*` tools. Either way the
application depends on neither tool: both are features it can leave off.

**Survey for the MCP side**, verdicts in egui-pop's usual words:

| Crate | Notes | Verdict |
|---|---|---|
| `rmcp` | the official Rust SDK (modelcontextprotocol/rust-sdk), 3.9, about 46 M downloads, active | **adopt**, behind the `mcp` feature |
| `rust-mcp-sdk` | 2.0, active, smaller | no: `rmcp` is the reference |
| `mcp-attr`, `mcpr` | small, quiet since 2025 | no |
| `egui_mcp` (Rerun) | an MCP server for the `inspection` protocol | **adopt as a client tool**, not a dependency |
| `egui-mcp` (dijdzv) | Linux only, AT-SPI over D-Bus, an in-app client on Tokio; text and combo items do not work | no |
| `egui-mcp` (aydiler) | an E2E bridge crate, 0 stars | no |

**Security.** Nothing listens unless the application turns a transport
on, and the user or a test asks for it (a flag, an environment
variable). The socket is a Unix socket in the user's runtime directory
(mode 0600), or a named pipe on Windows, not TCP. The bridge mode
applies as today: read-only is a sound default for a transport the user
did not ask for.

## 4. Scenarios

A scenario is a RON file of steps. Each step is a capability call or a
check:

```ron
Scenario(
    name: "import-without-library",
    fixtures: [Folder(jpeg: 3, png: 1)],
    steps: [
        Call("commands.run", (id: "library.importWindow", args: (source: "$folder"))),
        Wait(Settled),
        Ui(Click(label: "Import")),
        Wait(Event("command.done") | Event("command.failed")),
        Expect(NoError),
        Expect(Tree(has: (role: "window", label: "Import photos"))),
        Shot("after-import"),
    ],
)
```

- **Calls** are capability names from `describe`, nothing else, so the
  format grows with the registry.
- **Ui steps** (`Click`, `Type`, `Key`, `Scroll`, by label or role) go
  through egui_kittest in-process and through `inspection` live. They are
  the same step either way.
- **Expectations** read snapshots, the journal or the tree.
  `Expect(Value("collector.path", 12))` compares a snapshot's value.
- **Shots** are snapshot images, compared like the kittest snapshots,
  only where the scenario asks.
- **Fixtures** are capabilities too: `test.*` collectors that the
  application registers in test builds (a new library, a fake card, a
  folder of N JPEG and M raw files), so a scenario sets up its world
  without Rust code.

**One runner, two places:**
- **in-process**: a kittest harness in `egui-pop-test`, the application
  building its `App`; this is what CI and the box run;
- **live**: against a running instance over the socket and
  `inspection`, in wbox.

**Recording.** With a live session, the transport can write every call
an agent makes, with the replies, as a scenario. When an agent finds a
bug, it trims the recording to the steps that show it and saves it
under the application's `tests/scenarios/`. From then on it is a
regression test.

## 5. Invariants

Checked after every step, in both places, unless a scenario lifts one
by name (`allow: [ErrorToast]`):

| Invariant | Breaks when |
|---|---|
| `NoErrorToast` | an error notification shows, not expected by the scenario |
| `NoFailedCommand` | `command.failed` on the journal, not expected |
| `NoUnknownCommand` | a command run that is not registered (a "no command" refusal) |
| `NoPanic` | a panic, caught by the harness or the crash hook |
| `StableWindows` | a window or modal's rect still changes after `Settled` |
| `FrameBudget` | a frame over the budget (in-process: CPU time of the update; generous, a guard against loops) |
| `NoMissingText` | an i18n key shown untranslated |

An application adds its own: a predicate on snapshots, registered with
the runner.

## 6. Order of work

1. **Schemas and `bridge.wait`** in the bridge; `protocol` in
   `describe`; the commands' entries in the same list.
2. **The scenario runner, in-process**, with the invariants, in
   `egui-pop-test`; gallery scenarios as its tests.
3. **`egui-pop-agent`**: the socket transport, then MCP over `rmcp`
   generated from `describe`, then recording.
4. **The `ui.*` side live** over eframe's `inspection`: documented for
   any application, beyond the gallery.
5. **Applications**: Helios turns on the transport behind a flag, adds
   its fixtures and collectors, and writes its first scenarios.
   Exploration (an agent walking every command from `describe` with
   fixtures, reporting what breaks an invariant) comes after.

Step 2 is the first to pay off: in-process scenarios on CI give
autonomous tests before any live transport exists.

## 7. Open questions

1. RON or JSON for scenarios: RON reads better and matches the
   snapshots' tooling; JSON is what an MCP client emits when recording.
   The proposal is RON, with the recorder writing RON.
2. Whether `ui.*` steps should exist on the bridge too, as synthetic
   egui events from a collector action. That would give one connection,
   at the cost of duplicating `inspection`.
3. Where fixtures live: `test.*` collectors compiled only with a
   `test-fixtures` feature, or a separate registry the runner owns.
