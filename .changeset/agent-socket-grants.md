---
egui-pop-agent: minor
egui-pop-api: minor
egui-pop: minor
---

Agents on the socket are held to grants the application sets. A `Grant` says which commands an agent may run (patterns), whether it only reads, which value paths `value.set` and `value.reset` may write, and whether `ui` may act for it; presets `Grant::read_only()`, `Grant::editor()` and `Grant::full()`. `AgentGrants` gives one to unpaired clients (read-only by default), one to paired agents (editor by default) and one per paired agent; set them with `AppBuilder::agent_grants` (`AgentGrantsExt`) or live with `grants::set_grants`. The socket refuses what a grant does not allow before it runs or asks the user (`-32001`), and a listener named `pop.agent.grants` on `pop.command.run`, placed before the application's own listeners, refuses it on any path; the journal says `command.refused` (or `debug.call.refused`) with `by: "pop.agent.grants"`. **Behaviour change:** a client that does not say `hello` no longer writes, even with the bridge on; an application that relies on it (scenario runs, the MCP binary, `egui_pop_test::scenario::live`) says `AgentGrants::new().allow_unpaired_writes()`. `AppHandle::debug_mutates` says whether a bridge call changes the application.
