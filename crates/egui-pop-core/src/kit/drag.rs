//! The drag cursors a kit gives (`drag-cursor.<kind>.hover`, `.drag`,
//! `.policy`): per kind of drag, its cursors as CSS names and when the
//! hover one shows, over the kind's own ([`DragKind::cursors`]). The
//! pointer profile in force and the widget itself can still change them
//! (`docs/pointer-gestures.md`).
//! Compiled with the kit, read as plain fields.

use std::collections::BTreeMap;

use egui_pop_model::pointer::{CURSOR_NAMES, DragCursor, DragKind, HoverPolicy};

use super::{Tokens, string};

/// The drag cursors of a kit, by kind.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DragCursorKit(BTreeMap<DragKind, DragCursor>);

impl DragCursorKit {
    /// A kind's cursors in the kit: what it gives over the kind's own.
    pub fn get(&self, kind: DragKind) -> DragCursor {
        self.0.get(&kind).cloned().unwrap_or_else(|| kind.cursors())
    }

    pub(super) fn compile(t: &Tokens, problems: &mut Vec<String>) -> DragCursorKit {
        let mut kinds = BTreeMap::new();
        for kind in DragKind::ALL {
            let path = |term: &str| format!("drag-cursor.{}.{term}", kind.as_str());
            let mut given = DragCursor::default();
            for (term, slot) in [("hover", &mut given.hover), ("drag", &mut given.drag)] {
                let Some(name) = string(t, &path(term)) else {
                    continue;
                };
                if CURSOR_NAMES.contains(&name.as_str()) {
                    *slot = name;
                } else {
                    problems.push(format!(
                        "{}: `{name}` is not a cursor name (CSS's: `grab`, `col-resize`...)",
                        path(term)
                    ));
                }
            }
            if let Some(asked) = string(t, &path("policy")) {
                match HoverPolicy::parse(&asked) {
                    Some(p) => given.policy = Some(p),
                    None => problems.push(format!(
                        "{}: `{asked}` is not only-action, always or never",
                        path("policy")
                    )),
                }
            }
            let c = kind.cursors().over(&given);
            if c != kind.cursors() {
                kinds.insert(kind, c);
            }
        }
        DragCursorKit(kinds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::{Kit, Shapes};

    fn kit(json: &str) -> Kit {
        let mut themes = egui_pop_tokens::Themes::default();
        themes
            .add_text("drag-test", json)
            .expect("test theme reads");
        let tokens = Tokens::new(themes.resolve(&egui_pop_tokens::ThemeSpec::new("drag-test")));
        Kit::compile(&tokens, &Shapes::default())
    }

    #[test]
    fn the_default_kit_gives_each_kind_its_own() {
        let k = kit(r#"$extends: "pop-dark""#);
        assert!(k.problems.is_empty(), "{:?}", k.problems);
        for kind in DragKind::ALL {
            assert_eq!(k.drag_cursors.get(kind), kind.cursors(), "{kind:?}");
        }
    }

    #[test]
    fn a_kit_changes_a_kind_and_reports_what_it_cannot_read() {
        let k = kit(r#"
            $extends: "pop-dark"
            drag-cursor:
              pan:
                hover: {$type: "string", $value: "all-scroll"}
                policy: {$type: "string", $value: "always"}
              move:
                drag: {$type: "string", $value: "spinning"}
                policy: {$type: "string", $value: "sometimes"}
            "#);
        let pan = k.drag_cursors.get(DragKind::Pan);
        assert_eq!(
            (pan.hover.as_str(), pan.drag.as_str(), pan.policy),
            ("all-scroll", "grabbing", Some(HoverPolicy::Always))
        );
        assert_eq!(k.drag_cursors.get(DragKind::Move), DragKind::Move.cursors());
        assert_eq!(
            k.problems,
            [
                "drag-cursor.move.drag: `spinning` is not a cursor name (CSS's: `grab`, `col-resize`...)",
                "drag-cursor.move.policy: `sometimes` is not only-action, always or never",
            ]
        );
    }
}
