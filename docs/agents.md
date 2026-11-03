# Agents and the debug bridge

A command "yields its agent surface" ([CREDO](../CREDO.md)): unless it
says otherwise, every command an application declares can be run by an
agent through the **debug bridge**, with the arguments its spec
describes, and the run is marked as the agent's. This page says what
that bridge is, how an agent reaches it, what it can and cannot do, and
what is not protected today.

In short: the bridge is a Rust API inside the application's process.
egui-pop opens no socket and no port for it. An agent is code that holds
the `App` (a test, a harness, a transport the application writes
itself); it discovers the collectors and commands as JSON, runs the
commands offered to agents, reads every collector's state, and calls
the collector actions offered to agents. What keeps an action away from
it is the command's or the collector action's `agent` flag, the
command's `when` clause, step hooks (`debug.call` for every collector
action) and the host; the [known gaps](#known-gaps) list what nothing
checks.

## What an agent connects to

**No transport.** The bridge (`egui_pop::core::debug::DebugBridge`) is a
list of *collectors* held by each `App`. Nothing in egui-pop listens on
a TCP port, a local socket or a pipe for it, and there is no address to
connect to, loopback or not. It is reached by Rust calls:

| Call | What it does |
|---|---|
| `AppHandle::debug_describe()` | every collector: name, description, actions (each with `mutates`, `agent` and an example of its arguments) |
| `AppHandle::debug_snapshot()` | every collector's snapshot, by name, as one JSON object |
| `AppHandle::debug_call(collector, action, args)` | calls an action as the agent (`debug-bridge`); always returns a `Feedback`; `commands.run` is queued for the next frame |
| `App::debug_call(collector, action, args)` | the same, but `commands.run` runs at once and its result is in the feedback |
| `AppHandle::debug_call_as(collector, action, args, origin)`, `App::debug_call_as` | the same for another caller: the user's calls (the devtools panel, a debug page) pass `Origin::User` |
| `AppHandle::debug_log()` | the last 64 feedback blocks, refusals included |

**Always there.** Every `App` builds its bridge with the core collectors
and registers its own and its plugins' when it is built. There is no
Cargo feature, setting or build kind that removes it: release builds
have it, as debug builds do.

**Devtools is a view, not a gate.** `DevtoolsPlugin` (opt-in) shows the
same collectors in a panel, opened with `F12` once the
`developer.devtools` setting is on (off by default). The setting gates
the panel only; the bridge answers Rust calls whatever its value. The
panel is the user's: an action run from it goes through the
`devtools.call` step, then, as the user, through the `debug.call` step
every collector action goes through; a plugin may veto either, and a
veto of `debug.call` covers the panel and agents alike. See
[Debugging](debugging.md).

**Reaching it from another process** takes a transport the application
writes (a socket, stdin, an MCP server...) that turns requests into
`debug_call`s on the UI thread. egui-pop provides none. The ways out of
the process that do exist are not the bridge:

- the gallery's `--dump-debug` prints the bridge's snapshot as one JSON
  line on stdout when it quits by `--quit-after` (read only, the gallery
  only; see [tools/wbox](../tools/wbox));
- [tools/wbox](../tools/wbox) drives a real gallery window in a nested,
  headless compositor: screenshots, clicks and keys, as a person would;
- the single-instance plugin of `egui-pop-platform`
  ([instance.md](instance.md)) listens on a local socket for later
  launches of the same application; what they hand over (arguments,
  working directory, links) reaches the `instance.second_launch` and
  `deeplink.open` steps, not the bridge.

## What it can do

### Discover

`debug_describe()` lists the collectors. One entry, as the gallery
answers it:

```json
{
  "name": "store",
  "description": "The value store: every path, its value, its owner, who may write it and who changed it.",
  "actions": [
    { "name": "get", "description": "Reads a value with its descriptor.", "mutates": false, "agent": true, "args": { "path": "settings/x" } },
    { "name": "set", "description": "Writes a value (`path`, `value`) as its caller: the user through the framework, anyone else as itself, held to the value's writers (`Origin::writer`); for the user only unless offered (agents run the `value.set` command).", "mutates": true, "agent": false, "args": { "path": "settings/x", "value": 1 } },
    { "name": "undo", "description": "Undoes the last change of values as its caller, refused when it may not write one of them; for the user only unless offered (agents run the `value.undo` command).", "mutates": true, "agent": false, "args": null }
  ]
}
```

The `commands` collector's snapshot lists every command with what an
agent needs to run it: `id`, `title`, `category`, `description`, `when`
(where it applies), `keys`, `agent` (offered to agents or not), `args`
(a description of its arguments: free JSON, a schema or an example,
`null` when it takes none), `running`, `owner` and its icons; beside
them, the `context` (the context keys in force) and `history` (the last
runs). In the gallery:

```json
{
  "id": "about.read",
  "title": "Read about the application",
  "category": "Help",
  "description": "The application's name, version, description, licences, authors and links, egui-pop's and egui's versions, the platform, the renderer and the version info as text.",
  "when": "",
  "keys": [],
  "agent": true,
  "args": null,
  "running": false,
  "owner": "egui_pop.about",
  "icon": null,
  "icon_on": null,
  "icon_set_by": null,
  "unknown_icons": []
}
```

### Run a command

`commands.run` takes `{ "id", "args" }`. The bridge checks that the
command exists and is offered to agents, then runs it with
`Origin::Agent("debug-bridge")`. A collector action acts as its caller:
called by the user (the devtools panel, `debug_call_as` with
`Origin::User`), `commands.run` runs the command as the user, a command
kept from agents included. The reply is a `Feedback`, the same
shape for every call, success or not:

| Field | Meaning |
|---|---|
| `ok` | the call succeeded |
| `collector`, `action` | what was called |
| `mutates` | the action is one that changes the application (whether or not it did) |
| `message` | what happened, in a sentence |
| `data` | the result, `null` on failure |

Through `App::debug_call`, `about.read` in the gallery answers:

```json
{
  "ok": true,
  "collector": "commands",
  "action": "run",
  "mutates": true,
  "message": "`about.read` ran; the application may have changed",
  "data": {
    "authors": ["The egui-pop contributors"],
    "description": "egui-pop's components in every variant, state, theme and density.",
    "egui": "0.36",
    "egui_pop": "0.2.0",
    "licences": ["MIT OR Apache-2.0"],
    "links": {
      "docs": "https://quazardous.github.io/egui-pop/docs/",
      "issues": "https://github.com/quazardous/egui-pop/issues",
      "repository": "https://github.com/quazardous/egui-pop",
      "website": "https://quazardous.github.io/egui-pop/"
    },
    "name": "egui-pop gallery",
    "platform": "linux x86_64",
    "renderer": null,
    "version": "0.2.0 (59ba7f9f)",
    "version_info": "egui-pop gallery 0.2.0 (59ba7f9f)\negui-pop 0.2.0\negui 0.36\nPlatform: linux x86_64\n"
  }
}
```

Through `AppHandle::debug_call` the run is only queued:

```json
{ "ok": true, "collector": "commands", "action": "run", "mutates": true, "message": "`commands.run` done; the application changed", "data": { "queued": true } }
```

A command whose host answers later (`Reply::Later`) gives
`"data": { "pending": true }` and the message "`<id>` is running; its
result goes to the journal".

### Read state and results

- Every collector's snapshot: focus and the focus chain, overlays,
  fields (each input's label, kind and state), indicators, the
  announcements made to screen readers, the theme, the store (every
  path, value, owner, writers, last writer), the context stack, dialogs,
  file choices, jobs, the workbench, the windows' scale, and those of the
  plugins connected (about 50 in the gallery).
- The value store also as commands: `value.list`, `value.get`,
  `value.set`, `value.reset`, `value.undo`, `value.redo`.
- The journal: `journal.since` with `{ "seq": n }` returns the events
  after `n`. Each run of a command leaves a `command.dispatched` event
  and a `command.done` (or `command.failed`) event whose `cause` is the
  dispatch:

```json
[
  { "seq": 1, "source": "agent:debug-bridge", "cause": null, "name": "command.dispatched", "data": { "command": "about.read", "args": null } },
  { "seq": 2, "source": "agent:debug-bridge", "cause": 1, "name": "command.done", "data": { "command": "about.read", "result": { "name": "egui-pop gallery" } } }
]
```

(The second event's `result` is cut here; it is the whole answer shown
above.)

### Act through collector actions

Besides `commands.run`, collectors offer actions of their own. Each
says whether agents may call it (`agent`): by default those that only
read, and none that writes. In the gallery, agents may call
`commands.run` (the command's own `agent` flag is checked) and the
read-only actions: `state.slot`, `fonts.check`, `icons.search`,
`journal.since`, `store.get`, `mounts.where_can`, `mounts.check`,
`frame.hit` and the gallery's `gallery.viewport.wait_idle`. The actions
that write are the user's (the devtools panel, a debug page):
`screen.set_interface_zoom`, `focus.clear`, `overlays.close_top`,
`announcements.announce`, `plugins.set_enabled`, `store.set`,
`store.undo`, `i18n.set_locale`, `i18n.clear_missing`,
`contexts.request`, `contexts.hold`, `contexts.release`,
`dialogs.answer`, `files.answer`, the `jobs` actions (`cancel`, `pause`,
`resume`, `retry`, `dismiss`, `clear`), `toasts.dismiss` and
`viewport.set_view`. Most have a command an agent runs instead
(`value.set`, `value.undo`, `job.cancel`, `context.hold`...).

An agent's call to an action not offered to it is refused before
anything runs, in the bridge's log and on the journal:

```json
{ "ok": false, "collector": "store", "action": "set", "mutates": true, "message": "`store.set` refused: it is not offered to agents; only the user may call it", "data": null }
```

```json
{ "seq": 7, "source": "agent:debug-bridge", "cause": null, "name": "debug.call.refused", "data": { "collector": "store", "action": "set", "args": { "path": "settings/x", "value": 1 }, "reason": "it is not offered to agents; only the user may call it" } }
```

Every other call, the user's included, runs through the `debug.call`
step (`{ "collector", "action", "args", "origin" }`), whose
before-hooks may veto it ("`journal.since` vetoed by kiosk: ...",
`debug.call.vetoed` on the journal). A call that writes is published as
`debug.call`; one that only reads is published and traced only when
vetoed, so polling fills neither the journal nor the steps' traces.

An application may offer a writing action to agents it trusts with it
(a test harness, its own automation): `AppBuilder::offer_to_agents`
(`"dialogs.answer"`); a name that matches no action is reported
(`agent-offer-unknown`). A collector declares its own with
`FnCollector::agent`.

### Wait

There is no blocking wait on the bridge. An agent polls: `journal.since`
for a command's `command.done`, the `steps` collector for the last runs
of each step (who vetoed, redirected, ran), the `jobs` collector's
`idle`. An application may add its own (the gallery's
`gallery.viewport.wait_idle`).

### Screenshots and the accessibility tree

Not through the bridge. In tests, egui_kittest renders frames and
`egui_pop_test::a11y_tree` prints the accessibility tree
([Testing](testing.md)); for a real window, wbox takes screenshots. The
bridge gives the same information as data: the `fields`, `focus`,
`indicators` and `announcements` collectors.

## An example, end to end

An application with two commands, one kept off the agent surface. The
agent discovers the bridge, lists the commands offered to it, runs one,
reads its result and the journal, and is refused the other. This block
is compiled and run with the guides (`cargo test -p consumer-smoke
--doc`).

```rust
use egui_pop::egui;
use egui_pop::prelude::*;
use serde_json::{Value as Json, json};

struct Notes;

impl Plugin for Notes {
    fn id(&self) -> PluginId {
        PluginId::from_static("notes")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CommandId::from_static("notes.count"), "Count notes", "Notes")
                .description("How many notes a folder holds (`folder`).")
                .args(json!({ "folder": "inbox" })),
            |call| Ok(json!({ "folder": call.args["folder"], "count": 3 })),
        )?;
        // Never run by an agent.
        ctx.command(
            CommandSpec::new(CommandId::from_static("notes.purge"), "Purge notes", "Notes")
                .no_agent(),
            |_| Ok(Json::Null),
        )?;
        Ok(())
    }
}

let ctx = egui::Context::default();
let mut app = App::builder()
    .plugins(DefaultPlugins)
    .plugin(Notes)
    .build(&ctx)
    .unwrap();

// 1. Discover the collectors and their actions.
let described = app.handle().debug_describe();
let commands = described
    .as_array()
    .unwrap()
    .iter()
    .find(|c| c["name"] == "commands")
    .unwrap();
assert_eq!(commands["actions"][0]["name"], "run");
assert_eq!(commands["actions"][0]["mutates"], true);

// 2. The commands offered to agents, with their arguments.
let snapshot = app.handle().debug_snapshot();
let offered: Vec<&Json> = snapshot["commands"]["commands"]
    .as_array()
    .unwrap()
    .iter()
    .filter(|c| c["agent"] == true)
    .collect();
let count = offered.iter().find(|c| c["id"] == "notes.count").unwrap();
assert_eq!(count["args"], json!({ "folder": "inbox" }));
assert!(!offered.iter().any(|c| c["id"] == "notes.purge"));

// 3. Run one: `App::debug_call` runs it now and returns its result.
let before = app.handle().last_event();
let f = app.debug_call(
    "commands",
    "run",
    &json!({ "id": "notes.count", "args": { "folder": "inbox" } }),
);
assert!(f.ok, "{}", f.message);
assert_eq!(f.data, json!({ "folder": "inbox", "count": 3 }));

// 4. The journal says the agent did it.
let events = app.debug_call("journal", "since", &json!({ "seq": before }));
let done = events
    .data
    .as_array()
    .unwrap()
    .iter()
    .find(|e| e["name"] == "command.done")
    .unwrap();
assert_eq!(done["source"], "agent:debug-bridge");
assert_eq!(done["data"]["result"]["count"], 3);

// 5. The command kept off the surface is refused, and says why.
let f = app.debug_call("commands", "run", &json!({ "id": "notes.purge" }));
assert!(!f.ok);
assert_eq!(
    f.message,
    "`commands.run` failed: `notes.purge` is not offered to agents"
);
```

The JSON replies shown on this page were captured from the gallery
itself, headless: a test built it with `Gallery::new` on a plain
`egui::Context` (no window, no GPU), ran one frame of `App::logic`, and
printed `debug_describe`, the `commands` snapshot and the feedback of
`commands.run` (`about.read`, `app.quit`), `journal.since`, `store.get`
and `store.set`.

## What it cannot do, and the trust model

The bridge has one caller identity: every command it runs is
`Origin::Agent("debug-bridge")`, and the journal's source is
`agent:debug-bridge`. What is checked:

- **Collector actions' `agent` flag.** An action that writes is the
  user's unless it says otherwise (only `commands.run` does) or the
  application offers it (`AppBuilder::offer_to_agents`): the agent's
  call is refused and journaled (`debug.call.refused`). Every call that
  goes on, the agent's and the user's, runs through the `debug.call`
  step a plugin may veto (a kiosk build).
- **The `agent` flag.** `commands.run` refuses a command declared with
  `CommandSpec::no_agent()` (and an unknown one) with "`<id>` is not
  offered to agents". In egui-pop and the gallery: quitting, the
  window's close, minimise, maximise and menu, opening devtools,
  `deeplink.open`, `links.confirmed` (the confirmed link), the pointer
  gestures, `viewer.load_images`, and a few gallery commands. 31 of the
  gallery's 259 commands are not offered.
- **Where a command applies.** A command whose `when` clause does not
  hold in the current context is refused ("does not apply here"), for an
  agent as for a key.
- **Links.** A link an agent asks to open (`links.open`, `files.reveal`,
  `viewer.open_outside`) is always confirmed by the user, and the links
  plugin vetoes any answer to that confirmation but the user's: an agent
  may cancel it, not accept it ([Links](links.md)).
- **The clipboard.** An agent copies and pastes in its own slot
  (`agent:debug-bridge`): it neither reads nor overwrites what the user
  copied unless the owner allows it ([The clipboard](clipboard.md)).
- **The chain of commands.** A command's handler runs for its origin
  (`Call::origin`, `AppHandle::acting_origin`), and the commands it
  starts with `AppHandle::run` run for the same origin: an agent's
  command starts the agent's commands. While a command runs for anyone
  but the user, its handler may not run another as the user
  (`AppHandle::run_as` with `Origin::User` is refused, `command.refused`
  on the journal); one running for an agent may only name an agent. To
  act for the user, the handler asks the user: a dialog whose answer,
  the user's, runs its command as the user. `App::run` and
  `AppHandle::run_as` refuse a command not offered to agents to an
  agent origin, wherever it comes from (`command.refused`).
- **Deep links.** A link that comes with a launch, the first one's or a
  later one's through the single-instance socket, runs the
  application's links command as `Origin::Remote("deeplink")`
  (`instance::deeplink_origin`), not as the user: any program of the
  user's may launch the application with a link. The handler sees it,
  what it starts inherits it, it may not claim the user, and its writes
  are held to the values' writers. A handler that changes anything on a
  link asks the user first ([Instance](instance.md)).
- **Dialogs and file choices.** `dialogs.answer` and `files.answer`
  answer as their caller. Accepting a dialog runs its command with the
  caller's origin, and an answer that would run a command not offered to
  agents is refused to anyone but the user, even when the application
  offers `dialogs.answer` (`dialog.answer.refused` on the journal); an
  agent may still cancel it. A file choice carries its origin to its
  consumer (`files::chosen_origin`).
- **The value store.** An agent writes as itself
  (`agent:debug-bridge`, `Origin::writer`), through `value.set`,
  `value.reset`, `value.undo`, `value.redo` and, when offered,
  `store.set` and `store.undo`: a value's declared writers
  ([ADR 0016](adr/0016-one-writer-per-value.md)) apply, so it is
  refused a setting only its owner writes (`settings/developer.devtools`)
  and the undo of a change to one. To let it write a value, name it
  among the value's writers (`Descriptor::writers`). The user writes
  through the framework, as the settings form does. A write is also
  refused when the value is held by someone else's gesture, changed
  since `if_seq`, or invalid for its type; past the soft range only with
  `force`.
- **Steps.** Every step's before-hooks see its arguments and may veto
  it; steps that carry an `origin` (`dialog.answer`, `terminal.input`
  among them) let a hook refuse agents only.
- **The host.** Every command dispatched to the host carries its origin
  (`Dispatch::origin`): a host may refuse agents there, in one place.

### Keeping an action off the agent surface

- Declare the command with `.no_agent()`; it stays in menus, keys and
  the palette.
- A dialog whose command is declared `.no_agent()` is accepted by the
  user only, even when the application offers `dialogs.answer` to
  agents. When a dialog confirms an action whose command agents may run
  otherwise, veto the `dialog.answer` step unless its `origin` is
  `user`, as the links plugin does.
- To refuse a collector action to everyone, the user's devtools panel
  included, veto the `debug.call` step for it.
- When a step does the work, veto it from a before-hook for agents
  (the origin is in the arguments when the step carries it).
- In the host's `dispatch`, refuse `Origin::Agent` for the commands it
  should never run for one. This does not cover the commands run in the
  interface without the host's dispatch (undo and redo, `history.*`,
  the context stack's commands, the clipboard's), which take the two
  ways above.

```rust
use egui_pop::api::dialogs::STEP_ANSWER;
use egui_pop::api::host::{Dispatch, Host, Journal, LocalHost, Reply, Waker};
use egui_pop::egui;
use egui_pop::prelude::*;
use serde_json::{Value as Json, json};

/// The default host, refusing agents the `notes.` commands.
struct Guarded(LocalHost);

impl Host for Guarded {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        if matches!(d.origin, Origin::Agent(_)) && d.command.as_str().starts_with("notes.") {
            return Reply::Done(Err(format!("`{}` is not for agents", d.command)));
        }
        self.0.dispatch(d)
    }
    fn journal(&self) -> Option<&Journal> {
        self.0.journal()
    }
    fn journal_mut(&mut self) -> Option<&mut Journal> {
        self.0.journal_mut()
    }
    fn store(&self) -> SharedStore {
        self.0.store()
    }
    fn set_waker(&mut self, waker: Waker) {
        self.0.set_waker(waker);
    }
    fn layout_document(&self) -> Option<String> {
        self.0.layout_document()
    }
}

struct Notes;

impl Plugin for Notes {
    fn id(&self) -> PluginId {
        PluginId::from_static("notes")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CommandId::from_static("notes.count"), "Count notes", "Notes"),
            |_| Ok(json!(3)),
        )?;
        // Only the user accepts a dialog; an agent may still cancel one.
        ctx.before(STEP_ANSWER, |_, args| {
            let cancel = args["answer"]["answer"] == "cancel";
            if !cancel && args["origin"] != "user" {
                Before::Veto("only the user answers dialogs".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

let mut app = App::builder()
    .host(Guarded(LocalHost::new(SharedStore::default())))
    .plugins(DefaultPlugins)
    .plugin(Notes)
    .build(&egui::Context::default())
    .unwrap();
let count = CommandId::from_static("notes.count");
let f = app.debug_call("commands", "run", &json!({ "id": "notes.count" }));
assert!(!f.ok);
assert!(f.message.contains("is not for agents"), "{}", f.message);
// The user still runs it.
assert_eq!(app.run(&count, Json::Null, Origin::User).now(), Some(Ok(json!(3))));
```

## Known gaps

What is not protected today. None of these is fixed by documenting it.

- **No transport, no authentication.** egui-pop has no notion of who
  the agent is: every bridge call is `debug-bridge`. An application that
  exposes the bridge over a socket or a pipe gives whoever reaches it
  everything below; any local process able to connect to that transport
  is an agent. Access control is the transport's, and egui-pop offers no
  helper for it.
- **The bridge cannot be turned off.** It is in every build, release
  included; there is no feature or setting to remove it or to make it
  read-only. Any code in the process (a plugin included) can call it.
- **An offered `files.answer` answers a choice with any paths.** The
  origin goes with the choice (`files::chosen_origin` on its result,
  `origin` in the `file.chosen` step, `by` in the `files` collector's
  last choices), but nothing refuses it: a consumer that acts only on a
  person's choice checks it, or a hook on `file.chosen` vetoes it.
- **Origin is a label, not a credential.** Code in the process names
  any `Origin` outside a command's handler: a plugin's own code, a
  step's hook or a timer may still run a command as the user. Only the
  chain from a command running for someone else is held (above). The
  origin is not carried into work a handler leaves for later (a
  thread, a host's `Reply::Later` that starts other commands): code
  there names the origin itself, with `AppHandle::run_as`.
- **Commands that reach the system are offered by default.**
  `terminal.send` writes to a terminal's program: with a pseudo-terminal
  shell open (the `editors-pty` feature, as in the gallery), an agent
  types into the user's shell without a confirmation, unless the
  application vetoes `terminal.input` for agents. Every new command is
  offered until it says `.no_agent()`.
- **The host's dispatch does not see every command.** Undo, redo,
  `history.*`, the context stack's commands and the clipboard's run in
  the interface without `Host::dispatch`, so a guard there does not see
  them.
- **Snapshots are not redacted.** Every value in the store, the file
  choices and their folders, the terminal steps with their arguments
  (`terminal_io`), the journal's arguments and results are readable
  through the bridge.
- **`mutates` is per action, not per command.** `commands.run` is
  always flagged `mutates`, read-only commands included (`about.read`,
  `value.get`); commands carry no read-only flag an agent could rely on.
- **No wait and no rate limit.** An agent polls; nothing limits how
  often it calls. A refused `commands.run` is in the bridge's log
  (`debug_log`, the last 64 calls), not in the journal.
