---
egui-pop-core: minor
egui-pop-api: minor
egui-pop-test: minor
egui-pop: minor
---

Agents can discover every capability with its schemas and wait on the
application:

- a collector action may carry JSON Schemas of its arguments and its
  result (`FnCollector::schema`, `FnCollector::result_schema`,
  `Collector::schemas`); `debug_describe` lists them;
- `AppHandle::debug_capabilities()`, and the new `bridge.describe`
  action, list every collector action and every command as one list,
  under protocol 1 (`DebugBridge::capabilities`, `PROTOCOL`);
- `bridge.wait` checks a journal event (`event`, `cause`), whether the
  application is settled, or the first of several; in Rust,
  `AppHandle::wait_met` and `AppHandle::settled` (`egui_pop_api::wait`);
- `egui_pop_test::wait::wait_until` and `settle` step a harness's
  frames until the condition holds, or until a deadline.
