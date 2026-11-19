# 0030: The bus is a contract the application supplies

Date: 2026-11-16. Status: accepted (2026-11-16, by the lead maintainer's
decision under GOVERNANCE, without the comment period). Slices 1 to 3
done; slice 3 (2026-11-19, egui-pop 0.17) took the contract's `Journal`
as it shipped in bus-contract 0.3 rather than a new trait: see its
entry below.

This ADR decides; the slices at the end are each their own change.

## Context

[ADR 0029](0029-not-on-the-bus.md) put egui-pop's window, popup, job and
plugin points on a bus: requests, events and named listeners in a
declared order, registered in a hook kernel (`egui-pop-hooks`) that
`egui-pop-api` creates for each application (`AppHandle::bus`).

That bus belongs to the library. The first application, Helios, already
runs buses of its own: the shell's journal, the node relay, the vault's
and the engine's (four sources it polls each pass), and is moving its
modules' extension points onto a hook kernel too. Each library owning
its bus means two sources of truth for one fact and bridges written by
hand between them, the very thing the bus was meant to end.

The maintainer's principle (2026-11-16): **each part offers contracts;
whether to run one bus or several belongs to the client, not to the
library.** The model is PHP's PSR-3 (logging) and PSR-14 (event
dispatching): a library describes a contract, the application installs
the implementation. Rust has the same shape in `log` (a trait and
macros; the application installs the logger), `tracing` (a
`Subscriber`), `raw-window-handle` and `embedded-hal`.

One difference from `log` and `tracing`: their implementation is a
process-global. A bus is not: several applications, or several buses,
in one process or one test must work (the test kit builds many
applications per test binary). So the bus is passed at construction,
never installed globally.

## Decision

### A contract crate: `bus-contract`, outside egui-pop

The contract is **its own crate in its own repository**,
[`bus-contract`](https://github.com/quazardous/bus-contract) (decided by
the maintainer, 2026-11-16), MIT OR Apache-2.0 like egui-pop, so any
Rust library or application can take it without taking egui-pop.
It has **no dependencies** (std only), no egui and no egui-pop
vocabulary: points, listeners, requests, events, alter, collect, the
order dump. It holds the contract and nothing that runs; egui-pop
depends on it like on any external crate:

- **`Point`**: a hook point is a type: its `ID` (dotted,
  `doc.close`), the `Input` every listener reads, the `Value` a
  listener edits, and its `KIND` (below). Moved from `egui-pop-hooks`
  unchanged, plus `KIND` with a default.
- **`PointKind`**: what a point is for, for introspection and review:
  - `Request`: a request (`<noun>.<verb>`, egui-pop's ADR 0029): its listeners do
    the work in a declared order; the value is what each did.
  - `Event`: an event's listeners, each event delivered once, in
    journal order.
  - `Alter`: one value passed through the listeners in turn.
  - `Collect`: each listener adds its own part.

  The kind is declared, not enforced: a request is an `alter` emit over
  a request's input, an event point an `alter` with a unit value.
- **`PointSpec`** (description, deadline), **`ListenOpts`**
  (owner, `after`, `before`, `last`, priority, deadline, threaded),
  **`Failure`**, **`Refused`**, **`HookError`**, **`Altered`**,
  **`Collected`**, **`PointInfo`**, **`ListenerInfo`**,
  `DEFAULT_DEADLINE`, `PLUGIN_DEADLINE`, `LAST`, `FAILED_FACT`: moved
  from `egui-pop-hooks` unchanged; the kind is added to `PointInfo`.
- **`Bus`**, the trait an implementation provides, object safe and
  `Send + Sync`, its values type-erased:

  | Method | Does |
  |---|---|
  | `declare_point(key, spec)` | Declares a point: its description, deadline and kind |
  | `listen_any(key, id, opts, run)` | Registers a listener under an id and an owner, ordered by `after` / `before` / `last`, then priority, then registration |
  | `unlisten(point, id)` | Removes one listener |
  | `forget(owner)` | Removes every listener of an owner, on every point ([ADR 0028](0028-removed-contributor-work-in-flight.md)) |
  | `alter_any(key, input, value, clone)` | Passes the value through the listeners in order (requests, events, alter points) |
  | `collect_any(key, input, fresh)` | Gives each listener a fresh value, gathers the parts |
  | `add_failure_sink(sink)` | Receives each dropped contribution (an error, a panic, a missed deadline) |
  | `points()`, `listeners(point)` | Introspection: points, listeners in run order, owners, constraints, missing targets, counters |

  A `PointKey` (`ID`, kind, `TypeId`, type name) names a point to the
  erased methods, so an implementation refuses an id registered with two
  types. `Arc<B>` is a `Bus` when `B` is, so `&Arc<dyn Bus>` goes where
  `&dyn Bus` is asked.
- **The typed calls** (`declare::<P>`, `listen::<P>`, `alter::<P>`,
  `collect::<P>`, `on_failure`) are methods on `dyn Bus` itself,
  written once in the contract crate over the erased ones. Code that holds an
  `Arc<dyn Bus>` calls `bus.listen::<WindowClose>(…)` exactly as it
  called the kernel; no implementation writes them again.
- **Tools on the contract**: `order_dump(bus, points)` and
  `missing_targets(bus, points)` move here and take `&dyn Bus`, so the
  reviewed order files, the test kit and the debug bridge's `bus`
  collector read any implementation the same way.
- **The contract table**: a library publishes its points as a constant,
  `pub const POINTS: &[PointDecl]`, each `PointDecl` built by a `const
  fn` from the point's type (`PointDecl::of::<WindowClose>(…)`: its id,
  kind, description, deadline and a function giving its key).
  `declare_all(bus, POINTS)` declares them. The application reads the
  table to know what it will host before building anything; the
  listeners a library registers are its other half, read after
  installation with `order_dump` and kept as reviewed files
  (in egui-pop, `crates/egui-pop-api/tests/bus/*.txt`).

What the contract does not hold, and why:

- **No journal yet.** Events are the host's journal
  (`AppHandle::publish`, `AppHandle::events_since`): facts with a
  sequence number, a source and a cause, catch-up from a sequence. Its
  contract (`Journal`: publish, `since(revision)`, the last revision,
  the event type an associated type, since egui-pop's events carry
  JSON and the contract crate has no dependencies) is slice 3 below,
  written when an application supplies its own: Helios's journal has
  the same shape (`helios_core::bus::Bus`: `publish`, `since`,
  `last_seq`), so the trait is cut against two real implementations,
  not one.
- **No transport.** Out-of-process listeners stay the caller's
  transport, given a longer deadline (`PLUGIN_DEADLINE`).

### What stays in `egui-pop-hooks`

`egui-pop-hooks` is the **default implementation**: its `Kernel`
implements `Bus` (ordering, the error boundary with caught panics,
inline and threaded deadlines, sinks, counters). It re-exports the
contract's types, so `egui_pop_hooks::{ListenOpts, Point, PointSpec,
LAST}` keep their paths; `Kernel` keeps its typed methods as inherent
ones for code that holds a `Kernel` and not a `dyn Bus`. It depends on
`bus-contract` and on nothing else.

### How a library receives the bus

- **A library never constructs the bus it publishes on.** It declares
  its points (`POINTS`), registers its listeners, and asks requests on
  the bus the application gives it.
- egui-pop: `App::builder().bus(bus)` takes an `Arc<dyn Bus>`. Without
  it the builder makes the default, an `bus_kernel::Kernel`: an
  application that does not care changes nothing but the major version.
  `AppHandle::bus()` returns the `Arc<dyn Bus>`.
- **One egui-pop application per bus.** egui-pop's listeners hold that
  application's window registry; a second application on the same bus
  would find its points declared and its listener ids taken. The
  builder refuses it (`BuildError::Bus`, naming the point), and the
  application gives each egui-pop application its own bus or one bus
  per subsystem, as it chooses.
- A check keeps the rule: `cargo xtask bus --check` fails when a
  library crate's source (`crates/*/src`, outside `egui-pop-hooks`
  itself and the builder's default in `egui-pop-api`'s `bus.rs`)
  constructs a `Kernel` (`Kernel::new`, `Kernel::with_default_deadline`,
  `Kernel::default`). Tests, examples, the gallery and applications may.

### Dyn, not generic

The bus is an `Arc<dyn Bus>`, not a type parameter:

- A generic `App<B: Bus>` would carry `B` through `AppHandle`, `Ctx`,
  every plugin's registration and every view that holds a handle: the
  whole public API of `egui-pop-api` and of each extension, for a choice
  most applications never make.
- Bus calls are **not per frame**. Per user action or per job state:
  window and popup requests, job requests, plugin connect and
  disconnect. Per pass: only `AppHandle::deliver_events`, which reads
  the journal (not the bus) and emits on the bus only for the job events
  it finds, usually none. Per-frame painting and hot paths (pointer
  moves, scroll deltas, progress ticks) stay off the bus by
  [ADR 0029](0029-not-on-the-bus.md)'s exceptions.
- The cost of erasure per emit: one virtual call, the input in an
  `Arc`, the value in a `Box`, one downcast per listener. The kernel
  already clones the value per listener for its error boundary and
  takes a lock; erasure adds nothing of another order.

### Migration and versions

- `bus-contract`: a new crate in its own repository, released on its
  own; egui-pop depends on `0.1.0` from crates.io (published
  2026-11-16).
- `egui-pop-hooks`: **major** (0.x). The types move to `bus-contract`
  and are re-exported; `PointInfo` gains `kind` (a struct literal or
  pattern without it no longer builds); `Point` gains `KIND` with a
  default (no change for implementors).
- `egui-pop-api`: **major**. `AppHandle::bus()` returns
  `Arc<dyn Bus>` instead of `Arc<Kernel>`; `bus::order_dump` and
  `bus::missing_targets` take `&dyn Bus` (re-exported from the
  contract); `AppBuilder::bus` is new. Callers of `bus()` keep their
  code: the typed methods are on `dyn Bus`.
- The order files and the existing bus tests stay unchanged: the
  listeners, their names and their order do not move.

### For Helios

Helios is the application, so the choice is its own. Its intent is
**one bus**:

- `main.rs` (the composition root) builds one `Arc<dyn Bus>` (the
  default kernel, or its own implementation) and hands it to egui-pop
  (`App::builder().bus(bus.clone())`) and to each Helios crate that
  publishes or listens, at construction. No Helios library crate
  constructs a kernel; the same check runs in its xtask.
- Its points (`health.report` first, then the rest of its hooks design)
  are declared from each owner crate's `POINTS` table on that bus; the
  debug bridge's `bus` collector then shows egui-pop's and Helios's
  points in one dump.
- The vault, the engine and the node relay publish on that bus instead
  of their own. The B11 change of Helios's bus audit ("join the four
  polled sources") becomes "**the four sources join the one bus**":
  their facts go through the journal contract (slice 3), and their
  listeners are listeners of the one bus, ordered and named like
  egui-pop's.
- Helios may still choose several buses (a separate one for a
  subsystem, a node process with its own): each library takes whichever
  it is given.

## Survey: crates that already do this

Checked on crates.io on 2026-11-16 (version, licence, last release).
`bus-contract` and egui-pop are MIT OR Apache-2.0 and the contract takes no
dependency; Helios (GPLv3) takes only permissive ones. What we need:
listeners ordered by name (`after`, `before`, `last`), owners and
`forget`, requests with a veto (steps, ADR 0011), `alter` and `collect`
with an error boundary and deadlines, introspection and an order dump,
synchronous and inline on the interface thread with no async runtime,
a journal with catch-up.

| Crate | Version, licence, last release | What it is | Verdict |
|---|---|---|---|
| `signals2` | 0.3.3, BSL-1.0, 2023-04 | Boost.Signals2 port: slots in ordered groups, combiners, disconnect | Closest in spirit. Groups are numbers, not names (no `after` a listener), no owners, no deadlines or error boundary, no introspection; little used (≈500 downloads in 90 days), no release since 2023. **Own** |
| `event-listener` | 5.4.2, MIT OR Apache-2.0, 2026-07 | A notification primitive (wake waiters) | A building block below a bus: nothing to order or collect. **Not used** |
| `tokio::sync::broadcast` | `tokio` 1.54.1, MIT, 2026-10 | Multi-consumer channel | Fan-out of values, asynchronous delivery, needs the runtime for waiting; no order, no result back. **Not used** |
| `async-broadcast` | 0.7.2, MIT OR Apache-2.0, 2024-12 | Async broadcast channel | Same as above. **Not used** |
| `postage` | 0.5.0, MIT, 2022-04 | Async channels | Same; unmaintained since 2022. **Not used** |
| `bus` | 2.4.1, MIT OR Apache-2.0, 2023-08 | Lock-free single-producer broadcast channel | A channel: one producer, no ordering, no answer. **Not used** |
| `eventbus` | 0.5.1, AGPL-3.0+, 2018-12 | Event bus with priorities | Licence excluded, unmaintained. **Excluded** |
| `pharos` | 0.5.3, Unlicense, 2021-10 | Observer pattern over async streams | Fan-out, asynchronous, no order or result. **Not used** |
| `bevy_ecs` events and observers | 0.20.1, MIT OR Apache-2.0, 2026-10 | Events read by systems; observers triggered on entities | Tied to the ECS world and its schedule; ordering by system sets, not per listener; far too large a dependency for a contract. **Not used** |
| `qubit-event-bus` | 0.21.3, Apache-2.0, 2026-11 | Typed sync and async event-bus facades with pluggable providers | The same idea (a facade, providers plugged in), but young (≈800 downloads), five dependencies of its own family, no ordering by name or deadlines. **Not used**; noted as prior art for the facade |
| `typed-emitter`, `event-emitter-rs`, `omnibus` | 0.2.0 MIT 2026-10; 0.1.4 MIT OR Apache-2.0 2020-08; 0.1.0 MIT 2026-03 | Node-style emitters, a small pub/sub | Fan-out without order or result. **Not used** |
| `log`, `tracing` | MIT OR Apache-2.0 | Facade crates: a contract, the application installs the implementation | The **shape** we adopt (a contract crate, the implementation chosen by the application), not the global install |

None fits as the default implementation behind the contract: what
makes the bus ours (named ordering, owners, the error boundary,
deadlines, introspection) is exactly what they lack. **Own** the
contract and keep `egui-pop-hooks` as its default implementation; an
application that prefers another implementation writes one against the
contract.

## Slices

1. **The contract crate and the kernel on it**: `bus-contract` (its own
   repository) with the traits, the typed calls on `dyn Bus`, `PointDecl` and
   `declare_all`, `order_dump`, `missing_targets`; `egui-pop-hooks`'s
   `Kernel` implements `Bus` and re-exports the types. *Test:* the
   kernel's suite unchanged, run through `Arc<dyn Bus>` as well; a
   contract test with a minimal implementation.
2. **egui-pop-api on the contract**: `AppBuilder::bus`, every point and
   listener through `dyn Bus`, egui-pop's `POINTS` table, the
   `cargo xtask bus --check` rule in pr-lint. *Test:* the bus suites
   (`window_bus`, `window_invariants`, `jobs_bus`, `admin_window`,
   `shell_windows`, `removal`) and the order files unchanged; an
   application-supplied bus (a test implementation recording each call)
   receives egui-pop's points and listeners, in their order, and the
   requests asked on it.
3. **The journal contract**: `Journal` in `bus-contract`, the host's
   journal implementing it, `AppBuilder::journal`; cut against Helios's
   journal as the second implementation. *Done (2026-11-19)*:
   `Journal` came in bus-contract 0.3 as a capability of the bus
   (`Bus::journal`), cut against Helios's fact journal. So no
   `AppBuilder::journal`: when the bus given to `AppBuilder::bus` offers
   the capability and the host keeps no journal of its own, egui-pop's
   journal is that one, its events entries of the journaled point
   `pop.journal` sharing one numbered sequence with the application's
   facts; agents' `wait` reads it ([The hook kernel](../hook-kernel.md),
   "One journal per process", says what changes for a `wait`).
4. **Helios** (its own pull requests): one bus built in `main.rs`, given
   to egui-pop and to each crate; its points declared from their
   owners' tables; the vault, engine and node relay on it (B11).

ADR 0029's slices 5 to 11 continue after slice 2, on the contract.

## Consequences

- A library's dependency on the bus is a dependency on a crate with no
  code to run; the application picks the implementation.
- Tests may hand egui-pop a recording or failing bus to check what it
  declares and asks, without a window.
- Two majors at once (`egui-pop-hooks`, `egui-pop-api`); code that only
  calls `bus()` and the typed methods compiles unchanged.
- The kernel's contract page ([The hook kernel](../hook-kernel.md))
  describes the default implementation; the contract is described by
  `bus-contract`'s documentation.
