//! The translations page: the language setting, egui-pop's own strings
//! and declared titles as they follow it, plurals, numbers and dates
//! from this page's catalogue, keys shown instead of text, and the keys
//! missing in the current language.

use std::sync::atomic::{AtomicI64, Ordering};

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::binding::{self, Binding};
use egui_pop::core::strings;
use egui_pop::i18n::{Catalogue, args, plugin, t};
use egui_pop::model::ids::{PluginId, StoryId};

use super::section;
use crate::shell::SHELL;

pub const I18N_PAGE: PluginId = PluginId::from_static("gallery.i18n");

/// The page's catalogues.
pub static CATALOGUES: &[Catalogue] = &[
    Catalogue::new(
        "en",
        "gallery.ftl",
        include_str!("../../locales/en/gallery.ftl"),
    ),
    Catalogue::new(
        "fr",
        "gallery.ftl",
        include_str!("../../locales/fr/gallery.ftl"),
    ),
];

/// How many images the plural example counts.
static COUNT: AtomicI64 = AtomicI64::new(1);

/// The translations page.
#[derive(Debug, Default)]
pub struct I18nPage;

impl Plugin for I18nPage {
    fn id(&self) -> PluginId {
        I18N_PAGE
    }

    fn title(&self) -> String {
        "Translations".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, plugin::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.story(
            Story::new(StoryId::from_static("i18n"), "Translations", show)
                .description(
                    "Fluent catalogues per language: egui-pop's own text, declared titles, plurals, numbers and dates follow the language setting.",
                )
                .group("Framework", 105),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(i18n) = plugin::of(ui.ctx()) else {
        ui.weak("The translation plugin is not connected.");
        return;
    };
    let ctx = ui.ctx().clone();
    ui.label(t(&ctx, "gallery-i18n-intro", &[]));
    section(ui, &t(&ctx, "gallery-i18n-language", &[]));
    let setting = egui_pop::core::lock(&app.store())
        .get(&format!("settings/{}", plugin::SETTING_LANGUAGE))
        .and_then(|e| e.value.as_choice().map(str::to_string))
        .unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        let mut choices = vec![(
            plugin::SYSTEM.to_string(),
            t(&ctx, "gallery-i18n-system", &[]),
        )];
        choices.extend(i18n.offered());
        for (value, name) in choices {
            if ui.selectable_label(setting == value, name).clicked() {
                let _ = plugin::set_language(app, &value);
            }
        }
    });
    let store = app.store();
    binding::edit(
        ui,
        Binding::bound(&store, &format!("settings/{}", plugin::SETTING_SHOW_KEYS)),
        |ui, on: &mut bool| ui.checkbox(on, t(&ctx, "gallery-i18n-show-keys", &[])),
    );

    section(ui, &t(&ctx, "gallery-i18n-own", &[]));
    ui.horizontal_wrapped(|ui| {
        for key in [strings::OK, strings::CANCEL, strings::RETRY, strings::RESET] {
            let _ = ui.button(strings::tr(&ctx, &key));
        }
    });
    ui.label(strings::tr_args(
        &ctx,
        &strings::FAULT_TITLE,
        &[("name", "Log")],
    ));

    section(ui, &t(&ctx, "gallery-i18n-declared", &[]));
    for (spec, _) in app.commands().into_iter().take(4) {
        ui.label(format!(
            "{}: {}",
            strings::category(&ctx, &spec.category),
            strings::command_title(&ctx, spec.id.as_str(), &spec.title)
        ));
    }

    section(ui, &t(&ctx, "gallery-i18n-formats", &[]));
    let mut count = COUNT.load(Ordering::Relaxed);
    ui.horizontal(|ui| {
        let label = ui.label(t(&ctx, "gallery-i18n-count", &[]));
        ui.add(egui::DragValue::new(&mut count).range(0..=100_000))
            .labelled_by(label.id);
    });
    COUNT.store(count, Ordering::Relaxed);
    ui.label(t(&ctx, "gallery-i18n-photos", &args![count = count]));
    ui.label(t(&ctx, "gallery-i18n-size", &args![mib = 1234.56]));
    ui.label(t(&ctx, "gallery-i18n-date", &args![when = "2026-10-12"]));

    let locale = i18n.locale();
    let missing = i18n.missing().remove(&locale).unwrap_or_default();
    if missing.is_empty() {
        ui.weak(t(
            &ctx,
            "gallery-i18n-none-missing",
            &args![locale = locale.as_str()],
        ));
    } else {
        ui.weak(t(
            &ctx,
            "gallery-i18n-missing",
            &args![locale = locale.as_str(), count = missing.len() as f64],
        ));
        for key in missing.iter().take(8) {
            ui.monospace(key);
        }
    }
}
