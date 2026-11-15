//! Placeholders (docs/progress.md, "Skeletons and empty states"): a
//! skeleton is a busy region named by what loads, in the content's
//! layout and still under a given clock; an empty state says its title
//! and sentence, offers its action once, and draws an unknown icon as
//! the placeholder, reported.

use egui::{Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::icon::Icon;
use egui_pop_core::motion;
use egui_pop_core::placeholder::{EmptyState, Skeleton};
use egui_pop_core::theme::{self, ThemeSpec, Tokens};

struct Shown {
    skeleton: Skeleton,
    rect: Option<Rect>,
}

const WIDTH: f32 = 300.0;

fn skeleton(skeleton: Skeleton) -> Harness<'static, Shown> {
    let mut h = Harness::builder()
        .with_size(vec2(WIDTH + 40.0, 400.0))
        .build_ui_state(
            |ui, s: &mut Shown| {
                ui.set_max_width(WIDTH);
                s.rect = Some(ui.add(s.skeleton.clone()).rect);
            },
            Shown {
                skeleton,
                rect: None,
            },
        );
    theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    let _clock = motion::freeze(&h.ctx, 1.0);
    h.run();
    h
}

fn size(s: Skeleton) -> egui::Vec2 {
    skeleton(s).state().rect.unwrap().size()
}

#[test]
fn a_skeleton_is_busy_and_named_by_what_loads() {
    let h = skeleton(Skeleton::text(3));
    let node = h.get_by_label("Loading");
    assert!(node.accesskit_node().is_busy(), "Loading by default, busy");
    let h = skeleton(Skeleton::rows(2).label("Loading the queue"));
    assert!(
        h.get_by_label("Loading the queue")
            .accesskit_node()
            .is_busy()
    );
}

#[test]
fn a_skeleton_takes_the_height_of_its_content_lines_or_rows() {
    let t = {
        let h = skeleton(Skeleton::text(1));
        Tokens::of(&h.ctx)
    };
    let (line, gap, block) = (t.font.body, t.space.s, t.size.control_height);
    let text = size(Skeleton::text(3));
    assert_eq!(text.y, 3.0 * line + 2.0 * gap);
    assert_eq!(text.x, WIDTH, "the available width by default");
    assert_eq!(
        size(Skeleton::text(0)),
        size(Skeleton::text(1)),
        "at least a line"
    );
    assert_eq!(size(Skeleton::rows(2)).y, 2.0 * block + gap);
    assert_eq!(
        size(Skeleton::rows(0)),
        size(Skeleton::rows(1)),
        "at least a row"
    );
}

#[test]
fn a_skeleton_keeps_the_width_given_and_rows_keep_room_for_their_block() {
    let block = {
        let h = skeleton(Skeleton::text(1));
        Tokens::of(&h.ctx).size.control_height
    };
    assert_eq!(
        size(Skeleton::text(1).width(40.0)).x,
        40.0,
        "a late value's short line"
    );
    assert_eq!(size(Skeleton::rows(1).width(10.0)).x, 2.0 * block);
}

#[test]
fn a_skeleton_is_still_under_a_given_clock() {
    let mut h = Harness::builder()
        .with_size(vec2(340.0, 200.0))
        .build_ui(|ui| {
            ui.add(Skeleton::rows(2));
        });
    theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    let clock = motion::freeze(&h.ctx, 1.0);
    h.run();
    let shapes = |h: &Harness<'_>| format!("{:?}", h.output().shapes);
    let before = shapes(&h);
    // Half a shimmer later: the band would have crossed half the shapes.
    clock.advance(f64::from(Tokens::of(&h.ctx).motion.skeleton_shimmer) / 2.0);
    h.step();
    assert_eq!(shapes(&h), before, "the same picture: no shimmer in tests");
}

#[derive(Default)]
struct Empty {
    action: bool,
    unknown_icon: bool,
    clicks: u32,
    room: f32,
}

fn empty(action: bool, unknown_icon: bool) -> Harness<'static, Empty> {
    let mut h = Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui_state(
            |ui, s: &mut Empty| {
                s.room = ui.available_rect_before_wrap().center().x;
                let mut e = if s.unknown_icon {
                    EmptyState::new("test-placeholder:no-such-icon", "No images in this folder")
                } else {
                    EmptyState::new(Icon::Folder, "No images in this folder")
                };
                e = e.text("Import images to see them here.");
                if s.action {
                    e = e.action("Import images");
                }
                if e.show(ui) {
                    s.clicks += 1;
                }
            },
            Empty {
                action,
                unknown_icon,
                ..Empty::default()
            },
        );
    theme::install(&h.ctx, &ThemeSpec::new("pop-dark"));
    h.run();
    h
}

#[test]
fn an_empty_state_says_its_title_and_sentence_centred() {
    let h = empty(false, false);
    let title = h.get_by_label("No images in this folder");
    assert!(
        (title.rect().center().x - h.state().room).abs() < 1.0,
        "centred in the available width: {:?}",
        title.rect()
    );
    assert!(
        h.query_by_label("Import images to see them here.")
            .is_some()
    );
    assert!(
        h.query_all_by_role(egui::accesskit::Role::Button)
            .next()
            .is_none(),
        "no action, no button"
    );
    assert!(
        h.query_by_label(Icon::Folder.name()).is_none(),
        "an icon is never a name"
    );
}

#[test]
fn an_empty_state_action_is_a_button_clicked_once() {
    let mut h = empty(true, false);
    let button = h.get_by_label("Import images");
    assert_eq!(
        button.accesskit_node().role(),
        egui::accesskit::Role::Button
    );
    button.click();
    h.run();
    assert_eq!(h.state().clicks, 1, "show says it was clicked, once");
}

#[test]
fn an_empty_state_draws_an_unknown_icon_name_as_the_placeholder_reported() {
    let h = empty(false, true);
    assert!(
        h.query_by_label("No images in this folder").is_some(),
        "drawn"
    );
    let unknown = egui_pop_core::icon::registry::unknown();
    let reported = unknown
        .iter()
        .find(|u| u.name == "test-placeholder:no-such-icon")
        .expect("the name is reported");
    assert!(
        reported.surfaces.iter().any(|s| s == "empty-state"),
        "{reported:?}"
    );
}
