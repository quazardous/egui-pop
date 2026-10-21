//! A reference [`GraphModel`] kept in a YAML file, for applications that
//! have no graph model of their own (the `graph-yaml` feature).
//!
//! [`YamlGraph`] reads a graph file and is the model the editor draws;
//! [`YamlGraph::apply`] is its one mutation point: each [`Edit`] the
//! editor hands back (or a `graph.*` step lets through) changes the
//! file's text where the edit lands, and nothing else. What the editor
//! did not touch is written back as it was read: the order of keys,
//! comments, blank lines, quoting, and keys egui-pop does not know.
//!
//! ```yaml
//! $extensions:
//!   org.egui-pop.graph: {version: 1}
//! groups:
//!   detail: Detail
//! kinds:
//!   load:
//!     title: Load
//!     outputs: {image: image}
//!   blur:
//!     title: Blur
//!     inputs: {image: image}
//!     outputs: {image: image}
//!     params:
//!       radius: {label: Radius, value: 2, min: 0, max: 20, step: 0.1, unit: px}
//! nodes:
//!   - id: load
//!     kind: load
//!     pos: [0, 0]
//!   # Softens the source.
//!   - id: blur
//!     kind: blur
//!     pos: [240, 0]
//!     group: detail
//!     params:
//!       radius: 3.5
//! edges:
//!   - load.image -> blur.image
//! ```
//!
//! The format is described in `docs/node-graph.md` ("A graph file").

mod entries;
mod text;

use std::collections::HashSet;
use std::fmt;

use serde_json::Value as Json;

use super::{Edge, Edit, GraphModel, Group, Kind, Node, Param, Port, PortRef, PortType};
use entries::{
    At, EdgeEntry, EdgeItem, Entries, Field, KindEntry, NodeEntry, ParamEntry, PortEntry, RawFile,
};
use text::{
    Entry, Splices, Text, comment_start, edge_inline, item_keys, items_block, node_lines, num,
    param_inline, pos, replace_value, sections, seq_items, string, sub_keys,
};

/// The key of the root `$extensions` that stamps a graph file with the
/// version of its format: `org.egui-pop.graph: {version: 1}`, as token
/// files carry `org.egui-pop.format`.
pub const FORMAT_EXTENSION: &str = "org.egui-pop.graph";

/// The version of the graph format this egui-pop writes. A file
/// without a stamp is read as this version.
pub const FORMAT_VERSION: u64 = 1;

/// A new graph's text.
const EMPTY: &str = "$extensions:\n  org.egui-pop.graph: {version: 1}\nnodes: []\nedges: []\n";

/// Why a graph file cannot be read: what is wrong, and where when that
/// is known (a line and a column, from 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YamlGraphError {
    pub line: Option<u64>,
    pub column: Option<u64>,
    pub message: String,
}

impl YamlGraphError {
    fn at(at: At, message: String) -> YamlGraphError {
        let at = at.known();
        YamlGraphError {
            line: at.map(|a| a.line),
            column: at.map(|a| a.column),
            message,
        }
    }
}

impl fmt::Display for YamlGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(l), Some(c)) => write!(f, "line {l}, column {c}: {}", self.message),
            (Some(l), None) => write!(f, "line {l}: {}", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for YamlGraphError {}

/// The graph as the editor reads it.
#[derive(Clone, Debug, Default, PartialEq)]
struct Built {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    params: Vec<(String, Vec<Param>)>,
    groups: Vec<Group>,
    kinds: Vec<Kind>,
}

/// A graph kept in a YAML file: the editor's model, changed by
/// [`apply`](YamlGraph::apply), written back by [`text`](YamlGraph::text).
///
/// ```
/// use egui_pop_viewport::graph::yaml::YamlGraph;
/// use egui_pop_viewport::graph::{Edit, GraphModel, Moved};
///
/// let src = "nodes:\n  - id: a # the first\n    pos: [0, 0]\n";
/// let mut graph = YamlGraph::parse(src).unwrap();
/// assert_eq!(graph.text(), src);
/// graph.apply(&Edit::Move {
///     moves: vec![Moved { node: "a".into(), from: [0.0, 0.0], to: [40.0, 10.0] }],
/// });
/// assert_eq!(graph.text(), "nodes:\n  - id: a # the first\n    pos: [40, 10]\n");
/// assert_eq!(graph.nodes()[0].pos, [40.0, 10.0]);
/// ```
#[derive(Clone, Debug)]
pub struct YamlGraph {
    text: String,
    entries: Entries,
    built: Built,
    version: u64,
    warnings: Vec<String>,
}

impl Default for YamlGraph {
    fn default() -> Self {
        YamlGraph::empty()
    }
}

impl YamlGraph {
    /// A graph without nodes, its file stamped with [`FORMAT_VERSION`].
    pub fn empty() -> YamlGraph {
        YamlGraph::parse(EMPTY).expect("the empty graph reads")
    }

    /// Reads a graph file. Its text is kept as it is: [`text`](Self::text)
    /// gives it back byte for byte until an edit.
    pub fn parse(text: &str) -> Result<YamlGraph, YamlGraphError> {
        let t = Text::new(text);
        let secs = sections(&t).map_err(|i| YamlGraphError {
            line: Some(i as u64 + 1),
            column: Some(1),
            message: "the top level must be a block mapping, one `key:` per line".into(),
        })?;
        let raw: RawFile = if secs.is_empty() {
            RawFile::default()
        } else {
            serde_saphyr::from_str(text).map_err(|e| {
                let at = e.location().filter(|l| l.line() > 0);
                YamlGraphError {
                    line: at.map(|l| l.line()),
                    column: at.map(|l| l.column()),
                    message: e.without_snippet().to_string(),
                }
            })?
        };
        let entries = Entries::from(raw);
        let version = version(entries.extensions.as_ref())?;
        let built = build(&entries)?;
        let mut warnings = Vec::new();
        if version > FORMAT_VERSION {
            warnings.push(format!(
                "the file is stamped with graph format {version}, newer than {FORMAT_VERSION}: \
                 it is read as {FORMAT_VERSION}, keys not known are kept as they are"
            ));
        }
        Ok(YamlGraph {
            text: text.to_string(),
            entries,
            built,
            version,
            warnings,
        })
    }

    /// The file's text: as read, with the edits applied since.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The format version the file is stamped with ([`FORMAT_VERSION`]
    /// without a stamp).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// What reading the file noticed without failing (a stamp newer than
    /// this egui-pop knows).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// A parameter's value.
    pub fn param(&self, node: &str, param: &str) -> Option<f64> {
        self.built
            .params
            .iter()
            .find(|(n, _)| n == node)?
            .1
            .iter()
            .find(|p| p.id == param)
            .map(|p| p.value)
    }

    /// Applies an edit: the one place the graph changes. Returns whether
    /// it did (an edit naming a node, port or parameter the file does
    /// not have, or a kind it does not declare, changes nothing).
    ///
    /// The text changes only where the edit lands: a moved node's `pos`,
    /// a parameter's value, the items of the nodes and wires deleted, new
    /// items after the last of their list. A part that is not in block
    /// style (a flow mapping or sequence) is written again whole, in
    /// block style, when an edit reaches into it; comments inside it are
    /// lost.
    ///
    /// Each edit is one gesture (a drag's `Move` comes at its end; a
    /// slider's frames come with `done: false`): an application that
    /// keeps an undo history records [`text`](Self::text) before an edit
    /// and reads it back to undo.
    pub fn apply(&mut self, edit: &Edit) -> bool {
        let Some(after) = self.changed(edit) else {
            return false;
        };
        if after == self.entries {
            return false;
        }
        let Ok(expected) = build(&after) else {
            return false;
        };
        let doc = {
            let t = Text::new(&self.text);
            let candidates: [&dyn Fn() -> Option<String>; 2] =
                [&|| self.splice(&t, edit, &after), &|| rewrite(&t, &after)];
            // Read back: the text must give the graph the edit makes.
            candidates.into_iter().find_map(|candidate| {
                YamlGraph::parse(&candidate()?)
                    .ok()
                    .filter(|doc| doc.built == expected)
            })
        };
        match doc {
            Some(doc) => {
                *self = doc;
                true
            }
            None => false,
        }
    }

    /// The entries once the edit is made; `None` when it names what is
    /// not there.
    fn changed(&self, edit: &Edit) -> Option<Entries> {
        let mut e = self.entries.clone();
        match edit {
            Edit::Move { moves } => {
                for m in moves {
                    let n = e.nodes.iter_mut().find(|n| n.id() == m.node)?;
                    if n.field("pos") != Some(&Field::Pos(m.to)) {
                        n.set("pos", Field::Pos(m.to));
                    }
                }
            }
            Edit::Param {
                node, param, to, ..
            } => {
                self.param(node, param)?;
                let n = e.nodes.iter_mut().find(|n| n.id() == node)?;
                let mut params = match n.field("params") {
                    Some(Field::Params(p)) => p.clone(),
                    _ => Vec::new(),
                };
                match params.iter_mut().find(|(id, _)| id == param) {
                    Some((_, ParamEntry::Value(v))) => *v = *to,
                    Some((_, ParamEntry::Full(f))) => set_json(f, "value", Json::from(*to)),
                    None => params.push((param.clone(), ParamEntry::Value(*to))),
                }
                n.set("params", Field::Params(params));
            }
            Edit::Delete { .. } | Edit::Disconnect { .. } | Edit::Connect { .. } => {
                e.nodes.retain(|n| !drops_node(edit, n.id()));
                e.edges.retain(|x| !drops_edge(edit, &x.entry));
                if let Edit::Connect { from, to, .. } = edit {
                    e.edges.push(EdgeItem {
                        entry: EdgeEntry::Text(edge_text(&Edge::new(from.clone(), to.clone()))),
                        at: At::default(),
                    });
                }
            }
            Edit::Add { nodes } => {
                for new in nodes {
                    if !e.kinds.iter().any(|(k, _)| *k == new.kind) {
                        continue;
                    }
                    let id = (1..)
                        .map(|k| format!("{}-{k}", new.kind))
                        .find(|id| !e.nodes.iter().any(|n| n.id() == id))
                        .unwrap_or_default();
                    let mut fields = vec![
                        ("id".to_string(), Field::Id(id)),
                        ("kind".to_string(), Field::Kind(new.kind.clone())),
                        ("pos".to_string(), Field::Pos(new.at)),
                    ];
                    let source = new
                        .copy_of
                        .as_ref()
                        .and_then(|c| e.nodes.iter().find(|n| n.id() == c));
                    if let Some(source) = source {
                        for key in ["group", "params"] {
                            if let Some(f) = source.field(key) {
                                fields.push((key.to_string(), f.clone()));
                            }
                        }
                    }
                    e.nodes.push(NodeEntry {
                        at: At::default(),
                        fields,
                    });
                }
            }
        }
        Some(e)
    }

    /// The text with the edit spliced in where it lands; `None` when the
    /// lines it reaches are not in block style.
    fn splice(&self, t: &Text<'_>, edit: &Edit, after: &Entries) -> Option<String> {
        let secs = sections(t).ok()?;
        let items = |key: &str, count: usize| -> Option<Vec<Entry>> {
            let v = seq_items(t, secs.iter().find(|s| s.key == key)?)?;
            (v.len() == count).then_some(v)
        };
        let nodes = &self.entries.nodes;
        let index = |id: &str| nodes.iter().position(|n| n.id() == id);
        let mut s = Splices::default();
        match edit {
            Edit::Move { moves } => {
                let items = items("nodes", nodes.len())?;
                for m in moves {
                    let i = index(&m.node)?;
                    splice_pos(t, &mut s, &items[i], &after.nodes[i], m.to)?;
                }
            }
            Edit::Param {
                node, param, to, ..
            } => {
                let items = items("nodes", nodes.len())?;
                let i = index(node)?;
                splice_param(t, &mut s, &items[i], &after.nodes[i], param, *to)?;
            }
            Edit::Delete { .. } | Edit::Disconnect { .. } | Edit::Connect { .. } => {
                let gone: Vec<usize> = (0..nodes.len())
                    .filter(|&i| drops_node(edit, nodes[i].id()))
                    .collect();
                if !gone.is_empty() {
                    let items = items("nodes", nodes.len())?;
                    for i in gone {
                        s.lines(t, items[i].lead, items[i].end, "");
                    }
                }
                let edges = &self.entries.edges;
                let gone: Vec<usize> = (0..edges.len())
                    .filter(|&i| drops_edge(edit, &edges[i].entry))
                    .collect();
                if !gone.is_empty() {
                    let items = items("edges", edges.len())?;
                    for i in gone {
                        s.lines(t, items[i].lead, items[i].end, "");
                    }
                }
                if let Edit::Connect { .. } = edit {
                    let new = vec![edge_inline(&after.edges.last()?.entry)];
                    append(t, &mut s, &secs, "edges", edges.len(), &[new])?;
                }
            }
            Edit::Add { .. } => {
                let new: Vec<Vec<String>> =
                    after.nodes[nodes.len()..].iter().map(node_lines).collect();
                append(t, &mut s, &secs, "nodes", nodes.len(), &new)?;
            }
        }
        Some(s.apply(t.src))
    }
}

impl GraphModel for YamlGraph {
    fn nodes(&self) -> Vec<Node> {
        self.built.nodes.clone()
    }

    fn edges(&self) -> Vec<Edge> {
        self.built.edges.clone()
    }

    fn groups(&self) -> Vec<Group> {
        self.built.groups.clone()
    }

    fn params(&self, node: &str) -> Vec<Param> {
        self.built
            .params
            .iter()
            .find(|(n, _)| n == node)
            .map(|(_, p)| p.clone())
            .unwrap_or_default()
    }

    fn kinds(&self) -> Vec<Kind> {
        self.built.kinds.clone()
    }
}

/// Whether the edit removes this node.
fn drops_node(edit: &Edit, id: &str) -> bool {
    matches!(edit, Edit::Delete { nodes, .. } if nodes.iter().any(|n| n == id))
}

/// Whether the edit removes this wire (a deleted node takes its wires).
fn drops_edge(edit: &Edit, e: &EdgeEntry) -> bool {
    let Some(ed) = edge_of(e) else {
        return false;
    };
    match edit {
        Edit::Delete { nodes, edges } => edges.contains(&ed) || nodes.iter().any(|n| ed.touches(n)),
        Edit::Disconnect { edges } => edges.contains(&ed),
        Edit::Connect { replaces, .. } => replaces.contains(&ed),
        _ => false,
    }
}

/// The node's item with its `pos` set.
fn splice_pos(
    t: &Text<'_>,
    s: &mut Splices,
    item: &Entry,
    after: &NodeEntry,
    to: [f32; 2],
) -> Option<()> {
    let Some(keys) = item_keys(t, item) else {
        rewrite_item(t, s, item, after);
        return Some(());
    };
    match keys.iter().find(|k| k.key == "pos") {
        Some(k)
            if k.entry.end == k.entry.start + 1
                && !t.inline_value(k.entry.start, k.col)?.is_empty() =>
        {
            let line = replace_value(t.content(k.entry.start), k.col, &pos(to))?;
            s.line(t, k.entry.start, line);
        }
        Some(k) => {
            let head = &t.content(k.entry.start)[..k.col];
            s.lines(
                t,
                k.entry.start,
                k.entry.end,
                &format!("{head}pos: {}\n", pos(to)),
            );
        }
        None => {
            let pad = " ".repeat(keys.first()?.col);
            s.insert(t, item.end, &format!("{pad}pos: {}\n", pos(to)));
        }
    }
    Some(())
}

/// The node's item with a parameter's value set.
fn splice_param(
    t: &Text<'_>,
    s: &mut Splices,
    item: &Entry,
    after: &NodeEntry,
    param: &str,
    to: f64,
) -> Option<()> {
    let entry = match after.field("params") {
        Some(Field::Params(ps)) => ps.iter().find(|(id, _)| id == param)?.1.clone(),
        _ => return None,
    };
    let Some(keys) = item_keys(t, item) else {
        rewrite_item(t, s, item, after);
        return Some(());
    };
    let Some(pk) = keys.iter().find(|k| k.key == "params") else {
        let col = keys.first()?.col;
        s.insert(
            t,
            item.end,
            &format!(
                "{}params:\n{}{}: {}\n",
                " ".repeat(col),
                " ".repeat(col + 2),
                string(param),
                param_inline(&entry)
            ),
        );
        return Some(());
    };
    let Some((col, subs)) = sub_keys(t, pk) else {
        rewrite_item(t, s, item, after);
        return Some(());
    };
    match subs.iter().find(|k| k.key == param) {
        None => {
            let pad = " ".repeat(col.unwrap_or(pk.col + 2));
            s.insert(
                t,
                pk.entry.end,
                &format!("{pad}{}: {}\n", string(param), param_inline(&entry)),
            );
        }
        Some(k) if k.entry.end == k.entry.start + 1 => {
            let line = replace_value(t.content(k.entry.start), k.col, &param_inline(&entry))?;
            s.line(t, k.entry.start, line);
        }
        Some(k) => match sub_keys(t, k) {
            Some((vcol, vkeys)) => match vkeys.iter().find(|v| v.key == "value") {
                Some(v) if v.entry.end == v.entry.start + 1 => {
                    let line = replace_value(t.content(v.entry.start), v.col, &num(to))?;
                    s.line(t, v.entry.start, line);
                }
                Some(_) => rewrite_item(t, s, item, after),
                None => {
                    let pad = " ".repeat(vcol.unwrap_or(k.col + 2));
                    s.insert(t, k.entry.end, &format!("{pad}value: {}\n", num(to)));
                }
            },
            None => rewrite_item(t, s, item, after),
        },
    }
    Some(())
}

/// A node's item written again whole, in block style.
fn rewrite_item(t: &Text<'_>, s: &mut Splices, item: &Entry, node: &NodeEntry) {
    let block = items_block(t.indent(item.start), &[node_lines(node)]);
    s.lines(t, item.start, item.end, &block);
}

/// New items after the last of a section's list (`count` items there
/// now), the section written when the list is empty or missing.
fn append(
    t: &Text<'_>,
    s: &mut Splices,
    secs: &[text::Section],
    key: &str,
    count: usize,
    new: &[Vec<String>],
) -> Option<()> {
    if new.is_empty() {
        return Some(());
    }
    let Some(sec) = secs.iter().find(|x| x.key == key) else {
        s.insert(
            t,
            t.lines.len(),
            &format!("{key}:\n{}", items_block(2, new)),
        );
        return Some(());
    };
    let items = seq_items(t, sec)?;
    if items.len() != count {
        return None;
    }
    match items.last() {
        Some(last) => s.insert(t, last.end, &items_block(t.indent(last.start), new)),
        None => {
            let c = t.content(sec.header);
            let comment = comment_start(c)
                .map(|i| format!(" {}", &c[i..]))
                .unwrap_or_default();
            s.lines(
                t,
                sec.header,
                sec.header + 1,
                &format!("{key}:{comment}\n{}", items_block(2, new)),
            );
        }
    }
    Some(())
}

/// The text with its `nodes` and `edges` written again whole, in block
/// style: what an edit falls back to when its splice does not read back
/// as it should. Comments inside these two lists are lost; the rest of
/// the file is kept.
fn rewrite(t: &Text<'_>, after: &Entries) -> Option<String> {
    let secs = sections(t).ok()?;
    let nodes: Vec<Vec<String>> = after.nodes.iter().map(node_lines).collect();
    let edges: Vec<Vec<String>> = after
        .edges
        .iter()
        .map(|e| vec![edge_inline(&e.entry)])
        .collect();
    let mut s = Splices::default();
    for (key, items) in [("nodes", nodes), ("edges", edges)] {
        let block = if items.is_empty() {
            format!("{key}: []\n")
        } else {
            format!("{key}:\n{}", items_block(2, &items))
        };
        match secs.iter().find(|x| x.key == key) {
            Some(sec) => {
                let end = t
                    .content_end(sec.header + 1, sec.end)
                    .unwrap_or(sec.header + 1);
                s.lines(t, sec.header, end, &block);
            }
            None => s.insert(t, t.lines.len(), &block),
        }
    }
    Some(s.apply(t.src))
}

fn set_json(fields: &mut Vec<(String, Json)>, key: &str, value: Json) {
    match fields.iter_mut().find(|(k, _)| k == key) {
        Some((_, v)) => *v = value,
        None => fields.push((key.to_string(), value)),
    }
}

fn get<'a>(fields: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// `node.port`: the port's id after the last dot.
fn port_ref(s: &str) -> Option<PortRef> {
    let (node, port) = s.trim().rsplit_once('.')?;
    let (node, port) = (node.trim(), port.trim());
    (!node.is_empty() && !port.is_empty()).then(|| PortRef::new(node, port))
}

/// The wire an entry names, when it is written as one.
fn edge_of(e: &EdgeEntry) -> Option<Edge> {
    match e {
        EdgeEntry::Text(s) => {
            let (from, to) = s.split_once("->")?;
            Some(Edge::new(port_ref(from)?, port_ref(to)?))
        }
        EdgeEntry::Map(f) => Some(Edge::new(
            port_ref(get(f, "from")?.as_str()?)?,
            port_ref(get(f, "to")?.as_str()?)?,
        )),
    }
}

fn edge_text(e: &Edge) -> String {
    format!(
        "{}.{} -> {}.{}",
        e.from.node, e.from.port, e.to.node, e.to.port
    )
}

/// A port's or a parameter's label when the file gives none: its id,
/// capitalised.
fn label_of(id: &str) -> String {
    let mut c = id.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// The stamp's version.
fn version(extensions: Option<&Json>) -> Result<u64, YamlGraphError> {
    let Some(stamp) = extensions.and_then(|x| x.get(FORMAT_EXTENSION)) else {
        return Ok(FORMAT_VERSION);
    };
    stamp
        .get("version")
        .and_then(Json::as_u64)
        .filter(|v| *v > 0)
        .ok_or_else(|| YamlGraphError {
            line: None,
            column: None,
            message: format!(
                "`$extensions: {FORMAT_EXTENSION}` needs a `version`, a whole number from 1"
            ),
        })
}

fn port(node: &str, id: &str, p: &PortEntry) -> Result<Port, String> {
    let f = match p {
        PortEntry::Type(t) => return Ok(Port::new(id, label_of(id), PortType::new(t.as_str()))),
        PortEntry::Full(f) => f,
    };
    let text = |key: &str| -> Result<Option<&str>, String> {
        get(f, key)
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| format!("node `{node}`: port `{id}`: `{key}` is not a string"))
            })
            .transpose()
    };
    let ty = text("type")?.ok_or_else(|| format!("node `{node}`: port `{id}` needs a `type`"))?;
    let mut ty = PortType::new(ty);
    if let Some(c) = text("color")? {
        ty = ty.color(c);
    }
    let label = text("label")?.map_or_else(|| label_of(id), str::to_string);
    let mut port = Port::new(id, label, ty);
    match get(f, "multiple") {
        None | Some(Json::Bool(false)) => {}
        Some(Json::Bool(true)) => port = port.multiple(),
        Some(_) => {
            return Err(format!(
                "node `{node}`: port `{id}`: `multiple` is not true or false"
            ));
        }
    }
    Ok(port)
}

/// A kind's ports with a node's own: a node's port of the same id
/// replaces the kind's, others come after.
fn ports(
    node: &str,
    base: &[(String, PortEntry)],
    own: Option<&Field>,
) -> Result<Vec<Port>, String> {
    let mut all: Vec<(String, PortEntry)> = base.to_vec();
    if let Some(Field::Inputs(own) | Field::Outputs(own)) = own {
        for (id, p) in own {
            match all.iter_mut().find(|(k, _)| k == id) {
                Some((_, slot)) => *slot = p.clone(),
                None => all.push((id.clone(), p.clone())),
            }
        }
    }
    all.iter().map(|(id, p)| port(node, id, p)).collect()
}

fn param_fields(p: &ParamEntry) -> Vec<(String, Json)> {
    match p {
        ParamEntry::Value(v) => vec![("value".to_string(), Json::from(*v))],
        ParamEntry::Full(f) => f.clone(),
    }
}

/// A kind's parameters with a node's: a node's fields over the kind's
/// (a number alone sets the value), its own parameters after. The
/// kind's value is the default a double click goes back to.
fn params(
    node: &str,
    base: &[(String, ParamEntry)],
    own: Option<&Field>,
) -> Result<Vec<Param>, String> {
    // Each parameter's fields, and the kind's value.
    type Merged = (String, Vec<(String, Json)>, Option<f64>);
    let mut all: Vec<Merged> = base
        .iter()
        .map(|(id, p)| {
            let fields = param_fields(p);
            let value = get(&fields, "value").and_then(Json::as_f64);
            (id.clone(), fields, value)
        })
        .collect();
    if let Some(Field::Params(own)) = own {
        for (id, p) in own {
            match all.iter_mut().find(|(k, _, _)| k == id) {
                Some((_, fields, _)) => {
                    for (k, v) in param_fields(p) {
                        set_json(fields, &k, v);
                    }
                }
                None => all.push((id.clone(), param_fields(p), None)),
            }
        }
    }
    all.iter()
        .map(|(id, f, kind_value)| {
            let number = |key: &str| -> Result<Option<f64>, String> {
                get(f, key)
                    .map(|v| {
                        v.as_f64().ok_or_else(|| {
                            format!("node `{node}`: parameter `{id}`: `{key}` is not a number")
                        })
                    })
                    .transpose()
            };
            let text = |key: &str| -> Result<Option<&str>, String> {
                get(f, key)
                    .map(|v| {
                        v.as_str().ok_or_else(|| {
                            format!("node `{node}`: parameter `{id}`: `{key}` is not a string")
                        })
                    })
                    .transpose()
            };
            let (Some(min), Some(max)) = (number("min")?, number("max")?) else {
                return Err(format!(
                    "node `{node}`: parameter `{id}` needs a `min` and a `max`"
                ));
            };
            if min > max {
                return Err(format!(
                    "node `{node}`: parameter `{id}`: `min` is above `max`"
                ));
            }
            let default = number("default")?.or(*kind_value);
            let value = number("value")?.or(default).unwrap_or(min);
            let label = text("label")?.map_or_else(|| label_of(id), str::to_string);
            let mut p = Param::new(id.as_str(), label, value, min..=max);
            if let Some(step) = number("step")? {
                p = p.step(step);
            }
            if let Some(unit) = text("unit")? {
                p = p.unit(unit);
            }
            if let Some(d) = default {
                p = p.default(d);
            }
            Ok(p)
        })
        .collect()
}

/// The graph the entries describe, or the first thing wrong in them,
/// with where it was written.
fn build(e: &Entries) -> Result<Built, YamlGraphError> {
    let groups: Vec<Group> = e
        .groups
        .iter()
        .map(|(id, g)| Group::new(id.as_str(), g.0.clone().unwrap_or_else(|| id.clone())))
        .collect();
    let kinds: Vec<Kind> = e
        .kinds
        .iter()
        .map(|(id, k)| Kind::new(id.as_str(), k.title.clone().unwrap_or_else(|| id.clone())))
        .collect();
    let mut seen = HashSet::new();
    let mut nodes = Vec::with_capacity(e.nodes.len());
    let mut all_params = Vec::with_capacity(e.nodes.len());
    let empty = KindEntry::default();
    for n in &e.nodes {
        let id = n.id();
        let err = |message: String| YamlGraphError::at(n.at, message);
        if id.is_empty() {
            return Err(err("a node's `id` is empty".into()));
        }
        if !seen.insert(id) {
            return Err(err(format!("a second node `{id}`")));
        }
        let (kind_id, kind) = match n.field("kind") {
            Some(Field::Kind(k)) => {
                let kind = e
                    .kinds
                    .iter()
                    .find(|(id, _)| id == k)
                    .map(|(_, d)| d)
                    .ok_or_else(|| err(format!("node `{id}`: no kind `{k}` in `kinds`")))?;
                (k.as_str(), kind)
            }
            _ => ("", &empty),
        };
        let title = match n.field("title") {
            Some(Field::Title(t)) => t.clone(),
            _ => kind.title.clone().unwrap_or_else(|| id.to_string()),
        };
        let at = match n.field("pos") {
            Some(Field::Pos(p)) => *p,
            _ => [0.0, 0.0],
        };
        let mut node = Node::new(id, kind_id, title, at);
        if let Some(Field::Group(g)) = n.field("group") {
            if !e.groups.iter().any(|(id, _)| id == g) {
                return Err(err(format!("node `{id}`: no group `{g}` in `groups`")));
            }
            node = node.group(g.as_str());
        }
        node.inputs = ports(id, &kind.inputs.0, n.field("inputs")).map_err(err)?;
        node.outputs = ports(id, &kind.outputs.0, n.field("outputs")).map_err(err)?;
        all_params.push((
            id.to_string(),
            params(id, &kind.params.0, n.field("params")).map_err(err)?,
        ));
        nodes.push(node);
    }
    let mut edges = Vec::with_capacity(e.edges.len());
    for item in &e.edges {
        let err = |message: String| YamlGraphError::at(item.at, message);
        let edge = edge_of(&item.entry).ok_or_else(|| {
            err("a wire is `node.port -> node.port`, or a mapping with `from` and `to`".into())
        })?;
        let name = edge_text(&edge);
        let end = |r: &PortRef, output: bool| -> Result<(), YamlGraphError> {
            let node = nodes
                .iter()
                .find(|n| n.id == r.node)
                .ok_or_else(|| err(format!("wire `{name}`: no node `{}`", r.node)))?;
            let ports = if output { &node.outputs } else { &node.inputs };
            if ports.iter().any(|p| p.id == r.port) {
                Ok(())
            } else {
                let side = if output { "output" } else { "input" };
                Err(err(format!(
                    "wire `{name}`: no {side} `{}` on node `{}`",
                    r.port, r.node
                )))
            }
        };
        end(&edge.from, true)?;
        end(&edge.to, false)?;
        edges.push(edge);
    }
    Ok(Built {
        nodes,
        edges,
        params: all_params,
        groups,
        kinds,
    })
}
