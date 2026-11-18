---
egui-pop-model: major
egui-pop-core: minor
egui-pop-views: minor
egui-pop-api: major
egui-pop: major
egui-pop-agent: major
egui-pop-editors: major
egui-pop-i18n: major
egui-pop-imaging: major
egui-pop-platform: major
egui-pop-test: major
egui-pop-viewport: major
---

A focused widget gets the keys it reads before a global binding of them. A widget says which keys it reads while focused (`focus::reads_keys`): the list, the tree and the thumbnail grid say so of their arrows, Home, End, Page Up, Page Down, Space and Enter. Pressed without Ctrl or Alt while it has the focus, such a key reaches the widget instead of a global binding, which the trace shows as `Verdict::LeftToWidget`; the context key `focusedWidgetKeys` names them. Bindings scoped to the widget's surface, panel or window, chorded ones and those firing `always` still go first. Breaking: `Verdict` has the new variant `LeftToWidget`.
