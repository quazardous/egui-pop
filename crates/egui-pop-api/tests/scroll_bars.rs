//! The `appearance.scroll_bars` setting: from the store and the settings
//! window to egui's style, with the system raising `auto` and nothing else.

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::core::theme::{ScrollBars, ThemeSpec};
use egui_pop_api::scroll_bars::{self, SETTING_SCROLL_BARS};
use egui_pop_api::{App, DefaultPlugins};
use egui_pop_model::commands::Origin;
use egui_pop_model::values::{Change, Set, Value};

fn harness(system_always: bool) -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .theme(ThemeSpec::new("pop-dark"))
        .system_always_shows_scroll_bars(system_always)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn set(h: &mut Harness<'_, App>, value: &str) {
    egui_pop_api::core::lock(&h.state().handle().store())
        .set(Set::new(
            &format!("settings/{SETTING_SCROLL_BARS}"),
            Change::To(Value::Choice(value.into())),
            Origin::User,
        ))
        .unwrap();
    h.run();
}

/// The bars egui draws now.
fn style(h: &Harness<'_, App>) -> egui::style::ScrollStyle {
    h.ctx.global_style().spacing.scroll
}

#[test]
fn the_setting_is_auto_under_appearance_and_leaves_the_theme_s_bars() {
    let h = harness(false);
    let setting = h
        .state()
        .handle()
        .settings()
        .into_iter()
        .find(|s| s.id == SETTING_SCROLL_BARS)
        .unwrap();
    assert_eq!(setting.section, "Appearance");
    assert_eq!(setting.descriptor.default, Value::Choice("auto".into()));
    assert_eq!(
        scroll_bars::scroll_bars(h.state().handle()),
        ScrollBars::Auto
    );
    let s = style(&h);
    assert!(s.floating);
    assert_eq!(s.floating_allocated_width, 6.0, "pop-dark's quiet bar");
    assert!(s.dormant_handle_opacity > 0.0);
}

#[test]
fn each_choice_reaches_egui_s_style_and_auto_brings_the_theme_s_back() {
    let mut h = harness(false);
    let quiet = style(&h);
    set(&mut h, "always");
    assert_eq!(
        h.state().handle().theme().spec().scroll_bars,
        ScrollBars::Always
    );
    let s = style(&h);
    assert!(!s.floating, "beside the content");
    assert_eq!(s.floating_allocated_width, 0.0);
    set(&mut h, "hover");
    let s = style(&h);
    assert!(s.floating);
    assert_eq!(s.dormant_handle_opacity, 0.0, "nothing at rest");
    set(&mut h, "auto");
    assert_eq!(style(&h), quiet);
}

#[test]
fn the_choice_stays_when_the_theme_changes() {
    let mut h = harness(false);
    set(&mut h, "always");
    h.state().handle().theme().set(ThemeSpec::new("pop-light"));
    h.run();
    let spec = h.state().handle().theme().spec();
    assert_eq!(spec.base, "pop-light");
    assert_eq!(spec.scroll_bars, ScrollBars::Always);
    assert!(!style(&h).floating);
}

#[test]
fn auto_leaves_an_application_s_own_spec() {
    let app = App::builder()
        .theme(ThemeSpec::new("pop-dark").scroll_bars(ScrollBars::Hover))
        .build(&egui::Context::default())
        .unwrap();
    assert_eq!(app.handle().theme().spec().scroll_bars, ScrollBars::Hover);
}

#[test]
fn the_system_s_always_raises_auto_and_no_choice() {
    let mut h = harness(true);
    assert!(!style(&h).floating, "auto is raised");
    set(&mut h, "hover");
    assert!(style(&h).floating, "the user's choice stands");
    set(&mut h, "auto");
    assert!(!style(&h).floating);
    // Nothing is saved for the system: the setting still says auto.
    assert_eq!(
        scroll_bars::scroll_bars(h.state().handle()),
        ScrollBars::Auto
    );
}

#[test]
fn a_system_that_says_nothing_does_not_lower_the_theme_s_bars() {
    let h = harness(false);
    assert!(style(&h).dormant_handle_opacity > 0.0, "still seen at rest");
}

#[test]
fn the_settings_window_offers_the_choices() {
    let mut h = harness(false);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Comma);
    h.run();
    // The search brings the row into view, as a person would.
    h.get_by_role_and_label(Role::TextInput, "Settings").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Settings")
        .type_text("scroll bars");
    h.run();
    h.get_by_label("Scroll bars").click();
    h.run();
    h.get_by_label("always").click();
    h.run();
    assert_eq!(
        scroll_bars::scroll_bars(h.state().handle()),
        ScrollBars::Always
    );
    assert!(!style(&h).floating);
}
