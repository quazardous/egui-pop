//! Quick settings: a small panel over the window it is asked in
//! (`admin.quick`, `Ctrl+Shift+Q`) that edits a setting without the full
//! Settings app. Before anything is typed it shows the settings the
//! manifests mark quick ([`model::Manifest::quick`]), then those changed
//! lately in this session ([`RECENT_SECONDS`], at most [`RECENT_MAX`]);
//! typing searches every setting. Each row is the Settings app's own
//! row: its editor, its reset, and its "Set for" choice only on a setting
//! that also has a host level open now (a library). A change goes where
//! the Settings app would write it, through the host's store. Each row
//! links to its page in Settings.
//!
//! Each application window has its own panel, as the palette: it opens
//! in the window it is asked in, and closes with it.

use egui::Key;
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Kind, OverlaySpec};
use egui_pop_core::strings;
use egui_pop_core::ui::UiPop as _;
use egui_pop_model::admin::Field;
use egui_pop_model::ids::CommandId;
use serde_json::json;

use super::{ADMIN, Admin, CMD_OPEN, model};
use crate::app::AppHandle;
use crate::hooks::SettingEntry;
use crate::strings as txt;

/// Opens and closes the quick settings in the window it is asked in.
pub const CMD_QUICK: CommandId = CommandId::from_static("admin.quick");

/// How long a change counts as lately, in seconds.
pub const RECENT_SECONDS: f64 = 3600.0;

/// How many settings changed lately are shown at most.
pub const RECENT_MAX: usize = 6;

/// How many search hits are shown at most.
const HITS_MAX: usize = 12;

const SPEC_ID: &str = "egui_pop.admin.quick";

/// The panel open in a window group.
#[derive(Clone, Debug, Default)]
pub(super) struct Open {
    query: String,
    /// The window it is shown in: the one active when it first draws.
    window: Option<egui::ViewportId>,
}

/// Seconds since the epoch.
pub fn now() -> f64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

impl Admin {
    /// Notes that the person changed settings (their ids) at `at`
    /// (seconds since the epoch): offered among the recent ones in the
    /// quick settings until [`RECENT_SECONDS`] later.
    pub fn note_changed<'a>(&self, ids: impl IntoIterator<Item = &'a str>, at: f64) {
        let mut r = lock(&self.recent);
        for id in ids {
            r.retain(|(x, _)| x != id);
            r.push((id.to_string(), at));
        }
        let keep = r.len().saturating_sub(64);
        r.drain(..keep);
    }

    /// The settings marked quick that exist, in order.
    pub fn quick_settings(&self, app: &AppHandle) -> Vec<String> {
        let entries = app.settings();
        self.tree(app)
            .quick
            .into_iter()
            .filter(|id| entries.iter().any(|e| e.id.as_str() == id))
            .collect()
    }

    /// The settings changed lately at `now`, the latest first, without
    /// the quick ones, at most [`RECENT_MAX`].
    pub fn recent_settings(&self, app: &AppHandle, now: f64) -> Vec<String> {
        let quick = self.quick_settings(app);
        let entries = app.settings();
        lock(&self.recent)
            .iter()
            .rev()
            .filter(|(id, at)| {
                now - at <= RECENT_SECONDS
                    && !quick.contains(id)
                    && entries.iter().any(|e| e.id.as_str() == id)
            })
            .map(|(id, _)| id.clone())
            .take(RECENT_MAX)
            .collect()
    }

    /// Whether the quick settings are open in a window group.
    pub fn quick_open_in(&self, group: egui::ViewportId) -> bool {
        lock(&self.quick).contains_key(&group)
    }

    /// Opens or closes them in a window group; whether they are open.
    pub fn toggle_quick(&self, group: egui::ViewportId) -> bool {
        let mut q = lock(&self.quick);
        if q.remove(&group).is_some() {
            false
        } else {
            q.insert(group, Open::default());
            true
        }
    }

    /// The window they are shown in, in a window group, once drawn.
    pub fn quick_window_in(&self, group: egui::ViewportId) -> Option<egui::ViewportId> {
        lock(&self.quick).get(&group).and_then(|o| o.window)
    }
}

/// The command.
pub(super) fn commands(ctx: &mut crate::Ctx<'_>) -> Result<(), crate::PluginError> {
    use egui_pop_model::commands::CommandSpec;
    use egui_pop_model::keymap::BindOptions;
    ctx.command(
        CommandSpec::new(CMD_QUICK, "Quick settings", "File")
            .icon("settings")
            .description(
                "The settings changed often and those changed lately, edited without opening Settings.",
            )
            .key_with("Ctrl+Shift+Q", BindOptions::global().through_modal())
            .menu(crate::app::MENU_FILE, "8_settings", 1),
        |call| {
            let a = call.app.service(&ADMIN).ok_or("no admin")?;
            let group = call.app.current_window_group();
            let open = a.toggle_quick(group);
            Ok(json!({ "open": open, "window": call.app.window_of_group(group) }))
        },
    )?;
    Ok(())
}

/// The entries shown: the search's hits, or the quick ones then the
/// recent ones.
fn rows(
    ctx: &egui::Context,
    app: &AppHandle,
    admin: &Admin,
    query: &str,
) -> (Vec<String>, Vec<String>) {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return (admin.quick_settings(app), admin.recent_settings(app, now()));
    }
    let hits = app
        .settings()
        .iter()
        .filter(|e| {
            let d = crate::settings_window::shown(ctx, e);
            d.label.to_lowercase().contains(&q) || e.id.as_str().to_lowercase().contains(&q)
        })
        .map(|e| e.id.as_str().to_string())
        .take(HITS_MAX)
        .collect();
    (hits, Vec::new())
}

/// The manifests' field of a setting (its value scopes), or a plain one.
fn field_of(tree: &model::Tree, id: &str) -> Field {
    tree.fields()
        .into_iter()
        .find(|(_, f)| f.setting_id() == Some(id))
        .map(|(_, f)| f.clone())
        .unwrap_or_else(|| Field::setting(id))
}

/// Draws the panel in the window of its group it opened in.
pub(super) fn show(ui: &mut egui::Ui, app: &AppHandle) {
    let Some(admin) = app.service(&ADMIN) else {
        return;
    };
    let ctx = ui.ctx().clone();
    let group = app.window_group(ctx.viewport_id());
    // A closed window's panel goes with it.
    lock(&admin.quick).retain(|g, _| app.window_group(*g) == *g);
    if !admin.quick_open_in(group) {
        return;
    }
    let window = {
        let mut all = lock(&admin.quick);
        let Some(s) = all.get_mut(&group) else { return };
        *s.window.get_or_insert_with(|| app.shown_in(group))
    };
    if app.still_open(window) != ctx.viewport_id() {
        return;
    }
    let title = strings::tr(&ctx, &txt::ADMIN_QUICK_TITLE);
    let spec = OverlaySpec::new(SPEC_ID, &title, Kind::Modal).keep_on_escape();
    let mut open = true;
    let mut close = false;
    let mut go: Option<String> = None;
    let entries = app.settings();
    overlay::show(&ctx, &spec, &mut open, None, |ui| {
        let mut query = lock(&admin.quick)
            .get(&group)
            .map(|o| o.query.clone())
            .unwrap_or_default();
        let heading = ui.strong(&title);
        let edit = ui
            .add(
                egui::TextEdit::singleline(&mut query)
                    .hint_text(strings::tr(&ctx, &txt::ADMIN_QUICK_HINT))
                    .desired_width(f32::INFINITY),
            )
            .labelled_by(heading.id);
        if !ctx.memory(|m| m.focused().is_some()) {
            edit.request_focus();
        }
        egui_pop_core::focus::escape_passes(ui.ctx(), edit.id);
        if let Some(o) = lock(&admin.quick).get_mut(&group) {
            o.query = query.clone();
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            close = true;
            return;
        }
        let (first, recent) = rows(&ctx, app, &admin, &query);
        let tree = admin.tree(app);
        ui.set_width(420.0);
        ui.scroll_area(egui::ScrollArea::vertical().max_height(420.0), |ui| {
            let mut list = |ui: &mut egui::Ui, ids: &[String]| {
                for id in ids {
                    let Some(e) = entries.iter().find(|e| e.id.as_str() == id) else {
                        continue;
                    };
                    if row(ui, app, &admin, e, &field_of(&tree, id)) {
                        go = Some(id.clone());
                    }
                }
            };
            if query.trim().is_empty() {
                if !first.is_empty() {
                    ui.weak(strings::tr(&ctx, &txt::ADMIN_QUICK_MARKED));
                    list(ui, &first);
                }
                if !recent.is_empty() {
                    ui.weak(strings::tr(&ctx, &txt::ADMIN_QUICK_RECENT));
                    list(ui, &recent);
                }
                if first.is_empty() && recent.is_empty() {
                    ui.weak(strings::tr(&ctx, &txt::ADMIN_QUICK_NONE));
                }
            } else if first.is_empty() {
                ui.weak(strings::tr(&ctx, &txt::ADMIN_QUICK_NO_HIT));
            } else {
                list(ui, &first);
            }
        });
    });
    if !open || close {
        lock(&admin.quick).remove(&group);
    }
    if let Some(key) = go {
        lock(&admin.quick).remove(&group);
        app.run(CMD_OPEN, json!({ "key": key }));
    }
}

/// One setting: the Settings app's row, then a link to its page; whether
/// the link was clicked.
fn row(ui: &mut egui::Ui, app: &AppHandle, admin: &Admin, e: &SettingEntry, f: &Field) -> bool {
    let ctx = ui.ctx().clone();
    let mut go = false;
    ui.push_id(("egui_pop.admin.quick.row", e.id.as_str()), |ui| {
        super::shell::field_row(ui, app, admin, e, f);
        let d = crate::settings_window::shown(&ctx, e);
        let text = strings::tr(&ctx, &txt::ADMIN_QUICK_OPEN);
        let r = ui.link(&text);
        egui_pop_core::a11y::name(
            &r,
            egui::accesskit::Role::Link,
            &strings::tr_args(
                &ctx,
                &txt::ADMIN_QUICK_OPEN_NAME,
                &[("setting", d.label.as_str())],
            ),
        );
        go = r.clicked();
    });
    go
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_ages_out() {
        let a = Admin::new();
        a.note_changed(["x.a"], 1000.0);
        a.note_changed(["x.b"], 2000.0);
        a.note_changed(["x.a"], 3000.0);
        let r: Vec<String> = lock(&a.recent).iter().map(|(i, _)| i.clone()).collect();
        assert_eq!(r, ["x.b", "x.a"]);
    }
}
