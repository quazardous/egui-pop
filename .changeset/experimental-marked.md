---
egui-pop: patch
egui-pop-model: patch
egui-pop-core: patch
egui-pop-tokens: patch
egui-pop-api: patch
egui-pop-viewport: patch
egui-pop-imaging: patch
egui-pop-views: patch
egui-pop-editors: patch
egui-pop-platform: patch
egui-pop-hooks: patch
egui-pop-test: patch
ortho-router: patch
---

The experimental tier is marked in the API documentation, not only in `docs/stability.md`: an experimental crate, module or item shows a warning block ("Experimental: may change in any release"). The whole of `egui-pop-viewport`, `egui-pop-imaging`, `egui-pop-views`, `egui-pop-editors`, `egui-pop-platform`, `egui-pop-hooks` and `ortho-router` is marked, so are the experimental modules of the other crates (the workbench, the host, kits, tooltips, histories and the rest of stability.md's list), and the methods of stable types that reach them (`AppHandle::arrange`, `App::run`'s `host::Reply`, the value store's snapshots). Nothing is gated and nothing changes behaviour: the same items stay public, with their tier now said where users read them.
