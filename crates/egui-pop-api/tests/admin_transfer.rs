//! Settings export and import: a file written and read back, the import's
//! plan (what changes, and why the rest is left out), the changes applied
//! as one undo step, secrets never written, and the import wizard driven
//! by its commands and seen through the accessibility tree.

use std::path::PathBuf;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::admin::transfer::{CMD_EXPORT, CMD_IMPORT, CMD_UNDO_IMPORT, IMPORT_WIZARD};
use egui_pop_api::admin::{self, ADMIN, AdminPlugin, CMD_WIZARD_NEXT, CMD_WIZARD_OPEN};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{CommandId, PluginId, SettingId};
use egui_pop_model::values::Descriptor;
use serde_json::{Value as Json, json};

const VOLUME: SettingId = SettingId::from_static("demo.volume");
const COUNT: SettingId = SettingId::from_static("demo.count");
const THEME: SettingId = SettingId::from_static("demo.theme");
const TOKEN: SettingId = SettingId::from_static("demo.token");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.setting(VOLUME, Descriptor::float("Volume", 0.0, 1.0, 0.5))?;
        ctx.setting(COUNT, Descriptor::int("Count", 1, 9, 3))?;
        ctx.setting(
            THEME,
            Descriptor::choice("Theme", &["dark", "light"], "dark"),
        )?;
        ctx.setting(TOKEN, Descriptor::text("Sync token", "").sensitive())?;
        Ok(())
    }
}

fn app() -> App {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(AdminPlugin)
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap()
}

fn harness() -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app());
    h.run();
    h
}

fn run(h: &mut Harness<'_, App>, cmd: &CommandId, args: Json) -> Result<Json, String> {
    let out = h.state_mut().run(cmd, args, Origin::User).now().unwrap();
    h.run();
    h.run();
    out
}

fn set(h: &mut Harness<'_, App>, id: &SettingId, value: Json) {
    run(
        h,
        &admin::CMD_SET,
        json!({ "key": id.as_str(), "value": value }),
    )
    .unwrap();
}

fn get(h: &Harness<'_, App>, id: &SettingId) -> Json {
    let store = h.state().handle().store();
    let s = store.lock().unwrap();
    s.get(&id.path()).unwrap().value.to_json()
}

fn file(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("settings-transfer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn an_export_reads_back_as_it_was_and_never_holds_a_secret() {
    let mut h = harness();
    set(&mut h, &VOLUME, json!(0.8));
    set(&mut h, &THEME, json!("light"));
    set(&mut h, &TOKEN, json!("s3cret"));
    let path = file("round-trip.json");
    let out = run(
        &mut h,
        &CMD_EXPORT,
        json!({ "file": path.display().to_string(), "shortcuts": true }),
    )
    .unwrap();
    assert_eq!(out["written"], true);
    assert_eq!(out["leftOut"], json!(["demo.token"]));
    assert_eq!(out["shortcuts"], true);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("s3cret"), "the secret is never written");
    assert!(text.contains("\"kind\": \"settings-export\""));
    assert_eq!(
        out["document"]["settings"]["values"]["demo.volume"],
        json!(0.8)
    );
    assert!(
        out["document"]["settings"]["values"]
            .get("demo.token")
            .is_none()
    );
    // A dry run writes nothing.
    let dry = file("dry.json");
    run(
        &mut h,
        &CMD_EXPORT,
        json!({ "file": dry.display().to_string(), "dryRun": true }),
    )
    .unwrap();
    assert!(!dry.exists());
    // Back to the defaults, then the file read back.
    set(&mut h, &VOLUME, json!(0.5));
    set(&mut h, &THEME, json!("dark"));
    let out = run(
        &mut h,
        &CMD_IMPORT,
        json!({ "file": path.display().to_string() }),
    )
    .unwrap();
    assert_eq!(out["applied"], json!(["demo.theme", "demo.volume"]));
    assert_eq!(get(&h, &VOLUME), json!(0.8));
    assert_eq!(get(&h, &THEME), json!("light"));
    assert_eq!(
        get(&h, &TOKEN),
        json!("s3cret"),
        "the secret stays as it is"
    );
}

#[test]
fn the_plan_shows_each_change_and_why_the_rest_is_left_out() {
    let mut h = harness();
    set(&mut h, &COUNT, json!(4));
    let document = json!({
        "kind": "settings-export",
        "version": 1,
        "settings": { "version": 3, "values": {
            "demo.volume": 0.25,
            "demo.count": 4,
            "demo.theme": "sepia",
            "demo.token": "stolen",
            "demo.retired": true,
            "elsewhere.thing": 1,
        } },
    });
    h.state()
        .handle()
        .service(&ADMIN)
        .unwrap()
        .obsolete("demo.retired", "replaced by Theme");
    let plan = run(
        &mut h,
        &CMD_IMPORT,
        json!({ "document": document, "dryRun": true }),
    )
    .unwrap();
    let verdicts: Vec<(String, String)> = plan["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["key"].as_str().unwrap().to_string(),
                r["verdict"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let v = |k: &str, w: &str| (k.to_string(), w.to_string());
    assert_eq!(
        verdicts,
        [
            v("demo.count", "same"),
            v("demo.retired", "obsolete"),
            v("demo.theme", "invalid"),
            v("demo.token", "sensitive"),
            v("demo.volume", "change"),
            v("elsewhere.thing", "unknown"),
        ]
    );
    let volume = &plan["changes"][0];
    assert_eq!(
        (&volume["current"], &volume["imported"]),
        (&json!(0.5), &json!(0.25))
    );
    assert_eq!(volume["title"], "Volume");
    let theme = plan["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "demo.theme")
        .unwrap();
    assert_eq!(
        theme["reason"],
        "invalid: `sepia` is not one of dark, light"
    );
    // Applied: only the change; the rest is skipped with its reason.
    let out = run(&mut h, &CMD_IMPORT, json!({ "document": document })).unwrap();
    assert_eq!(out["applied"], json!(["demo.volume"]));
    let skipped: Vec<&str> = out["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        skipped,
        [
            "demo.retired",
            "demo.theme",
            "demo.token",
            "elsewhere.thing"
        ]
    );
    assert_eq!(
        get(&h, &THEME),
        json!("dark"),
        "an invalid value is never applied"
    );
    assert_eq!(get(&h, &TOKEN), json!(""), "nor a secret");
}

#[test]
fn an_import_is_one_undo_step() {
    let mut h = harness();
    let document = json!({
        "kind": "settings-export",
        "version": 1,
        "settings": { "version": 3, "values": {
            "demo.volume": 0.9, "demo.count": 7, "demo.theme": "light",
        } },
    });
    // Only the changes chosen.
    let out = run(
        &mut h,
        &CMD_IMPORT,
        json!({ "document": document, "keys": ["demo.volume", "demo.theme"] }),
    )
    .unwrap();
    assert_eq!(out["applied"], json!(["demo.theme", "demo.volume"]));
    assert_eq!(
        out["skipped"][0],
        json!({ "key": "demo.count", "reason": "not chosen" })
    );
    assert_eq!(get(&h, &COUNT), json!(3));
    let store = h.state().handle().store();
    let steps = store
        .lock()
        .unwrap()
        .undo_steps_in(&egui_pop_model::history::SETTINGS)
        .len();
    assert_eq!(steps, 1, "one step for the whole import");
    run(&mut h, &CMD_UNDO_IMPORT, json!({})).unwrap();
    assert_eq!(get(&h, &VOLUME), json!(0.5));
    assert_eq!(get(&h, &THEME), json!("dark"));
    assert!(
        run(&mut h, &CMD_UNDO_IMPORT, json!({})).is_err(),
        "nothing left to undo"
    );
    // The settings' own undo takes it back too.
    run(&mut h, &CMD_IMPORT, json!({ "document": document })).unwrap();
    assert_eq!(get(&h, &COUNT), json!(7));
    store
        .lock()
        .unwrap()
        .undo_in(&egui_pop_model::history::SETTINGS, &Origin::User)
        .unwrap();
    assert_eq!(
        (get(&h, &VOLUME), get(&h, &COUNT), get(&h, &THEME)),
        (json!(0.5), json!(3), json!("dark"))
    );
}

#[test]
fn the_import_wizard_shows_the_changes_and_what_is_left_out() {
    let mut h = harness();
    let path = file("wizard.json");
    std::fs::write(
        &path,
        json!({
            "kind": "settings-export",
            "version": 1,
            "settings": { "version": 3, "values": {
                "demo.volume": 0.75, "demo.theme": "light", "demo.count": 40,
                "elsewhere.thing": 1,
            } },
        })
        .to_string(),
    )
    .unwrap();
    // The entries in General › Advanced.
    run(
        &mut h,
        &admin::CMD_OPEN,
        json!({ "path": "general/advanced" }),
    )
    .unwrap();
    h.get_by_label("Export settings…");
    h.get_by_label("Import settings…").click();
    for _ in 0..3 {
        h.run();
    }
    egui_pop_test::assert_all_named(&h);
    h.get_by_role_and_label(Role::ListItem, "Step 1 of 3, File");
    // A file that is not a settings file blocks Next with its reason.
    let bad = file("bad.json");
    std::fs::write(&bad, "{\"kind\": \"keymap\"}").unwrap();
    run(
        &mut h,
        &admin::CMD_WIZARD_SET,
        json!({ "key": "file", "value": bad.display().to_string() }),
    )
    .unwrap();
    let e = run(&mut h, &CMD_WIZARD_NEXT, json!({})).unwrap_err();
    assert!(e.starts_with("not a settings file"), "{e}");
    run(
        &mut h,
        &admin::CMD_WIZARD_SET,
        json!({ "key": "file", "value": path.display().to_string() }),
    )
    .unwrap();
    let s = run(&mut h, &CMD_WIZARD_NEXT, json!({})).unwrap();
    assert_eq!(s["step"], "changes");
    for _ in 0..3 {
        h.run();
    }
    // Each change ticked, with its value now and in the file.
    h.get_by_role_and_label(Role::CheckBox, "Volume: 0.5 → 0.75");
    h.get_by_role_and_label(Role::CheckBox, "Theme: dark → light");
    // Left out, with the reason.
    h.get_by_label("invalid: 40 is above 9");
    h.get_by_label("unknown: no such setting here");
    egui_pop_test::assert_all_named(&h);
    // Untick the theme, then import.
    h.get_by_role_and_label(Role::CheckBox, "Theme: dark → light")
        .click();
    for _ in 0..2 {
        h.run();
    }
    let a = h.state().handle().service(&ADMIN).unwrap();
    assert_eq!(
        a.wizard_flow().unwrap().values["keys"],
        json!(["demo.volume"])
    );
    run(&mut h, &admin::CMD_WIZARD_GO, json!({ "step": "summary" })).unwrap();
    h.get_by_role_and_label(Role::Group, "Apply: Volume: 0.5 → 0.75");
    let done = run(&mut h, &admin::CMD_WIZARD_FINISH, json!({})).unwrap();
    assert_eq!(done["command"], "settings.import");
    h.run();
    assert_eq!(get(&h, &VOLUME), json!(0.75));
    assert_eq!(get(&h, &THEME), json!("dark"));
    // Opened alone, the wizard's id deep-links too.
    run(&mut h, &CMD_WIZARD_OPEN, json!({ "id": IMPORT_WIZARD })).unwrap();
    assert_eq!(a.wizard_flow().unwrap().at, "file");
}
