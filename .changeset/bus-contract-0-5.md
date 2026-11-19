---
egui-pop-hooks: major
egui-pop-api: major
egui-pop: major
egui-pop-core: major
egui-pop-viewport: major
egui-pop-agent: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-views: major
---

egui-pop moves to bus-contract 0.5 and bus-kernel 0.3 (bus-contract's `MIGRATION-0.5.md` covers the details), and to egui_stack 0.30 (`egui-pop-core` re-exports its sections types, so it breaks too). `egui-pop-hooks` re-exports bus-kernel 0.3. Point ids are checked: `Point::ID` is a `bus_contract::PointId` (`const ID: PointId = PointId::new("notes.note.delete")`), and the bus refuses a listener id without a prefix (`Refused::BadId`). egui-pop's points keep their names, except the job events point `pop.job.*`, which was not a valid id and is now `pop.job.event` (`JobEvents`); the job events it delivers are unchanged. The `*_POINTS` tables stay `[&str; N]`; pass `&P::ID` or `P::ID.as_str()` where a `&str` is needed. The capabilities left the core: `egui_pop_api::bus` re-exports `bus_contract_defer`, `bus_contract_timings`, `bus_contract_journal` and `bus_contract_crossing`, whose extension traits (`BusDefer`, `BusTimings`, `BusJournal`, `BusCrossing`) give `bus.defer()`, `bus.timings()`, `bus.journal()` and `bus.relay()`; a journaled point is declared with `.facets(&[JOURNALED])`. A custom `Bus` handed to `AppBuilder::bus` implements bus-contract 0.5 (`Bus::capability` instead of the getters). The debug collector `bus` keeps its fields (`journaled`, `relayable`, `capabilities.relay`).
