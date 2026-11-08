---
egui-pop-model: minor
egui-pop-api: patch
---

Administration app: a setting a manifest places is no longer also
generated on its prefix's page (`Manifest::bound_settings`), so it shows
once, where the manifest puts it.

The default scopes leave room between their orders (General 0, Interface
10, Input 20, Developer 110) and `Manifest::scope_at` places an
application's scope among them, so Developer stays last.
