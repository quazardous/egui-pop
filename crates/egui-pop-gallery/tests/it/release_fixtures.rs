//! Documents written by released versions, loaded by today's code
//! (ADR 0008).
//!
//! `fixtures/<version>/` at the repository's root holds what the gallery
//! of that release wrote after a fixed scenario ([`write_fixtures`],
//! `cargo xtask fixtures <version>` at release time, see
//! `docs/releasing.md`): every persisted document, a theme file saved by
//! the theme editor, and `expected.json`, the values the scenario set.
//!
//! [`documents_of_earlier_releases_load`] reads every folder with
//! today's code: each document loads without error and reports its
//! migrations when it was written at an older version, the gallery
//! started on the folder shows every expected value, and it reports no
//! problem that a gallery started on nothing does not.
//!
//! The scenario uses only calls every release since 0.19 has, so that it
//! can be run again at an older tag (copied into its checkout) to fill a
//! missing folder.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_pop::api::theme_editor::{Cause, THEME_EDITOR};
use egui_pop::api::{App, AppHandle, arrangements, contexts};
use egui_pop::core::persist::{Persist, Source};
use egui_pop::model::commands::Origin;
use egui_pop::model::doc::Schema;
use egui_pop::model::ids::{CommandId, PanelId};
use egui_pop::model::keymap::{KeymapDoc, Layer};
use egui_pop::model::panels::Area;
use egui_pop::model::values::{Change, Set, Value, settings_schema};
use egui_pop_gallery::Options;
use serde_json::{Value as Json, json};

/// Where the scenario writes when run by `cargo xtask fixtures`.
const OUT: &str = "EGUI_POP_FIXTURES_OUT";
/// The release the folder is written for, recorded in `expected.json`.
const VERSION: &str = "EGUI_POP_FIXTURES_VERSION";

/// What the scenario changes.
const SETTING: &str = "settings/dialogs.movable";
const COMMAND: &str = "gallery.notes.ask_clear";
const KEYS: &str = "Ctrl+Alt+N";
const PANEL: PanelId = PanelId::from_static("gallery.properties");
const AREA: Area = Area::StartSide;
const ARRANGEMENT: &str = "Fixture";
const PLACE_VALUE: &str = "fixtures.note";
const THEME_FILE: &str = "theme.tokens.json";
const TOKEN: &str = "accent.default";
const TOKEN_VALUE: &str = "#2060c0";

/// The documents a folder may hold, by name, with their schema.
fn schema_of(name: &str) -> Option<Schema> {
    use egui_pop::model::{context, keymap_schema, layout};
    Some(match name {
        "settings" => settings_schema(),
        "keymap" => keymap_schema(),
        "layout" => layout::schema(),
        "context" => context::schema(),
        "view" => context::view_schema(),
        _ => return None,
    })
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// A fresh directory under the system's temporary one.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "egui-pop-fixtures-{tag}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The gallery keeping its documents in `dir`, in a headless harness.
fn gallery(dir: Option<&Path>) -> Harness<'static, App> {
    let ctx = egui::Context::default();
    let mut builder = egui_pop_gallery::builder(&Options::default());
    if let Some(dir) = dir {
        builder = builder.persist(dir);
    }
    let app = builder.build(&ctx).expect("the gallery builds");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    settle(&mut h);
    h
}

fn settle(h: &mut Harness<'_, App>) {
    for _ in 0..4 {
        let _ = h.run_ok();
    }
}

fn handle(h: &Harness<'_, App>) -> AppHandle {
    h.state().handle().clone()
}

/// The keys the user's layer gives a command, as the keymap exports them.
fn user_keys(app: &AppHandle, command: &str) -> Option<String> {
    let doc: KeymapDoc = app.export_keymap(Layer::User);
    doc.bindings
        .iter()
        .find(|b| b.command.as_deref() == Some(command))
        .map(|b| b.keys.clone())
}

fn setting(app: &AppHandle, path: &str) -> Option<Json> {
    let store = app.store();
    let store = egui_pop::core::lock(&store);
    store
        .get(path)
        .map(|e| serde_json::to_value(&e.value).unwrap())
}

/// The scenario: a setting changed, a key rebound, a panel moved, an
/// arrangement saved, a value kept for the place shown, a theme saved
/// from the editor; then every document written, and `expected.json`
/// listing what was set. `cargo xtask fixtures <version>` runs it.
#[test]
#[ignore = "writes a release's fixtures: cargo xtask fixtures <version>"]
fn write_fixtures() {
    let out = PathBuf::from(std::env::var(OUT).expect("EGUI_POP_FIXTURES_OUT names the folder"));
    let version = std::env::var(VERSION).unwrap_or_default();
    std::fs::create_dir_all(&out).unwrap();
    let mut h = gallery(Some(&out));
    let app = handle(&h);

    egui_pop::core::lock(&app.store())
        .set(Set::new(
            SETTING,
            Change::To(Value::Bool(true)),
            Origin::User,
        ))
        .expect("the setting is declared");
    app.rebind(CommandId::new(COMMAND).unwrap(), Some(KEYS.into()));
    assert!(app.move_panel(&PANEL, AREA).is_some_and(|o| o.ok()));
    settle(&mut h);
    assert!(arrangements::save(&app, ARRANGEMENT).ok());
    contexts::set_place_value(&app, PLACE_VALUE, json!("kept"));
    let editor = app.service(&THEME_EDITOR).expect("the theme editor");
    editor
        .set(&app, TOKEN, Some(json!(TOKEN_VALUE)), Cause::Agent)
        .expect("the token is set");
    settle(&mut h);
    editor
        .save_file(&app, &out.join(THEME_FILE))
        .expect("the theme is saved");
    h.state().save().expect("the documents are written");

    // What the live gallery shows is what the folder must give back.
    let keys = user_keys(&app, COMMAND).expect("the key is rebound");
    assert_eq!(app.layout().area_of(&PANEL), Some(AREA), "the panel moved");
    let expected = json!({
        "version": version,
        "settings": { SETTING: setting(&app, SETTING).expect("the setting is kept") },
        "keys": { COMMAND: keys },
        "panels": { PANEL.as_str(): format!("{AREA:?}") },
        "arrangements": [ARRANGEMENT],
        "place_values": { PLACE_VALUE: "kept" },
        "theme": { "file": THEME_FILE, "tokens": { TOKEN: TOKEN_VALUE } },
    });
    std::fs::write(
        out.join("expected.json"),
        serde_json::to_string_pretty(&expected).unwrap() + "\n",
    )
    .unwrap();
    // The backups and anything half-written are not part of a release's
    // output.
    for entry in std::fs::read_dir(&out).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name.ends_with(".bak") || name.ends_with(".tmp") {
            std::fs::remove_file(&path).unwrap();
        }
    }
}

/// Every fixture folder: its documents, each read with today's schema.
#[test]
fn documents_of_earlier_releases_load() {
    let root = fixtures_root();
    let mut folders: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    assert!(
        !folders.is_empty(),
        "no fixture folder in {}",
        root.display()
    );
    let baseline = problems(&handle(&gallery(None)));
    for folder in folders {
        check_folder(&folder, &baseline);
    }
}

/// The errors and warnings an app reports, as `code: subject`.
fn problems(app: &AppHandle) -> Vec<String> {
    use egui_pop::model::diagnostics::Level;
    app.diagnostics()
        .into_iter()
        .filter(|d| matches!(d.level, Level::Error | Level::Warning))
        .map(|d| format!("{}: {}: {}", d.code, d.subject, d.message))
        .collect()
}

fn check_folder(folder: &Path, baseline: &[String]) {
    let release = folder.file_name().unwrap().to_string_lossy().to_string();
    let expected: Json = serde_json::from_str(
        &std::fs::read_to_string(folder.join("expected.json"))
            .unwrap_or_else(|e| panic!("{release}: expected.json: {e}")),
    )
    .unwrap();
    let theme_file = expected["theme"]["file"].as_str().unwrap_or_default();

    // Each document alone: read, migrated and said so.
    let persist = Persist::new(folder);
    let mut documents = 0;
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        if file == "expected.json" || file == theme_file {
            continue;
        }
        let name = file
            .strip_suffix(".json")
            .unwrap_or_else(|| panic!("{release}: {file} is not a document"));
        let schema = schema_of(name).unwrap_or_else(|| {
            panic!("{release}: {file}: a document today's test does not know; add its schema")
        });
        let read = persist
            .load::<Json>(name, &schema)
            .unwrap_or_else(|e| panic!("{release}: {file}: {e}"))
            .unwrap_or_else(|| panic!("{release}: {file}: not read"));
        assert_eq!(read.source, Source::Main, "{release}: {file}");
        if read.from_version < schema.current {
            assert!(
                read.notes.iter().any(|n| n.starts_with("migrated")),
                "{release}: {file} was written at version {} and today's is {}, but no migration is reported: {:?}",
                read.from_version,
                schema.current,
                read.notes
            );
        }
        documents += 1;
    }
    assert!(documents > 0, "{release}: no document");

    // The gallery started on a copy (it may write): every value back.
    let dir = scratch(&release);
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(&path, dir.join(path.file_name().unwrap())).unwrap();
    }
    let h = gallery(Some(&dir));
    let app = handle(&h);
    let new: Vec<String> = problems(&app)
        .into_iter()
        .filter(|p| !baseline.contains(p))
        .collect();
    assert!(new.is_empty(), "{release}: the gallery reports {new:#?}");
    let known = [
        "version",
        "settings",
        "keys",
        "panels",
        "arrangements",
        "place_values",
        "theme",
    ];
    for key in expected.as_object().unwrap().keys() {
        assert!(
            known.contains(&key.as_str()),
            "{release}: expected.json says `{key}`, which today's test does not check"
        );
    }
    for (path, value) in object(&expected["settings"]) {
        assert_eq!(
            setting(&app, &path).as_ref(),
            Some(&value),
            "{release}: setting {path}"
        );
    }
    for (command, keys) in object(&expected["keys"]) {
        assert_eq!(
            user_keys(&app, &command).map(Json::from),
            Some(keys),
            "{release}: keys of {command}"
        );
    }
    for (panel, area) in object(&expected["panels"]) {
        let id = PanelId::new(&panel).unwrap();
        assert_eq!(
            app.layout().area_of(&id).map(|a| format!("{a:?}")),
            area.as_str().map(str::to_string),
            "{release}: area of {panel}"
        );
    }
    let names = arrangements::names(&app);
    for name in expected["arrangements"].as_array().into_iter().flatten() {
        let name = name.as_str().unwrap();
        assert!(
            names.iter().any(|n| n == name),
            "{release}: arrangement {name} in {names:?}"
        );
        assert!(
            arrangements::saved(&app).contains_key(name),
            "{release}: arrangement {name} saved"
        );
    }
    for (name, value) in object(&expected["place_values"]) {
        assert_eq!(
            contexts::place_value(&app, &name),
            Some(value),
            "{release}: place value {name}"
        );
    }
    if !theme_file.is_empty() {
        let editor = app.service(&THEME_EDITOR).expect("the theme editor");
        editor
            .open_file(&app, &dir.join(theme_file))
            .unwrap_or_else(|e| panic!("{release}: {theme_file}: {e}"));
        for (token, value) in object(&expected["theme"]["tokens"]) {
            let read = editor
                .read(&app, Some(&token))
                .unwrap_or_else(|e| panic!("{release}: token {token}: {e}"));
            assert!(
                read.to_string().contains(value.as_str().unwrap()),
                "{release}: token {token} is not {value}: {read}"
            );
        }
    }
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

fn object(v: &Json) -> BTreeMap<String, Json> {
    v.as_object()
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}
