# 0029: What is not on the bus does not exist: a module knows the bus, not its neighbours; the window lifecycle first

Date: 2026-11-15. Status: accepted (2026-11-15, by the lead maintainer's
decision under GOVERNANCE, without the comment period).

This ADR decides and lists; it changes no behaviour. Each slice at the
end is its own change.

## Context

An application (Helios) reported a black window without a title bar
after closing the Settings app shown in its own window. The application's
state said "Settings is closed" while a system window was still on
screen. A first fix guarded only the main window: the guard was put where
the fault was guessed. A second one (#439) gave application windows one
close path and a paint guard on every window. The application's log then
showed the cause: **the close happened in two steps.** The Settings
window's entry went at once, in that window's own pass ("a pass of
viewport 9FA2 after its window went … it paints nothing"); the rest
followed at the main window's next pass, which came 1.0 to 1.7 seconds
later: the admin app's flags caught up (`follow_window`), the viewport
stopped being declared, the application logged "the Settings app is
closed". In between, the backend still had the viewport and presented
passes that drew nothing: black on GNOME under Wayland, where hiding a
window does nothing.

What that showed is wider than one window:

- **Two truths.** What egui-pop believes is shown (its entries, the
  overlay stacks, each module's `open` flag) is never compared with what
  exists (the backend's viewports and what they painted).
- **No single owner of a window's life.** Application windows
  (`windows.rs`), detached panels (`workbench.rs`), the Settings app's
  window (`admin/shell.rs`), the frame's close (`frame.rs`), the paint
  guard (`paint_guard.rs`), dialogs, overlays and popups each show and
  hide on their own.
- **Cascades to remember.** Closing an application window ends in
  `window_closed` (`crates/egui-pop-api/src/windows.rs`), a function
  whose whole job is to remember who else must be told, in order: the
  thread-local registry, the paint guard's watch, the dialogs, the
  overlay's window separation, the journal, a repaint. A seventh part
  that cares (the runner's handles, a file choice, a web view) is added
  by whoever remembers to. The Settings window adds a second layer
  (`follow_window`, a four-case match keeping two flags in step with the
  window), and the application copies it for each of its own windows.
  Forgetting one call is silent.
- **Following instead of being told.** These "follow" functions poll in
  the main window's pass and catch up with what changed elsewhere. They
  are only as fast as that pass: the two-step close above is their
  failure.

The pieces of a central bus already exist, apart:

- **Requests**: steps ([ADR 0011](0011-lifecycle-steps.md)): named,
  with arguments, vetoable, replaceable, traced; commands go through the
  host ([ADR 0023](0023-every-command-through-the-host.md)).
- **Events**: the host's journal (`AppHandle::publish`,
  `AppHandle::events_since`): facts with a source and a cause, which
  tests and agents wait on ([The native runner, reads and
  waiting](../runner.md)).
- **Ordered listeners**: the hook kernel
  ([The hook kernel](../hook-kernel.md)): listeners with ids, `after` /
  `before`, priority, an error boundary and deadlines per listener,
  introspection in run order. No egui-pop crate uses it yet; the steps'
  after-hooks run in registration order.

## Decision

### The principle

1. **A module or component knows the bus, not its neighbours.** A state
   change another part may care about is a **request** (asked on the
   bus, with who asks and why) and an **event** (said on the bus, once
   done or refused). Whoever cares listens. Nobody calls a neighbour to
   tell it.
2. **Not on the bus is a defect**, as a silent no-op is
   ([ADR 0003](0003-no-silent-no-op.md)): a state another part can see
   or depend on that changed without its request and event is a bug,
   whatever else works.
3. **The smell this removes** is the function whose job is to remember:
   one that notifies or cleans several other parts, in an order its
   author knew. It becomes one request, and each part it called becomes
   a named listener that declares where it runs. Adding a part that
   cares adds a listener, and edits no other module.
4. **The bus is the three pieces above, joined**: a bus point is a
   step (the request: `<noun>.<verb>`), its journal events
   (`<noun>.<past>`, `<noun>.refused`), and the listeners of each,
   registered in a hook kernel the application holds (`egui-pop-api`
   takes `egui-pop-hooks`, L0, as a dependency). The kernel stays what
   it is (no transport, nothing across processes); the journal stays the
   record.

### Ordering

- **Every listener is named** (`<module>.<what>`: `window.registry`,
  `dialogs.cancel-on-close`) and owned (its plugin or `egui_pop`), so
  it goes with its owner ([ADR 0028](0028-removed-contributor-work-in-flight.md)).
- **Order is declared with `after` / `before`**, then priority.
  Registration order is never relied on: a listener whose place matters
  and declares nothing is a review finding.
- **The order run is printable.** For each point, the listeners in run
  order with their owner and constraints (`egui_pop_hooks::Kernel::listeners`
  gives it): a `bus` collector on the debug bridge, a text dump in debug
  builds, and a test that keeps the dump of the window points as a
  reviewed file. "Who runs when" is read, not remembered.
- **A cycle or a missing target fails tests.** A cycle is already
  refused at registration. A missing `after` / `before` target is
  reported and ignored at run time (a plugin may be off); the test kit
  fails a test that ends with one on an egui-pop point.
- **An invariants listener runs after all others of its point.** The
  kernel has no such constraint today: it gains "last" (one per point, a
  second refused), not a low priority that a later listener could
  undercut.
- Listeners run on the interface thread, inline, inside the kernel's
  error boundary: one that fails or panics is reported (`hook.failed`)
  and the others still run.

### Timing

- **A request's listeners all run in the same pass, in the call that
  asked.** When the request returns, every listener has run, in its
  declared order, each seeing what the ones before it left; the event
  (`window.closed`) is published after the last one, so whoever reads
  it finds the owner's state, the focus and the registry already agree.
- **Nobody follows by polling.** A part that compares flags in a later
  pass to catch up with a change made elsewhere (the `follow` functions)
  is a listener that was never registered. It goes in the slice that
  moves its path.
- **What may defer, and only this:** what the backend or the disk does
  on its own clock.
  - The system window's destruction: eframe drops a deferred viewport at
    the next main pass that no longer declares it. The registry has
    already marked the window closed, so it is no longer declared; the
    window is hidden where the system allows, and a main pass is asked
    at once. A pass of that viewport in between draws the window's last
    page or the paint guard's fallback, never nothing; it is the "one
    pass" invariant 1 allows, and a second one is a `window.invariant`.
  - Writing the place to disk: `window.place` updates the place in
    memory in the pass; the write follows persistence's own schedule.
- A listener that needs a frame of its own to do its work (drawing the
  closing window once more) is not a listener: the window's pass reads
  the registry instead.
- The 1.0 to 1.7 seconds between "its window went" and "the Settings app
  is closed" in the context above are the counter-example: under this
  ADR they are one request, and its test asserts both in the same pass.

### Exceptions

The list below is closed: an exception names one of these reasons, and
a new reason is an ADR.

| Reason | What stays off the bus | Why |
|---|---|---|
| Per-frame painting | Drawing, layout, hover and animation state of a pass; egui's own tooltips | Nobody can act on it by the next pass; the bus would carry noise |
| Pixel and bulk data | Tiles, textures, thumbnails, text buffers | Too large; their arrival is an event, their content is not |
| Private internals | State no other part reads or depends on (a cache, a scroll offset, a widget's memory slot) | No neighbour exists to forget |
| Hot paths | Pointer moves, key repeats, scroll deltas, job progress ticks | Rate; their settled result is an event (`zoom.change`, a finished gesture) |

How one is declared: where a lint guards the raw call
(`clippy::disallowed_methods`), an `#[expect(clippy::disallowed_methods,
reason = "bus: per-frame painting, …")]` at the call; all of them listed
in `xtask/bus.allow`, one line per call site with its reason, checked by
`cargo xtask bus --check` (a call missing from the file fails; a line
matching nothing fails, so the file shrinks as paths move). **The list
only shrinks**: a pull request adding a line says so in its description
and is reviewed as a decision.

### First application: the window lifecycle

"Window" below is anything that occupies the screen with a life of its
own: the main window, an application window
([ADR 0025](0025-application-windows.md)), a detached panel's window, a
native child window egui does not paint (the system file chooser, the
viewer's web view). A "popup" is what floats inside or over a window and
closes by rules: a dialog, a palette, a menu, a flyout, a popover, a
hover card.

**Requests** (steps), each with `owner` (a plugin, a command, or a
parent window), `reason` (`user`, `command`, `parent-closed`,
`owner-gone`, `guard`, `system`, `quit`) and the cause's sequence number:

| Request | Asks |
|---|---|
| `window.open` | A window of a kind, with its id, title, parent if any |
| `window.close` | A window closed; the main window: the application's close ([ADR 0025](0025-application-windows.md)'s move-in rule is a listener's answer) |
| `window.reparent` | A window's content moved to another window (into the main window and back; a panel docked back or detached) |
| `popup.open` | A popup in a window, with its parent (a window, or the popup under it) |
| `popup.close` | A popup closed |

**Events** (journal):

| Event | Says |
|---|---|
| `window.opened` | The registry holds it: id, kind, owner, parent, viewport |
| `window.closed` | It left the registry: who asked, why |
| `window.painted` | Its first paint; then each change between painted and painted nothing |
| `window.orphaned` | Its owner or parent went and it is still there (then closed by the children listener, or kept and marked, per ADR 0028) |
| `window.refused` | A request not done: vetoed, unknown id, no owner, a parent that does not exist |
| `window.invariant` | An invariant below does not hold: its `rule`, the window, its owner and state |

`popup.opened` and `popup.closed` say the same for popups. The names
published today that say less (`window.moved`, `window.outlived`,
`window.painted_nothing`, `window.focus_hole`) become `window.reparent`'s
event and `rule`s of `window.invariant` as their path moves; they are
Experimental.

**The registry is a listener, and the one truth.** `window.registry`
listens to the requests and holds every window and popup: id, kind,
owner, parent, viewport, opened by which request, painted or not, since
when. Nothing else keeps a list of windows: the thread-local shim, the
paint guard's watches, the overlay's window separation and the
workbench's auxiliary list read it or are derived from it. It is read
as `windows.list` (the application, the debug bridge, tests), and
because every change is a journal event, "what was on screen and why"
can be replayed after the fact.

**Invariants**, checked by `window.invariants` at the end of every pass
of the main window and after every window request:

1. The registry's windows are the backend's viewports (egui's
   `raw.viewports`, the runner's windows): none more, none fewer, past
   the one pass the backend needs to create or destroy one.
2. A shown window painted something within N passes and the guard's
   delay (three passes and two seconds today,
   `egui_pop_api::paint_guard::DEFAULT_DELAY`).
3. No window or popup without an owner.
4. A popup's parent exists.
5. A closed parent has no live child.

A violation is one `window.invariant` event and one error line naming
the window, its owner and its state; the paint guard's fallback page
and hiding ([Containment](../containment.md)) are how it is contained.
In tests (the test kit's harnesses, `MultiWindow`, scenarios) a
violation fails the test; in release builds it is logged.

**The listeners of `window.close`, in their declared order:**

| # | Listener | Does | Declares |
|---|---|---|---|
| 1 | `window.children` | Asks `window.close` / `popup.close` (`parent-closed`) for every child: its dialogs cancelled, its overlays, its file choice, its native children | first |
| 2 | the owner's listener (`admin.window`, an application's `<app>.window`) | The window's owner closes the state the window showed: the Settings app closes, an application's focus or import state closes. Registered by the owner | `after` 1, `before` 3 |
| 3 | `window.focus` | The active window and the keyboard focus move to the window worked in before; modal blocking is recomputed | `after` 2 |
| 4 | `window.registry` | Marks it closed: no longer declared to the backend, hidden where the system allows; forgets its handles | `after` 3 |
| 5 | `window.place` | Saves its place (the one owner of a window's place, [eframe comparison](../eframe-comparison.md)) | `after` 4 |
| 6 | `window.invariants` | Checks the five rules | last |

The owner's listener may veto in its before-hook (unsaved changes: the
step's veto, `window.refused`); once the close runs it cannot defer.
`window.open` runs the owner (its state open), registry, place
(restored), focus, invariants; `window.reparent` runs registry, place
(each id keeps its own), focus, invariants.

### What the code does today: the migration list

Every place that shows, hides or closes a viewport, a native window or
a popup (egui-pop `next`, 2026-11-15).

| File (under `crates/`) | Today | Becomes |
|---|---|---|
| `egui-pop-api/src/windows.rs` | `open_window` pushes an entry, fills a thread-local registry, tells the paint guard, publishes `window.opened`; `close_entry` (the one close path of #439) marks the entry, keeps the geometry, sends `Visible(false)`, then `window_closed` calls the registry, the paint guard, the dialogs, the overlay separation, publishes, repaints; `move_into_root` and the way home swap contents and publish `window.closed` / `window.moved` / `window.opened` by hand; `declare_windows` calls `show_viewport_deferred` and first lets the Settings app follow; `quit` and the root pass send `ViewportCommand::Close` | `window.open` / `close` / `reparent`; `window_closed` split into the listeners above; `declare_windows` is the registry's one raw call |
| `egui-pop-api/src/app.rs` | The main pass sees the root's close request, tries `move_into_root`, sends `CancelClose`; calls `forget_closed_windows` | A `window.close` of the main window; the move-in is a before-hook's redirect to `window.reparent` |
| `egui-pop-api/src/admin/shell.rs` | `follow_window`: a four-case match per pass between "the app is open", "its window is there" and a `window_shown` flag; `window_content` closes the app and the window from the window's pass | The Settings app asks `window.open` / `window.close` with itself as owner, and listens to `window.closed` to close; no flag |
| `egui-pop-api/src/workbench.rs` | Detached panels are `show_viewport_immediate` per pass from the layout's auxiliary list; a close request runs the `workbench.close_window` step, `CancelClose` on a veto; `forget_closed_windows` rebuilds the list of live windows for the overlays | Detach and dock back ask `window.open` / `window.reparent`; the registry holds the windows the overlays keep |
| `egui-pop-api/src/frame.rs` | The `frame.close` step sends `ViewportCommand::Close` to the viewport it is drawn in; the system menu is an overlay | `frame.close` asks `window.close` |
| `egui-pop-api/src/paint_guard.rs` | Its own tables of watched and closed windows (`app_window_opened`, `app_window_closed`), `watch_closed_windows` compares them with `raw.viewports` and publishes `window.outlived`; `orphan_pass` sends `Visible(false)` and `Close` | The watch reads the registry; its findings are `window.painted` and `window.invariant`; closing an orphan is a `window.close` (`guard`) |
| `egui-pop-api/src/runner/native.rs` | Pairs winit windows with viewports, forgets handles on `Destroyed` | Feeds invariant 1 with the windows that exist; handles forgotten by `window.registry` |
| `egui-pop-api/src/dialogs.rs` | One queue per window group, drawn with `overlay::show`; `window_closed` is called by `windows.rs` to cancel a closed window's dialogs | `popup.open` / `popup.close`; `dialogs.cancel-on-close` listens to `window.close` |
| `egui-pop-api/src/files.rs`, `files/browser.rs` | The in-app chooser is an overlay; the system chooser (`rfd`) is a native window parented by a recorded handle, its request tied to the window that asked | The in-app one a popup; the system one a native child in the registry |
| `egui-pop-views/src/viewer/wry_view.rs` | A GTK child window created and closed by the viewer | A native child in the registry (owner: the viewer's mount) |
| `egui-pop-core/src/overlay.rs` | The overlay stack per viewport in egui's memory: `show`, `hold`, `close`, `retain_windows`; it closes what is above and gives the focus back by itself | Stays the model of popups inside a window; its opens and closes are `popup.*`, its per-window stacks derived from the registry |
| `egui-pop-api/src/menus.rs`, `palette.rs`, `strips.rs`, `admin/quick.rs`, `admin/shell.rs` (overlay mode), `settings_window.rs` | Each calls `overlay::show` / `hold` / `close` with its own `open` flag; popup menus through `egui::Popup` | `popup.open` / `popup.close` with their owner |
| `egui-pop-api/src/jobs_centre.rs`, `toolbars.rs`, `palettes.rs`, `egui-pop-core/src/input.rs`, `egui-pop-editors/src/assist_ui.rs`, `egui-pop-views/src/library/tags.rs` | `egui::Popup` and bare `egui::Area` popovers (the jobs popover, a toolbar's overflow, choice lists, completions) | Popovers with a life (opened, closed by rules) are popups; a list drawn only while its field is focused is per-frame painting, declared |
| `egui-pop-core/src/hover_card.rs`, `tooltip.rs` | A hover card has `open` / `close` and an area; a tooltip is egui's | Hover cards are popups; tooltips are the per-frame exception |
| `egui-pop-api/src/toasts.rs`, `tips/screen.rs`, `zen.rs`, `window.rs`, `egui-pop-viewport/src/navigation.rs` | Areas drawn each pass from a state (toasts per window group, the tips card, zen's reveal bar, floating panels, the navigator) | Not windows; toasts already journal; reviewed in the popups slice and declared where they stay off |
| `egui-pop-platform/src/window.rs` | `raise` sends `Visible(true)`, `Minimized(false)`, `Focus` to the main window | A request on the bus (`window.raise`, with the existing `window.switch`) |
| `egui-pop-test/src/windows.rs` | `MultiWindow` plays the backend: declared viewports, their commands | The "backend" side of invariant 1 in tests |
| `egui-pop-gallery/src/lib.rs`, `examples/multi_window*.rs` | Raw `ViewportCommand::Close` and `show_viewport_deferred` in the smoke exit and the spike | The gallery asks `window.close`; the spike is an example, declared |

In the first application, Helios (`crates/helios-gui/src/`), the same
shapes, moved in its own pull requests once egui-pop's slice for them
has landed:

| File | Today | Becomes |
|---|---|---|
| `shell/focus_window.rs` | `follow`: the Activity, Calibration and Library foci each open and close their window from a four-case match in the main pass, with an `open` flag and a `shown` id; the window's own pass closes the focus and calls `window.close` and `hide_closing_window` | The focus asks `window.open` / `window.close` as owner; its listener closes its state |
| `shell/import_window.rs` | The same `follow` match for the import window, `close_window`, and `hide_closing_window` (raw `Visible(false)`, `Minimized(true)`, `Focus` to the main window) | Same; the hiding is `window.registry`'s |
| `shell/trace.rs`, `shell.rs` | `Screen::follow` samples the Settings app's state in the main pass and logs "the Settings app is open / closed" when it changed (the line that came 1 to 2 seconds late) | Logs from `window.opened` / `window.closed` and the admin app's events |
| `shell/health.rs`, `shell/storage.rs` | Read whether the Settings app is open on a scope, each pass | Read the registry / the admin app's state; unchanged where per-frame painting |
| `main.rs` | Raw `ViewportCommand::Close` for the session's end, a restart after a restore, the end splash | `window.close` of the main window with reason `quit` / `system`; the splash (before the shell exists) is declared |
| `shell.rs` (`open_window`) | Opens the main window before the shell exists | The registry's first entry once the shell is built |

## Slices

One pull request each, in this order; each lands with its acceptance
test and leaves `next` green. Slices 1 and 2 come first; the paths of
3 to 8 move in that order; 9 to 11 close the set.

1. **Registry, events and the close of an application window.**
   `egui-pop-api` takes the hook kernel; the window and popup points
   exist; `window.registry` holds every window (paths not moved yet
   report to it); `windows.list`; the kernel's "last" constraint; the
   order dump. `window.close` of an application window is the first
   request moved: `close_entry` and `window_closed` become its
   listeners, and the Settings app's owner listener (`admin.window`)
   closes the app in the same call, whoever asked. *Test:* the
   two-step close reproduced with the Settings app in its own window,
   closed by its title bar, the page's Close, Escape and `window.close`:
   no pass of the viewport after its window went, and the app closed in
   the pass its window closed; it fails on the commit before the slice
   and passes after. Also: each kind of window gives the events above
   and the right `windows.list`; the window points' order dump matches
   its reviewed file; a missing target fails.
2. **Invariants listener, failing tests.** The five rules, run every
   pass; `window.invariant`; the test kit fails on one. *Test:* each
   rule broken on purpose (a viewport shown outside the registry, a
   window that paints nothing, no owner, a popup whose parent closed, a
   child left alive) fails its test with the window named; the existing
   suites stay green, or what they find is fixed or filed first.
3. **The Settings app's window.** Opening goes through `window.open`;
   `follow_window` and its `window_shown` flag go; `follow` no longer
   runs in the main pass. *Test:* `admin_window`: opened and closed
   every way (title bar, the page's Close, Escape, the command, the app
   closing under it, its plugin disconnected), the registry and the
   backend agree after each.
   Done (2026-11-16): the app is its window's owner and asks
   `window.open` (opened, or brought to the front) and `window.close`
   where what it wants changes (`Admin::open`, `Admin::close`, the
   `admin.window` setting written through the app, a focus-screen
   wizard starting or ending), in that call; `admin.window` listens to
   both points (first on `window.open`; after `window.children`, before
   `window.focus` on `window.close`), closing the app unless it moves
   over the main window ("Pop in"). Its plugin disconnected closes its
   window through `window.owner-gone`, a listener of `plugin.disconnect`.
   The overlay reads the registry to know whether the window draws the
   app. Left: the `admin.window` setting changed by another writer (the
   settings store, `settings.set`) is followed at the app's next request,
   until settings changes are on the bus.
4. **The shell: application windows, the main window, detached
   panels.** `open_window`, moving into the main window and back
   (`window.reparent`), the main window's close, `quit`, `frame.close`,
   detach and dock back. *Test:* `app_windows` and `window_guard` pass
   unchanged; closing the main window with a window open moves it in as
   a `window.reparent`; a vetoed panel close leaves the registry
   unchanged.
5. **Dialogs and choosers.** Dialogs, the in-app and system file
   choosers, the viewer's web view. *Test:* a window closed with a
   dialog and a file choice open leaves `popup.closed` (`parent-closed`)
   for each and no child in `windows.list`.
6. **Popups, menus and tooltips.** The overlay stack's opens and closes,
   popup menus, the palette, flyouts, popovers, hover cards; tooltips and
   field-bound lists declared as exceptions. *Test:* every overlay of
   the gallery opened and closed appears in and leaves `windows.list`;
   invariant 4 holds while a parent closes under a menu.
7. **Standard windows.** The settings window, About, the jobs centre,
   tips. *Test:* each opened from a second window belongs to it, and
   goes with it.
8. **The application's own windows** (in Helios, its own pull
   requests): the import window, the focus windows, the screen log, the
   raw closes of `main.rs`, from the table above. *Test:* the
   application's window tests close each window every way and find the
   owner's state closed in the same pass.
9. **The Clippy list.** `show_viewport_deferred`,
   `show_viewport_immediate`, `send_viewport_cmd` / `send_viewport_cmd_to`
   with `Close`, `Visible` or `CancelClose`, and `overlay::show` /
   `hold` / `close` are disallowed outside the registry, in egui-pop and
   in the application; `xtask/bus.allow` and `cargo xtask bus --check`
   in pr-lint. *Test:* the check itself, with a fixture that must fail.
10. **The agent bridge's `windows.list`.** The `windows` collector gives
    each window and popup with owner, parent, kind, painted or not;
    `describe` and the MCP server follow. *Test:* the bridge's answer
    equals the registry's in a two-window scenario.
11. **The journey asserts the window set at every step.** The test
    kit's scenarios take an expected window set per step; an
    application's end-to-end journey compares it with the compositor's
    list after each step. *Test:* a step that leaves a window behind
    fails naming it.

## Next candidates

After windows, each its own later slice set, in the order an
application's bugs ask for:

- **Plugins connecting and disconnecting**
  ([ADR 0028](0028-removed-contributor-work-in-flight.md) already
  journals `plugin.disconnected`): `remove_owner` on every registry is
  the same remembering function; each registry becomes a listener.
- **Jobs**: moved (2026-11-16). `job.cancel`, `job.pause`,
  `job.resume` and `job.retry` are requests whose listeners are
  `jobs.model` then `jobs.host`; the jobs model is the `jobs.model`
  listener of the `job.*` events, which the bus delivers once each, in
  journal order; cancel-on-disconnect and the return of a plugin's
  jobs are listeners of `plugin.disconnect` (asked before
  `plugin.disconnected` is said) and `plugin.connect`. Forgetting a
  finished or orphaned job (`JobsService::dismiss`, `clear_finished`)
  is still a direct call, the jobs view's own list.
- **Panels and layout**: the workbench's changes are steps already;
  their followers (layout repair, persistence, focus) are calls.
- **Selection**: has subscribers of its own (`selection.rs`), beside
  the journal.
- **Settings changes**: value listeners per path
  ([ADR 0016](0016-one-writer-per-value.md)); what follows a change
  (theme install, scale, fonts) is called from the main pass.

## Consequences

- `egui-pop-api` depends on `egui-pop-hooks`; the kernel's contract
  page says who uses it. The kernel gains "last" and nothing else.
- Steps' after-hooks on bus points get names and declared order;
  elsewhere they keep registration order until their family moves.
- More journal events per window change; the journal's capacity (1024)
  holds, `window.painted` being said on change, not per pass.
- A module's tests no longer need its neighbours: it emits a request
  and asserts the events.
- Until slice 9, nothing forbids a new raw call: reviews do.
