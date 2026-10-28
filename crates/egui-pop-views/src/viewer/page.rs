//! A markdown page read once: cut into its top-level blocks (each drawn
//! on its own, so find in page and anchors know where they are), with
//! each block's plain text, its heading's anchor, its links and its
//! images. No egui here.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// The parser's options: egui_commonmark's, with heading attributes
/// (`# Title {#id}`), so both read a page the same way.
pub fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_HEADING_ATTRIBUTES
}

/// An image in a block: where its markdown is, its alt text and its
/// address as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageRef {
    /// Its bytes in the block's source.
    pub range: std::ops::Range<usize>,
    pub alt: String,
    pub dest: String,
}

/// A top-level block of a page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// Its markdown.
    pub source: String,
    /// Its text as read (for find in page), lower case.
    pub text: String,
    /// Its anchor, when it is a heading.
    pub anchor: Option<String>,
    /// Its heading's level (1 to 6).
    pub level: Option<u8>,
    /// The links' destinations, as written.
    pub links: Vec<String>,
    pub images: Vec<ImageRef>,
}

/// A markdown page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Markdown {
    pub blocks: Vec<Block>,
    /// The page's reference definitions (`[id]: target`), given to each
    /// block so a reference link drawn alone still resolves.
    pub references: String,
    /// The first heading's text.
    pub title: Option<String>,
}

impl Markdown {
    /// Reads a page.
    pub fn parse(source: &str) -> Markdown {
        let parser = Parser::new_ext(source, options());
        let mut references = String::new();
        for (label, def) in parser.reference_definitions().iter() {
            references.push_str(&format!("\n[{label}]: <{}>", def.dest));
        }
        let mut page = Markdown {
            references,
            ..Markdown::default()
        };
        let mut depth = 0usize;
        let mut start = 0usize;
        let mut cur = Pending::default();
        let mut used: Vec<String> = Vec::new();
        for (event, range) in parser.into_offset_iter() {
            match &event {
                Event::Start(tag) => {
                    if depth == 0 {
                        start = range.start;
                        cur = Pending::default();
                        if let Tag::Heading { level, id, .. } = tag {
                            cur.level = Some(level_of(*level));
                            cur.id = id.as_ref().map(ToString::to_string);
                        }
                    }
                    match tag {
                        Tag::Link { dest_url, .. } => cur.links.push(dest_url.to_string()),
                        Tag::Image { dest_url, .. } => {
                            cur.image = Some((range.clone(), dest_url.to_string(), String::new()));
                        }
                        _ => {}
                    }
                    depth += 1;
                }
                Event::End(end) => {
                    if matches!(end, TagEnd::Image)
                        && let Some((r, dest, alt)) = cur.image.take()
                    {
                        cur.images.push(ImageRef {
                            range: r.start - start..r.end - start,
                            alt,
                            dest,
                        });
                    }
                    if matches!(end, TagEnd::Paragraph | TagEnd::Item | TagEnd::TableCell) {
                        cur.text.push(' ');
                    }
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        page.push(
                            &source[start..range.end],
                            std::mem::take(&mut cur),
                            &mut used,
                        );
                    }
                }
                Event::Text(t) | Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t) => {
                    cur.text.push_str(t);
                    if let Some((_, _, alt)) = &mut cur.image {
                        alt.push_str(t);
                    }
                    if depth == 0 {
                        page.push(&source[range.clone()], std::mem::take(&mut cur), &mut used);
                    }
                }
                Event::SoftBreak | Event::HardBreak => cur.text.push(' '),
                Event::Rule | Event::Html(_) | Event::InlineHtml(_) if depth == 0 => {
                    page.push(&source[range.clone()], Pending::default(), &mut used);
                }
                _ => {}
            }
        }
        page
    }

    fn push(&mut self, source: &str, cur: Pending, used: &mut Vec<String>) {
        let heading = cur.level.map(|_| cur.text.trim().to_string());
        let anchor = heading.as_ref().map(|h| {
            let base = cur.id.clone().unwrap_or_else(|| slug(h));
            unique(base, used)
        });
        if self.title.is_none()
            && let Some(h) = &heading
        {
            self.title = Some(h.clone());
        }
        self.blocks.push(Block {
            source: source.to_string(),
            text: cur.text.to_lowercase(),
            anchor,
            level: cur.level,
            links: cur.links,
            images: cur.images,
        });
    }

    /// The block a heading's anchor names (`#install`, `install`).
    pub fn anchor(&self, name: &str) -> Option<usize> {
        let name = name.trim_start_matches('#');
        self.blocks
            .iter()
            .position(|b| b.anchor.as_deref() == Some(name))
            .or_else(|| {
                let s = slug(name);
                self.blocks
                    .iter()
                    .position(|b| b.anchor.as_deref() == Some(s.as_str()))
            })
    }

    /// Where `query` is found (case aside): one block index per match,
    /// in reading order.
    pub fn find(&self, query: &str) -> Vec<usize> {
        let q = query.to_lowercase();
        if q.trim().is_empty() {
            return Vec::new();
        }
        self.blocks
            .iter()
            .enumerate()
            .flat_map(|(i, b)| std::iter::repeat_n(i, b.text.matches(q.as_str()).count()))
            .collect()
    }

    /// Every link destination of the page, once each.
    pub fn links(&self) -> Vec<String> {
        let mut all: Vec<String> = self.blocks.iter().flat_map(|b| b.links.clone()).collect();
        all.sort();
        all.dedup();
        all
    }

    /// The page's text as read, for an agent.
    pub fn text(&self) -> String {
        self.blocks
            .iter()
            .map(|b| b.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Default)]
struct Pending {
    text: String,
    level: Option<u8>,
    id: Option<String>,
    links: Vec<String>,
    images: Vec<ImageRef>,
    image: Option<(std::ops::Range<usize>, String, String)>,
}

fn level_of(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// A heading's anchor as hosting sites make it: lower case, letters,
/// digits, `-` and `_` kept, spaces made `-`, the rest dropped.
pub fn slug(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

fn unique(base: String, used: &mut Vec<String>) -> String {
    let mut name = base.clone();
    let mut n = 1;
    while used.contains(&name) {
        name = format!("{base}-{n}");
        n += 1;
    }
    used.push(name.clone());
    name
}

/// Text written into markdown as it is (its punctuation escaped).
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A destination resolved against the page it is on: an anchor stays
/// an anchor (`#x`), an address with a scheme is kept, a relative one
/// (`guide.md`, `../img/a.png`) joins the page's folder: a `docs:` page
/// gives a `docs:` page, a web or file page joins as the web does.
pub fn resolve(base: &str, dest: &str) -> String {
    let dest = dest.trim();
    if dest.is_empty() || dest.starts_with('#') || has_scheme(dest) {
        return dest.to_string();
    }
    if let Some(page) = base.strip_prefix("docs:") {
        let page = page.split('#').next().unwrap_or_default();
        let (path, fragment) = match dest.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (dest, None),
        };
        let mut parts: Vec<&str> = if path.starts_with('/') {
            Vec::new()
        } else {
            let mut p: Vec<&str> = page.split('/').collect();
            p.pop();
            p
        };
        for seg in path.split('/') {
            match seg {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                s => parts.push(s),
            }
        }
        let mut out = format!("docs:{}", parts.join("/"));
        if let Some(f) = fragment {
            out.push('#');
            out.push_str(f);
        }
        return out;
    }
    match url::Url::parse(base).and_then(|b| b.join(dest)) {
        Ok(u) => u.to_string(),
        Err(_) => dest.to_string(),
    }
}

/// Whether an address names its scheme (`https:`, `docs:`, `mailto:`);
/// a Windows drive (`C:\\`) is not one.
pub fn has_scheme(text: &str) -> bool {
    match text.split_once(':') {
        Some((scheme, _)) => {
            scheme.len() > 1
                && scheme
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "\
# Getting started

Read [the guide](guide.md#install) and [the site][site].

![A screenshot](img/shot.png)

## Install

Run `cargo add`.

[site]: https://example.org/
";

    #[test]
    fn blocks_anchors_links_and_images() {
        let m = Markdown::parse(PAGE);
        assert_eq!(m.title.as_deref(), Some("Getting started"));
        assert_eq!(m.blocks.len(), 5);
        assert_eq!(m.blocks[0].anchor.as_deref(), Some("getting-started"));
        assert_eq!(m.anchor("#install"), Some(3));
        assert_eq!(
            m.blocks[1].links,
            ["guide.md#install", "https://example.org/"]
        );
        let img = &m.blocks[2].images[0];
        assert_eq!(
            (img.alt.as_str(), img.dest.as_str()),
            ("A screenshot", "img/shot.png")
        );
        assert_eq!(
            &m.blocks[2].source[img.range.clone()],
            "![A screenshot](img/shot.png)"
        );
        assert!(m.references.contains("[site]: <https://example.org/>"));
        assert_eq!(m.find("INSTALL"), [3]);
        assert_eq!(m.find("the"), [1, 1]);
    }

    #[test]
    fn destinations_resolve_against_their_page() {
        assert_eq!(
            resolve("docs:guide/start", "install.md"),
            "docs:guide/install.md"
        );
        assert_eq!(
            resolve("docs:guide/start", "../img/a.png"),
            "docs:img/a.png"
        );
        assert_eq!(resolve("docs:guide/start", "/index#top"), "docs:index#top");
        assert_eq!(resolve("docs:a", "#x"), "#x");
        assert_eq!(
            resolve("docs:a", "https://example.org/"),
            "https://example.org/"
        );
        assert_eq!(
            resolve("https://docs.example.org/a/b.md", "c.png"),
            "https://docs.example.org/a/c.png"
        );
        assert!(!has_scheme("C:\\photos"));
    }
}
