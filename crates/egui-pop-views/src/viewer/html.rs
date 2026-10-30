//! The safe HTML subset (ADR 0017): a page parsed by html5ever, its tree
//! read through an allow-list and written as markdown, so an HTML page
//! becomes the same blocks as a markdown page: the same drawing, find in
//! page, anchors, links through the links service and images behind the
//! user's click.
//!
//! The allow-list is the mapping itself, and nothing else leaves it:
//!
//! - **Attributes**: only `href`, `src`, `alt`, a heading's `id` and a
//!   list's `start` are read. `on*`, `style`, `class` and every other
//!   attribute are never looked at.
//! - **Addresses**: a link or an image keeps its address when it is
//!   relative or its scheme is allowed (`page::schemes`); otherwise a link
//!   stays as plain text and an image as its alt text (`javascript:`,
//!   `data:`, however spelled: the address is read as a browser would). A
//!   `file:` image refused is named in [`Html::dropped`].
//! - **Elements**: what is mapped is drawn; an element from [`DROPPED`] (a
//!   script, a form, a frame, a video, foreign SVG or MathML) goes with
//!   its content and is named in [`Html::dropped`]; a template's content
//!   is never read; any other element leaves its text.
//! - **Text** is escaped: it cannot become markdown, or HTML again.
//!
//! No HTML is written back, so nothing reads the result as HTML (and no
//! mutation of the markup can happen on a second parse). ammonia, the
//! sanitiser ADR 0017 first chose, is not used: every release of it that
//! passes the advisories needs cssparser, under the MPL 2.0 `deny.toml`
//! refuses, and it would clean a markup this module does not emit.

use std::cell::RefCell;
use std::collections::BTreeSet;

use html5ever::interface::{ElemName, ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{Attribute, LocalName, Namespace, QualName, ns};

use super::page::escape;

/// Elements dropped with their content; each found is named to the user,
/// who can open the page outside.
const DROPPED: &[&str] = &[
    "applet", "audio", "button", "canvas", "datalist", "dialog", "embed", "form", "frame",
    "frameset", "iframe", "input", "noembed", "noframes", "object", "script", "select", "template",
    "textarea", "video",
];

/// Elements that hold no content for a reader: no text, no report.
const SILENT: &[&str] = &[
    "area", "base", "col", "colgroup", "link", "meta", "param", "source", "style", "title",
    "track", "wbr",
];

/// Elements mapped to blocks, or holding blocks.
const BLOCKS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "caption",
    "center",
    "dd",
    "details",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hgroup",
    "hr",
    "html",
    "legend",
    "li",
    "main",
    "menu",
    "nav",
    "noscript",
    "ol",
    "p",
    "pre",
    "search",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
];

/// How deep the mapping follows the tree; deeper content is kept as plain
/// text, so a hostile page cannot exhaust the stack.
const MAX_DEPTH: usize = 48;

/// A page read: its markdown, and what it could not keep.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Html {
    pub markdown: String,
    /// Its `<title>`.
    pub title: Option<String>,
    /// The elements dropped with their content and the `file:` images
    /// refused, each once, sorted.
    pub dropped: Vec<String>,
}

/// Reads a page: parsed as a browser would (unclosed tags, misnesting),
/// then mapped.
pub(super) fn read(source: &str, schemes: &BTreeSet<String>) -> Html {
    let mut opts = html5ever::ParseOpts::default();
    // Nothing runs here: what a page shows without scripts is what counts.
    opts.tree_builder.scripting_enabled = false;
    let sink = html5ever::parse_document(Sink::default(), opts).one(source);
    let nodes = sink.nodes.into_inner();
    let mut reader = Reader {
        nodes: &nodes,
        schemes,
        title: None,
        dropped: BTreeSet::new(),
    };
    let blocks = reader.flow(&nodes[DOCUMENT].kids, 0);
    Html {
        markdown: blocks.join("\n\n"),
        title: reader.title,
        dropped: reader.dropped.into_iter().collect(),
    }
}

// ---- The tree -----------------------------------------------------------

/// The document's node.
const DOCUMENT: usize = 0;

#[derive(Debug)]
enum Data {
    Document,
    Element {
        name: QualName,
        attrs: Vec<Attribute>,
    },
    Text(String),
    /// A comment, a processing instruction, a template's content.
    Other,
}

#[derive(Debug)]
struct Node {
    parent: Option<usize>,
    kids: Vec<usize>,
    data: Data,
}

/// An arena html5ever builds its tree in (markup5ever_rcdom says it is
/// meant for html5ever's own tests); with no pointers between nodes, a
/// deep tree cannot overflow the stack when it is dropped.
#[derive(Debug)]
struct Sink {
    nodes: RefCell<Vec<Node>>,
}

impl Default for Sink {
    fn default() -> Self {
        Self {
            nodes: RefCell::new(vec![Node {
                parent: None,
                kids: Vec::new(),
                data: Data::Document,
            }]),
        }
    }
}

impl Sink {
    fn add(&self, data: Data) -> usize {
        let mut nodes = self.nodes.borrow_mut();
        nodes.push(Node {
            parent: None,
            kids: Vec::new(),
            data,
        });
        nodes.len() - 1
    }

    /// Takes a node out of its parent's children; where it was.
    fn detach(&self, node: usize) -> Option<(usize, usize)> {
        let mut nodes = self.nodes.borrow_mut();
        let parent = nodes[node].parent.take()?;
        let at = nodes[parent].kids.iter().position(|k| *k == node)?;
        nodes[parent].kids.remove(at);
        Some((parent, at))
    }

    /// Puts a child in `parent` at `at`, text joining the text before it.
    fn insert(&self, parent: usize, at: usize, child: NodeOrText<usize>) {
        let child = match child {
            NodeOrText::AppendNode(n) => n,
            NodeOrText::AppendText(t) => {
                let mut nodes = self.nodes.borrow_mut();
                let before = at.checked_sub(1).map(|i| nodes[parent].kids[i]);
                if let Some(b) = before
                    && let Data::Text(text) = &mut nodes[b].data
                {
                    text.push_str(&t);
                    return;
                }
                drop(nodes);
                self.add(Data::Text(t.to_string()))
            }
        };
        self.detach(child);
        let mut nodes = self.nodes.borrow_mut();
        // Detaching may have moved the place, within the same parent.
        let at = at.min(nodes[parent].kids.len());
        nodes[parent].kids.insert(at, child);
        nodes[child].parent = Some(parent);
    }
}

/// An element's name, owned: the arena's borrow does not outlive the call.
#[derive(Debug)]
struct Name(QualName);

impl ElemName for Name {
    fn ns(&self) -> &Namespace {
        &self.0.ns
    }

    fn local_name(&self) -> &LocalName {
        &self.0.local
    }
}

impl TreeSink for Sink {
    type Handle = usize;
    type Output = Self;
    type ElemName<'a> = Name;

    fn finish(self) -> Self {
        self
    }

    fn parse_error(&self, _msg: std::borrow::Cow<'static, str>) {}

    fn get_document(&self) -> usize {
        DOCUMENT
    }

    fn elem_name<'a>(&'a self, target: &'a usize) -> Name {
        match &self.nodes.borrow()[*target].data {
            Data::Element { name, .. } => Name(name.clone()),
            // html5ever asks only of elements it made.
            _ => Name(QualName::new(None, ns!(html), "".into())),
        }
    }

    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, _: ElementFlags) -> usize {
        self.add(Data::Element { name, attrs })
    }

    fn create_comment(&self, _: StrTendril) -> usize {
        self.add(Data::Other)
    }

    fn create_pi(&self, _: StrTendril, _: StrTendril) -> usize {
        self.add(Data::Other)
    }

    fn append(&self, parent: &usize, child: NodeOrText<usize>) {
        let at = self.nodes.borrow()[*parent].kids.len();
        self.insert(*parent, at, child);
    }

    fn append_based_on_parent_node(&self, element: &usize, prev: &usize, child: NodeOrText<usize>) {
        if self.nodes.borrow()[*element].parent.is_some() {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev, child);
        }
    }

    fn append_doctype_to_document(&self, _: StrTendril, _: StrTendril, _: StrTendril) {}

    fn get_template_contents(&self, _: &usize) -> usize {
        // A place of its own, out of the document: never read.
        self.add(Data::Other)
    }

    fn same_node(&self, x: &usize, y: &usize) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _: QuirksMode) {}

    fn append_before_sibling(&self, sibling: &usize, new_node: NodeOrText<usize>) {
        let place = {
            let nodes = self.nodes.borrow();
            nodes[*sibling].parent.and_then(|p| {
                let at = nodes[p].kids.iter().position(|k| k == sibling)?;
                Some((p, at))
            })
        };
        if let Some((parent, at)) = place {
            self.insert(parent, at, new_node);
        }
    }

    fn add_attrs_if_missing(&self, target: &usize, new: Vec<Attribute>) {
        if let Data::Element { attrs, .. } = &mut self.nodes.borrow_mut()[*target].data {
            for a in new {
                if !attrs.iter().any(|o| o.name == a.name) {
                    attrs.push(a);
                }
            }
        }
    }

    fn remove_from_parent(&self, target: &usize) {
        self.detach(*target);
    }

    fn reparent_children(&self, node: &usize, new_parent: &usize) {
        let kids = std::mem::take(&mut self.nodes.borrow_mut()[*node].kids);
        let mut nodes = self.nodes.borrow_mut();
        for k in &kids {
            nodes[*k].parent = Some(*new_parent);
        }
        nodes[*new_parent].kids.extend(kids);
    }
}

// ---- The mapping --------------------------------------------------------

struct Reader<'a> {
    nodes: &'a [Node],
    schemes: &'a BTreeSet<String>,
    title: Option<String>,
    dropped: BTreeSet<String>,
}

/// What an element is to the mapping.
enum Class {
    Silent,
    Dropped,
    Block,
    Inline,
}

impl Reader<'_> {
    /// An element's tag, when it is an HTML one; a foreign root (`svg`,
    /// `math`) goes with its content, named.
    fn class(&mut self, node: usize) -> Option<(&'static str, Class)> {
        let Data::Element { name, .. } = &self.nodes[node].data else {
            return None;
        };
        if name.ns != ns!(html) {
            self.dropped.insert(name.local.to_string());
            return None;
        }
        let tag: &str = &name.local;
        if tag == "title" && self.title.is_none() {
            let text = collapse(&self.text_of(node));
            if !text.is_empty() {
                self.title = Some(text);
            }
        }
        if let Some(t) = DROPPED.iter().find(|t| **t == tag) {
            self.dropped.insert((*t).to_string());
            Some((t, Class::Dropped))
        } else if let Some(t) = SILENT.iter().find(|t| **t == tag) {
            Some((t, Class::Silent))
        } else if let Some(t) = BLOCKS.iter().find(|t| **t == tag) {
            Some((t, Class::Block))
        } else {
            Some((inline_tag(tag), Class::Inline))
        }
    }

    fn attr(&self, node: usize, name: &str) -> Option<&str> {
        let Data::Element { attrs, .. } = &self.nodes[node].data else {
            return None;
        };
        attrs
            .iter()
            .find(|a| &*a.name.local == name)
            .map(|a| &*a.value)
    }

    /// An address when it may be kept, as a markdown destination.
    fn address(&self, node: usize, name: &str) -> Option<String> {
        let raw = self.attr(node, name)?.trim();
        if raw.is_empty() {
            return None;
        }
        match url::Url::parse(raw) {
            Ok(u) if self.schemes.contains(u.scheme()) => Some(destination(raw)),
            Ok(_) => None,
            Err(url::ParseError::RelativeUrlWithoutBase) => Some(destination(raw)),
            Err(_) => None,
        }
    }

    /// All the text under a node, as written.
    fn text_of(&self, node: usize) -> String {
        let mut out = String::new();
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            match &self.nodes[n].data {
                Data::Text(t) => out.push_str(t),
                Data::Element { name, .. }
                    if name.ns == ns!(html)
                        && (DROPPED.contains(&&*name.local) || &*name.local == "style") => {}
                _ => stack.extend(self.nodes[n].kids.iter().rev()),
            }
        }
        out
    }

    /// Blocks and the inline runs between them.
    fn flow(&mut self, kids: &[usize], depth: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut run = Inline::default();
        for &k in kids {
            if let Data::Text(t) = &self.nodes[k].data {
                run.text(t);
                continue;
            }
            match self.class(k) {
                Some((tag, Class::Block)) => {
                    run.finish_into(&mut out);
                    if depth < MAX_DEPTH {
                        out.extend(self.block(k, tag, depth + 1));
                    } else {
                        let text = Inline::of_text(&self.text_of(k));
                        if !text.is_empty() {
                            out.push(text);
                        }
                    }
                }
                Some((tag, Class::Inline)) => self.inline(k, tag, &mut run, depth + 1),
                _ => {}
            }
        }
        run.finish_into(&mut out);
        out
    }

    fn block(&mut self, node: usize, tag: &'static str, depth: usize) -> Vec<String> {
        let kids = &self.nodes[node].kids;
        match tag {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = usize::from(tag.as_bytes()[1] - b'0');
                let text = self.line(node, depth, false);
                if text.is_empty() {
                    return Vec::new();
                }
                let id = self
                    .attr(node, "id")
                    .filter(|i| {
                        !i.is_empty()
                            && i.chars()
                                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                    })
                    .map_or_else(String::new, |i| format!(" {{#{i}}}"));
                vec![format!("{} {text}{id}", "#".repeat(level))]
            }
            "hr" => vec!["---".to_string()],
            "pre" => {
                let text = self.text_of(node);
                let text = text.trim_end_matches(['\n', '\r']);
                if text.is_empty() {
                    return Vec::new();
                }
                let fence = "`".repeat(backticks(text).max(2) + 1);
                vec![format!("{fence}text\n{text}\n{fence}")]
            }
            "blockquote" => {
                let body = self.flow(kids, depth).join("\n\n");
                if body.is_empty() {
                    return Vec::new();
                }
                vec![prefixed(&body, "> ", "> ")]
            }
            "ul" | "menu" | "dir" => self.list(node, None, depth),
            "ol" => {
                let start = self
                    .attr(node, "start")
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .filter(|s| *s < 1_000_000_000)
                    .unwrap_or(1);
                self.list(node, Some(start), depth)
            }
            "dl" => self.definitions(node, depth),
            "table" => self.table(node, depth),
            "summary" | "dt" => {
                let text = self.line(node, depth, false);
                if text.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("**{text}**")]
                }
            }
            _ => self.flow(kids, depth),
        }
    }

    /// A node's content on one line.
    fn line(&mut self, node: usize, depth: usize, table: bool) -> String {
        let mut run = Inline {
            one_line: true,
            table,
            ..Inline::default()
        };
        self.inline_kids(node, &mut run, depth);
        run.out
    }

    fn inline_kids(&mut self, node: usize, run: &mut Inline, depth: usize) {
        for &k in &self.nodes[node].kids {
            if let Data::Text(t) = &self.nodes[k].data {
                run.text(t);
                continue;
            }
            match self.class(k) {
                Some((tag, Class::Inline)) => self.inline(k, tag, run, depth + 1),
                Some((_, Class::Block)) => {
                    // A block inside inline content: its text, set apart.
                    run.space();
                    if depth < MAX_DEPTH {
                        self.inline_kids(k, run, depth + 1);
                    } else {
                        run.text(&self.text_of(k));
                    }
                    run.space();
                }
                _ => {}
            }
        }
    }

    fn inline(&mut self, node: usize, tag: &str, run: &mut Inline, depth: usize) {
        if depth >= MAX_DEPTH {
            run.text(&self.text_of(node));
            return;
        }
        let wrap = |this: &mut Self, run: &mut Inline, open: &str, close: &str| {
            let mut inner = Inline {
                one_line: run.one_line,
                table: run.table,
                ..Inline::default()
            };
            this.inline_kids(node, &mut inner, depth);
            run.wrapped(inner, open, close);
        };
        match tag {
            "br" => run.line_break(),
            "img" => {
                let alt = collapse(self.attr(node, "alt").unwrap_or_default());
                match self.address(node, "src") {
                    Some(src) => run.raw(&format!("![{}](<{src}>)", escape(&alt))),
                    None => {
                        let refused = self
                            .attr(node, "src")
                            .and_then(|src| url::Url::parse(src.trim()).ok())
                            .filter(|u| u.scheme() == "file");
                        if let Some(file) = refused {
                            self.dropped.insert(file.to_string());
                        }
                        run.text(&alt);
                    }
                }
            }
            "a" => match self.address(node, "href") {
                Some(href) => {
                    let close = format!("](<{href}>)");
                    wrap(self, run, "[", &close);
                }
                None => self.inline_kids(node, run, depth),
            },
            "em" | "i" | "cite" | "dfn" | "var" => wrap(self, run, "*", "*"),
            "strong" | "b" => wrap(self, run, "**", "**"),
            "del" | "s" | "strike" => wrap(self, run, "~~", "~~"),
            "q" => wrap(self, run, "“", "”"),
            "code" | "kbd" | "samp" | "tt" => {
                let text = collapse(&self.text_of(node));
                if !text.is_empty() {
                    run.code(&text);
                }
            }
            _ => self.inline_kids(node, run, depth),
        }
    }

    fn list(&mut self, node: usize, start: Option<u32>, depth: usize) -> Vec<String> {
        let mut items: Vec<String> = Vec::new();
        for &k in &self.nodes[node].kids {
            let is_item = matches!(
                &self.nodes[k].data,
                Data::Element { name, .. } if &*name.local == "li" && name.ns == ns!(html)
            );
            let body = if is_item {
                // Goes through `class` for its own sake: never dropped.
                join_item(&self.flow(&self.nodes[k].kids, depth))
            } else {
                // Lists nested without a `li`, stray text: part of the
                // item before.
                let body = join_item(&self.flow(&[k], depth));
                if body.is_empty() {
                    continue;
                }
                if let Some(last) = items.last_mut() {
                    last.push('\n');
                    last.push_str(&body);
                    continue;
                }
                body
            };
            items.push(body);
        }
        if items.is_empty() {
            return Vec::new();
        }
        let list = items
            .iter()
            .enumerate()
            .map(|(i, body)| {
                let marker = match start {
                    Some(s) => format!("{}. ", u64::from(s) + i as u64),
                    None => "- ".to_string(),
                };
                let rest = " ".repeat(marker.len());
                prefixed(body, &marker, &rest)
            })
            .collect::<Vec<_>>()
            .join("\n");
        vec![list]
    }

    /// `term`, then `: definition` lines; a definition without a term is a
    /// paragraph.
    fn definitions(&mut self, node: usize, depth: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut defined = false;
        let mut stack: Vec<usize> = self.nodes[node].kids.iter().rev().copied().collect();
        while let Some(k) = stack.pop() {
            let Some((tag, class)) = self.class(k) else {
                continue;
            };
            match (tag, class) {
                ("dt", _) => {
                    if defined {
                        out.push(std::mem::take(&mut cur));
                        defined = false;
                    }
                    let text = self.line(k, depth, false);
                    if !text.is_empty() {
                        if !cur.is_empty() {
                            cur.push('\n');
                        }
                        cur.push_str(&text);
                    }
                }
                ("dd", _) => {
                    let text = self.line(k, depth, false);
                    if text.is_empty() {
                    } else if cur.is_empty() {
                        out.push(text);
                    } else {
                        cur.push_str("\n: ");
                        cur.push_str(&text);
                        defined = true;
                    }
                }
                // `dl > div > dt`, as the standard allows.
                ("div", _) => stack.extend(self.nodes[k].kids.iter().rev()),
                _ => {}
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    }

    fn table(&mut self, node: usize, depth: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut stack: Vec<usize> = self.nodes[node].kids.iter().rev().copied().collect();
        while let Some(k) = stack.pop() {
            let Some((tag, _)) = self.class(k) else {
                continue;
            };
            match tag {
                "caption" => {
                    let text = self.line(k, depth, false);
                    if !text.is_empty() {
                        out.push(text);
                    }
                }
                "thead" | "tbody" | "tfoot" => stack.extend(self.nodes[k].kids.iter().rev()),
                "tr" => {
                    let mut row = Vec::new();
                    for &c in &self.nodes[k].kids {
                        if matches!(self.class(c), Some(("td" | "th", _))) {
                            row.push(self.line(c, depth, true));
                        }
                    }
                    if !row.is_empty() {
                        rows.push(row);
                    }
                }
                _ => {}
            }
        }
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns > 0 {
            let line = |row: &[String]| {
                let mut s = String::from("|");
                for i in 0..columns {
                    s.push(' ');
                    s.push_str(row.get(i).map_or("", String::as_str));
                    s.push_str(" |");
                }
                s
            };
            let mut table = vec![line(&rows[0])];
            table.push(format!("|{}", " --- |".repeat(columns)));
            table.extend(rows[1..].iter().map(|r| line(r)));
            out.push(table.join("\n"));
        }
        out
    }
}

/// The name an inline element is matched by (`'static` for the mapping;
/// one it does not know only leaves its text).
fn inline_tag(tag: &str) -> &'static str {
    const KNOWN: &[&str] = &[
        "a", "b", "br", "cite", "code", "del", "dfn", "em", "i", "img", "kbd", "q", "s", "samp",
        "strike", "strong", "tt", "var",
    ];
    KNOWN.iter().find(|t| **t == tag).copied().unwrap_or("")
}

/// The longest run of backticks in a text.
fn backticks(text: &str) -> usize {
    text.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

/// Runs of ASCII whitespace made one space, the ends trimmed.
fn collapse(text: &str) -> String {
    text.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}

/// An address as a markdown destination between `<` and `>`.
fn destination(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for c in url.chars() {
        match c {
            '\t' | '\n' | '\r' => {}
            '&' => out.push_str("&amp;"),
            ' ' | '<' | '>' | '|' | '\\' | '`' => out.push_str(&format!("%{:02X}", c as u32)),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// A text with `first` before its first line and `rest` before the others;
/// empty lines stay without trailing spaces.
fn prefixed(text: &str, first: &str, rest: &str) -> String {
    let mut out = String::with_capacity(text.len() + first.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let prefix = if i == 0 { first } else { rest };
        if line.is_empty() {
            out.push_str(prefix.trim_end());
        } else {
            out.push_str(prefix);
            out.push_str(line);
        }
    }
    out
}

/// A list item's blocks: a list right after its text stays tight.
fn join_item(blocks: &[String]) -> String {
    let mut out = String::new();
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push_str(if is_list(b) { "\n" } else { "\n\n" });
        }
        out.push_str(b);
    }
    out
}

/// Whether a block written here is a list (text cannot look like one: its
/// punctuation is escaped).
fn is_list(block: &str) -> bool {
    let digits = block.chars().take_while(char::is_ascii_digit).count();
    block.starts_with("- ") || (digits > 0 && block[digits..].starts_with(". "))
}

/// Inline content being written: whitespace collapsed as a browser does.
#[derive(Debug, Default)]
struct Inline {
    out: String,
    /// Whitespace waiting for what follows it.
    pending: Pending,
    /// It began with whitespace.
    lead: bool,
    /// A line break is a space here (a heading, a table's cell).
    one_line: bool,
    /// In a table's cell: a `|` in code must be escaped.
    table: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Pending {
    #[default]
    None,
    Space,
    Break,
}

impl Inline {
    /// A text alone, as a paragraph.
    fn of_text(text: &str) -> String {
        let mut run = Inline::default();
        run.text(text);
        run.out
    }

    fn space(&mut self) {
        if self.out.is_empty() {
            self.lead = true;
        }
        self.pending = self.pending.max(Pending::Space);
    }

    fn line_break(&mut self) {
        if self.one_line {
            self.space();
        } else {
            self.pending = Pending::Break;
        }
    }

    /// Writes what waits, unless nothing comes before it.
    fn flush(&mut self) {
        if !self.out.is_empty() {
            match self.pending {
                Pending::None => {}
                Pending::Space => self.out.push(' '),
                Pending::Break => self.out.push_str("\\\n"),
            }
        }
        self.pending = Pending::None;
    }

    fn text(&mut self, text: &str) {
        for (i, word) in text.split_ascii_whitespace().enumerate() {
            if i > 0 || text.starts_with(|c: char| c.is_ascii_whitespace()) {
                self.space();
            }
            self.flush();
            self.out.push_str(&escape(word));
        }
        if text.ends_with(|c: char| c.is_ascii_whitespace()) {
            self.space();
        }
    }

    /// Markdown already written.
    fn raw(&mut self, markdown: &str) {
        self.flush();
        self.out.push_str(markdown);
    }

    fn code(&mut self, text: &str) {
        let fence = "`".repeat(backticks(text) + 1);
        let pad = if text.starts_with('`') || text.ends_with('`') {
            " "
        } else {
            ""
        };
        let text = if self.table {
            text.replace('|', "\\|")
        } else {
            text.to_string()
        };
        self.raw(&format!("{fence}{pad}{text}{pad}{fence}"));
    }

    /// Content between markers, its whitespace outside them; nothing when
    /// it is empty.
    fn wrapped(&mut self, inner: Inline, open: &str, close: &str) {
        if inner.lead {
            self.space();
        }
        if !inner.out.is_empty() {
            self.raw(&format!("{open}{}{close}", inner.out));
        }
        if inner.pending >= Pending::Space {
            self.space();
        }
    }

    fn finish_into(&mut self, out: &mut Vec<String>) {
        if !self.out.is_empty() {
            out.push(std::mem::take(&mut self.out));
        }
        *self = Inline::default();
    }
}

#[cfg(test)]
mod tests {
    use egui_pop_model::links::Policy;

    use super::super::page::{Markdown, schemes};
    use super::*;

    fn read_page(source: &str) -> Html {
        read(source, &schemes(&Policy::default(), "docs:page"))
    }

    fn md(source: &str) -> String {
        read_page(source).markdown
    }

    #[test]
    fn blocks_map_to_markdown() {
        assert_eq!(
            md("<h2 id=\"go\">Get <em>going</em></h2><p>One\n  two<br>three</p>"),
            "## Get *going* {#go}\n\nOne two\\\nthree"
        );
        assert_eq!(
            md("<ul><li>a<ul><li>b</li></ul></li><li>c</ul>"),
            "- a\n  - b\n- c"
        );
        assert_eq!(md("<ol start=3><li>a<li>b</ol>"), "3. a\n4. b");
        assert_eq!(
            md("<blockquote><p>a</p><p>b</p></blockquote>"),
            "> a\n>\n> b"
        );
        assert_eq!(md("<pre>a\n  b\n</pre>"), "```text\na\n  b\n```");
        assert_eq!(
            md(
                "<table><tr><th>A</th><th>B</th></tr><tr><td>1 | <code>x|y</code></td></tr></table>"
            ),
            "| A | B |\n| --- | --- |\n| 1 \\| `x\\|y` |  |"
        );
        assert_eq!(md("<dl><dt>Term<dd>Meaning</dl>"), "Term\n: Meaning");
        assert_eq!(md("a<hr>b"), "a\n\n---\n\nb");
        assert_eq!(
            md("<p>a <a href='guide.html#x'> b </a>c <img src='i.png' alt='I'></p>"),
            "a [b](<guide.html#x>) c ![I](<i.png>)"
        );
    }

    #[test]
    fn a_whole_document_reads_like_a_fragment() {
        let h = read_page(
            "<!doctype html><html><head><title>The  title</title><meta charset=utf-8>\
             <style>p{color:red}</style></head><body><p>Text</body></html>",
        );
        assert_eq!(h.markdown, "Text");
        assert_eq!(h.title.as_deref(), Some("The title"));
        // A stylesheet is the page's look, not its content.
        assert!(h.dropped.is_empty());
    }

    #[test]
    fn scripts_and_active_content_are_dropped_and_named() {
        let h = read_page(
            "<p>before</p><script>alert('x')</script><iframe src=https://e.example>f</iframe>\
             <form action=/x><input name=a><button>Send</button></form>\
             <video src=a.mp4>Your browser</video><template><p>hidden</p></template>\
             <svg><a href='https://e.example'><text>vector</text></a></svg>\
             <math><mi>x</mi></math><object data=a></object><p>after</p>",
        );
        assert_eq!(h.markdown, "before\n\nafter");
        assert_eq!(
            h.dropped,
            [
                "form", "iframe", "math", "object", "script", "svg", "template", "video"
            ]
        );
    }

    #[test]
    fn event_and_style_attributes_are_never_read() {
        let h = read_page(
            "<p onclick=\"alert(1)\" style=\"color:red\" class=c>Text</p>\
             <a href=\"https://example.org/\" onmouseover=\"alert(2)\" style=\"x\">Link</a>\
             <img src=a.png onerror=\"alert(3)\" alt=Alt>",
        );
        assert!(!h.markdown.contains("alert"), "{}", h.markdown);
        assert!(!h.markdown.contains("color"), "{}", h.markdown);
        let page = Markdown::parse(&h.markdown);
        assert_eq!(page.links(), ["https://example.org/"]);
    }

    #[test]
    fn dangerous_addresses_are_dropped_however_spelled() {
        for href in [
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            " javascript:alert(1)",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            "&#106;avascript:alert(1)",
            "javascript&colon;alert(1)",
            "vbscript:x",
            "data:text/html,<script>alert(1)</script>",
            "unknown:x",
        ] {
            let source = format!("<a href=\"{href}\">Label</a><img src=\"{href}\" alt=Alt>");
            let h = read_page(&source);
            let page = Markdown::parse(&h.markdown);
            assert_eq!(page.links(), Vec::<String>::new(), "{href}: {}", h.markdown);
            assert!(page.blocks[0].images.is_empty(), "{href}: {}", h.markdown);
            assert_eq!(h.markdown, "Label Alt".replace(' ', ""), "{href}");
        }
    }

    #[test]
    fn schemes_follow_the_policy_and_the_page() {
        let policy = Policy::default().app_scheme("helios");
        let docs = schemes(&policy, "docs:page");
        for s in [
            "https", "http", "mailto", "docs", "file", "command", "helios",
        ] {
            assert!(docs.contains(s), "{s}");
        }
        let web = schemes(&policy, "https://docs.example.org/a.html");
        assert!(!web.contains("command"));
        assert!(!web.contains("file"));
        assert!(web.contains("https"));
        let mut opened = Policy::default();
        opened.schemes.insert("javascript".into());
        opened.schemes.remove("http");
        let s = schemes(&opened, "docs:page");
        assert!(
            !s.contains("javascript"),
            "never, even when the host adds it"
        );
        assert!(!s.contains("http"));
        let links = |page: &str| {
            let h = read(
                "<a href='command:x'>c</a><img src='file:///etc/passwd' alt=f>",
                &schemes(&Policy::default(), page),
            );
            let m = Markdown::parse(&h.markdown);
            (m.links().len(), m.blocks[0].images.len())
        };
        assert_eq!(links("docs:page"), (1, 1));
        assert_eq!(links("https://docs.example.org/a.html"), (0, 0));
    }

    #[test]
    fn a_local_image_a_remote_page_may_not_load_is_named() {
        let web = schemes(&Policy::default(), "https://docs.example.org/a.html");
        let h = read(
            "<img src='file:///some/path.png' alt=Local><img src='data:x' alt=Data>",
            &web,
        );
        assert_eq!(h.dropped, ["file:///some/path.png"]);
        assert_eq!(h.markdown, "LocalData");
        let docs = schemes(&Policy::default(), "docs:page");
        let h = read("<img src='file:///some/path.png' alt=Local>", &docs);
        assert!(h.dropped.is_empty());
    }

    #[test]
    fn text_cannot_become_markup() {
        let source = "<p>[x](javascript:y) &lt;script&gt; *a* `b` ![i](j)</p><p># no</p><p>1. no</p>\
                      <p>- no</p><p>[r]: https://e.example</p>";
        let h = read_page(source);
        let page = Markdown::parse(&h.markdown);
        assert!(page.links().is_empty(), "{}", h.markdown);
        assert!(page.blocks.iter().all(|b| b.images.is_empty()));
        assert!(page.blocks.iter().all(|b| b.level.is_none()));
        assert_eq!(page.blocks.len(), 5, "{}", h.markdown);
        assert!(page.text().contains("[x](javascript:y) <script> *a* `b`"));
        assert!(page.references.is_empty());
    }

    #[test]
    fn broken_markup_is_read_as_a_browser_does() {
        assert_eq!(md("<p>one<p>two"), "one\n\ntwo");
        assert_eq!(
            md("<b>bold<i>both</b> italic</i>"),
            "**bold*both*** *italic*"
        );
        assert_eq!(md("<table><tr><td>a</table>text"), "| a |\n| --- |\n\ntext");
    }

    #[test]
    fn deep_nesting_stays_on_the_stack() {
        for tag in ["blockquote", "div", "ul", "span", "b", "table"] {
            let source = format!("<{tag}>").repeat(2_000) + "deep";
            let h = read_page(&source);
            assert!(h.markdown.contains("deep"), "{tag}");
            Markdown::parse(&h.markdown);
        }
    }

    #[test]
    fn anchors_and_titles_reach_the_page() {
        let h = read_page("<h1>One</h1><h2 id='two-x'>Two</h2><h2 id='bad id'>Three</h2>");
        let page = Markdown::parse(&h.markdown);
        assert_eq!(page.anchor("two-x"), Some(1));
        assert_eq!(page.anchor("three"), Some(2));
        assert_eq!(page.title.as_deref(), Some("One"));
    }
}
