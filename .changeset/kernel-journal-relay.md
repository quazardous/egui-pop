---
egui-pop-hooks: minor
egui-pop-api: minor
egui-pop: minor
---

The hook kernel delegates bus-contract 0.3.2's optional `Journal` and `Relay` capabilities to its host: `Kernel::with_journal` and `Kernel::with_relay` make `bus.journal()` and `bus.relay()` answer, so points may be declared journaled or relayable (refused without them, by `declare_all` and `Kernel::declare_point`). The kernel refuses a publish on a point it does not hold journaled before the host sees it, and `Kernel::declaration` gives the host a point's key and spec (its codec). The debug collector `bus` shows each point's `journaled` and `relayable`, the bus's `capabilities` and its `journal_seq`.
