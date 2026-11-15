//! `doc-paths --check`: every Rust path the documentation names in
//! backticks resolves to a module or an item of the workspace's crates.
//!
//! A path is a backtick span under `docs/` (fenced code blocks left out)
//! that starts with `egui_pop` (`egui_pop_core::kit::Tone`,
//! `egui_pop::core::kit::Tone`) or `crate::`, or a backtick span starting
//! with `egui_pop` in a crate's rustdoc comments (`//!`, `///` in
//! `crates/*/src`; `crate::` there is the crate's own, left to rustdoc). Its leading path is kept
//! (`f(x)`, `<T>`, `!` and what follows are dropped); a `{a, b}` list
//! after `::` is expanded; a segment ending in `*` matches any name
//! starting with what precedes it.
//!
//! The resolution is textual, from each crate's `src/`: `pub mod` (a file
//! or inline), `pub use` (aliases, lists and globs, followed across
//! crates), `pub` items (`fn`, `struct`, `enum`, `trait`, `type`,
//! `const`, `static`, `union`) and `macro_rules!`. Past an item (a type,
//! a trait), the remaining segments only need to name a function, a
//! constant or an enum variant somewhere in that crate: methods are not
//! tied to their type. A path under an outside crate (`egui`, `eframe`)
//! is taken as it is. `crate::` resolves when any crate has the path.
//!
//! Paths that cannot resolve on purpose (an old path a page tells you to
//! move from, an example of a user's own crate) are listed in
//! [`ALLOW`], one per line with the reason after ` # `.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The exceptions, from the workspace root.
pub const ALLOW: &str = "xtask/doc-paths.allow";

/// Runs the check: `cargo xtask doc-paths --check`.
pub fn doc_paths(args: &[String]) -> Result<(), String> {
    match args {
        [a] if a == "--check" => {}
        _ => return Err("usage: cargo xtask doc-paths --check".into()),
    }
    let root = crate::root();
    let problems = check(&root)?;
    if problems.is_empty() {
        println!("every path named in docs/ and in rustdoc resolves");
        return Ok(());
    }
    for p in &problems {
        eprintln!("{p}");
    }
    Err(format!(
        "{} path(s) in docs/ or rustdoc that resolve to nothing (fix them, or list them in {ALLOW} with a reason)",
        problems.len()
    ))
}

/// The problems found: unresolved paths, and exceptions no longer needed.
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let allow = read_allow(&root.join(ALLOW))?;
    let ws = Workspace::load(&root.join("crates"))?;
    let mut docs = Vec::new();
    markdown_files(&root.join("docs"), &mut docs)?;
    docs.sort();
    let mut problems = Vec::new();
    let mut allowed_used = BTreeSet::new();
    for file in docs {
        let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let rel = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .display()
            .to_string();
        for (line, span) in spans(&text) {
            for path in expand(&span) {
                if allow.contains(&path) {
                    allowed_used.insert(path);
                    continue;
                }
                if !ws.resolves(&path) {
                    problems.push(format!("{rel}:{line}: `{path}` resolves to nothing"));
                }
            }
        }
    }
    let mut sources = Vec::new();
    for entry in fs::read_dir(root.join("crates")).map_err(|e| format!("crates: {e}"))? {
        let src = entry.map_err(|e| e.to_string())?.path().join("src");
        if src.is_dir() {
            rust_files(&src, &mut sources)?;
        }
    }
    sources.sort();
    for file in sources {
        let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let rel = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .display()
            .to_string();
        for (line, span) in spans(&rustdoc(&text)) {
            if !span.starts_with("egui_pop") {
                continue;
            }
            for path in expand(&span) {
                if allow.contains(&path) {
                    allowed_used.insert(path);
                    continue;
                }
                if !ws.resolves(&path) {
                    problems.push(format!("{rel}:{line}: `{path}` resolves to nothing"));
                }
            }
        }
    }
    for path in allow.difference(&allowed_used) {
        problems.push(format!(
            "{ALLOW}: `{path}` is no longer named in docs/ or rustdoc, remove it"
        ));
    }
    Ok(problems)
}

/// The allowed paths: one per line, `path # reason`, `#` lines skipped.
fn read_allow(path: &Path) -> Result<BTreeSet<String>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut set = BTreeSet::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((p, reason)) = line.split_once(" # ") else {
            return Err(format!(
                "{}:{}: `path # reason` expected",
                path.display(),
                i + 1
            ));
        };
        if reason.trim().is_empty() {
            return Err(format!("{}:{}: a reason is needed", path.display(), i + 1));
        }
        set.insert(p.trim().to_owned());
    }
    Ok(set)
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            markdown_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    Ok(())
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// A source file's rustdoc as markdown, line for line: a `//!` or `///`
/// line gives its text, any other line an empty one (so line numbers
/// hold).
pub fn rustdoc(text: &str) -> String {
    let mut out = String::with_capacity(text.len() / 2);
    for line in text.lines() {
        let t = line.trim_start();
        let doc = t
            .strip_prefix("//!")
            .or_else(|| t.strip_prefix("///").filter(|r| !r.starts_with('/')));
        if let Some(d) = doc {
            out.push_str(d.strip_prefix(' ').unwrap_or(d));
        }
        out.push('\n');
    }
    out
}

/// The backtick spans of a page naming a path, with their line, outside
/// fenced code blocks.
pub fn spans(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut fenced = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        for (k, span) in line.split('`').enumerate() {
            if k % 2 == 1 && (span.starts_with("egui_pop") || span.starts_with("crate::")) {
                out.push((i + 1, span.to_owned()));
            }
        }
    }
    out
}

/// The paths a span names: its leading path, a trailing `{a, b}` list
/// expanded.
pub fn expand(span: &str) -> Vec<String> {
    let lead: String = span
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '*'))
        .collect();
    let lead = lead.trim_end_matches(':').to_owned();
    let rest = &span[lead.len()..];
    if let Some(list) = rest
        .strip_prefix("::{")
        .and_then(|r| r.split_once('}'))
        .map(|(l, _)| l)
    {
        return list
            .split(',')
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(|m| {
                let m: String = m
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '*'))
                    .collect();
                format!("{lead}::{m}")
            })
            .collect();
    }
    vec![lead]
}

type ModId = usize;

/// One module: its child modules, items, re-exports and globs.
#[derive(Default, Debug)]
struct Module {
    krate: usize,
    parent: Option<ModId>,
    children: BTreeMap<String, ModId>,
    /// Modules that are not `pub`: followed by `pub use`, not by docs.
    private: BTreeMap<String, ModId>,
    items: BTreeSet<String>,
    /// `pub use a::b as c;`: `c` -> `a::b`.
    reexports: BTreeMap<String, Vec<String>>,
    /// `pub use a::*;`.
    globs: Vec<Vec<String>>,
}

/// Where a name leads.
#[derive(Clone, Copy, Debug)]
enum Loc {
    Module(ModId),
    /// An item of the crate (members are looked up crate-wide).
    Item(usize),
    /// A crate outside the workspace.
    External,
}

/// Every crate's module tree.
struct Workspace {
    modules: Vec<Module>,
    /// Crate name (underscores) -> root module.
    roots: BTreeMap<String, ModId>,
    /// Per crate: functions, constants and variants, for members.
    members: Vec<BTreeSet<String>>,
}

impl Workspace {
    fn load(crates: &Path) -> Result<Self, String> {
        let mut ws = Workspace {
            modules: Vec::new(),
            roots: BTreeMap::new(),
            members: Vec::new(),
        };
        let mut dirs: Vec<PathBuf> = fs::read_dir(crates)
            .map_err(|e| format!("{}: {e}", crates.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        dirs.sort();
        for dir in dirs {
            let (Ok(manifest), true) = (
                fs::read_to_string(dir.join("Cargo.toml")),
                dir.join("src/lib.rs").is_file(),
            ) else {
                continue;
            };
            let Some(name) = manifest.lines().find_map(|l| {
                l.strip_prefix("name = \"")
                    .and_then(|r| r.strip_suffix('"'))
            }) else {
                continue;
            };
            let k = ws.members.len();
            ws.members.push(BTreeSet::new());
            let root = ws.new_module(k, None);
            ws.roots.insert(name.replace('-', "_"), root);
            ws.load_file(&dir.join("src/lib.rs"), &dir.join("src"), root)?;
        }
        Ok(ws)
    }

    fn new_module(&mut self, krate: usize, parent: Option<ModId>) -> ModId {
        self.modules.push(Module {
            krate,
            parent,
            ..Module::default()
        });
        self.modules.len() - 1
    }

    /// Reads one file into `module`; `dir` is where its `mod x;` files live.
    fn load_file(&mut self, file: &Path, dir: &Path, module: ModId) -> Result<(), String> {
        let text = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let krate = self.modules[module].krate;
        // (module, its directory, the depth its body starts at)
        let mut stack: Vec<(ModId, PathBuf, i32)> = vec![(module, dir.to_owned(), 0)];
        let mut depth = 0;
        let mut pending_use: Option<String> = None;
        for raw in text.lines() {
            let line = strip_comment(raw);
            let t = line.trim();
            while stack.len() > 1 && depth < stack.last().map_or(0, |s| s.2) {
                stack.pop();
            }
            let (cur, cur_dir, _) = stack.last().cloned().unwrap_or((module, dir.to_owned(), 0));
            if let Some(acc) = pending_use.as_mut() {
                acc.push(' ');
                acc.push_str(t);
                if t.contains(';') {
                    let stmt = pending_use.take().unwrap_or_default();
                    self.add_use(cur, &stmt);
                }
                depth += braces(&line);
                continue;
            }
            if let Some(rest) = t.strip_prefix("pub use ") {
                if rest.contains(';') {
                    self.add_use(cur, rest);
                } else {
                    pending_use = Some(rest.to_owned());
                }
                depth += braces(&line);
                continue;
            }
            for word in ["fn", "const"] {
                if let Some(name) = after_keyword(t, word) {
                    self.members[krate].insert(name);
                }
            }
            if let Some(v) = variant(t) {
                self.members[krate].insert(v);
            }
            if let Some(name) = t
                .strip_prefix("macro_rules! ")
                .map(ident)
                .filter(|n| !n.is_empty())
            {
                let root = self.root_of(cur);
                self.modules[root].items.insert(name);
            }
            if let Some(field) = t.strip_prefix("pub ").map(ident).filter(|n| {
                !n.is_empty() && t["pub ".len() + n.len()..].trim_start().starts_with(':')
            }) {
                self.members[krate].insert(field);
            }
            let private_mod = ["mod ", "pub(crate) mod ", "pub(super) mod "]
                .iter()
                .find_map(|p| t.strip_prefix(p));
            let public = t.strip_prefix("pub ").map(strip_qualifiers);
            if let Some(rest) = public.or(private_mod) {
                let module_decl = if public.is_some() {
                    rest.strip_prefix("mod ")
                } else {
                    Some(rest)
                };
                if let Some(m) = module_decl {
                    let name = ident(m);
                    let child = self.new_module(krate, Some(cur));
                    if public.is_some() {
                        self.modules[cur].children.insert(name.clone(), child);
                    } else {
                        self.modules[cur].private.insert(name.clone(), child);
                    }
                    let child_dir = cur_dir.join(&name);
                    if m.trim_end().ends_with(';') {
                        let flat = cur_dir.join(format!("{name}.rs"));
                        let nested = child_dir.join("mod.rs");
                        let path = if flat.is_file() { flat } else { nested };
                        if path.is_file() {
                            self.load_file(&path, &child_dir, child)?;
                        }
                    } else if line.contains('{') {
                        stack.push((child, child_dir, depth + 1));
                    }
                } else if public.is_some() {
                    for word in [
                        "fn", "struct", "enum", "trait", "type", "const", "static", "union",
                    ] {
                        if let Some(name) = rest
                            .strip_prefix(word)
                            .and_then(|r| r.strip_prefix(' '))
                            .map(ident)
                            .filter(|n| !n.is_empty())
                        {
                            self.modules[cur].items.insert(name);
                        }
                    }
                }
            }
            depth += braces(&line);
        }
        Ok(())
    }

    fn root_of(&self, mut m: ModId) -> ModId {
        while let Some(p) = self.modules[m].parent {
            m = p;
        }
        m
    }

    /// Records a `pub use` statement (without `pub use `).
    fn add_use(&mut self, module: ModId, stmt: &str) {
        let stmt = stmt.split(';').next().unwrap_or_default();
        let mut out = Vec::new();
        use_tree(&[], stmt, &mut out);
        for (path, alias) in out {
            match alias {
                UseName::Glob => self.modules[module].globs.push(path),
                UseName::Name(n) => {
                    self.modules[module].reexports.insert(n, path);
                }
            }
        }
    }

    /// Whether a documentation path resolves.
    fn resolves(&self, path: &str) -> bool {
        let segs: Vec<&str> = path.split("::").collect();
        let Some((first, rest)) = segs.split_first() else {
            return false;
        };
        if *first == "crate" {
            return self
                .roots
                .values()
                .any(|&r| self.walk(Loc::Module(r), rest, 0));
        }
        match self.roots.get(*first) {
            Some(&r) => self.walk(Loc::Module(r), rest, 0),
            None => false,
        }
    }

    fn walk(&self, loc: Loc, segs: &[&str], guard: usize) -> bool {
        if guard > 32 {
            return false;
        }
        let Some((name, rest)) = segs.split_first() else {
            return true;
        };
        match loc {
            Loc::External => true,
            Loc::Item(k) => segs.iter().all(|s| matches_set(&self.members[k], s)),
            Loc::Module(m) => self
                .lookup(m, name, guard + 1)
                .into_iter()
                .any(|next| self.walk(next, rest, guard + 1)),
        }
    }

    /// Where `name` in module `m` may lead.
    fn lookup(&self, m: ModId, name: &str, guard: usize) -> Vec<Loc> {
        self.lookup_in(m, name, guard, false)
    }

    /// As [`Self::lookup`]; `inner` also follows modules that are not `pub`
    /// (a `use` path written inside the crate).
    fn lookup_in(&self, m: ModId, name: &str, guard: usize, inner: bool) -> Vec<Loc> {
        if guard > 32 {
            return Vec::new();
        }
        let module = &self.modules[m];
        let mut found = Vec::new();
        for (child, &id) in &module.children {
            if glob_eq(name, child) {
                found.push(Loc::Module(id));
            }
        }
        if inner && let Some(&id) = module.private.get(name) {
            found.push(Loc::Module(id));
        }
        if module.items.iter().any(|i| glob_eq(name, i)) {
            found.push(Loc::Item(module.krate));
        }
        for (alias, target) in &module.reexports {
            if glob_eq(name, alias) {
                found.extend(self.use_target(m, target, guard + 1));
            }
        }
        if found.is_empty() {
            for glob in &module.globs {
                for target in self.use_target(m, glob, guard + 1) {
                    if let Loc::Module(t) = target {
                        found.extend(self.lookup_in(t, name, guard + 1, inner));
                    }
                }
            }
        }
        found
    }

    /// Where a `use` path written in module `m` leads.
    fn use_target(&self, m: ModId, path: &[String], guard: usize) -> Vec<Loc> {
        let Some((first, rest)) = path.split_first() else {
            return vec![Loc::Module(m)];
        };
        let start = match first.as_str() {
            "crate" => vec![Loc::Module(self.root_of(m))],
            "self" => vec![Loc::Module(m)],
            "super" => vec![Loc::Module(self.modules[m].parent.unwrap_or(m))],
            other => {
                let local = self.lookup_direct(m, other, guard + 1);
                if !local.is_empty() {
                    local
                } else if let Some(&r) = self.roots.get(other) {
                    vec![Loc::Module(r)]
                } else {
                    vec![Loc::External]
                }
            }
        };
        let mut locs = start;
        for seg in rest {
            let mut next = Vec::new();
            for loc in locs {
                match loc {
                    Loc::Module(id) => next.extend(self.lookup_in(id, seg, guard + 1, true)),
                    other => next.push(other),
                }
            }
            locs = next;
        }
        locs
    }

    /// A child module or item of `m` by its exact name, without globs.
    fn lookup_direct(&self, m: ModId, name: &str, guard: usize) -> Vec<Loc> {
        let module = &self.modules[m];
        let mut found = Vec::new();
        if let Some(&id) = module.children.get(name).or(module.private.get(name)) {
            found.push(Loc::Module(id));
        }
        if module.items.contains(name) {
            found.push(Loc::Item(module.krate));
        }
        if module.reexports.contains_key(name) {
            found.extend(self.lookup(m, name, guard + 1));
        }
        found
    }
}

/// `name` (possibly ending in `*`) matches `actual`.
fn glob_eq(name: &str, actual: &str) -> bool {
    match name.strip_suffix('*') {
        Some(prefix) => actual.starts_with(prefix),
        None => name == actual,
    }
}

fn matches_set(set: &BTreeSet<String>, name: &str) -> bool {
    match name.strip_suffix('*') {
        Some(prefix) => set.iter().any(|s| s.starts_with(prefix)),
        None => set.contains(name),
    }
}

enum UseName {
    Name(String),
    Glob,
}

/// Expands a use tree: `a::{b, c::d as e, f::*}` into paths and names.
fn use_tree(prefix: &[String], tree: &str, out: &mut Vec<(Vec<String>, UseName)>) {
    let tree = tree.trim().trim_start_matches("::");
    if tree.is_empty() {
        return;
    }
    // Split at the first `{` that opens the list of this level.
    if let Some(open) = tree.find('{') {
        let head = tree[..open].trim().trim_end_matches("::");
        let close = tree.rfind('}').unwrap_or(tree.len());
        let inner = &tree[open + 1..close];
        let mut base = prefix.to_vec();
        base.extend(
            head.split("::")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        );
        for part in split_top(inner) {
            let part = part.trim();
            if part == "self" {
                if let Some(last) = base.last() {
                    out.push((base.clone(), UseName::Name(last.clone())));
                }
            } else {
                use_tree(&base, part, out);
            }
        }
        return;
    }
    let (path, alias) = match tree.split_once(" as ") {
        Some((p, a)) => (p.trim(), Some(a.trim())),
        None => (tree, None),
    };
    let mut full = prefix.to_vec();
    full.extend(
        path.split("::")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    );
    if full.last().is_some_and(|l| l == "*") {
        full.pop();
        out.push((full, UseName::Glob));
        return;
    }
    let name = alias
        .map(str::to_owned)
        .or_else(|| full.last().cloned())
        .unwrap_or_default();
    if name != "_" && !name.is_empty() {
        out.push((full, UseName::Name(name)));
    }
}

/// Splits a list at its top-level commas.
fn split_top(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// The line without its `//` comment (outside strings, roughly).
fn strip_comment(line: &str) -> String {
    let mut out = String::new();
    let mut in_str = false;
    let mut prev = '\0';
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' && prev != '\\' {
            in_str = !in_str;
        }
        if !in_str && c == '/' && chars.peek() == Some(&'/') {
            break;
        }
        out.push(c);
        prev = c;
    }
    out
}

/// The change of brace depth on a line, strings and char literals left out.
fn braces(line: &str) -> i32 {
    let mut d = 0;
    let mut in_str = false;
    let mut prev = '\0';
    let bytes: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '"' && prev != '\\' {
            in_str = !in_str;
        } else if !in_str && c == '\'' && i + 2 < bytes.len() && bytes[i + 2] == '\'' {
            i += 3;
            prev = '\'';
            continue;
        } else if !in_str && c == '{' {
            d += 1;
        } else if !in_str && c == '}' {
            d -= 1;
        }
        prev = if prev == '\\' && c == '\\' { '\0' } else { c };
        i += 1;
    }
    d
}

/// Drops `pub`'s scope and qualifiers: `(crate)` makes it not public.
fn strip_qualifiers(rest: &str) -> &str {
    let mut r = rest.trim_start();
    for q in ["async ", "const fn", "unsafe ", "extern \"C\" "] {
        if q == "const fn" {
            if r.starts_with("const fn") {
                r = &r["const ".len()..];
            }
        } else if let Some(s) = r.strip_prefix(q) {
            r = s.trim_start();
        }
    }
    r
}

/// The name after `word ` anywhere a declaration starts on the line.
fn after_keyword(t: &str, word: &str) -> Option<String> {
    let pat = format!("{word} ");
    let mut search = t;
    while let Some(i) = search.find(&pat) {
        let before = &search[..i];
        let ok_start = before.is_empty() || before.ends_with(' ') || before.ends_with(')');
        let name = ident(&search[i + pat.len()..]);
        if ok_start && !name.is_empty() {
            return Some(name);
        }
        search = &search[i + pat.len()..];
    }
    None
}

/// An enum variant at the start of a line: `Name,`, `Name(`, `Name {`, `Name =`.
fn variant(t: &str) -> Option<String> {
    let name = ident(t);
    let first = name.chars().next()?;
    if !first.is_uppercase() {
        return None;
    }
    let after = t[name.len()..].trim_start();
    (after.is_empty() || after.starts_with([',', '(', '{', '='])).then_some(name)
}

fn ident(s: &str) -> String {
    s.trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_skip_fences_and_expand_lists() {
        let text = "a `egui_pop_core::kit::{Tone, Glyph}` b\n```\n`egui_pop::x`\n```\n`f(egui_pop)` `egui_pop::api::wait::until(ctx)`";
        let spans = spans(text);
        assert_eq!(spans.len(), 2);
        assert_eq!(
            expand(&spans[0].1),
            ["egui_pop_core::kit::Tone", "egui_pop_core::kit::Glyph"]
        );
        assert_eq!(expand(&spans[1].1), ["egui_pop::api::wait::until"]);
    }

    #[test]
    fn rustdoc_keeps_doc_lines_in_place() {
        let src = "//! `egui_pop_a::x`\nfn f() {} // `egui_pop_b::y`\n    /// `egui_pop_c::z`\n//// `egui_pop_d::w`\n";
        let spans = spans(&rustdoc(src));
        assert_eq!(
            spans,
            [
                (1, "egui_pop_a::x".to_owned()),
                (3, "egui_pop_c::z".to_owned())
            ]
        );
    }

    #[test]
    fn use_trees_expand() {
        let mut out = Vec::new();
        use_tree(&[], "a::{b, c::d as e, f::*, g::{self, h}}", &mut out);
        let names: Vec<String> = out
            .iter()
            .map(|(p, n)| match n {
                UseName::Name(n) => format!("{}={n}", p.join("::")),
                UseName::Glob => format!("{}=*", p.join("::")),
            })
            .collect();
        assert_eq!(
            names,
            ["a::b=b", "a::c::d=e", "a::f=*", "a::g=g", "a::g::h=h"]
        );
    }

    #[test]
    fn the_committed_docs_name_paths_that_resolve() {
        let problems = check(&crate::root()).expect("the docs and the crates read");
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }
}
