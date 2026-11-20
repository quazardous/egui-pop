//! A flat tree's rows (`TreeRow::depth`): each indented by its depth
//! times the theme's indent, toggles and leaves alike, and said to be at
//! level depth + 1, on its toggle and on its row (the line holding the
//! caller's label); the caller's row (a selectable label) keeps its own
//! selection.

use egui::accesskit::Role;
use egui::{Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::fold::TreeRow;
use egui_pop_core::theme::{self, ThemeSpec};

/// The flat list: a name, its depth, whether it has rows below it.
const ROWS: &[(&str, usize, bool)] = &[
    ("Photos", 0, true),
    ("Holidays", 2, false),
    ("Winter", 3, true),
];

#[derive(Default)]
struct State {
    selected: Option<&'static str>,
    rows: Vec<Rect>,
    indent: f32,
}

fn harness() -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(vec2(320.0, 200.0))
        .build_ui_state(
            |ui, s: &mut State| {
                s.rows.clear();
                s.indent = ui.spacing().indent;
                for &(name, depth, branch) in ROWS {
                    let r = TreeRow::new(ui.make_persistent_id(name), name)
                        .leaf(!branch)
                        .depth(depth)
                        .show(ui, |ui| {
                            let on = s.selected == Some(name);
                            ui.selectable_label(on, name)
                        });
                    if r.row.inner.clicked() {
                        s.selected = Some(name);
                    }
                    s.rows.push(r.row.inner.rect);
                    r.keep(ui.ctx());
                }
            },
            State::default(),
        );
    theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    h.run();
    h
}

#[test]
fn a_flat_row_is_indented_by_its_depth_times_the_theme_s_indent() {
    let h = harness();
    let s = h.state();
    assert!(s.indent > 0.0);
    let left = s.rows[0].left();
    for (i, &(name, depth, _)) in ROWS.iter().enumerate() {
        let shift = s.rows[i].left() - left;
        let want = s.indent * depth as f32;
        assert!(
            (shift - want).abs() < 0.5,
            "{name} at depth {depth}: shifted {shift}, want {want}"
        );
    }
}

#[test]
fn a_flat_row_says_its_level_on_its_toggle_and_its_row() {
    let h = harness();
    for &(name, depth, branch) in ROWS {
        let level = Some(depth + 1);
        // The row: the line holding the caller's label.
        let label = h.get_by_label(name);
        let node = label.accesskit_node();
        let row = node
            .parent()
            .unwrap_or_else(|| panic!("{name}'s label is in its row"));
        assert_eq!(row.level(), level, "{name}'s row");
        let toggle = row
            .children()
            .find(|n| n.role() == Role::DisclosureTriangle);
        match toggle {
            Some(t) => {
                assert!(branch, "{name} is a branch");
                assert_eq!(t.level(), level, "{name}'s toggle");
            }
            None => assert!(!branch, "{name} is a leaf"),
        }
    }
}

#[test]
fn the_caller_s_row_keeps_its_selection() {
    let mut h = harness();
    h.get_by_label("Holidays").click();
    h.run();
    assert_eq!(h.state().selected, Some("Holidays"));
}
