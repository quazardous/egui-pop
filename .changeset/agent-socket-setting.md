---
egui-pop-agent: minor
egui-pop-i18n: patch
---

The agent socket can follow a setting: `developer.agentSocket` (off by
default, "Lets local programs drive this app (read-only unless the app
allows more)", in English and French) starts the socket when turned on
and stops it, its file removed, when turned off, live.
`AgentSocketExt::agent_socket_on_setting` declares it;
`AgentSocketExt::agent_socket_from_env_or_setting` turns the socket on
with `EGUI_POP_AGENT_SOCKET` whatever the setting says, and follows the
setting otherwise. A client still connected when the setting turns off
is refused.
