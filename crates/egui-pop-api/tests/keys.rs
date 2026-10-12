//! The keymap at run time: commands declared and removed after build, a
//! full keymap document loaded and exported with its diff, every key
//! removed from a command without a placeholder, conflicts by scope,
//! rebinding as a step a plugin may veto.

use egui_pop_api::app::{STEP_LOAD_KEYMAP, STEP_REBIND};
use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, OwnerId, PluginId};
use egui_pop_model::keymap::{DocBinding, KeymapDoc, Keys, Layer, Resolution};
use egui_pop_model::when::WhenContext;
use serde_json::Value as Json;

const GO: CommandId = CommandId::from_static("demo.go");
const STOP: CommandId = CommandId::from_static("demo.stop");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(CommandSpec::new(GO, "Go", "Demo").key("Ctrl+G"), |_| {
            Ok(Json::Null)
        })?
        .command(CommandSpec::new(STOP, "Stop", "Demo").key("Ctrl+H"), |_| {
            Ok(Json::Null)
        })?;
        ctx.before(STEP_REBIND, |_, args| {
            if args["keys"] == "Ctrl+Q" {
                Before::Veto("Ctrl+Q stays Quit".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn app() -> App {
    App::builder()
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap()
}

fn resolves(app: &App, keys: &str) -> Resolution {
    app.keymap()
        .resolve(&Keys::parse(keys).unwrap(), &WhenContext::new())
}

#[test]
fn a_command_declared_after_build_has_its_key_menu_and_palette_entry() {
    let mut app = app();
    let h = app.handle().clone();
    let id = CommandId::from_static("host.export");
    h.declare(
        &OwnerId::from_static("host"),
        CommandSpec::new(id.clone(), "Export", "File")
            .key("Ctrl+E")
            .menu(egui_pop_api::app::MENU_FILE, "5_host", 0),
    );
    app.process_requests();
    assert!(h.palette().iter().any(|e| e.command == id));
    assert!(h.menu(&egui_pop_api::app::MENU_FILE).iter().any(|m| {
        matches!(&m.item, egui_pop_model::menus::Item::Command { id: c, .. } if *c == id)
    }));
    assert_eq!(resolves(&app, "Ctrl+E"), Resolution::Run(id.clone()));
    // A second declaration of the id is refused and said.
    h.declare(
        &OwnerId::from_static("host"),
        CommandSpec::new(id.clone(), "Again", "File"),
    );
    app.process_requests();
    assert!(
        h.events_since(0)
            .iter()
            .any(|e| e.name == "command.declare_failed")
    );
    // Removed: its key and entries go.
    h.undeclare(&id);
    app.process_requests();
    assert!(h.palette().iter().all(|e| e.command != id));
    assert_eq!(resolves(&app, "Ctrl+E"), Resolution::Nothing);
    let _ = Origin::User;
}

#[test]
fn a_keymap_round_trip_keeps_scopes_clauses_and_every_key() {
    let mut app = app();
    let doc = KeymapDoc {
        bindings: vec![
            DocBinding {
                keys: "Ctrl+K Ctrl+G".into(),
                command: Some("demo.go".into()),
                when: "!textInputFocus".into(),
                ..DocBinding::default()
            },
            DocBinding {
                keys: "F5".into(),
                command: Some("demo.go".into()),
                scope: "surface:canvas".into(),
                ..DocBinding::default()
            },
            DocBinding {
                keys: String::new(),
                command: Some("-demo.stop".into()),
                ..DocBinding::default()
            },
        ],
    };
    let diff = app.load_keymap(&doc, Layer::User).unwrap();
    assert_eq!(diff.added.len(), 3);
    assert!(diff.skipped.is_empty());
    let back = app.handle().export_keymap(Layer::User);
    assert_eq!(back, doc);
    // Stop lost every key, with no placeholder key.
    assert!(app.handle().keys_for(&STOP).is_empty());
    assert_eq!(resolves(&app, "Ctrl+H"), Resolution::Nothing);
    // Loading again: a changed binding is reported as changed.
    let mut doc2 = doc.clone();
    doc2.bindings[1].command = Some("demo.stop".into());
    let diff = app.load_keymap(&doc2, Layer::User).unwrap();
    assert_eq!(diff.changed.len(), 1, "{diff:?}");
    assert!(diff.added.is_empty() && diff.removed.is_empty());
    // A conflict is reported, not dropped.
    let doc3 = KeymapDoc {
        bindings: vec![DocBinding {
            keys: "Ctrl+H".into(),
            command: Some("demo.go".into()),
            ..DocBinding::default()
        }],
    };
    let diff = app.load_keymap(&doc3, Layer::User).unwrap();
    assert!(
        diff.conflicts.iter().any(|c| c.contains("Ctrl+H")),
        "{:?}",
        diff.conflicts
    );
    assert!(!app.handle().key_conflicts_by_scope().is_empty());
    assert!(
        app.handle()
            .step_traces()
            .iter()
            .any(|t| t.step == STEP_LOAD_KEYMAP)
    );
    // Only the user's and the workspace's layers load.
    assert!(app.load_keymap(&doc, Layer::Default).is_err());
}

#[test]
fn removing_every_key_needs_no_placeholder_and_a_rebind_may_be_vetoed() {
    let mut app = app();
    let h = app.handle().clone();
    h.rebind(GO, None);
    app.process_requests();
    assert!(h.keys_for(&GO).is_empty());
    let doc = h.export_keymap(Layer::User);
    assert_eq!(doc.bindings.len(), 1);
    assert_eq!(doc.bindings[0].keys, "");
    assert_eq!(doc.bindings[0].command.as_deref(), Some("-demo.go"));
    // A plugin vetoes one rebinding.
    h.rebind(GO, Some("Ctrl+Q".into()));
    app.process_requests();
    assert!(h.keys_for(&GO).is_empty());
    let t = h.step_traces().pop().unwrap();
    assert_eq!(t.step, STEP_REBIND);
    assert!(t.vetoed_by.is_some());
    h.rebind(GO, Some("Ctrl+J".into()));
    app.process_requests();
    assert_eq!(h.keys_for(&GO), ["Ctrl+J"]);
}

#[test]
fn an_old_keymap_document_with_the_placeholder_key_migrates() {
    let old = r#"{"version": 1, "kind": "keymap", "bindings": [{"keys": "Ctrl+Alt+Shift+F24", "command": "-demo.go"}]}"#;
    let read = egui_pop_model::keymap_schema()
        .load::<KeymapDoc>(old)
        .unwrap();
    assert_eq!(read.value.bindings[0].keys, "");
}
