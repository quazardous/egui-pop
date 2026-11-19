# The agent socket

`egui-pop-agent` serves the [debug bridge](agents.md) out of the
process, so an agent (a script, an MCP client, a test runner) can drive
a running application: list its capabilities, read its state, call its
actions and commands, and wait for what they cause. It is the local
socket transport of the [agent control design](design/agent-control.md).

## Turning it on

Nothing listens unless the application asks for it:

```rust,ignore
use egui_pop_agent::{AgentSocket, AgentSocketExt as _};

let builder = App::builder()
    .app_id("org.example.App")
    // Always, at the default place:
    // .agent_socket(AgentSocket::new())
    // Or only when the user asks: EGUI_POP_AGENT_SOCKET=1 (the default
    // place) or EGUI_POP_AGENT_SOCKET=/path/to/app.sock.
    .agent_socket_from_env();
```

### From a setting

An application that lets its users turn the socket on from its settings
window uses `agent_socket_from_env_or_setting()`:

```rust,ignore
let builder = App::builder()
    .app_id("org.example.App")
    .agent_socket_from_env_or_setting();
```

It declares the `developer.agentSocket` setting (Developer section, off
by default; "Lets local programs drive this app (read-only unless the
app allows more)"). The socket follows it live: it starts, at the
default place, when the setting turns on, and stops, its file removed,
when it turns off; a client still connected then has its requests
refused (`-32001`, "the agent socket is off"). `EGUI_POP_AGENT_SOCKET`
still turns it on (at its path, when it gives one) whatever the setting
says. `agent_socket_on_setting(AgentSocket::at(path))` follows the
setting at another place. The bridge is made read-only at build when the
application chose no mode, the setting on or off.

- **Where.** By default `egui-pop-agent-<app id>.sock` in
  `$XDG_RUNTIME_DIR` (`$TMPDIR` on macOS); on Windows the named pipe
  `\\.\pipe\egui-pop-agent-<app id>`. The path is logged once, at start
  (`egui_pop_agent` target).
- **Private or not at all.** The socket's directory must not be
  writable by group or others, and the socket is made mode 0600; when
  either fails, or another instance answers on the path, the socket is
  not started and the reason is logged (the application still starts).
  A socket file left by an instance that is gone is replaced.
- **The user's own processes only.** On Linux and macOS, a client whose
  process runs as another user (the socket's peer credentials:
  `SO_PEERCRED`, `getpeereid`) is answered `-32001` and closed before
  any request; on a platform without peer credentials, every client is.
  On Windows, the pipe's access list (SDDL `D:P(A;;GA;;;OW)(A;;GA;;;SY)`)
  lets in only its owner, the user the application runs as, and the
  system; remote clients are rejected, and the first instance is created
  with `FILE_FLAG_FIRST_PIPE_INSTANCE`, so a pipe another process made
  first under the name is never served. A client that does not say
  [`hello`](#identity-hello-and-pairing) is the agent
  `Origin::Agent("agent-socket")`.
- **Limits.** At most 8 clients at once (`MAX_CONNECTIONS`): one more is
  answered `-32004` and closed. A request line longer than 1 MiB
  (`MAX_REQUEST_BYTES`) is answered `-32004` and its connection closed.
  A connection serves one request at a time, so at most 8 waits are
  held at once, each 120 s at most.
- **Read-only by default.** When the application chose no bridge mode,
  turning the socket on makes the bridge `ReadOnly`: snapshots, actions
  that do not mutate and read-only commands. An application that offers
  more says so with `AppBuilder::debug_bridge(BridgeMode::On)`, before
  or after; the mode applies to every agent, not only the socket's.
- **Unpaired clients read only.** Whatever the mode, a client that does
  not say `hello` is held to the read-only grant
  ([Grants](#grants-what-each-agent-may-do)); a paired agent to the
  editor grant unless the application says otherwise.

## The protocol

JSON-RPC 2.0, one request per line and one response per line (UTF-8
JSON, `\n`-terminated). A request without `id` is a notification: run,
not answered. A connection's calls are made as
`Origin::Agent("agent-socket")`, or, once it said `hello`, as its paired
agent, `Origin::Agent("<id>")`; either way the bridge's mode, the
`agent` flags, the `debug.call` step and the journal apply as to any
agent.

| Method | Params | Result |
|---|---|---|
| `hello` | `{ "name", "token"?, "deadline_ms"? }` | `{ "agent", "name", "origin", "paired": true, "token"? }`: see [Identity](#identity-hello-and-pairing) |
| `describe` | none | `bridge.describe`: `{ "protocol": 1, "capabilities": [..] }` ([Capabilities](agents.md#capabilities)) |
| `snapshot` | `{ "collector"? }` | every collector's snapshot by name, or one collector's |
| `call` | `{ "collector", "action", "args"?, "deadline_ms"? }` | the bridge's feedback block: `{ "ok", "collector", "action", "mutates", "message", "data" }`, a refusal included (`ok: false`); a destructive call is held until the user confirms it, see [Destructive calls](#destructive-calls-the-user-confirms) |
| `wait` | `bridge.wait`'s arguments (`event` and `cause`, `settled: true`, or `any`; `since`) and `deadline_ms` (5 s by default, 120 s at most) | `{ "met", "by", "event", "last" }`; `met: false` at the deadline |
| `ui` | `{ "action": "tree", "label"?, "role"?, "window"? }` | `{ "count", "nodes" }`: the accessibility tree's nodes (`id`, `parent`, `role`, `label`, `value`, `rect` in points, `disabled`), every one or those matching |
| `ui` | `{ "action": "click" \| "hover" \| "secondary_click" \| "type" \| "scroll", "label"?, "role"?, "nth"?, "text"?, "up"?, "window"? }` | `{ "node" }`, once the input went in |
| `ui` | `{ "action": "key", "key": "Ctrl+K", "window"? }` | `{ "key" }` |

**The interface.** `ui` reads the AccessKit tree of the frame just
drawn (AccessKit is turned on at the first `ui` request) and acts on it
as a person does: a node is found by its `label` (exactly; a text
label's text) and its `role` (`Button`, `TextInput`, `Window`, any
case), and an action needs exactly one, or `nth` picks one of several
in reading order (top to bottom, then left to right, from 0). `click` hovers the node's
centre, presses and releases, a frame each; `hover` moves the pointer
over its centre and leaves it there, so egui shows its tooltip after its
delay (read the tree for it); `secondary_click` is a click with the
secondary (right) button, which opens a context menu; `type` clicks it, then
types `text`; `scroll` turns the wheel over it, down unless `up`; `key`
presses and releases a key with its modifiers (`Ctrl`, `Shift`, `Alt`,
`Cmd`). The events go into egui's raw input through an egui plugin
(`input_hook`), so they work the same in a kittest harness, headless in
a nested compositor and in a window on a desktop. `tree` needs the
bridge on or read-only; the actions change the application, so they
need it on, and are refused when it is read-only. A node not found, or
found twice, is error `-32003`, with the reason.

`window` names the application window `ui` reads and acts in (its id,
`main` for the main window; [ADR 0025](adr/0025-application-windows.md)):
an OS window of its own has its own tree and its own input. Without
it, the main window, with what egui embeds in it. Where egui embeds the
application windows in the main one (the web, a kittest harness), a
window is its embedded `Window` node (found by its title) and what is
in it, and `main` is the main window without them. In a window of its
own, the root node (a `Window` node egui leaves unnamed) takes the
window's title, so an application window's tree has a `Window` node
named by its title either way. A window that is
not open is error `-32602`, naming the open ones. An action that closes
its window (`Escape`, a close button) is answered once the window is
gone, with `"window_closed": true`; the events left for it are dropped,
and the application keeps running.

`commands.run` through `call` is queued for the next frame, as
`AppHandle::debug_call` does: its result comes on the journal. Wait for
it with `wait { "event": "command.done", "since": <last> }`, `last` from
an earlier `wait`.

Errors: `-32700` (not JSON), `-32600` (not a request), `-32601` (no such
method), `-32602` (bad params), `-32001` (the bridge refused `describe`,
`snapshot` or `wait`, `data` being the feedback block when there is
one; or the user declined a pairing or a destructive call, or a token
is unknown), `-32002` (the application did not answer: it ran no frame,
or it closed; or the user did not answer a pairing or a destructive
call in time), `-32003` (`ui` found no
node, or more than one, to act on), `-32004` (a transport limit: too
many clients, a request too long; the connection closes).

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"wait","params":{"settled":true}}' \
    | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/egui-pop-agent-org.example.App.sock
{"id":1,"jsonrpc":"2.0","result":{"by":"settled","event":null,"last":42,"met":true}}
```

When the bridge is `Off`, `hello`, `describe`, `snapshot`, `wait` and
every `call` are refused: nothing but the user reads or drives it.

## Identity: hello and pairing

An agent names itself once per connection, with `hello`; from then on
the connection's calls run as `Origin::Agent("<id>")`, so the journal,
the histories and the bridge's log tell agents apart. A connection that
never says `hello` keeps working as before, as
`Origin::Agent("agent-socket")`, with the same rights.

- **The first time**, `hello { "name": "Claude Code" }` asks the user:
  a confirmation dialog, "Pair the agent “Claude Code”?", says which id
  it would act as. The request is held until the user answers, as a
  `wait` is (`deadline_ms`, 120 s by default and at most). Accepted,
  the answer carries the agent's `id` (`claude-code`: the name in lower
  case, `[a-z0-9._-]`, 32 characters at most, a number added when the
  id is taken or reserved: `agent-socket`, `anonymous`, `user`,
  `debug-bridge`) and its `token`, given this once. Declined, it is
  error `-32001`; unanswered at the deadline, `-32002` (the dialog stays
  open, and accepting it then pairs nothing).
- **The next times**, `hello { "name": "Claude Code", "token": "egp_…" }`
  is paired at once; an unknown token is error `-32001`, and the agent
  asks again without one.
- **Only the user pairs.** The dialog's answer runs the command
  `pop.agent.confirm` (`egui_pop_agent::CMD_CONFIRM`), hidden and kept
  from agents; an agent's accept through `dialogs.answer` is vetoed,
  its cancel goes through.
- **Kept.** The pairings are written to `agent-pairings.json` in the
  application's persistence directory (`AppBuilder::persist`), with
  each token's SHA-256, never the token; without a directory they last
  as long as the application runs. `egui_pop_agent::pairings(app)`
  lists them.
- **One `hello` per connection**: a second one is error `-32602`.
- **The journal** says `agent.paired` (`agent`, `name`),
  `agent.declined` (`name`, `reason`), `agent.connected` (a `hello`
  that paired the connection) and `agent.disconnected` (a paired
  connection closed), from the agent's origin (`agent:<id>`; `user` for
  a decline).

A token says which agent calls; it does not isolate agents from one
another: every process of the user may read what another one keeps.
What each agent may do is its [grant](#grants-what-each-agent-may-do),
within the bridge's mode and the `agent` flags.

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"hello","params":{"name":"my script"}}' \
    | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/egui-pop-agent-org.example.App.sock
{"id":1,"jsonrpc":"2.0","result":{"agent":"my-script","name":"my script","origin":"agent:my-script","paired":true,"token":"egp_…"}}
```

## Destructive calls: the user confirms

A call that destroys or discards what cannot easily be had back waits
for the user: `commands.run` of a command declared
`CommandSpec::destructive()`, or an action its collector says is
(`Collector::destructive(action, args)`; `FnCollector::destructive`,
`FnCollector::destructive_when`). `describe` says it (`destructive` on
each capability; for an action, without arguments).

- **Held.** The agent's `call` opens a confirmation dialog, in the
  danger style: "Let “claude-code” run “files.trash”?", with the call's
  arguments (the command's own for `commands.run`, cut at 240
  characters). The request is held until the user answers, as `hello`
  is: `deadline_ms`, 60 s by default (`CONFIRM_DEADLINE`), 120 s at
  most (`MAX_CONFIRM_DEADLINE`).
- **Accepted**, the call runs as the agent asked it, with the agent's
  origin (the journal's `command.done` says `agent:<id>`, not `user`),
  and answers as any `call`.
- **Declined** (Cancel, or the dialog closed), it is error `-32001`;
  unanswered at the deadline, `-32002`. Either way the journal says
  `command.refused` (`command`, `origin`, `reason`, `by:
  "pop.agent.confirm"`) for `commands.run`, `debug.call.refused` (with
  the same `by`) for another action, from the agent's origin.
- **Only the user answers.** The dialog's answer runs
  `pop.agent.confirm` (`CMD_CONFIRM`, the pairing's), kept from agents;
  an agent's accept through `dialogs.answer` is vetoed, its cancel goes
  through.
- **Never asked**: the user's own calls and runs, a call that is not
  destructive, and a call the bridge refuses anyway (its mode, the
  `agent` flags): that one is refused at once, as before.

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"call","params":{"collector":"commands","action":"run","args":{"id":"files.trash","args":{"path":"a.jpg"}},"deadline_ms":30000}}' \
    | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/egui-pop-agent-org.example.App.sock
{"error":{"code":-32001,"message":"the user declined `files.trash`: it is destructive and needs the user's confirmation"},"id":1,"jsonrpc":"2.0"}
```

## Grants: what each agent may do

The application says what each of the socket's agents may do, with an
`egui_pop_agent::Grant` per agent:

| Field | Says |
|---|---|
| `commands` | the commands it may run: `*`, a prefix (`files.*`) or an id |
| `read_only` | only read-only commands and calls that change nothing |
| `values` | the value paths `value.set` and `value.reset` may write: `*`, a prefix (`settings/*`) or a path |
| `ui` | `ui` may act for it (click, type, keys, scroll); reading the tree is always allowed |

Three presets: `Grant::read_only()` (every command that reads, nothing
written), `Grant::editor()` (every command offered to agents, no value
written directly, no `ui` action) and `Grant::full()` (everything the
bridge's mode allows). `.commands([..])`, `.values([..])` and `.ui(..)`
narrow or widen one.

```rust
use egui_pop_agent::{AgentGrants, AgentGrantsExt as _, Grant};

let app = App::builder()
    .debug_bridge(BridgeMode::On)
    .agent_socket_from_env_or_setting()
    .agent_grants(
        AgentGrants::new()
            // A paired agent without a grant of its own (editor by default).
            .paired(Grant::editor().commands(["library.*", "about.read"]))
            // The paired agent `claude-code`.
            .agent("claude-code", Grant::full().ui(false)),
    );
```

- **Who gets which.** A client that said no `hello`
  (`Origin::Agent("agent-socket")`) gets `AgentGrants::unpaired`,
  read-only by default; a paired agent its own grant, or
  `AgentGrants::paired`, the editor by default. Other origins (the user,
  an in-process caller, a script) are not the socket's: grants say
  nothing about them.
- **Within the mode.** The bridge's mode stays the ceiling: under
  `ReadOnly` no grant writes, and destructive calls are still confirmed
  by the user ([Destructive calls](#destructive-calls-the-user-confirms)).
- **Enforced** fail-closed, twice. The socket checks each `call` and each
  `ui` action first, so the agent is answered `-32001` at once, before
  anything runs or is asked (a call the bridge refuses anyway, its mode
  or an `agent` flag, is answered by the bridge as before). And a listener named `pop.agent.grants`
  (`egui_pop_agent::GRANTS`) on `pop.command.run` refuses any run its
  grant does not allow, whatever path it took. It has priority
  `grants::PRIORITY` (10 000): it runs before the application's own
  listeners (`helios.policy`, for one), which see only what the grants
  let through; one that must run first says
  `ListenOpts::new().before(GRANTS)`.
- **On the journal**, a refusal is `command.refused` (a command) or
  `debug.call.refused` (another action, `ui` actions as collector `ui`),
  with `by: "pop.agent.grants"` and the reason, from the agent's origin.
- **Changed live** with `egui_pop_agent::grants::set_grants(app, ..)`;
  `grants::grants(app)` reads them, `grants::grant_of(app, origin)` the
  one an origin is held to.

**The escape hatch.** Before grants, an unpaired client wrote whatever
the mode allowed. A client that does not say `hello` yet (the MCP
binary, `egui_pop_test::scenario::live`, a script) and must write, for a
scenario run on a throwaway profile, needs the application to say so:

```rust
App::builder()
    .debug_bridge(BridgeMode::On)
    .agent_socket(AgentSocket::new())
    .agent_grants(AgentGrants::new().allow_unpaired_writes());
```

`allow_unpaired_writes` gives unpaired clients `Grant::full()`, today's
behaviour. Put it behind the same switch that turns the bridge `On`.

## MCP

With the `mcp` feature, the crate serves the socket as an MCP server
(on [`rmcp`](https://crates.io/crates/rmcp)): the binary
`egui-pop-agent-mcp <socket>` speaks MCP over stdio and the socket's
JSON-RPC to the application.

```console
$ cargo install --path crates/egui-pop-agent --features mcp
$ claude mcp add my-app -- egui-pop-agent-mcp $XDG_RUNTIME_DIR/egui-pop-agent-org.example.App.sock
```

- **Tools from `describe`.** `tools/list` asks the application's
  `describe` each time, so a capability added since is a tool with no
  code. Each capability offered to agents becomes a tool: its name
  sanitised for MCP (`about.read` becomes `about_read`; `[A-Za-z0-9_-]`,
  64 characters at most, a number added on a clash), its description,
  its input schema (the args' JSON Schema; without one, an object schema
  made from the example; a schema that is not an object's is taken as
  `args`), its output schema when it declares a result schema of an
  object, `readOnlyHint` from `mutates`, and `destructiveHint: true`
  for a destructive one (its description says the user confirms each
  run; an undeclared command keeps MCP's default).
- **Calls.** An action's tool is the socket's `call`: its `data` is the
  result (structured when there is an output schema), a refusal an error
  result (`isError: true`) the model reads. A command's tool runs it
  with `commands.run`, then waits for its `command.done` (or
  `command.failed`) on the journal, 10 s at most, and answers the
  command's result.
- **Built-ins.** `snapshot` (`collector`, optional), `wait` (the
  socket's `wait`, its params and its answer as structured content) and
  `ui` (the socket's `ui`: the tree, click, type, key, scroll).
  `bridge.wait`, which does not block, is not offered as a tool.
- **Off.** When `describe` is refused (the bridge is off) or the
  application does not answer, `tools/list` offers `snapshot` and
  `wait`, which are refused too.

In Rust, `egui_pop_agent::mcp::McpBridge` implements rmcp's
`ServerHandler` over a `Backend` (`SocketClient` for a socket), and
`mcp::tools` makes the tools from a `describe` reply.

## Threading

The socket's threads never touch the application. A request goes to the
UI thread through a channel that the application drains at each pass of
its logic (`Lifecycle::Pass`); the reply comes back on a one-shot
channel. The socket's thread asks for a repaint when a request arrives,
so a window that is idle answers at once.

A `wait` is held by the transport, never by the UI thread: it is checked
when it arrives, then again at each pass, until it holds or its deadline
passes; meanwhile a repaint is asked for every 10 ms. The repaints the
transport asks for are delayed ones, so they never count as the
application's own (`settled` stays reachable).

## Known gaps

- **No recording** of a session as a scenario yet.
- **No `ui.*`.** Tree, screenshots and input go through eframe's
  `inspection` feature ([inspection.md](inspection.md)), a second
  connection.
- **Windows** is compile-checked only.
- **Grants are the application's.** The user cannot change them yet
  (an Agents page comes later); a pairing cannot be revoked yet but by
  removing it from `agent-pairings.json`. The MCP binary does not say
  `hello` yet, so it reads only unless the application allows unpaired
  writes.
- **Confirmation through `ui`.** An agent allowed to act through `ui`
  (the bridge on, its grant's `ui`) can click the confirmation's button, which answers as
  the user; the input lock while a confirmation is open comes later.
- **A confirmation left open.** Past its deadline the dialog stays
  open; accepting it then runs nothing. Calls made in the application's
  own process (`AppHandle::debug_call`) are not asked: only the
  socket's are.
