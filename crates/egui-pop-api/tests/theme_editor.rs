//! The theme editor: a token edited in its view changes the theme, its
//! contrast is measured, undo takes it back, a theme exported is
//! imported the same, and steps veto changes.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::Before;
use egui_pop_api::theme_editor::{
    self, CMD_THEME_EXPORT, CMD_THEME_IMPORT, CMD_THEME_READ, CMD_THEME_SET, CMD_THEME_START,
    CMD_THEME_UNDO, Cause, STEP_THEME_TOKEN, THEME_EDITOR, ThemeEditorPlugin,
};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::theme::{Tokens, color32};
use egui_pop_core::tokens::{Color, ThemeSpec};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, PluginId};
use serde_json::{Value as Json, json};

/// Draws the editor in the main zone, and vetoes any change of `danger`.
struct Host;

impl Plugin for Host {
    fn id(&self) -> PluginId {
        PluginId::from_static("host")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("host.editor"), 0, |ui, app| {
                egui::ScrollArea::vertical().show(ui, |ui| theme_editor::show(ui, app));
            }),
        );
        ctx.before(STEP_THEME_TOKEN, |_, args| {
            if args["path"] == "danger" {
                Before::Veto("danger stays red".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn app(spec: ThemeSpec) -> App {
    App::builder()
        .theme(spec)
        .plugin(ThemeEditorPlugin)
        .plugin(Host)
        .build(&egui::Context::default())
        .unwrap()
}

fn harness(spec: ThemeSpec) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(1000.0, 900.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app(spec))
}

fn run(h: &mut Harness<'_, App>, command: &egui_pop_model::ids::CommandId, args: Json) -> Json {
    h.state_mut()
        .run(command, args, Origin::Agent("test".into()))
        .now()
        .expect("answered now")
        .expect("ran")
}

#[test]
fn a_token_typed_in_changes_the_theme_shows_its_contrast_and_undoes() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    h.run();
    h.get_by_label("text.default").click();
    h.run();
    // Its pairs are listed, all passing in pop-dark.
    assert!(h.query_by_label("text.default on surface.panel").is_some());
    assert!(h.query_by_label("fail").is_none());

    // Text the colour of the panels: unreadable, and said so.
    let panel = Tokens::of(&h.ctx)
        .resolved()
        .color("surface.panel")
        .unwrap();
    h.get_by_label("Value of text.default").click();
    h.run();
    // Replace the whole text: select all, then type.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_label("Value of text.default")
        .type_text(&panel.to_hex());
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    let spec = h.state().handle().theme().spec();
    assert_eq!(
        spec.overrides.get("text.default"),
        Some(&json!(panel.to_hex()))
    );
    // Installed: what widgets draw with.
    assert_eq!(
        Tokens::of(&h.ctx).color("text.default"),
        Some(color32(panel))
    );
    let read = run(&mut h, &CMD_THEME_READ, json!({ "path": "text.default" }));
    let pair = read["contrast"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["background"] == "surface.panel")
        .unwrap()
        .clone();
    assert_eq!(pair["ratio"], json!(1.0));
    assert_eq!(pair["aa"], false);
    assert_eq!(pair["aaa"], false);
    assert!(h.query_all_by_label("fail").count() > 0);

    // Undo, from the editor's button.
    h.get_by_label("Undo").click();
    h.run();
    h.run();
    assert!(h.state().handle().theme().spec().overrides.is_empty());
    let read = run(&mut h, &CMD_THEME_READ, json!({}));
    assert_eq!(read["can_undo"], false);
    assert_eq!(read["can_redo"], true);
    assert!(read["failing"].as_array().unwrap().is_empty());
}

#[test]
fn a_high_contrast_theme_needs_seven_to_one_for_text() {
    let app = app(ThemeSpec::new("pop-high-contrast"));
    let handle = app.handle();
    let editor = handle.service(&THEME_EDITOR).unwrap();
    let tokens = editor.resolved(handle);
    let readouts = theme_editor::contrast_of(&tokens, "text.default");
    assert!(!readouts.is_empty());
    assert!(readouts.iter().all(|r| r.passes()
        && (r.pair.kind != egui_pop_core::tokens::contrast::PairKind::Text || r.required == 7.0)));
    // A grey that passes AA but not AAA on black.
    let grey = Color::from_hex("#808080").unwrap();
    editor
        .set(
            handle,
            "text.default",
            Some(json!(grey.to_hex())),
            Cause::Agent,
        )
        .unwrap();
    let tokens = editor.resolved(handle);
    let on_canvas = theme_editor::contrast_of(&tokens, "text.default")
        .into_iter()
        .find(|r| r.pair.background == "surface.canvas")
        .unwrap();
    assert!(on_canvas.passes_aa());
    assert_eq!(on_canvas.passes_aaa(), Some(false));
    assert!(!on_canvas.passes());
}

#[test]
fn a_theme_exported_imports_the_same_and_steps_can_veto() {
    let mut h = harness(ThemeSpec::new("pop-light"));
    h.run();
    for (path, value) in [
        ("accent.default", json!("#2060c0")),
        ("radius.widget", json!({ "value": 2, "unit": "px" })),
        ("button.border", json!("{border.strong}")),
    ] {
        let out = run(
            &mut h,
            &CMD_THEME_SET,
            json!({ "path": path, "value": value }),
        );
        assert_eq!(out["changed"], true, "{path}");
    }
    // A plugin vetoes changes of `danger`.
    let vetoed = h.state_mut().run(
        &CMD_THEME_SET,
        json!({ "path": "danger", "value": "#00ff00" }),
        Origin::Agent("test".into()),
    );
    assert!(vetoed.now().unwrap().is_err());
    // A value of another type is refused before any step.
    let wrong = h.state_mut().run(
        &CMD_THEME_SET,
        json!({ "path": "radius.widget", "value": "#000000" }),
        Origin::Agent("test".into()),
    );
    assert!(wrong.now().unwrap().is_err());
    let before = h.state().handle().theme().spec().overrides;
    assert_eq!(before.len(), 3);

    let exported = run(&mut h, &CMD_THEME_EXPORT, json!({ "name": "mine" }));
    let text = exported["text"].as_str().unwrap().to_string();
    assert!(text.contains("\"$extends\": \"pop-light\""));

    run(&mut h, &CMD_THEME_START, json!({ "base": "pop-dark" }));
    h.run();
    let spec = h.state().handle().theme().spec();
    assert_eq!(spec.base, "pop-dark");
    assert!(spec.overrides.is_empty());

    let out = run(&mut h, &CMD_THEME_IMPORT, json!({ "text": text }));
    assert_eq!(out["skipped"], json!([]));
    h.run();
    let spec = h.state().handle().theme().spec();
    assert_eq!(spec.base, "pop-light");
    assert_eq!(spec.overrides, before);
    assert_eq!(
        Tokens::of(&h.ctx).color("accent.default"),
        Color::from_hex("#2060c0").map(color32)
    );
    // Importing starts a new history.
    assert_eq!(run(&mut h, &CMD_THEME_UNDO, json!({})), Json::Null);
}

#[test]
fn the_theme_switched_elsewhere_restarts_the_editor() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    h.run();
    run(
        &mut h,
        &CMD_THEME_SET,
        json!({ "path": "surface.panel", "value": "#101010" }),
    );
    h.state()
        .handle()
        .theme()
        .set(ThemeSpec::new("gruvbox-dark"));
    h.run();
    let read = run(&mut h, &CMD_THEME_READ, json!({}));
    assert_eq!(read["base"], "gruvbox-dark");
    assert_eq!(read["overrides"], json!({}));
    assert_eq!(read["can_undo"], false);
}

#[test]
fn ctrl_z_in_the_editor_undoes_its_own_change_not_a_store_step() {
    use egui_pop_model::values::{Change, Descriptor, Set, Value};
    let mut h = harness(ThemeSpec::new("pop-dark"));
    let handle = h.state().handle().clone();
    // An unrelated value of the application, changed by the user: a step
    // of the store, what Edit > Undo takes back.
    let store = handle.store();
    egui_pop_core::lock(&store)
        .declare(
            &egui_pop_model::ids::OwnerId::USER,
            "settings/level",
            Descriptor::float("Level", 0.0, 1.0, 0.0),
        )
        .unwrap();
    egui_pop_core::lock(&store)
        .set(Set::new(
            "settings/level",
            Change::To(Value::Float(0.5)),
            Origin::User,
        ))
        .unwrap();
    h.run();
    let editor = handle.service(&THEME_EDITOR).unwrap();
    editor
        .set(
            &handle,
            "accent.default",
            Some(json!("#2060c0")),
            Cause::Edit,
        )
        .unwrap();
    h.run();
    // The focus in the editor: a token picked in its list.
    h.get_by_label("text.default").click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    h.run();
    assert!(
        handle.theme().spec().overrides.is_empty(),
        "the editor's change is undone"
    );
    let s = egui_pop_core::lock(&store);
    assert_eq!(
        s.get("settings/level").unwrap().value,
        Value::Float(0.5),
        "the store's step is left alone"
    );
    assert_eq!(s.undo_steps().len(), 1);
    drop(s);
    // And Ctrl+Shift+Z makes it again.
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::Z,
    );
    h.run();
    h.run();
    assert_eq!(handle.theme().spec().overrides.len(), 1);
}
