//! Icons on commands someone else declared: egui-pop's own commands carry
//! built-in icons, and a plugin sets the icon of a command it does not own
//! (`Ctx::command_icon`); the last one wins, it goes with its plugin, and
//! a command that is not declared is reported.

use egui_pop_api::clipboard::{self, ClipboardPlugin};
use egui_pop_api::{App, AppBuilder, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{CommandId, OwnerId, PluginId};

const RESKIN: PluginId = PluginId::from_static("reskin");
const MISSING: CommandId = CommandId::from_static("demo.missing");

/// Sets the icons of the clipboard's commands, and of one nobody declared.
struct Reskin;

impl Plugin for Reskin {
    fn id(&self) -> PluginId {
        RESKIN
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![clipboard::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command_icon(clipboard::CMD_PASTE, "file", None)
            .command_icon(clipboard::CMD_CUT, "delete", Some("close"))
            .command_icon(MISSING, "file", None);
        Ok(())
    }
}

/// A second plugin setting the paste icon again, after `Reskin`.
struct Later;

impl Plugin for Later {
    fn id(&self) -> PluginId {
        PluginId::from_static("later")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![RESKIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command_icon(clipboard::CMD_PASTE, "folder", None);
        Ok(())
    }
}

fn build(builder: AppBuilder) -> App {
    builder.build(&egui::Context::default()).unwrap()
}

fn icon(app: &App, id: &CommandId) -> (Option<String>, Option<String>) {
    let spec = app.handle().command(id).unwrap();
    (spec.icon, spec.icon_on)
}

#[test]
fn the_edit_commands_carry_built_in_icons() {
    let app = build(App::builder().plugin(ClipboardPlugin));
    for (id, name) in [
        (clipboard::CMD_CUT, "cut"),
        (clipboard::CMD_COPY, "copy"),
        (clipboard::CMD_PASTE, "paste"),
        (clipboard::CMD_PASTE_SPECIAL, "paste-special"),
        (egui_pop_api::app::CMD_UNDO, "undo"),
        (egui_pop_api::app::CMD_REDO, "redo"),
    ] {
        assert_eq!(icon(&app, &id).0.as_deref(), Some(name), "{id}");
        assert!(egui_pop_core::icon::registry::is_known(name), "{name}");
    }
    assert!(
        app.handle()
            .diagnostics()
            .iter()
            .all(|d| d.code != "unknown-icon"),
        "{:?}",
        app.handle().diagnostics()
    );
}

#[test]
fn a_plugin_sets_the_icon_of_a_command_it_does_not_own() {
    let app = build(App::builder().plugin(ClipboardPlugin).plugin(Reskin));
    assert_eq!(
        icon(&app, &clipboard::CMD_PASTE),
        (Some("file".into()), None)
    );
    assert_eq!(
        icon(&app, &clipboard::CMD_CUT),
        (Some("delete".into()), Some("close".into()))
    );
    // Untouched commands keep their own.
    assert_eq!(icon(&app, &clipboard::CMD_COPY).0.as_deref(), Some("copy"));
    assert_eq!(
        app.handle().icon_set_by(&clipboard::CMD_PASTE),
        Some(OwnerId::from(&RESKIN))
    );
    let snapshot = app.handle().debug_snapshot();
    let commands = snapshot["commands"]["commands"].as_array().unwrap();
    let paste = commands.iter().find(|c| c["id"] == "edit.paste").unwrap();
    assert_eq!(paste["icon"], "file");
    assert_eq!(paste["icon_set_by"], "reskin");
    // A command nobody declared is reported, naming who set its icon.
    let missing: Vec<_> = app
        .handle()
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "unknown-command" && d.subject == MISSING.as_str())
        .collect();
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing[0].owner, Some(OwnerId::from(&RESKIN)));
}

#[test]
fn the_last_plugin_wins_and_the_icon_goes_with_its_plugin() {
    let mut app = build(
        App::builder()
            .plugin(ClipboardPlugin)
            .plugin(Reskin)
            .plugin(Later),
    );
    assert_eq!(
        icon(&app, &clipboard::CMD_PASTE).0.as_deref(),
        Some("folder")
    );
    app.disconnect(&RESKIN);
    app.rebuild();
    // `Later` required `Reskin`: both are gone, the declared icons back.
    assert_eq!(
        icon(&app, &clipboard::CMD_PASTE).0.as_deref(),
        Some("paste")
    );
    assert_eq!(icon(&app, &clipboard::CMD_CUT).0.as_deref(), Some("cut"));
    assert_eq!(app.handle().icon_set_by(&clipboard::CMD_PASTE), None);
}
