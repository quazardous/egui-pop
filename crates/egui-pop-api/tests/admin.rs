//! The administration app: the declared settings as generated pages,
//! opened by deep links, searched, edited in value scopes (a library
//! value over the user's, its origin said and its reset), the user's
//! file keeping the user's value; a manifest's section rendered from
//! data; every row a named node.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::admin::model::hooks::Hook;
use egui_pop_api::admin::model::{
    Action, Callout, Clauses as _, Confirmation, FORMAT, Field, Manifest, Records, RowField,
    Status, Tone,
};
use egui_pop_api::admin::{self, ADMIN, AdminPlugin, CMD_OPEN, CMD_POP_OUT, CMD_RESET, CMD_SET};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::admin::ValueScope;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{PluginId, SettingId};
use egui_pop_model::values::Descriptor;
use serde_json::{Value as Json, json};

const THREADS: SettingId = SettingId::from_static("compute.threads");
const LONG: SettingId = SettingId::from_static("compute.long");
const DEVICES: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("compute.devices");
const DISABLE: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("compute.disable");
const NODES: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("compute.nodes");
const NODE_SET: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("compute.nodeSet");
const NODE_REMOVE: egui_pop_model::ids::CommandId =
    egui_pop_model::ids::CommandId::from_static("compute.nodeRemove");

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
        ctx.setting(
            LONG,
            Descriptor::choice("Long words", &["one", "two"], "one").description(
                "A description long enough to run past the right edge of a narrow page if the row did not wrap it, which it must, so that every control of the row stays inside the page and none is cut off.",
            ),
        )?;
        ctx.command(
            egui_pop_model::commands::CommandSpec::new(DEVICES, "Devices", "Compute").read_only(),
            |_| {
                Ok(json!({ "devices": [
                { "id": "gpu0", "name": "Main GPU", "level": "success", "speed": 12 },
                { "id": "cpu", "name": "Processor", "level": "warning", "speed": 1 }
            ], "state": { "text": "Two devices", "tone": "success" } }))
            },
        )?;
        ctx.command(
            egui_pop_model::commands::CommandSpec::new(DISABLE, "Disable a device", "Compute"),
            |call| Ok(json!({ "disabled": call.args["device"] })),
        )?;
        ctx.command(
            egui_pop_model::commands::CommandSpec::new(NODES, "Nodes", "Compute").read_only(),
            |_| {
                Ok(json!({ "nodes": [
                    { "id": "n1", "name": "Render box", "on": true, "pool": "fast",
                      "pools": [{ "value": "slow", "label": "Slow pool" }],
                      "share": 0.25, "shareText": "A quarter busy", "remote": true },
                    { "id": "n2", "name": "Desk", "on": false, "pool": "slow",
                      "share": 0.5, "shareText": "Half busy", "remote": false }
                ] }))
            },
        )?;
        ctx.command(
            egui_pop_model::commands::CommandSpec::new(NODE_SET, "Set a node", "Compute"),
            |call| Ok(call.args.clone()),
        )?;
        ctx.command(
            egui_pop_model::commands::CommandSpec::new(NODE_REMOVE, "Remove a node", "Compute"),
            |call| Ok(call.args.clone()),
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
                        .status(Status::hook("load", "Load", "compute.load"))
                        .action(
                            Action::command("admin.open")
                                .title("Open the interface settings")
                                .args(json!({ "path": "interface" })),
                        );
                })
                .section("hardware/devices", "Devices", |s| {
                    s.status(Status::command(
                        "state",
                        "State",
                        "compute.devices",
                        "/state",
                    ))
                    .action(
                        Action::command("admin.close")
                            .title("Greyed")
                            .enabled("form.never", "Never available."),
                    )
                    .table(
                        Records::command("devices", "Devices", "compute.devices", "/devices", "id")
                            .column("name", "Name")
                            .column("speed", "Speed")
                            .tone("level")
                            .row_action(
                                Action::command("compute.disable")
                                    .title("Disable")
                                    .args(json!({ "device": "${row.id}" })),
                            ),
                    )
                    .list(
                        Records::command(
                            "none",
                            "Remote nodes",
                            "compute.devices",
                            "/remote",
                            "id",
                        )
                        .empty("No remote node yet.", None),
                    )
                    .action(
                        Action::command("admin.close")
                            .title("Shown when threads are many")
                            .when("form.compute.threads == 100"),
                    )
                    .field(Field::setting("compute.threads"));
                })
                .section("hardware/nodes", "Nodes", |s| {
                    let set = |key: &str| json!({ "node": "${row.id}", key: "${value}" });
                    s.table(
                        Records::command("nodes", "", "compute.nodes", "/nodes", "id")
                            .column("name", "Name")
                            .meter("share", "Load", "shareText")
                            .row_field(RowField::text(
                                "name",
                                "Node name",
                                "compute.nodeSet",
                                set("name"),
                            ))
                            .row_field(RowField::toggle(
                                "on",
                                "Enabled",
                                "compute.nodeSet",
                                set("on"),
                            ))
                            .row_field(
                                RowField::choice(
                                    "pool",
                                    "Pool",
                                    &[(json!("fast"), "Fast pool")],
                                    "compute.nodeSet",
                                    set("pool"),
                                )
                                .choices_from("pools"),
                            )
                            .row_action(
                                Action::command("compute.nodeRemove")
                                    .title("Remove")
                                    .args(json!({ "node": "${row.id}" }))
                                    .when("row.remote")
                                    .confirmation(
                                        Confirmation::new(
                                            "Remove ${row.name}?",
                                            "Its jobs go back to the queue.",
                                            "Remove the node",
                                        )
                                        .danger(),
                                    ),
                            ),
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
fn a_setting_a_manifest_places_is_not_also_generated() {
    let app = app();
    let a = app.handle().service(&ADMIN).unwrap();
    let tree = a.tree(app.handle());
    let places = |id: &str| -> Vec<String> {
        tree.fields()
            .into_iter()
            .filter(|(_, f)| f.setting_id() == Some(id))
            .map(|(path, _)| path)
            .collect()
    };
    // Placed twice by the manifest, never by its prefix.
    assert_eq!(
        places("compute.threads"),
        ["hardware/compute", "hardware/devices"]
    );
    // A setting no manifest places keeps its generated page.
    assert_eq!(places("compute.long"), ["general/compute"]);
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
    h.get_by_label("Hardware › Compute");
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
    // Escape lets go of the search field first (egui-pop's Escape order),
    // then clears the search, then closes.
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(a.shown().2, "worker");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(admin::is_open(h.state().handle()));
    assert_eq!(a.shown().2, "");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!admin::is_open(h.state().handle()));
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

#[test]
fn rows_keep_their_controls_inside_the_page() {
    let width = 640.0;
    let mut h = Harness::builder()
        .with_size(egui::vec2(width, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app());
    h.run();
    let a = h.state().handle().service(&ADMIN).unwrap();
    a.offer(ValueScope::Library, true);
    a.set_in(h.state().handle(), ValueScope::Library, &LONG, json!("two"))
        .unwrap();
    let inside = |h: &Harness<'_, App>, what: &str| {
        let right = width - 1.0;
        for label in [
            "Reset Long words".to_string(),
            "Where Long words is set".to_string(),
        ] {
            let r = h.get_by_label(&label).rect();
            assert!(
                r.max.x <= right,
                "{what}: `{label}` ends at {} past the page ({right})",
                r.max.x
            );
        }
    };
    // On its page, and among the search's hits.
    run(&mut h, &CMD_OPEN, json!({ "key": "compute.long" }));
    h.run();
    inside(&h, "page");
    let hint = "Search every setting";
    h.get_by_role_and_label(egui::accesskit::Role::SearchInput, hint)
        .click();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::SearchInput, hint)
        .type_text("long");
    h.run();
    h.run();
    inside(&h, "search");
}

#[test]
fn records_and_statuses_read_commands_and_clauses_apply() {
    let mut h = harness();
    run(&mut h, &CMD_OPEN, json!({ "path": "hardware/devices" }));
    for _ in 0..3 {
        h.run();
    }
    // The status and the table read the read-only command.
    h.get_by_label("Two devices");
    h.get_by_label("Main GPU");
    h.get_by_label("Processor");
    // An empty source says so.
    h.get_by_label("No remote node yet.");
    // `when` hides, `enabled` greys.
    assert!(h.query_by_label("Shown when threads are many").is_none());
    assert!(h.get_by_label("Greyed").accesskit_node().is_disabled());
    egui_pop_test::assert_all_named(&h);
    // A row action runs with its row's values.
    h.get_all_by_label("Disable").next().unwrap().click();
    h.run();
    h.run();
    let last = h
        .state()
        .handle()
        .history()
        .into_iter()
        .rev()
        .find(|r| r.id == DISABLE)
        .expect("the row action ran");
    assert_eq!(last.args, json!({ "device": "gpu0" }));
    let a = h.state().handle().service(&ADMIN).unwrap();
    assert!(
        a.diagnostics()
            .all()
            .iter()
            .all(|d| d.code != "admin-bad-clause")
    );
}

fn last_run(h: &Harness<'_, App>, id: &egui_pop_model::ids::CommandId) -> Option<Json> {
    h.state()
        .handle()
        .history()
        .into_iter()
        .rev()
        .find(|r| r.id == *id)
        .map(|r| r.args)
}

#[test]
fn row_fields_write_through_their_command_and_actions_confirm_first() {
    let mut h = harness();
    run(&mut h, &CMD_OPEN, json!({ "path": "hardware/nodes/n2" }));
    for _ in 0..3 {
        h.run();
    }
    // The meter, a choice from the declared and the record's choices.
    h.get_by_label("Load: A quarter busy");
    h.get_by_label("Half busy");
    h.get_by_label("Fast pool");
    egui_pop_test::assert_all_named(&h);
    // A toggle writes the row's id and the new value.
    h.get_all_by_role_and_label(egui::accesskit::Role::CheckBox, "Enabled")
        .next()
        .unwrap()
        .click();
    h.run();
    h.run();
    assert_eq!(
        last_run(&h, &NODE_SET),
        Some(json!({ "node": "n1", "on": false }))
    );
    // A text is written when left.
    macro_rules! edit {
        () => {
            h.get_all_by_role_and_label(egui::accesskit::Role::TextInput, "Node name")
                .nth(1)
                .unwrap()
        };
    }
    edit!().click();
    h.run();
    h.key_press(egui::Key::End);
    edit!().type_text(" 2");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    assert_eq!(
        last_run(&h, &NODE_SET),
        Some(json!({ "node": "n2", "name": "Desk 2" }))
    );
    // `when` reads the row: only the remote node can be removed, and
    // only after its confirmation.
    assert_eq!(h.get_all_by_label("Remove").count(), 1);
    h.get_by_label("Remove").click();
    h.run();
    h.run();
    assert!(last_run(&h, &NODE_REMOVE).is_none(), "asked first");
    h.get_by_label("Remove Render box?");
    h.get_by_label("Remove the node").click();
    h.run();
    h.run();
    assert_eq!(last_run(&h, &NODE_REMOVE), Some(json!({ "node": "n1" })));
}
