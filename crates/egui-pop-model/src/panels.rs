//! Panels are declared, never hard-coded: a plugin or the shell says what
//! a panel is and where it goes by default; the shell places it, saves
//! where the user moved it, and offers its commands.

use serde::{Deserialize, Serialize};

use crate::ids::{CommandId, OwnerId, PanelId};
use crate::registry::Registry;

/// The shell's areas. Start and end, not left and right: ready for
/// right-to-left layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    /// The centre: editors, tabbed and splittable.
    Center,
    /// The side bar at the start (left in left-to-right).
    StartSide,
    /// The side bar at the end.
    EndSide,
    Bottom,
}

impl Area {
    pub const ALL: [Area; 4] = [Area::Center, Area::StartSide, Area::EndSide, Area::Bottom];

    /// As people call it.
    pub fn name(self) -> &'static str {
        match self {
            Area::Center => "the centre",
            Area::StartSide => "the start side bar",
            Area::EndSide => "the end side bar",
            Area::Bottom => "the bottom panel",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PanelSpec {
    pub id: PanelId,
    pub title: String,
    /// Its default area.
    pub area: Area,
    /// The areas it may be moved to besides `area`; empty: any area.
    pub areas: Vec<Area>,
    /// Command ids the panel contributes (its toggle and focus commands
    /// are added by the shell).
    pub commands: Vec<CommandId>,
}

impl PanelSpec {
    pub fn new(id: PanelId, title: &str, area: Area) -> Self {
        Self {
            id,
            title: title.into(),
            area,
            areas: Vec::new(),
            commands: Vec::new(),
        }
    }

    /// Restricts where it may go (its default area always allowed).
    pub fn only_in(mut self, areas: &[Area]) -> Self {
        self.areas = areas.to_vec();
        self
    }

    /// Whether the panel may live in an area.
    pub fn allows(&self, area: Area) -> bool {
        self.areas.is_empty() || area == self.area || self.areas.contains(&area)
    }
}

/// The declared panels, each owned.
#[derive(Debug, Default)]
pub struct Panels {
    specs: Registry<PanelId, PanelSpec>,
}

impl Panels {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn declare(&mut self, owner: &OwnerId, spec: PanelSpec) -> Result<(), String> {
        let id = spec.id.clone();
        self.specs
            .insert(owner, id, spec)
            .map_err(|t| format!("panel `{}` is already declared by `{}`", t.id, t.by))
    }

    pub fn get(&self, id: &str) -> Option<&PanelSpec> {
        PanelId::new(id).ok().and_then(|id| self.specs.get(&id))
    }

    /// Every panel, in declaration order.
    pub fn all(&self) -> Vec<&PanelSpec> {
        self.specs
            .in_order()
            .into_iter()
            .map(|(_, s)| &s.item)
            .collect()
    }

    pub fn remove_owner(&mut self, owner: &OwnerId) -> Vec<PanelId> {
        self.specs
            .remove_owner(owner)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// The commands the shell adds for each panel: focus and toggle, with
    /// their titles.
    pub fn shell_commands(&self) -> Vec<(CommandId, String)> {
        self.all()
            .into_iter()
            .flat_map(|p| {
                let focus = p.id.as_str().to_string() + ".focus";
                let toggle = p.id.as_str().to_string() + ".toggle";
                [
                    (CommandId::new(focus), format!("Focus {}", p.title)),
                    (CommandId::new(toggle), format!("Show or hide {}", p.title)),
                ]
            })
            .filter_map(|(id, t)| id.ok().map(|id| (id, t)))
            .collect()
    }
}
