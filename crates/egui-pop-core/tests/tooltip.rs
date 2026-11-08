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
    let mut h = Harness::builder()
        .with_size(egui::vec2(500.0, 300.0))
        .build_ui(|ui| {
            ui.add_space(40.0);
            ui.horizontal(|ui| {
                ui.add_space(100.0);
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
