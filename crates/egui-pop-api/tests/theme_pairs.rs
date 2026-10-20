//! A theme with a counterpart in an application: the light or dark
//! preference (a setting, through the theme service) switches tokens,
//! egui's style and the kit without installing again, follows the
//! system when asked, and a theme alone keeps its mode; the toggle
//! command, and the theme editor editing the variant shown.

use egui_kittest::Harness;
use egui_pop_api::host::Reply;
use egui_pop_api::theme_editor::{Cause, THEME_EDITOR, ThemeEditorPlugin};
use egui_pop_api::{App, CMD_THEME_TOGGLE, DefaultPlugins};
use egui_pop_core::kit::{Kit, State};
use egui_pop_core::theme::{self, Tokens};
use egui_pop_core::tokens::ThemeSpec;
use egui_pop_model::commands::Origin;
use serde_json::json;

fn harness(spec: ThemeSpec) -> Harness<'static, App> {
    let app = App::builder()
        .theme(spec)
        .plugins(DefaultPlugins)
        .plugin(ThemeEditorPlugin)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn base(h: &Harness<'_, App>) -> String {
    Tokens::of(&h.ctx).resolved().spec().base.clone()
}

fn toggle(h: &mut Harness<'_, App>) -> Result<serde_json::Value, String> {
    let out = h
        .state_mut()
        .run(&CMD_THEME_TOGGLE, json!({}), Origin::User);
    h.run();
    match out {
        Reply::Done(r) => r,
        Reply::Later(_) => panic!("the toggle answers at once"),
    }
}

#[test]
fn the_preference_switches_the_pair_without_installing_again() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    assert!(theme::is_paired(&h.ctx));
    assert_eq!(base(&h), "pop-dark");
    let dark_kit = Kit::of(&h.ctx);
    let service = h.state().handle().theme();
    assert_eq!(service.preference(), egui::ThemePreference::System);
    assert_eq!(service.counterpart().as_deref(), Some("pop-light"));

    service.set_preference(egui::ThemePreference::Light);
    h.run();
    assert_eq!(base(&h), "pop-light");
    assert_eq!(service.shown(&h.ctx), "pop-light");
    let light = Tokens::of(&h.ctx);
    assert_eq!(h.ctx.global_style().visuals.panel_fill, light.surface.panel);
    let light_kit = Kit::of(&h.ctx);
    assert_ne!(
        light_kit.button.style(State::REST).fill,
        dark_kit.button.style(State::REST).fill
    );
    // The spec is the same: nothing was installed again.
    assert_eq!(service.spec().base, "pop-dark");
}

#[test]
fn the_system_preference_follows_the_system() {
    let mut h = harness(ThemeSpec::new("solarized-light"));
    // The system says nothing: the theme asked for.
    assert_eq!(base(&h), "solarized-light");
    h.input_mut().system_theme = Some(egui::Theme::Dark);
    h.run();
    assert_eq!(base(&h), "solarized-dark");
    assert_eq!(h.state().handle().theme().shown(&h.ctx), "solarized-dark");
    h.input_mut().system_theme = Some(egui::Theme::Light);
    h.run();
    assert_eq!(base(&h), "solarized-light");
}

#[test]
fn a_theme_alone_keeps_its_mode() {
    let mut h = harness(ThemeSpec::new("pop-high-contrast"));
    assert!(!theme::is_paired(&h.ctx));
    let service = h.state().handle().theme();
    service.set_preference(egui::ThemePreference::Light);
    h.input_mut().system_theme = Some(egui::Theme::Light);
    h.run();
    assert_eq!(h.ctx.theme(), egui::Theme::Dark);
    assert_eq!(base(&h), "pop-high-contrast");
    assert!(toggle(&mut h).is_err());
    assert_eq!(base(&h), "pop-high-contrast");
}

#[test]
fn the_toggle_command_shows_the_other_variant() {
    let mut h = harness(ThemeSpec::new("gruvbox-light"));
    assert_eq!(toggle(&mut h).unwrap()["theme"], "gruvbox-dark");
    assert_eq!(base(&h), "gruvbox-dark");
    assert_eq!(
        h.state().handle().theme().preference(),
        egui::ThemePreference::Dark
    );
    assert_eq!(toggle(&mut h).unwrap()["dark"], false);
    assert_eq!(base(&h), "gruvbox-light");
}

#[test]
fn picking_a_theme_shows_it() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    let service = h.state().handle().theme();
    service.set_preference(egui::ThemePreference::Dark);
    service.pick(&h.ctx, "one-light");
    h.run();
    assert_eq!(base(&h), "one-light");
    assert_eq!(service.preference(), egui::ThemePreference::Light);
    // A theme of the mode asked keeps the preference.
    service.pick(&h.ctx, "github-light");
    h.run();
    assert_eq!(base(&h), "github-light");
    assert_eq!(service.preference(), egui::ThemePreference::Light);
}

#[test]
fn the_theme_editor_edits_the_variant_shown() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    let handle = h.state().handle().clone();
    handle.theme().set_preference(egui::ThemePreference::Light);
    h.run();
    let editor = handle.service(&THEME_EDITOR).unwrap();
    editor.sync(&handle);
    assert_eq!(editor.draft().base(), "pop-light");
    editor
        .set(&handle, "danger", Some(json!("#ff0000")), Cause::Agent)
        .unwrap();
    h.run();
    // The variant edited is shown alone, with the edit.
    let spec = handle.theme().spec();
    assert_eq!(spec.base, "pop-light");
    assert_eq!(base(&h), "pop-light");
    assert!(!theme::is_paired(&h.ctx));
    assert_eq!(
        Tokens::of(&h.ctx).status.danger,
        egui::Color32::from_rgb(255, 0, 0)
    );
}
