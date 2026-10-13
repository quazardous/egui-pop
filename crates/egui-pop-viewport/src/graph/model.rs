//! The graph an application supplies ([`GraphModel`]), and what the
//! editor hands back: the [`Edit`]s it proposes, the [`Request`]s it
//! takes from outside and the [`GraphEvent`]s it reports.
//!
//! Identifiers are strings the application chooses: a node's `id`, a
//! port's `id` within its node, a group's `id`, a kind's `id`. They are
//! what steps, commands and the debug bridge show, so they stay readable.

use std::ops::RangeInclusive;

use serde_json::{Value as Json, json};

/// A port of a node: a node's id and the port's id within it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortRef {
    pub node: String,
    pub port: String,
}

impl PortRef {
    pub fn new(node: impl Into<String>, port: impl Into<String>) -> PortRef {
        PortRef {
            node: node.into(),
            port: port.into(),
        }
    }

    /// `{"node": ..., "port": ...}`.
    pub fn to_json(&self) -> Json {
        json!({ "node": self.node, "port": self.port })
    }

    pub fn from_json(v: &Json) -> Option<PortRef> {
        Some(PortRef::new(
            v.get("node")?.as_str()?,
            v.get("port")?.as_str()?,
        ))
    }
}

/// A wire, from an output to an input.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Edge {
    pub from: PortRef,
    pub to: PortRef,
}

impl Edge {
    pub fn new(from: PortRef, to: PortRef) -> Edge {
        Edge { from, to }
    }

    /// `{"from": {node, port}, "to": {node, port}}`.
    pub fn to_json(&self) -> Json {
        json!({ "from": self.from.to_json(), "to": self.to.to_json() })
    }

    pub fn from_json(v: &Json) -> Option<Edge> {
        Some(Edge::new(
            PortRef::from_json(v.get("from")?)?,
            PortRef::from_json(v.get("to")?)?,
        ))
    }

    /// Whether the wire touches a node.
    pub fn touches(&self, node: &str) -> bool {
        self.from.node == node || self.to.node == node
    }
}

/// The colour tokens a port type takes by default, one picked from its
/// name (`graph.type-1` to `graph.type-6`).
pub const TYPE_TOKENS: [&str; 6] = [
    "graph.type-1",
    "graph.type-2",
    "graph.type-3",
    "graph.type-4",
    "graph.type-5",
    "graph.type-6",
];

/// What flows through a port (`image`, `mask`, `number`), and the token
/// its ports and wires are drawn with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortType {
    pub name: String,
    /// A colour token's path (`graph.type-2`, or one of the
    /// application's own tokens).
    pub color: String,
}

impl PortType {
    /// A type coloured by one of [`TYPE_TOKENS`], the same for the same
    /// name.
    pub fn new(name: impl Into<String>) -> PortType {
        let name = name.into();
        let h = name
            .bytes()
            .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
        let color = TYPE_TOKENS[(h % TYPE_TOKENS.len() as u32) as usize].to_string();
        PortType { name, color }
    }

    /// Drawn with this colour token instead.
    pub fn color(mut self, token: impl Into<String>) -> PortType {
        self.color = token.into();
        self
    }
}

/// Which side of a node a port is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Side {
    Input,
    Output,
}

impl Side {
    pub fn name(self) -> &'static str {
        match self {
            Side::Input => "input",
            Side::Output => "output",
        }
    }
}

/// A port: where a wire ends (an input) or starts (an output).
#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    pub id: String,
    pub label: String,
    pub ty: PortType,
    /// An input taking several wires. By default an input takes one,
    /// and a new wire replaces the old one; an output always feeds any
    /// number.
    pub multiple: bool,
}

impl Port {
    pub fn new(id: impl Into<String>, label: impl Into<String>, ty: PortType) -> Port {
        Port {
            id: id.into(),
            label: label.into(),
            ty,
            multiple: false,
        }
    }

    /// An input taking several wires.
    pub fn multiple(mut self) -> Port {
        self.multiple = true;
        self
    }
}

/// A node as the application describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: String,
    /// What it is (`blur`, `load`): what a duplicate or an addition
    /// names.
    pub kind: String,
    pub title: String,
    /// Its top-left corner, in graph units (points at 100 %).
    pub pos: [f32; 2],
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    /// The group it belongs to.
    pub group: Option<String>,
}

impl Node {
    pub fn new(
        id: impl Into<String>,
        kind: impl Into<String>,
        title: impl Into<String>,
        pos: [f32; 2],
    ) -> Node {
        Node {
            id: id.into(),
            kind: kind.into(),
            title: title.into(),
            pos,
            inputs: Vec::new(),
            outputs: Vec::new(),
            group: None,
        }
    }

    pub fn input(mut self, port: Port) -> Node {
        self.inputs.push(port);
        self
    }

    pub fn output(mut self, port: Port) -> Node {
        self.outputs.push(port);
        self
    }

    pub fn group(mut self, group: impl Into<String>) -> Node {
        self.group = Some(group.into());
        self
    }

    /// A port by side and id.
    pub fn port(&self, side: Side, id: &str) -> Option<&Port> {
        match side {
            Side::Input => &self.inputs,
            Side::Output => &self.outputs,
        }
        .iter()
        .find(|p| p.id == id)
    }
}

/// A group of nodes: drawn as a frame around them, or folded into a
/// stack.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: String,
    pub label: String,
}

impl Group {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Group {
        Group {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// A node's parameter, edited inline with a slider in the detail view.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub id: String,
    pub label: String,
    pub value: f64,
    /// What the bar spans.
    pub range: RangeInclusive<f64>,
    /// The arrows' step (and the precision shown).
    pub step: Option<f64>,
    pub unit: Option<String>,
    /// What a double click goes back to.
    pub default: Option<f64>,
}

impl Param {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        value: f64,
        range: RangeInclusive<f64>,
    ) -> Param {
        Param {
            id: id.into(),
            label: label.into(),
            value,
            range,
            step: None,
            unit: None,
            default: None,
        }
    }

    pub fn step(mut self, step: f64) -> Param {
        self.step = Some(step);
        self
    }

    pub fn unit(mut self, unit: impl Into<String>) -> Param {
        self.unit = Some(unit.into());
        self
    }

    pub fn default(mut self, default: f64) -> Param {
        self.default = Some(default);
        self
    }
}

/// A kind of node the user may add (the canvas's menu).
#[derive(Clone, Debug, PartialEq)]
pub struct Kind {
    pub id: String,
    pub title: String,
}

impl Kind {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Kind {
        Kind {
            id: id.into(),
            title: title.into(),
        }
    }
}

/// Why a wire is refused.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// The output's type is not the input's.
    Types { from: String, to: String },
    /// A node cannot feed itself.
    SameNode,
    /// Two inputs, or two outputs.
    SameSide,
    /// The wire is there already (nothing to do; never reported).
    Exists,
    /// The application's own reason, shown as is.
    Other(String),
}

/// The default type check: an output feeds an input of the same type,
/// on another node.
pub fn check(from: (&Node, &Port), to: (&Node, &Port)) -> Result<(), Refusal> {
    if from.0.id == to.0.id {
        return Err(Refusal::SameNode);
    }
    if from.1.ty.name != to.1.ty.name {
        return Err(Refusal::Types {
            from: from.1.ty.name.clone(),
            to: to.1.ty.name.clone(),
        });
    }
    Ok(())
}

/// The graph, as the application keeps it. The editor reads it each
/// frame and never changes it: what the user does comes back as
/// [`Edit`]s for the application to apply.
pub trait GraphModel {
    /// Every node, in drawing order (the last on top).
    fn nodes(&self) -> Vec<Node>;

    /// Every wire.
    fn edges(&self) -> Vec<Edge>;

    /// The groups nodes name.
    fn groups(&self) -> Vec<Group> {
        Vec::new()
    }

    /// A node's parameters, shown in the detail view.
    fn params(&self, _node: &str) -> Vec<Param> {
        Vec::new()
    }

    /// The kinds of node the canvas's menu offers to add.
    fn kinds(&self) -> Vec<Kind> {
        Vec::new()
    }

    /// Whether an output may feed an input; by default [`check`] (same
    /// type, another node).
    fn can_connect(&self, from: (&Node, &Port), to: (&Node, &Port)) -> Result<(), Refusal> {
        check(from, to)
    }
}

/// A node moved: where it was and where it goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Moved {
    pub node: String,
    pub from: [f32; 2],
    pub to: [f32; 2],
}

/// A node to add: its kind and place, or a copy of another node.
#[derive(Clone, Debug, PartialEq)]
pub struct NewNode {
    pub kind: String,
    pub at: [f32; 2],
    /// The node it duplicates (with its parameters), if any.
    pub copy_of: Option<String>,
}

/// What the editor asks the application to do to its graph.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    /// A wire from an output to an input; `replaces` are the wires it
    /// takes the place of (the input's old wire, or the wire dragged off
    /// another input), to remove first.
    Connect {
        from: PortRef,
        to: PortRef,
        replaces: Vec<Edge>,
    },
    /// Wires to remove.
    Disconnect { edges: Vec<Edge> },
    /// Nodes moved (one gesture).
    Move { moves: Vec<Moved> },
    /// Nodes to delete, with the wires touching them.
    Delete {
        nodes: Vec<String>,
        edges: Vec<Edge>,
    },
    /// Nodes to add (from the menu, a command, or a duplicate).
    Add { nodes: Vec<NewNode> },
    /// A parameter's slider moved: a drag's frames with `done: false`,
    /// its end once more from the value before it with `done: true`.
    Param {
        node: String,
        param: String,
        from: f64,
        to: f64,
        done: bool,
    },
}

fn pair(p: [f32; 2]) -> Json {
    json!([p[0], p[1]])
}

fn read_pair(v: &Json) -> Option<[f32; 2]> {
    Some([v.get(0)?.as_f64()? as f32, v.get(1)?.as_f64()? as f32])
}

fn edges_json(edges: &[Edge]) -> Json {
    Json::Array(edges.iter().map(Edge::to_json).collect())
}

fn read_edges(v: Option<&Json>) -> Option<Vec<Edge>> {
    match v {
        None | Some(Json::Null) => Some(Vec::new()),
        Some(v) => v.as_array()?.iter().map(Edge::from_json).collect(),
    }
}

fn read_strings(v: &Json) -> Option<Vec<String>> {
    v.as_array()?
        .iter()
        .map(|s| s.as_str().map(str::to_string))
        .collect()
}

impl Edit {
    /// Its name in steps and commands: `connect`, `disconnect`, `move`,
    /// `delete`, `add`, `param`.
    pub fn kind(&self) -> &'static str {
        match self {
            Edit::Connect { .. } => "connect",
            Edit::Disconnect { .. } => "disconnect",
            Edit::Move { .. } => "move",
            Edit::Delete { .. } => "delete",
            Edit::Add { .. } => "add",
            Edit::Param { .. } => "param",
        }
    }

    /// Every edit but a slider's drag frames is a finished gesture.
    pub fn done(&self) -> bool {
        match self {
            Edit::Param { done, .. } => *done,
            _ => true,
        }
    }

    /// Its fields as a JSON object (a step's arguments).
    pub fn to_json(&self) -> Json {
        match self {
            Edit::Connect { from, to, replaces } => json!({
                "from": from.to_json(),
                "to": to.to_json(),
                "replaces": edges_json(replaces),
            }),
            Edit::Disconnect { edges } => json!({ "edges": edges_json(edges) }),
            Edit::Move { moves } => json!({
                "moves": moves
                    .iter()
                    .map(|m| json!({ "node": m.node, "from": pair(m.from), "to": pair(m.to) }))
                    .collect::<Vec<_>>(),
            }),
            Edit::Delete { nodes, edges } => json!({
                "nodes": nodes,
                "edges": edges_json(edges),
            }),
            Edit::Add { nodes } => json!({
                "nodes": nodes
                    .iter()
                    .map(|n| json!({ "kind": n.kind, "at": pair(n.at), "copy_of": n.copy_of }))
                    .collect::<Vec<_>>(),
            }),
            Edit::Param {
                node,
                param,
                from,
                to,
                done,
            } => json!({
                "node": node,
                "param": param,
                "from": from,
                "to": to,
                "done": done,
            }),
        }
    }

    /// An edit of `kind` read back from its fields (a redirect, a
    /// command's arguments); `None` when they do not make one.
    pub fn from_json(kind: &str, v: &Json) -> Option<Edit> {
        Some(match kind {
            "connect" => Edit::Connect {
                from: PortRef::from_json(v.get("from")?)?,
                to: PortRef::from_json(v.get("to")?)?,
                replaces: read_edges(v.get("replaces"))?,
            },
            "disconnect" => Edit::Disconnect {
                edges: read_edges(Some(v.get("edges")?))?,
            },
            "move" => Edit::Move {
                moves: v
                    .get("moves")?
                    .as_array()?
                    .iter()
                    .map(|m| {
                        let to = read_pair(m.get("to")?)?;
                        Some(Moved {
                            node: m.get("node")?.as_str()?.to_string(),
                            from: m.get("from").and_then(read_pair).unwrap_or(to),
                            to,
                        })
                    })
                    .collect::<Option<_>>()?,
            },
            "delete" => Edit::Delete {
                nodes: read_strings(v.get("nodes")?)?,
                edges: read_edges(v.get("edges"))?,
            },
            "add" => Edit::Add {
                nodes: v
                    .get("nodes")?
                    .as_array()?
                    .iter()
                    .map(|n| {
                        Some(NewNode {
                            kind: n.get("kind")?.as_str()?.to_string(),
                            at: n.get("at").and_then(read_pair).unwrap_or([0.0, 0.0]),
                            copy_of: n.get("copy_of").and_then(Json::as_str).map(str::to_string),
                        })
                    })
                    .collect::<Option<_>>()?,
            },
            "param" => Edit::Param {
                node: v.get("node")?.as_str()?.to_string(),
                param: v.get("param")?.as_str()?.to_string(),
                from: v.get("from")?.as_f64()?,
                to: v.get("to")?.as_f64()?,
                done: v.get("done").and_then(Json::as_bool).unwrap_or(true),
            },
            _ => return None,
        })
    }
}

/// How much the editor shows, from the zoom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Level {
    /// Zoomed out: compact nodes with their titles, groups folded into
    /// stacks, wires between nodes.
    Overview,
    /// Nodes with their ports, wires between ports.
    Ports,
    /// Zoomed in: also the parameters, edited inline.
    Detail,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Overview => "overview",
            Level::Ports => "ports",
            Level::Detail => "detail",
        }
    }
}

/// What the editor reports besides edits.
#[derive(Clone, Debug, PartialEq)]
pub enum GraphEvent {
    /// The selected nodes changed (now these).
    Selection(Vec<String>),
    /// The zoom crossed into another level.
    Level(Level),
    /// A wire was refused before any edit: the ports' types differ, the
    /// node would feed itself, or the application said no.
    Refused {
        from: PortRef,
        to: PortRef,
        reason: String,
    },
}

/// What an application (or an agent, through a command) asks of the
/// editor, done at its next frame as if the user had done it.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Propose an edit: a connection is type-checked, then the edit goes
    /// through the hook like the user's.
    Edit(Edit),
    /// Select these nodes.
    Select(Vec<String>),
    /// Fit the whole graph in view.
    Fit,
    /// Zoom to show a level around the view's centre.
    Level(Level),
    /// Give a node the keyboard focus (and bring it into view).
    Focus(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(n: &str, q: &str) -> PortRef {
        PortRef::new(n, q)
    }

    #[test]
    fn edits_round_trip_through_json() {
        let edits = [
            Edit::Connect {
                from: p("load", "image"),
                to: p("blur", "image"),
                replaces: vec![Edge::new(p("old", "image"), p("blur", "image"))],
            },
            Edit::Disconnect {
                edges: vec![Edge::new(p("a", "out"), p("b", "in"))],
            },
            Edit::Move {
                moves: vec![Moved {
                    node: "a".into(),
                    from: [10.0, 20.5],
                    to: [40.0, -3.25],
                }],
            },
            Edit::Delete {
                nodes: vec!["a".into()],
                edges: vec![Edge::new(p("a", "out"), p("b", "in"))],
            },
            Edit::Add {
                nodes: vec![NewNode {
                    kind: "blur".into(),
                    at: [1.0, 2.0],
                    copy_of: Some("blur-1".into()),
                }],
            },
            Edit::Param {
                node: "blur".into(),
                param: "radius".into(),
                from: 1.0,
                to: 2.5,
                done: true,
            },
        ];
        for e in edits {
            assert_eq!(Edit::from_json(e.kind(), &e.to_json()), Some(e.clone()));
        }
        assert_eq!(Edit::from_json("connect", &json!({})), None);
        assert_eq!(Edit::from_json("nothing", &json!({})), None);
    }

    #[test]
    fn the_default_check_wants_one_type_and_two_nodes() {
        let img = PortType::new("image");
        let mask = PortType::new("mask");
        let a = Node::new("a", "k", "A", [0.0, 0.0]);
        let b = Node::new("b", "k", "B", [0.0, 0.0]);
        let out = Port::new("out", "Out", img.clone());
        let input = Port::new("in", "In", img);
        let m = Port::new("m", "Mask", mask);
        assert_eq!(check((&a, &out), (&b, &input)), Ok(()));
        assert_eq!(check((&a, &out), (&a, &input)), Err(Refusal::SameNode));
        assert!(matches!(
            check((&a, &out), (&b, &m)),
            Err(Refusal::Types { .. })
        ));
        // A type's colour is stable.
        assert_eq!(PortType::new("image").color, PortType::new("image").color);
    }
}
