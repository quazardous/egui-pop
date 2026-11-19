//! A thumbnail grid's rubber band senses the grid's own room, never what
//! lies beside the grid in the same window.

use std::cell::Cell;
use std::rc::Rc;

use egui::{Rect, UiBuilder, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::thumbnail::ThumbnailGrid;

#[test]
fn a_button_beside_a_grid_with_a_band_takes_its_clicks() {
    let clicks = Rc::new(Cell::new(0));
    let counted = clicks.clone();
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 200.0))
        .build_ui(move |ui| {
            // A strip on the left with a button, the grid on the right in
            // a child `Ui` that keeps its parent's clip rect.
            let room = ui.available_rect_before_wrap();
            let strip = Rect::from_min_size(room.min, vec2(100.0, room.height()));
            let mut strip_ui = ui.new_child(UiBuilder::new().max_rect(strip));
            if strip_ui.button("Beside").clicked() {
                counted.set(counted.get() + 1);
            }
            let grid = Rect::from_min_max(pos2(strip.max.x + 8.0, room.min.y), room.max);
            let mut grid_ui = ui.new_child(UiBuilder::new().max_rect(grid));
            ThumbnailGrid::new(6, vec2(60.0, 40.0))
                .id_salt("grid")
                .rubber_band(true)
                .show_with_band(&mut grid_ui, |ui, _| {
                    ui.label("cell");
                });
        });
    h.run();
    let at = h.get_by_label("Beside").rect().center();
    h.hover_at(at);
    h.run();
    h.drag_at(at);
    h.run();
    h.drop_at(at);
    h.run();
    assert_eq!(clicks.get(), 1, "the band took the button's click");
}
