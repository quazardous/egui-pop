//! A workspace's arrangement: which panels sit in which area, in which
//! tabs and splits, which ones were detached into auxiliary windows and on
//! which screen. Saved as the `layout` document, restored at start-up,
//! repaired when it no longer matches the panels or the screens.

use std::collections::BTreeMap;

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

/// One member of a floating group: a palette, or palettes held as tabs
/// in one slot of a stack (a tab group nested in the stack, one level
/// only).
///
/// Saved as the palette it was before nesting (its first tab's `panel`),
/// the other tabs under `tabs`: an application that predates nesting
/// reads the first tab and puts the others back as it does any panel
/// missing from a layout (a palette floats again on its own).
///
/// ```
/// use egui_pop_model::ids::PanelId;
/// use egui_pop_model::layout::Member;
///
/// let tools = PanelId::from_static("app.tools");
/// let m = Member::palette(tools.clone());
/// assert!(!m.is_tabs());
/// assert_eq!(m.shown(), &tools);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member {
    /// The palette, or the first of its tabs.
    pub panel: PanelId,
    /// Rolled up to its header band (each member of a stack rolls up on
    /// its own; a tab group rolls up as one).
    #[serde(default, skip_serializing_if = "is_false")]
    pub rolled: bool,
    /// The panels held as tabs with `panel`, after it: empty for a
    /// palette.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<PanelId>,
    /// The shown tab: 0 for `panel`, `n` for `tabs[n - 1]`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub shown: usize,
}

impl Member {
    /// A palette, rolled down.
    pub fn palette(panel: PanelId) -> Member {
        Member {
            panel,
            rolled: false,
            tabs: Vec::new(),
            shown: 0,
        }
    }

    /// Whether it holds tabs (a tab group nested in a stack).
    pub fn is_tabs(&self) -> bool {
        !self.tabs.is_empty()
    }

    /// Its panels, in order.
    pub fn panels(&self) -> impl Iterator<Item = &PanelId> {
        std::iter::once(&self.panel).chain(&self.tabs)
    }

    /// Whether a panel is one of its panels.
    pub fn holds(&self, panel: &PanelId) -> bool {
        self.panels().any(|p| p == panel)
    }

    /// The panel shown: the palette, or its shown tab.
    pub fn shown(&self) -> &PanelId {
        match self.shown.min(self.tabs.len()) {
            0 => &self.panel,
            n => &self.tabs[n - 1],
        }
    }

    /// Shows one of its tabs; whether it holds it.
    pub fn show(&mut self, panel: &PanelId) -> bool {
        let at = self.panels().position(|p| p == panel);
        if let Some(i) = at {
            self.shown = i;
        }
        at.is_some()
    }

    /// Its panels as a list, and back.
    fn list(&self) -> Vec<PanelId> {
        self.panels().cloned().collect()
    }

    fn from_list(mut panels: Vec<PanelId>, shown: usize, rolled: bool) -> Member {
        let panel = panels.remove(0);
        Member {
            panel,
            rolled,
            shown: shown.min(panels.len()),
            tabs: panels,
        }
    }

    /// Takes one of its tabs out (not its only panel); the shown tab out,
    /// its neighbour on the right is shown, or on the left at the end.
    fn take_tab(&mut self, panel: &PanelId) -> bool {
        let mut list = self.list();
        let Some(i) = list.iter().position(|p| p == panel) else {
            return false;
        };
        if list.len() == 1 {
            return false;
        }
        list.remove(i);
        let shown = shown_after_removal(self.shown, i, list.len());
        *self = Member::from_list(list, shown, self.rolled);
        true
    }

    /// Its panels as palettes of their own (a tab group taken apart).
    fn split(self) -> Vec<Member> {
        let rolled = self.rolled;
        self.panels()
            .cloned()
            .map(|p| Member {
                rolled,
                ..Member::palette(p)
            })
            .collect()
    }
}

/// The shown index once the item at `removed` is out of a list now `len`
/// long: an earlier one shifts it left; the shown one out, its right
/// neighbour (now at its index) shows, or the left one at the end.
fn shown_after_removal(shown: usize, removed: usize, len: usize) -> usize {
    let shown = if removed < shown { shown - 1 } else { shown };
    shown.min(len.saturating_sub(1))
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
/// group of one), or palettes stacked in a column or held as tabs. A
/// stack's member may itself be a tab group ([`Member::tabs`]): one level
/// of nesting, a tab group never holding a stack. The group moves, pins,
/// anchors and is resized as one; each member rolls up on its own.
///
/// Saved as the floating panel it was before groups (its first member's
/// `panel` and `rolled`, the group's `pos`, `pinned` and `anchor`), the
/// other members under `group`: an application that predates groups
/// reads the first member and floats the others again as new palettes.
/// A nested tab group is saved as its first tab with the others under
/// `tabs` (see [`Member`]).
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
    /// Its members, in order (never empty): top to bottom in a stack,
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
    /// Its size, in points, once the user resized it (its frame, bands
    /// included); `None`: sized by its content.
    pub size: Option<[f32; 2]>,
}

/// A floating group as saved (see [`FloatingGroup`]).
#[derive(Clone, Serialize, Deserialize)]
struct GroupDoc {
    panel: PanelId,
    pos: [f32; 2],
    #[serde(default, skip_serializing_if = "is_false")]
    rolled: bool,
    /// The first member's other tabs, when it is a nested tab group.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tabs: Vec<PanelId>,
    /// The first member's shown tab.
    #[serde(default, skip_serializing_if = "is_zero")]
    shown: usize,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size: Option<[f32; 2]>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

impl From<GroupDoc> for FloatingGroup {
    fn from(d: GroupDoc) -> Self {
        let mut members = vec![Member {
            panel: d.panel,
            rolled: d.rolled,
            tabs: d.tabs,
            shown: d.shown,
        }];
        members.extend(d.group);
        let mut g = FloatingGroup {
            members,
            arrangement: d.arrangement,
            active: d.active,
            pos: d.pos,
            pinned: d.pinned,
            anchor: d.anchor,
            size: d.size,
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
            tabs: first.tabs,
            shown: first.shown,
            pinned: g.pinned,
            anchor: g.anchor,
            group: members.collect(),
            arrangement: g.arrangement,
            active: g.active,
            size: g.size,
        }
    }
}

impl FloatingGroup {
    /// A palette floating alone at `pos`.
    pub fn lone(panel: PanelId, pos: [f32; 2]) -> FloatingGroup {
        FloatingGroup {
            members: vec![Member::palette(panel)],
            arrangement: Arrangement::Stack,
            active: 0,
            pos,
            pinned: false,
            anchor: Anchor::default(),
            size: None,
        }
    }

    /// Where the `n`th palette opens when it has no place yet: down and
    /// right from the workbench's top-left, one step each.
    pub fn cascade(n: usize) -> [f32; 2] {
        let at = 16.0 + 28.0 * (n % 8) as f32;
        [at, at]
    }

    /// Its panels, in order (a nested tab group's tabs in their order).
    pub fn panels(&self) -> impl Iterator<Item = &PanelId> {
        self.members.iter().flat_map(Member::panels)
    }

    /// Whether a panel is one of its panels.
    pub fn holds(&self, panel: &PanelId) -> bool {
        self.members.iter().any(|m| m.holds(panel))
    }

    /// The member holding a panel (a nested tab group for any of its
    /// tabs).
    pub fn member(&self, panel: &PanelId) -> Option<&Member> {
        self.members.iter().find(|m| m.holds(panel))
    }

    /// The member holding a panel, to change it.
    pub fn member_mut(&mut self, panel: &PanelId) -> Option<&mut Member> {
        self.members.iter_mut().find(|m| m.holds(panel))
    }

    /// Whether the member holding a panel is rolled up (`false` for a
    /// panel not in it).
    pub fn rolled(&self, panel: &PanelId) -> bool {
        self.member(panel).is_some_and(|m| m.rolled)
    }

    /// Its first member's first panel: the one that names it in steps.
    pub fn lead(&self) -> &PanelId {
        &self.members[0].panel
    }

    /// The member shown: the active tab, or the first of a stack (its
    /// shown tab, for a nested tab group).
    pub fn shown(&self) -> &PanelId {
        let i = match self.arrangement {
            Arrangement::Tabs => self.active.min(self.members.len() - 1),
            Arrangement::Stack => 0,
        };
        self.members[i].shown()
    }

    /// Shows a panel: the tab it is in a group of tabs, or its tab in a
    /// nested tab group; whether it is one of its panels.
    pub fn show(&mut self, panel: &PanelId) -> bool {
        let Some(i) = self.members.iter().position(|m| m.holds(panel)) else {
            return false;
        };
        if self.arrangement == Arrangement::Tabs {
            self.active = i;
        }
        self.members[i].show(panel)
    }

    /// A palette alone, not grouped with others.
    pub fn is_lone(&self) -> bool {
        self.members.len() == 1 && !self.members[0].is_tabs()
    }

    /// Keeps its fields consistent: nested tab groups only in a stack, a
    /// stack of one tab group made a group of tabs, the active tab
    /// inside, and a palette alone a plain stack.
    fn tidy(&mut self) {
        if self.arrangement == Arrangement::Tabs && self.members.iter().any(Member::is_tabs) {
            // Tabs hold palettes: a nested tab group's tabs join them.
            let shown = self.shown().clone();
            let members = std::mem::take(&mut self.members);
            self.members = members.into_iter().flat_map(Member::split).collect();
            self.active = self
                .members
                .iter()
                .position(|m| m.panel == shown)
                .unwrap_or(0);
        }
        if self.members.len() == 1 && self.members[0].is_tabs() {
            // A stack of one tab group: the group's tabs.
            let m = self.members.remove(0);
            self.active = m.shown.min(m.tabs.len());
            self.members = m.split();
            self.arrangement = Arrangement::Tabs;
        }
        for m in &mut self.members {
            m.shown = m.shown.min(m.tabs.len());
        }
        if self.members.len() <= 1 {
            self.arrangement = Arrangement::Stack;
            self.active = 0;
        } else if self.arrangement == Arrangement::Stack {
            self.active = 0;
        } else {
            self.active = self.active.min(self.members.len() - 1);
        }
    }

    /// Takes a panel out, if it is in: out of its nested tab group, or a
    /// member out. The shown tab out, its neighbour on the right is
    /// shown, or the one on the left when it was the last.
    fn take(&mut self, panel: &PanelId) -> Option<Member> {
        let i = self.members.iter().position(|m| m.holds(panel))?;
        let out = if self.members[i].take_tab(panel) {
            Member {
                rolled: self.members[i].rolled,
                ..Member::palette(panel.clone())
            }
        } else {
            let m = self.members.remove(i);
            self.active = shown_after_removal(self.active, i, self.members.len());
            m
        };
        self.tidy();
        Some(out)
    }

    /// Takes out the member holding a panel, whole (a nested tab group
    /// with its tabs).
    fn take_member(&mut self, panel: &PanelId) -> Option<Member> {
        let i = self.members.iter().position(|m| m.holds(panel))?;
        let m = self.members.remove(i);
        self.active = shown_after_removal(self.active, i, self.members.len());
        self.tidy();
        Some(m)
    }

    /// Keeps only the panels `keep` says, in their members; members left
    /// empty go.
    fn retain_panels(&mut self, keep: impl Fn(&PanelId) -> bool) {
        let was = self.shown().clone();
        let members = std::mem::take(&mut self.members);
        for m in members {
            let shown = m.shown().clone();
            let list: Vec<PanelId> = m.panels().filter(|p| keep(p)).cloned().collect();
            if list.is_empty() {
                continue;
            }
            let at = list.iter().position(|p| *p == shown).unwrap_or(0);
            self.members.push(Member::from_list(list, at, m.rolled));
        }
        if self.members.is_empty() {
            return;
        }
        self.active = self.members.iter().position(|m| m.holds(&was)).unwrap_or(0);
        self.tidy();
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
    /// The width each panel was given as a flyout over the centre, out
    /// of its folded side area's strip, in points.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub flyouts: BTreeMap<PanelId, f32>,
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
            flyouts: BTreeMap::new(),
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
            was.retain_panels(|p| p == panel || (area.is_some() && self.area_of(p) == area));
            if was.panels().count() > 1 {
                was.show(panel);
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
    /// `into`, and says where:
    ///
    /// - [`Join::Above`] or [`Join::Below`] it: the group is then a stack.
    ///   Beside a group of tabs, the tabs stay tabs, as one member of the
    ///   stack (a nested tab group).
    /// - [`Join::Tab`]: as tabs of it, the first of them shown. In a group
    ///   of tabs or with a palette alone, the group turns to tabs; on a
    ///   member of a stack, that member alone becomes a tab group.
    ///
    /// A nested tab group whose tabs all move keeps them, unless it joins
    /// as tabs; each moved member keeps whether it is rolled up; groups
    /// left empty go.
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
        let mut moving: Vec<Member> = Vec::new();
        // A whole group of tabs moving: one tab group, its tabs kept.
        while let Some(i) = self.floating.iter().position(|g| {
            g.arrangement == Arrangement::Tabs
                && !g.holds(into)
                && g.panels().all(|p| panels.contains(p))
        }) {
            let g = self.floating.remove(i);
            let rolled = g.members.get(g.active).is_some_and(|m| m.rolled);
            let list = g.panels().cloned().collect();
            moving.push(Member::from_list(list, g.active, rolled));
        }
        for p in panels {
            if moving.iter().any(|m| m.holds(p)) {
                continue;
            }
            let Some(g) = self.floating_mut(p) else {
                continue;
            };
            let whole = g
                .member(p)
                .is_some_and(|m| m.is_tabs() && m.panels().all(|q| panels.contains(q)));
            let m = if whole { g.take_member(p) } else { g.take(p) };
            moving.extend(m);
        }
        self.floating.retain(|g| !g.members.is_empty());
        let g = self.floating_mut(into).expect("checked above");
        let mut i = g.members.iter().position(|m| m.holds(into)).unwrap_or(0);
        match at {
            Join::Above | Join::Below => {
                if g.arrangement == Arrangement::Tabs {
                    // Beside tabs: they stay tabs, one member of the stack.
                    let rolled = g.members.get(g.active).is_some_and(|m| m.rolled);
                    let list = g.members.iter().map(|m| m.panel.clone()).collect();
                    g.members = vec![Member::from_list(list, g.active, rolled)];
                    g.arrangement = Arrangement::Stack;
                    g.active = 0;
                    i = 0;
                }
                let first = if at == Join::Above { i } else { i + 1 };
                for (k, m) in moving.into_iter().enumerate() {
                    g.members.insert(first + k, m);
                }
            }
            Join::Tab if g.arrangement == Arrangement::Stack && g.members.len() > 1 => {
                // On a member of a stack: that member holds the tabs.
                let m = &mut g.members[i];
                let mut list = m.list();
                let at = list.iter().position(|p| p == into).unwrap_or(0) + 1;
                let new: Vec<PanelId> = moving.iter().flat_map(|m| m.list()).collect();
                list.splice(at..at, new);
                *m = Member::from_list(list, at, m.rolled);
            }
            Join::Tab => {
                let first = i + 1;
                let flat: Vec<Member> = moving.into_iter().flat_map(Member::split).collect();
                for (k, m) in flat.into_iter().enumerate() {
                    g.members.insert(first + k, m);
                }
                g.arrangement = Arrangement::Tabs;
                g.active = first;
            }
        }
        g.tidy();
        Ok(())
    }

    /// Takes a floating panel out of its group, alone at `pos` (it keeps
    /// whether it is rolled up); out of a nested tab group, the group's
    /// other tabs stay.
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

    /// Takes the tab group a floating panel is a tab of, nested in a
    /// stack, out of the stack whole: a group of tabs at `pos`. A palette
    /// that is not in a nested tab group comes out alone, as
    /// [`Layout::ungroup`] takes it.
    pub fn ungroup_tabs(&mut self, panel: &PanelId, pos: [f32; 2]) -> Result<(), String> {
        let g = self
            .floating_mut(panel)
            .ok_or_else(|| format!("`{panel}` does not float"))?;
        if !g.member(panel).is_some_and(Member::is_tabs) || g.members.len() < 2 {
            return self.ungroup(panel, pos);
        }
        let m = g.take_member(panel).expect("held");
        let mut tabs = FloatingGroup::lone(m.panel.clone(), pos);
        tabs.members = vec![m];
        tabs.tidy();
        self.floating.push(tabs);
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
        self.docked_groups.retain(|g| g.panels().count() > 1);
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
            let mut keep = Vec::new();
            for p in g.panels() {
                match panels.get(p.as_str()) {
                    None => notes.push(format!("panel `{p}` no longer exists")),
                    Some(spec) if !spec.floatable => {
                        notes.push(format!(
                            "`{p}` no longer floats: docked in its default area"
                        ));
                        redock.push(p.clone());
                    }
                    Some(_) => keep.push(p.clone()),
                }
            }
            g.retain_panels(|p| keep.contains(p));
            if !g.members.is_empty() {
                floating.push(g);
            }
        }
        let docked_groups: Vec<FloatingGroup> = self
            .docked_groups
            .into_iter()
            .filter_map(|mut g| {
                g.retain_panels(|p| known(p));
                (g.panels().count() > 1).then_some(g)
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
            // The centre never folds.
            folded: self
                .folded
                .into_iter()
                .filter(|a| *a != Area::Center)
                .collect(),
            flyouts: self
                .flyouts
                .into_iter()
                .filter(|(p, w)| known(p) && w.is_finite() && *w > 0.0)
                .collect(),
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
