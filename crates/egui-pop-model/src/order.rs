//! Plugin ordering (ADR 0004): requirements first, then higher priority,
//! then id, so the order is stable; typed errors for a missing
//! requirement, a cycle or a duplicate id.
//!
//! ```
//! use egui_pop_model::ids::PluginId;
//! use egui_pop_model::order::{Node, order};
//!
//! let p = PluginId::from_static;
//! let nodes = [
//!     Node { id: p("b"), requires: vec![p("a")], priority: 9 },
//!     Node { id: p("a"), requires: vec![], priority: 0 },
//!     Node { id: p("c"), requires: vec![], priority: 5 },
//! ];
//! // `c` outranks `a`, but `b` has to wait for `a`.
//! assert_eq!(order(&nodes).unwrap(), [2, 1, 0]);
//! ```

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};
use std::fmt;

use crate::ids::PluginId;

/// A plugin as ordering sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: PluginId,
    pub requires: Vec<PluginId>,
    pub priority: i32,
}

/// Why plugins cannot be ordered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderError {
    /// `plugin` requires `missing`, which is not there.
    Missing { plugin: PluginId, missing: PluginId },
    /// These plugins require each other, in this order (the first again
    /// at the end).
    Cycle { path: Vec<PluginId> },
    /// Two plugins share an id.
    Duplicate { id: PluginId },
}

impl fmt::Display for OrderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderError::Missing { plugin, missing } => {
                write!(
                    f,
                    "plugin `{plugin}` requires `{missing}`, which is not there"
                )
            }
            OrderError::Cycle { path } => {
                let names: Vec<&str> = path.iter().map(PluginId::as_str).collect();
                write!(f, "plugins require each other: {}", names.join(" -> "))
            }
            OrderError::Duplicate { id } => write!(f, "two plugins have the id `{id}`"),
        }
    }
}

impl std::error::Error for OrderError {}

/// The order to connect plugins in, as indexes into `nodes`.
pub fn order(nodes: &[Node]) -> Result<Vec<usize>, OrderError> {
    let mut index: BTreeMap<&PluginId, usize> = BTreeMap::new();
    for (i, n) in nodes.iter().enumerate() {
        if index.insert(&n.id, i).is_some() {
            return Err(OrderError::Duplicate { id: n.id.clone() });
        }
    }
    let mut waiting = vec![0usize; nodes.len()];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        for r in &n.requires {
            let &j = index.get(r).ok_or_else(|| OrderError::Missing {
                plugin: n.id.clone(),
                missing: r.clone(),
            })?;
            waiting[i] += 1;
            dependents[j].push(i);
        }
    }
    // Ready plugins, highest priority first, then by id.
    let key = |i: usize| (nodes[i].priority, Reverse(nodes[i].id.clone()), i);
    let mut ready: BinaryHeap<_> = (0..nodes.len())
        .filter(|&i| waiting[i] == 0)
        .map(key)
        .collect();
    let mut out = Vec::with_capacity(nodes.len());
    while let Some((_, _, i)) = ready.pop() {
        out.push(i);
        for &d in &dependents[i] {
            waiting[d] -= 1;
            if waiting[d] == 0 {
                ready.push(key(d));
            }
        }
    }
    if out.len() == nodes.len() {
        return Ok(out);
    }
    // A cycle: walk requirements among the plugins left until one repeats.
    let mut at = (0..nodes.len())
        .find(|&i| waiting[i] > 0)
        .expect("a plugin left");
    let mut path: Vec<usize> = Vec::new();
    loop {
        if let Some(pos) = path.iter().position(|&p| p == at) {
            let mut cycle: Vec<PluginId> =
                path[pos..].iter().map(|&i| nodes[i].id.clone()).collect();
            cycle.push(nodes[at].id.clone());
            return Err(OrderError::Cycle { path: cycle });
        }
        path.push(at);
        at = nodes[at]
            .requires
            .iter()
            .map(|r| index[r])
            .find(|&j| waiting[j] > 0)
            .expect("a plugin in a cycle requires another left");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: &'static str, requires: &[&'static str], priority: i32) -> Node {
        Node {
            id: PluginId::from_static(id),
            requires: requires.iter().map(|r| PluginId::from_static(r)).collect(),
            priority,
        }
    }

    fn ids(nodes: &[Node]) -> Vec<&str> {
        order(nodes)
            .unwrap()
            .into_iter()
            .map(|i| nodes[i].id.as_str())
            .collect()
    }

    #[test]
    fn requirements_then_priority_then_id() {
        let nodes = [
            n("theme", &[], 100),
            n("pages", &["theme", "shell"], 0),
            n("shell", &["theme"], 0),
            n("about", &[], 0),
            n("debug", &["shell"], 50),
        ];
        assert_eq!(ids(&nodes), ["theme", "about", "shell", "debug", "pages"]);
        // Same input, same order, whatever the declaration order.
        let mut rev = nodes.to_vec();
        rev.reverse();
        assert_eq!(ids(&rev), ["theme", "about", "shell", "debug", "pages"]);
    }

    #[test]
    fn errors_are_typed_and_name_the_plugins() {
        let missing = order(&[n("a", &["ghost"], 0)]).unwrap_err();
        assert_eq!(
            missing,
            OrderError::Missing {
                plugin: PluginId::from_static("a"),
                missing: PluginId::from_static("ghost")
            }
        );
        let cycle = order(&[
            n("x", &[], 0),
            n("a", &["b"], 0),
            n("b", &["c"], 0),
            n("c", &["a"], 0),
        ])
        .unwrap_err();
        assert_eq!(
            cycle.to_string(),
            "plugins require each other: a -> b -> c -> a"
        );
        let dup = order(&[n("a", &[], 0), n("a", &[], 1)]).unwrap_err();
        assert!(matches!(dup, OrderError::Duplicate { .. }));
        assert_eq!(
            order(&[n("s", &["s"], 0)]).unwrap_err().to_string(),
            "plugins require each other: s -> s"
        );
    }
}
