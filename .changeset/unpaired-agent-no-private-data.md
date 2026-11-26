---
egui-pop-agent: major
---

An unpaired agent on the debug socket reads no private data by default:
`AgentGrants::unpaired` is now `Grant::read_only().private(false)`
(was `Grant::read_only()`). An agent is an actor, not an implicit admin,
so places, people and file paths need pairing (the paired default, the
editor preset, still reads them) or an explicit grant. A command
declared `CommandSpec::private` is refused to an unpaired agent, with a
reason that says pairing or a grant with `private` unlocks it, and a
mixed result leaves its private parts out (`grants::reads_private`).
This is a breaking default change. To move: pair the agent (a
pre-issued token for scenario runners), or keep the old default with
`AgentGrants::new().unpaired(Grant::read_only())`, or let the user show
private data to unpaired clients on the Agents page.
