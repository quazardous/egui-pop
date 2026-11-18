---
egui-pop-api: minor
egui-pop: minor
---

A pass of a point's listeners over its budget is said as the `bus.overBudget` fact (point, `took_us`, `budget_us`, each listener's time, `slowest`, `unsaid`) and one warning in the log, at most once a minute per point. The debug collector `bus` shows each point's deadline, budget and passes, each listener's deadline, threaded, local and runs, and `slowest`: every listener timed, the slowest first.
