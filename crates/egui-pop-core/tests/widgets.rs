//! egui's small widgets drawn through the kit: they keep egui's layout,
//! clicks, keyboard and what they tell assistive technology.

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::widgets::{self, Checkbox, ToggleChip};

#[derive(Default)]
struct Values {
    checked: bool,
    choice: usize,
    picked: usize,
    case: bool,
}

fn app() -> Harness<'static, Values> {
    Harness::new_ui_state(
        |ui, v: &mut Values| {
            widgets::checkbox(ui, &mut v.checked, "Wrap lines");
            for (i, name) in ["Left", "Right"].iter().enumerate() {
                widgets::radio_value(ui, &mut v.choice, i, *name);
            }
            for (i, name) in ["First", "Second"].iter().enumerate() {
                widgets::selectable_value(ui, &mut v.picked, i, *name);
            }
            ui.add(ToggleChip::new(&mut v.case, "Aa").name("Match case"));
            widgets::combo_box_labelled("combo", "Mode")
                .selected_text(["First", "Second"][v.picked])
                .show_ui(ui, |ui| {
                    for (i, name) in ["First", "Second"].iter().enumerate() {
                        widgets::selectable_value(ui, &mut v.picked, i, *name);
                    }
                });
        },
        Values::default(),
    )
}

#[test]
fn a_checkbox_toggles_on_a_click_and_on_space() {
    let mut h = app();
    h.run();
    let node = h.get_by_label("Wrap lines");
    assert_eq!(node.accesskit_node().role(), Role::CheckBox);
    assert_eq!(node.accesskit_node().toggled(), Some(Toggled::False));
    node.click();
    h.run();
    assert!(h.state().checked);
    assert_eq!(
        h.get_by_label("Wrap lines").accesskit_node().toggled(),
        Some(Toggled::True)
    );
    h.get_by_label("Wrap lines").focus();
    h.run();
    h.key_press(egui::Key::Space);
    h.run();
    assert!(!h.state().checked);
}

#[test]
fn a_checkbox_is_as_tall_as_egui_s() {
    let mut h = Harness::new_ui(|ui| {
        let mut a = true;
        ui.add(Checkbox::new(&mut a, "Ours"));
        let mut b = true;
        ui.add(egui::Checkbox::new(&mut b, "egui's"));
    });
    h.run();
    let ours = h.get_by_label("Ours").rect();
    let egui = h.get_by_label("egui's").rect();
    assert_eq!(ours.height(), egui.height());
}

#[test]
fn a_radio_value_picks_its_alternative() {
    let mut h = app();
    h.run();
    let right = h.get_by_label("Right");
    assert_eq!(right.accesskit_node().role(), Role::RadioButton);
    assert_eq!(right.accesskit_node().toggled(), Some(Toggled::False));
    right.click();
    h.run();
    assert_eq!(h.state().choice, 1);
    assert_eq!(
        h.get_by_label("Right").accesskit_node().toggled(),
        Some(Toggled::True)
    );
}

#[test]
fn a_selectable_row_is_a_selected_button() {
    let mut h = app();
    h.run();
    let first = h.get_by_label("First");
    assert_eq!(first.accesskit_node().role(), Role::Button);
    assert_eq!(first.accesskit_node().toggled(), Some(Toggled::True));
    h.get_by_label("Second").click();
    h.run();
    assert_eq!(h.state().picked, 1);
}

#[test]
fn a_toggle_chip_is_a_named_checkbox() {
    let mut h = app();
    h.run();
    let chip = h.get_by_label("Match case");
    assert_eq!(chip.accesskit_node().role(), Role::CheckBox);
    chip.click();
    h.run();
    assert!(h.state().case);
    assert_eq!(
        h.get_by_label("Match case").accesskit_node().toggled(),
        Some(Toggled::True)
    );
}

#[test]
fn a_combo_box_with_the_kit_s_arrow_opens_and_picks() {
    let mut h = app();
    h.run();
    h.get_by_role(Role::ComboBox).click();
    h.run();
    // The list shows the choices again, as selectable rows.
    let seconds = h.query_all_by_label("Second").count();
    assert_eq!(seconds, 2);
    h.query_all_by_label("Second").last().unwrap().click();
    h.run();
    assert_eq!(h.state().picked, 1);
}
