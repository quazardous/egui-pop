---
egui-pop-api: minor
egui-pop: minor
---

The Settings app keeps its values in a store the host provides (`Admin::attach_store`, the `SettingsStore` trait): loaded when attached or a scope is offered, saved off the UI thread, changes made elsewhere followed. `CompositeStore` routes each setting to the first handler claiming it, the rest to a default; `FileStore` and `MemoryStore` come ready. Store errors show in the window, and so does a notice when nothing keeps the values. `load_layer` and `export_layer` are deprecated.
