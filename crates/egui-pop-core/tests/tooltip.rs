//! Kit tooltips, headless: drawn with the `tooltip` component's text
//! size and placement, egui's look by default.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::theme::{self, ThemeSpec, Themes};
use egui_pop_core::tooltip::TooltipExt as _;

/// GPUI's tooltips: smaller, under the pointer, 8 across and 4 down.
const SMALL: &str = r#"
$extends: "pop-dark"
tooltip:
  placement: {$type: "string", $value: "pointer"}
  offset: {$type: "dimension", $value: {value: 6, unit: "px"}}
  font-size: {$type: "dimension", $value: {value: 11, unit: "px"}}
  padding-x: {$type: "dimension", $value: {value: 8, unit: "px"}}
  padding-y: {$type: "dimension", $value: {value: 4, unit: "px"}}
"#;

fn harness(theme: Option<&str>) -> Harness<'static> {
    harness_at(theme, 100.0, 40.0)
}

/// The button `left` points across the window and `top` down.
fn harness_at(theme: Option<&str>, left: f32, top: f32) -> Harness<'static> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(500.0, 300.0))
        .build_ui(move |ui| {
            ui.add_space(top);
            ui.horizontal(|ui| {
                ui.add_space(left);
                let _ = ui
                    .button("A rather wide new ticket button")
                    .tip("Create a ticket");
            });
        });
    let mut themes = Themes::default();
    let spec = match theme {
        Some(yaml) => {
            themes
                .add_text("small", yaml)
                .expect("the test theme reads");
            ThemeSpec::new("small")
        }
        None => ThemeSpec::new("pop-dark"),
    };
    theme::install_with(&h.ctx, &themes, &spec);
    // No delay: the pointer opens it at once.
    h.ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    h.run();
    h
}

/// The tip's painted text: its rectangle and size.
fn tip(h: &Harness<'_>) -> (egui::Rect, f32) {
    h.output()
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::Shape::Text(s) if s.galley.job.text == "Create a ticket" => Some((
                c.shape.visual_bounding_rect(),
                s.galley.job.sections[0].format.font_id.size,
            )),
            _ => None,
        })
        .expect("the tooltip is painted")
}

fn hover_at(h: &mut Harness<'_>, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
    for _ in 0..4 {
        h.run();
    }
}

#[test]
fn a_tip_describes_its_widget_and_follows_egui_s_look_by_default() {
    let mut h = harness(None);
    let button = h.get_by_label("A rather wide new ticket button");
    assert_eq!(
        button.accesskit_node().description().as_deref(),
        Some("Create a ticket")
    );
    let rect = button.rect();
    let pointer = rect.center();
    hover_at(&mut h, pointer);
    let (text, size) = tip(&h);
    let body = h.ctx.global_style().text_styles[&egui::TextStyle::Body].size;
    assert_eq!(size, body);
    // Under the widget, from its start edge, as egui's.
    assert!(text.top() > rect.bottom(), "{text:?} under {rect:?}");
    assert!(
        (text.left() - rect.left()).abs() < 20.0,
        "{text:?} {rect:?}"
    );
}

#[test]
fn a_theme_draws_tips_smaller_and_under_the_pointer() {
    let mut h = harness(Some(SMALL));
    let rect = h.get_by_label("A rather wide new ticket button").rect();
    let pointer = egui::pos2(rect.center().x + 40.0, rect.center().y);
    hover_at(&mut h, pointer);
    let (text, size) = tip(&h);
    assert_eq!(size, 11.0);
    // From the pointer: its offset, then the padding (and the border).
    let left = pointer.x + 8.0;
    assert!(
        (text.left() - left).abs() < 3.0,
        "{text:?} from {pointer:?}"
    );
    assert!(text.top() > pointer.y + 6.0, "{text:?} from {pointer:?}");
    assert!(
        text.top() < pointer.y + 6.0 + 4.0 + 6.0,
        "{text:?} from {pointer:?}"
    );
}

/// GPUI's tooltips at the pointer: their corner 13 across and 13 down
/// from it, on its other side where the window ends.
const GPUI: &str = r#"
$extends: "pop-dark"
tooltip:
  placement: {$type: "string", $value: "pointer"}
  offset: {$type: "dimension", $value: {value: 13, unit: "px"}}
  offset-x: {$type: "dimension", $value: {value: 13, unit: "px"}}
  padding-x: {$type: "dimension", $value: {value: 8, unit: "px"}}
  padding-y: {$type: "dimension", $value: {value: 4, unit: "px"}}
  rest:
    border-width: {$type: "dimension", $value: {value: 0, unit: "px"}}
"#;

/// The tooltip's frame: the smallest rectangle painted around its text.
fn frame(h: &Harness<'_>) -> egui::Rect {
    fn rects(shape: &egui::Shape, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Rect(r) => out.push(r.rect),
            egui::Shape::Vec(v) => v.iter().for_each(|s| rects(s, out)),
            _ => {}
        }
    }
    let text = tip(h).0;
    let mut all = Vec::new();
    for c in &h.output().shapes {
        rects(&c.shape, &mut all);
    }
    all.into_iter()
        .filter(|r| r.contains_rect(text))
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap_or_else(|| panic!("the tooltip's frame is painted around {text:?}"))
}

#[test]
fn a_pointer_tip_starts_its_offsets_across_and_down_from_the_pointer() {
    let mut h = harness(Some(GPUI));
    let rect = h.get_by_label("A rather wide new ticket button").rect();
    let pointer = egui::pos2(rect.center().x, rect.center().y);
    hover_at(&mut h, pointer);
    let f = frame(&h);
    assert!(
        (f.left() - (pointer.x + 13.0)).abs() < 2.0,
        "{f:?} from {pointer:?}"
    );
    assert!(
        (f.top() - (pointer.y + 13.0)).abs() < 2.0,
        "{f:?} from {pointer:?}"
    );
}

#[test]
fn a_pointer_tip_flips_to_the_pointer_s_start_side_at_the_window_s_end_edge() {
    // The button at the window's right edge: no room for the tip on the
    // pointer's right.
    let mut h = harness_at(Some(GPUI), 270.0, 40.0);
    let rect = h.get_by_label("A rather wide new ticket button").rect();
    let pointer = egui::pos2(rect.right() - 4.0, rect.center().y);
    assert!(pointer.x + 13.0 + 80.0 > 500.0, "{pointer:?}");
    hover_at(&mut h, pointer);
    let f = frame(&h);
    assert!(
        (f.right() - (pointer.x - 13.0)).abs() < 2.0,
        "{f:?} ends before {pointer:?}"
    );
    assert!(
        (f.top() - (pointer.y + 13.0)).abs() < 2.0,
        "{f:?} from {pointer:?}"
    );
}

#[test]
fn a_pointer_tip_flips_above_the_pointer_at_the_window_s_bottom_edge() {
    // The button low in the window: no room for the tip under it.
    let mut h = harness_at(Some(GPUI), 100.0, 270.0);
    let rect = h.get_by_label("A rather wide new ticket button").rect();
    let pointer = rect.center();
    hover_at(&mut h, pointer);
    let f = frame(&h);
    assert!(
        (f.bottom() - (pointer.y - 13.0)).abs() < 2.0,
        "{f:?} ends above {pointer:?}"
    );
    assert!(
        (f.left() - (pointer.x + 13.0)).abs() < 2.0,
        "{f:?} from {pointer:?}"
    );
}
