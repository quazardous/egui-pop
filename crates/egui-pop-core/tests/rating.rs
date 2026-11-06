//! Rating stars driven as a user would: the preview under the pointer,
//! a change, a clear on the current star, the keys, and a read-only
//! rating that only reads.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::rating::{RatingStars, StarLook};

#[derive(Default)]
struct Stars {
    value: u8,
    read_only: bool,
    preview: Option<u8>,
    hovered: bool,
}

fn app(value: u8, read_only: bool) -> Harness<'static, Stars> {
    Harness::builder()
        .with_size(egui::vec2(300.0, 80.0))
        .build_ui_state(
            |ui, s: &mut Stars| {
                ui.label("Above");
                let out = RatingStars::new(s.value)
                    .read_only(s.read_only)
                    .size(20.0)
                    .show(ui);
                s.preview = out.preview;
                s.hovered = out.hovered;
                if let Some(v) = out.changed {
                    s.value = v;
                }
            },
            Stars {
                value,
                read_only,
                ..Stars::default()
            },
        )
}

/// The stars' names, in order: each star is a button named by what a
/// click on it gives.
fn star_names(h: &Harness<'static, Stars>) -> Vec<String> {
    h.query_all_by_role(Role::Button)
        .filter_map(|n| n.accesskit_node().label())
        .collect()
}

#[test]
fn each_star_says_what_a_click_gives_and_the_current_one_clears() {
    let mut h = app(3, false);
    h.run();
    assert_eq!(
        star_names(&h),
        [
            "Set 1 of 5",
            "Set 2 of 5",
            "Clear rating",
            "Set 4 of 5",
            "Set 5 of 5"
        ]
    );
    // The row is a slider for assistive technologies.
    let row = h.get_by_role_and_label(Role::Slider, "Rating");
    assert_eq!(row.accesskit_node().numeric_value(), Some(3.0));
}

#[test]
fn hovering_a_lower_star_previews_it_and_keeps_the_current_value_as_a_ghost() {
    let mut h = app(3, false);
    h.run();
    let first = h.get_by_label("Set 1 of 5").rect().center();
    h.hover_at(first);
    h.run();
    assert!(h.state().hovered);
    assert_eq!(h.state().preview, Some(1));
    let looks: Vec<StarLook> = (1..=5)
        .map(|s| StarLook::of(s, h.state().value, h.state().preview))
        .collect();
    assert_eq!(
        looks,
        [
            StarLook::Preview,
            StarLook::Ghost,
            StarLook::Ghost,
            StarLook::Empty,
            StarLook::Empty
        ]
    );
    // Between two stars the preview stays on one of them: never none.
    let second = h.get_by_label("Set 2 of 5").rect();
    h.hover_at(egui::pos2(second.left(), second.center().y));
    h.run();
    assert!(matches!(h.state().preview, Some(1 | 2)));
    // The current star previews the clear.
    let third = h.get_by_label("Clear rating").rect().center();
    h.hover_at(third);
    h.run();
    assert_eq!(h.state().preview, Some(0));
    // Away from the stars: no preview.
    let above = h.get_by_label("Above").rect().center();
    h.hover_at(above);
    h.run();
    assert!(!h.state().hovered);
    assert_eq!(h.state().preview, None);
}

#[test]
fn a_click_changes_the_rating_and_a_click_on_the_current_star_clears_it() {
    let mut h = app(3, false);
    h.run();
    h.get_by_label("Set 1 of 5").click();
    h.run();
    assert_eq!(h.state().value, 1);
    h.get_by_label("Set 4 of 5").click();
    h.run();
    assert_eq!(h.state().value, 4);
    // The fourth star now clears.
    h.get_by_label("Clear rating").click();
    h.run();
    assert_eq!(h.state().value, 0);
    assert_eq!(star_names(&h)[0], "Set 1 of 5", "nothing to clear");
}

#[test]
fn focused_stars_take_0_to_5_and_the_arrows() {
    let mut h = app(2, false);
    h.run();
    h.get_by_role_and_label(Role::Slider, "Rating").focus();
    h.run();
    h.key_press(egui::Key::Num5);
    h.run();
    assert_eq!(h.state().value, 5);
    h.key_press(egui::Key::ArrowLeft);
    h.run();
    assert_eq!(h.state().value, 4);
    h.key_press(egui::Key::Num0);
    h.run();
    assert_eq!(h.state().value, 0);
}

#[test]
fn a_read_only_rating_reads_its_value_and_ignores_the_pointer() {
    let mut h = app(3, true);
    h.run();
    assert!(star_names(&h).is_empty(), "no star to click");
    let shown = h.get_by_role_and_label(Role::Label, "3 of 5");
    let rect = shown.rect();
    h.hover_at(rect.left_center() + egui::vec2(5.0, 0.0));
    h.run();
    assert_eq!(h.state().preview, None, "no preview");
    h.get_by_role_and_label(Role::Label, "3 of 5").click();
    h.run();
    assert_eq!(h.state().value, 3);
    assert!(h.query_by_role_and_label(Role::Slider, "Rating").is_none());
}
