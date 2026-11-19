---
egui-pop-model: minor
egui-pop-api: minor
egui-pop-agent: minor
egui-pop-i18n: patch
egui-pop: minor
---

Agents may be kept from private data. `CommandSpec::private()` marks a command whose result is private data (places, people, absolute file paths); `describe` lists `private` and its MCP tool's description says so. `Grant::private` (on in every preset, `.private(false)` withholds it): the socket and the `pop.agent.grants` listener refuse a private command to an agent whose grant withholds private data (`command.refused`, `by: "pop.agent.grants"`), and `grants::reads_private(app, origin)` lets a handler whose result is only partly private leave those fields out. The Agents page gets a Private data switch per agent and for the defaults (`pop.agent.grant` takes `private`); a preset keeps the switch. English and French strings.
