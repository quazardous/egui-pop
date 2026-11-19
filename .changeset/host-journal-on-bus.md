---
egui-pop-api: minor
egui-pop: minor
---

One journal per process. When the bus given to `AppBuilder::bus` offers the contract's `Journal` capability and the host keeps no journal of its own (the default host), egui-pop's journal is the bus's: `AppHandle::publish`, `events_since` and `last_event`, and so agents' `wait`, the jobs' `job.*`, links' `link.*` and agents' `agent.*` events, go to it as entries of the new journaled point `pop.journal` (`bus::HOST_JOURNAL`, `bus::HostEvents`, `bus::HostEvent`), numbered in one sequence with the application's own journaled facts. `events_since` reads those facts too, named by their point. A `wait` on such a bus counts the facts in its numbers and matches a fact by its point's id; the window is the bus journal's (docs/hook-kernel.md, "One journal per process"). Without a journal on the bus, or with a host keeping its own, nothing changes.
