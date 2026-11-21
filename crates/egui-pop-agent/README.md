# egui-pop-agent

Serves [egui-pop](https://github.com/quazardous/egui-pop)'s debug bridge
out of the process: JSON-RPC 2.0 (`describe`, `snapshot`, `call`, `wait`,
`ui`, and `inspect`, egui's inspection protocol without its TCP port)
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
capabilities, generated from its `describe`, plus `snapshot`, `wait`,
`ui` and the `widget_*` tools: Rerun's
[`egui_inspection_tools`](https://github.com/rerun-io/egui_crates/tree/main/crates/egui_inspection_tools)
(by Lucas Meurer and Emil Ernerfeldt) over `inspect`, for testing by
pointer and pixels (screenshots, clicks, drags, typed text, keys,
batches, waits), under the same grants and input lock. Until Rerun
publishes that crate, it comes from its temporary, unchanged fork
`egui-pop-inspection-tools`.

See `docs/agent-socket.md` in the repository.
