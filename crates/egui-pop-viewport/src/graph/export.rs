//! A graph exported as an image: SVG drawn from the model and its
//! layout (not from the screen), so it is crisp at any size and the same
//! graph gives the same text.
//!
//! [`GraphExport`] is a copy of what the editor would draw: the nodes,
//! wires and groups, the state's selection, collapsed nodes and folded
//! groups, the wire style, and the theme's colours ([`ExportTheme`]).
//! [`GraphExport::svg`] lays it out at a level ([`ExportOptions`]) and
//! writes the SVG: group frames, then wires, then nodes and stacks, as
//! the editor draws them; what only an interactive canvas shows (the
//! grid, the selection's outline, the focus ring, the chevrons, the
//! application's slot in title bars) is left out.
//!
//! [`GraphExport::png`] (feature `graph-png`) and [`GraphExport::pdf`]
//! (feature `graph-pdf`) render that SVG, with the theme's installed
//! faces ([`GraphExport::faces`]) so the labels match the screen.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use egui::{Color32, Pos2, Rect, pos2, vec2};
use egui_pop_core::fonts::{self, InstalledFace, Role};
use egui_pop_core::theme::Tokens;

use super::model::{Edge, GraphModel, Group, Level, Node, Param, PortType, Side};
use super::state::GraphState;
use super::wires::{RouteCache, WireStyle};
use super::{
    GROUP_LABEL, GraphTokens, HEADER, LayoutIn, PAD, PORT_R, Placed, ROW, lay_out, overview_height,
    param_h, wires_drawn,
};

/// What an export draws with: the theme's colours and text sizes,
/// resolved, so that an export needs no `egui::Context`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportTheme {
    /// The graph's colours (`graph.*`).
    pub colors: GraphTokens,
    /// The port types' colours, by token path ([`PortType::color`]); a
    /// type missing here is drawn in the plain wire's colour.
    pub types: BTreeMap<String, Color32>,
    /// The CSS font family of the text (the theme's `font.family.ui`,
    /// then `sans-serif`).
    pub font_family: String,
    /// The titles' size, in points.
    pub body: f32,
    /// The ports', parameters' and groups' labels' size, in points.
    pub small: f32,
    /// A parameter row's height, in graph units (the detail level).
    pub param_h: f32,
}

impl ExportTheme {
    /// The theme of a context, with the colours of a model's port types.
    pub fn of(ctx: &egui::Context, model: &dyn GraphModel) -> ExportTheme {
        ExportTheme::of_nodes(ctx, &model.nodes())
    }

    pub(super) fn of_nodes(ctx: &egui::Context, nodes: &[Node]) -> ExportTheme {
        let t = Tokens::of(ctx);
        let colors = GraphTokens::of(ctx);
        let mut types = BTreeMap::new();
        for n in nodes {
            for p in n.inputs.iter().chain(&n.outputs) {
                types
                    .entry(p.ty.color.clone())
                    .or_insert_with(|| colors.type_color(ctx, &p.ty));
            }
        }
        let mut family: Vec<String> = t
            .font
            .family_ui
            .iter()
            .map(|f| {
                if f.contains(|c: char| !c.is_ascii_alphanumeric() && c != '-') {
                    format!("'{}'", f.replace(['\'', '"'], ""))
                } else {
                    f.clone()
                }
            })
            .collect();
        family.push("sans-serif".into());
        ExportTheme {
            colors,
            types,
            font_family: family.join(", "),
            body: t.font.body,
            small: t.font.small,
            param_h: param_h(ctx),
        }
    }

    fn type_color(&self, ty: &PortType) -> Color32 {
        self.types
            .get(&ty.color)
            .copied()
            .unwrap_or(self.colors.wire)
    }
}

/// What an export draws.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ExportOptions {
    /// Only the selected nodes, the wires between them and the frames
    /// of their groups (default `false`: the whole graph). An empty
    /// selection gives an empty image.
    pub selection: bool,
    /// The level drawn (default [`Level::Ports`]): the overview's
    /// compact nodes and stacks, the ports, or the detail with each
    /// node's parameters and their values.
    pub level: Level,
    /// Fills the image with the graph's background (default `true`).
    pub background: bool,
    /// Around the drawing, in graph units (default 24).
    pub margin: f32,
    /// The image's title, for screen readers (default "Node graph").
    pub title: Option<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            selection: false,
            level: Level::Ports,
            background: true,
            margin: 24.0,
            title: None,
        }
    }
}

impl ExportOptions {
    pub fn selection(mut self, on: bool) -> Self {
        self.selection = on;
        self
    }

    pub fn level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    pub fn background(mut self, on: bool) -> Self {
        self.background = on;
        self
    }

    pub fn margin(mut self, margin: f32) -> Self {
        self.margin = margin.max(0.0);
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Why a PNG or PDF export failed.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ExportError {
    /// The image would be empty (an empty graph without a margin), the
    /// scale is not a positive number, or a side would pass
    /// [`GraphExport::MAX_SIDE`] pixels.
    Size(String),
    /// The SVG could not be read back to render it.
    Svg(String),
    /// The PDF could not be written.
    Pdf(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::Size(e) => write!(f, "image size: {e}"),
            ExportError::Svg(e) => write!(f, "SVG: {e}"),
            ExportError::Pdf(e) => write!(f, "PDF: {e}"),
        }
    }
}

impl std::error::Error for ExportError {}

#[cfg(any(feature = "graph-png", feature = "graph-pdf"))]
mod fonts_db;
#[cfg(feature = "graph-pdf")]
mod pdf;

/// A graph ready to export: an owned copy of what the editor draws,
/// kept as long as needed (an agent's `graph.export` reads the copy the
/// editor kept at its last frame).
#[derive(Clone, Debug, PartialEq)]
pub struct GraphExport {
    nodes: Vec<Node>,
    /// Each node's parameters, by index.
    params: Vec<Vec<Param>>,
    edges: Vec<Edge>,
    groups: Vec<Group>,
    selection: BTreeSet<String>,
    collapsed: BTreeSet<String>,
    folded: BTreeSet<String>,
    unfolded: BTreeSet<String>,
    wires: WireStyle,
    node_width: f32,
    theme: ExportTheme,
    /// The faces the PNG and PDF exports draw text with.
    faces: Vec<InstalledFace>,
}

impl GraphExport {
    /// A model with an editor's state (the wire style and node width it
    /// drew last), in a context's theme.
    pub fn capture(ctx: &egui::Context, model: &dyn GraphModel, state: &GraphState) -> GraphExport {
        let nodes = model.nodes();
        let theme = ExportTheme::of_nodes(ctx, &nodes);
        GraphExport::from_parts(model, nodes, model.edges(), model.groups(), state, theme)
            .faces(fonts::installed_faces(ctx, Role::Ui).unwrap_or_default())
    }

    /// A model with an editor's state, in a theme of the caller's (a
    /// light export of a dark interface, a test's fixed colours). Its
    /// text is drawn in egui's bundled faces unless [`GraphExport::faces`]
    /// gives others.
    pub fn with_theme(
        model: &dyn GraphModel,
        state: &GraphState,
        theme: ExportTheme,
    ) -> GraphExport {
        GraphExport::from_parts(
            model,
            model.nodes(),
            model.edges(),
            model.groups(),
            state,
            theme,
        )
    }

    pub(super) fn from_parts(
        model: &dyn GraphModel,
        nodes: Vec<Node>,
        edges: Vec<Edge>,
        groups: Vec<Group>,
        state: &GraphState,
        theme: ExportTheme,
    ) -> GraphExport {
        let params = nodes.iter().map(|n| model.params(&n.id)).collect();
        GraphExport {
            nodes,
            params,
            edges,
            groups,
            selection: state.selection.clone(),
            collapsed: state.collapsed.clone(),
            folded: state.folded.clone(),
            unfolded: state.unfolded.clone(),
            wires: state.wire_style,
            node_width: state.node_width,
            theme,
            faces: Vec::new(),
        }
    }

    /// How wires are drawn (by default the style the editor drew last).
    pub fn wires(mut self, style: WireStyle) -> Self {
        self.wires = style;
        self
    }

    /// A node's width in graph units (by default the editor's last).
    pub fn node_width(mut self, width: f32) -> Self {
        self.node_width = width.max(80.0);
        self
    }

    /// The faces the PNG and PDF exports draw text with, in order (the
    /// first found for a family wins, the others fill in missing
    /// characters); by default the theme's installed faces
    /// ([`fonts::installed_faces`]) when captured from a context. egui's
    /// bundled faces always come after them, so text never vanishes.
    /// The SVG names the theme's families and leaves fonts to its reader.
    pub fn faces(mut self, faces: Vec<InstalledFace>) -> Self {
        self.faces = faces;
        self
    }

    /// The theme it is drawn in.
    pub fn theme(&self) -> &ExportTheme {
        &self.theme
    }

    /// The selected nodes, as captured.
    pub fn selection(&self) -> &BTreeSet<String> {
        &self.selection
    }

    /// The graph as an SVG document, in graph units (a point at 100 %
    /// is one unit): `<svg>` with a `viewBox` around what is drawn and
    /// the margin. Every node is a `<g data-node="id">`, every wire a
    /// `<path data-edge="node.port->node.port">`, every group frame a
    /// `<g data-group="id">`, every stack a `<g data-stack="id">`.
    pub fn svg(&self, options: &ExportOptions) -> String {
        let keep: Vec<usize> = (0..self.nodes.len())
            .filter(|&i| !options.selection || self.selection.contains(&self.nodes[i].id))
            .collect();
        let nodes: Vec<Node> = keep.iter().map(|&i| self.nodes[i].clone()).collect();
        let params: Vec<&[Param]> = keep.iter().map(|&i| self.params[i].as_slice()).collect();
        let ids: BTreeSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        let edges: Vec<Edge> = self
            .edges
            .iter()
            .filter(|e| ids.contains(e.from.node.as_str()) && ids.contains(e.to.node.as_str()))
            .cloned()
            .collect();
        let level = options.level;
        let folded: Vec<bool> = self
            .groups
            .iter()
            .map(|g| {
                if level == Level::Overview {
                    !self.unfolded.contains(&g.id)
                } else {
                    self.folded.contains(&g.id)
                }
            })
            .collect();
        let theme = &self.theme;
        let overview_h = overview_height(theme.body, 1.0);
        let lay = lay_out(&LayoutIn {
            nodes: &nodes,
            groups: &self.groups,
            folded: &folded,
            collapsed: &self.collapsed,
            level,
            node_width: self.node_width,
            overview_h,
            param_h: theme.param_h,
            params: &|i| params[i].len(),
            pos: &|n| pos2(n.pos[0], n.pos[1]),
        });

        // The wires, as polylines or curves.
        let drawn = wires_drawn(&nodes, &edges, &lay, None);
        let routes: Vec<Vec<Pos2>> = match self.wires {
            WireStyle::Orthogonal => {
                let boxes: Vec<Rect> = lay.boxes.iter().map(|b| b.rect).collect();
                let ends: Vec<(Pos2, Pos2)> = drawn.iter().map(|&(_, a, b)| (a, b)).collect();
                RouteCache::default().routes(&boxes, &ends).to_vec()
            }
            WireStyle::Straight => drawn.iter().map(|&(_, a, b)| vec![a, b]).collect(),
            WireStyle::Curves => drawn
                .iter()
                .map(|&(_, a, b)| {
                    let [_, c1, c2, _] = curve(a, b);
                    vec![a, c1, c2, b]
                })
                .collect(),
        };

        // What the image covers.
        let mut bounds = Rect::NOTHING;
        for b in &lay.boxes {
            let mut r = b.rect;
            if b.node.is_none() {
                // A stack's edges behind.
                r.max += vec2(12.0, 12.0);
            }
            bounds = bounds.union(r);
        }
        for (_, r) in &lay.frames {
            bounds = bounds.union(*r);
        }
        for points in &routes {
            for p in points {
                bounds.extend_with(*p);
            }
        }
        if !bounds.is_positive() {
            bounds = Rect::from_min_size(Pos2::ZERO, vec2(0.0, 0.0));
        }
        let bounds = bounds.expand(options.margin);

        let mut w = Svg::default();
        let title = options.title.as_deref().unwrap_or("Node graph");
        let _ = write!(
            w.out,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" role=\"img\" font-family=\"{}\">\n<title>{}</title>\n",
            num(bounds.width()),
            num(bounds.height()),
            num(bounds.min.x),
            num(bounds.min.y),
            num(bounds.width()),
            num(bounds.height()),
            escape(&theme.font_family),
            escape(title),
        );
        if options.background {
            let _ = writeln!(
                w.out,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>",
                num(bounds.min.x),
                num(bounds.min.y),
                num(bounds.width()),
                num(bounds.height()),
                paint("fill", theme.colors.background),
            );
        }

        let members = |g: usize| -> usize {
            let id = self.groups[g].id.as_str();
            nodes
                .iter()
                .filter(|n| n.group.as_deref() == Some(id))
                .count()
        };

        // Group frames.
        for &(g, r) in &lay.frames {
            let group = &self.groups[g];
            let _ = writeln!(w.out, "<g data-group=\"{}\">", escape(&group.id));
            w.rect(
                r,
                6.0,
                &paint("fill", theme.colors.group.gamma_multiply(0.45)),
            );
            w.border(r, 6.0, 1.0, theme.colors.group_border);
            let band = Rect::from_min_size(r.min, vec2(r.width(), GROUP_LABEL));
            w.text(
                band.left_center() + vec2(PAD + 2.0, 0.0),
                Anchor::Start,
                &group.label,
                theme.small,
                theme.colors.text_muted,
                None,
            );
            w.out.push_str("</g>\n");
        }

        // Wires.
        if !drawn.is_empty() {
            w.out.push_str(
                "<g fill=\"none\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n",
            );
            for ((e, _, _), points) in drawn.iter().zip(&routes) {
                let color = nodes
                    .iter()
                    .find(|n| n.id == e.from.node)
                    .and_then(|n| n.port(Side::Output, &e.from.port))
                    .map_or(theme.colors.wire, |p| theme.type_color(&p.ty));
                let mut d = String::new();
                match (self.wires, points.as_slice()) {
                    (WireStyle::Curves, [a, c1, c2, b]) => {
                        let _ = write!(
                            d,
                            "M{} {}C{} {} {} {} {} {}",
                            num(a.x),
                            num(a.y),
                            num(c1.x),
                            num(c1.y),
                            num(c2.x),
                            num(c2.y),
                            num(b.x),
                            num(b.y)
                        );
                    }
                    _ => {
                        for (k, p) in points.iter().enumerate() {
                            let _ = write!(
                                d,
                                "{}{} {}",
                                if k == 0 { "M" } else { "L" },
                                num(p.x),
                                num(p.y)
                            );
                        }
                    }
                }
                let _ = writeln!(
                    w.out,
                    "<path data-edge=\"{}.{}-&gt;{}.{}\" d=\"{}\"{}/>",
                    escape(&e.from.node),
                    escape(&e.from.port),
                    escape(&e.to.node),
                    escape(&e.to.port),
                    d,
                    paint("stroke", color),
                );
            }
            w.out.push_str("</g>\n");
        }

        // Nodes and stacks.
        let title_size = if level == Level::Overview {
            (overview_h * 0.45).round()
        } else {
            theme.body
        };
        for b in &lay.boxes {
            match b.node {
                Some(i) => self.node(&mut w, &lay, b, i, &nodes[i], params[i], level, title_size),
                None => {
                    if let Some(g) = b.group {
                        self.stack(&mut w, b, g, members(g), title_size);
                    }
                }
            }
        }
        w.out.push_str("</svg>\n");
        w.out
    }

    /// The longest side of a PNG export, in pixels.
    pub const MAX_SIDE: u32 = 16_384;

    /// The graph as a PNG image (feature `graph-png`): [`GraphExport::svg`]
    /// rendered by resvg at `scale` pixels per graph unit (1 for a point
    /// at 100 %, 2 for a high density screen), its text in
    /// [`GraphExport::faces`] then egui's bundled faces. Transparent
    /// without the background. The same graph, options and scale give
    /// the same bytes.
    #[cfg(feature = "graph-png")]
    pub fn png(&self, options: &ExportOptions, scale: f32) -> Result<Vec<u8>, ExportError> {
        use resvg::{tiny_skia, usvg};
        if !(scale.is_finite() && scale > 0.0) {
            return Err(ExportError::Size(format!("scale {scale} is not positive")));
        }
        let svg = self.svg(options);
        let (db, family) = fonts_db::database(&self.faces);
        let opts = usvg::Options {
            font_family: family,
            fontdb: std::sync::Arc::new(db),
            ..usvg::Options::default()
        };
        let tree = usvg::Tree::from_str(&svg, &opts).map_err(|e| match e {
            usvg::Error::InvalidSize => ExportError::Size("the image is empty".into()),
            e => ExportError::Svg(e.to_string()),
        })?;
        let size = tree.size();
        let side = |v: f32| (v * scale).ceil();
        let (w, h) = (side(size.width()), side(size.height()));
        let max = Self::MAX_SIDE as f32;
        if !(w <= max && h <= max) {
            return Err(ExportError::Size(format!(
                "{w} × {h} pixels, more than {} on a side",
                Self::MAX_SIDE
            )));
        }
        let mut pixmap = tiny_skia::Pixmap::new(w as u32, h as u32)
            .ok_or_else(|| ExportError::Size("the image is empty".into()))?;
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        pixmap
            .encode_png()
            .map_err(|e| ExportError::Svg(e.to_string()))
    }

    /// The graph as a one-page PDF (feature `graph-pdf`):
    /// [`GraphExport::svg`] converted by svg2pdf, a graph unit a PDF
    /// point, its text kept as text in [`GraphExport::faces`] then
    /// egui's bundled faces, each embedded as a subset. The metadata is
    /// fixed (the title, `egui-pop` as producer, no date), and the
    /// objects are written in a canonical order, so the same graph and
    /// options give the same bytes.
    #[cfg(feature = "graph-pdf")]
    pub fn pdf(&self, options: &ExportOptions) -> Result<Vec<u8>, ExportError> {
        use svg2pdf::usvg;
        let svg = self.svg(options);
        let (db, family) = fonts_db::database(&self.faces);
        let opts = usvg::Options {
            font_family: family,
            fontdb: std::sync::Arc::new(db),
            ..usvg::Options::default()
        };
        let tree = usvg::Tree::from_str(&svg, &opts).map_err(|e| match e {
            usvg::Error::InvalidSize => ExportError::Size("the image is empty".into()),
            e => ExportError::Svg(e.to_string()),
        })?;
        let bytes = svg2pdf::to_pdf(
            &tree,
            svg2pdf::ConversionOptions::default(),
            svg2pdf::PageOptions::default(),
        )
        .map_err(|e| ExportError::Pdf(e.to_string()))?;
        let title = options.title.as_deref().unwrap_or("Node graph");
        pdf::canonical(&bytes, title)
            .ok_or_else(|| ExportError::Pdf("svg2pdf's output could not be read back".into()))
    }

    fn stack(&self, w: &mut Svg, b: &Placed, g: usize, count: usize, size: f32) {
        let t = &self.theme.colors;
        let r = b.rect;
        let group = &self.groups[g];
        let _ = writeln!(w.out, "<g data-stack=\"{}\">", escape(&group.id));
        for k in [2.0, 1.0] {
            let back = r.translate(vec2(6.0, 6.0) * k);
            w.rect(back, 6.0, &paint("fill", t.node_header));
            w.border(back, 6.0, 1.0, t.node_border);
        }
        w.rect(r, 6.0, &paint("fill", t.node));
        w.border(r, 6.0, 1.0, t.node_border);
        let text_r = Rect::from_min_max(
            r.min + vec2(PAD * 2.0, 0.0),
            pos2(r.right() - PAD * 2.0 - size * 2.0, r.max.y),
        );
        w.text(
            text_r.left_center(),
            Anchor::Start,
            &group.label,
            size,
            t.text,
            Some(text_r),
        );
        w.text(
            r.right_center() - vec2(PAD * 2.0, 0.0),
            Anchor::End,
            &format!("+{count}"),
            size * 0.85,
            t.text_muted,
            None,
        );
        w.out.push_str("</g>\n");
    }

    #[allow(clippy::too_many_arguments)]
    fn node(
        &self,
        w: &mut Svg,
        lay: &super::Layout,
        b: &Placed,
        i: usize,
        n: &Node,
        params: &[Param],
        level: Level,
        title_size: f32,
    ) {
        let theme = &self.theme;
        let t = &theme.colors;
        let r = b.rect;
        let overview = level == Level::Overview;
        let header_h = if overview { r.height() } else { HEADER };
        let header = Rect::from_min_size(r.min, vec2(r.width(), header_h));
        let _ = writeln!(w.out, "<g data-node=\"{}\">", escape(&n.id));
        w.rect(r, 6.0, &paint("fill", t.node));
        if !overview {
            let low = if b.collapsed { 6.0 } else { 0.0 };
            w.path(header, [6.0, 6.0, low, low], &paint("fill", t.node_header));
        }
        // The colour of what it gives, as a band on the left.
        if let Some(p) = n.outputs.first().or(n.inputs.first()) {
            let band = Rect::from_min_size(r.min, vec2(4.0, header.height()));
            let low = if overview || b.collapsed { 6.0 } else { 0.0 };
            w.path(
                band,
                [6.0, 0.0, 0.0, low],
                &paint("fill", theme.type_color(&p.ty)),
            );
        }
        w.border(r, 6.0, 1.0, t.node_border);
        let text_r = Rect::from_min_max(
            pos2(r.left() + PAD * 2.0, header.top()),
            pos2(header.right() - PAD, header.bottom()),
        );
        w.text(
            text_r.left_center(),
            Anchor::Start,
            &n.title,
            title_size,
            t.text,
            Some(text_r),
        );
        if overview || b.collapsed {
            w.out.push_str("</g>\n");
            return;
        }
        // Ports.
        for (side, list) in [(Side::Input, &n.inputs), (Side::Output, &n.outputs)] {
            for (k, port) in list.iter().enumerate() {
                let Some(&at) = lay.ports.get(&(i, side, k)) else {
                    continue;
                };
                let color = theme.type_color(&port.ty);
                let wired = self.edges.iter().any(|e| match side {
                    Side::Input => e.to.node == n.id && e.to.port == port.id,
                    Side::Output => e.from.node == n.id && e.from.port == port.id,
                });
                let _ = writeln!(
                    w.out,
                    "<circle data-port=\"{}\" data-side=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{}\"{}{} stroke-width=\"1.5\"/>",
                    escape(&port.id),
                    side.name(),
                    num(at.x),
                    num(at.y),
                    num(PORT_R),
                    paint("fill", if wired { color } else { t.node }),
                    paint("stroke", color),
                );
                let (text_at, anchor) = match side {
                    Side::Input => (at + vec2(PORT_R + 5.0, 0.0), Anchor::Start),
                    Side::Output => (at - vec2(PORT_R + 5.0, 0.0), Anchor::End),
                };
                w.text(
                    text_at,
                    anchor,
                    &port.label,
                    theme.small,
                    t.text_muted,
                    None,
                );
            }
        }
        if level != Level::Detail {
            w.out.push_str("</g>\n");
            return;
        }
        // Parameters: each a row with its label, its value and how far
        // along its range it is.
        let rows = n.inputs.len().max(n.outputs.len());
        let h = theme.param_h;
        let mut y = r.top() + HEADER + rows as f32 * ROW;
        for p in params {
            let row = Rect::from_min_size(
                pos2(r.left() + PAD * 2.0, y + 2.0),
                vec2(r.width() - PAD * 4.0, h - 4.0),
            );
            y += h;
            let _ = writeln!(w.out, "<g data-param=\"{}\">", escape(&p.id));
            w.rect(row, 4.0, &paint("fill", t.node_header));
            let span = p.range.end() - p.range.start();
            let along = if span > 0.0 {
                ((p.value - p.range.start()) / span).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            if along > 0.0 {
                let fill = Rect::from_min_size(row.min, vec2(row.width() * along, row.height()));
                w.rect(
                    fill,
                    4.0,
                    &paint("fill", t.node_selected.gamma_multiply(0.35)),
                );
            }
            w.text(
                row.left_center() + vec2(PAD, 0.0),
                Anchor::Start,
                &p.label,
                theme.small,
                t.text_muted,
                None,
            );
            w.text(
                row.right_center() - vec2(PAD, 0.0),
                Anchor::End,
                &value_text(p),
                theme.small,
                t.text,
                None,
            );
            w.out.push_str("</g>\n");
        }
        w.out.push_str("</g>\n");
    }
}

/// A wire's curve from an output at `a` to an input at `b`: its four
/// control points, as the editor draws it.
pub(super) fn curve(a: Pos2, b: Pos2) -> [Pos2; 4] {
    let dx = ((b.x - a.x).abs() * 0.5).max(40.0);
    [a, a + vec2(dx, 0.0), b - vec2(dx, 0.0), b]
}

/// A parameter's value as text: as many decimals as its step has (two
/// without a step), then its unit.
fn value_text(p: &Param) -> String {
    let decimals = match p.step {
        Some(step) if step > 0.0 => {
            let mut d = 0;
            while d < 6 && ((step * 10f64.powi(d)).round() - step * 10f64.powi(d)).abs() > 1e-9 {
                d += 1;
            }
            d as usize
        }
        _ => 2,
    };
    let mut v = format!("{:.*}", decimals, p.value);
    if v.starts_with('-') && v[1..].chars().all(|c| c == '0' || c == '.') {
        v.remove(0);
    }
    match &p.unit {
        Some(u) if !u.is_empty() => format!("{v} {u}"),
        _ => v,
    }
}

/// Where a text sits from its point.
#[derive(Clone, Copy)]
enum Anchor {
    Start,
    End,
}

/// The document being written.
#[derive(Default)]
struct Svg {
    out: String,
    /// Clip paths written, for their ids.
    clips: usize,
}

impl Svg {
    fn rect(&mut self, r: Rect, radius: f32, paint: &str) {
        let rx = radius.min(r.width() / 2.0).min(r.height() / 2.0).max(0.0);
        let _ = writeln!(
            self.out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"{paint}/>",
            num(r.min.x),
            num(r.min.y),
            num(r.width()),
            num(r.height()),
            num(rx),
        );
    }

    /// A stroke inside a rectangle's edges, as egui's `StrokeKind::Inside`.
    fn border(&mut self, r: Rect, radius: f32, width: f32, color: Color32) {
        let inner = r.shrink(width / 2.0);
        let rx = (radius - width / 2.0)
            .min(inner.width() / 2.0)
            .min(inner.height() / 2.0)
            .max(0.0);
        let _ = writeln!(
            self.out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"none\"{} stroke-width=\"{}\"/>",
            num(inner.min.x),
            num(inner.min.y),
            num(inner.width()),
            num(inner.height()),
            num(rx),
            paint("stroke", color),
            num(width),
        );
    }

    /// A rectangle with its own radius at each corner: north-west,
    /// north-east, south-east, south-west.
    fn path(&mut self, r: Rect, radii: [f32; 4], paint: &str) {
        let most = (r.width() / 2.0).min(r.height() / 2.0).max(0.0);
        let [nw, ne, se, sw] = radii.map(|v| v.clamp(0.0, most));
        let (l, t, rr, b) = (r.left(), r.top(), r.right(), r.bottom());
        let mut d = format!("M{} {}H{}", num(l + nw), num(t), num(rr - ne));
        let arc = |d: &mut String, radius: f32, x: f32, y: f32| {
            if radius > 0.0 {
                let _ = write!(
                    d,
                    "A{} {} 0 0 1 {} {}",
                    num(radius),
                    num(radius),
                    num(x),
                    num(y)
                );
            }
        };
        arc(&mut d, ne, rr, t + ne);
        let _ = write!(d, "V{}", num(b - se));
        arc(&mut d, se, rr - se, b);
        let _ = write!(d, "H{}", num(l + sw));
        arc(&mut d, sw, l, b - sw);
        let _ = write!(d, "V{}", num(t + nw));
        arc(&mut d, nw, l + nw, t);
        d.push('Z');
        let _ = writeln!(self.out, "<path d=\"{d}\"{paint}/>");
    }

    /// A line of text centred on `at` vertically, cut to `clip` if given.
    fn text(
        &mut self,
        at: Pos2,
        anchor: Anchor,
        text: &str,
        size: f32,
        color: Color32,
        clip: Option<Rect>,
    ) {
        let clip_attr = match clip {
            Some(c) => {
                let id = format!("clip-{}", self.clips);
                self.clips += 1;
                let _ = writeln!(
                    self.out,
                    "<clipPath id=\"{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>",
                    num(c.min.x),
                    num(c.min.y),
                    num(c.width().max(0.0)),
                    num(c.height().max(0.0)),
                );
                format!(" clip-path=\"url(#{id})\"")
            }
            None => String::new(),
        };
        let anchor = match anchor {
            Anchor::Start => "",
            Anchor::End => " text-anchor=\"end\"",
        };
        let _ = writeln!(
            self.out,
            "<text x=\"{}\" y=\"{}\" dy=\"0.35em\" font-size=\"{}\"{anchor}{}{clip_attr}>{}</text>",
            num(at.x),
            num(at.y),
            num(size),
            paint("fill", color),
            escape(text),
        );
    }
}

/// A number as SVG writes it: at most two decimals, no trailing zeros,
/// never `-0`.
fn num(v: f32) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{:.2}", (v * 100.0).round() / 100.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.to_string()
    }
}

/// A colour attribute (`fill`, `stroke`), with its opacity when it is
/// not opaque.
fn paint(attr: &str, c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    let mut s = format!(" {attr}=\"#{r:02x}{g:02x}{b:02x}\"");
    if a < 255 {
        let _ = write!(s, " {attr}-opacity=\"{}\"", num(f32::from(a) / 255.0));
    }
    s
}

/// Text and attribute values, escaped for XML.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Not allowed in XML 1.0.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short_and_never_negative_zero() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(1.5), "1.5");
        assert_eq!(num(-0.001), "0");
        assert_eq!(num(2.345_67), "2.35");
        assert_eq!(num(f32::NAN), "0");
    }

    #[test]
    fn colours_carry_their_opacity() {
        assert_eq!(
            paint("fill", Color32::from_rgb(255, 0, 16)),
            " fill=\"#ff0010\""
        );
        let half = Color32::from_rgba_unmultiplied(0, 0, 0, 128);
        assert!(paint("stroke", half).contains("stroke-opacity=\"0.5\""));
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(escape("a<b & \"c\"\u{1}"), "a&lt;b &amp; &quot;c&quot;");
    }

    #[test]
    fn values_take_their_step_s_decimals() {
        let p = Param::new("r", "Radius", 2.0, 0.0..=10.0)
            .step(0.1)
            .unit("px");
        assert_eq!(value_text(&p), "2.0 px");
        let p = Param::new("e", "Exposure", -0.0001, -3.0..=3.0).step(0.01);
        assert_eq!(value_text(&p), "0.00");
        let p = Param::new("n", "Count", 3.0, 0.0..=10.0).step(1.0);
        assert_eq!(value_text(&p), "3");
    }
}
