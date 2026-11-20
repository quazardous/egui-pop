//! Panel instances: a panel declared with several
//! ([`PanelSpec::instances`](egui_pop_model::panels::PanelSpec::instances),
//! [`Instances::Multiple`](egui_pop_model::panels::Instances::Multiple))
//! opens as many times as asked.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! The panel declared is instance 1; each new one is `<kind>#<n>`
//! ([`PanelId::instance`]), still a [`PanelId`], so the dock, the focus,
//! the fault boundary and the panel menu treat it as any panel. One draw
//! function draws them all ([`Ctx::instance_panel`](crate::Ctx::instance_panel)),
//! told which [`Instance`] it draws. Each instance holds a payload the
//! application owns (opaque JSON): kept in the layout
//! (`Layout::instances`), saved with it and handed back on restore
//! ([`AppHandle::instance_payload`], [`AppHandle::set_instance_payload`]).
//!
//! `pop.panel.open` with `instance: "new"` opens a new one where it is
//! asked; closing an instance drops it with its payload (closing the
//! panel declared keeps its place, as any panel's). The journal says
//! both: `panel.instance.opened` and `panel.instance.closed`
//! (`{ "panel": <id>, "kind": <kind> }`).

use std::collections::BTreeSet;

use egui::Ui;
use egui_pop_model::ids::PanelId;
use egui_pop_model::panels::Instances;
use serde_json::{Value as Json, json};

use crate::app::AppHandle;

/// Draws an instance of a panel declared with several.
pub type InstanceDrawFn = Box<dyn FnMut(&mut Ui, &AppHandle, &Instance)>;

/// The instance a panel's draw function draws.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Instance {
    /// The panel declared (`app.gallery`).
    pub kind: PanelId,
    /// This instance's id: the kind for instance 1, else `<kind>#<n>`.
    pub id: PanelId,
    /// Its number, from 1.
    pub number: u32,
    /// Its title as shown ("Gallery 2").
    pub title: String,
}

impl Instance {
    pub(crate) fn of(id: &PanelId, title: &str) -> Instance {
        Instance {
            kind: id.kind(),
            id: id.clone(),
            number: id.instance_number(),
            title: title.to_string(),
        }
    }
}

/// The journal's event when an instance opens.
pub const EVENT_OPENED: &str = "panel.instance.opened";
/// The journal's event when an instance closes (its payload dropped).
pub const EVENT_CLOSED: &str = "panel.instance.closed";

/// The instances open now (`<kind>#<n>`).
pub(crate) fn open_ids(app: &AppHandle) -> BTreeSet<PanelId> {
    app.workbench_mut()
        .specs
        .iter()
        .filter(|s| s.id.is_instance())
        .map(|s| s.id.clone())
        .collect()
}

/// Says on the journal the instances opened and closed since `before`.
pub(crate) fn say(app: &AppHandle, before: &BTreeSet<PanelId>) {
    let now = open_ids(app);
    for id in now.difference(before) {
        app.publish(
            "egui_pop",
            None,
            EVENT_OPENED,
            json!({ "panel": id, "kind": id.kind() }),
        );
    }
    for id in before.difference(&now) {
        app.publish(
            "egui_pop",
            None,
            EVENT_CLOSED,
            json!({ "panel": id, "kind": id.kind() }),
        );
    }
}

impl AppHandle {
    /// The payload the application keeps for a panel instance (`Null`
    /// when it has none): as it was set, or as the layout restored it.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn instance_payload(&self, id: &PanelId) -> Json {
        self.workbench_mut()
            .layout
            .instance(id)
            .map(|i| i.payload.clone())
            .unwrap_or(Json::Null)
    }

    /// Sets the payload of a panel instance open now (or of the panel
    /// declared, instance 1, of a kind declared with several), saved with
    /// the layout. The layout's change is said (`pop.layout.changed`).
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn set_instance_payload(&self, id: &PanelId, payload: Json) -> Result<(), String> {
        {
            let mut wb = self.workbench_mut();
            let multiple = wb
                .spec(&id.kind())
                .is_some_and(|s| s.instances == Instances::Multiple);
            if !multiple || wb.spec(id).is_none() {
                return Err(format!(
                    "`{id}` is no open instance of a panel declared with several"
                ));
            }
            if wb.layout.instance(id).is_some_and(|i| i.payload == payload) {
                return Ok(());
            }
            wb.layout.instance_mut(id).payload = payload;
        }
        self.note_layout("panel.instance.payload");
        Ok(())
    }

    /// The instances of a panel declared with several, open now, the
    /// panel declared first when it shows.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn panel_instances(&self, kind: &PanelId) -> Vec<PanelId> {
        let kind = kind.kind();
        let mut wb = self.workbench_mut();
        let mut out: Vec<PanelId> = wb
            .specs
            .iter()
            .filter(|s| s.id.is_instance() && s.id.kind() == kind)
            .map(|s| s.id.clone())
            .collect();
        out.sort_by_key(PanelId::instance_number);
        if wb.shows(&kind) {
            out.insert(0, kind);
        }
        out
    }
}
