---
egui-pop-agent: minor
egui-pop-api: minor
---

`egui-pop-agent` serves the agent socket as an MCP server, behind the
new `mcp` feature (on `rmcp`):

- the binary `egui-pop-agent-mcp <socket>` speaks MCP over stdio and the
  socket's JSON-RPC to the running application;
- its tools are generated at run time from the application's `describe`:
  one per capability offered to agents (name sanitised for MCP, its
  description, its args schema or an object schema made from the
  example, its result schema as the output schema), plus `snapshot` and
  `wait`; a command's tool runs it and answers its result;
- `egui_pop_agent::mcp::McpBridge` implements rmcp's `ServerHandler`
  over a `Backend`, and `mcp::tools` makes the tools from a `describe`
  reply.

The socket's `snapshot` is now refused when the debug bridge is `Off`,
as its calls are. `AppHandle::debug_bridge_mode` says the bridge's mode
now. See `docs/agent-socket.md`.
