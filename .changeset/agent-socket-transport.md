---
egui-pop-agent: minor
---

The agent socket lets in only the user's own processes and holds clients to limits. On Linux and macOS, a client whose process runs as another user is refused before any request (its peer credentials); on Windows, the pipe's access list grants only its owner and the system, remote clients are rejected and the first instance is created as such. At most `MAX_CONNECTIONS` (8) clients at once and requests of at most `MAX_REQUEST_BYTES` (1 MiB): one more client, or a longer request, is answered with the new error `RpcError::LIMIT` (`-32004`) and closed.
