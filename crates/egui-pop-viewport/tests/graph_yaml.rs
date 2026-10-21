//! The reference graph model kept in a YAML file: what it reads, that an
//! untouched file comes back byte for byte, that an edit changes only
//! the lines it lands on, and where a malformed file is wrong.

#![cfg(feature = "graph-yaml")]

use egui_pop_viewport::graph::yaml::{FORMAT_VERSION, YamlGraph};
use egui_pop_viewport::graph::{Edge, Edit, GraphModel, Moved, NewNode, PortRef};

const PIPELINE: &str = include_str!("graph-yaml/pipeline.yaml");

fn graph() -> YamlGraph {
    YamlGraph::parse(PIPELINE).expect("the fixture reads")
}

fn wire(from: &str, to: &str) -> Edge {
    let r = |s: &str| {
        let (n, p) = s.split_once('.').unwrap();
        PortRef::new(n, p)
    };
    Edge::new(r(from), r(to))
}

fn moved(node: &str, to: [f32; 2]) -> Edit {
    Edit::Move {
        moves: vec![Moved {
            node: node.into(),
            from: [0.0, 0.0],
            to,
        }],
    }
}

fn param(node: &str, param: &str, to: f64) -> Edit {
    Edit::Param {
        node: node.into(),
        param: param.into(),
        from: 0.0,
        to,
        done: true,
    }
}

/// The fixture with `from` replaced by `to`, once.
fn fixture_with(from: &str, to: &str) -> String {
    assert_eq!(PIPELINE.matches(from).count(), 1, "{from:?}");
    PIPELINE.replacen(from, to, 1)
}

#[test]
fn an_untouched_file_comes_back_byte_for_byte() {
    assert_eq!(graph().text(), PIPELINE);
    let crlf = PIPELINE.replace('\n', "\r\n");
    assert_eq!(YamlGraph::parse(&crlf).unwrap().text(), crlf);
    let bare = PIPELINE.trim_end();
    assert_eq!(YamlGraph::parse(bare).unwrap().text(), bare);
}

#[test]
fn the_file_reads_as_a_graph() {
    let g = graph();
    assert_eq!(g.version(), FORMAT_VERSION);
    assert!(g.warnings().is_empty());
    let nodes = g.nodes();
    let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["load", "blur", "mix", "output"]);
    // A title from the kind, or the node's own.
    assert_eq!(nodes[0].title, "Load");
    assert_eq!(nodes[1].title, "Soft blur");
    assert_eq!(nodes[1].pos, [240.0, 0.0]);
    assert_eq!(nodes[1].group.as_deref(), Some("detail"));
    // Ports from the kind, in their order; a label from the id.
    let mix = &nodes[2];
    let inputs: Vec<&str> = mix.inputs.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(inputs, ["base", "top", "mask"]);
    assert_eq!(mix.inputs[0].label, "Base");
    assert_eq!(mix.inputs[2].ty.name, "mask");
    assert_eq!(mix.inputs[2].label, "Mask");
    assert!(nodes[3].inputs[0].multiple);
    assert!(!mix.inputs[0].multiple);
    // A node's value over the kind's, whose value is the default.
    let radius = &g.params("blur")[0];
    assert_eq!(radius.label, "Radius");
    assert_eq!(radius.value, 3.5);
    assert_eq!(radius.default, Some(2.0));
    assert_eq!(radius.range, 0.0..=20.0);
    assert_eq!(radius.step, Some(0.1));
    assert_eq!(radius.unit.as_deref(), Some("px"));
    assert_eq!(g.param("mix", "opacity"), Some(0.8));
    assert!(g.params("load").is_empty());
    assert_eq!(
        g.edges(),
        [
            wire("load.image", "blur.image"),
            wire("blur.image", "mix.base"),
            wire("load.image", "mix.top"),
            wire("mix.image", "output.image"),
        ]
    );
    let groups: Vec<(String, String)> = g.groups().into_iter().map(|g| (g.id, g.label)).collect();
    assert_eq!(
        groups,
        [
            ("detail".to_string(), "Detail".to_string()),
            ("tone".to_string(), "Tone".to_string())
        ]
    );
    let kinds: Vec<String> = g.kinds().into_iter().map(|k| k.title).collect();
    assert_eq!(kinds, ["Load", "Blur", "Blend", "Output"]);
}

#[test]
fn a_move_changes_only_the_pos() {
    let mut g = graph();
    assert!(g.apply(&moved("blur", [260.0, 20.0])));
    assert_eq!(
        g.text(),
        fixture_with("    pos: [240, 0]\n", "    pos: [260, 20]\n")
    );
    assert_eq!(g.nodes()[1].pos, [260.0, 20.0]);
    // The comment after the value stays.
    let mut g = graph();
    assert!(g.apply(&moved("load", [20.0, -12.5])));
    assert_eq!(
        g.text(),
        fixture_with("pos: [0, 0]   # top left", "pos: [20, -12.5]   # top left")
    );
    // Where it already is: nothing changes.
    assert!(!g.apply(&moved("load", [20.0, -12.5])));
}

#[test]
fn a_parameter_changes_only_its_value() {
    let mut g = graph();
    assert!(g.apply(&param("blur", "radius", 4.0)));
    assert_eq!(
        g.text(),
        fixture_with("radius: 3.5 # softer", "radius: 4 # softer")
    );
    let mut g = graph();
    assert!(g.apply(&param("mix", "opacity", 0.5)));
    assert_eq!(
        g.text(),
        fixture_with("        value: 0.8\n", "        value: 0.5\n")
    );
    assert_eq!(g.param("mix", "opacity"), Some(0.5));
    // A slider's frames go through the same path.
    let mut g = graph();
    assert!(g.apply(&Edit::Param {
        node: "blur".into(),
        param: "radius".into(),
        from: 3.5,
        to: 3.6,
        done: false,
    }));
    assert_eq!(g.param("blur", "radius"), Some(3.6));
}

#[test]
fn deleting_a_node_takes_its_item_its_comment_and_its_wires() {
    let mut g = graph();
    assert!(g.apply(&Edit::Delete {
        nodes: vec!["blur".into()],
        edges: vec![
            wire("load.image", "blur.image"),
            wire("blur.image", "mix.base")
        ],
    }));
    let expected = PIPELINE
        .replacen(
            "  - id: blur\n    kind: blur\n    title: Soft blur\n    pos: [240, 0]\n    group: detail\n    params:\n      radius: 3.5 # softer\n",
            "",
            1,
        )
        .replacen("  - load.image -> blur.image\n", "", 1)
        .replacen("  - blur.image -> mix.base\n", "", 1);
    assert_eq!(g.text(), expected);
    assert_eq!(g.nodes().len(), 3);
    assert_eq!(g.edges().len(), 2);

    // The comment right above an item goes with it.
    let mut g = graph();
    assert!(g.apply(&Edit::Delete {
        nodes: vec!["load".into()],
        edges: Vec::new(),
    }));
    let expected = PIPELINE
        .replacen(
            "  # The source.\n  - id: load\n    kind: load\n    pos: [0, 0]   # top left\n    note: \"kept as written\"\n",
            "",
            1,
        )
        .replacen("  - load.image -> blur.image\n", "", 1)
        .replacen("  - {from: load.image, to: mix.top, color: accent}\n", "", 1);
    assert_eq!(g.text(), expected);
}

#[test]
fn connecting_appends_a_wire_and_drops_the_one_it_replaces() {
    let mut g = graph();
    let new = wire("load.image", "mix.base");
    assert!(g.apply(&Edit::Connect {
        from: new.from.clone(),
        to: new.to.clone(),
        replaces: vec![wire("blur.image", "mix.base")],
    }));
    let expected = format!(
        "{}  - load.image -> mix.base\n",
        PIPELINE.replacen("  - blur.image -> mix.base\n", "", 1)
    );
    assert_eq!(g.text(), expected);
    assert_eq!(g.edges().last(), Some(&new));
}

#[test]
fn disconnecting_drops_a_wire_written_as_a_mapping() {
    let mut g = graph();
    assert!(g.apply(&Edit::Disconnect {
        edges: vec![wire("load.image", "mix.top")],
    }));
    assert_eq!(
        g.text(),
        fixture_with("  - {from: load.image, to: mix.top, color: accent}\n", "")
    );
}

#[test]
fn adding_writes_an_item_after_the_last_node() {
    let mut g = graph();
    assert!(g.apply(&Edit::Add {
        nodes: vec![NewNode {
            kind: "blur".into(),
            at: [100.0, 200.0],
            copy_of: Some("blur".into()),
        }],
    }));
    assert_eq!(
        g.text(),
        fixture_with(
            "    pos: [720, 40]\n",
            "    pos: [720, 40]\n  - id: blur-1\n    kind: blur\n    pos: [100, 200]\n    group: detail\n    params:\n      radius: 3.5\n"
        )
    );
    assert_eq!(g.param("blur-1", "radius"), Some(3.5));

    // A node of a kind's parameters, set for the first time.
    let mut g = graph();
    assert!(g.apply(&Edit::Add {
        nodes: vec![NewNode {
            kind: "blur".into(),
            at: [0.0, 0.0],
            copy_of: None,
        }],
    }));
    assert_eq!(g.param("blur-1", "radius"), Some(2.0));
    assert!(g.apply(&param("blur-1", "radius", 5.0)));
    assert_eq!(
        g.text(),
        fixture_with(
            "    pos: [720, 40]\n",
            "    pos: [720, 40]\n  - id: blur-1\n    kind: blur\n    pos: [0, 0]\n    params:\n      radius: 5\n"
        )
    );
}

#[test]
fn edits_naming_what_is_not_there_change_nothing() {
    let mut g = graph();
    assert!(!g.apply(&moved("nothing", [1.0, 1.0])));
    assert!(!g.apply(&param("load", "radius", 1.0)));
    assert!(!g.apply(&Edit::Add {
        nodes: vec![NewNode {
            kind: "unknown".into(),
            at: [0.0, 0.0],
            copy_of: None,
        }],
    }));
    assert!(!g.apply(&Edit::Connect {
        from: PortRef::new("load", "nothing"),
        to: PortRef::new("blur", "image"),
        replaces: Vec::new(),
    }));
    assert_eq!(g.text(), PIPELINE);
}

#[test]
fn a_flow_style_node_is_written_again_in_block_style() {
    let mut g = YamlGraph::parse("nodes:\n  - {id: a, pos: [0, 0], extra: 1}\n").unwrap();
    assert!(g.apply(&moved("a", [5.0, 5.0])));
    assert_eq!(
        g.text(),
        "nodes:\n  - id: a\n    pos: [5, 5]\n    extra: 1\n"
    );
}

#[test]
fn a_flow_style_list_is_written_again_in_block_style() {
    let mut g = YamlGraph::parse("nodes: [{id: a}, {id: b}]\nedges: []\n# end\n").unwrap();
    assert!(g.apply(&Edit::Delete {
        nodes: vec!["a".into()],
        edges: Vec::new(),
    }));
    assert_eq!(g.text(), "nodes:\n  - id: b\nedges: []\n# end\n");
}

#[test]
fn an_empty_graph_grows() {
    let g = YamlGraph::empty();
    assert_eq!(g.version(), FORMAT_VERSION);
    assert!(g.nodes().is_empty());
    assert!(g.text().contains("org.egui-pop.graph: {version: 1}"));

    let add = Edit::Add {
        nodes: vec![NewNode {
            kind: "k".into(),
            at: [0.0, 0.0],
            copy_of: None,
        }],
    };
    let mut g = YamlGraph::parse("kinds:\n  k: {}\nnodes: []  # none yet\nedges: []\n").unwrap();
    assert!(g.apply(&add));
    assert_eq!(
        g.text(),
        "kinds:\n  k: {}\nnodes: # none yet\n  - id: k-1\n    kind: k\n    pos: [0, 0]\nedges: []\n"
    );
    // No list yet: one is written at the end.
    let mut g = YamlGraph::parse("kinds:\n  k: {}\n").unwrap();
    assert!(g.apply(&add));
    assert_eq!(
        g.text(),
        "kinds:\n  k: {}\nnodes:\n  - id: k-1\n    kind: k\n    pos: [0, 0]\n"
    );
}

#[test]
fn a_newer_stamp_is_read_with_a_warning() {
    let g =
        YamlGraph::parse("$extensions:\n  org.egui-pop.graph: {version: 2}\nnodes: []\n").unwrap();
    assert_eq!(g.version(), 2);
    assert_eq!(g.warnings().len(), 1);
    let err = YamlGraph::parse("$extensions:\n  org.egui-pop.graph: {version: x}\n").unwrap_err();
    assert!(err.message.contains("version"), "{err}");
}

#[test]
fn malformed_files_say_where() {
    let at = |src: &str| {
        let err = YamlGraph::parse(src).expect_err(src);
        (err.line, err.to_string())
    };
    // Not YAML.
    let (line, text) = at("nodes:\n  - id: a\n    pos: [0, 0\n");
    assert!(line.is_some(), "{text}");
    // Not a mapping at the top.
    let (line, text) = at("- a\n- b\n");
    assert_eq!(line, Some(1), "{text}");
    assert!(text.contains("block mapping"), "{text}");
    // Two nodes with one id: the second.
    let (line, text) = at("nodes:\n  - id: a\n  - id: a\n");
    assert_eq!(line, Some(3), "{text}");
    assert!(text.contains("a second node `a`"), "{text}");
    // A kind not declared.
    let (line, text) = at("nodes:\n  - id: a\n    kind: blur\n");
    assert_eq!(line, Some(2), "{text}");
    assert!(text.contains("no kind `blur`"), "{text}");
    // A group not declared.
    let (line, text) = at("nodes:\n  - id: a\n    group: g\n");
    assert_eq!(line, Some(2), "{text}");
    assert!(text.contains("no group `g`"), "{text}");
    // A wire to a port that is not there.
    let (line, text) = at("nodes:\n  - id: a\n  - id: b\nedges:\n  - a.out -> b.in\n");
    assert_eq!(line, Some(5), "{text}");
    assert!(text.contains("no output `out` on node `a`"), "{text}");
    // A wire that is not one.
    let (line, text) = at("nodes:\n  - id: a\nedges:\n  - just a\n");
    assert_eq!(line, Some(4), "{text}");
    // A parameter without its range.
    let (line, text) = at("nodes:\n  - id: a\n    params:\n      r: {min: 0}\n");
    assert_eq!(line, Some(2), "{text}");
    assert!(text.contains("`max`"), "{text}");
    // A position that is not two numbers.
    let (line, text) = at("nodes:\n  - id: a\n    pos: [1]\n");
    assert!(text.contains("two numbers"), "{text} {line:?}");
}
