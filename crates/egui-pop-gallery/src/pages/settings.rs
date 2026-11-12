//! The settings page: settings declared by a plugin, applied live, each
//! with its reset; a custom page of the settings window, "Greeting
//! preview", under the gallery's section, drawing two of them with the
//! preview; and the Settings app (`admin`), every setting as full pages
//! generated from the declarations, with a manifest's section (a
//! callout, a status from a hook, an action) and a value scope the
//! gallery declares, "This project", that the size alone opts into (its
//! row's "Set for"), every other setting app-wide; and a wizard (the wizard kit)
//! that sets the greeting up in four steps, one of them shown only for a
//! bold greeting, and hands its draft to a command.

use egui::{RichText, Ui};
use egui_pop::api::admin::model::hooks::Hook;
use egui_pop::api::admin::model::{
    Action, Callout, Confirmation, FORMAT, Field, FieldKind, Manifest, Records, RowField, Status,
    Tone, Wizard,
};
use egui_pop::api::admin::{ADMIN, CMD_OPEN, CMD_WIZARD_OPEN, ExtraScope};
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

/// The greeting wizard's id.
pub const WIZARD: &str = "gallery.greeting";

/// What the greeting wizard's finish runs.
const APPLY: egui_pop::model::ids::CommandId =
    egui_pop::model::ids::CommandId::from_static("gallery.greeting.apply");

const SIZE: SettingId = SettingId::from_static("gallery.size");
const STRONG: SettingId = SettingId::from_static("gallery.strong");

/// The value scope the gallery declares above the app's: a demo host's
/// "project", which the size opts into.
pub const PROJECT: ExtraScope = ExtraScope::new(
    "project",
    egui_pop::core::strings::StrKey::new("gallery-scope-project", "This project"),
    egui_pop::core::strings::StrKey::new(
        "gallery-scope-project-about",
        "while this project is open, over the app's value",
    ),
);

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
                .description("The preview's font size.")
                .scopes(&[PROJECT.scope]),
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
            admin.declare_scope(PROJECT);
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
                        serde_json::json!({ "id": "en", "name": "English", "greeting": "Hello", "state": "success", "known": 0.9, "knownText": "Most people", "strong": true }),
                        serde_json::json!({ "id": "fr", "name": "French", "greeting": "Bonjour", "state": "success", "known": 0.4, "knownText": "Many people", "strong": false }),
                        serde_json::json!({ "id": "eo", "name": "Esperanto", "greeting": "Saluton", "state": "warning", "known": 0.05, "knownText": "A few people", "strong": false }),
                    ]
                })),
            );
            admin.manifest(Manifest::new(FORMAT).section(
                "general/greetings",
                "Greetings",
                |s| {
                    s.description("Records from a hook: a table and a list, each row with its fields and actions.")
                        .group("table", "Table")
                        .table(
                            Records::hook("languages", "", "gallery.greeting.languages", "id")
                                .column("name", "Language")
                                .column("greeting", "Greeting")
                                .meter("known", "Known by", "knownText")
                                .tone("state")
                                .row_field(RowField::toggle(
                                    "strong",
                                    "In bold",
                                    "admin.set",
                                    serde_json::json!({ "key": "gallery.strong", "value": "${value}" }),
                                ))
                                .row_action(
                                    Action::command("admin.set")
                                        .title("Use")
                                        .args(serde_json::json!({ "key": "gallery.greeting", "value": "${row.greeting}" }))
                                        .confirmation(Confirmation::new(
                                            "Greet in ${row.name}?",
                                            "The greeting becomes \"${row.greeting}\".",
                                            "Use it",
                                        )),
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
            admin.manifest(Manifest::new(FORMAT).wizard(greeting_wizard()));
        }
        ctx.command(
            egui_pop::model::commands::CommandSpec::new(APPLY, "Use a greeting", "Gallery")
                .description("The greeting wizard's result: its words, weight, size and colour.")
                .args(serde_json::json!({ "words": "Hello", "bold": false, "size": 18, "colour": null })),
            |call| {
                use egui_pop::model::admin::ValueScope;
                let admin = call.app.service(&ADMIN).ok_or("no admin")?;
                let set = |id: &str, v: &serde_json::Value| -> Result<(), String> {
                    if v.is_null() {
                        return Ok(());
                    }
                    let id = SettingId::new(id).map_err(|e| format!("{e:?}"))?;
                    admin.set_in(call.app, ValueScope::App, &id, v.clone())
                };
                set("gallery.greeting", &call.args["words"])?;
                set("gallery.strong", &call.args["bold"])?;
                set("gallery.size", &call.args["size"])?;
                set("gallery.tone", &call.args["colour"])?;
                Ok(call.args.clone())
            },
        )?;
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
    if ui.button("Open the settings window").clicked() {
        app.run(
            egui_pop::api::settings_window::CMD_SETTINGS_WINDOW,
            serde_json::Value::Null,
        );
    }
    if ui.button("Open the Settings app").clicked() {
        open_admin(app);
    }
    if ui.button("Open the greeting wizard").clicked() {
        app.run(CMD_WIZARD_OPEN, serde_json::json!({ "id": WIZARD }));
    }
    ui.add_space(t.space.s);
    settings::form(ui, app);
    ui.add_space(t.space.l);
    ui.strong("Preview");
    preview(ui, app);
}

/// The greeting wizard: the words (required), the weight and size, the
/// colour (a step only for a bold greeting), a summary, then
/// `gallery.greeting.apply` with the draft.
fn greeting_wizard() -> Wizard {
    let field = |key: &str, title: &str, kind| Field::draft(WIZARD, key, title, kind);
    Wizard::new(WIZARD, "Set up the greeting", "gallery.greeting.apply")
        .description("A wizard declared as data: each step a form, checked before the next.")
        .step("words", "Words", |s| {
            s.description("What the preview says.").field(
                field("words", "Words", FieldKind::Text)
                    .required()
                    .help("Shown in the preview, in the colour and size chosen next."),
            );
        })
        .step("weight", "Weight and size", |s| {
            s.field(field("bold", "Bold", FieldKind::Toggle).default_value(serde_json::json!(false)))
                .field(
                    field("size", "Size in points", FieldKind::Number)
                        .range(8.0, 48.0)
                        .default_value(serde_json::json!(18)),
                )
                .callout(Callout::new(
                    "bold",
                    Tone::Info,
                    "A bold greeting gets a colour step.",
                ));
        })
        .step("colour", "Colour", |s| {
            s.field(
                field("colour", "Colour", FieldKind::Choice)
                    .choice(serde_json::json!("default"), "The text's colour")
                    .choice(serde_json::json!("accent"), "The accent")
                    .choice(serde_json::json!("muted"), "Muted")
                    .default_value(serde_json::json!("accent")),
            );
        })
        .when("form.bold")
        .optional()
        .args(serde_json::json!({ "words": "${words}", "bold": "${bold}", "size": "${size}", "colour": "${colour}" }))
        .verb("Use this greeting")
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

/// Opens the Settings app on the gallery's section, a project open with
/// its size over the user's.
pub fn open_admin(app: &AppHandle) {
    if let Some(admin) = app.service(&ADMIN) {
        admin.offer(PROJECT.scope, true);
        if admin.export_layer(PROJECT.scope).is_empty() {
            admin.load_layer(
                app,
                PROJECT.scope,
                [("gallery.size".to_string(), serde_json::json!(24.0))].into(),
            );
        }
    }
    app.run(
        CMD_OPEN,
        serde_json::json!({ "path": "general/gallery-settings" }),
    );
}
