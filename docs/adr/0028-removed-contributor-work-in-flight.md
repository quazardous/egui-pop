# 0028: A removed contributor's work in flight is closed, refused or kept dormant, and said

Date: 2026-11-15. Status: accepted (2026-11-15).

## Context

ADR 0004 removes everything a plugin registered through `Ctx` when it is
disconnected (`remove_owner` on every registry), and tests check the
registries come back to their previous state
(`everything_a_plugin_registers_goes_away_with_it`,
`disconnecting_a_plugin_disconnects_those_requiring_it_first`). It says
nothing of what the plugin left **in flight**: a command queued with
`run_as`, a dialog open on its command, steps in a history, a job the
host runs for it, a user's key bound to its command, an icon, a mounted
component, a value still loading. An outside review
(`docs/review-2026-11.md`, point 1) asked for that decision.

Other hosts of extensions: VS Code calls an extension's `deactivate`
and disposes what it pushed to `context.subscriptions` (commands, views,
listeners); user keybindings naming a command no longer contributed stay
in `keybindings.json` and do nothing until it comes back; an open
webview of the extension is closed. Blender's add-on `unregister`
removes its classes (operators, panels, keymap items it added); the
user's keymap entries naming a gone operator stay, shown as unknown;
undo steps are memory snapshots and outlive the add-on.

A "contributor" here is a plugin (ADR 0004), or anything that registers
under an owner id and can go: a panel's block, a component mounted in a
host (ADR 0013), a custom dialog's body.

## Decision

1. **Removal happens between passes, never inside a command.** A plugin
   asks for it through `AppHandle::set_plugin_enabled` (or the agent
   bridge's `set_enabled`); the request is queued and done in request
   order before the next pass. Requests queued before it run while the
   plugin is still there; those queued after find it gone. Dependents go
   first (ADR 0004).
2. **Each kind of work has one rule and one cleaner**, in the table
   below. The kernel (the host, ADR 0010) owns what it runs and keeps;
   egui-pop owns what it registered, drew or queued; the contributor's
   `disconnect` and its `Drop` own what it holds outside the framework.
3. **Nothing goes silently (ADR 0003).** The disconnection itself is a
   journal event (`plugin.disconnected`, with what went, by kind) and a
   `log::info!`; each thing refused or closed later because its owner is
   gone says so where its kind already reports: the journal
   (`command.refused`, `dialog.cancelled`), the diagnostics list, the
   key trace.
4. **What the user made is kept, dormant, and comes back.** Values they
   changed, keys they bound, a panel's place: kept while the plugin is
   off, persisted, alive again when it reconnects. What only made sense
   while it ran (a dialog, an overlay, a queued run) is closed or
   refused, never replayed on reconnection.

| Work in flight | Guarantee | Who cleans | What the user sees | What is logged |
|---|---|---|---|---|
| Commands, menu items, toolbar items, palette entries, keys it declared | Removed | egui-pop (`remove_owner`) | They disappear; a menu item of another plugin naming one is shown greyed | `unknown-command` warning in the diagnostics, per item |
| A run of its command queued (`run`, `run_as`, a menu, a toast's or a finished job's action, an accepted dialog) | Refused | egui-pop (`App::run_in`) | Nothing runs; a toast's action of a gone command is greyed | `command.refused` on the journal with "its plugin `x` is disconnected", from the caller |
| A dialog open on its command (or with its custom body) | Cancelled | egui-pop (dialogs plugin, on `Disconnected`) | The dialog closes as cancelled; the next queued one shows | `dialog.cancelled` with the reason |
| Overlays, popups, flyouts drawn by its blocks or panels | Closed at the next pass | egui-pop (an overlay its owner stops showing is closed) | They close; the focus goes back as on Escape | the overlay's close, as any |
| Panels and blocks it mounted | Removed; a panel's place in the layout is kept dormant | egui-pop (`remove_owner`, layout repair) | The panel and its detached window go; when the plugin comes back, the panel comes back where it was | `layout:` info line per panel |
| Components and host offers it declared (ADR 0013), their mounts | Removed with their mounts | egui-pop (mount registry) | Nothing more | the `mounts` collector no longer lists them |
| Hooks, step hooks, lifecycle handlers, value listeners, collectors, context providers, clipboard formats, link handlers | Removed | egui-pop (`remove_owner`, `On::Disconnected` listeners) | Nothing | the census in tests; collectors leave the bridge |
| Listeners in a standalone hook kernel (`egui-pop-hooks`) | Removed by `Kernel::forget(owner)` | the application that holds the kernel | Nothing | the kernel's introspection |
| Icons it registered (`<plugin>:<name>`, command icons it set) | Removed | egui-pop (icon set, command icons) | A reference to one shows the unknown-icon mark | `unknown-icon` diagnostic per reference |
| Its jobs (run by the host, followed from `job.*` events) | Cancelled when the job names it (`owner`, or `command`) and is cancellable; otherwise (not cancellable, or published by the plugin itself) kept and marked orphaned | the host runs `job.cancel`; egui-pop asks it as the plugin goes | The jobs centre shows the job cancelling, or "its plugin x is off" with Dismiss | `job.cancel` dispatched by the framework with the reason, then the host's `job.*` events; `job.orphaned`; both counted on `plugin.disconnected` |
| Late values and latest-wins loads its panels hold | Dropped with the panel; a running load is told to stop, its result dropped | the contributor's drop (`Latest`'s `Drop`) | Nothing | nothing (no state left) |
| Undo steps in the value store's histories | Steps touching only its paths are dropped; a step mixing its paths with others' is dropped too | egui-pop (value store `remove_owner`) | Undo skips them; the history panel no longer lists them | a `log::info!` and a journal note with the count dropped, per history |
| Steps in histories the kernel owns (ADR 0014) | The kernel's call | the kernel (it sees `Disconnected`) | as the kernel decides | the kernel's |
| Its local undo scopes (a component's own history) | Removed | egui-pop (`history::remove_owner`) | Undo in that surface no longer applies | nothing |
| Values it declared, changed by the user | Kept dormant (pending), persisted, back on reconnection | egui-pop (value store) | They are not shown while it is off | nothing |
| Keys the user or the workspace bound to its commands | Kept dormant, persisted, alive on reconnection; a dormant binding does not resolve, the key falls through | egui-pop (keymap) | The shortcuts editor lists them as dormant, with the plugin's name | `dormant` in the key trace (the `because` of a lost verdict until the verdict enum can take a variant), the `keys` collector's `dormant` list, an info line |
| Its own files, threads, sockets, caches | Its business | the plugin's `disconnect` and `Drop` | as it decides | as it decides; egui-pop never deletes data |

## Consequences / to do

Each line is a gap between the code of 2026-11-15 (egui-pop `next`,
when this was written) and the decision above, small enough to be its
own change with its frame test. **All nine are closed (2026-11-15)**;
the frame tests are in `crates/egui-pop-api/tests/it/removal.rs`.

- **No trace of the disconnection itself.** `App::disconnect` emits
  `Lifecycle::Disconnected` to handlers only: no journal event, no log
  line. Add `plugin.disconnected` with counts by kind
  (`crates/egui-pop-api/src/app.rs`, `disconnect`, `remove_owner`).
  **Closed (2026-11-15):** `plugin.disconnected` from `egui_pop`, with
  the reason and what went by kind, and an info line.
- **A queued run of a gone command leaves no journal entry.** `run_in`
  skips `record` for `RunRefused::Unknown` and the request loop only
  logs a warning; refuse it as `command.refused` with the owner that
  went (`crates/egui-pop-api/src/app.rs`, `run_in` and
  `process_requests`). A key dispatched to a gone command only logs at
  info level (`dispatch_keys`). **Closed (2026-11-15):** such a run is
  a `command.refused` event naming the plugin that went.
- **Open dialogs survive their command's plugin.** A dialog has no
  owner; it stays open, and accepting it publishes `dialog.answered`
  then queues a command that is refused. Tag a dialog with its
  command's owner (and a custom body with its opener) and cancel it on
  `Disconnected` (`crates/egui-pop-api/src/dialogs.rs`, `Open`,
  `apply`). Toasts' actions naming a gone command are not greyed
  (`crates/egui-pop-api/src/toasts.rs`). **Closed (2026-11-15):** a
  dialog on a gone plugin's command, or a custom one it opened, is
  cancelled at once (`dialog.cancelled` from `egui_pop` with its
  `reason`); a toast's action naming a gone command is greyed with the
  plugin's name (`AppHandle::command_gone_with`).
- **A panel's place is forgotten.** Layout repair drops panels no longer
  declared, and the repaired layout is what is saved; keep their place
  dormant instead (`crates/egui-pop-model/src/layout.rs`, `repair`).
  **Closed (2026-11-15):** `SavedLayout::dormant` keeps the place (docked,
  detached, floating or closed), saved, and `repair` gives it back when
  the panel is declared again.
- **Mount declarations outlive their plugin.** Components and host
  offers are kept by owner but never removed, and `Mounted` records
  carry no owner (`crates/egui-pop-api/src/mounts.rs`, `Registry`,
  `record_mount`; `remove_owner` in `app.rs`). **Closed (2026-11-15):**
  its components, host offers, the mounts of either and those made while
  its code ran are removed, counted on `plugin.disconnected`.
- **Icons are never removed.** The icon set keeps every name for the
  process and has no way to remove one, though `docs/icons.md` says a
  plugin's icons go with it (`crates/egui-pop-core/src/icon_set.rs`;
  `remove_owner` in `app.rs`). **Closed (2026-11-15):**
  `icon_set::unregister_owner`, called when a plugin goes.
- **Jobs have no owner.** `job.*` events carry none, so nothing can
  cancel or mark a gone plugin's job; add an optional `owner` to
  `job.started` and act on `Disconnected`
  (`crates/egui-pop-model/src/jobs.rs`, `crates/egui-pop-api/src/jobs.rs`).
  **Closed (2026-11-15):** `job.started` carries `owner` and `command`;
  as a plugin goes, the framework runs `job.cancel` (with the reason,
  kept as `Job::reason`) for its jobs that name it and can be
  cancelled, and publishes `job.orphaned` for the others
  (`Job::orphaned`, shown by the jobs centre, dismissable, taken back by
  `job.adopted` on reconnection); `plugin.disconnected` counts them
  (`jobs_cancelled`, `jobs_orphaned`). The model changes through the
  journal's events alone.
- **Undo steps go without a word.** The value store drops them, mixed
  steps included, and says nothing
  (`crates/egui-pop-model/src/values/mod.rs`, `Store::remove_owner`).
  **Closed (2026-11-15):** `Store::remove_owner_report` counts them per
  history, logged and carried by `plugin.disconnected`.
- **Dormant keys still resolve.** User and workspace bindings naming a
  gone command stay in the keymap (right) but win their key and fail at
  run time, and `derived.keys` lists only declared commands, so the
  shortcuts editor does not show them (`crates/egui-pop-model/src/keymap.rs`,
  resolution; `crates/egui-pop-api/src/app.rs`, `rebuild` and
  `dispatch_keys`). **Closed (2026-11-15):** `Keymap::set_dormant`
  takes the bindings of a gone plugin's commands out of the resolution
  (the key goes to the next binding, they show no key and make no
  conflict) while they stay in their layer, saved;
  `AppHandle::dormant_keys` gives them with the plugin, and the
  shortcuts editor lists them under "Dormant keys"; they are live again
  on reconnection. The trace says so through a lost verdict's `because`
  (`keymap::DORMANT`): a variant of its own would break the stable
  `Verdict` enum, and waits for the next breaking release.

Already as decided: removal between passes, dependents first; registry
contents by owner; overlays closed when their owner stops drawing them
(`crates/egui-pop-core/src/overlay.rs`); store values kept pending and
back on reconnection; local undo scopes; collectors leaving the bridge;
`Latest` stopping its load on drop; the standalone kernel's `forget`.
