---
egui-pop-core: minor
egui-pop-model: minor
egui-pop-api: minor
egui-pop-agent: minor
egui-pop: minor
---

A command's arguments may be a JSON Schema, and then it is enforced: `CommandSpec::schema(json)` (or `CommandSpec::args` given a schema) makes every run whose arguments do not match refused before its handler, whoever asks it, the user included. The journal says `command.refused` with `by: "pop.command.args"` (`bus::ARGS_VALIDATOR`) and a reason naming where and why (`/factor: expected a number, got a string`); over the agent socket `commands.run` answers the refusal at once, and a destructive command's run refused so is not asked about. A command whose arguments are only described (an example, a text) checks nothing, as before. The checker is egui-pop's own subset, `egui_pop_core::schema::check` (types, `required`, `properties`, `additionalProperties`, `enum`, `const`, ranges, lengths, `items`, combinators); other keywords are notes. `AppHandle::command_args_refusal` asks before a run. `describe` lists the schema and MCP gives it as the tool's `inputSchema`.
