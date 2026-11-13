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
- **Read-only by default.** When the application chose no bridge mode,
  turning the socket on makes the bridge `ReadOnly`: snapshots, actions
  that do not mutate and read-only commands. An application that offers
  more says so with `AppBuilder::debug_bridge(BridgeMode::On)`, before
  or after; the mode applies to every agent, not only the socket's.

## The protocol

JSON-RPC 2.0, one request per line and one response per line (UTF-8
JSON, `\n`-terminated). A request without `id` is a notification: run,
not answered. Every call is made as `Origin::Agent("agent-socket")`, so
the bridge's mode, the `agent` flags, the `debug.call` step and the
journal apply as to any agent.

| Method | Params | Result |
|---|---|---|
| `describe` | none | `bridge.describe`: `{ "protocol": 1, "capabilities": [..] }` ([Capabilities](agents.md#capabilities)) |
| `snapshot` | `{ "collector"? }` | every collector's snapshot by name, or one collector's |
| `call` | `{ "collector", "action", "args"? }` | the bridge's feedback block: `{ "ok", "collector", "action", "mutates", "message", "data" }`, a refusal included (`ok: false`) |
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
`snapshot` or `wait`; `data` is the feedback block, when there is one), `-32002` (the application did
not answer: it ran no frame, or it closed), `-32003` (`ui` found no
node, or more than one, to act on).

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"wait","params":{"settled":true}}' \
    | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/egui-pop-agent-org.example.App.sock
{"id":1,"jsonrpc":"2.0","result":{"by":"settled","event":null,"last":42,"met":true}}
```

When the bridge is `Off`, `describe`, `snapshot`, `wait` and every
`call` are refused: nothing but the user reads or drives it.

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
  object, and `readOnlyHint` from `mutates`.
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
- **Windows** is compile-checked only; the pipe keeps the system's
  default access (the user, administrators and the system).
