//! The line between a side area and the centre while hovered and
//! dragged, and the side area's grip: their states come from the kit's
//! tokens (`side-area.hover.border-*`, `side-area.active.border-*`,
//! `area-grip.hover.fill`), egui's widget strokes when the theme says
//! nothing. The same tokens draw the free edge of a side area lying over
//! the centre; the guide shown while a drag would fold the area takes
//! `side-area.fold.border-*`.

use egui::{Color32, Pos2, Rect, Shape, Stroke};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::theme::ThemeSpec;
use egui_pop_core::tokens::kit::KitData;
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::layout::Placement;
use egui_pop_model::panels::{Area, PanelSpec};

const FILES: PanelId = PanelId::from_static("demo.files");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(FILES, "Files", Area::StartSide), |ui, _| {
            ui.label("files body");
        })?;
        Ok(())
    }
}

/// The app, with a kit of `yaml` as its theme when given.
fn harness(yaml: Option<&str>) -> Harness<'static, App> {
    let mut builder = App::builder().plugins(DefaultPlugins).plugin(Demo);
    if let Some(yaml) = yaml {
        builder = builder
            .kit(KitData::from_text("sash", yaml))
            .theme(ThemeSpec::new("sash"));
    }
    let app = builder.build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

/// The start side area's sash, as assistive technology sees it.
fn sash(h: &Harness<'_, App>) -> Rect {
    let side = h.state().handle().area_rect(Area::StartSide).unwrap();
    h.query_all_by_label_contains("Resize")
        .map(|n| n.rect())
        .min_by(|a, b| {
            let d = |r: &Rect| (r.center().x - side.right()).abs();
            d(a).total_cmp(&d(b))
        })
        .expect("the start side area's sash")
}

fn walk<'a>(s: &'a Shape, out: &mut Vec<&'a Shape>) {
    if let Shape::Vec(v) = s {
        v.iter().for_each(|s| walk(s, out));
    } else {
        out.push(s);
    }
}

fn shapes(h: &Harness<'_, App>) -> Vec<Shape> {
    let output = h.output();
    let mut out = Vec::new();
    for c in &output.shapes {
        walk(&c.shape, &mut out);
    }
    out.into_iter().cloned().collect()
}

/// The strokes of the full-height lines whose x lies in `xs`.
fn lines(h: &Harness<'_, App>, xs: std::ops::RangeInclusive<f32>, height: f32) -> Vec<Stroke> {
    shapes(h)
        .iter()
        .filter_map(|s| match s {
            Shape::LineSegment { points, stroke }
                if xs.contains(&points[0].x)
                    && (points[0].x - points[1].x).abs() < 0.5
                    && (points[0].y - points[1].y).abs() > height / 2.0 =>
            {
                Some(Stroke::new(stroke.width, stroke.color))
            }
            _ => None,
        })
        .collect()
}

fn fills(h: &Harness<'_, App>) -> Vec<Color32> {
    shapes(h)
        .iter()
        .filter_map(|s| match s {
            Shape::Rect(r) => Some(r.fill),
            _ => None,
        })
        .collect()
}

fn send(h: &mut Harness<'_, App>, e: egui::Event) {
    h.input_mut().events.push(e);
    h.step();
}

fn hover(h: &mut Harness<'_, App>, at: Pos2) {
    send(h, egui::Event::PointerMoved(at));
    h.run();
}

/// Presses on `at` and drags right by 20 points, held there.
fn drag(h: &mut Harness<'_, App>, at: Pos2) {
    send(h, egui::Event::PointerMoved(at));
    send(
        h,
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    for i in 1..=4 {
        send(
            h,
            egui::Event::PointerMoved(at + egui::vec2(5.0 * i as f32, 0.0)),
        );
    }
    h.step();
}

const NO_LINE: &str = r##"
side-area:
  hover:
    border-width: {$type: "dimension", $value: {value: 0, unit: "px"}}
  active:
    border-width: {$type: "dimension", $value: {value: 0, unit: "px"}}
"##;

#[test]
fn without_tokens_the_sash_line_keeps_egui_s_strokes() {
    let mut h = harness(None);
    let rect = sash(&h);
    let widgets = h.ctx.global_style().visuals.widgets.clone();
    let xs = rect.left() - 4.0..=rect.right() + 4.0;
    hover(&mut h, rect.center());
    assert!(
        lines(&h, xs.clone(), rect.height()).contains(&widgets.hovered.fg_stroke),
        "hovered: {:?}",
        lines(&h, xs, rect.height())
    );
    drag(&mut h, rect.center());
    let xs = rect.left() - 4.0..=rect.right() + 24.0;
    assert!(
        lines(&h, xs.clone(), rect.height()).contains(&widgets.active.fg_stroke),
        "dragged: {:?}",
        lines(&h, xs, rect.height())
    );
}

#[test]
fn a_zero_width_sash_line_is_not_painted_on_hover_or_drag() {
    let mut h = harness(Some(NO_LINE));
    let rect = sash(&h);
    let xs = rect.left() - 4.0..=rect.right() + 4.0;
    assert!(
        !lines(&h, xs.clone(), rect.height()).is_empty(),
        "the line at rest stays"
    );
    hover(&mut h, rect.center());
    assert_eq!(lines(&h, xs, rect.height()), vec![], "hovered");
    drag(&mut h, rect.center());
    let xs = rect.left() - 4.0..=rect.right() + 24.0;
    assert_eq!(lines(&h, xs, rect.height()), vec![], "dragged");
}

#[test]
fn the_grip_takes_its_hover_fill() {
    let rest = Color32::from_rgb(0, 255, 0);
    let hovered = Color32::from_rgb(255, 0, 255);
    let mut h = harness(Some(
        r##"
area-grip:
  rest:
    fill: {$type: "color", $value: "#00ff00"}
  hover:
    fill: {$type: "color", $value: "#ff00ff"}
"##,
    ));
    let rect = sash(&h);
    // Away from the edge: at rest.
    hover(&mut h, egui::pos2(450.0, 300.0));
    assert!(fills(&h).contains(&rest), "at rest");
    assert!(!fills(&h).contains(&hovered), "at rest");
    hover(&mut h, rect.center());
    assert!(fills(&h).contains(&hovered), "hovered");
}

/// Presses on `at` and drags to `to` in steps, held there.
fn drag_to(h: &mut Harness<'_, App>, at: Pos2, to: Pos2) {
    send(h, egui::Event::PointerMoved(at));
    send(
        h,
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    for i in 1..=8 {
        send(h, egui::Event::PointerMoved(at.lerp(to, i as f32 / 8.0)));
    }
    h.step();
}

/// Drags the start side area's sash to the window's left edge, where the
/// area folds on release, and returns the full-height lines left of where
/// the sash was: the fold guide among them.
fn fold_guide_lines(h: &mut Harness<'_, App>) -> Vec<Stroke> {
    let rect = sash(h);
    drag_to(h, rect.center(), egui::pos2(4.0, rect.center().y));
    lines(h, 0.0..=rect.left() - 40.0, rect.height())
}

#[test]
fn without_tokens_the_fold_guide_keeps_egui_stack_s_stroke() {
    let mut h = harness(None);
    let guide = h.ctx.global_style().visuals.selection.stroke.color;
    let lines = fold_guide_lines(&mut h);
    assert!(
        lines.contains(&Stroke::new(1.0, guide)),
        "the fold guide: {lines:?}"
    );
}

#[test]
fn the_fold_guide_takes_its_tokens() {
    let mut h = harness(Some(
        r##"
side-area:
  fold:
    border-width: {$type: "dimension", $value: {value: 3, unit: "px"}}
    border-color: {$type: "color", $value: "#ff0000"}
"##,
    ));
    let lines = fold_guide_lines(&mut h);
    assert!(
        lines.contains(&Stroke::new(3.0, Color32::RED)),
        "the fold guide: {lines:?}"
    );
}

#[test]
fn a_zero_width_fold_guide_is_not_painted() {
    // Red, so the guide tells itself from the dragged line beside it.
    let mut h = harness(Some(
        r##"
side-area:
  fold:
    border-width: {$type: "dimension", $value: {value: 0, unit: "px"}}
    border-color: {$type: "color", $value: "#ff0000"}
"##,
    ));
    let lines = fold_guide_lines(&mut h);
    assert!(
        lines.iter().all(|s| s.color != Color32::RED),
        "no fold guide: {lines:?}"
    );
}

/// The app with the start side area lying over the centre, and the edge
/// that resizes it.
fn over(yaml: Option<&str>) -> (Harness<'static, App>, Rect) {
    let mut h = harness(yaml);
    let app = h.state().handle().clone();
    let out = app.place_area(Area::StartSide, Placement::Over).unwrap();
    assert!(out.ok(), "{out:?}");
    h.run();
    h.run();
    assert_eq!(app.area_placement(Area::StartSide), Placement::Over);
    let edge = sash(&h);
    (h, edge)
}

const OVER_LINE: &str = r##"
side-area:
  hover:
    border-width: {$type: "dimension", $value: {value: 3, unit: "px"}}
    border-color: {$type: "color", $value: "#ff0000"}
  active:
    border-width: {$type: "dimension", $value: {value: 4, unit: "px"}}
    border-color: {$type: "color", $value: "#00ff00"}
"##;

#[test]
fn the_edge_of_an_area_lying_over_takes_the_hover_and_drag_line() {
    let (mut h, rect) = over(Some(OVER_LINE));
    let xs = rect.left() - 6.0..=rect.right() + 6.0;
    hover(&mut h, rect.center());
    let hovered = Stroke::new(3.0, Color32::RED);
    assert!(
        lines(&h, xs.clone(), rect.height()).contains(&hovered),
        "hovered: {:?}",
        lines(&h, xs, rect.height())
    );
    drag(&mut h, rect.center());
    let xs = rect.left() - 6.0..=rect.right() + 26.0;
    let dragged = Stroke::new(4.0, Color32::from_rgb(0, 255, 0));
    assert!(
        lines(&h, xs.clone(), rect.height()).contains(&dragged),
        "dragged: {:?}",
        lines(&h, xs, rect.height())
    );
}

#[test]
fn without_tokens_the_edge_of_an_area_lying_over_draws_no_line() {
    let (mut h, rect) = over(None);
    let widgets = h.ctx.global_style().visuals.widgets.clone();
    let xs = rect.left() - 6.0..=rect.right() + 6.0;
    hover(&mut h, rect.center());
    let seen = lines(&h, xs, rect.height());
    assert!(
        !seen.contains(&widgets.hovered.fg_stroke),
        "hovered: {seen:?}"
    );
    drag(&mut h, rect.center());
    let xs = rect.left() - 6.0..=rect.right() + 26.0;
    let seen = lines(&h, xs, rect.height());
    assert!(
        !seen.contains(&widgets.active.fg_stroke),
        "dragged: {seen:?}"
    );
}
