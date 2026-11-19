---
egui-pop-api: minor
egui-pop-agent: minor
egui-pop-i18n: patch
egui-pop: minor
---

The agent socket's plugin declares an Agents page in the Settings app (Developer › Agents), as a manifest: the paired agents (name, id, paired since, last seen, connected now) with their access, and the defaults for unpaired clients and paired agents. The access is a preset (read only, editor, full) applied live through `grants::set_grants`; Revoke asks first, then removes the pairing's stored token hash, drops its grant, closes its open connections and journals `agent.revoked`. New user-only commands `pop.agent.grant` and `pop.agent.revoke` (kept from agents, refused to any origin but the user's), the read-only `pop.agent.list`, and `egui_pop_agent::revoke`, `pairing::connected` and `Pairing::last_seen`. The Settings app translates a manifest's declared texts a component registers with `Admin::texts` (group, column, row field, choice, empty state, status, callout and confirmation texts). English and French strings; the gallery gets the socket behind its setting and the page.
