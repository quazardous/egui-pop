//! `egui-pop-agent-mcp <socket>`: an MCP server over stdio for a running
//! egui-pop application, bridged to its agent socket. The tools are the
//! application's capabilities (its `describe`), plus `snapshot` and
//! `wait`. See `docs/agent-socket.md`.

use egui_pop_agent::mcp::{McpBridge, SocketClient};
use rmcp::ServiceExt as _;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let path = match (args.next(), args.next()) {
        (Some(p), None) if p != "-h" && p != "--help" => p,
        _ => {
            eprintln!(
                "usage: egui-pop-agent-mcp <socket>\n\n\
                 Serves MCP over stdio for the egui-pop application listening on <socket>\n\
                 (EGUI_POP_AGENT_SOCKET=1 logs the path it listens at)."
            );
            std::process::exit(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("egui-pop-agent-mcp: {e}");
            std::process::exit(1);
        }
    };
    let result = runtime.block_on(async {
        let server = McpBridge::new(SocketClient::new(path))
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| e.to_string())?;
        server.waiting().await.map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    });
    if let Err(e) = result {
        eprintln!("egui-pop-agent-mcp: {e}");
        std::process::exit(1);
    }
}
