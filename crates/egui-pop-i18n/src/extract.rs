//! The extractor: which keys an application uses, with their English;
//! the English catalogue written from them; and, per locale, the keys
//! missing or unused.
//!
//! Keys come from two places:
//!
//! - **sources** ([`scan_source`], [`scan_dir`]): calls written in Rust
//!   with a literal key, `tr!("key")`, `t(ctx, "key", …)`,
//!   `text(ctx, "key", "English", …)`, `StrKey::new("key", "English")`,
//!   `lookup(ctx, "key", "English")` (see [`Pattern::DEFAULTS`]; an
//!   application adds its own);
//! - **declarations** ([`declarations`]): what the plugins of a built
//!   application declare (commands, categories, panels, submenus,
//!   settings and their choices, plugins), under the keys
//!   `egui_pop_core::strings` derives from their ids.
//!
//! [`english`] writes the English catalogue from them, sorted, so the
//! same keys always give the same bytes; [`update`] merges them into an
//! existing one, keeping the English written there for keys the code
//! gives none; [`report`] compares every locale with them.
//!
//! ```
//! use egui_pop_i18n::extract::{self, Pattern};
//!
//! let src = r#"
//!     ui.label(tr!("welcome"));
//!     const SAVE: StrKey = StrKey::new("app-save", "Save { $name }");
//! "#;
//! let found = extract::scan_source(src, "src/lib.rs", Pattern::DEFAULTS);
//! let messages = extract::messages(&found).unwrap();
//! let ftl = extract::english(&messages, "");
//! assert!(ftl.contains("app-save = Save { $name }\n"));
//! assert!(ftl.contains("welcome = welcome\n"), "{ftl}");
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io;
use std::path::Path;

use egui_pop_api::AppHandle;
use egui_pop_api::app::MENU_MAIN;
use egui_pop_core::strings::{self, kind};
use egui_pop_model::ids::MenuId;
use egui_pop_model::menus::Item;
use fluent_syntax::ast::{Entry, Identifier, Message, Pattern as FtlPattern, Resource};
use serde::{Deserialize, Serialize};

/// A key's English, as found.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "snake_case")]
pub enum English {
    /// Text to show as is (a declaration's title): braces are escaped
    /// when written to Fluent.
    Plain(String),
    /// A Fluent pattern (`Save { $name }`), written as is.
    Pattern(String),
    /// The code gives none (`tr!("key")`): the catalogue's English is
    /// kept, or the key is written as a placeholder.
    None,
}

/// A key found, with its English and where.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    pub key: String,
    pub english: English,
    /// `file:line`, or what declares it (`command app.quit`).
    pub origin: String,
}

/// A call whose arguments hold a key, and maybe its English.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pattern {
    /// What is called, as written before `(` (`tr!`, `t`,
    /// `StrKey::new`). A name without a path also matches with one
    /// (`t` matches `egui_pop_i18n::t`).
    pub callee: &'static str,
    /// Which argument is the key (from 0).
    pub key: usize,
    /// Which argument is the English, if any.
    pub english: Option<usize>,
}

impl Pattern {
    /// `tr!("key", …)`, `t(ctx, "key", …)`, `text(ctx, "key",
    /// "English", …)`, `StrKey::new("key", "English")`, `lookup(ctx,
    /// "key", "English")` and `lookup_args(ctx, "key", "English", …)`.
    pub const DEFAULTS: &'static [Pattern] = &[
        Pattern {
            callee: "tr!",
            key: 0,
            english: None,
        },
        Pattern {
            callee: "t",
            key: 1,
            english: None,
        },
        Pattern {
            callee: "text",
            key: 1,
            english: Some(2),
        },
        Pattern {
            callee: "StrKey::new",
            key: 0,
            english: Some(1),
        },
        Pattern {
            callee: "lookup",
            key: 1,
            english: Some(2),
        },
        Pattern {
            callee: "lookup_args",
            key: 1,
            english: Some(2),
        },
    ];
}

/// A key as Fluent allows it: an identifier, optionally `.attribute`.
pub fn valid_key(key: &str) -> bool {
    let ident = |s: &str| {
        let mut c = s.chars();
        c.next().is_some_and(|f| f.is_ascii_alphabetic())
            && c.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    match key.split_once('.') {
        Some((id, attr)) => ident(id) && ident(attr),
        None => ident(key),
    }
}

/// Source with comments and the contents of string literals other than
/// the ones we read blanked, so calls are found by their text.
fn tokens(source: &str) -> Vec<Tok> {
    let b = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            line += 1;
            i += 1;
        } else if b[i..].starts_with(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i..].starts_with(b"/*") {
            i += 2;
            while i < b.len() && !b[i..].starts_with(b"*/") {
                if b[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 2;
        } else if c == b'"' || (c == b'r' && matches!(b.get(i + 1), Some(b'"' | b'#'))) {
            let start_line = line;
            let (text, end) = string_literal(source, i);
            line += source[i..end].matches('\n').count();
            out.push(Tok::Str(text, start_line));
            i = end;
        } else if c == b'\'' {
            // A char literal or a lifetime: skip `'x'` and `'\n'`.
            if b.get(i + 2) == Some(&b'\'') {
                i += 3;
            } else if b.get(i + 1) == Some(&b'\\') {
                i += 2;
                while i < b.len() && b[i] != b'\'' {
                    i += 1;
                }
                i += 1;
            } else {
                i += 1;
            }
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Ident(source[start..i].to_string(), line));
        } else if c.is_ascii_whitespace() {
            i += 1;
        } else {
            out.push(Tok::Punct(c as char));
            i += 1;
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String, usize),
    Str(String, usize),
    Punct(char),
}

/// A string literal at `i` (plain or raw), unescaped, and where it ends.
fn string_literal(source: &str, i: usize) -> (String, usize) {
    let b = source.as_bytes();
    if b[i] == b'r' {
        let mut j = i + 1;
        let mut hashes = 0;
        while b.get(j) == Some(&b'#') {
            hashes += 1;
            j += 1;
        }
        let start = j + 1;
        let close = format!("\"{}", "#".repeat(hashes));
        let end = source[start..]
            .find(&close)
            .map_or(source.len(), |p| start + p);
        return (
            source[start..end].to_string(),
            (end + close.len()).min(source.len()),
        );
    }
    let mut out = String::new();
    let mut j = i + 1;
    let chars: Vec<(usize, char)> = source[j..].char_indices().collect();
    let mut k = 0;
    while k < chars.len() {
        let (off, c) = chars[k];
        match c {
            '"' => return (out, j + off + 1),
            '\\' => {
                k += 1;
                let Some(&(_, e)) = chars.get(k) else { break };
                match e {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    '\'' => out.push('\''),
                    '0' => out.push('\0'),
                    '\n' => {
                        // A line continuation: the next line's indent goes.
                        while chars.get(k + 1).is_some_and(|(_, c)| c.is_whitespace()) {
                            k += 1;
                        }
                    }
                    'u' => {
                        let hex: String = chars[k + 1..]
                            .iter()
                            .map(|(_, c)| *c)
                            .skip(1)
                            .take_while(|c| *c != '}')
                            .collect();
                        if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                        {
                            out.push(c);
                        }
                        k += hex.len() + 2;
                    }
                    other => out.push(other),
                }
            }
            c => out.push(c),
        }
        k += 1;
    }
    j = source.len();
    (out, j)
}

/// The keys a Rust source uses through `patterns`, with their English.
/// A call whose key is not a literal, or not a valid Fluent key, is
/// skipped.
pub fn scan_source(source: &str, file: &str, patterns: &[Pattern]) -> Vec<Found> {
    let toks = tokens(source);
    let mut out = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        let Tok::Ident(name, line) = t else { continue };
        // The callee as written: a path `a::b`, then `!` for a macro.
        let mut callee = name.clone();
        let mut j = i + 1;
        while matches!(toks.get(j), Some(Tok::Punct(':')))
            && matches!(toks.get(j + 1), Some(Tok::Punct(':')))
        {
            match toks.get(j + 2) {
                Some(Tok::Ident(n, _)) => {
                    callee.push_str("::");
                    callee.push_str(n);
                    j += 3;
                }
                _ => break,
            }
        }
        if matches!(toks.get(j), Some(Tok::Punct('!'))) {
            callee.push('!');
            j += 1;
        }
        if !matches!(toks.get(j), Some(Tok::Punct('('))) {
            continue;
        }
        // Not a method or a definition: `x.t(`, `fn t(`.
        if i > 0
            && (matches!(toks.get(i - 1), Some(Tok::Punct('.')))
                || matches!(toks.get(i - 1), Some(Tok::Ident(k, _)) if k == "fn"))
        {
            continue;
        }
        // Inside a path, only the last segments are matched.
        if i > 1
            && matches!(toks.get(i - 1), Some(Tok::Punct(':')))
            && matches!(toks.get(i - 2), Some(Tok::Punct(':')))
        {
            continue;
        }
        let Some(p) = patterns.iter().find(|p| {
            p.callee == callee
                || callee
                    .rsplit_once("::")
                    .is_some_and(|(_, last)| p.callee == last)
        }) else {
            continue;
        };
        let args = arguments(&toks, j);
        let literal = |n: usize| match args.get(n) {
            Some(a) if a.len() == 1 => match &a[0] {
                Tok::Str(s, _) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        };
        let Some(key) = literal(p.key).filter(|k| valid_key(k)) else {
            continue;
        };
        let english = match p.english.and_then(literal) {
            Some(e) => English::Pattern(e),
            None => English::None,
        };
        out.push(Found {
            key,
            english,
            origin: format!("{file}:{line}"),
        });
    }
    out
}

/// The arguments of the call whose `(` is at `open`, each as its tokens.
fn arguments(toks: &[Tok], open: usize) -> Vec<Vec<Tok>> {
    let mut out = vec![Vec::new()];
    let mut depth = 0;
    for t in &toks[open..] {
        match t {
            Tok::Punct('(' | '[' | '{') => {
                depth += 1;
                if depth == 1 {
                    continue;
                }
            }
            Tok::Punct(')' | ']' | '}') => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Tok::Punct(',') if depth == 1 => {
                out.push(Vec::new());
                continue;
            }
            _ => {}
        }
        if let Some(last) = out.last_mut() {
            last.push(t.clone());
        }
    }
    out
}

/// [`scan_source`] over every `.rs` file under `dir` (`target` and hidden
/// directories skipped), files in a stable order; origins are relative
/// to `dir`.
pub fn scan_dir(dir: &Path, patterns: &[Pattern]) -> io::Result<Vec<Found>> {
    let mut files = Vec::new();
    walk(dir, &mut files)?;
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let source = std::fs::read_to_string(&f)?;
        let name = f.strip_prefix(dir).unwrap_or(&f).display().to_string();
        out.extend(scan_source(&source, &name, patterns));
    }
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if p.is_dir() {
            if name != "target" && !name.starts_with('.') {
                walk(&p, out)?;
            }
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
    Ok(())
}

/// What a built application declares, under the keys
/// `egui_pop_core::strings` derives: commands (title, description,
/// category; a panel's `focus` and `toggle` commands only their
/// category, their texts being shared), panels, submenus (from the main menu and `menus`),
/// settings (label, description, each choice) and plugins (title).
pub fn declarations(app: &AppHandle, menus: &[MenuId]) -> Vec<Found> {
    let mut out = Vec::new();
    let mut push = |key: String, text: &str, origin: String| {
        if !text.trim().is_empty() {
            out.push(Found {
                key,
                english: English::Plain(text.to_string()),
                origin,
            });
        }
    };
    let ctx = app.egui();
    for (spec, _) in app.commands() {
        let id = spec.id.as_str();
        let origin = format!("command {id}");
        // A panel's commands read texts shared by every panel (egui-pop's
        // catalogue has them): nothing of their own to translate.
        if strings::panel_command(&ctx, id).is_none() {
            push(
                strings::declared_key(kind::COMMAND, id, Some("title")),
                &spec.title,
                origin.clone(),
            );
            push(
                strings::declared_key(kind::COMMAND, id, Some("description")),
                &spec.description,
                origin.clone(),
            );
        }
        push(
            strings::category_key(&spec.category),
            &spec.category,
            origin,
        );
    }
    for p in app.panels() {
        push(
            strings::declared_key(kind::PANEL, p.id.as_str(), Some("title")),
            &p.title,
            format!("panel {}", p.id),
        );
    }
    let mut seen = BTreeSet::new();
    let mut todo: Vec<MenuId> = vec![MENU_MAIN];
    todo.extend(menus.iter().cloned());
    while let Some(menu) = todo.pop() {
        if !seen.insert(menu.clone()) {
            continue;
        }
        for item in app.menu(&menu) {
            if let Item::Submenu { menu: sub, label } = &item.item {
                push(
                    strings::declared_key(kind::MENU, sub.as_str(), Some("label")),
                    label,
                    format!("menu {sub}"),
                );
                todo.push(sub.clone());
            }
        }
    }
    for s in app.settings() {
        let id = s.id.as_str();
        let origin = format!("setting {id}");
        push(
            strings::declared_key(kind::SETTING, id, Some("label")),
            &s.descriptor.label,
            origin.clone(),
        );
        push(
            strings::declared_key(kind::SETTING, id, Some("description")),
            &s.descriptor.description,
            origin.clone(),
        );
        for c in &s.descriptor.choices {
            push(strings::setting_choice_key(id, c), c, origin.clone());
        }
    }
    for p in app.plugins().into_iter().filter(|p| p.connected) {
        push(
            strings::declared_key(kind::PLUGIN, p.id.as_str(), Some("title")),
            &p.title,
            format!("plugin {}", p.id),
        );
    }
    out
}

/// Keys with their English, deduplicated.
pub type Messages = BTreeMap<String, English>;

/// What was found, by key. Refused when one key is given two different
/// English texts (two ids that differ only by punctuation, one key used
/// with two texts); categories, keyed by their English, keep the first.
pub fn messages(found: &[Found]) -> Result<Messages, String> {
    let mut out: Messages = BTreeMap::new();
    let mut origin: BTreeMap<&str, &str> = BTreeMap::new();
    let mut errors = Vec::new();
    for f in found {
        match out.get(&f.key) {
            None => {
                out.insert(f.key.clone(), f.english.clone());
                origin.insert(&f.key, &f.origin);
            }
            Some(English::None) => {
                out.insert(f.key.clone(), f.english.clone());
            }
            Some(e) if *e == f.english || f.english == English::None => {}
            Some(_) if f.key.starts_with("category-") => {}
            Some(e) => errors.push(format!(
                "`{}` has two English texts: {e:?} ({}) and {:?} ({})",
                f.key,
                origin.get(f.key.as_str()).unwrap_or(&"?"),
                f.english,
                f.origin
            )),
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors.join("\n"))
    }
}

/// Text as a Fluent pattern that formats back to itself: braces are
/// string literals, continuation lines indented, and a continuation line
/// that would read as syntax quoted.
pub fn escape(text: &str) -> String {
    let text = text.trim();
    let mut out = String::new();
    for (n, line) in text.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        let mut chars = line.trim_start().chars().peekable();
        if n > 0 && matches!(chars.peek(), Some('[' | '*' | '.')) {
            let c = chars.next().expect("peeked");
            let _ = write!(out, "{{\"{c}\"}}");
        }
        for c in chars {
            match c {
                '{' => out.push_str("{\"{\"}"),
                '}' => out.push_str("{\"}\"}"),
                c => out.push(c),
            }
        }
    }
    out
}

/// A Fluent pattern written as a value: continuation lines indented.
fn indent(pattern: &str) -> String {
    pattern.trim().replace('\n', "\n    ")
}

/// The pattern source of each key of a Fluent file, as written.
fn existing(source: &str) -> BTreeMap<String, String> {
    let resource = match fluent_syntax::parser::parse(source) {
        Ok(r) | Err((r, _)) => r,
    };
    let mut out = BTreeMap::new();
    for entry in &resource.body {
        if let Entry::Message(m) = entry {
            if let Some(v) = &m.value {
                out.insert(m.id.name.to_string(), serialize(v));
            }
            for a in &m.attributes {
                out.insert(format!("{}.{}", m.id.name, a.id.name), serialize(&a.value));
            }
        }
    }
    out
}

/// A pattern's source as Fluent's serializer writes it.
pub(crate) fn serialize(pattern: &FtlPattern<&str>) -> String {
    let resource = Resource {
        body: vec![Entry::Message(Message {
            id: Identifier { name: "m" },
            value: Some(pattern.clone()),
            attributes: Vec::new(),
            comment: None,
        })],
    };
    let s = fluent_syntax::serializer::serialize(&resource);
    let s = s.strip_prefix("m =").unwrap_or(&s);
    // A value written on its own lines comes back indented: unindent it.
    s.trim_end()
        .lines()
        .map(|l| l.strip_prefix("    ").unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Writes keys with their pattern sources as a Fluent file, messages
/// sorted by id, attributes under their message.
fn write(header: &str, patterns: &BTreeMap<String, String>) -> String {
    let mut messages: BTreeMap<&str, (Option<&str>, BTreeMap<&str, &str>)> = BTreeMap::new();
    for (key, pattern) in patterns {
        let (id, attr) = crate::catalog::split_key(key);
        let entry = messages.entry(id).or_default();
        match attr {
            Some(a) => {
                entry.1.insert(a, pattern);
            }
            None => entry.0 = Some(pattern),
        }
    }
    let mut out = String::from(header);
    for (id, (value, attrs)) in messages {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(id);
        out.push_str(" =");
        if let Some(v) = value {
            out.push(' ');
            out.push_str(&indent(v));
        }
        out.push('\n');
        for (a, v) in attrs {
            let _ = writeln!(out, "    .{a} = {}", indent(v).replace('\n', "\n    "));
        }
    }
    out
}

/// A key's English as a Fluent pattern; `None` when the code gives none.
fn pattern_of(english: &English) -> Option<String> {
    match english {
        English::Plain(t) => Some(escape(t)),
        English::Pattern(p) => Some(p.trim().to_string()),
        English::None => None,
    }
}

/// The English catalogue of `messages`: sorted, the same keys giving
/// the same bytes; a key without English reads as itself. `header` (a
/// Fluent comment, `# …` lines) comes first.
pub fn english(messages: &Messages, header: &str) -> String {
    let patterns = messages
        .iter()
        .map(|(k, e)| (k.clone(), pattern_of(e).unwrap_or_else(|| k.clone())))
        .collect();
    write(header, &patterns)
}

/// An English catalogue brought up to date.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Updated {
    /// The new source.
    pub source: String,
    /// Keys that were not in it.
    pub added: Vec<String>,
    /// Keys whose English the code changed.
    pub changed: Vec<String>,
    /// Keys in it that the code no longer uses (dropped when pruning).
    pub unused: Vec<String>,
}

/// `messages` merged into the English catalogue `source`: the code's
/// English wins; for a key the code gives none, the catalogue's is kept
/// (a new one reads as its key). Unused keys are kept unless `prune`.
/// The result is sorted like [`english`].
pub fn update(source: &str, messages: &Messages, header: &str, prune: bool) -> Updated {
    let old = existing(source);
    let mut out = Updated::default();
    let mut patterns = BTreeMap::new();
    for (key, english) in messages {
        let new = pattern_of(english);
        let pattern = match (old.get(key), new) {
            (None, new) => {
                out.added.push(key.clone());
                new.unwrap_or_else(|| key.clone())
            }
            (Some(was), Some(new)) => {
                if canonical(was) != canonical(&new) {
                    out.changed.push(key.clone());
                }
                new
            }
            (Some(was), None) => was.clone(),
        };
        patterns.insert(key.clone(), pattern);
    }
    for (key, was) in &old {
        if !messages.contains_key(key) {
            out.unused.push(key.clone());
            if !prune {
                patterns.insert(key.clone(), was.clone());
            }
        }
    }
    out.source = write(header, &patterns);
    out
}

/// A pattern as Fluent's serializer writes it, to compare two.
fn canonical(pattern: &str) -> String {
    existing(&format!("m = {}\n", indent(pattern)))
        .remove("m")
        .unwrap_or_else(|| pattern.to_string())
}

/// One locale against what the code uses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LocaleKeys {
    pub locale: String,
    /// Used by the code, not in this locale's catalogue.
    pub missing: Vec<String>,
    /// In this locale's catalogue, not used by the code.
    pub unused: Vec<String>,
}

/// Every locale of `catalogs` against the keys the code uses.
pub fn report(catalogs: &crate::check::Catalogs, messages: &Messages) -> Vec<LocaleKeys> {
    let mut keys: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for f in &catalogs.files {
        keys.entry(&f.locale)
            .or_default()
            .extend(existing(&f.source).into_keys());
    }
    keys.into_iter()
        .map(|(locale, theirs)| LocaleKeys {
            locale: locale.to_string(),
            missing: messages
                .keys()
                .filter(|k| !theirs.contains(*k))
                .cloned()
                .collect(),
            unused: theirs
                .iter()
                .filter(|k| !messages.contains_key(*k))
                .cloned()
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_are_found_with_their_english() {
        let src = r##"
            // tr!("in-a-comment")
            fn t(x: &str) {}
            let a = tr!("plain");
            let b = egui_pop_i18n::t(&ctx, "with-path", &args![n = 1]);
            let c = obj.t(&ctx, "a-method");
            const K: StrKey = StrKey::new(
                "multi-line",
                "Line { $n }",
            );
            let d = strings::lookup(ui.ctx(), "lookup-key", r#"Raw "quoted""#);
            let e = tr!(dynamic_key);
            let f = tr!("not a key");
            let g = text(ctx, "attr-key.title", "Esc\"aped\n", &[]);
        "##;
        let found = scan_source(src, "x.rs", Pattern::DEFAULTS);
        let keys: Vec<(&str, &English)> =
            found.iter().map(|f| (f.key.as_str(), &f.english)).collect();
        assert_eq!(
            keys,
            [
                ("plain", &English::None),
                ("with-path", &English::None),
                ("multi-line", &English::Pattern("Line { $n }".into())),
                ("lookup-key", &English::Pattern("Raw \"quoted\"".into())),
                ("attr-key.title", &English::Pattern("Esc\"aped\n".into())),
            ]
        );
        assert_eq!(found[0].origin, "x.rs:4");
        assert_eq!(found[2].origin, "x.rs:7");
    }

    #[test]
    fn the_english_catalogue_is_sorted_and_round_trips() {
        let found = vec![
            Found {
                key: "command-b.title".into(),
                english: English::Plain("Use {braces}".into()),
                origin: "command b".into(),
            },
            Found {
                key: "a".into(),
                english: English::Pattern("Hi { $name }".into()),
                origin: "x.rs:1".into(),
            },
            Found {
                key: "command-b.description".into(),
                english: English::Plain("Two\nlines".into()),
                origin: "command b".into(),
            },
        ];
        let m = messages(&found).unwrap();
        let ftl = english(&m, "# Generated.\n");
        assert_eq!(
            ftl,
            "# Generated.\n\na = Hi { $name }\n\ncommand-b =\n    .description = Two\n        lines\n    .title = Use {\"{\"}braces{\"}\"}\n"
        );
        let i = crate::I18n::new("en");
        i.add("en", &ftl).unwrap();
        assert_eq!(i.t("command-b.title", &[]), "Use {braces}");
        assert_eq!(i.t("command-b.description", &[]), "Two\nlines");
        let mut twice = found.clone();
        twice.push(Found {
            key: "a".into(),
            english: English::Pattern("Other".into()),
            origin: "y.rs:2".into(),
        });
        assert!(messages(&twice).unwrap_err().contains("x.rs:1"));
    }

    #[test]
    fn update_keeps_written_english_and_reports_changes() {
        let old = "# h\n\nkept = Written by hand\nold = Gone\nchanged = Before\n";
        let mut m = Messages::new();
        m.insert("kept".into(), English::None);
        m.insert("changed".into(), English::Pattern("After".into()));
        m.insert("new".into(), English::None);
        let u = update(old, &m, "# h\n", false);
        assert_eq!(u.added, ["new"]);
        assert_eq!(u.changed, ["changed"]);
        assert_eq!(u.unused, ["old"]);
        assert!(u.source.contains("kept = Written by hand\n"));
        assert!(u.source.contains("new = new\n"));
        assert!(u.source.contains("old = Gone\n"));
        let pruned = update(old, &m, "# h\n", true);
        assert!(!pruned.source.contains("old ="));
        // Up to date: nothing changes.
        let again = update(&u.source, &m, "# h\n", false);
        assert_eq!(again.source, u.source);
        assert!(again.added.is_empty() && again.changed.is_empty());
    }

    #[test]
    fn multi_line_patterns_survive_a_rewrite() {
        let old = "plural =\n    { $n ->\n        [one] one\n       *[other] many\n    }\n";
        let mut m = Messages::new();
        m.insert("plural".into(), English::None);
        let u = update(old, &m, "", false);
        let i = crate::I18n::new("en");
        i.add("en", &u.source).unwrap();
        assert_eq!(i.t("plural", &crate::args![n = 1]), "one");
        assert_eq!(i.t("plural", &crate::args![n = 5]), "many");
    }
}
