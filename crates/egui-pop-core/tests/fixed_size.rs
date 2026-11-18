//! Fixed sizes in `UiPop` (`egui_pop_core::ui`): a widget filling a box of
//! a set size (`sized`), content in a box of exactly a set size
//! (`sized_ui`), content placed in a rectangle and clipped to it
//! (`rect_ui`). What is drawn keeps its role, its name and its clicks.

use egui::accesskit::Role;
use egui::epaint::Shape;
use egui::{Layout, Rect, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::theme::{self, ThemeSpec};
use egui_pop_core::ui::UiPop;
use egui_pop_core::widgets::SelectableRow;

#[derive(Default)]
struct Boxes {
    clicks: usize,
    /// What `sized_ui` answered, and where the next widget went.
    cell: Option<Rect>,
    after: Option<Rect>,
    /// The rectangle `rect_ui` was given.
    slot: Option<Rect>,
}

const ROW: egui::Vec2 = vec2(220.0, 30.0);
const CELL: egui::Vec2 = vec2(60.0, 24.0);

fn boxes() -> Harness<'static, Boxes> {
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui_state(
            |ui, s: &mut Boxes| {
                if ui
                    .sized(ROW, SelectableRow::new(false, "Show all"))
                    .clicked()
                {
                    s.clicks += 1;
                }
                ui.horizontal(|ui| {
                    let cell = ui.sized_ui(
                        CELL,
                        Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| ui.label("1"),
                    );
                    s.cell = Some(cell.response.rect);
                    s.after = Some(ui.label("after").rect);
                });
                let slot = Rect::from_min_size(pos2(10.0, 200.0), vec2(80.0, 20.0));
                s.slot = Some(slot);
                ui.rect_ui(slot, Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label("A long line that never goes past its rectangle")
                });
            },
            Boxes::default(),
        );
    theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    h.run();
    h
}

#[test]
fn a_sized_widget_fills_its_box_and_keeps_its_role_and_clicks() {
    let mut h = boxes();
    let row = h.get_by_label("Show all");
    assert_eq!(row.accesskit_node().role(), Role::Button);
    let rect = row.rect();
    assert!(
        (rect.width() - ROW.x).abs() < 0.5 && (rect.height() - ROW.y).abs() < 0.5,
        "{rect:?} fills {ROW:?}"
    );
    row.click();
    h.run();
    assert_eq!(h.state().clicks, 1);
}

#[test]
fn a_sized_box_takes_its_size_whatever_its_content() {
    let h = boxes();
    let s = h.state();
    let cell = s.cell.unwrap();
    assert_eq!(cell.size(), CELL, "the box's own size, not its content's");
    let label = h.get_by_label("1").rect();
    assert!(cell.contains_rect(label), "{label:?} inside {cell:?}");
    assert!(
        (label.center().x - cell.center().x).abs() < 1.0,
        "centred: {label:?} in {cell:?}"
    );
    // The next widget goes after the box.
    assert!(s.after.unwrap().left() >= cell.right());
}

#[test]
fn content_in_a_rectangle_is_placed_there_and_clipped_to_it() {
    let h = boxes();
    let slot = h.state().slot.unwrap();
    let label = h
        .get_by_label("A long line that never goes past its rectangle")
        .rect();
    assert!(
        (label.left() - slot.left()).abs() < 0.5,
        "{label:?} at {slot:?}"
    );
    let text = h
        .output()
        .shapes
        .iter()
        .find(|c| match &c.shape {
            Shape::Text(t) => t.galley.text().starts_with("A long line"),
            _ => false,
        })
        .expect("the line is painted");
    assert!(
        slot.contains_rect(text.clip_rect),
        "clipped to {slot:?}: {:?}",
        text.clip_rect
    );
}
