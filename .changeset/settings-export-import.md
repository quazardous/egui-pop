---
egui-pop-model: minor
egui-pop-api: minor
egui-pop: minor
---

Settings can be exported to a file and imported back, from Settings › General › Advanced or by command.

- **Export** (`settings.export`, a wizard): the settings you changed, or a library's or workspace's values, all of them or one family or section, with your shortcuts and the settings of custom pages if you like. The file is a versioned JSON document (`settings-export`, `egui_pop_model::admin::transfer`). A setting marked `Descriptor::sensitive` is never written: it is listed as left out.
- **Import** (`settings.import`, a wizard): picks the file, shows each change (its value now and in the file) as a tick box, and what is left out and why: unknown settings, settings an application retired (`Admin::obsolete`), values of the wrong type or out of range (`Descriptor::accept`, never clamped), secrets. The changes chosen apply into a value scope as one undo step; `settings.undoImport` takes them back. `dryRun` (and the read-only `settings.importPreview`) says what it would do.
- **The wizard kit** gains a path field with Browse (`Field::open_file`, `Field::save_file`) and a multi field (a tick box per choice, `null` for all); a record source's command arguments are a template over the form's values.
