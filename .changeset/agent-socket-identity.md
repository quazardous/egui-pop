---
egui-pop-agent: minor
---

Agents on the socket can name themselves. The new `hello` method (`name`, `token`) asks the user, through a confirmation dialog, to pair the agent the first time, holding the request until the user answers, and hands the agent a token for the next times; from then on the connection's calls run as `Origin::Agent("<id>")` instead of `Origin::Agent("agent-socket")`. Only the user answers: the answer command `pop.agent.confirm` (`CMD_CONFIRM`) is kept from agents. Pairings are kept in `agent-pairings.json` in the persistence directory, tokens as their SHA-256 (`pairings` lists them), and the journal says `agent.paired`, `agent.declined`, `agent.connected` and `agent.disconnected`. A client that does not say `hello` works as before.
