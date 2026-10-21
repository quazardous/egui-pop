//! The user's text settings: the text size multiplies the theme's size
//! scale (and survives a change of theme), chosen in Settings ›
//! Appearance.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::text::{self, SETTING_TEXT_SIZE};
use egui_pop_api::{App, DefaultPlugins, settings_window};
use egui_pop_core::theme::Tokens;
use egui_pop_core::tokens::ThemeSpec;
use egui_pop_model::values::Value;

fn harness(spec: ThemeSpec) -> Harness<'static, App> {
    let app = App::builder()
        .theme(spec)
        .plugins(DefaultPlugins)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

#[test]
fn the_text_size_scales_the_text_and_survives_a_change_of_theme() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    let before = Tokens::of(&h.ctx);
    assert_eq!(before.font.body, 13.0);
    let app = h.state().handle().clone();
    text::set_text_size(&app, 1.3);
    h.run();
    assert_eq!(text::text_size(&app), 1.3);
    let t = Tokens::of(&h.ctx);
    assert_eq!(t.font.body, 17.0);
    assert_eq!(t.font.scale.m, 17.0);
    assert_eq!(
        h.ctx.global_style().text_styles[&egui::TextStyle::Body].size,
        17.0
    );
    // Spacing does not move.
    assert_eq!(t.space.m, before.space.m);
    // Another theme keeps the user's text size.
    app.theme().pick(&h.ctx, "pop-high-contrast");
    h.run();
    let t = Tokens::of(&h.ctx);
    assert_eq!(t.resolved().spec().base, "pop-high-contrast");
    assert_eq!(t.resolved().spec().text_size, 1.3);
    // Back to 100 %.
    text::set_text_size(&app, 1.0);
    h.run();
    assert_eq!(Tokens::of(&h.ctx).resolved().spec().text_size, 1.0);
}

#[test]
fn an_applications_own_text_size_stays_until_the_setting_moves() {
    let h = harness(ThemeSpec::new("pop-dark").text_size(1.15));
    assert_eq!(Tokens::of(&h.ctx).resolved().spec().text_size, 1.15);
}

#[test]
fn the_text_size_is_chosen_in_settings_appearance() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Comma);
    h.run();
    assert!(settings_window::is_open(h.state().handle()));
    h.get_by_label("Text size").click();
    h.run();
    h.get_by_label("150%").click();
    h.run();
    h.run();
    let path = format!("settings/{SETTING_TEXT_SIZE}");
    let value = egui_pop_core::lock(&h.state().handle().store())
        .get(&path)
        .unwrap()
        .value
        .clone();
    assert_eq!(value, Value::Choice("150%".into()));
    assert_eq!(Tokens::of(&h.ctx).font.body, 19.5);
    h.get_by_label("Reset Text size").click();
    h.run();
    h.run();
    assert_eq!(Tokens::of(&h.ctx).font.body, 13.0);
}
