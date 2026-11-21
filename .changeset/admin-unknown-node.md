---
egui-pop-model: major
egui-pop-api: major
egui-pop: major
egui-pop-agent: major
egui-pop-core: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-viewport: major
egui-pop-views: major
---

An admin manifest item whose `node` this version does not know (a newer
manifest's brick) no longer fails the whole manifest: it reads as the new
`Item::Unknown` (kept as written, written back unchanged), and
`Manifest::check` reports it as `admin-unknown-node` and leaves it out, so
the rest of the page still shows. A known node that does not read is still
an error. To move: a `match` over `admin::Item` needs an `Item::Unknown(_)`
arm.
