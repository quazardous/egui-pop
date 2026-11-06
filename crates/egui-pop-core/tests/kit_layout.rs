//! A state restyles a kit part by paint alone (docs/kit.md): its content
//! stays in place whatever the state, in every built-in theme and every
//! example kit, whatever the borders the states give.

use std::cell::RefCell;
use std::rc::Rc;

use egui::epaint::Shape;
use egui::{Pos2, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::kit::{Kit, State};
use egui_pop_core::widgets::{self, Checkbox, SelectableRow, ToggleChip};
use egui_pop_tokens::kit::KitData;
use egui_pop_tokens::{BUILT_IN_THEMES, ThemeSpec, Themes};

/// Every theme to walk: the built-in ones, then the example kits' (both
/// variants of a kit that has two), each with the themes it resolves in.
fn themes() -> Vec<(Themes, String)> {
    let mut all: Vec<(Themes, String)> = BUILT_IN_THEMES
        .iter()
        .map(|name| (Themes::default(), (*name).to_string()))
        .collect();
    let kits: [KitData; 3] = [
        egui_pop_tokens::kit!("../../../examples/kits/underline-tabs"),
        egui_pop_tokens::kit!("../../../examples/kits/flavour-macos"),
        egui_pop_tokens::kit!(
            "../../../examples/kits/rerun-like",
            tokens = ["shared.tokens.yaml"]
        ),
    ];
    for kit in kits {
        let mut themes = Themes::default();
        themes.add_kit(&kit);
        let names = std::iter::once(kit.name.clone()).chain(kit.counterpart.map(|c| c.name));
        for name in names {
            all.push((themes.clone(), name));
        }
    }
    all
}

/// Every combination of states, rest first.
fn every_state() -> Vec<State> {
    (0..1u8 << State::ORDER.len())
        .map(|bits| {
            State::ORDER
                .iter()
                .enumerate()
                .fold(State::REST, |s, (i, (flag, _))| {
                    s.with(*flag, bits & (1 << i) != 0)
                })
        })
        .collect()
}

/// Where a content sits in its part: its offset from the part's corner,
/// its size and the part's.
type Placement = (Vec2, Vec2, Vec2);

#[test]
fn a_part_s_content_stays_in_place_in_every_state() {
    let states = every_state();
    let mut failures = Vec::new();
    for (themes, name) in themes() {
        let seen: Rc<RefCell<Vec<(String, State, Placement)>>> = Rc::default();
        let into = seen.clone();
        let states_ = states.clone();
        let mut h = Harness::new_ui(move |ui| {
            into.borrow_mut().clear();
            let kit = Kit::of(ui.ctx());
            for c in kit.components() {
                for (i, state) in states_.iter().enumerate() {
                    ui.push_id((&c.name, i), |ui| {
                        let r = c.show(ui, *state, egui::Shadow::NONE, |ui| ui.label("Content"));
                        let (outer, inner) = (r.response.rect, r.inner.rect);
                        into.borrow_mut().push((
                            c.name.clone(),
                            *state,
                            (inner.min - outer.min, inner.size(), outer.size()),
                        ));
                    });
                }
            }
        });
        egui_pop_core::theme::install_with(&h.ctx, &themes, &ThemeSpec::new(name.clone()));
        h.run();
        let seen = seen.borrow();
        let mut rest: Option<(&str, Placement)> = None;
        for (component, state, placement) in seen.iter() {
            if *state == State::REST {
                rest = Some((component, *placement));
            } else if let Some((c, at_rest)) = rest
                && c == component
                && *placement != at_rest
            {
                failures.push(format!(
                    "{name}: {component} in {state:?} {placement:?}, at rest {at_rest:?}"
                ));
            }
        }
        assert!(!seen.is_empty(), "{name}: nothing shown");
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Where the text `label` is painted, among `shapes`.
fn text_at(shapes: &[Shape], label: &str) -> Option<Pos2> {
    shapes.iter().find_map(|s| match s {
        Shape::Text(t) if t.galley.text() == label => Some(t.pos),
        Shape::Vec(v) => text_at(v, label),
        _ => None,
    })
}

#[derive(Default)]
struct Widgets {
    on: bool,
    off: bool,
    checked: bool,
}

/// The widgets drawn through the kit, at rest, under the pointer and
/// focused: each one's text is painted at the same place.
#[test]
fn a_widget_s_text_does_not_move_under_the_pointer_or_with_the_focus() {
    const LABELS: [&str; 7] = [
        "Row", "Chosen", "Chip", "Chip on", "Check", "Radio", "Button",
    ];
    let mut failures = Vec::new();
    for (themes, name) in themes() {
        let mut h = Harness::new_ui_state(
            |ui, w: &mut Widgets| {
                ui.add(SelectableRow::new(false, "Row"));
                ui.add(SelectableRow::new(true, "Chosen"));
                ui.add(ToggleChip::new(&mut w.off, "Chip"));
                ui.add(ToggleChip::new(&mut w.on, "Chip on"));
                ui.add(Checkbox::new(&mut w.checked, "Check"));
                let _ = widgets::radio(ui, false, "Radio");
                let _ = ui.button("Button");
            },
            Widgets {
                on: true,
                ..Widgets::default()
            },
        );
        egui_pop_core::theme::install_with(&h.ctx, &themes, &ThemeSpec::new(name.clone()));
        h.run();
        let text = |h: &Harness<'_, Widgets>, label: &str| {
            let shapes: Vec<Shape> = h.output().shapes.iter().map(|c| c.shape.clone()).collect();
            text_at(&shapes, label)
        };
        for label in LABELS {
            let Some(rest) = text(&h, label) else {
                failures.push(format!("{name}: no text {label:?}"));
                continue;
            };
            h.get_by_label(label).hover();
            h.run();
            let hover = text(&h, label);
            h.get_by_label(label).focus();
            h.run();
            let focus = text(&h, label);
            h.remove_cursor();
            h.run();
            if hover != Some(rest) || focus != Some(rest) {
                failures.push(format!(
                    "{name}: {label:?} at {rest:?} at rest, {hover:?} hovered, {focus:?} focused"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
