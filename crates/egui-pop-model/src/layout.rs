//! A workspace's arrangement: which panels sit in which area, in which
//! tabs and splits, which ones were detached into auxiliary windows and on
//! which screen. Saved as the `layout` document, restored at start-up,
//! repaired when it no longer matches the panels or the screens.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::doc::{Schema, same};
use crate::ids::PanelId;
use crate::panels::{Area, Panels};

/// Tabs and splits of one area.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Node {
    /// Panels as tabs; `active` is the shown one.
    Tabs { panels: Vec<PanelId>, active: usize },
    Split {
        vertical: bool,
        ratio: f32,
        a: Box<Node>,
        b: Box<Node>,
    },
}

impl Node {
    /// One tab.
    pub fn tab(panel: PanelId) -> Node {
        Node::Tabs {
            panels: vec![panel],
            active: 0,
        }
    }

    pub fn panels(&self) -> Vec<&PanelId> {
        match self {
            Node::Tabs { panels, .. } => panels.iter().collect(),
            Node::Split { a, b, .. } => {
                let mut v = a.panels();
                v.extend(b.panels());
                v
            }
        }
    }

    /// Makes a panel the shown tab of its tabs; whether it is here.
    fn activate(&mut self, panel: &PanelId) -> bool {
        match self {
            Node::Tabs { panels, active } => match panels.iter().position(|p| p == panel) {
                Some(i) => {
                    *active = i;
                    true
                }
                None => false,
            },
            Node::Split { a, b, .. } => a.activate(panel) || b.activate(panel),
        }
    }

    /// Adds a panel to the first tabs, shown.
    fn push(&mut self, panel: PanelId) {
        match self {
            Node::Tabs { panels, active } => {
                panels.push(panel);
                *active = panels.len() - 1;
            }
            Node::Split { a, .. } => a.push(panel),
        }
    }

    /// Drops panels that are not in `keep`; `None` when nothing is left.
    fn retain(self, keep: &dyn Fn(&PanelId) -> bool) -> Option<Node> {
        match self {
            Node::Tabs { panels, active } => {
                let shown = panels.get(active).cloned();
                let panels: Vec<PanelId> = panels.into_iter().filter(|p| keep(p)).collect();
                if panels.is_empty() {
                    return None;
                }
                let active = shown
                    .and_then(|s| panels.iter().position(|p| *p == s))
                    .unwrap_or(0);
                Some(Node::Tabs { panels, active })
            }
            Node::Split {
                vertical,
                ratio,
                a,
                b,
            } => match (a.retain(keep), b.retain(keep)) {
                (Some(a), Some(b)) => Some(Node::Split {
                    vertical,
                    ratio: ratio.clamp(0.05, 0.95),
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (Some(n), None) | (None, Some(n)) => Some(n),
                (None, None) => None,
            },
        }
    }
}

/// A panel detached into its own window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Auxiliary {
    pub panel: PanelId,
    /// The screen it was on, by name, and its position there.
    pub screen: String,
    pub rect: [f32; 4],
}

/// One panel of a floating group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub panel: PanelId,
    /// Rolled up to its header band (each member of a stack rolls up on
    /// its own).
    #[serde(default, skip_serializing_if = "is_false")]
    pub rolled: bool,
}

/// How the members of a floating group are laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arrangement {
    /// One under the other, in a column.
    #[default]
    Stack,
    /// As tabs, one shown at a time.
    Tabs,
}

impl Arrangement {
    fn is_stack(&self) -> bool {
        *self == Arrangement::Stack
    }

    /// Its name in documents and arguments.
    pub fn as_str(self) -> &'static str {
        match self {
            Arrangement::Stack => "stack",
            Arrangement::Tabs => "tabs",
        }
    }
}

/// Where a palette joins another one's group: above it, below it (a
/// stack), or as a tab of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Join {
    Above,
    Below,
    Tab,
}

impl Join {
    /// Its name in step arguments.
    pub fn as_str(self) -> &'static str {
        match self {
            Join::Above => "above",
            Join::Below => "below",
            Join::Tab => "tab",
        }
    }

    /// A place from its name.
    pub fn parse(name: &str) -> Option<Join> {
        [Join::Above, Join::Below, Join::Tab]
            .into_iter()
            .find(|j| j.as_str() == name)
    }
}

/// Panels floating over the interface as one unit: a palette alone (a
/// group of one), or palettes stacked in a column or held as tabs. The
/// group moves, pins and anchors as one; each member rolls up on its
/// own.
///
/// Saved as the floating panel it was before groups (its first member's
/// `panel` and `rolled`, the group's `pos`, `pinned` and `anchor`), the
/// other members under `group`: an application that predates groups
/// reads the first member and floats the others again as new palettes.
///
/// ```
/// use egui_pop_model::ids::PanelId;
/// use egui_pop_model::layout::{Arrangement, FloatingGroup};
///
/// let tools = PanelId::from_static("app.tools");
/// let g = FloatingGroup::lone(tools.clone(), [16.0, 16.0]);
/// assert_eq!(g.panels().collect::<Vec<_>>(), [&tools]);
/// assert_eq!(g.arrangement, Arrangement::Stack);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "GroupDoc", into = "GroupDoc")]
pub struct FloatingGroup {
    /// Its panels, in order (never empty): top to bottom in a stack,
    /// start to end as tabs.
    pub members: Vec<Member>,
    pub arrangement: Arrangement,
    /// The shown member of tabs.
    pub active: usize,
    /// Its top-left corner, in points from the workbench's top-left.
    pub pos: [f32; 2],
    /// Pinned: it stays where it is and in sight (not dragged, not
    /// hidden with the others, not faded).
    pub pinned: bool,
    /// The workbench edges it sticks to: set when it is dropped on them,
    /// kept as the window is resized or the layout restored.
    pub anchor: Anchor,
}

/// A floating group as saved (see [`FloatingGroup`]).
#[derive(Clone, Serialize, Deserialize)]
struct GroupDoc {
    panel: PanelId,
    pos: [f32; 2],
    #[serde(default, skip_serializing_if = "is_false")]
    rolled: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pinned: bool,
    #[serde(default, skip_serializing_if = "Anchor::is_none")]
    anchor: Anchor,
    /// The other members, after the first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    group: Vec<Member>,
    #[serde(default, skip_serializing_if = "Arrangement::is_stack")]
    arrangement: Arrangement,
    #[serde(default, skip_serializing_if = "is_zero")]
    active: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

impl From<GroupDoc> for FloatingGroup {
    fn from(d: GroupDoc) -> Self {
        let mut members = vec![Member {
            panel: d.panel,
            rolled: d.rolled,
        }];
        members.extend(d.group);
        let mut g = FloatingGroup {
            members,
            arrangement: d.arrangement,
            active: d.active,
            pos: d.pos,
            pinned: d.pinned,
            anchor: d.anchor,
        };
        g.tidy();
        g
    }
}

impl From<FloatingGroup> for GroupDoc {
    fn from(g: FloatingGroup) -> Self {
        let mut members = g.members.into_iter();
        let first = members.next().expect("a floating group has a member");
        GroupDoc {
            panel: first.panel,
            pos: g.pos,
            rolled: first.rolled,
            pinned: g.pinned,
            anchor: g.anchor,
            group: members.collect(),
            arrangement: g.arrangement,
            active: g.active,
        }
    }
}

impl FloatingGroup {
    /// A palette floating alone at `pos`.
    pub fn lone(panel: PanelId, pos: [f32; 2]) -> FloatingGroup {
        FloatingGroup {
            members: vec![Member {
                panel,
                rolled: false,
            }],
            arrangement: Arrangement::Stack,
            active: 0,
            pos,
            pinned: false,
            anchor: Anchor::default(),
        }
    }

    /// Where the `n`th palette opens when it has no place yet: down and
    /// right from the workbench's top-left, one step each.
    pub fn cascade(n: usize) -> [f32; 2] {
        let at = 16.0 + 28.0 * (n % 8) as f32;
        [at, at]
    }

    /// Its panels, in order.
    pub fn panels(&self) -> impl Iterator<Item = &PanelId> {
        self.members.iter().map(|m| &m.panel)
    }

    /// Whether a panel is one of its members.
    pub fn holds(&self, panel: &PanelId) -> bool {
        self.members.iter().any(|m| m.panel == *panel)
    }

    /// A member by its panel.
    pub fn member(&self, panel: &PanelId) -> Option<&Member> {
        self.members.iter().find(|m| m.panel == *panel)
    }

    /// A member by its panel, to change it.
    pub fn member_mut(&mut self, panel: &PanelId) -> Option<&mut Member> {
        self.members.iter_mut().find(|m| m.panel == *panel)
    }

    /// Whether a member is rolled up (`false` for a panel not in it).
    pub fn rolled(&self, panel: &PanelId) -> bool {
        self.member(panel).is_some_and(|m| m.rolled)
    }

    /// Its first member: the one that names it in steps.
    pub fn lead(&self) -> &PanelId {
        &self.members[0].panel
    }

    /// The member shown: the active tab, or the first of a stack.
    pub fn shown(&self) -> &PanelId {
        let i = match self.arrangement {
            Arrangement::Tabs => self.active.min(self.members.len() - 1),
            Arrangement::Stack => 0,
        };
        &self.members[i].panel
    }

    /// A palette alone, not grouped with others.
    pub fn is_lone(&self) -> bool {
        self.members.len() == 1
    }

    /// Keeps its fields consistent: the active tab inside, and a palette
    /// alone a plain stack.
    fn tidy(&mut self) {
        if self.members.len() <= 1 {
            self.arrangement = Arrangement::Stack;
            self.active = 0;
        } else if self.arrangement == Arrangement::Stack {
            self.active = 0;
        } else {
            self.active = self.active.min(self.members.len() - 1);
        }
    }

    /// Takes a member out, if it is in; the shown tab stays shown when
    /// it is another one.
    fn take(&mut self, panel: &PanelId) -> Option<Member> {
        let i = self.members.iter().position(|m| m.panel == *panel)?;
        let m = self.members.remove(i);
        // The shown tab out: the one before it is shown.
        if i < self.active || (i == self.active && i > 0) {
            self.active -= 1;
        }
        self.tidy();
        Some(m)
    }
}

/// Which workbench edge a floating panel sticks to, on one axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorEdge {
    /// None: it keeps its place from the workbench's top-left, clamped
    /// inside.
    #[default]
    None,
    /// The left edge (x) or the top edge (y).
    Start,
    /// The right edge (x) or the bottom edge (y).
    End,
}

impl AnchorEdge {
    fn is_none(&self) -> bool {
        *self == AnchorEdge::None
    }
}

/// The workbench edges a floating panel sticks to, horizontally and
/// vertically, and its distance to each: when the workbench changes size,
/// an anchored edge keeps that distance.
///
/// ```
/// use egui_pop_model::layout::{Anchor, AnchorEdge};
///
/// let right = Anchor { x: AnchorEdge::End, ..Anchor::default() };
/// assert!(!right.is_none());
/// assert!(Anchor::default().is_none());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    #[serde(default, skip_serializing_if = "AnchorEdge::is_none")]
    pub x: AnchorEdge,
    #[serde(default, skip_serializing_if = "AnchorEdge::is_none")]
    pub y: AnchorEdge,
    /// The distance to the anchored edge on each axis, in points (zero on
    /// an axis that is not anchored).
    #[serde(default)]
    pub offset: [f32; 2],
}

impl Anchor {
    /// Anchored on neither axis.
    pub fn is_none(&self) -> bool {
        self.x.is_none() && self.y.is_none()
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Where a side area lies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Placement {
    /// Beside the centre, taking its room (the default).
    #[default]
    Beside,
    /// Over the centre, with a shadow; its folded strip keeps the room in
    /// the layout, so the centre keeps its width.
    Over,
}

impl Placement {
    /// Its name in documents and arguments.
    pub fn as_str(self) -> &'static str {
        match self {
            Placement::Beside => "beside",
            Placement::Over => "over",
        }
    }

    /// A placement from its name.
    pub fn parse(name: &str) -> Option<Placement> {
        [Placement::Beside, Placement::Over]
            .into_iter()
            .find(|p| p.as_str() == name)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub areas: Vec<(Area, Node)>,
    #[serde(default)]
    pub auxiliary: Vec<Auxiliary>,
    /// Panels the user closed: not put back as new ones are.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<PanelId>,
    /// Areas folded to their strip: their panels stay, unshown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folded: Vec<Area>,
    /// Side areas lying over the centre ([`Placement::Over`]): their
    /// folded strip keeps their room, the centre its width.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub over: Vec<Area>,
    /// Panels floating over the interface (palettes), each group as one
    /// unit (a palette alone is a group of one).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub floating: Vec<FloatingGroup>,
    /// Groups docked whole in an area (their members there as tabs),
    /// kept so that floating one of them out again gives the group back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub docked_groups: Vec<FloatingGroup>,
}

/// The `layout` document. Version 2 named the areas by start and end
/// (`start_side`) instead of primary and secondary.
pub fn schema() -> Schema {
    Schema::new("layout", 2)
        .migration(0, same)
        .migration(1, rename_areas)
}

fn rename_areas(mut json: Json) -> Result<Json, String> {
    let rename = |name: &str| {
        match name {
            "editors" => "center",
            "primary_side_bar" => "start_side",
            "secondary_side_bar" => "end_side",
            "bottom_panel" => "bottom",
            other => other,
        }
        .to_string()
    };
    if let Some(areas) = json.get_mut("areas").and_then(Json::as_array_mut) {
        for pair in areas {
            if let Some(name) = pair.get(0).and_then(Json::as_str).map(rename) {
                pair[0] = Json::String(name);
            }
        }
    }
    Ok(json)
}

impl Layout {
    /// Every declared panel in its default area, as tabs.
    pub fn default_for(panels: &Panels) -> Layout {
        let mut areas: Vec<(Area, Node)> = Vec::new();
        let mut hidden = Vec::new();
        let mut floating = Vec::new();
        for p in panels.all() {
            if !p.open {
                hidden.push(p.id.clone());
                continue;
            }
            if p.palette {
                floating.push(FloatingGroup::lone(
                    p.id.clone(),
                    FloatingGroup::cascade(floating.len()),
                ));
                continue;
            }
            match areas.iter_mut().find(|(a, _)| *a == p.area) {
                Some((_, Node::Tabs { panels, .. })) => panels.push(p.id.clone()),
                _ => areas.push((p.area, Node::tab(p.id.clone()))),
            }
        }
        areas.sort_by_key(|(a, _)| *a);
        Layout {
            areas,
            auxiliary: Vec::new(),
            hidden,
            folded: Vec::new(),
            over: Vec::new(),
            floating,
            docked_groups: Vec::new(),
        }
    }

    /// Where a side area lies (the centre and the bottom area always
    /// lie beside).
    pub fn placement(&self, area: Area) -> Placement {
        if self.over.contains(&area) {
            Placement::Over
        } else {
            Placement::Beside
        }
    }

    /// Lays a side area over the centre or beside it.
    pub fn set_placement(&mut self, area: Area, placement: Placement) {
        self.over.retain(|a| *a != area);
        if placement == Placement::Over {
            self.over.push(area);
        }
    }

    /// Every panel placed, docked, detached or floating.
    pub fn placed(&self) -> Vec<&PanelId> {
        let mut v: Vec<&PanelId> = self.areas.iter().flat_map(|(_, n)| n.panels()).collect();
        v.extend(self.auxiliary.iter().map(|a| &a.panel));
        v.extend(self.floating.iter().flat_map(|g| g.panels()));
        v
    }

    /// The group a floating panel floats in, if it floats.
    pub fn floating_of(&self, panel: &PanelId) -> Option<&FloatingGroup> {
        self.floating.iter().find(|g| g.holds(panel))
    }

    /// The group a floating panel floats in, to change it.
    pub fn floating_mut(&mut self, panel: &PanelId) -> Option<&mut FloatingGroup> {
        self.floating.iter_mut().find(|g| g.holds(panel))
    }

    /// Floats a panel over the interface at `pos` (taken out of where it
    /// was, opened if it was closed); a panel floating already moves
    /// there with its group, keeping its state. A panel docked with the
    /// group it was docked in whole ([`Layout::dock_group`]) floats with
    /// the members still docked beside it: the group comes back.
    pub fn float(&mut self, panel: &PanelId, pos: [f32; 2]) {
        if let Some(g) = self.floating_mut(panel) {
            g.pos = pos;
            return;
        }
        let area = self.area_of(panel);
        let mut group = FloatingGroup::lone(panel.clone(), pos);
        if let Some(i) = self.docked_groups.iter().position(|g| g.holds(panel)) {
            let mut was = self.docked_groups.remove(i);
            was.members.retain(|m| {
                m.panel == *panel || (area.is_some() && self.area_of(&m.panel) == area)
            });
            if was.members.len() > 1 {
                if was.arrangement == Arrangement::Tabs {
                    was.active = was
                        .members
                        .iter()
                        .position(|m| m.panel == *panel)
                        .unwrap_or(0);
                }
                was.pos = pos;
                was.anchor = Anchor::default();
                group = was;
                group.tidy();
            }
        }
        for p in group.panels() {
            self.remove(p);
            self.hidden.retain(|h| h != p);
        }
        self.floating.push(group);
    }

    /// Moves floating panels into the group of another floating panel,
    /// `into`: above it or below it (the group is then a stack), or as
    /// tabs (the group is then tabs, the first of them shown). Each keeps
    /// whether it is rolled up; groups left empty go.
    pub fn group(&mut self, panels: &[PanelId], into: &PanelId, at: Join) -> Result<(), String> {
        if panels.is_empty() {
            return Err("no panel to group".into());
        }
        if panels.contains(into) {
            return Err(format!("`{into}` cannot join its own group"));
        }
        for p in panels {
            if self.floating_of(p).is_none() {
                return Err(format!("`{p}` does not float"));
            }
        }
        if self.floating_of(into).is_none() {
            return Err(format!("`{into}` does not float"));
        }
        let mut moving = Vec::new();
        for p in panels {
            if let Some(g) = self.floating_mut(p)
                && let Some(m) = g.take(p)
            {
                moving.push(m);
            }
        }
        self.floating.retain(|g| !g.members.is_empty());
        let g = self.floating_mut(into).expect("checked above");
        let i = g.members.iter().position(|m| m.panel == *into).unwrap_or(0);
        let first = match at {
            Join::Above => i,
            Join::Below | Join::Tab => i + 1,
        };
        for (k, m) in moving.into_iter().enumerate() {
            g.members.insert(first + k, m);
        }
        match at {
            Join::Above | Join::Below => g.arrangement = Arrangement::Stack,
            Join::Tab => {
                g.arrangement = Arrangement::Tabs;
                g.active = first;
            }
        }
        g.tidy();
        Ok(())
    }

    /// Takes a floating panel out of its group, alone at `pos` (it keeps
    /// whether it is rolled up).
    pub fn ungroup(&mut self, panel: &PanelId, pos: [f32; 2]) -> Result<(), String> {
        let g = self
            .floating_mut(panel)
            .ok_or_else(|| format!("`{panel}` does not float"))?;
        if g.is_lone() {
            return Err(format!("`{panel}` is not in a group"));
        }
        let m = g.take(panel).expect("held");
        let mut lone = FloatingGroup::lone(m.panel, pos);
        lone.members[0].rolled = m.rolled;
        self.floating.push(lone);
        Ok(())
    }

    /// Docks every member of a floating panel's group in an area, as
    /// tabs, the group's shown member shown; the group is kept
    /// ([`Layout::docked_groups`]) so that floating one of them out
    /// again gives it back. The panels docked.
    pub fn dock_group(&mut self, panel: &PanelId, area: Area) -> Vec<PanelId> {
        let Some(i) = self.floating.iter().position(|g| g.holds(panel)) else {
            self.place(panel, area);
            return vec![panel.clone()];
        };
        let group = self.floating.remove(i);
        let panels: Vec<PanelId> = group.panels().cloned().collect();
        for p in &panels {
            self.place(p, area);
        }
        self.activate(group.shown());
        self.docked_groups
            .retain(|g| !panels.iter().any(|p| g.holds(p)));
        if !group.is_lone() {
            self.docked_groups.push(group);
        }
        panels
    }

    /// The area a panel is docked in, if it is.
    pub fn area_of(&self, panel: &PanelId) -> Option<Area> {
        self.areas
            .iter()
            .find(|(_, n)| n.panels().contains(&panel))
            .map(|(a, _)| *a)
    }

    /// Whether a panel is docked or detached, and not closed.
    pub fn shows(&self, panel: &PanelId) -> bool {
        !self.hidden.contains(panel) && self.placed().contains(&panel)
    }

    /// Takes a panel out of its area, its window or its float (out of
    /// its group, the others staying together; it stays known: not
    /// closed). Whether it was placed.
    pub fn remove(&mut self, panel: &PanelId) -> bool {
        let before = self.placed().len();
        let areas = std::mem::take(&mut self.areas);
        self.areas = areas
            .into_iter()
            .filter_map(|(a, n)| n.retain(&|p: &PanelId| p != panel).map(|n| (a, n)))
            .collect();
        self.auxiliary.retain(|x| x.panel != *panel);
        for g in &mut self.floating {
            g.take(panel);
        }
        self.floating.retain(|g| !g.members.is_empty());
        self.placed().len() != before
    }

    /// Docks a panel in an area (taken out of where it was), as the
    /// shown tab of the area's first tabs.
    pub fn place(&mut self, panel: &PanelId, area: Area) {
        self.remove(panel);
        self.hidden.retain(|h| h != panel);
        match self.areas.iter_mut().find(|(a, _)| *a == area) {
            Some((_, node)) => node.push(panel.clone()),
            None => {
                self.areas.push((area, Node::tab(panel.clone())));
                self.areas.sort_by_key(|(a, _)| *a);
            }
        }
    }

    /// Closes a panel: out of its area or window, and kept closed.
    pub fn hide(&mut self, panel: &PanelId) {
        self.remove(panel);
        for g in &mut self.docked_groups {
            g.take(panel);
        }
        self.docked_groups.retain(|g| g.members.len() > 1);
        if !self.hidden.contains(panel) {
            self.hidden.push(panel.clone());
        }
    }

    /// Makes a docked panel the shown tab of its tabs; whether it is
    /// docked.
    pub fn activate(&mut self, panel: &PanelId) -> bool {
        self.areas.iter_mut().any(|(_, n)| n.activate(panel))
    }

    /// The shown panels of an area, in order (the shown tab of each
    /// tabs node).
    pub fn active_in(&self, area: Area) -> Vec<&PanelId> {
        fn walk<'a>(n: &'a Node, out: &mut Vec<&'a PanelId>) {
            match n {
                Node::Tabs { panels, active } => {
                    if let Some(p) = panels.get(*active) {
                        out.push(p);
                    }
                }
                Node::Split { a, b, .. } => {
                    walk(a, out);
                    walk(b, out);
                }
            }
        }
        let mut out = Vec::new();
        for (a, n) in &self.areas {
            if *a == area {
                walk(n, &mut out);
            }
        }
        out
    }

    /// Brings a saved layout in line with today's panels and screens:
    /// panels no longer declared are dropped, new ones go to their default
    /// area, so do panels in an area they may no longer be in, a window on
    /// a screen that is gone comes back docked. Says what it changed.
    pub fn repair(self, panels: &Panels, screens: &[&str]) -> (Layout, Vec<String>) {
        let mut notes = Vec::new();
        let known = |id: &PanelId| panels.get(id.as_str()).is_some();
        let mut auxiliary = Vec::new();
        let mut redock = Vec::new();
        for a in self.auxiliary {
            if !known(&a.panel) {
                notes.push(format!("panel `{}` no longer exists", a.panel));
            } else if !screens.contains(&a.screen.as_str()) {
                notes.push(format!(
                    "screen `{}` is gone: `{}` docked back",
                    a.screen, a.panel
                ));
                redock.push(a.panel);
            } else {
                auxiliary.push(a);
            }
        }
        let mut floating = Vec::new();
        for mut g in self.floating {
            let members = std::mem::take(&mut g.members);
            for m in members {
                match panels.get(m.panel.as_str()) {
                    None => notes.push(format!("panel `{}` no longer exists", m.panel)),
                    Some(spec) if !spec.floatable => {
                        notes.push(format!(
                            "`{}` no longer floats: docked in its default area",
                            m.panel
                        ));
                        redock.push(m.panel);
                    }
                    Some(_) => g.members.push(m),
                }
            }
            if !g.members.is_empty() {
                g.tidy();
                floating.push(g);
            }
        }
        let docked_groups: Vec<FloatingGroup> = self
            .docked_groups
            .into_iter()
            .filter_map(|mut g| {
                g.members.retain(|m| known(&m.panel));
                (g.members.len() > 1).then(|| {
                    g.tidy();
                    g
                })
            })
            .collect();
        let mut areas: Vec<(Area, Node)> = Vec::new();
        for (area, node) in self.areas {
            for p in node.panels() {
                match panels.get(p.as_str()) {
                    None => notes.push(format!("panel `{p}` no longer exists")),
                    Some(spec) if !spec.allows(area) => {
                        notes.push(format!(
                            "`{p}` cannot be in {}: moved to its default area",
                            area.name()
                        ));
                        redock.push(p.clone());
                    }
                    Some(_) => {}
                }
            }
            let allowed = |id: &PanelId| panels.get(id.as_str()).is_some_and(|s| s.allows(area));
            if let Some(n) = node.retain(&allowed) {
                areas.push((area, n));
            }
        }
        let hidden: Vec<PanelId> = self.hidden.into_iter().filter(|h| known(h)).collect();
        let mut layout = Layout {
            areas,
            auxiliary,
            hidden: hidden.clone(),
            folded: self.folded,
            over: self
                .over
                .into_iter()
                .filter(|a| matches!(a, Area::StartSide | Area::EndSide))
                .collect(),
            floating,
            docked_groups,
        };
        let placed: Vec<PanelId> = layout.placed().into_iter().cloned().collect();
        for p in panels.all() {
            if placed.contains(&p.id) || hidden.contains(&p.id) {
                continue;
            }
            if !p.open && !redock.contains(&p.id) {
                layout.hidden.push(p.id.clone());
                continue;
            }
            if !redock.contains(&p.id) {
                if p.palette {
                    notes.push(format!("new palette `{}` floats", p.id));
                    let pos = FloatingGroup::cascade(layout.floating.len());
                    layout.float(&p.id, pos);
                    continue;
                }
                notes.push(format!("new panel `{}` placed in its default area", p.id));
            }
            match layout.areas.iter_mut().find(|(a, _)| *a == p.area) {
                Some((_, Node::Tabs { panels, .. })) => panels.push(p.id.clone()),
                Some((_, node)) => {
                    let old = std::mem::replace(node, Node::tab(p.id.clone()));
                    *node = Node::Split {
                        vertical: false,
                        ratio: 0.7,
                        a: Box::new(old),
                        b: Box::new(Node::tab(p.id.clone())),
                    };
                }
                None => layout.areas.push((p.area, Node::tab(p.id.clone()))),
            }
        }
        layout.areas.sort_by_key(|(a, _)| *a);
        (layout, notes)
    }

    /// The document's text.
    pub fn to_json(&self) -> String {
        schema().save(self).expect("layouts serialize")
    }

    /// Reads a layout document of any known version.
    pub fn from_json(text: &str) -> Result<Layout, String> {
        schema()
            .load(text)
            .map(|l| l.value)
            .map_err(|e| format!("layout: {e}"))
    }
}
