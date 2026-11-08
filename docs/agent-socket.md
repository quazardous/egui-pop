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

`commands.run` through `call` is queued for the next frame, as
`AppHandle::debug_call` does: its result comes on the journal. Wait for
it with `wait { "event": "command.done", "since": <last> }`, `last` from
an earlier `wait`.

Errors: `-32700` (not JSON), `-32600` (not a request), `-32601` (no such
method), `-32602` (bad params), `-32001` (the bridge refused `describe`
or `wait`; `data` is the feedback block), `-32002` (the application did
not answer: it ran no frame, or it closed).

```console
$ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"wait","params":{"settled":true}}' \
    | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/egui-pop-agent-org.example.App.sock
{"id":1,"jsonrpc":"2.0","result":{"by":"settled","event":null,"last":42,"met":true}}
```

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

- **No MCP yet.** The MCP side (tools generated from `describe` on
  `rmcp`, a stdio bridge to the socket) comes next.
- **No recording** of a session as a scenario yet.
- **No `ui.*`.** Tree, screenshots and input go through eframe's
  `inspection` feature ([inspection.md](inspection.md)), a second
  connection.
- **Windows** is compile-checked only; the pipe keeps the system's
  default access (the user, administrators and the system).
