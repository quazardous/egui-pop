//! The settings page: settings declared by a plugin, applied live, each
//! with its reset; a custom page of the settings window, "Greeting
//! preview", under the gallery's section, drawing two of them with the
//! preview; and the Settings app (`admin`), every setting as full pages
//! generated from the declarations, with a manifest's section (a
//! callout, a status from a hook, an action) and a library and a
//! workspace layer over the user's values.

use egui::{RichText, Ui};
use egui_pop::api::admin::model::hooks::Hook;
use egui_pop::api::admin::model::{
    Action, Callout, FORMAT, Field, Manifest, Records, Status, Tone,
};
use egui_pop::api::admin::{ADMIN, CMD_OPEN};
use egui_pop::api::settings;
use egui_pop::api::settings_pages::SettingsPage as CustomPage;
use egui_pop::api::settings_window;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::binding;
use egui_pop::model::ids::{PluginId, SettingId, StoryId};
use egui_pop::model::values::Descriptor;

use crate::shell::SHELL;

pub const SETTINGS: PluginId = PluginId::from_static("gallery.settings");

/// The custom page of the settings window.
pub const PREVIEW_PAGE: &str = "gallery.preview";

const SIZE: SettingId = SettingId::from_static("gallery.size");
const STRONG: SettingId = SettingId::from_static("gallery.strong");

/// The settings page.
#[derive(Debug, Default)]
pub struct SettingsPage;

impl Plugin for SettingsPage {
    fn id(&self) -> PluginId {
        SETTINGS
    }

    fn title(&self) -> String {
        "Gallery settings".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, PluginId::from_static("egui_pop.admin")]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.setting(
            SettingId::from_static("gallery.greeting"),
            Descriptor::text("Greeting", "Hello").description("The preview's text."),
        )?
        .setting(
            SIZE,
            Descriptor::float("Size", 8.0, 48.0, 18.0)
                .step(1.0)
                .unit("pt")
                .description("The preview's font size."),
        )?
        .setting(
            STRONG,
            Descriptor::bool("Strong", false).description("Draw the preview in bold."),
        )?
        .setting(
            SettingId::from_static("gallery.tone"),
            Descriptor::choice("Tone", &["default", "accent", "muted"], "default"),
        )?
        .story(
            Story::new(StoryId::from_static("settings"), "Settings", show)
                .description("Settings declared by plugins, applied as you change them, each with its reset.")
                .group("Framework", 110),
        )?;
        // A section of the Settings app declared as data: no widget drawn
        // by this plugin.
        if let Some(admin) = ctx.app().service(&ADMIN) {
            admin.hook(
                "gallery.greeting.length",
                Hook::Status(Box::new(|| ("5 letters".into(), Tone::Success))),
            );
            admin.manifest(Manifest::new(FORMAT).section(
                "general/greeting",
                "Greeting text",
                |s| {
                    s.description("The preview's text and weight, declared as a manifest.")
                        .callout(Callout::new(
                            "tip",
                            Tone::Info,
                            "Values set for the library or the workspace win over yours.",
                        ))
                        .group("text", "Text")
                        .field(Field::setting("gallery.greeting"))
                        .field(Field::setting("gallery.strong"))
                        .status(Status::hook("length", "Length", "gallery.greeting.length"))
                        .action(
                            Action::command("admin.open")
                                .title("Open the interface settings")
                                .args(serde_json::json!({ "path": "interface" })),
                        );
                },
            ));
            admin.hook(
                "gallery.greeting.languages",
                Hook::Records(Box::new(|| {
                    vec![
                        serde_json::json!({ "id": "en", "name": "English", "greeting": "Hello", "state": "success" }),
                        serde_json::json!({ "id": "fr", "name": "French", "greeting": "Bonjour", "state": "success" }),
                        serde_json::json!({ "id": "eo", "name": "Esperanto", "greeting": "Saluton", "state": "warning" }),
                    ]
                })),
            );
            admin.manifest(Manifest::new(FORMAT).section(
                "general/greetings",
                "Greetings",
                |s| {
                    s.description("Records from a hook: a table and a list, each row with its actions.")
                        .group("table", "Table")
                        .table(
                            Records::hook("languages", "", "gallery.greeting.languages", "id")
                                .column("name", "Language")
                                .column("greeting", "Greeting")
                                .tone("state")
                                .row_action(
                                    Action::command("admin.set")
                                        .title("Use")
                                        .args(serde_json::json!({ "key": "gallery.greeting", "value": "${row.greeting}" })),
                                ),
                        )
                        .group("list", "List")
                        .list(
                            Records::hook("list", "", "gallery.greeting.languages", "id")
                                .titles("name", Some("greeting")),
                        )
                        .list(
                            Records::hook("none", "", "gallery.greeting.none", "id")
                                .empty("No hook gives these records yet.", None),
                        );
                },
            ));
            admin.hook("gallery.greeting.none", Hook::Records(Box::new(Vec::new)));
        }
        // A page of its own in the settings window, under this plugin's
        // section: the size and weight with the preview they change.
        ctx.settings_page(
            CustomPage::new(PREVIEW_PAGE, "Greeting preview")
                .under("Gallery settings")
                .keywords(&["specimen", "sample", "font"])
                .description("The greeting as the settings draw it.")
                .settings(&[SIZE, STRONG]),
            |ui, app| {
                settings_window::setting_row(ui, app, &SIZE);
                settings_window::setting_row(ui, app, &STRONG);
                preview(ui, app);
            },
        );
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let t = egui_pop::theme::Tokens::of(ui.ctx());
    if ui.button("Open the settings window (Ctrl+,)").clicked() {
        app.run(
            egui_pop::api::settings_window::CMD_SETTINGS,
            serde_json::json!({ "section": "" }),
        );
    }
    if ui.button("Open the Settings app").clicked() {
        open_admin(app);
    }
    ui.add_space(t.space.s);
    settings::form(ui, app);
    ui.add_space(t.space.l);
    ui.strong("Preview");
    preview(ui, app);
}

/// The greeting, drawn as the settings say.
fn preview(ui: &mut Ui, app: &AppHandle) {
    let t = egui_pop::theme::Tokens::of(ui.ctx());
    let store = app.store();
    let text: String = binding::read(&store, "settings/gallery.greeting").unwrap_or_default();
    let size: f64 = binding::read(&store, "settings/gallery.size").unwrap_or(18.0);
    let strong: bool = binding::read(&store, "settings/gallery.strong").unwrap_or(false);
    let tone = egui_pop::core::lock(&store)
        .get("settings/gallery.tone")
        .and_then(|e| e.value.as_choice().map(str::to_string))
        .unwrap_or_default();
    let color = match tone.as_str() {
        "accent" => t.accent.default,
        "muted" => t.text.muted,
        _ => t.text.default,
    };
    let mut rich = RichText::new(text).size(size as f32).color(color);
    if strong {
        rich = rich.strong();
    }
    ui.label(rich);
}

/// Opens the Settings app on the gallery's section, a library and a
/// workspace open with a value each over the user's.
pub fn open_admin(app: &AppHandle) {
    use egui_pop::model::admin::ValueScope;
    if let Some(admin) = app.service(&ADMIN) {
        admin.offer(ValueScope::Library, true);
        admin.offer(ValueScope::Workspace, true);
        if admin.export_layer(ValueScope::Library).is_empty() {
            admin.load_layer(
                app,
                ValueScope::Library,
                [("gallery.size".to_string(), serde_json::json!(24.0))].into(),
            );
            admin.load_layer(
                app,
                ValueScope::Workspace,
                [("gallery.tone".to_string(), serde_json::json!("accent"))].into(),
            );
        }
    }
    app.run(
        CMD_OPEN,
        serde_json::json!({ "path": "general/gallery-settings" }),
    );
}
