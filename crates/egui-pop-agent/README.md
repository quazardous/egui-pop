# egui-pop-agent

Serves [egui-pop](https://github.com/quazardous/egui-pop)'s debug bridge
out of the process: JSON-RPC 2.0 (`describe`, `snapshot`, `call`, `wait`)
over a Unix socket in the user's runtime directory (mode 0600), or a
named pipe on Windows. Off unless the application turns it on; read-only
unless the application offers more.

```rust,ignore
use egui_pop_agent::{AgentSocket, AgentSocketExt as _};

let builder = egui_pop_api::App::builder()
    // On when `EGUI_POP_AGENT_SOCKET` is `1` or a path; nothing otherwise.
    .agent_socket_from_env();
```

With the `mcp` feature, `egui-pop-agent-mcp <socket>` serves the socket
as an MCP server over stdio (on `rmcp`): the tools are the application's
capabilities, generated from its `describe`, plus `snapshot` and `wait`.

See `docs/agent-socket.md` in the repository.
