//! The thumbnail grid's zoom: a count of thumbnails per row that share
//! the panel's width, kept through a resize (kittest).

use egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_core::theme::Tokens;
use egui_pop_views::library::grid::{
    GridResize, LibraryItems, LibraryState, MIN_SIZE, Signal, Thumb, ThumbGrid, ThumbSource, Zoom,
};

struct Shoot(u32);

impl LibraryItems<u32> for Shoot {
    fn len(&self) -> usize {
        self.0 as usize
    }
    fn key(&self, i: usize) -> u32 {
        i as u32
    }
    fn label(&self, i: usize) -> String {
        format!("IMG_{i:05}")
    }
}

struct Thumbs;

impl ThumbSource<u32> for Thumbs {
    fn thumb(&self, _: &u32, _: u32) -> Thumb {
        Thumb::Pending { average: None }
    }
}

/// A grid in a panel `width` points wide.
struct Panel {
    state: LibraryState<u32>,
    width: f32,
    columns: usize,
    /// The panel's inner width (the scroll bar aside) and the gap between
    /// cells, as the grid saw them.
    inner: f32,
    gap: f32,
    zooms: Vec<u32>,
    /// What the grid signalled of a zoom kept as a size.
    sizes: Vec<f32>,
    resize: GridResize,
}

fn panel(width: f32, zoom: Zoom) -> Harness<'static, Panel> {
    panel_under(width, zoom, GridResize::PerRow)
}

fn panel_under(width: f32, zoom: Zoom, resize: GridResize) -> Harness<'static, Panel> {
    let mut state = LibraryState::new("shoot");
    state.set_zoom(zoom);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 700.0))
        .build_ui_state(
            |ui, p: &mut Panel| {
                let width = p.width;
                ui.allocate_ui(egui::vec2(width, 600.0), |ui| {
                    ui.set_width(width);
                    p.inner = ui.available_width() - ui.spacing().scroll.allocated_width();
                    p.gap = Tokens::of(ui.ctx()).space.s;
                    let out = ThumbGrid::new("grid").height(500.0).resize(p.resize).show(
                        ui,
                        &mut p.state,
                        &Shoot(200),
                        &Thumbs,
                    );
                    p.columns = out.columns;
                    for s in out.signals {
                        match s {
                            Signal::Zoom { per_row } => p.zooms.push(per_row),
                            Signal::ZoomSize { points } => p.sizes.push(points),
                            _ => {}
                        }
                    }
                });
            },
            Panel {
                state,
                width,
                columns: 0,
                inner: 0.0,
                gap: 0.0,
                zooms: Vec::new(),
                sizes: Vec::new(),
                resize,
            },
        );
    h.run();
    h
}

fn resize(h: &mut Harness<'_, Panel>, width: f32) {
    h.state_mut().width = width;
    h.run();
}

fn press(h: &mut Harness<'_, Panel>, key: Key) {
    h.key_press(key);
    h.run();
}

/// Clicks the first cell, so the grid has the keyboard.
fn focus(h: &mut Harness<'_, Panel>) {
    h.get_by_label("IMG_00000").click();
    h.run();
}

#[test]
fn the_count_per_row_is_kept_after_a_width_change() {
    let mut h = panel(800.0, Zoom::PerRow(4));
    assert_eq!(h.state().columns, 4);
    let wide = h.state().state.size();
    resize(&mut h, 500.0);
    assert_eq!(h.state().columns, 4);
    assert!(
        h.state().state.size() < wide,
        "the cells shrink with the panel"
    );
    resize(&mut h, 1200.0);
    assert_eq!(h.state().columns, 4);
    assert!(h.state().state.size() > wide, "and grow with it");
    assert_eq!(h.state().state.zoom(), Zoom::PerRow(4));
    assert!(h.state().zooms.is_empty(), "a resize is no zoom");
}

#[test]
fn one_per_row_fills_the_panel() {
    for width in [400.0, 900.0, 1300.0] {
        let h = panel(width, Zoom::PerRow(1));
        let p = h.state();
        assert_eq!(p.columns, 1);
        let size = p.state.size();
        assert!(
            (size + 2.0 * p.gap - p.inner).abs() < 0.5,
            "{size} across a panel of {} inside",
            p.inner
        );
        let cell = h.get_by_label("IMG_00000").rect();
        assert!(
            cell.width() > p.inner - 2.0 * p.gap - 1.0,
            "{cell:?} in {width}"
        );
    }
}

#[test]
fn plus_and_minus_step_through_the_counts() {
    let mut h = panel(1000.0, Zoom::PerRow(5));
    focus(&mut h);
    press(&mut h, Key::Minus);
    assert_eq!(h.state().state.zoom(), Zoom::PerRow(6));
    press(&mut h, Key::Minus);
    assert_eq!(h.state().state.zoom(), Zoom::PerRow(8));
    for _ in 0..6 {
        press(&mut h, Key::Plus);
    }
    assert_eq!(h.state().state.zoom(), Zoom::PerRow(1));
    assert_eq!(h.state().columns, 1);
    // The closest step is one per row: no further, and no signal.
    assert_eq!(h.state().zooms, [6, 8, 6, 5, 4, 3, 2, 1]);
}

#[test]
fn the_wheel_with_ctrl_steps_too() {
    let mut h = panel(1000.0, Zoom::PerRow(5));
    h.hover_at(egui::pos2(100.0, 100.0));
    h.run();
    h.event(egui::Event::Zoom(1.25));
    h.run();
    assert_eq!(h.state().state.zoom(), Zoom::PerRow(4));
    h.event(egui::Event::Zoom(0.8));
    h.run();
    assert_eq!(h.state().state.zoom(), Zoom::PerRow(5));
}

#[test]
fn the_floor_bounds_the_count_in_a_narrow_panel() {
    let mut h = panel(1200.0, Zoom::PerRow(16));
    assert_eq!(h.state().columns, 16);
    resize(&mut h, 300.0);
    let p = h.state();
    let most = Zoom::most_per_row(p.inner, p.gap) as usize;
    assert!(most < 16);
    assert_eq!(p.columns, most);
    assert!(p.state.size() >= MIN_SIZE, "{}", p.state.size());
    assert_eq!(p.state.zoom(), Zoom::PerRow(16), "the zoom is kept");
    resize(&mut h, 1200.0);
    assert_eq!(h.state().columns, 16, "and shown again once there is room");
    // Narrow: out from what fits does nothing; closer steps from what is
    // shown.
    resize(&mut h, 300.0);
    focus(&mut h);
    press(&mut h, Key::Minus);
    assert!(h.state().zooms.is_empty());
    press(&mut h, Key::Plus);
    let n = *h.state().zooms.last().expect("a step");
    assert!((n as usize) < most);
    resize(&mut h, 1200.0);
    assert_eq!(h.state().columns, n as usize);
}

#[test]
fn a_size_in_points_becomes_the_count_it_gave() {
    // The old layout: as many cells of the size as fit.
    assert_eq!(Zoom::from_points(128.0, 796.0, 4.0), Zoom::PerRow(6));
    assert_eq!(Zoom::from_points(512.0, 300.0, 4.0), Zoom::PerRow(1));
    assert_eq!(
        Zoom::from_points(10.0, 796.0, 4.0),
        Zoom::from_points(MIN_SIZE, 796.0, 4.0)
    );
    let h = panel(800.0, Zoom::Points(128.0));
    let p = h.state();
    let Zoom::PerRow(n) = Zoom::from_points(128.0, p.inner, p.gap) else {
        unreachable!()
    };
    assert_eq!(p.state.zoom(), Zoom::PerRow(n));
    assert_eq!(p.columns, n as usize);
    assert_eq!(p.zooms, [n], "signalled once, for the app to store");
}

#[test]
fn the_steps_go_by_counts() {
    assert_eq!(Zoom::PerRow(1).zoomed_in(), Zoom::PerRow(1));
    assert_eq!(Zoom::PerRow(2).zoomed_in(), Zoom::PerRow(1));
    assert_eq!(Zoom::PerRow(8).zoomed_in(), Zoom::PerRow(6));
    assert_eq!(Zoom::PerRow(7).zoomed_in(), Zoom::PerRow(6));
    assert_eq!(Zoom::PerRow(6).zoomed_out(), Zoom::PerRow(8));
    assert_eq!(Zoom::PerRow(32).zoomed_out(), Zoom::PerRow(40));
    assert_eq!(Zoom::default(), Zoom::PerRow(5));
}

#[test]
fn a_fixed_size_grid_gains_columns_when_widened() {
    let mut h = panel_under(800.0, Zoom::Points(150.0), GridResize::FixedSize);
    let narrow = h.state().columns;
    let size = h.state().state.size();
    assert!(size >= 150.0, "a cell is at least the size asked: {size}");
    resize(&mut h, 1300.0);
    assert!(
        h.state().columns > narrow,
        "a wider panel shows more columns: {} then {}",
        narrow,
        h.state().columns
    );
    let wide = h.state().state.size();
    assert!(
        (150.0..150.0 * 2.0).contains(&wide),
        "the thumbnails keep about their size: {size} then {wide}"
    );
    resize(&mut h, 500.0);
    assert!(h.state().columns < narrow, "and a narrower one fewer");
    assert_eq!(
        h.state().state.zoom(),
        Zoom::Points(150.0),
        "the size is kept"
    );
    assert!(h.state().sizes.is_empty(), "a resize is no zoom");
}

#[test]
fn a_per_row_grid_keeps_its_count_when_widened() {
    let mut h = panel_under(800.0, Zoom::PerRow(4), GridResize::PerRow);
    assert_eq!(h.state().columns, 4);
    resize(&mut h, 1300.0);
    assert_eq!(h.state().columns, 4, "the count holds, the cells grow");
}

#[test]
fn a_fixed_size_grid_turns_a_count_into_its_size_and_steps_by_columns() {
    let mut h = panel_under(800.0, Zoom::PerRow(4), GridResize::FixedSize);
    assert_eq!(h.state().columns, 4, "the count shows as it did");
    let Zoom::Points(p) = h.state().state.zoom() else {
        panic!("the zoom is now a size: {:?}", h.state().state.zoom());
    };
    assert_eq!(
        h.state().sizes,
        vec![p],
        "the size is signalled once, to keep"
    );
    focus(&mut h);
    press(&mut h, Key::Minus);
    assert!(h.state().columns > 4, "a step out shows more columns");
    assert!(
        matches!(h.state().state.zoom(), Zoom::Points(q) if q < p),
        "and keeps a smaller size"
    );
    assert_eq!(h.state().sizes.len(), 2);
    assert!(h.state().zooms.is_empty(), "no count is signalled");
}
