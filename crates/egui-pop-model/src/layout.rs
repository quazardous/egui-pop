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

/// A panel floating over the interface: a palette.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Floating {
    pub panel: PanelId,
    /// Its top-left corner, in points from the workbench's top-left.
    pub pos: [f32; 2],
    /// Rolled up to its header band.
    #[serde(default, skip_serializing_if = "is_false")]
    pub rolled: bool,
    /// Pinned: it stays where it is and in sight (not dragged, not
    /// hidden with the others, not faded).
    #[serde(default, skip_serializing_if = "is_false")]
    pub pinned: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Floating {
    /// Where the `n`th palette opens when it has no place yet: down and
    /// right from the workbench's top-left, one step each.
    pub fn cascade(n: usize) -> [f32; 2] {
        let at = 16.0 + 28.0 * (n % 8) as f32;
        [at, at]
    }
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
    /// Panels floating over the interface (palettes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub floating: Vec<Floating>,
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
                floating.push(Floating {
                    panel: p.id.clone(),
                    pos: Floating::cascade(floating.len()),
                    rolled: false,
                    pinned: false,
                });
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
        v.extend(self.floating.iter().map(|f| &f.panel));
        v
    }

    /// A floating panel's place, if it floats.
    pub fn floating_of(&self, panel: &PanelId) -> Option<&Floating> {
        self.floating.iter().find(|f| f.panel == *panel)
    }

    /// A floating panel's place, to change it.
    pub fn floating_mut(&mut self, panel: &PanelId) -> Option<&mut Floating> {
        self.floating.iter_mut().find(|f| f.panel == *panel)
    }

    /// Floats a panel over the interface at `pos` (taken out of where it
    /// was, opened if it was closed); a panel floating already moves
    /// there, keeping its state.
    pub fn float(&mut self, panel: &PanelId, pos: [f32; 2]) {
        if let Some(f) = self.floating_mut(panel) {
            f.pos = pos;
            return;
        }
        self.remove(panel);
        self.hidden.retain(|h| h != panel);
        self.floating.push(Floating {
            panel: panel.clone(),
            pos,
            rolled: false,
            pinned: false,
        });
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

    /// Takes a panel out of its area, its window or its float (it stays
    /// known: not closed). Whether it was placed.
    pub fn remove(&mut self, panel: &PanelId) -> bool {
        let before = self.placed().len();
        let areas = std::mem::take(&mut self.areas);
        self.areas = areas
            .into_iter()
            .filter_map(|(a, n)| n.retain(&|p: &PanelId| p != panel).map(|n| (a, n)))
            .collect();
        self.auxiliary.retain(|x| x.panel != *panel);
        self.floating.retain(|x| x.panel != *panel);
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
        for f in self.floating {
            match panels.get(f.panel.as_str()) {
                None => notes.push(format!("panel `{}` no longer exists", f.panel)),
                Some(spec) if !spec.floatable => {
                    notes.push(format!(
                        "`{}` no longer floats: docked in its default area",
                        f.panel
                    ));
                    redock.push(f.panel);
                }
                Some(_) => floating.push(f),
            }
        }
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
                    let pos = Floating::cascade(layout.floating.len());
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
