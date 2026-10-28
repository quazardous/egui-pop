//! The extractor: which keys an application uses, with their English;
//! the English catalogue written from them; and, per locale, the keys
//! missing or unused.
//!
//! Keys come from two places:
//!
//! - **sources** ([`scan`], [`scan_tree`]): calls written in Rust
//!   with a literal key, `tr!("key")`, `t(ctx, "key", …)`,
//!   `text(ctx, "key", "English", …)`, `StrKey::new("key", "English")`,
//!   `lookup(ctx, "key", "English")` (see [`Pattern::DEFAULTS`]; an
//!   application adds its own), under any path (`app::Message::new`);
//!   a constructor call it cannot read is reported ([`Scan::unread`]);
//! - **declarations** ([`declarations`]): what the plugins of a built
//!   application declare (commands, categories, panels, submenus,
//!   settings and their choices, plugins), under the keys
//!   `egui_pop_core::strings` derives from their ids.
//!
//! [`english`] writes the English catalogue from them, sorted, so the
//! same keys always give the same bytes; [`update`] merges them into an
//! existing one, keeping the English written there for keys the code
//! gives none; [`out_of_date`] checks a catalogue against them, listing
//! the calls not read; [`report`] compares every locale with them.
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

/// A call the scanner took for a message constructor but could not
/// read: the message it declares is missing from what was found, or
/// found without its English.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unread {
    /// The callee as written, path included (`app::Message::new`).
    pub callee: String,
    /// `file:line` of the call.
    pub origin: String,
    /// Why it was not read (`the key is not a string literal`).
    pub reason: String,
}

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: `{}(…)` not read: {}",
            self.origin, self.callee, self.reason
        )
    }
}

/// What a scan gives: the keys found, and the constructor calls that
/// could not be read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scan {
    pub found: Vec<Found>,
    pub unread: Vec<Unread>,
}

impl Scan {
    /// Adds another scan's results after this one's.
    pub fn extend(&mut self, other: Scan) {
        self.found.extend(other.found);
        self.unread.extend(other.unread);
    }
}

/// Whether `callee` (as written) is the call `pattern` names: the same
/// path, or a longer one ending in it (`a::b::Message::new` and
/// `crate::Message::new` are `Message::new`; `egui_pop_i18n::t` is `t`).
fn names(callee: &str, pattern: &str) -> bool {
    callee == pattern
        || callee
            .strip_suffix(pattern)
            .is_some_and(|head| head.ends_with("::"))
}

/// A constructor: a pattern whose callee ends in `::new`
/// (`StrKey::new`, an application's `Message::new`). It declares a
/// message, so its key and English must be literals the scanner reads.
fn constructor(pattern: &Pattern) -> bool {
    pattern.callee.ends_with("::new")
}

/// The keys a Rust source uses through `patterns`, with their English,
/// and the constructor calls it could not read.
///
/// A callee is matched by its path's last segments, however long the
/// path and however it is laid out (`helios::Message::new(`,
/// `::app::strings::Message::new(`, a path or arguments split across
/// lines, comments between). A call whose key is not a literal, or not
/// a valid Fluent key, is skipped; when the pattern is a constructor
/// (its callee ends in `::new`, as `StrKey::new`), the call is also
/// reported in [`Scan::unread`] with its file and line, and logged as a
/// warning; so is a constructor whose English is not a literal (its key
/// is still found, without English).
pub fn scan(source: &str, file: &str, patterns: &[Pattern]) -> Scan {
    let toks = tokens(source);
    let mut out = Scan::default();
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
        // Inside a path (`a::b`), the path was read from its first
        // segment; a leading `::` (`::app::Message::new`) starts one.
        if i > 2
            && matches!(toks.get(i - 1), Some(Tok::Punct(':')))
            && matches!(toks.get(i - 2), Some(Tok::Punct(':')))
            && matches!(toks.get(i - 3), Some(Tok::Ident(..)))
        {
            continue;
        }
        let Some(p) = patterns.iter().find(|p| names(&callee, p.callee)) else {
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
        let origin = format!("{file}:{line}");
        let mut unread = |reason: String| {
            if constructor(p) {
                let u = Unread {
                    callee: callee.clone(),
                    origin: origin.clone(),
                    reason,
                };
                log::warn!("i18n extraction: {u}");
                out.unread.push(u);
            }
        };
        let key = match literal(p.key) {
            Some(k) if valid_key(&k) => k,
            Some(k) => {
                unread(format!("`{k}` is not a Fluent key"));
                continue;
            }
            None => {
                unread("the key is not a string literal".into());
                continue;
            }
        };
        let english = match p.english.map(|n| (n, literal(n))) {
            Some((_, Some(e))) => English::Pattern(e),
            Some((n, None)) => {
                unread(format!(
                    "its English (argument {}) is not a string literal: `{key}` is kept without it",
                    n + 1
                ));
                English::None
            }
            None => English::None,
        };
        out.found.push(Found {
            key,
            english,
            origin,
        });
    }
    out
}

/// The keys a Rust source uses through `patterns`, with their English:
/// [`scan`]'s keys, its unread calls only logged.
pub fn scan_source(source: &str, file: &str, patterns: &[Pattern]) -> Vec<Found> {
    scan(source, file, patterns).found
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

/// [`scan`] over every `.rs` file under `dir` (`target` and hidden
/// directories skipped), files in a stable order; origins are relative
/// to `dir`.
pub fn scan_tree(dir: &Path, patterns: &[Pattern]) -> io::Result<Scan> {
    let mut files = Vec::new();
    walk(dir, &mut files)?;
    files.sort();
    let mut out = Scan::default();
    for f in files {
        let source = std::fs::read_to_string(&f)?;
        let name = f.strip_prefix(dir).unwrap_or(&f).display().to_string();
        out.extend(scan(&source, &name, patterns));
    }
    Ok(out)
}

/// [`scan_tree`]'s keys, its unread calls only logged.
pub fn scan_dir(dir: &Path, patterns: &[Pattern]) -> io::Result<Vec<Found>> {
    Ok(scan_tree(dir, patterns)?.found)
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
/// settings (label, description, each choice), plugins (title) and the
/// context stack's frames (title).
pub fn declarations(app: &AppHandle, menus: &[MenuId]) -> Vec<Found> {
    let mut out = Vec::new();
    // Command titles, which may say `{ $application }`.
    let mut titles = Vec::new();
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
            // A title may say `{ $application }` (filled where it is
            // drawn): kept as a placeable, the rest as plain text.
            if !spec.title.trim().is_empty() {
                titles.push(Found {
                    key: strings::declared_key(kind::COMMAND, id, Some("title")),
                    english: plain_with(&spec.title, COMMAND_TITLE_VARIABLES),
                    origin: origin.clone(),
                });
            }
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
    for (_, t) in egui_pop_api::clipboard::types(app) {
        push(
            egui_pop_api::clipboard::type_title_key(&t.format),
            &t.title,
            format!("clipboard type {}", t.format),
        );
    }
    let frames = egui_pop_api::contexts::with_owner(app, |c| c.registry()).unwrap_or_default();
    for f in frames.iter() {
        push(
            strings::declared_key(kind::FRAME, f.id.as_str(), Some("title")),
            &f.title,
            format!("frame {}", f.id),
        );
    }
    out.extend(titles);
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

/// The variables a command's title may say (`{ $application }`, the
/// application's name), filled where the title is drawn.
pub const COMMAND_TITLE_VARIABLES: &[&str] = &["application"];

/// Plain text in which the placeables of `variables` (`{ $name }` or
/// `{$name}`) stay placeables: [`English::Plain`] when it has none, else
/// an [`English::Pattern`] with every other brace escaped.
///
/// ```
/// use egui_pop_i18n::extract::{English, plain_with};
///
/// assert_eq!(
///     plain_with("About { $application } {x}", &["application"]),
///     English::Pattern("About { $application } {\"{\"}x{\"}\"}".into())
/// );
/// assert_eq!(plain_with("Use { $other }", &["application"]), English::Plain("Use { $other }".into()));
/// ```
pub fn plain_with(text: &str, variables: &[&str]) -> English {
    let mut pattern = escape(text);
    let mut found = false;
    for v in variables {
        for (spaced, written) in [(true, format!("{{ ${v} }}")), (false, format!("{{${v}}}"))] {
            let escaped = if spaced {
                format!("{{\"{{\"}} ${v} {{\"}}\"}}")
            } else {
                format!("{{\"{{\"}}${v}{{\"}}\"}}")
            };
            if text.contains(&written) {
                found = true;
                pattern = pattern.replace(&escaped, &format!("{{ ${v} }}"));
            }
        }
    }
    if found {
        English::Pattern(pattern)
    } else {
        English::Plain(text.to_string())
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

/// The catalogue check of an English catalogue: `None` when `current`
/// (the file at `path`) is `fresh` (what [`english`] or [`update`]
/// writes now); else the failure message, saying `path` is out of date
/// and listing `unread` (see [`Scan::unread`]), the calls whose
/// messages are missing from it.
pub fn out_of_date(path: &str, current: &str, fresh: &str, unread: &[Unread]) -> Option<String> {
    if current == fresh {
        return None;
    }
    let mut out = format!("{path} is out of date");
    if !unread.is_empty() {
        out.push_str("; the extractor could not read these calls, their messages are missing:");
        for u in unread {
            let _ = write!(out, "\n  {u}");
        }
    }
    Some(out)
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

    /// An application's constructor, next to the defaults.
    const MESSAGE: &[Pattern] = &[
        Pattern {
            callee: "Message::new",
            key: 0,
            english: Some(1),
        },
        Pattern {
            callee: "t",
            key: 1,
            english: None,
        },
    ];

    #[test]
    fn a_path_qualified_constructor_is_extracted() {
        let src = r#"
            const A: Message = Message::new("plain", "Plain");
            const B: Message = app_core::Message::new("one-segment", "One");
            const C: Message = crate::strings::Message::new("crate-path", "Crate");
            const D: Message = ::app_core::Message::new("leading-colons", "Leading");
            const E: StrKey = egui_pop::core::strings::StrKey::new("str-key", "Key");
            const F: Other = app::NotMessage::new("not-ours", "No");
        "#;
        let mut patterns = MESSAGE.to_vec();
        patterns.extend_from_slice(Pattern::DEFAULTS);
        let s = scan(src, "x.rs", &patterns);
        let keys: Vec<&str> = s.found.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "plain",
                "one-segment",
                "crate-path",
                "leading-colons",
                "str-key"
            ]
        );
        assert_eq!(s.found[2].english, English::Pattern("Crate".into()));
        assert_eq!(s.found[3].origin, "x.rs:5");
        assert!(s.unread.is_empty(), "{:?}", s.unread);
    }

    #[test]
    fn a_multi_line_qualified_constructor_is_extracted() {
        let src = "
            pub const READY: Message = app_core
                ::strings::
                Message // the status bar's
                ::new(
                    \"status-ready\",
                    \"Ready\",
                );
        ";
        let s = scan(src, "src/status.rs", MESSAGE);
        assert_eq!(
            s.found,
            [Found {
                key: "status-ready".into(),
                english: English::Pattern("Ready".into()),
                origin: "src/status.rs:2".into(),
            }]
        );
        assert!(s.unread.is_empty(), "{:?}", s.unread);
    }

    #[test]
    fn an_unreadable_constructor_is_reported_with_its_file_and_line() {
        let src = r#"
            const OK: Message = Message::new("fine", "Fine");
            let dynamic = app::Message::new(id, "Text");
            let bad = Message::new("not a key", "Text");
            let computed = app::Message::new(
                "computed",
                concat!("Comp", "uted"),
            );
            // Not a constructor: a dynamic key stays silent.
            let looked_up = t(ctx, key);
        "#;
        let s = scan(src, "src/ui.rs", MESSAGE);
        let keys: Vec<(&str, &English)> = s
            .found
            .iter()
            .map(|f| (f.key.as_str(), &f.english))
            .collect();
        assert_eq!(
            keys,
            [
                ("fine", &English::Pattern("Fine".into())),
                ("computed", &English::None)
            ]
        );
        let places: Vec<(&str, &str)> = s
            .unread
            .iter()
            .map(|u| (u.origin.as_str(), u.callee.as_str()))
            .collect();
        assert_eq!(
            places,
            [
                ("src/ui.rs:3", "app::Message::new"),
                ("src/ui.rs:4", "Message::new"),
                ("src/ui.rs:5", "app::Message::new"),
            ]
        );
        assert_eq!(
            s.unread[0].to_string(),
            "src/ui.rs:3: `app::Message::new(…)` not read: the key is not a string literal"
        );
        // The catalogue check lists them when the catalogue is stale.
        let m = messages(&s.found).unwrap();
        let fresh = english(&m, "");
        assert_eq!(out_of_date("en/app.ftl", &fresh, &fresh, &s.unread), None);
        let failure = out_of_date("en/app.ftl", "", &fresh, &s.unread).unwrap();
        assert!(
            failure.starts_with("en/app.ftl is out of date"),
            "{failure}"
        );
        for u in &s.unread {
            assert!(failure.contains(&u.to_string()), "{failure}");
        }
    }

    #[test]
    fn a_command_title_keeps_its_application_placeable() {
        let found = vec![Found {
            key: "command-about.title".into(),
            english: plain_with("About { $application }", COMMAND_TITLE_VARIABLES),
            origin: "command about".into(),
        }];
        let ftl = english(&messages(&found).unwrap(), "");
        assert_eq!(
            ftl.trim(),
            "command-about =\n    .title = About { $application }"
        );
        let i = crate::I18n::new("en");
        i.add("en", &ftl).unwrap();
        assert_eq!(
            i.t("command-about.title", &[("application", "Helios".into())]),
            "About Helios"
        );
        // Written without spaces, and beside a brace that stays text.
        assert_eq!(
            plain_with("{$application} {x}", COMMAND_TITLE_VARIABLES),
            English::Pattern("{ $application } {\"{\"}x{\"}\"}".into())
        );
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
    fn the_context_stack_frames_are_declarations() {
        use egui_pop_api::contexts::{self, ContextsPlugin, FrameSpec};
        use egui_pop_api::{Ctx, Plugin, PluginError};
        use egui_pop_model::ids::{FrameId, GroupId, PluginId};
        struct Frames;
        impl Plugin for Frames {
            fn id(&self) -> PluginId {
                PluginId::from_static("demo")
            }
            fn requires(&self) -> Vec<PluginId> {
                vec![contexts::PLUGIN]
            }
            fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
                ctx.frame(FrameSpec::place(
                    FrameId::from_static("demo.album"),
                    "Album",
                ))?
                .frame(FrameSpec::mode(
                    FrameId::from_static("demo.culling"),
                    GroupId::from_static("orientation"),
                    "Culling",
                ))?;
                Ok(())
            }
        }
        let app = egui_pop_api::App::builder()
            .plugin(ContextsPlugin::default())
            .plugin(Frames)
            .build(&egui::Context::default())
            .unwrap();
        let found = declarations(app.handle(), &[]);
        let frames: Vec<(&str, &English, &str)> = found
            .iter()
            .filter(|f| f.key.starts_with("frame-"))
            .map(|f| (f.key.as_str(), &f.english, f.origin.as_str()))
            .collect();
        assert_eq!(
            frames,
            [
                (
                    "frame-demo-album.title",
                    &English::Plain("Album".into()),
                    "frame demo.album"
                ),
                (
                    "frame-demo-culling.title",
                    &English::Plain("Culling".into()),
                    "frame demo.culling"
                ),
            ]
        );
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
