//! The administration app: the declared settings as generated pages,
//! opened by deep links, searched, edited in value scopes (a library
//! value over the user's, its origin said and its reset), the user's
//! file keeping the user's value; a manifest's section rendered from
//! data; every row a named node.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::admin::model::hooks::Hook;
use egui_pop_api::admin::model::{Action, Callout, FORMAT, Field, Manifest, Tone};
use egui_pop_api::admin::{self, ADMIN, AdminPlugin, CMD_OPEN, CMD_POP_OUT, CMD_RESET, CMD_SET};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::admin::ValueScope;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{PluginId, SettingId};
use egui_pop_model::values::Descriptor;
use serde_json::{Value as Json, json};

const THREADS: SettingId = SettingId::from_static("compute.threads");

struct Compute;

impl Plugin for Compute {
    fn id(&self) -> PluginId {
        PluginId::from_static("compute")
    }

    fn title(&self) -> String {
        "Compute".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![PluginId::from_static("egui_pop.admin")]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.setting(
            THREADS,
            Descriptor::int("Worker threads", 1, 64, 4).description("Threads the pipeline uses."),
        )?;
        let admin = ctx.app().service(&ADMIN).expect("the admin service");
        admin.hook(
            "compute.load",
            Hook::Status(Box::new(|| ("Idle".into(), Tone::Success))),
        );
        admin.manifest(
            Manifest::new(FORMAT)
                .scope("hardware", "Hardware", "chip")
                .section("hardware/compute", "Compute", |s| {
                    s.description("Where the pipeline runs.")
                        .callout(Callout::new("note", Tone::Info, "One device found."))
                        .group("workers", "Workers")
                        .field(Field::setting("compute.threads"))
                        .field(Field::setting("compute.missing"))
                        .status(egui_pop_api::admin::model::Status::hook(
                            "load",
                            "Load",
                            "compute.load",
                        ))
                        .action(
                            Action::command("admin.open")
                                .title("Open the interface settings")
                                .args(json!({ "path": "interface" })),
                        );
                }),
        );
        Ok(())
    }
}

fn app() -> App {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(AdminPlugin)
        .plugin(Compute)
        .build(&egui::Context::default())
        .unwrap()
}

fn harness() -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 720.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app());
    h.run();
    h
}

fn run(h: &mut Harness<'_, App>, cmd: &egui_pop_model::ids::CommandId, args: Json) -> Json {
    let out = h.state_mut().run(cmd, args, Origin::User).now().unwrap();
    h.run();
    h.run();
    out.unwrap()
}

fn threads(app: &App) -> i64 {
    app.handle().setting::<i64>(&THREADS).unwrap()
}

#[test]
fn a_library_value_wins_says_its_origin_and_leaves_the_users_file_alone() {
    let dir = std::env::temp_dir().join(format!("egui-pop-admin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(AdminPlugin)
        .plugin(Compute)
        .persist(&dir)
        .build(&egui::Context::default())
        .unwrap();
    let a = app.handle().service(&ADMIN).unwrap();
    // No library open: refused.
    let refused = app.run(
        &CMD_SET,
        json!({ "key": "compute.threads", "scope": "library", "value": 8 }),
        Origin::User,
    );
    assert!(refused.now().unwrap().is_err());
    a.offer(ValueScope::Library, true);
    let ok = |r: egui_pop_api::host::Reply| r.now().unwrap().unwrap();
    ok(app.run(
        &CMD_SET,
        json!({ "key": "compute.threads", "scope": "app", "value": 6 }),
        Origin::User,
    ));
    ok(app.run(
        &CMD_SET,
        json!({ "key": "compute.threads", "scope": "library", "value": 8 }),
        Origin::User,
    ));
    assert_eq!(threads(&app), 8, "what everyone reads is the library's");
    assert_eq!(
        a.layers().origin("settings/compute.threads"),
        ValueScope::Library
    );
    assert_eq!(
        a.export_layer(ValueScope::Library)["compute.threads"],
        json!(8)
    );
    app.save().unwrap();
    let saved: String = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| std::fs::read_to_string(e.unwrap().path()).ok())
        .collect();
    assert!(saved.contains("compute.threads"), "{saved}");
    assert!(saved.contains('6') && !saved.contains('8'), "{saved}");
    // The library's reset gives the user's value back.
    ok(app.run(
        &CMD_RESET,
        json!({ "key": "compute.threads", "scope": "library" }),
        Origin::User,
    ));
    assert_eq!(threads(&app), 6);
    // A command line override is shown, never edited.
    a.set_run(app.handle(), &THREADS, json!(2)).unwrap();
    assert_eq!(threads(&app), 2);
    assert!(
        app.run(
            &CMD_SET,
            json!({ "key": "compute.threads", "scope": "run", "value": 3 }),
            Origin::User,
        )
        .now()
        .unwrap()
        .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_settings_open_as_generated_pages_and_deep_links_land() {
    let mut h = harness();
    let out = run(&mut h, &CMD_OPEN, json!({ "key": "compute.threads" }));
    assert_eq!(out["scope"], "hardware");
    assert_eq!(out["section"], "compute");
    assert!(admin::is_open(h.state().handle()));
    egui_pop_test::assert_all_named(&h);
    // Every row is a named node; the manifest's status and callout show.
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Worker threads");
    h.get_by_label("Idle");
    h.get_by_label("One device found.");
    // A setting the manifest names that is not declared is left out,
    // and reported.
    let a = h.state().handle().service(&ADMIN).unwrap();
    assert!(
        a.diagnostics()
            .all()
            .iter()
            .any(|d| d.code == "admin-no-setting")
    );
    // The action runs its command: a deep link to a scope.
    h.get_by_label("Open the interface settings").click();
    h.run();
    h.run();
    assert_eq!(a.shown().0.as_deref(), Some("interface"));
    // A generated section by path.
    let out = run(&mut h, &CMD_OPEN, json!({ "path": "general" }));
    assert_eq!(out["scope"], "general");
    assert!(
        h.state_mut()
            .run(&CMD_OPEN, json!({ "path": "nowhere/at-all" }), Origin::User)
            .now()
            .unwrap()
            .is_err()
    );
    // Escape closes.
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!admin::is_open(h.state().handle()));
}

#[test]
fn the_search_finds_settings_in_every_scope_and_a_row_edits_its_scope() {
    let mut h = harness();
    run(&mut h, &CMD_OPEN, json!({ "path": "general" }));
    let hint = "Search every setting";
    h.get_by_role_and_label(egui::accesskit::Role::SearchInput, hint)
        .click();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SearchInput, hint)
        .type_text("worker");
    h.run();
    h.run();
    // The hit, under its breadcrumb, editable in place.
    h.get_by_label("General › Compute");
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Worker threads");
    // A library value: the row says it is modified and where it comes
    // from.
    let a = h.state().handle().service(&ADMIN).unwrap();
    a.offer(ValueScope::Library, true);
    a.set_in(h.state().handle(), ValueScope::Library, &THREADS, json!(12))
        .unwrap();
    h.run();
    h.run();
    h.get_by_role_and_label(
        egui::accesskit::Role::Group,
        "Worker threads, modified, library",
    );
    h.get_by_label("Where Worker threads is set");
    egui_pop_test::assert_all_named(&h);
    // Escape clears the search first, then closes.
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(admin::is_open(h.state().handle()));
    assert_eq!(a.shown().2, "");
}

#[test]
fn pop_out_is_remembered() {
    let mut h = harness();
    run(&mut h, &CMD_OPEN, json!({}));
    assert!(!admin::in_window(h.state().handle()));
    run(&mut h, &CMD_POP_OUT, json!({}));
    assert!(admin::in_window(h.state().handle()));
    run(&mut h, &CMD_OPEN, json!({ "mode": "overlay" }));
    assert!(!admin::in_window(h.state().handle()));
}
