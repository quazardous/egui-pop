---
egui-pop-agent: minor
egui-pop-core: minor
egui-pop-model: minor
egui-pop-api: minor
egui-pop: minor
---

Destructive calls wait for the user. A command declared `CommandSpec::destructive()`, or a collector action that says it is (`Collector::destructive(action, args)`, `FnCollector::destructive` and `destructive_when`), asked by an agent through the agent socket opens a confirmation dialog naming the agent, what it asks and its arguments; the call is held until the user answers (`deadline_ms`, 60 s by default, 120 s at most). Accepted, it runs with the agent's origin; declined or unanswered, it is refused (`-32001`, `-32002`) and the journal says `command.refused` (or `debug.call.refused`) with `by: "pop.agent.confirm"`. Only the user answers, through `pop.agent.confirm`, shared with pairing. The user's own runs are never asked. `describe` lists `destructive` for every capability, and the MCP binary sets `destructiveHint` on destructive tools. `AppHandle::debug_destructive` and `AppHandle::debug_refusal` ask the bridge before a call.
