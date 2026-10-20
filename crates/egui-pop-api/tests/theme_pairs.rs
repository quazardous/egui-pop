//! A theme with a counterpart in an application: the light or dark
//! preference (a setting, through the theme service) switches tokens,
//! egui's style and the kit without installing again, follows the
//! system when asked, and a theme alone keeps its mode; the toggle
//! command, and the theme editor editing the variant shown.

use egui_kittest::Harness;
use egui_pop_api::host::Reply;
use egui_pop_api::theme_editor::{Cause, THEME_EDITOR, ThemeEditorPlugin};
use egui_pop_api::{App, CMD_THEME_TOGGLE, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::kit::{Kit, State};
use egui_pop_core::theme::{self, Tokens};
use egui_pop_core::tokens::ThemeSpec;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};
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

/// Two panels sharing the bottom area: drawn in an egui_dock dock.
struct Docked;

impl Plugin for Docked {
    fn id(&self) -> PluginId {
        PluginId::from_static("docked")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for (id, title) in [
            (PanelId::from_static("docked.log"), "Log"),
            (PanelId::from_static("docked.notes"), "Notes"),
        ] {
            ctx.panel(
                PanelSpec::new(id, title, Area::Bottom).min_size(80.0),
                |ui, _| {
                    ui.label("body");
                },
            )?;
        }
        Ok(())
    }
}

/// Every rectangle fill painted in the last pass.
fn fills(h: &Harness<'_, App>) -> Vec<egui::Color32> {
    fn walk(shape: &egui::Shape, out: &mut Vec<egui::Color32>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            egui::Shape::Rect(r) => out.push(r.fill),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in &h.output().shapes {
        walk(&c.shape, &mut out);
    }
    out
}

#[test]
fn docks_follow_every_theme_switch() {
    let app = App::builder()
        .theme(ThemeSpec::new("pop-dark"))
        .plugins(DefaultPlugins)
        .plugin(Docked)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    let service = h.state().handle().theme();
    let picks = [
        "pop-light",
        "solarized-dark",
        "pop-high-contrast",
        "gruvbox-light",
        "one-dark-pro",
        "github-light",
        "pop-high-contrast",
        "catppuccin-mocha",
        "pop-dark",
        "solarized-light",
    ];
    let preferences = [
        egui::ThemePreference::System,
        egui::ThemePreference::Light,
        egui::ThemePreference::Dark,
    ];
    let systems = [Some(egui::Theme::Dark), Some(egui::Theme::Light)];
    let mut step = 0;
    for round in 0..3 {
        for name in picks {
            for preference in preferences {
                h.input_mut().system_theme = systems[step % 2];
                step += 1;
                service.pick(&h.ctx, name);
                h.run();
                service.set_preference(preference);
                h.run();
                // The dock's tab bodies are filled with egui's window
                // fill: the overlay surface of the variant drawn now.
                let shown = Tokens::of(&h.ctx);
                assert_eq!(
                    h.ctx.global_style().visuals.window_fill,
                    shown.surface.overlay
                );
                let other = Tokens::of_theme(
                    &h.ctx,
                    match h.ctx.theme() {
                        egui::Theme::Dark => egui::Theme::Light,
                        egui::Theme::Light => egui::Theme::Dark,
                    },
                );
                let fills = fills(&h);
                let context = format!(
                    "round {round}, `{name}` with {preference:?}: `{}` drawn",
                    shown.resolved().spec().base
                );
                assert!(
                    fills.contains(&shown.surface.overlay),
                    "{context}: the dock is not filled with its overlay"
                );
                if other.surface.overlay != shown.surface.overlay {
                    assert!(
                        !fills.contains(&other.surface.overlay),
                        "{context}: the other variant's overlay is painted"
                    );
                }
            }
        }
    }
}

#[test]
fn an_accent_colour_setting_generates_both_variants_ramps() {
    use egui_pop_api::SETTING_ACCENT_COLOR;
    use egui_pop_core::tokens::Color;
    let mut h = harness(ThemeSpec::new("pop-dark").accent("blue"));
    let service = h.state().handle().theme();
    let seed = Color::from_hex("#c0392b").unwrap();
    service.set_accent_color(Some(seed));
    h.run();
    assert_eq!(service.accent_color(), Some(seed));
    let stored = egui_pop_core::lock(&h.state().handle().store())
        .get(&format!("settings/{SETTING_ACCENT_COLOR}"))
        .and_then(|e| e.value.as_text().map(str::to_string));
    assert_eq!(stored.as_deref(), Some("#c0392b"));
    // Each variant has its own ramp, from the same seed.
    let dark = Tokens::of_theme(&h.ctx, egui::Theme::Dark);
    let light = Tokens::of_theme(&h.ctx, egui::Theme::Light);
    assert_eq!(
        dark.resolved()
            .get("accent.default")
            .unwrap()
            .alias
            .as_deref(),
        Some("custom.9")
    );
    assert!(!dark.resolved().color("accent.default").unwrap().is_dark());
    assert!(light.resolved().color("accent.default").unwrap().is_dark());
    // Emptied: the theme's own accent again.
    service.set_accent_color(None);
    h.run();
    assert_eq!(service.accent_color(), None);
    assert_eq!(
        Tokens::of(&h.ctx)
            .resolved()
            .get("accent.default")
            .unwrap()
            .alias
            .as_deref(),
        Some("gold.9")
    );
}
