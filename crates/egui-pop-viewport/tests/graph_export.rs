//! The node graph exported as SVG: well formed, the same text for the
//! same graph, every node, wire, group and stack where it belongs, at
//! each level and in each wire style, the whole graph or the selection;
//! as PNG (feature `graph-png`) and PDF (feature `graph-pdf`).

use std::collections::BTreeMap;

use egui::Color32;
use egui_pop_viewport::graph::{
    Edge, ExportOptions, ExportTheme, GraphExport, GraphModel, GraphState, GraphTokens, Group,
    Level, Node, Param, Port, PortRef, PortType, WireStyle,
};

struct Pipeline {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

impl GraphModel for Pipeline {
    fn nodes(&self) -> Vec<Node> {
        self.nodes.clone()
    }

    fn edges(&self) -> Vec<Edge> {
        self.edges.clone()
    }

    fn groups(&self) -> Vec<Group> {
        vec![Group::new("detail", "Detail & <more>")]
    }

    fn params(&self, node: &str) -> Vec<Param> {
        match node {
            "blur" => vec![
                Param::new("radius", "Radius", 2.5, 0.0..=10.0)
                    .step(0.1)
                    .unit("px"),
            ],
            _ => Vec::new(),
        }
    }
}

fn wire(a: &str, p: &str, b: &str, q: &str) -> Edge {
    Edge::new(PortRef::new(a, p), PortRef::new(b, q))
}

fn pipeline() -> Pipeline {
    let image = || PortType::new("image");
    let mask = || PortType::new("mask").color("graph.type-3");
    Pipeline {
        nodes: vec![
            Node::new("load", "load", "Load \"raw\"", [0.0, 0.0]).output(Port::new(
                "image",
                "Image",
                image(),
            )),
            Node::new("blur", "blur", "Blur", [260.0, 0.0])
                .input(Port::new("image", "Image", image()))
                .output(Port::new("image", "Image", image()))
                .group("detail"),
            Node::new("mask", "mask", "Mask", [260.0, 200.0])
                .input(Port::new("image", "Image", image()))
                .output(Port::new("mask", "Mask", mask()))
                .group("detail"),
            Node::new("mix", "mix", "Mix", [560.0, 90.0])
                .input(Port::new("a", "Base", image()))
                .input(Port::new("m", "Mask", mask()))
                .output(Port::new("image", "Image", image())),
        ],
        edges: vec![
            wire("load", "image", "blur", "image"),
            wire("load", "image", "mask", "image"),
            wire("blur", "image", "mix", "a"),
            wire("mask", "mask", "mix", "m"),
        ],
    }
}

/// Fixed colours and sizes: the test does not move with the themes.
fn theme() -> ExportTheme {
    let c = |v: u8| Color32::from_rgb(v, v, v);
    ExportTheme {
        colors: GraphTokens {
            background: c(20),
            grid: c(40),
            node: c(50),
            node_header: c(60),
            node_border: c(70),
            node_selected: Color32::from_rgb(80, 140, 255),
            wire: c(150),
            wire_preview: c(160),
            wire_refused: Color32::from_rgb(220, 60, 60),
            group: Color32::from_rgb(40, 60, 90),
            group_border: c(90),
            band: c(100),
            text: c(230),
            text_muted: c(170),
            focus: c(255),
        },
        types: BTreeMap::from([("graph.type-3".to_string(), Color32::from_rgb(200, 160, 40))]),
        font_family: "Inter, sans-serif".into(),
        body: 14.0,
        small: 12.0,
        param_h: 26.0,
    }
}

/// Parses the document, checks its root, and counts its elements with
/// an attribute.
fn parse(svg: &str) -> roxmltree::Document<'_> {
    let doc = roxmltree::Document::parse(svg).unwrap_or_else(|e| panic!("{e}:\n{svg}"));
    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "svg");
    assert_eq!(
        root.tag_name().namespace(),
        Some("http://www.w3.org/2000/svg")
    );
    let view: Vec<f32> = root
        .attribute("viewBox")
        .expect("a viewBox")
        .split(' ')
        .map(|v| v.parse().expect("a number"))
        .collect();
    assert_eq!(view.len(), 4);
    assert!(view[2] > 0.0 && view[3] > 0.0, "{view:?}");
    doc
}

fn with<'a, 'i>(doc: &'a roxmltree::Document<'i>, attr: &str) -> Vec<roxmltree::Node<'a, 'i>> {
    doc.descendants()
        .filter(|n| n.has_attribute(attr))
        .collect()
}

fn export(state: &GraphState) -> GraphExport {
    GraphExport::with_theme(&pipeline(), state, theme())
}

#[test]
fn an_export_is_well_formed_and_the_same_for_the_same_graph() {
    let state = GraphState::new();
    for wires in WireStyle::ALL {
        for level in [Level::Overview, Level::Ports, Level::Detail] {
            let options = ExportOptions::default().level(level);
            let svg = export(&state).wires(wires).svg(&options);
            assert_eq!(svg, export(&state).wires(wires).svg(&options), "stable");
            let doc = parse(&svg);
            let title = doc
                .descendants()
                .find(|n| n.has_tag_name("title"))
                .and_then(|n| n.text());
            assert_eq!(title, Some("Node graph"));
            if level == Level::Overview {
                // Groups fold into stacks in the overview.
                assert_eq!(with(&doc, "data-node").len(), 2, "{svg}");
                assert_eq!(with(&doc, "data-stack").len(), 1, "{svg}");
                assert!(with(&doc, "data-port").is_empty());
            } else {
                assert_eq!(with(&doc, "data-node").len(), 4, "{svg}");
                assert_eq!(with(&doc, "data-edge").len(), 4, "{svg}");
                assert_eq!(with(&doc, "data-group").len(), 1, "{svg}");
                assert_eq!(with(&doc, "data-port").len(), 8, "{svg}");
                let label = with(&doc, "data-group")[0]
                    .descendants()
                    .find(|n| n.has_tag_name("text"))
                    .and_then(|n| n.text());
                assert_eq!(label, Some("Detail & <more>"));
            }
            let params = with(&doc, "data-param");
            if level == Level::Detail {
                assert_eq!(params.len(), 1);
                let texts: Vec<&str> = params[0]
                    .descendants()
                    .filter(|n| n.has_tag_name("text"))
                    .filter_map(|n| n.text())
                    .collect();
                assert_eq!(texts, ["Radius", "2.5 px"]);
            } else {
                assert!(params.is_empty());
            }
        }
    }
}

#[test]
fn wires_follow_the_style_and_take_their_type_colour() {
    let state = GraphState::new();
    let svg = export(&state)
        .wires(WireStyle::Orthogonal)
        .svg(&ExportOptions::default());
    let doc = parse(&svg);
    for e in with(&doc, "data-edge") {
        let d = e.attribute("d").expect("a path");
        assert!(!d.contains('C'), "orthogonal: {d}");
        let points: Vec<(f32, f32)> = d
            .split(['M', 'L'])
            .filter(|s| !s.is_empty())
            .map(|p| {
                let (x, y) = p.split_once(' ').expect("x y");
                (x.parse().expect("x"), y.parse().expect("y"))
            })
            .collect();
        assert!(points.len() >= 2, "{d}");
        for w in points.windows(2) {
            assert!(w[0].0 == w[1].0 || w[0].1 == w[1].1, "slanted: {d}");
        }
    }
    let mask = with(&doc, "data-edge")
        .into_iter()
        .find(|e| e.attribute("data-edge") == Some("mask.mask->mix.m"))
        .expect("the mask's wire");
    assert_eq!(mask.attribute("stroke"), Some("#c8a028"));

    let svg = export(&state)
        .wires(WireStyle::Curves)
        .svg(&ExportOptions::default());
    let doc = parse(&svg);
    assert!(
        with(&doc, "data-edge")
            .iter()
            .all(|e| e.attribute("d").is_some_and(|d| d.contains('C')))
    );
}

#[test]
fn the_selection_exports_alone() {
    let mut state = GraphState::new();
    state
        .selection
        .extend(["blur".to_string(), "mix".to_string()]);
    let svg = export(&state).svg(&ExportOptions::default().selection(true).background(false));
    let doc = parse(&svg);
    let nodes: Vec<&str> = with(&doc, "data-node")
        .iter()
        .filter_map(|n| n.attribute("data-node"))
        .collect();
    assert_eq!(nodes, ["blur", "mix"]);
    let edges: Vec<&str> = with(&doc, "data-edge")
        .iter()
        .filter_map(|n| n.attribute("data-edge"))
        .collect();
    assert_eq!(edges, ["blur.image->mix.a"]);
    // No background: the first drawing is the group's frame.
    let first = doc
        .root_element()
        .children()
        .find(|n| n.is_element() && !n.has_tag_name("title"));
    assert_eq!(
        first.and_then(|n| n.attribute("data-group")),
        Some("detail")
    );

    // Nothing selected: an empty, valid image.
    let svg = export(&GraphState::new()).svg(&ExportOptions::default().selection(true));
    let doc = parse(&svg);
    assert!(with(&doc, "data-node").is_empty());
}

#[test]
fn collapsed_nodes_and_folded_groups_export_as_drawn() {
    let mut state = GraphState::new();
    state.collapsed.insert("mix".into());
    state.folded.insert("detail".into());
    let svg = export(&state).svg(&ExportOptions::default().title("Pipeline"));
    let doc = parse(&svg);
    let mix = with(&doc, "data-node")
        .into_iter()
        .find(|n| n.attribute("data-node") == Some("mix"))
        .expect("mix");
    assert!(
        !mix.descendants().any(|n| n.has_tag_name("circle")),
        "a collapsed node shows no ports"
    );
    let stacks = with(&doc, "data-stack");
    assert_eq!(stacks.len(), 1);
    let badge = stacks[0]
        .descendants()
        .filter(|n| n.has_tag_name("text"))
        .filter_map(|n| n.text())
        .next_back();
    assert_eq!(badge, Some("+2"));
    let title = doc
        .descendants()
        .find(|n| n.has_tag_name("title"))
        .and_then(|n| n.text());
    assert_eq!(title, Some("Pipeline"));
}

/// The view box of an SVG export: x, y, width, height.
#[cfg(feature = "graph-png")]
fn view_box(svg: &str) -> [f32; 4] {
    let doc = parse(svg);
    let v: Vec<f32> = doc
        .root_element()
        .attribute("viewBox")
        .expect("a viewBox")
        .split(' ')
        .map(|v| v.parse().expect("a number"))
        .collect();
    [v[0], v[1], v[2], v[3]]
}

/// A node's box in an SVG export: its first `<rect>`.
#[cfg(feature = "graph-png")]
fn node_rect(svg: &str, id: &str) -> [f32; 4] {
    let doc = parse(svg);
    let node = with(&doc, "data-node")
        .into_iter()
        .find(|n| n.attribute("data-node") == Some(id))
        .expect("the node");
    let rect = node
        .descendants()
        .find(|n| n.has_tag_name("rect"))
        .expect("its box");
    let a = |k: &str| -> f32 { rect.attribute(k).expect(k).parse().expect("a number") };
    [a("x"), a("y"), a("width"), a("height")]
}

#[cfg(feature = "graph-png")]
#[test]
fn a_png_decodes_at_its_scale_and_draws_the_nodes_and_their_labels() {
    use resvg::tiny_skia::Pixmap;

    let state = GraphState::new();
    let options = ExportOptions::default();
    let svg = export(&state).svg(&options);
    let [vx, vy, vw, vh] = view_box(&svg);
    let scale = 2.0;
    let png = export(&state).png(&options, scale).expect("a PNG");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let image = Pixmap::decode_png(&png).expect("it decodes");
    assert_eq!(image.width(), (vw * scale).ceil() as u32);
    assert_eq!(image.height(), (vh * scale).ceil() as u32);

    let pixel = |x: f32, y: f32| {
        let p = image
            .pixel(((x - vx) * scale) as u32, ((y - vy) * scale) as u32)
            .expect("inside the image")
            .demultiply();
        [p.red(), p.green(), p.blue(), p.alpha()]
    };
    // The background (20), and a node's body (50) at its centre.
    assert_eq!(pixel(vx + 2.0, vy + 2.0), [20, 20, 20, 255]);
    let [x, y, w, h] = node_rect(&svg, "mix");
    assert_eq!(pixel(x + w / 2.0, y + h - 2.5), [50, 50, 50, 255]);

    // The title is drawn: light pixels (text, 230) in the header (60),
    // right of the type band.
    let header_h = 28.0_f32.min(h);
    let mut light = 0;
    for py in ((y * scale) as i32)..(((y + header_h) * scale) as i32) {
        for px in (((x + 12.0) * scale) as i32)..(((x + w - 6.0) * scale) as i32) {
            let p = image
                .pixel(
                    (px as f32 - vx * scale) as u32,
                    (py as f32 - vy * scale) as u32,
                )
                .expect("inside the image");
            if p.red() > 150 {
                light += 1;
            }
        }
    }
    assert!(
        light > 20,
        "no text in the title bar ({light} light pixels)"
    );
}

#[cfg(feature = "graph-png")]
#[test]
fn a_png_is_the_same_for_the_same_graph_and_transparent_without_background() {
    use resvg::tiny_skia::Pixmap;

    let state = GraphState::new();
    let options = ExportOptions::default().level(Level::Detail);
    let png = export(&state).png(&options, 1.5).expect("a PNG");
    assert_eq!(png, export(&state).png(&options, 1.5).expect("a PNG"));

    let options = options.background(false);
    let image =
        Pixmap::decode_png(&export(&state).png(&options, 1.0).expect("a PNG")).expect("it decodes");
    assert_eq!(image.pixel(1, 1).expect("a corner").alpha(), 0);
}

#[cfg(feature = "graph-png")]
#[test]
fn a_png_refuses_a_bad_scale_and_an_empty_image() {
    use egui_pop_viewport::graph::ExportError;

    let state = GraphState::new();
    let options = ExportOptions::default();
    for scale in [0.0, -1.0, f32::NAN] {
        assert!(matches!(
            export(&state).png(&options, scale),
            Err(ExportError::Size(_))
        ));
    }
    assert!(matches!(
        export(&state).png(&options, 10_000.0),
        Err(ExportError::Size(_))
    ));
    // Nothing selected and no margin: nothing to draw.
    let empty = ExportOptions::default().selection(true).margin(0.0);
    assert!(matches!(
        export(&state).png(&empty, 1.0),
        Err(ExportError::Size(_))
    ));
}

#[cfg(feature = "graph-pdf")]
#[test]
fn a_pdf_embeds_its_font_and_is_the_same_for_the_same_graph() {
    let state = GraphState::new();
    let options = ExportOptions::default()
        .level(Level::Detail)
        .title("Pipeline");
    let pdf = export(&state).pdf(&options).expect("a PDF");
    assert!(pdf.starts_with(b"%PDF-"));
    assert!(pdf.ends_with(b"%%EOF"));
    let has = |s: &[u8]| pdf.windows(s.len()).any(|w| w == s);
    assert!(has(b"/FontFile"), "a font is embedded");
    assert!(has(b"/Producer (egui-pop)"));
    assert!(!has(b"/CreationDate") && !has(b"/ID"));
    // Run again: svg2pdf orders its resources by hash, the export does
    // not.
    for _ in 0..3 {
        assert_eq!(pdf, export(&state).pdf(&options).expect("a PDF"));
    }

    // The cross-reference table points at each object.
    let text = String::from_utf8_lossy(&pdf);
    let start: usize = text
        .rsplit("startxref\n")
        .next()
        .and_then(|t| t.split('\n').next())
        .and_then(|n| n.parse().ok())
        .expect("startxref");
    let table = &text[start..];
    let count: usize = table
        .lines()
        .nth(1)
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|n| n.parse().ok())
        .expect("the table's size");
    for (n, line) in table.lines().skip(3).take(count - 1).enumerate() {
        let offset: usize = line[..10].parse().expect("an offset");
        assert!(
            pdf[offset..].starts_with(format!("{} 0 obj\n", n + 1).as_bytes()),
            "object {}",
            n + 1
        );
    }
}
