//! The user's text settings: the text size multiplies the theme's size
//! scale, a family per role comes before the theme's (and both survive a
//! change of theme), chosen on the settings window's Fonts page.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::settings_window::FONTS;
use egui_pop_api::text::{self, SETTING_FONT_MONO, SETTING_TEXT_SIZE};
use egui_pop_api::{App, DefaultPlugins, settings_window};
use egui_pop_core::fonts::{self, Role};
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

fn open_fonts(h: &mut Harness<'_, App>) {
    h.key_press_modifiers(Modifiers::COMMAND, Key::Comma);
    h.run();
    assert!(settings_window::is_open(h.state().handle()));
    h.get_by_label(FONTS).click();
    h.run();
}

fn stored(h: &Harness<'_, App>, id: &egui_pop_model::ids::SettingId) -> Value {
    egui_pop_core::lock(&h.state().handle().store())
        .get(&format!("settings/{id}"))
        .unwrap()
        .value
        .clone()
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
fn the_text_size_is_chosen_on_the_fonts_page() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    open_fonts(&mut h);
    h.get_by_label("Text size").click();
    h.run();
    h.get_by_label("150%").click();
    h.run();
    h.run();
    assert_eq!(stored(&h, &SETTING_TEXT_SIZE), Value::Choice("150%".into()));
    assert_eq!(Tokens::of(&h.ctx).font.body, 19.5);
    h.get_by_label("Reset Text size").click();
    h.run();
    h.run();
    assert_eq!(Tokens::of(&h.ctx).font.body, 13.0);
}

#[test]
fn a_family_per_role_comes_before_the_themes_and_survives_a_change_of_theme() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    let app = h.state().handle().clone();
    assert_eq!(text::font(&app, Role::Ui), None);
    text::set_font(&app, Role::Ui, Some("Hack"));
    text::set_font(&app, Role::Mono, Some("Ubuntu"));
    h.run();
    let t = Tokens::of(&h.ctx);
    assert_eq!(t.font.family_ui[0], "Hack");
    assert_eq!(t.font.family_mono[0], "Ubuntu");
    // Installed: egui's families follow.
    let (ui, mono) = fonts::installed_families(&h.ctx).unwrap();
    assert_eq!((ui[0].as_str(), mono[0].as_str()), ("Hack", "Ubuntu"));
    app.theme().pick(&h.ctx, "pop-light");
    h.run();
    assert_eq!(Tokens::of(&h.ctx).font.family_ui[0], "Hack");
    // Back to the theme's.
    text::set_font(&app, Role::Ui, None);
    text::set_font(&app, Role::Mono, Some("  "));
    h.run();
    let t = Tokens::of(&h.ctx);
    assert_eq!(t.font.family_ui[0], "sans-serif");
    assert_eq!(t.font.family_mono[0], "monospace");
    assert_eq!(text::font(&app, Role::Mono), None);
}

#[test]
fn the_fonts_page_picks_a_family_among_those_there_are_and_previews_it() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    open_fonts(&mut h);
    for label in ["Text size", "Interface font", "Monospace font", "Preview"] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    // Numbers in the preview, in the numeric font.
    assert!(h.query_by_label("-1234.56").is_some());
    h.get_by_label("Monospace font").click();
    h.run();
    // Only monospace families are offered for numbers and code.
    assert!(h.query_by_label("Ubuntu").is_none());
    h.get_by_label("Hack").click();
    h.run();
    h.run();
    assert_eq!(stored(&h, &SETTING_FONT_MONO), Value::Text("Hack".into()));
    assert_eq!(Tokens::of(&h.ctx).font.family_mono[0], "Hack");
    h.get_by_label("Reset Monospace font").click();
    h.run();
    assert_eq!(stored(&h, &SETTING_FONT_MONO), Value::Text(String::new()));
}

#[test]
fn resting_on_a_family_in_the_picker_previews_it_before_it_is_picked() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    // A system face of the test's own: nothing depends on the machine.
    let ubuntu = egui::FontDefinitions::default().font_data["Ubuntu-Light"]
        .font
        .to_vec();
    fonts::set_system_faces(
        &h.ctx,
        Some(vec![fonts::Face::from_owned(
            "Fake Sans",
            fonts::Weight::REGULAR,
            ubuntu,
        )]),
    );
    open_fonts(&mut h);
    h.get_by_label("Interface font").click();
    h.run();
    h.get_by_label("Fake Sans").hover();
    h.run();
    h.run_steps(2);
    assert_eq!(
        fonts::preview(&h.ctx, Role::Ui),
        Some(vec!["Fake Sans".to_string()])
    );
    let report = fonts::report(&h.ctx).unwrap();
    let chain = |family: &str| {
        report
            .chains
            .iter()
            .find(|(f, _)| f == family)
            .map(|(_, c)| c.clone())
            .unwrap_or_default()
    };
    assert_eq!(chain("pop.preview.ui.400")[0], "pop:Fake Sans:400:system");
    // Not picked yet: the theme's fonts are untouched.
    assert_eq!(Tokens::of(&h.ctx).font.family_ui[0], "sans-serif");
    // The bundled interface face: egui-pop's Inter, else egui's own.
    let bundled = if cfg!(feature = "bundled-fonts") {
        "pop:Inter:400:bundled"
    } else {
        "Ubuntu-Light"
    };
    assert_eq!(chain("pop.ui.400")[0], bundled);
    // Picked: applied, and the preview ends.
    h.get_by_label("Fake Sans").click();
    h.run();
    h.run();
    assert_eq!(Tokens::of(&h.ctx).font.family_ui[0], "Fake Sans");
    assert_eq!(fonts::preview(&h.ctx, Role::Ui), None);
}
