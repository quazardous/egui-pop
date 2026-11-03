---
egui-pop-core: major
egui-pop-api: major
egui-pop-viewport: major
egui-pop-i18n: major
egui-pop-platform: major
egui-pop-views: major
egui-pop-editors: major
egui-pop-imaging: major
egui-pop-test: major
egui-pop: major
---

egui-pop is built on egui 0.37 (eframe, egui-wgpu, egui_kittest and
egui_extras 0.37, egui_table 0.11, egui_commonmark 0.26), and the
workbench draws its docked areas with egui_stack, the continuation of
egui_dock, in place of egui_dock 0.21. An application moves to egui 0.37
with egui-pop: its own egui code follows egui's changelog (`Id::unique`
for `Id::new`, `Ui::scope_id` for `Ui::id`, `egui::Role` for
`WidgetType`, whose names follow AccessKit's: `TextInput`, `CheckBox`,
`Unknown`...). egui-pop's functions taking a widget type, such as
`a11y::name`, take an `egui::Role`. `widgets::combo_box` and
`combo_box_labelled` return `ComboBox<'static>`. With eframe, egui-pop
turns on eframe's `clipboard` feature, a feature of its own since 0.37.

New: `ui::set_scroll_offset` moves a scroll area by its id, egui 0.37
having made the offset of its state private.

Until egui_stack and an egui-phosphor on egui 0.37 are on crates.io
(issues #145 and #146), egui-pop takes them from git and is not
published; an application depending on egui-pop through git needs the
same `[patch.crates-io]` entry for egui-phosphor as egui-pop's
workspace.
