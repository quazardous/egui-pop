//! The focus the Settings edit: whose host values (a library's, a
//! workspace's...) the Settings show and write, when the host has several
//! windows that each have their own.
//!
//! The host names its foci ([`SettingsFoci::foci`]): an id, a label, the
//! application windows that belong to each. Opened from a window
//! (`admin.open`), the Settings edit that window's focus; a header above
//! the page names it and a switch picks another, shown once there are
//! several foci. Editing a focus is the host's to do
//! ([`SettingsFoci::edit`]): it attaches the store holding that focus's
//! values and offers its scopes, so the fields show that focus's values.
//! A setting with no host level is the app's and the same in every focus.
//!
//! egui-pop resolves one value per setting in a process: the layers of
//! the focus edited are the ones applied.

use std::sync::Arc;

use egui::Ui;
use egui::accesskit::Role;
use egui_pop_core::{a11y, lock, strings};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::CommandId;
use serde_json::{Value as Json, json};

use super::{ADMIN, APP_WINDOW, Admin};
use crate::app::AppHandle;
use crate::strings as txt;
use crate::windows::WindowId;
use crate::{Ctx, PluginError};

/// Edits a focus's values in the Settings (`{"id"}`); without `id`, says
/// which is edited and which there are.
pub const CMD_FOCUS: CommandId = CommandId::from_static("admin.focus");

/// A focus the Settings can edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsFocus {
    /// Its id, kept while it is open.
    pub id: String,
    /// What the header names it, in the interface's language ("Main
    /// window: library Photos").
    pub label: String,
    /// The application windows that belong to it: Settings opened from one
    /// of them edit it.
    pub windows: Vec<WindowId>,
}

impl SettingsFocus {
    pub fn new(id: &str, label: impl Into<String>, windows: &[WindowId]) -> SettingsFocus {
        SettingsFocus {
            id: id.into(),
            label: label.into(),
            windows: windows.to_vec(),
        }
    }
}

/// The host's foci, as the Settings ask for them.
pub trait SettingsFoci: Send + Sync + 'static {
    /// The foci open now, the main one first: the first is edited when
    /// the one edited closes.
    fn foci(&self, app: &AppHandle) -> Vec<SettingsFocus>;

    /// Makes the Settings edit a focus: the host attaches its store
    /// ([`Admin::attach_store`]) and offers its scopes ([`Admin::offer`]).
    ///
    /// # Errors
    /// The focus cannot be edited.
    fn edit(&self, app: &AppHandle, focus: &str) -> Result<(), String>;
}

/// The host's foci and the one edited.
#[derive(Default)]
pub(super) struct Foci {
    host: Option<Arc<dyn SettingsFoci>>,
    edited: Option<String>,
}

impl Admin {
    /// Gives the Settings the host's foci (see the module's docs).
    pub fn set_foci(&self, foci: impl SettingsFoci) {
        lock(&self.foci).host = Some(Arc::new(foci));
    }

    /// The host's foci open now (none without [`Admin::set_foci`]).
    pub fn foci(&self, app: &AppHandle) -> Vec<SettingsFocus> {
        let host = lock(&self.foci).host.clone();
        host.map(|h| h.foci(app)).unwrap_or_default()
    }

    /// The focus the Settings edit, once one was chosen.
    pub fn edited_focus(&self) -> Option<String> {
        lock(&self.foci).edited.clone()
    }

    /// Makes the Settings edit a focus of the host's.
    ///
    /// # Errors
    /// No such focus, or the host cannot edit it.
    pub fn edit_focus(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let host = lock(&self.foci).host.clone().ok_or("no foci")?;
        if !host.foci(app).iter().any(|f| f.id == id) {
            return Err(format!("no focus `{id}`"));
        }
        if self.edited_focus().as_deref() == Some(id) {
            return Ok(());
        }
        host.edit(app, id)?;
        lock(&self.foci).edited = Some(id.into());
        Ok(())
    }

    /// The Settings asked in `window`: they edit its focus. The Settings'
    /// own window, or a window of no focus, leaves the one edited.
    pub(super) fn asked_in(&self, app: &AppHandle, window: &WindowId) {
        if window.as_str() == APP_WINDOW {
            return;
        }
        let focus = self
            .foci(app)
            .into_iter()
            .find(|f| f.windows.contains(window));
        if let Some(f) = focus
            && let Err(e) = self.edit_focus(app, &f.id)
        {
            log::warn!("settings: the focus `{}` not edited: {e}", f.id);
        }
    }

    /// Each pass: a focus edited that closed gives way to the first.
    pub(super) fn keep_focus(&self, app: &AppHandle) {
        let Some(edited) = self.edited_focus() else {
            return;
        };
        let foci = self.foci(app);
        if foci.iter().any(|f| f.id == edited) {
            return;
        }
        lock(&self.foci).edited = None;
        if let Some(first) = foci.first()
            && let Err(e) = self.edit_focus(app, &first.id)
        {
            log::warn!("settings: the focus `{}` not edited: {e}", first.id);
        }
    }
}

/// The header above the page: the focus edited and the switch, when the
/// host has several.
pub(super) fn header(ui: &mut Ui, app: &AppHandle, admin: &Admin) {
    let foci = admin.foci(app);
    if foci.len() < 2 {
        return;
    }
    let ctx = ui.ctx().clone();
    let edited = admin.edited_focus();
    let current = foci
        .iter()
        .find(|f| Some(&f.id) == edited.as_ref())
        .unwrap_or(&foci[0]);
    let mut picked = current.id.clone();
    ui.horizontal(|ui| {
        let title = strings::tr(&ctx, &txt::ADMIN_FOCUS);
        ui.label(&title);
        let r = egui::ComboBox::from_id_salt("egui_pop.admin.focus")
            .selected_text(&current.label)
            .show_ui(ui, |ui| {
                for f in &foci {
                    ui.selectable_value(&mut picked, f.id.clone(), &f.label);
                }
            })
            .response
            .on_hover_text(strings::tr(&ctx, &txt::ADMIN_FOCUS_TIP));
        a11y::name(&r, Role::ComboBox, &title);
    });
    if picked != current.id
        && let Err(e) = admin.edit_focus(app, &picked)
    {
        log::warn!("settings: the focus `{picked}` not edited: {e}");
    }
}

pub(super) fn commands(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    ctx.command(
        CommandSpec::new(CMD_FOCUS, "Settings: edit a focus", "File")
            .hidden()
            .args(json!({ "id": "" })),
        |call| {
            let a = call.app.service(&ADMIN).ok_or("no admin")?;
            if let Some(id) = call
                .args
                .get("id")
                .and_then(Json::as_str)
                .filter(|i| !i.is_empty())
            {
                a.edit_focus(call.app, id)?;
            }
            Ok(state(call.app, &a))
        },
    )?;
    Ok(())
}

/// The foci and the one edited, as agents read them.
pub(super) fn state(app: &AppHandle, admin: &Admin) -> Json {
    json!({
        "edited": admin.edited_focus(),
        "foci": admin.foci(app).iter().map(|f| json!({
            "id": f.id,
            "label": f.label,
            "windows": f.windows.iter().map(WindowId::as_str).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}
