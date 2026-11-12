---
egui-pop-model: major
egui-pop-core: major
egui-pop-viewport: major
egui-pop-api: major
egui-pop-i18n: major
egui-pop-platform: major
egui-pop-views: major
egui-pop-editors: major
egui-pop-imaging: major
egui-pop-agent: major
egui-pop-test: major
egui-pop: major
---

Settings are app-wide by default: the Settings app draws no "Set for" choice unless a setting opts into a value scope, in its declaration (`Descriptor::scopes`) or a manifest's field (`Field::scopes`, where empty now means the app only). egui-pop no longer knows libraries or workspaces: the host declares its scopes with `Admin::declare_scope(ExtraScope::new(name, label, about))`, lowest first, a later one winning; `ValueScope` is now a name (`ValueScope::new`), `ValueScope::Library` and `Workspace` are deprecated and `ValueScope::ALL` is gone. The choice is labelled "Set for", with a tooltip naming each scope and which wins; `set_in` refuses a scope the setting did not opt into.
