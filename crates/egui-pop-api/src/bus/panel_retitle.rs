//! A panel instance retitled on the bus (ADR 0029): `pop.panel.retitle`
//! names an instance's tab title, whoever asks (the application's code
//! through [`AppHandle::set_instance_title`], an agent, a test).
//!
//! The ask names an open instance `<kind>#<n>` of a panel declared with
//! several (`PanelSpec::instances`) and its title; `title: null` gives the
//! generated one back (the kind's title and the number, "Gallery 2"). The
//! workbench's listener ([`PANEL_RETITLE`]) keeps the title in the
//! layout's instance entry (`Layout::instances`), so it is saved and
//! restored with the layout; the tab, the panel menu's list of instances,
//! the View menu and accessibility read it, and so does the
//! [`Instance`](crate::workbench::instances::Instance) handed to the draw
//! function. The journal says `panel.instance.retitled`. Anything else
//! (the panel declared, a panel declared once, an instance not open) is
//! refused with the reason.
//!
//! [`AppHandle::set_instance_title`]: crate::AppHandle::set_instance_title

use bus_contract::{Bus, ListenOpts, Point, PointId, PointKind};
use egui_pop_model::ids::PanelId;
use serde::{Deserialize, Serialize};

use super::{OWNER, asking};

/// A panel instance asked retitled: which instance, its title (`None`:
/// the generated one again).
///
/// ```
/// use egui_pop_api::bus::PanelRetitleAsk;
///
/// let ask = PanelRetitleAsk::new("app.gallery#2", Some("Picks".into()));
/// let json = serde_json::to_value(&ask).unwrap();
/// assert_eq!(json["title"], "Picks");
/// let back: PanelRetitleAsk = serde_json::from_value(serde_json::json!({
///     "panel": "app.gallery#2", "title": null
/// })).unwrap();
/// assert_eq!(back.title, None);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PanelRetitleAsk {
    /// The instance's id (`<kind>#<n>`).
    pub panel: String,
    /// Its title; `None` (`null`) gives the generated title back.
    #[serde(default)]
    pub title: Option<String>,
}

impl PanelRetitleAsk {
    pub fn new(panel: impl Into<String>, title: Option<String>) -> Self {
        Self {
            panel: panel.into(),
            title,
        }
    }
}

/// A panel instance is asked retitled (`pop.panel.retitle`); its
/// listeners say what they did, one line each, in the value.
#[derive(Debug)]
pub struct PanelRetitle;
impl Point for PanelRetitle {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.panel.retitle");
    type Input = PanelRetitleAsk;
    type Value = Vec<String>;
}

/// The panel retitling points, in the order [`super::order_dump`] prints
/// them.
pub const PANEL_RETITLE_POINTS: [&str; 1] = [PanelRetitle::ID.as_str()];

/// The workbench's listener of `pop.panel.retitle`: keeps the title in
/// the layout's instance entry.
pub const PANEL_RETITLE: &str = "pop.workbench.panel_retitle";

/// Registers the workbench's listener of `pop.panel.retitle`.
pub(super) fn install(kernel: &dyn Bus) {
    let listened =
        kernel.listen::<PanelRetitle>(PANEL_RETITLE, ListenOpts::new().owner(OWNER), |ask, did| {
            let Some(app) = asking() else {
                return Ok(());
            };
            let panel = PanelId::new(&ask.panel).map_err(|e| e.to_string())?;
            let changed = app.retitle_instance(&panel, ask.title.clone())?;
            did.push(if changed {
                format!("{PANEL_RETITLE}: `{panel}` retitled")
            } else {
                format!("{PANEL_RETITLE}: `{panel}` unchanged")
            });
            Ok(())
        });
    debug_assert!(listened.is_ok(), "{listened:?}");
}
