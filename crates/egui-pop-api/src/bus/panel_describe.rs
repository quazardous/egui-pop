//! A panel described on the bus (ADR 0029): `pop.panel.describe` answers
//! what a declared panel is, its kind first ([`PanelKind`]: whether it
//! edits), for agents, tests and the application's own parts, without a
//! screenshot.
//!
//! The workbench's listener ([`PANEL_DESCRIBE`]) fills the description
//! from the panel's declaration; a listener after it may add to it. It is
//! asked on the interface's thread ([`AppHandle::describe_panel`]):
//! elsewhere nobody answers and the description stays `None`.
//!
//! [`AppHandle::describe_panel`]: crate::AppHandle::describe_panel

use bus_contract::{Bus, ListenOpts, Point, PointKind};
use egui_pop_model::ids::PanelId;
use egui_pop_model::panels::PanelKind;
use serde::{Deserialize, Serialize};

use super::{OWNER, asking};

/// A declared panel, as the bus describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PanelDescription {
    /// The panel's id.
    pub panel: String,
    /// Its title, as declared (English).
    pub title: String,
    /// What it does to what the application edits.
    pub kind: PanelKind,
    /// Whether it is a palette (it floats when it opens).
    pub palette: bool,
}

/// A panel is described (`pop.panel.describe`): the input is its id, the
/// value its description, `None` while nobody knows it.
#[derive(Debug)]
pub struct PanelDescribe;
impl Point for PanelDescribe {
    const KIND: PointKind = PointKind::Alter;
    const ID: &'static str = "pop.panel.describe";
    type Input = String;
    type Value = Option<PanelDescription>;
}

/// The panel description points, in the order [`super::order_dump`]
/// prints them.
pub const PANEL_DESCRIBE_POINTS: [&str; 1] = [PanelDescribe::ID];

/// The workbench's listener of `pop.panel.describe`: describes a declared
/// panel from its declaration.
pub const PANEL_DESCRIBE: &str = "pop.workbench.panel_describe";

/// Registers the workbench's listener of `pop.panel.describe`.
pub(super) fn install(kernel: &dyn Bus) {
    let listened = kernel.listen::<PanelDescribe>(
        PANEL_DESCRIBE,
        ListenOpts::new().owner(OWNER),
        |panel, value| {
            let (Some(app), Ok(id)) = (asking(), PanelId::new(panel)) else {
                return Ok(());
            };
            let Some(wb) = app.try_workbench() else {
                return Err(format!("`{panel}`: the workbench is busy").into());
            };
            if let Some(spec) = wb.spec(&id) {
                *value = Some(PanelDescription {
                    panel: spec.id.to_string(),
                    title: spec.title.clone(),
                    kind: spec.kind,
                    palette: spec.palette,
                });
            }
            Ok(())
        },
    );
    debug_assert!(listened.is_ok(), "{listened:?}");
}
