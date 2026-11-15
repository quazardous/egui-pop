//! Rows and grids of `UiPop` (`egui_pop_core::ui::layout`): cells on one text baseline,
//! tracks by content, fraction and em, equal widths, a header/value grid
//! folding to one column when narrow.

use egui::epaint::Shape;
use egui::{Context, FullOutput, RawInput, Rect, Ui, pos2, vec2};
use egui_pop_core::theme::{self, ThemeSpec, Tokens};
use egui_pop_core::ui::{Align, Grid, Length, Row, Space, Track, UiPop};

fn context() -> Context {
    let ctx = Context::default();
    theme::install(&ctx, &ThemeSpec::new("pop-dark"));
    ctx
}

/// Three frames `width` wide, the last one's output.
fn frames(ctx: &Context, width: f32, mut add: impl FnMut(&mut Ui)) -> FullOutput {
    let input = || RawInput {
        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 600.0))),
        ..RawInput::default()
    };
    let mut out = ctx.run_ui(input(), &mut add);
    for _ in 0..2 {
        out.textures_delta.clear();
        out = ctx.run_ui(input(), &mut add);
    }
    out.textures_delta.clear();
    out
}

/// Every text painted, with its first baseline and its rectangle.
fn texts(out: &FullOutput) -> Vec<(String, f32, Rect)> {
    fn visit(shape: &Shape, all: &mut Vec<(String, f32, Rect)>) {
        match shape {
            Shape::Vec(v) => v.iter().for_each(|s| visit(s, all)),
            Shape::Text(t) => {
                if let Some(row) = t.galley.rows.iter().find(|r| !r.row.glyphs.is_empty()) {
                    let y = t.pos.y + row.pos.y + row.row.glyphs[0].pos.y;
                    let rect = t.galley.rect.translate(t.pos.to_vec2());
                    all.push((t.galley.text().to_owned(), y, rect));
                }
            }
            _ => {}
        }
    }
    let mut all = Vec::new();
    out.shapes.iter().for_each(|c| visit(&c.shape, &mut all));
    all
}

fn text<'a>(all: &'a [(String, f32, Rect)], s: &str) -> &'a (String, f32, Rect) {
    all.iter()
        .find(|(t, ..)| t == s)
        .unwrap_or_else(|| panic!("{s} not painted in {all:?}"))
}

fn spread(all: &[(String, f32, Rect)], of: &[&str]) -> f32 {
    let ys: Vec<f32> = of.iter().map(|s| text(all, s).1).collect();
    let max = ys.iter().copied().fold(f32::MIN, f32::max);
    let min = ys.iter().copied().fold(f32::MAX, f32::min);
    max - min
}

fn line(ui: &mut Ui, align: Align, name: &mut String) {
    let tracks = [
        Track::CONTENT,
        Track::fr(1.0),
        Track::CONTENT,
        Track::CONTENT,
    ];
    ui.row(Row::new(tracks).align(align), |r| {
        r.cell(|ui| ui.section_title("Name"));
        r.cell(|ui| ui.text_edit_singleline(name));
        r.cell(|ui| ui.button("Rename"));
        r.cell(|ui| ui.meta("24 MB"));
    });
}

const LINE: [&str; 4] = ["Name", "IMG_0001", "Rename", "24 MB"];

#[test]
fn a_row_puts_a_title_a_field_a_button_and_small_print_on_one_baseline() {
    let ctx = context();
    let mut name = "IMG_0001".to_owned();
    let out = frames(&ctx, 600.0, |ui| line(ui, Align::Baseline, &mut name));
    let all = texts(&out);
    assert!(spread(&all, &LINE) <= 0.5, "baselines apart: {all:?}");
    // At rest, one pass a frame: nothing measured twice.
    let before = ctx.cumulative_pass_nr();
    frames(&ctx, 600.0, |ui| line(ui, Align::Baseline, &mut name));
    assert_eq!(ctx.cumulative_pass_nr() - before, 3);
}

#[test]
fn without_baseline_alignment_the_same_row_is_not_on_one_line() {
    // The test above bites: tops alone leave the texts apart.
    let ctx = context();
    let mut name = "IMG_0001".to_owned();
    let out = frames(&ctx, 600.0, |ui| line(ui, Align::Start, &mut name));
    assert!(spread(&texts(&out), &LINE) > 0.5);
}

#[test]
fn a_header_value_grid_aligns_values_after_the_widest_header_on_each_baseline() {
    let ctx = context();
    let mut name = "IMG_0001".to_owned();
    let mut value_x = 0.0;
    let out = frames(&ctx, 600.0, |ui| {
        let grid =
            Grid::new("details", [Track::CONTENT, Track::fr(1.0)]).fold_below(Length::Em(30.0));
        ui.grid(grid, |g| {
            g.header("Kind");
            g.cell(|ui| ui.label("Raw image"));
            g.header("Name of the file");
            g.cell(|ui| ui.text_edit_singleline(&mut name));
            g.header("Size");
            g.cell(|ui| ui.button("24 MB"));
        });
        value_x = Space::Group.points(&Tokens::of(ui.ctx()));
    });
    let all = texts(&out);
    for pair in [
        ["Kind", "Raw image"],
        ["Name of the file", "IMG_0001"],
        ["Size", "24 MB"],
    ] {
        assert!(spread(&all, &pair) <= 0.5, "{pair:?} apart: {all:?}");
    }
    // Values start one gap after the widest header, all at one x.
    let widest = text(&all, "Name of the file").2.right();
    let raw = text(&all, "Raw image").2.left();
    assert!(raw >= widest + value_x - 0.5, "{raw} vs {widest}");
    assert!((raw - text(&all, "IMG_0001").2.left()).abs() < 12.0);
    // Lines go down.
    assert!(text(&all, "Kind").1 < text(&all, "Name of the file").1);
    assert!(text(&all, "Name of the file").1 < text(&all, "Size").1);
}

#[test]
fn a_grid_narrower_than_its_fold_stacks_headers_over_values() {
    let ctx = context();
    let out = frames(&ctx, 200.0, |ui| {
        let grid =
            Grid::new("details", [Track::CONTENT, Track::fr(1.0)]).fold_below(Length::Em(30.0));
        ui.grid(grid, |g| {
            g.header("Kind");
            g.cell(|ui| ui.label("Raw image"));
            g.header("Size");
            g.cell(|ui| ui.label("24 MB"));
        });
    });
    let all = texts(&out);
    let (kind, raw, size) = (
        text(&all, "Kind").2,
        text(&all, "Raw image").2,
        text(&all, "Size").2,
    );
    assert_eq!(kind.left(), raw.left());
    assert!(raw.top() >= kind.bottom());
    assert!(size.top() > raw.bottom());
}

#[test]
fn tracks_size_by_fraction_em_and_bounds() {
    let ctx = context();
    let mut rects = Vec::new();
    let mut width = 0.0;
    let mut em = 0.0;
    let mut gap = 0.0;
    frames(&ctx, 600.0, |ui| {
        width = ui.available_width();
        let t = Tokens::of(ui.ctx());
        em = t.text_role.body.size;
        gap = Space::Inline.points(&t);
        rects.clear();
        let tracks = [
            Track::em(6.0),
            Track::fr(1.0),
            Track::fr(1.0).max(Length::Em(4.0)),
        ];
        ui.row(Row::new(tracks), |r| {
            for _ in 0..3 {
                r.cell(|ui| rects.push(ui.max_rect()));
            }
        });
    });
    assert!((rects[0].width() - 6.0 * em).abs() < 0.01);
    assert!((rects[2].width() - 4.0 * em).abs() < 0.01);
    let rest = width - 10.0 * em - 2.0 * gap;
    assert!((rects[1].width() - rest).abs() < 1.0, "{rects:?}");
    assert!(rects[1].left() - rects[0].right() >= gap - 1.0);
}

#[test]
fn equal_widths_make_a_button_bar_and_follow_its_labels() {
    let ctx = context();
    let run = |ctx: &Context, long: &'static str| {
        let mut buttons = Vec::new();
        let mut right = 0.0;
        frames(ctx, 600.0, |ui| {
            right = ui.max_rect().right();
            buttons.clear();
            let bar = Row::new([Track::CONTENT; 3])
                .justify(Align::End)
                .equal_widths();
            ui.row(bar, |r| {
                for label in ["OK", "Cancel", long] {
                    buttons.push(r.cell(|ui| ui.button(label)).rect);
                }
            });
        });
        (buttons, right)
    };
    let (wide, right) = run(&ctx, "Apply to all");
    for b in &wide {
        assert!((b.width() - wide[2].width()).abs() < 0.5, "{wide:?}");
    }
    assert!(
        (wide[2].right() - right).abs() < 1.0,
        "at the end: {wide:?}"
    );
    let (narrow, _) = run(&ctx, "Go");
    for b in &narrow {
        assert!((b.width() - narrow[1].width()).abs() < 0.5, "{narrow:?}");
    }
    assert!(
        narrow[0].width() < wide[0].width(),
        "shrinks with its labels"
    );
}
