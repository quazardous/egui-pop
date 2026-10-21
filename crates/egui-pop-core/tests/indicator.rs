//! Micro-indicators, headless: passive marks name their host, the
//! interactive ones follow the button rules, agents read their states.

use egui::Key;
use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::icon::Icon;
use egui_pop_core::indicator::{
    self, Anchor, Flag, Indicator, IndicatorExt as _, Led, Pip, Swatch, Tone,
};
use egui_pop_core::theme::{self, ThemeSpec};
use egui_pop_model::ids::CommandId;

#[derive(Default)]
struct State {
    unread: u64,
    clicks: u32,
    visible: bool,
    disabled: bool,
    /// Nothing drawn (a frame hiding every bar).
    hidden: bool,
}

fn harness(spec: ThemeSpec) -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 200.0))
        .build_ui_state(
            |ui, s: &mut State| {
                if s.hidden {
                    return;
                }
                ui.horizontal(|ui| {
                    ui.button("Notifications").with_indicator(
                        ui,
                        Indicator::count(s.unread).tone(Tone::Danger).id("unread"),
                        Anchor::TopEnd,
                    );
                    let mut reset = Indicator::dot(Tone::Accent)
                        .name("Changed")
                        .interactive("Reset exposure")
                        .tooltip("Reset the exposure to its default")
                        .command(CommandId::from_static("exposure.reset"))
                        .id("reset");
                    if s.disabled {
                        reset = reset.disabled("the exposure is locked");
                    }
                    if ui.add(reset).clicked() {
                        s.clicks += 1;
                    }
                    if ui
                        .add(
                            Indicator::toggle(s.visible, Icon::Visible, Icon::Hidden)
                                .interactive("Visible")
                                .tooltip("Show or hide the mask")
                                .id("visible"),
                        )
                        .clicked()
                    {
                        s.visible = !s.visible;
                    }
                    ui.add(Indicator::led(Led::Busy).id("node"));
                    ui.add(Indicator::pip(Pip::Progress(0.25)).id("export"));
                    ui.add(Indicator::flag(Flag::Modified).id("modified"));
                    ui.add(Indicator::swatch_of(Swatch::None).id("accent"));
                });
            },
            State {
                unread: 3,
                ..State::default()
            },
        );
    theme::install(&h.ctx, &spec);
    egui_pop_core::motion::freeze(&h.ctx, 1.0);
    h.run();
    h
}

#[test]
fn a_passive_badge_names_its_host_and_adds_no_node() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    let host = h.get_by_label("Notifications, 3");
    assert_eq!(host.accesskit_node().role(), Role::Button);
    assert!(
        h.query_all_by_label("3").next().is_none(),
        "the badge is not a node of its own"
    );
    // Zero hides the badge: the host keeps its own name.
    h.state_mut().unread = 0;
    h.run();
    h.get_by_label("Notifications");
    let r = indicator::find(&h.ctx, "unread").unwrap();
    assert!(!r.shown);
    // Past the cap, the badge says 99+ and the name the real count.
    h.state_mut().unread = 120;
    h.run();
    h.get_by_label("Notifications, 120");
    let r = indicator::find(&h.ctx, "unread").unwrap();
    assert_eq!(r.text.as_deref(), Some("99+"));
    assert_eq!(r.host.as_deref(), Some("Notifications"));
}

#[test]
fn an_interactive_dot_is_a_button_with_a_target_of_24_points() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    let node = h.get_by_label("Reset exposure");
    assert_eq!(node.accesskit_node().role(), Role::Button);
    assert_eq!(
        node.accesskit_node().description().as_deref(),
        Some("Changed")
    );
    let rect = node.rect();
    assert!(rect.width() >= 24.0 && rect.height() >= 24.0, "{rect:?}");
    h.get_by_label("Reset exposure").click();
    h.run();
    assert_eq!(h.state().clicks, 1);
    assert_eq!(
        indicator::take_commands(&h.ctx),
        vec![CommandId::from_static("exposure.reset")]
    );
    // The keyboard: focus, then Space and Enter.
    h.get_by_label("Reset exposure").focus();
    h.run();
    assert!(
        h.get_by_label("Reset exposure")
            .accesskit_node()
            .is_focused()
    );
    h.key_press(Key::Space);
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(h.state().clicks, 3);
}

#[test]
fn a_disabled_mark_says_why_and_takes_no_click() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    h.state_mut().disabled = true;
    h.run();
    let _ = indicator::take_commands(&h.ctx);
    let node = h.get_by_label("Reset exposure");
    assert!(node.accesskit_node().is_disabled());
    node.click();
    h.run();
    assert_eq!(h.state().clicks, 0);
    assert!(indicator::take_commands(&h.ctx).is_empty());
    let r = indicator::find(&h.ctx, "reset").unwrap();
    assert!(r.interactive && !r.enabled);
}

#[test]
fn a_toggle_is_a_check_box_with_its_state() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    let node = h.get_by_label("Visible");
    assert_eq!(node.accesskit_node().role(), Role::CheckBox);
    assert_eq!(node.accesskit_node().toggled(), Some(Toggled::False));
    node.click();
    h.run();
    assert!(h.state().visible);
    assert_eq!(
        h.get_by_label("Visible").accesskit_node().toggled(),
        Some(Toggled::True)
    );
    let r = indicator::find(&h.ctx, "visible").unwrap();
    assert_eq!((r.state.as_str(), r.value), ("on", Some(1.0)));
}

#[test]
fn passive_marks_are_read_by_their_meaning_and_recorded() {
    let h = harness(ThemeSpec::new("pop-dark"));
    for (label, id, state) in [
        ("Busy", "node", "busy"),
        ("25%", "export", "progress"),
        ("Modified", "modified", "modified"),
        ("No colour", "accent", "none"),
    ] {
        let node = h.get_by_label(label);
        assert_eq!(node.accesskit_node().role(), Role::Image, "{label}");
        let r = indicator::find(&h.ctx, id).unwrap();
        assert_eq!(r.state, state, "{id}");
        assert!(!r.interactive);
    }
    let node = indicator::find(&h.ctx, "node").unwrap();
    assert!(node.pulsing, "a busy LED pulses");
    assert_eq!(node.tone, Tone::Accent);
}

#[test]
fn reduced_motion_keeps_the_busy_state_as_a_still_ring() {
    let h = harness(ThemeSpec::new("pop-dark").reduced_motion(true));
    let node = indicator::find(&h.ctx, "node").unwrap();
    assert!(node.pulsing, "the state is kept, drawn still");
    assert!(egui_pop_core::motion::reduced(&h.ctx));
}

#[test]
fn the_debug_bridge_lists_the_indicators() {
    let h = harness(ThemeSpec::new("pop-light"));
    let bridge = egui_pop_core::debug::DebugBridge::with_core_collectors();
    let snap = bridge.snapshot_of(&h.ctx, "indicators").unwrap();
    let ids: Vec<&str> = snap
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    for id in [
        "unread", "reset", "visible", "node", "export", "modified", "accent",
    ] {
        assert!(ids.contains(&id), "{id} in {ids:?}");
    }
    let unread = snap
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "unread")
        .unwrap();
    assert_eq!(unread["kind"], "badge");
    assert_eq!(unread["value"], 3.0);
    assert_eq!(unread["tone"], "danger");
}

#[test]
fn right_to_left_moves_the_end_corner_to_the_left() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    let host = h.get_by_label("Notifications, 3").rect();
    let ltr = indicator::find(&h.ctx, "unread").unwrap().rect;
    assert!(
        ltr[0] + ltr[2] > host.right(),
        "past the right edge: {ltr:?}"
    );
    indicator::set_direction(&h.ctx, indicator::Direction::RightToLeft);
    h.run();
    let rtl = indicator::find(&h.ctx, "unread").unwrap().rect;
    assert!(rtl[0] < host.left(), "past the left edge: {rtl:?}");
}

#[test]
fn a_pass_drawing_no_mark_leaves_none_recorded() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    assert!(indicator::find(&h.ctx, "unread").is_some());
    h.state_mut().hidden = true;
    h.run();
    h.run();
    assert!(
        indicator::drawn(&h.ctx).is_empty(),
        "the marks of an older pass are gone"
    );
    h.state_mut().hidden = false;
    h.run();
    assert!(indicator::find(&h.ctx, "unread").is_some());
}
