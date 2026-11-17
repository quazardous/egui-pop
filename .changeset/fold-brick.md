---
egui-pop-core: minor
egui-pop-i18n: patch
---

Folds inside a panel (`fold`, experimental). `Fold` is a group whose header band (the kit's section band: disclosure mark, title, optional count, optional actions at the right end) folds and opens its body; `TreeRow` is a row of a hand-built tree with its disclosure toggle, a leaf keeping the toggle's room so rows line up, its rows below shown indented when open (`TreeRowResponse::body`). A click, Space or Enter toggles them, Left folds and Right opens while they have the keyboard; assistive technology reads whether they are expanded and can expand or collapse them. What is open is kept by id in egui's persisted memory, under the ids egui's `CollapsingHeader` and `CollapsingState` use, so either replaces egui's without losing what the user opened; `fold::is_open` and `fold::set_open` read and change it. `egui-pop-i18n`: the toggle's name, `egui-pop-fold-toggle` ("Show or hide { $item }"), in English and French. The gallery has a Folds page.
