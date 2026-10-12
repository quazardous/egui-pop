//! Zones and blocks (ADR 0005). A zone is a named place; blocks are drawn
//! in it, lightest first. Four operations: add, replace, extend (before
//! or after a block) and wrap (around a block, receiving it as a
//! closure). They are resolved when the application is built or a plugin
//! comes or goes, never per frame; what cannot apply is reported with
//! what happens instead.

use std::collections::BTreeMap;
use std::fmt;

use egui::Ui;
use egui_pop_model::diagnostics::Diagnostics;
use egui_pop_model::ids::{BlockId, OwnerId, ZoneId};
use serde::Serialize;

use crate::app::AppHandle;

/// The top bar, after the main menu.
pub const TOP_BAR: ZoneId = ZoneId::from_static("app.top-bar");
/// The side bar at the start (left in left-to-right).
pub const SIDE_BAR: ZoneId = ZoneId::from_static("app.side-bar");
/// The main area.
pub const MAIN: ZoneId = ZoneId::from_static("app.main");
/// The side bar at the end.
pub const INSPECTOR: ZoneId = ZoneId::from_static("app.inspector");
/// The status bar at the bottom.
pub const STATUS_BAR: ZoneId = ZoneId::from_static("app.status-bar");

/// Drawn after the shell, every frame: blocks there show overlays (the
/// command palette, dialogs, toasts) when they have something to show.
pub const OVERLAYS: ZoneId = ZoneId::from_static("app.overlays");

/// The shell's zones, declared by the framework.
pub const SHELL: [ZoneId; 6] = [TOP_BAR, SIDE_BAR, MAIN, INSPECTOR, STATUS_BAR, OVERLAYS];

/// Draws a block.
pub type DrawFn = Box<dyn FnMut(&mut Ui, &AppHandle)>;
/// Draws around a block; calls `inner` (or not) where it likes.
pub type WrapFn = Box<dyn FnMut(&mut Ui, &AppHandle, &mut dyn FnMut(&mut Ui))>;

/// Where an extending block goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Place {
    Before(BlockId),
    After(BlockId),
}

/// A block operation, as a plugin declares it.
pub enum Block {
    Add {
        id: BlockId,
        weight: i32,
        draw: DrawFn,
    },
    Replace {
        target: BlockId,
        id: BlockId,
        draw: DrawFn,
    },
    Extend {
        place: Place,
        id: BlockId,
        draw: DrawFn,
    },
    Wrap {
        target: BlockId,
        id: BlockId,
        wrap: WrapFn,
    },
}

impl fmt::Debug for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Block::Add { id, weight, .. } => write!(f, "Add({id}, {weight})"),
            Block::Replace { target, id, .. } => write!(f, "Replace({target} with {id})"),
            Block::Extend { place, id, .. } => write!(f, "Extend({id} {place:?})"),
            Block::Wrap { target, id, .. } => write!(f, "Wrap({target} in {id})"),
        }
    }
}

impl Block {
    /// A new block, drawn by weight (lighter first).
    pub fn add(id: BlockId, weight: i32, draw: impl FnMut(&mut Ui, &AppHandle) + 'static) -> Block {
        Block::Add {
            id,
            weight,
            draw: Box::new(draw),
        }
    }

    /// Takes the place (and weight) of `target`.
    pub fn replace(
        target: BlockId,
        id: BlockId,
        draw: impl FnMut(&mut Ui, &AppHandle) + 'static,
    ) -> Block {
        Block::Replace {
            target,
            id,
            draw: Box::new(draw),
        }
    }

    /// A new block right before or after another.
    pub fn extend(
        place: Place,
        id: BlockId,
        draw: impl FnMut(&mut Ui, &AppHandle) + 'static,
    ) -> Block {
        Block::Extend {
            place,
            id,
            draw: Box::new(draw),
        }
    }

    /// Draws around `target`.
    pub fn wrap(
        target: BlockId,
        id: BlockId,
        wrap: impl FnMut(&mut Ui, &AppHandle, &mut dyn FnMut(&mut Ui)) + 'static,
    ) -> Block {
        Block::Wrap {
            target,
            id,
            wrap: Box::new(wrap),
        }
    }

    pub fn id(&self) -> &BlockId {
        match self {
            Block::Add { id, .. }
            | Block::Replace { id, .. }
            | Block::Extend { id, .. }
            | Block::Wrap { id, .. } => id,
        }
    }

    fn op(&self) -> &'static str {
        match self {
            Block::Add { .. } => "add",
            Block::Replace { .. } => "replace",
            Block::Extend { .. } => "extend",
            Block::Wrap { .. } => "wrap",
        }
    }
}

/// A block as contributed.
#[derive(Debug)]
pub(crate) struct Contribution {
    pub zone: ZoneId,
    pub owner: OwnerId,
    /// The plugin's rank in connection order (later plugins apply later).
    pub rank: usize,
    pub seq: u64,
    /// Taken out while it is drawn.
    pub block: Option<Block>,
}

/// One block of a resolved zone: what draws it, what wraps it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Resolved {
    pub id: BlockId,
    pub owner: OwnerId,
    pub weight: i32,
    /// The contribution drawing it.
    #[serde(skip)]
    pub(crate) draw: usize,
    /// Wrappers, innermost first: (contribution, id, owner).
    #[serde(skip)]
    pub(crate) wraps: Vec<usize>,
    /// What was applied: `replaced by x`, `wrapped by y`.
    pub notes: Vec<String>,
}

/// A declared zone.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ZoneInfo {
    pub id: ZoneId,
    pub owner: OwnerId,
    pub title: String,
    pub blocks: Vec<Resolved>,
}

/// Zones, contributions and their resolution.
#[derive(Debug, Default)]
pub(crate) struct ZoneTable {
    pub zones: BTreeMap<ZoneId, (OwnerId, String)>,
    pub contributions: Vec<Contribution>,
    pub resolved: BTreeMap<ZoneId, Vec<Resolved>>,
    next: u64,
}

impl ZoneTable {
    pub fn declare(&mut self, owner: &OwnerId, zone: ZoneId, title: &str) -> Result<(), OwnerId> {
        if let Some((by, _)) = self.zones.get(&zone) {
            return Err(by.clone());
        }
        self.zones.insert(zone, (owner.clone(), title.into()));
        Ok(())
    }

    pub fn contribute(&mut self, owner: &OwnerId, rank: usize, zone: ZoneId, block: Block) {
        self.next += 1;
        self.contributions.push(Contribution {
            zone,
            owner: owner.clone(),
            rank,
            seq: self.next,
            block: Some(block),
        });
    }

    pub fn remove_owner(&mut self, owner: &OwnerId) {
        self.contributions.retain(|c| c.owner != *owner);
        self.zones.retain(|_, (o, _)| o != owner);
    }

    /// Sets the plugin ranks (connection order) for the resolution.
    pub fn set_rank(&mut self, owner: &OwnerId, rank: usize) {
        for c in &mut self.contributions {
            if c.owner == *owner {
                c.rank = rank;
            }
        }
    }

    /// Resolves every zone; diagnostics for what could not apply.
    pub fn resolve(&mut self, d: &mut Diagnostics) {
        self.resolved.clear();
        let mut by_zone: BTreeMap<ZoneId, Vec<usize>> = BTreeMap::new();
        for (i, c) in self.contributions.iter().enumerate() {
            if !self.zones.contains_key(&c.zone) {
                let op = c.block.as_ref().map_or("block", Block::op);
                let id = c
                    .block
                    .as_ref()
                    .map(|b| b.id().to_string())
                    .unwrap_or_default();
                d.error(
                    "unknown-zone",
                    Some(&c.owner),
                    &id,
                    format!(
                        "{op} `{id}` targets zone `{}`, which nobody declared: it is not shown",
                        c.zone
                    ),
                );
                continue;
            }
            by_zone.entry(c.zone.clone()).or_default().push(i);
        }
        for (zone, mut list) in by_zone {
            // Plugin order, then registration order.
            list.sort_by_key(|&i| (self.contributions[i].rank, self.contributions[i].seq));
            let out = self.resolve_zone(&zone, &list, d);
            self.resolved.insert(zone, out);
        }
        for zone in self.zones.keys() {
            self.resolved.entry(zone.clone()).or_default();
        }
    }

    fn resolve_zone(&self, zone: &ZoneId, list: &[usize], d: &mut Diagnostics) -> Vec<Resolved> {
        let c = &self.contributions;
        let mut out: Vec<Resolved> = Vec::new();
        let taken = |out: &Vec<Resolved>, id: &BlockId| out.iter().any(|r| r.id == *id);
        let dup = |d: &mut Diagnostics, i: usize, id: &BlockId| {
            d.error(
                "duplicate-block",
                Some(&c[i].owner),
                id.as_str(),
                format!("zone `{zone}` already has a block `{id}`: this one is not shown"),
            );
        };
        // Adds, by weight then order.
        let mut adds: Vec<(i32, usize)> = list
            .iter()
            .filter_map(|&i| match &c[i].block {
                Some(Block::Add { weight, .. }) => Some((*weight, i)),
                _ => None,
            })
            .collect();
        adds.sort_by_key(|(w, i)| (*w, c[*i].rank, c[*i].seq));
        for (weight, i) in adds {
            let id = c[i].block.as_ref().expect("a block").id().clone();
            if taken(&out, &id) {
                dup(d, i, &id);
                continue;
            }
            out.push(Resolved {
                id,
                owner: c[i].owner.clone(),
                weight,
                draw: i,
                wraps: Vec::new(),
                notes: Vec::new(),
            });
        }
        let last_weight = |out: &Vec<Resolved>| out.last().map_or(0, |r| r.weight);
        // Replaces.
        for &i in list {
            if let Some(Block::Replace { target, id, .. }) = &c[i].block {
                match out.iter().position(|r| r.id == *target) {
                    Some(p) => {
                        let old = out[p].owner.clone();
                        out[p].id = id.clone();
                        out[p].owner = c[i].owner.clone();
                        out[p].draw = i;
                        out[p].notes.push(format!("replaced `{target}` ({old})"));
                    }
                    None if taken(&out, id) => dup(d, i, id),
                    None => {
                        let weight = last_weight(&out);
                        d.warn(
                            "dangling-replace",
                            Some(&c[i].owner),
                            id.as_str(),
                            format!(
                                "`{id}` replaces `{target}` in zone `{zone}`, which has no such block: it is added as a new block at the end"
                            ),
                        );
                        out.push(Resolved {
                            id: id.clone(),
                            owner: c[i].owner.clone(),
                            weight,
                            draw: i,
                            wraps: Vec::new(),
                            notes: vec![format!("meant to replace missing `{target}`")],
                        });
                    }
                }
            }
        }
        // Extends.
        for &i in list {
            if let Some(Block::Extend { place, id, .. }) = &c[i].block {
                if taken(&out, id) {
                    dup(d, i, id);
                    continue;
                }
                let (target, after) = match place {
                    Place::Before(t) => (t, false),
                    Place::After(t) => (t, true),
                };
                let at = out.iter().position(|r| r.id == *target);
                let (pos, weight, note) = match at {
                    Some(p) => (
                        if after { p + 1 } else { p },
                        out[p].weight,
                        format!("{} `{target}`", if after { "after" } else { "before" }),
                    ),
                    None => {
                        d.warn(
                            "dangling-extend",
                            Some(&c[i].owner),
                            id.as_str(),
                            format!(
                                "`{id}` goes {} `{target}` in zone `{zone}`, which has no such block: it is added at the end",
                                if after { "after" } else { "before" }
                            ),
                        );
                        (
                            out.len(),
                            last_weight(&out),
                            format!("meant to go next to missing `{target}`"),
                        )
                    }
                };
                out.insert(
                    pos,
                    Resolved {
                        id: id.clone(),
                        owner: c[i].owner.clone(),
                        weight,
                        draw: i,
                        wraps: Vec::new(),
                        notes: vec![note],
                    },
                );
            }
        }
        // Wraps, in plugin order: the last applied is the outermost.
        for &i in list {
            if let Some(Block::Wrap { target, id, .. }) = &c[i].block {
                match out.iter_mut().find(|r| r.id == *target) {
                    Some(r) => {
                        r.wraps.push(i);
                        r.notes.push(format!("wrapped by `{id}` ({})", c[i].owner));
                    }
                    None => d.warn(
                        "dangling-wrap",
                        Some(&c[i].owner),
                        id.as_str(),
                        format!("`{id}` wraps `{target}` in zone `{zone}`, which has no such block: the wrap is skipped"),
                    ),
                }
            }
        }
        out
    }

    /// The zones with their resolved blocks.
    pub fn infos(&self) -> Vec<ZoneInfo> {
        self.zones
            .iter()
            .map(|(id, (owner, title))| ZoneInfo {
                id: id.clone(),
                owner: owner.clone(),
                title: title.clone(),
                blocks: self.resolved.get(id).cloned().unwrap_or_default(),
            })
            .collect()
    }
}

/// Draws a resolved block through its wrappers.
pub(crate) fn draw_chain(ui: &mut Ui, app: &AppHandle, draw: &mut DrawFn, wraps: &mut [WrapFn]) {
    match wraps.split_last_mut() {
        None => draw(ui, app),
        Some((outer, inner)) => {
            outer(ui, app, &mut |ui: &mut Ui| draw_chain(ui, app, draw, inner));
        }
    }
}
