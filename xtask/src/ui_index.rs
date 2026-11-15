//! `ui-index`: the "use these" index, `docs/ui-index.md`, mapping a need
//! ("a search field", "an empty state") to the call that makes it.
//!
//! Generated from the public API, never kept by hand: an item of a crate
//! names what it is for with one doc line or more,
//!
//! ```text
//! /// Need (input): a search field.
//! ```
//!
//! and the index lists every such line, grouped by its group (one of
//! [`GROUPS`]), with the call, its path through the facade and the item's
//! first doc sentence. A new widget appears once its doc says what need it
//! answers; `the_committed_index_is_current` fails when the file is stale.
//!
//! The scanner reads source lines, not a syntax tree: the line after an
//! item's doc block and attributes is its declaration (`fn`, `struct`,
//! `enum`, `trait`, `type`, `const`); a method's type is the nearest
//! `impl`, or `trait`, above it at the file's first column.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The groups a need may name, in the index's order, with their titles.
pub const GROUPS: &[(&str, &str)] = &[
    ("text", "Text and titles"),
    ("layout", "Spacing, cards and scrolling"),
    ("action", "Buttons and actions"),
    ("input", "Fields and choices"),
    ("state", "Loading, empty and progress"),
    ("status", "Status marks and badges"),
    ("icon", "Icons and glyphs"),
    ("help", "Tooltips, hover cards and notices"),
    ("dialog", "Dialogs, menus and pages"),
    ("data", "Lists, trees and tables"),
    ("image", "Images and imaging"),
];

/// One need line, resolved to its item.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    /// One of [`GROUPS`]' keys.
    pub group: String,
    /// What the need is, as the line says it, without its final period.
    pub need: String,
    /// How it is called: `ui.section_title(..)`, `Input::search(..)`, `EmptyState`.
    pub call: String,
    /// Its full path, through the facade when the crate is re-exported.
    pub path: String,
    /// The item's first doc sentence.
    pub summary: String,
    /// Where it is declared: `crates/<crate>/src/<file>.rs`.
    pub source: String,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace root")
        .to_path_buf()
}

/// `cargo xtask ui-index [--check]`.
pub fn ui_index(args: &[String]) -> Result<(), String> {
    let check = match args {
        [] => false,
        [a] if a == "--check" => true,
        _ => return Err("usage: cargo xtask ui-index [--check]".into()),
    };
    let root = root();
    let text = render(&scan_workspace(&root)?);
    let path = root.join("docs/ui-index.md");
    if check {
        let old = fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if old != text {
            return Err("docs/ui-index.md is stale: run `cargo xtask ui-index`".into());
        }
        println!("docs/ui-index.md is current");
        return Ok(());
    }
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("docs/ui-index.md written from the `/// Need` lines");
    Ok(())
}

/// Every need line of the workspace's crates (`crates/*/src`).
pub fn scan_workspace(root: &Path) -> Result<Vec<Entry>, String> {
    let facade = facade_names(&root.join("crates/egui-pop/src/lib.rs"));
    let mut entries = Vec::new();
    let mut crates: Vec<PathBuf> = fs::read_dir(root.join("crates"))
        .map_err(|e| format!("crates/: {e}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("src").is_dir())
        .collect();
    crates.sort();
    for dir in crates {
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let base = crate_path(&name, &facade);
        let rel = format!("crates/{name}");
        entries.extend(scan_crate(&dir, &base, &rel)?);
    }
    entries.sort();
    Ok(entries)
}

/// The facade's re-exports: `pub use egui_pop_core as core;` maps
/// `egui_pop_core` to `core`.
pub fn facade_names(lib: &Path) -> BTreeMap<String, String> {
    let text = fs::read_to_string(lib).unwrap_or_default();
    let mut names = BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("pub use ") else {
            continue;
        };
        let Some((from, to)) = rest.trim_end_matches(';').split_once(" as ") else {
            continue;
        };
        if !from.contains("::") {
            names.insert(from.trim().to_owned(), to.trim().to_owned());
        }
    }
    names
}

/// A crate's path for users: `egui_pop::core` for a crate the facade
/// re-exports, its own name otherwise.
pub fn crate_path(package: &str, facade: &BTreeMap<String, String>) -> String {
    let ident = package.replace('-', "_");
    if ident == "egui_pop" {
        return ident;
    }
    match facade.get(&ident) {
        Some(short) => format!("egui_pop::{short}"),
        None => ident,
    }
}

/// Every need line of one crate: `dir` holds its `src/`, `base` is its
/// path for users, `rel` its directory as the index shows it.
pub fn scan_crate(dir: &Path, base: &str, rel: &str) -> Result<Vec<Entry>, String> {
    let src = dir.join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);
    files.sort();
    let mut entries = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        if !text.contains("Need (") {
            continue;
        }
        let modules = module_path(&src, &file);
        let mut path = base.to_owned();
        for m in &modules {
            path.push_str("::");
            path.push_str(m);
        }
        let shown = format!(
            "{rel}/src/{}",
            file.strip_prefix(&src)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/")
        );
        entries.extend(scan_source(&text, &path, &shown)?);
    }
    Ok(entries)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for e in read.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The public modules leading to `file` from the crate's root. A module
/// declared without `pub` is left out: its items are re-exported by its
/// parent (`pub use glyph::Glyph`), so they are reached there.
fn module_path(src: &Path, file: &Path) -> Vec<String> {
    let Ok(rel) = file.strip_prefix(src) else {
        return Vec::new();
    };
    let mut parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let last = parts.pop().unwrap_or_default();
    let stem = last.trim_end_matches(".rs");
    if stem != "mod" && stem != "lib" && stem != "main" {
        parts.push(stem.to_owned());
    }
    let mut out = Vec::new();
    let mut parent = src.to_path_buf();
    for (i, m) in parts.iter().enumerate() {
        let parent_file = if i == 0 {
            src.join("lib.rs")
        } else {
            let mod_rs = parent.join("mod.rs");
            if mod_rs.is_file() {
                mod_rs
            } else {
                parent.with_extension("rs")
            }
        };
        let text = fs::read_to_string(&parent_file).unwrap_or_default();
        let public = text.lines().any(|l| {
            let l = l.trim();
            l == format!("pub mod {m};") || l == format!("pub mod {m} {{")
        });
        if public {
            out.push(m.clone());
        }
        parent = parent.join(m);
    }
    out
}

/// The need lines of one source file. `path` is the file's module path,
/// `shown` the file as the index names it.
pub fn scan_source(text: &str, path: &str, shown: &str) -> Result<Vec<Entry>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut entries = Vec::new();
    let mut docs: Vec<&str> = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if let Some(doc) = line.strip_prefix("///") {
            docs.push(doc.strip_prefix(' ').unwrap_or(doc));
            continue;
        }
        if line.starts_with("#[") || line.starts_with("#!") {
            continue;
        }
        let block = std::mem::take(&mut docs);
        let needs = needs_of(&block, shown, i)?;
        if needs.is_empty() {
            continue;
        }
        let Some((kind, name)) = declaration(line) else {
            return Err(format!(
                "{shown}:{}: a `Need` line is not followed by an item",
                i + 1
            ));
        };
        let (call, full) = if kind == "fn" && raw.starts_with(char::is_whitespace) {
            let owner = owner_of(&lines[..i])
                .ok_or_else(|| format!("{shown}:{}: no `impl` or `trait` above `{name}`", i + 1))?;
            let takes_self = signature(&lines[i..]).contains("self");
            let call = match (&owner, takes_self) {
                (Owner::Trait(t), true) if t.starts_with("Ui") => format!("ui.{name}(..)"),
                (_, true) => format!(".{name}(..)"),
                (_, false) => format!("{}::{name}(..)", owner.name()),
            };
            let full = format!("{path}::{}::{name}", owner.name());
            (call, full)
        } else if kind == "fn" {
            let module = path.rsplit("::").next().unwrap_or(path);
            (format!("{module}::{name}(..)"), format!("{path}::{name}"))
        } else {
            (name.clone(), format!("{path}::{name}"))
        };
        let summary = summary_of(&block);
        for (group, need) in needs {
            entries.push(Entry {
                group,
                need,
                call: call.clone(),
                path: full.clone(),
                summary: summary.clone(),
                source: shown.to_owned(),
            });
        }
    }
    Ok(entries)
}

enum Owner {
    Trait(String),
    Type(String),
}

impl Owner {
    fn name(&self) -> &str {
        match self {
            Owner::Trait(n) | Owner::Type(n) => n,
        }
    }
}

/// The `impl` or `trait` a method at the end of `above` belongs to.
fn owner_of(above: &[&str]) -> Option<Owner> {
    for line in above.iter().rev() {
        if line.starts_with(char::is_whitespace) || line.is_empty() {
            continue;
        }
        let l = line.trim_start_matches("pub ");
        if let Some(rest) = l.strip_prefix("trait ") {
            return Some(Owner::Trait(ident(rest)));
        }
        if let Some(rest) = l.strip_prefix("impl") {
            let rest = skip_generics(rest.trim_start());
            let ty = match rest.split_once(" for ") {
                Some((_, ty)) => ty,
                None => rest,
            };
            return Some(Owner::Type(ident(ty.trim())));
        }
        if l.starts_with('}') || l.starts_with("//") || l.starts_with('#') {
            continue;
        }
    }
    None
}

/// `rest` without a leading `<...>`.
fn skip_generics(rest: &str) -> &str {
    if !rest.starts_with('<') {
        return rest;
    }
    let mut depth = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return rest[i + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    rest
}

/// The identifier `text` starts with (its last path segment).
fn ident(text: &str) -> String {
    let word: String = text
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
        .collect();
    let word = word.trim_end_matches(':');
    word.rsplit("::").next().unwrap_or(word).to_owned()
}

/// A function's signature, up to its body or its `;`.
fn signature(lines: &[&str]) -> String {
    let mut out = String::new();
    for l in lines {
        out.push_str(l.trim());
        out.push(' ');
        if l.contains('{') || l.trim_end().ends_with(';') {
            break;
        }
    }
    out
}

/// An item's kind and name, from its declaration line.
fn declaration(line: &str) -> Option<(String, String)> {
    let mut l = line;
    for prefix in ["pub(crate) ", "pub(super) ", "pub "] {
        l = l.strip_prefix(prefix).unwrap_or(l);
    }
    for prefix in ["const fn ", "async fn ", "unsafe fn "] {
        if let Some(rest) = l.strip_prefix(prefix) {
            return Some(("fn".into(), ident(rest)));
        }
    }
    for kind in ["fn", "struct", "enum", "trait", "type", "const", "static"] {
        if let Some(rest) = l.strip_prefix(kind).and_then(|r| r.strip_prefix(' ')) {
            return Some((kind.into(), ident(rest)));
        }
    }
    None
}

/// The `Need (group): need.` lines of a doc block.
fn needs_of(docs: &[&str], shown: &str, line: usize) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for d in docs {
        let Some(rest) = d.trim().strip_prefix("Need (") else {
            continue;
        };
        let Some((group, need)) = rest.split_once("):") else {
            return Err(format!("{shown}:{line}: write `Need (group): the need.`"));
        };
        if !GROUPS.iter().any(|(g, _)| *g == group) {
            let known: Vec<&str> = GROUPS.iter().map(|(g, _)| *g).collect();
            return Err(format!(
                "{shown}:{line}: unknown need group `{group}` (known: {})",
                known.join(", ")
            ));
        }
        let need = need.trim().trim_end_matches('.').trim();
        if need.is_empty() {
            return Err(format!("{shown}:{line}: an empty need"));
        }
        out.push((group.to_owned(), need.to_owned()));
    }
    Ok(out)
}

/// An item's first doc sentence, its links reduced to their text.
fn summary_of(docs: &[&str]) -> String {
    let mut para = String::new();
    for d in docs {
        let d = d.trim();
        if d.starts_with("Need (") {
            continue;
        }
        if d.is_empty() {
            if para.is_empty() {
                continue;
            }
            break;
        }
        if d.starts_with("```") {
            break;
        }
        if !para.is_empty() {
            para.push(' ');
        }
        para.push_str(d);
    }
    let para = unlink(&para);
    let end = sentence_end(&para);
    para[..end].trim().to_owned()
}

/// The end of the first sentence: a period followed by a space, outside
/// backticks and parentheses, or the whole text.
fn sentence_end(text: &str) -> usize {
    let mut code = false;
    let mut depth = 0i32;
    let bytes = text.as_bytes();
    for (i, c) in text.char_indices() {
        match c {
            '`' => code = !code,
            '(' if !code => depth += 1,
            ')' if !code => depth -= 1,
            '.' if !code && depth <= 0 => {
                let next = bytes.get(i + 1).copied();
                if next.is_none() || next == Some(b' ') {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    text.len()
}

/// `[`X`]` and `[text](link)` / `[text][ref]` reduced to their text.
fn unlink(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else {
            out.push_str(&rest[open..]);
            return out;
        };
        out.push_str(&after[..close]);
        let mut tail = &after[close + 1..];
        for (o, c) in [('(', ')'), ('[', ']')] {
            if tail.starts_with(o)
                && let Some(end) = tail.find(c)
            {
                tail = &tail[end + 1..];
                break;
            }
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// The index's Markdown.
pub fn render(entries: &[Entry]) -> String {
    let mut out = String::from(
        "# UI index: what to use for a need\n\n\
         <!-- Generated by `cargo xtask ui-index` from the `/// Need (group): ...`\n\
         lines of the public API. Do not edit: add or change a `Need` line on the\n\
         item, then run the task again. A test fails when this file is stale. -->\n\n\
         Before drawing anything with egui's own widgets, look here: each row maps\n\
         a need to the egui-pop call that answers it, themed by the kit, named\n\
         for screen readers and agents. A need missing here is a gap to fill in\n\
         egui-pop, not a reason to hand-draw it.\n\n\
         `ui.*` methods come from the [`UiPop`] trait, in `egui_pop::prelude`.\n\
         To add an entry, give the item a doc line\n\
         `/// Need (group): a search field.`, the group one of: ",
    );
    let keys: Vec<String> = GROUPS.iter().map(|(g, _)| format!("`{g}`")).collect();
    out.push_str(&keys.join(", "));
    out.push_str(
        ".\n\n[`UiPop`]: https://docs.rs/egui-pop-core/latest/egui_pop_core/ui/trait.UiPop.html\n",
    );
    for (group, title) in GROUPS {
        let rows: Vec<&Entry> = entries.iter().filter(|e| e.group == *group).collect();
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "\n## {title}\n\n| Need | Use | What it is |\n|---|---|---|\n"
        ));
        let mut last = "";
        for e in rows {
            let need = if e.need == last { "" } else { e.need.as_str() };
            last = &e.need;
            out.push_str(&format!(
                "| {} | `{}`<br>`{}` | {} |\n",
                cell(need),
                cell(&e.call),
                cell(&e.path),
                cell(&e.summary)
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_index_is_current() {
        let root = root();
        let entries = scan_workspace(&root).expect("the need lines parse");
        assert!(
            entries.len() >= 20,
            "the index lost its entries: {}",
            entries.len()
        );
        let text = render(&entries);
        let committed = fs::read_to_string(root.join("docs/ui-index.md"))
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            committed == text,
            "docs/ui-index.md is stale: run `cargo xtask ui-index` and commit it"
        );
    }

    #[test]
    fn a_need_line_names_its_item_its_call_and_its_summary() {
        let src = "\
pub trait UiPop: sealed::Sealed {
    /// A section's title (`text-role.section`). More.
    /// Need (text): a section title.
    fn section_title(&mut self, text: impl Into<String>) -> Response;
}

impl<'a> Input<'a> {
    /// A search field: a [`Kind::Search`] input.
    ///
    /// Need (input): a search field.
    /// Need (input): a filter box.
    #[must_use]
    pub fn search(value: &'a mut String, label: &str) -> Self {
        todo!()
    }
}

/// An empty place. See [the module](self).
/// Need (state): an empty state.
#[derive(Debug)]
pub struct EmptyState {
}
";
        let got = scan_source(src, "egui_pop::core::x", "x.rs").expect("parses");
        let calls: Vec<(&str, &str, &str)> = got
            .iter()
            .map(|e| (e.need.as_str(), e.call.as_str(), e.summary.as_str()))
            .collect();
        assert_eq!(
            calls,
            [
                (
                    "a section title",
                    "ui.section_title(..)",
                    "A section's title (`text-role.section`)."
                ),
                (
                    "a search field",
                    "Input::search(..)",
                    "A search field: a `Kind::Search` input."
                ),
                (
                    "a filter box",
                    "Input::search(..)",
                    "A search field: a `Kind::Search` input."
                ),
                ("an empty state", "EmptyState", "An empty place."),
            ]
        );
        assert_eq!(got[0].path, "egui_pop::core::x::UiPop::section_title");
        assert_eq!(got[1].path, "egui_pop::core::x::Input::search");
        assert_eq!(got[3].path, "egui_pop::core::x::EmptyState");
    }

    #[test]
    fn an_unknown_group_is_refused() {
        let src = "/// Need (stuff): a thing.\npub struct Thing;\n";
        let err = scan_source(src, "p", "f.rs").expect_err("unknown group");
        assert!(err.contains("unknown need group `stuff`"), "{err}");
    }
}
