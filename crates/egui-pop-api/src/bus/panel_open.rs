//! A panel opened on the bus (ADR 0029): `pop.panel.open` is the one way
//! to open a panel, whoever asks: a menu, a tab bar's "+", a group's menu,
//! the `<panel>.toggle` command, an agent or a test.
//!
//! The ask names the panel and where it opens ([`OpenPlace`]):
//!
//! - `last`: where it was when it closed (its area among the same tabs, at
//!   its index; its window; its float). That place gone (the tabs it sat
//!   with are gone), it joins the current group, the focused panel's tabs,
//!   when its areas allow it there, else its declared area. A panel
//!   already open is shown where it is.
//! - `here`: in the tab group holding `group` (any panel of it, docked,
//!   floating or detached), else in the focused panel's; moved there when
//!   it is open elsewhere.
//! - an area (`start_side`, …): docked there.
//!
//! The workbench's listener ([`PANEL_OPEN`]) runs the `workbench.show`
//! step, so a plugin may veto it, and the layout's change is said as
//! `pop.layout.changed`. The ask's fields are open to more: a later one
//! names which instance of a panel opens.

use bus_contract::{Bus, ListenOpts, Point, PointId, PointKind};
use egui_pop_model::ids::PanelId;
use egui_pop_model::panels::Area;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{OWNER, asking};
use crate::steps::Outcome;
use crate::workbench::{Change, area_from, area_name};

/// Where a panel opens ([`PanelOpenAsk::place`]). Written `"last"`,
/// `"here"` or an area's name (`"start_side"`, `"end_side"`, `"bottom"`,
/// `"center"`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OpenPlace {
    /// Where it was when it closed; that place gone, the current group
    /// when its areas allow it, else its declared area.
    #[default]
    Last,
    /// In the tab group holding [`PanelOpenAsk::group`], else the focused
    /// panel's.
    Here,
    /// Docked in an area.
    Area(Area),
}

impl OpenPlace {
    /// Its name in an ask or a step's arguments.
    pub fn as_str(self) -> &'static str {
        match self {
            OpenPlace::Last => "last",
            OpenPlace::Here => "here",
            OpenPlace::Area(a) => area_name(a),
        }
    }

    /// A place from its name.
    pub fn parse(name: &str) -> Option<OpenPlace> {
        match name {
            "last" => Some(OpenPlace::Last),
            "here" => Some(OpenPlace::Here),
            other => area_from(other).map(OpenPlace::Area),
        }
    }
}

impl Serialize for OpenPlace {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for OpenPlace {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let name = String::deserialize(d)?;
        OpenPlace::parse(&name).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "no place `{name}` (`last`, `here` or an area's name)"
            ))
        })
    }
}

/// A panel asked open: which panel, where, and who asks.
///
/// ```
/// use egui_pop_api::bus::{OpenPlace, PanelOpenAsk};
///
/// let ask = PanelOpenAsk::new("app.outline", OpenPlace::Here, "app").group("app.inspector");
/// let json = serde_json::to_value(&ask).unwrap();
/// assert_eq!(json["place"], "here");
/// assert_eq!(json["group"], "app.inspector");
/// ```
///
/// Need (layout): open a closed panel again where it was, or in a given tab group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PanelOpenAsk {
    /// The panel's id.
    pub panel: String,
    /// Where it opens; `last` when not given.
    #[serde(default)]
    pub place: OpenPlace,
    /// For [`OpenPlace::Here`]: a panel of the tab group it joins; the
    /// focused panel's group when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Who asks: a plugin's id, a menu's, an agent's.
    pub owner: String,
}

impl PanelOpenAsk {
    pub fn new(panel: impl Into<String>, place: OpenPlace, owner: impl Into<String>) -> Self {
        Self {
            panel: panel.into(),
            place,
            group: None,
            owner: owner.into(),
        }
    }

    /// The tab group it joins, by one of its panels ([`OpenPlace::Here`]).
    pub fn group(mut self, panel: impl Into<String>) -> Self {
        self.group = Some(panel.into());
        self
    }
}

/// A panel is asked open (`pop.panel.open`); its listeners say what they
/// did, one line each, in the value.
#[derive(Debug)]
pub struct PanelOpen;
impl Point for PanelOpen {
    const KIND: PointKind = PointKind::Request;
    const ID: PointId = PointId::new("pop.panel.open");
    type Input = PanelOpenAsk;
    type Value = Vec<String>;
}

/// The panel opening points, in the order [`super::order_dump`] prints
/// them.
pub const PANEL_OPEN_POINTS: [&str; 1] = [PanelOpen::ID.as_str()];

/// The workbench's listener of `pop.panel.open`: opens the panel through
/// the `workbench.show` step.
pub const PANEL_OPEN: &str = "pop.workbench.panel_open";

/// Registers the workbench's listener of `pop.panel.open`.
pub(super) fn install(kernel: &dyn Bus) {
    let listened =
        kernel.listen::<PanelOpen>(PANEL_OPEN, ListenOpts::new().owner(OWNER), |ask, did| {
            let Some(app) = asking() else {
                return Ok(());
            };
            let panel = PanelId::new(&ask.panel).map_err(|e| e.to_string())?;
            let group = match &ask.group {
                Some(g) => Some(PanelId::new(g).map_err(|e| e.to_string())?),
                None => None,
            };
            let change = match (ask.place, group) {
                (OpenPlace::Last, _) => Change::Show(panel.clone()),
                (place, group) => Change::Open(panel.clone(), place, group),
            };
            match app.change_workbench(change) {
                Some(Outcome::Done(Ok(_))) => {
                    did.push(format!("{PANEL_OPEN}: `{panel}` opened"));
                    Ok(())
                }
                Some(Outcome::Done(Err(e))) => Err(e.into()),
                Some(Outcome::Vetoed { by, reason }) => {
                    Err(format!("stopped by `{by}`: {reason}").into())
                }
                None => {
                    did.push(format!("{PANEL_OPEN}: `{panel}` queued"));
                    Ok(())
                }
            }
        });
    debug_assert!(listened.is_ok(), "{listened:?}");
}
