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
        ("button.rest.border-color", json!("{border.strong}")),
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
    assert_eq!(out["warnings"], json!([]));
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

    // A file for a newer contract, with a token that does not check,
    // loads best-effort: the token left out, both said.
    let newer = r##"{
      "$extends": "pop-dark",
      "$extensions": { "org.egui-pop.format": { "version": 1, "contract": 99 } },
      "radius": { "widget": { "$type": "color", "$value": "#000000" } },
      "accent": { "default": { "$type": "color", "$value": "#2060c0" } }
    }"##;
    let out = run(&mut h, &CMD_THEME_IMPORT, json!({ "text": newer }));
    assert_eq!(out["skipped"], json!(["radius.widget"]));
    let warnings = out["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("read best-effort")),
        "{warnings:?}"
    );
    h.run();
    let spec = h.state().handle().theme().spec();
    assert_eq!(spec.base, "pop-dark");
    assert_eq!(spec.overrides.len(), 1);
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
fn a_scale_generated_from_a_colour_is_written_and_undone_as_one_change() {
    use egui_pop_api::theme_editor::CMD_THEME_GENERATE;
    let mut h = harness(ThemeSpec::new("pop-light"));
    h.run();
    let out = run(
        &mut h,
        &CMD_THEME_GENERATE,
        json!({ "scale": "gold", "seed": "#8e44ad" }),
    );
    assert_eq!(out["changed"], 13);
    h.run();
    let tokens = Tokens::of(&h.ctx);
    let solid = tokens.resolved().color("gold.9").unwrap();
    assert!(solid.is_dark(), "{solid}");
    // The light theme's contrast pairs still pass.
    assert!(
        run(&mut h, &CMD_THEME_READ, json!({}))["failing"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // One undo takes the whole scale back.
    run(&mut h, &CMD_THEME_UNDO, json!({}));
    h.run();
    assert!(h.state().handle().theme().spec().overrides.is_empty());
    // A scale the theme does not have is refused.
    let refused = h.state_mut().run(
        &CMD_THEME_GENERATE,
        json!({ "scale": "nope", "seed": "#8e44ad" }),
        Origin::Agent("test".into()),
    );
    assert!(refused.now().unwrap().is_err());
}

#[test]
fn a_generated_scale_is_exported_as_its_seed_and_imported_back() {
    use egui_pop_api::theme_editor::CMD_THEME_GENERATE;
    let mut h = harness(ThemeSpec::new("pop-light"));
    h.run();
    run(
        &mut h,
        &CMD_THEME_GENERATE,
        json!({ "scale": "gold", "seed": "#8e44ad" }),
    );
    h.run();
    let generated = h.state().handle().theme().spec().overrides.clone();
    assert_eq!(generated.len(), 13);
    let out = run(&mut h, &CMD_THEME_EXPORT, json!({ "name": "mine" }));
    let text = out["text"].as_str().expect("the file's text").to_string();
    let file: Json = serde_json::from_str(&text).unwrap();
    // The seed and its scheme, not the twelve values.
    assert_eq!(
        file["$extensions"]["org.egui-pop.seeds"]["gold"],
        json!({ "seed": "#8e44ad", "scheme": "light" })
    );
    assert!(file.get("gold").is_none(), "{text}");
    // Imported, the scale is regenerated with the same values.
    run(&mut h, &CMD_THEME_START, json!({ "base": "pop-light" }));
    h.run();
    assert!(h.state().handle().theme().spec().overrides.is_empty());
    run(&mut h, &CMD_THEME_IMPORT, json!({ "text": text }));
    h.run();
    assert_eq!(h.state().handle().theme().spec().overrides, generated);
    // Exported again, still as its seed.
    let again = run(&mut h, &CMD_THEME_EXPORT, json!({ "name": "mine" }));
    assert_eq!(again["text"].as_str(), Some(text.as_str()));
}

#[test]
fn the_generator_shows_for_a_palette_token() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    h.run();
    let editor = h.state().handle().service(&THEME_EDITOR).unwrap();
    editor.select("blue.3");
    h.run();
    assert!(h.query_by_label("Seed colour of blue").is_some());
    assert!(h.query_by_label("Hex of the seed colour of blue").is_some());
    h.get_by_label("Generate blue").click();
    h.run();
    assert!(
        h.query_by_label_contains("tokens of blue generated")
            .is_some()
    );
}

#[test]
fn in_preview_only_the_draft_is_drawn_in_its_pane_until_applied() {
    use std::sync::{Arc, Mutex};
    let mut h = harness(ThemeSpec::new("pop-dark"));
    h.run();
    let handle = h.state().handle().clone();
    let editor = handle.service(&THEME_EDITOR).unwrap();
    // The pane's content records the accent it is drawn with.
    let seen: Arc<Mutex<Option<egui::Color32>>> = Arc::default();
    let into = seen.clone();
    editor.set_sampler(Some(Arc::new(move |ui: &mut egui::Ui, _: &_| {
        *into.lock().unwrap() = Some(Tokens::of(ui.ctx()).accent.default);
        ui.label("sampled");
    })));
    h.get_by_label("Preview only").click();
    h.run();
    assert!(editor.preview_only());
    let installed = Tokens::of(&h.ctx).accent.default;
    run(
        &mut h,
        &CMD_THEME_SET,
        json!({ "path": "accent.default", "value": "#2060c0" }),
    );
    h.run();
    h.run();
    let draft = color32(Color::from_hex("#2060c0").unwrap());
    // The pane draws the draft; the window keeps the installed theme.
    assert_eq!(*seen.lock().unwrap(), Some(draft));
    assert_eq!(Tokens::of(&h.ctx).accent.default, installed);
    assert!(handle.theme().spec().overrides.is_empty());
    assert!(h.query_by_label("sampled").is_some());
    // Applied: installed for the whole window.
    h.get_by_label("Apply").click();
    h.run();
    h.run();
    assert_eq!(Tokens::of(&h.ctx).accent.default, draft);
    assert!(!editor.pending(&handle));
    // Discarded: back to what is installed.
    run(
        &mut h,
        &CMD_THEME_SET,
        json!({ "path": "accent.default", "value": "#a02020" }),
    );
    h.run();
    assert!(editor.pending(&handle));
    h.get_by_label("Discard").click();
    h.run();
    assert!(!editor.pending(&handle));
    assert_eq!(Tokens::of(&h.ctx).accent.default, draft);
}

#[test]
fn a_hex_field_beside_the_picker_sets_a_colour() {
    let mut h = harness(ThemeSpec::new("pop-dark"));
    h.run();
    let handle = h.state().handle().clone();
    handle
        .service(&THEME_EDITOR)
        .unwrap()
        .select("accent.default");
    h.run();
    h.get_by_label("Hex of accent.default").click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_label("Hex of accent.default").type_text("#123456");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    assert_eq!(
        handle.theme().spec().overrides.get("accent.default"),
        Some(&json!("#123456"))
    );
}

#[test]
fn files_are_opened_and_saved_through_the_host_s_dialog() {
    use egui_pop_api::host;
    use std::sync::{Arc, Mutex};
    let dir = std::env::temp_dir().join(format!("egui-pop-theme-editor-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mine.tokens.json");
    let mut h = harness(ThemeSpec::new("pop-dark"));
    h.run();
    let handle = h.state().handle().clone();
    let editor = handle.service(&THEME_EDITOR).unwrap();
    let asked: Arc<Mutex<Vec<String>>> = Arc::default();
    let (log, answer) = (asked.clone(), path.clone());
    editor.set_file_dialog(Some(Arc::new(move |_: &_, request| {
        log.lock()
            .unwrap()
            .push(request.to_json()["kind"].to_string());
        let (pending, resolver) = host::pending();
        resolver.resolve(Ok(json!({ "paths": [answer.display().to_string()] })));
        pending
    })));
    run(
        &mut h,
        &CMD_THEME_SET,
        json!({ "path": "accent.default", "value": "#2060c0" }),
    );
    h.run();
    h.get_by_label("Export and import").scroll_to_me();
    h.run();
    h.get_by_label("Export and import").click();
    h.run();
    h.get_by_label("Save as a file").scroll_to_me();
    h.run();
    h.get_by_label("Save as a file").click();
    h.run();
    h.run();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("#2060c0"), "{text}");
    // Back to the base, then the file opened again.
    run(&mut h, &CMD_THEME_START, json!({ "base": "pop-dark" }));
    h.run();
    assert!(handle.theme().spec().overrides.is_empty());
    h.get_by_label("Open a file").scroll_to_me();
    h.run();
    h.get_by_label("Open a file").click();
    h.run();
    h.run();
    assert_eq!(
        handle.theme().spec().overrides.get("accent.default"),
        Some(&json!("#2060c0"))
    );
    assert_eq!(asked.lock().unwrap().len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_fonts_tab_adds_a_file_reorders_families_and_applies_once() {
    use egui_pop_api::host;
    use egui_pop_api::theme_editor::Tab;
    use egui_pop_core::fonts::Role;
    use std::sync::Arc;
    let font = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../egui-pop-core/tests/fonts/InterVariable-subset.ttf");
    let mut h = harness(ThemeSpec::new("pop-dark"));
    h.run();
    let handle = h.state().handle().clone();
    let editor = handle.service(&THEME_EDITOR).unwrap();
    let answer = font.clone();
    editor.set_file_dialog(Some(Arc::new(move |_: &_, _| {
        let (pending, resolver) = host::pending();
        resolver.resolve(Ok(json!({ "paths": [answer.display().to_string()] })));
        pending
    })));
    h.get_by_label("Fonts").click();
    h.run();
    assert_eq!(editor.tab(), Tab::Fonts);
    h.get_by_label("Add a font file").click();
    h.run();
    h.run();
    let files = editor.edited_font_files();
    assert_eq!(files.len(), 1, "{files:?}");
    let family = files[0].family.clone();
    // Nothing applied yet: kept as edits.
    assert!(editor.font_files().is_empty());
    assert!(editor.fonts_pending(&handle));
    // The new family first: added at the end, then moved up to the top.
    let before = editor.families(&handle, Role::Ui);
    let mut list = before.clone();
    list.push(family.clone());
    editor.set_families(Role::Ui, list);
    h.run();
    for _ in 0..before.len() {
        h.get_by_label(&format!("Move {family} up")).click();
        h.run();
    }
    assert_eq!(editor.families(&handle, Role::Ui)[0], family);
    h.get_by_label("Apply the fonts").click();
    h.run();
    h.run();
    assert!(!editor.fonts_pending(&handle));
    assert_eq!(editor.font_files().len(), 1);
    assert_eq!(
        handle
            .theme()
            .spec()
            .overrides
            .get("font.family.ui")
            .and_then(|v| v[0].as_str()),
        Some(family.as_str())
    );
    let report = egui_pop_core::fonts::report(&h.ctx).unwrap();
    let primary = report
        .primary
        .iter()
        .find(|(f, _)| f == "pop.ui.400")
        .and_then(|(_, p)| p.clone())
        .unwrap_or_default();
    assert!(primary.contains("registered"), "{primary}");
    // One undo takes the families back.
    run(&mut h, &CMD_THEME_UNDO, json!({}));
    h.run();
    assert!(
        !handle
            .theme()
            .spec()
            .overrides
            .contains_key("font.family.ui")
    );
}

#[test]
fn a_theme_naming_a_font_file_is_saved_and_opened_with_it() {
    use egui_pop_core::fonts::FontFile;
    let font = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../egui-pop-core/tests/fonts/InterVariable-subset.ttf");
    let dir = std::env::temp_dir().join(format!("egui-pop-theme-package-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = harness(ThemeSpec::new("pop-dark"));
    h.run();
    let handle = h.state().handle().clone();
    let editor = handle.service(&THEME_EDITOR).unwrap();
    let file = FontFile::read(&font).unwrap();
    let family = file.family.clone();
    editor.set_font_files(&h.ctx, vec![file]);
    run(
        &mut h,
        &CMD_THEME_SET,
        json!({ "path": "font.family.ui", "value": [family, "sans-serif"] }),
    );
    h.run();
    // Saved: a folder named after the theme, the face beside the file.
    let written = editor
        .save_file(&handle, &dir.join("mine.tokens.json"))
        .unwrap();
    let package = dir.join("mine");
    assert_eq!(written, package.join("mine.tokens.json"));
    let face = package.join("fonts/InterVariable-subset.ttf");
    assert!(face.is_file());
    let text = std::fs::read_to_string(&written).unwrap();
    assert!(
        text.contains("org.egui-pop.fonts") && text.contains("fonts/InterVariable-subset.ttf"),
        "{text}"
    );
    // Opened again from a fresh start: the theme and its face.
    run(&mut h, &CMD_THEME_START, json!({ "base": "pop-dark" }));
    editor.set_font_files(&h.ctx, Vec::new());
    h.run();
    let warnings = editor.open_file(&handle, &package).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    h.run();
    assert_eq!(
        handle.theme().spec().overrides["font.family.ui"][0],
        json!(family)
    );
    let files = editor.font_files();
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].path, face);
    // With a font folder, the face is copied there and kept from there.
    let kept = dir.join("kept-fonts");
    editor.set_font_dir(Some(kept.clone()));
    editor.set_font_files(&h.ctx, Vec::new());
    let warnings = editor.open_file(&handle, &written).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        editor.font_files()[0].path,
        kept.join("InterVariable-subset.ttf")
    );
    // A theme without a font file of the user's is a lone token file.
    editor.set_font_files(&h.ctx, Vec::new());
    let lone = editor
        .save_file(&handle, &dir.join("lone.tokens.json"))
        .unwrap();
    assert_eq!(lone, dir.join("lone.tokens.json"));
    assert!(lone.is_file());
    // An archive, with the feature; registered for this run only
    // without a font folder.
    #[cfg(feature = "theme-zip")]
    {
        editor.set_font_files(&h.ctx, vec![FontFile::read(&font).unwrap()]);
        let zip = editor.save_file(&handle, &dir.join("mine.zip")).unwrap();
        assert_eq!(zip, dir.join("mine.zip"));
        editor.set_font_dir(None);
        editor.set_font_files(&h.ctx, Vec::new());
        let warnings = editor.open_file(&handle, &zip).unwrap();
        assert!(
            matches!(&warnings[..], [egui_pop_core::tokens::Warning::Font { reason, .. }] if reason.contains("this run only")),
            "{warnings:?}"
        );
        assert!(editor.font_files().is_empty());
    }
    #[cfg(not(feature = "theme-zip"))]
    assert!(editor.save_file(&handle, &dir.join("mine.zip")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_contrast_readouts_carry_the_apca_lightness_contrast() {
    let mut h = harness(ThemeSpec::new("pop-light"));
    h.run();
    let read = run(&mut h, &CMD_THEME_READ, json!({ "path": "text.default" }));
    let pair = read["contrast"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["background"] == "surface.panel")
        .unwrap()
        .clone();
    // Dark text on a light panel: a large, positive Lc.
    assert!(pair["lc"].as_f64().unwrap() > 75.0, "{pair}");
    h.state()
        .handle()
        .service(&THEME_EDITOR)
        .unwrap()
        .select("text.default");
    h.run();
    assert!(h.query_by_label("APCA Lc").is_some());
}
