//! The tree's model: what the view knows of the application's tree.
//!
//! The application owns its tree; the view keeps a cache of the
//! branches it loaded, keyed by node, so that edits (insert, remove,
//! move, rename) arriving at any time keep the selection and the
//! expansion, which are keyed too. The rows on screen are the cache
//! flattened through the expanded branches, rebuilt only when something
//! changed.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::select::Key;

/// A node as the application gives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node<K> {
    pub key: K,
    /// What the row shows, decorations included.
    pub label: String,
    /// Whether it may have children (they are loaded when it is first
    /// expanded).
    pub branch: bool,
    /// The plain name a rename starts from and type-ahead searches,
    /// when the label carries more (a count, a state); `None`: the
    /// label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl<K> Node<K> {
    pub fn leaf(key: K, label: impl Into<String>) -> Self {
        Self {
            key,
            label: label.into(),
            branch: false,
            name: None,
        }
    }

    pub fn branch(key: K, label: impl Into<String>) -> Self {
        Self {
            key,
            label: label.into(),
            branch: true,
            name: None,
        }
    }

    /// With a plain name apart from its label.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The plain name: the one given, else the label.
    pub fn name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.label)
    }
}

/// Where dropped nodes go, relative to the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Before,
    After,
    /// As the target's last children.
    Inside,
}

/// What the view knows of a branch's children.
#[derive(Clone, Debug, PartialEq)]
pub enum Children<K> {
    /// Asked for; `stale` are the children shown meanwhile (a reload).
    Loading {
        ticket: u64,
        stale: Vec<K>,
    },
    Loaded(Vec<K>),
    Failed(String),
    /// To be loaded again; shown as they were until then.
    Stale(Vec<K>),
}

/// A row on screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Row<K> {
    Node {
        key: K,
        depth: usize,
    },
    /// A branch whose children are on their way.
    Loading {
        parent: Option<K>,
        depth: usize,
    },
    /// A branch whose children could not be loaded.
    Failed {
        parent: Option<K>,
        depth: usize,
        reason: String,
    },
    /// An expanded branch without children.
    Empty {
        parent: Option<K>,
        depth: usize,
    },
}

impl<K> Row<K> {
    pub fn key(&self) -> Option<&K> {
        match self {
            Row::Node { key, .. } => Some(key),
            _ => None,
        }
    }

    pub fn depth(&self) -> usize {
        match self {
            Row::Node { depth, .. }
            | Row::Loading { depth, .. }
            | Row::Failed { depth, .. }
            | Row::Empty { depth, .. } => *depth,
        }
    }
}

/// A change to the cache, from the application or a source.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit<K> {
    /// A branch's children, from a load (`ticket`) or set directly
    /// (`None`).
    Children {
        parent: Option<K>,
        ticket: Option<u64>,
        result: Result<Vec<Node<K>>, String>,
    },
    /// A node under `parent`, at `index` (`None`: last).
    Insert {
        parent: Option<K>,
        index: Option<usize>,
        node: Node<K>,
    },
    /// A node and what is under it.
    Remove(K),
    /// A node to another place.
    Move {
        key: K,
        parent: Option<K>,
        index: Option<usize>,
    },
    Rename {
        key: K,
        label: String,
    },
    /// A node's label, name and kind.
    Update(Node<K>),
    /// A branch to load again (`None`: the roots).
    Reload(Option<K>),
}

/// The cache, the expansion and the rows.
#[derive(Debug)]
pub struct Model<K: Key> {
    nodes: HashMap<K, (Node<K>, Option<K>)>,
    children: HashMap<Option<K>, Children<K>>,
    expanded: HashSet<K>,
    rows: Vec<Row<K>>,
    /// The rows that are nodes, by index.
    node_rows: Vec<usize>,
    /// Branches the last flattening found expanded and not loaded.
    wanted: Vec<Option<K>>,
    dirty: bool,
    tickets: u64,
    /// Counts the changes to the nodes (not to the expansion).
    version: u64,
    /// The filter's matches, while there is a query.
    filter: Option<Filtered<K>>,
    /// How many times the rows were flattened.
    flattens: u64,
}

/// What a filter shows: the nodes that match, their ancestors, and the
/// branches opened or closed while it filters.
#[derive(Debug)]
struct Filtered<K> {
    matched: HashSet<K>,
    ancestors: HashSet<K>,
    folds: HashMap<K, bool>,
}

impl<K: Key> Default for Model<K> {
    fn default() -> Self {
        Self {
            nodes: HashMap::new(),
            children: HashMap::new(),
            expanded: HashSet::new(),
            rows: Vec::new(),
            node_rows: Vec::new(),
            wanted: Vec::new(),
            dirty: true,
            tickets: 0,
            version: 0,
            filter: None,
            flattens: 0,
        }
    }
}

impl<K: Key> Model<K> {
    pub fn node(&self, key: &K) -> Option<&Node<K>> {
        self.nodes.get(key).map(|(n, _)| n)
    }

    pub fn contains(&self, key: &K) -> bool {
        self.nodes.contains_key(key)
    }

    /// The parent of a known node (`Some(None)`: a root).
    pub fn parent(&self, key: &K) -> Option<Option<&K>> {
        self.nodes.get(key).map(|(_, p)| p.as_ref())
    }

    pub fn children(&self, parent: Option<&K>) -> Option<&Children<K>> {
        self.children.get(&parent.cloned())
    }

    /// A branch's loaded children.
    pub fn loaded(&self, parent: Option<&K>) -> Option<&[K]> {
        match self.children(parent)? {
            Children::Loaded(v) => Some(v),
            _ => None,
        }
    }

    /// Whether a branch is open in the fold the tree keeps (a filter
    /// may show it otherwise, see [`Model::is_open`]).
    pub fn is_expanded(&self, key: &K) -> bool {
        self.expanded.contains(key)
    }

    /// Whether a branch shows open: as the filter opened it while there
    /// is one, else as it is expanded.
    pub fn is_open(&self, key: &K) -> bool {
        match &self.filter {
            Some(f) => f
                .folds
                .get(key)
                .copied()
                .unwrap_or_else(|| f.ancestors.contains(key)),
            None => self.expanded.contains(key),
        }
    }

    /// Opens or closes a branch as it shows: while a filter is set, until
    /// the query changes, the fold kept untouched; else its expansion.
    pub(crate) fn set_open(&mut self, key: &K, open: bool) {
        match &mut self.filter {
            Some(f) => {
                f.folds.insert(key.clone(), open);
                self.dirty = true;
            }
            None => self.set_expanded(key, open),
        }
    }

    /// Every node the view knows, in no order.
    pub fn nodes(&self) -> impl Iterator<Item = &Node<K>> {
        self.nodes.values().map(|(n, _)| n)
    }

    /// Changes to the nodes so far: it moves when a node comes, goes,
    /// moves or changes.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether a filter is set.
    pub fn is_filtered(&self) -> bool {
        self.filter.is_some()
    }

    /// Whether a node matches the filter set.
    pub fn is_match(&self, key: &K) -> bool {
        self.filter
            .as_ref()
            .is_some_and(|f| f.matched.contains(key))
    }

    /// How many nodes match the filter set (`None`: no filter).
    pub fn matches(&self) -> Option<usize> {
        self.filter.as_ref().map(|f| f.matched.len())
    }

    /// Sets the nodes the filter matched (`None`: no filter). `fresh`: a
    /// new query, which forgets the branches opened or closed under the
    /// last one.
    pub(crate) fn set_matches(&mut self, matched: Option<HashSet<K>>, fresh: bool) {
        let Some(matched) = matched else {
            self.dirty |= self.filter.take().is_some();
            return;
        };
        let mut ancestors = HashSet::new();
        for k in &matched {
            let mut at = self.nodes.get(k).and_then(|(_, p)| p.clone());
            while let Some(p) = at {
                if !ancestors.insert(p.clone()) {
                    break;
                }
                at = self.nodes.get(&p).and_then(|(_, p)| p.clone());
            }
        }
        let folds = match self.filter.take() {
            Some(f) if !fresh => f.folds,
            _ => HashMap::new(),
        };
        self.filter = Some(Filtered {
            matched,
            ancestors,
            folds,
        });
        self.dirty = true;
    }

    pub fn set_expanded(&mut self, key: &K, expanded: bool) {
        let changed = if expanded {
            self.expanded.insert(key.clone())
        } else {
            self.expanded.remove(key)
        };
        self.dirty |= changed;
    }

    /// How many nodes the cache holds.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Whether `key` is `ancestor` or under it.
    pub fn is_within(&self, key: &K, ancestor: &K) -> bool {
        let mut at = Some(key.clone());
        while let Some(k) = at {
            if k == *ancestor {
                return true;
            }
            at = self.nodes.get(&k).and_then(|(_, p)| p.clone());
        }
        false
    }

    /// Marks a branch as loading and returns its ticket: only the
    /// answer carrying it is taken.
    pub fn begin_load(&mut self, parent: Option<&K>) -> u64 {
        self.tickets += 1;
        let ticket = self.tickets;
        let stale = match self.children.remove(&parent.cloned()) {
            Some(Children::Loaded(v) | Children::Stale(v)) => v,
            Some(Children::Loading { stale, .. }) => stale,
            _ => Vec::new(),
        };
        self.children
            .insert(parent.cloned(), Children::Loading { ticket, stale });
        self.dirty = true;
        ticket
    }

    /// Loads every branch again, as a [`Edit::Reload`] of each: what
    /// was loaded stays on screen until its new children come, a branch
    /// that was loading or had failed is asked for again (the answer it
    /// waited for is no longer taken). Used when edits were dropped.
    pub fn reload_all(&mut self) {
        let parents: Vec<Option<K>> = self.children.keys().cloned().collect();
        for parent in parents {
            let stale = match self.children.remove(&parent) {
                Some(Children::Loaded(v) | Children::Stale(v)) => v,
                Some(Children::Loading { stale, .. }) => stale,
                Some(Children::Failed(_)) | None => continue,
            };
            self.children.insert(parent, Children::Stale(stale));
        }
        self.dirty = true;
        self.version += 1;
    }

    /// Marks a branch as failed (a load vetoed or refused).
    pub fn fail(&mut self, parent: Option<&K>, reason: &str) {
        let stale = match self.children.remove(&parent.cloned()) {
            Some(Children::Loaded(v) | Children::Stale(v)) => v,
            Some(Children::Loading { stale, .. }) => stale,
            _ => Vec::new(),
        };
        for k in stale {
            self.forget(&k);
        }
        self.children
            .insert(parent.cloned(), Children::Failed(reason.into()));
        self.dirty = true;
        self.version += 1;
    }

    /// Applies an edit; returns whether it changed anything.
    pub fn apply(&mut self, edit: Edit<K>) -> bool {
        let changed = match edit {
            Edit::Children {
                parent,
                ticket,
                result,
            } => self.set_children(parent, ticket, result),
            Edit::Insert {
                parent,
                index,
                node,
            } => self.insert(parent, index, node),
            Edit::Remove(key) => {
                let known = self.contains(&key);
                self.detach(&key);
                self.forget(&key);
                known
            }
            Edit::Move { key, parent, index } => self.move_to(&key, parent, index),
            Edit::Rename { key, label } => match self.nodes.get_mut(&key) {
                Some((n, _)) if n.label != label => {
                    n.label = label;
                    true
                }
                _ => false,
            },
            Edit::Update(node) => match self.nodes.get_mut(&node.key) {
                Some((n, _)) if *n != node => {
                    *n = node;
                    true
                }
                _ => false,
            },
            Edit::Reload(parent) => match self.children.remove(&parent) {
                Some(Children::Loaded(v) | Children::Stale(v)) => {
                    self.children.insert(parent, Children::Stale(v));
                    true
                }
                Some(Children::Failed(_)) | None => true,
                Some(c @ Children::Loading { .. }) => {
                    self.children.insert(parent, c);
                    false
                }
            },
        };
        self.dirty |= changed;
        if changed {
            self.version += 1;
        }
        changed
    }

    fn set_children(
        &mut self,
        parent: Option<K>,
        ticket: Option<u64>,
        result: Result<Vec<Node<K>>, String>,
    ) -> bool {
        if let Some(t) = ticket
            && !matches!(self.children.get(&parent), Some(Children::Loading { ticket, .. }) if *ticket == t)
        {
            return false;
        }
        if let Some(p) = &parent
            && !self.contains(p)
        {
            return false;
        }
        let old = match self.children.remove(&parent) {
            Some(Children::Loaded(v) | Children::Stale(v)) => v,
            Some(Children::Loading { stale, .. }) => stale,
            _ => Vec::new(),
        };
        match result {
            Ok(nodes) => {
                let keep: HashSet<&K> = nodes.iter().map(|n| &n.key).collect();
                for k in old.iter().filter(|k| !keep.contains(k)) {
                    self.forget(k);
                }
                let mut keys = Vec::with_capacity(nodes.len());
                for node in nodes {
                    // A node shown elsewhere moves here.
                    if self.nodes.get(&node.key).is_some_and(|(_, p)| *p != parent) {
                        self.detach(&node.key);
                    }
                    keys.push(node.key.clone());
                    self.nodes.insert(node.key.clone(), (node, parent.clone()));
                }
                self.children.insert(parent, Children::Loaded(keys));
            }
            Err(reason) => {
                for k in &old {
                    self.forget(k);
                }
                self.children.insert(parent, Children::Failed(reason));
            }
        }
        true
    }

    fn insert(&mut self, parent: Option<K>, index: Option<usize>, node: Node<K>) -> bool {
        if self.contains(&node.key) {
            let key = node.key.clone();
            self.apply(Edit::Update(node));
            return self.move_to(&key, parent, index);
        }
        let Some(Children::Loaded(list)) = self.children.get_mut(&parent) else {
            // Not loaded: the node comes with the branch.
            return false;
        };
        let at = index.unwrap_or(list.len()).min(list.len());
        list.insert(at, node.key.clone());
        self.nodes.insert(node.key.clone(), (node, parent));
        true
    }

    fn move_to(&mut self, key: &K, parent: Option<K>, index: Option<usize>) -> bool {
        if !self.contains(key) || parent.as_ref().is_some_and(|p| self.is_within(p, key)) {
            return false;
        }
        self.detach(key);
        match self.children.get_mut(&parent) {
            Some(Children::Loaded(list)) => {
                let at = index.unwrap_or(list.len()).min(list.len());
                list.insert(at, key.clone());
                if let Some(e) = self.nodes.get_mut(key) {
                    e.1 = parent;
                }
            }
            // The branch shows it once loaded.
            _ => self.forget(key),
        }
        true
    }

    /// Moves nodes, in order, before, after or inside `target` (a
    /// branch not loaded takes them when it loads). Nodes that would go
    /// into themselves are left where they are.
    pub fn move_many(&mut self, keys: &[K], target: &K, position: Position) {
        let parent = match position {
            Position::Inside => Some(target.clone()),
            _ => match self.nodes.get(target) {
                Some((_, p)) => p.clone(),
                None => return,
            },
        };
        let keys: Vec<K> = keys
            .iter()
            .filter(|k| self.contains(k) && *k != target)
            .filter(|k| !parent.as_ref().is_some_and(|p| self.is_within(p, k)))
            .cloned()
            .collect();
        for k in &keys {
            self.detach(k);
        }
        self.dirty = true;
        self.version += 1;
        let Some(Children::Loaded(list)) = self.children.get_mut(&parent) else {
            for k in &keys {
                self.forget(k);
            }
            return;
        };
        let at = match position {
            Position::Inside => list.len(),
            Position::Before => list.iter().position(|k| k == target).unwrap_or(list.len()),
            Position::After => list
                .iter()
                .position(|k| k == target)
                .map_or(list.len(), |i| i + 1),
        };
        list.splice(at..at, keys.iter().cloned());
        for k in &keys {
            if let Some(e) = self.nodes.get_mut(k) {
                e.1 = parent.clone();
            }
        }
    }

    /// Takes a node out of its parent's list.
    fn detach(&mut self, key: &K) {
        let Some((_, parent)) = self.nodes.get(key) else {
            return;
        };
        if let Some(
            Children::Loaded(list) | Children::Stale(list) | Children::Loading { stale: list, .. },
        ) = self.children.get_mut(parent)
        {
            list.retain(|k| k != key);
        }
    }

    /// Drops a node and what is under it from the cache (not from its
    /// parent's list). The expansion is kept: a node coming back opens
    /// as it was.
    fn forget(&mut self, key: &K) {
        let mut todo = vec![key.clone()];
        while let Some(k) = todo.pop() {
            self.nodes.remove(&k);
            if let Some(
                Children::Loaded(v) | Children::Stale(v) | Children::Loading { stale: v, .. },
            ) = self.children.remove(&Some(k))
            {
                todo.extend(v);
            }
        }
        self.dirty = true;
        self.version += 1;
    }

    /// The rows, flattened again when something changed.
    pub fn rows(&mut self) -> &[Row<K>] {
        if self.dirty {
            self.flatten();
        }
        &self.rows
    }

    /// The indices of the rows that are nodes, as last flattened.
    pub fn node_rows(&self) -> &[usize] {
        &self.node_rows
    }

    /// The rows as last flattened.
    pub fn rows_now(&self) -> &[Row<K>] {
        &self.rows
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Branches shown expanded whose children were never asked for, or
    /// must be asked again, since the last call.
    pub fn take_wanted(&mut self) -> Vec<Option<K>> {
        if self.dirty {
            self.flatten();
        }
        std::mem::take(&mut self.wanted)
    }

    fn flatten(&mut self) {
        /// A node to show, its depth, and whether it is under a node that
        /// matches the filter (then all of its children show).
        enum Item<K> {
            Node(K, usize, bool),
            Row(Row<K>),
        }
        let filter = self.filter.as_ref();
        let mut rows = Vec::with_capacity(self.rows.len());
        let mut wanted = Vec::new();
        let mut stack: Vec<Item<K>> = Vec::new();
        let push = |parent: Option<K>,
                    depth: usize,
                    under: bool,
                    stack: &mut Vec<Item<K>>,
                    wanted: &mut Vec<Option<K>>| {
            // A filter hides what is neither a match, an ancestor of one,
            // nor under a match: the rows saying how a branch loads too.
            let all = filter.is_none() || under;
            let list = match self.children.get(&parent) {
                None => {
                    if all {
                        wanted.push(parent.clone());
                        stack.push(Item::Row(Row::Loading { parent, depth }));
                    }
                    return;
                }
                Some(Children::Loading { stale, .. }) if stale.is_empty() => {
                    if all {
                        stack.push(Item::Row(Row::Loading { parent, depth }));
                    }
                    return;
                }
                Some(Children::Failed(reason)) => {
                    if all {
                        stack.push(Item::Row(Row::Failed {
                            parent,
                            depth,
                            reason: reason.clone(),
                        }));
                    }
                    return;
                }
                Some(Children::Loaded(v)) if v.is_empty() => {
                    if all {
                        stack.push(Item::Row(Row::Empty { parent, depth }));
                    }
                    return;
                }
                Some(Children::Stale(v)) => {
                    wanted.push(parent.clone());
                    v
                }
                Some(Children::Loaded(v) | Children::Loading { stale: v, .. }) => v,
            };
            let shown = |k: &&K| {
                all || filter.is_some_and(|f| f.matched.contains(*k) || f.ancestors.contains(*k))
            };
            stack.extend(
                list.iter()
                    .rev()
                    .filter(shown)
                    .map(|k| Item::Node(k.clone(), depth, under)),
            );
        };
        push(None, 0, false, &mut stack, &mut wanted);
        while let Some(item) = stack.pop() {
            match item {
                Item::Row(r) => rows.push(r),
                Item::Node(key, depth, under) => {
                    let open = self
                        .nodes
                        .get(&key)
                        .is_some_and(|(n, _)| n.branch && self.is_open(&key));
                    let under = under || filter.is_some_and(|f| f.matched.contains(&key));
                    rows.push(Row::Node {
                        key: key.clone(),
                        depth,
                    });
                    if open {
                        push(Some(key), depth + 1, under, &mut stack, &mut wanted);
                    }
                }
            }
        }
        self.node_rows = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, Row::Node { .. }))
            .map(|(i, _)| i)
            .collect();
        self.rows = rows;
        self.wanted = wanted;
        self.dirty = false;
        self.flattens += 1;
    }

    /// How many times the rows were flattened: it moves when they may
    /// have changed.
    pub(crate) fn flattens(&self) -> u64 {
        self.flattens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(m: &mut Model<String>) -> Vec<String> {
        m.rows()
            .iter()
            .map(|r| match r {
                Row::Node { key, depth } => format!("{}{key}", " ".repeat(*depth)),
                Row::Loading { depth, .. } => format!("{}…", " ".repeat(*depth)),
                Row::Failed { depth, .. } => format!("{}!", " ".repeat(*depth)),
                Row::Empty { depth, .. } => format!("{}-", " ".repeat(*depth)),
            })
            .collect()
    }

    fn load(m: &mut Model<String>, parent: Option<&str>, nodes: Vec<Node<String>>) {
        let p = parent.map(String::from);
        let t = m.begin_load(p.as_ref());
        assert!(m.apply(Edit::Children {
            parent: p,
            ticket: Some(t),
            result: Ok(nodes),
        }));
    }

    fn b(k: &str) -> Node<String> {
        Node::branch(k.to_string(), k)
    }

    fn l(k: &str) -> Node<String> {
        Node::leaf(k.to_string(), k)
    }

    #[test]
    fn expanded_branches_ask_for_their_children_once() {
        let mut m = Model::<String>::default();
        assert_eq!(keys(&mut m), ["…"]);
        assert_eq!(m.take_wanted(), vec![None]);
        load(&mut m, None, vec![b("a"), l("b")]);
        assert_eq!(keys(&mut m), ["a", "b"]);
        m.set_expanded(&"a".into(), true);
        assert_eq!(keys(&mut m), ["a", " …", "b"]);
        assert_eq!(m.take_wanted(), vec![Some("a".into())]);
        let t = m.begin_load(Some(&"a".into()));
        assert!(m.take_wanted().is_empty(), "asked once");
        // A late answer of an older ticket is dropped.
        assert!(!m.apply(Edit::Children {
            parent: Some("a".into()),
            ticket: Some(t - 1),
            result: Ok(vec![]),
        }));
        m.apply(Edit::Children {
            parent: Some("a".into()),
            ticket: Some(t),
            result: Ok(vec![l("a1"), b("a2")]),
        });
        assert_eq!(keys(&mut m), ["a", " a1", " a2", "b"]);
    }

    #[test]
    fn edits_keep_the_expansion_and_refuse_cycles() {
        let mut m = Model::<String>::default();
        load(&mut m, None, vec![b("a"), b("b")]);
        m.set_expanded(&"a".into(), true);
        load(&mut m, Some("a"), vec![b("a1")]);
        m.set_expanded(&"a1".into(), true);
        load(&mut m, Some("a1"), vec![l("x")]);
        assert_eq!(keys(&mut m), ["a", " a1", "  x", "b"]);
        // Into its own child: refused.
        assert!(!m.apply(Edit::Move {
            key: "a".into(),
            parent: Some("a1".into()),
            index: None,
        }));
        m.apply(Edit::Insert {
            parent: None,
            index: Some(0),
            node: l("new"),
        });
        m.apply(Edit::Move {
            key: "x".into(),
            parent: None,
            index: None,
        });
        assert_eq!(keys(&mut m), ["new", "a", " a1", "  -", "b", "x"]);
        m.apply(Edit::Remove("a".into()));
        assert!(!m.contains(&"a1".into()));
        assert!(m.is_expanded(&"a".into()), "kept for when it comes back");
        assert_eq!(keys(&mut m), ["new", "b", "x"]);
    }

    #[test]
    fn a_reload_shows_the_old_children_until_the_new_ones_come() {
        let mut m = Model::<String>::default();
        load(&mut m, None, vec![l("a"), l("b")]);
        m.apply(Edit::Reload(None));
        assert_eq!(keys(&mut m), ["a", "b"]);
        assert_eq!(m.take_wanted(), vec![None]);
        let t = m.begin_load(None);
        assert_eq!(keys(&mut m), ["a", "b"]);
        m.apply(Edit::Children {
            parent: None,
            ticket: Some(t),
            result: Ok(vec![l("b"), l("c")]),
        });
        assert_eq!(keys(&mut m), ["b", "c"]);
        assert!(!m.contains(&"a".into()));
        let t = m.begin_load(None);
        m.apply(Edit::Children {
            parent: None,
            ticket: Some(t),
            result: Err("offline".into()),
        });
        assert_eq!(keys(&mut m), ["!"]);
    }

    #[test]
    fn several_nodes_drop_in_order() {
        let mut m = Model::<String>::default();
        load(&mut m, None, vec![l("a"), l("b"), l("c"), b("d")]);
        m.move_many(&["c".into(), "a".into()], &"b".into(), Position::Before);
        assert_eq!(keys(&mut m), ["c", "a", "b", "d"]);
        m.move_many(&["c".into()], &"d".into(), Position::After);
        assert_eq!(keys(&mut m), ["a", "b", "d", "c"]);
        m.set_expanded(&"d".into(), true);
        load(&mut m, Some("d"), vec![]);
        m.move_many(&["a".into(), "b".into()], &"d".into(), Position::Inside);
        assert_eq!(keys(&mut m), ["d", " a", " b", "c"]);
        // Into itself: left where it is.
        m.move_many(&["d".into()], &"a".into(), Position::After);
        assert_eq!(keys(&mut m), ["d", " a", " b", "c"]);
    }

    #[test]
    fn a_filter_shows_matches_with_their_ancestors_and_keeps_the_fold() {
        let mut m = Model::<String>::default();
        load(&mut m, None, vec![b("a"), b("b"), l("c")]);
        m.set_expanded(&"a".into(), true);
        load(&mut m, Some("a"), vec![b("a1"), l("a2")]);
        m.set_expanded(&"a1".into(), true);
        load(&mut m, Some("a1"), vec![l("x"), l("y")]);
        m.set_expanded(&"a1".into(), false);
        assert_eq!(keys(&mut m), ["a", " a1", " a2", "b", "c"]);
        // `y` matches: its folded ancestors open for it, `b` (not
        // loaded) and the rest hide.
        m.set_matches(Some(["y".to_string()].into()), true);
        assert_eq!(keys(&mut m), ["a", " a1", "  y"]);
        assert!(m.take_wanted().is_empty(), "nothing loads for a filter");
        assert!(!m.is_expanded(&"a1".into()) && m.is_open(&"a1".into()));
        // Closed while filtering: only the filter's fold changes.
        m.set_open(&"a1".into(), false);
        assert_eq!(keys(&mut m), ["a", " a1"]);
        assert!(!m.is_expanded(&"a1".into()));
        // The same query again keeps it; a new one forgets it.
        m.set_matches(Some(["y".to_string()].into()), false);
        assert_eq!(keys(&mut m), ["a", " a1"]);
        // A branch that matches opens onto all of its children.
        m.set_matches(Some(["a1".to_string()].into()), true);
        assert_eq!(keys(&mut m), ["a", " a1"]);
        m.set_open(&"a1".into(), true);
        assert_eq!(keys(&mut m), ["a", " a1", "  x", "  y"]);
        // Nothing matches: no row.
        m.set_matches(Some(Default::default()), true);
        assert!(keys(&mut m).is_empty());
        // No filter: the fold as it was kept.
        m.set_matches(None, true);
        assert_eq!(keys(&mut m), ["a", " a1", " a2", "b", "c"]);
    }

    #[test]
    fn a_hundred_thousand_nodes_flatten() {
        let mut m = Model::<u32>::default();
        let t = m.begin_load(None);
        m.apply(Edit::Children {
            parent: None,
            ticket: Some(t),
            result: Ok((0..1000).map(|i| Node::branch(i, format!("{i}"))).collect()),
        });
        for i in 0..1000u32 {
            m.set_expanded(&i, true);
            let t = m.begin_load(Some(&i));
            m.apply(Edit::Children {
                parent: Some(i),
                ticket: Some(t),
                result: Ok((0..99)
                    .map(|j| Node::leaf(10_000 + i * 100 + j, format!("{i}.{j}")))
                    .collect()),
            });
        }
        assert_eq!(m.rows().len(), 100_000);
        assert_eq!(m.len(), 100_000);
    }
}
