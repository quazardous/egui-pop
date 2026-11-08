---
egui-pop-agent: minor
egui-pop-api: minor
---

A new opt-in crate, `egui-pop-agent`, serves the debug bridge out of the
process, for agents that drive a running application:

- JSON-RPC 2.0, one message per line, with the methods `describe`,
  `snapshot`, `call` and `wait`, every call made as
  `Origin::Agent("agent-socket")`;
- over a Unix socket in the user's runtime directory, mode 0600, in a
  directory no one else may write to (refused otherwise), or a named pipe
  on Windows;
- off unless the application turns it on, with
  `AppBuilder::agent_socket(AgentSocket::new())` or, when the user sets
  `EGUI_POP_AGENT_SOCKET=1` (or a path), `agent_socket_from_env()`; the
  bridge is then read-only unless the application chose a mode;
- requests are answered on the UI thread at each pass of the logic, and
  a `wait` is held by the transport until its condition holds or its
  deadline passes: the UI thread never blocks.

`AppBuilder::chosen_debug_bridge` says which bridge mode the application
chose, if any. See `docs/agent-socket.md`.
