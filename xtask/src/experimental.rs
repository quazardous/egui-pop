//! `experimental --check`: the experimental tier is marked in the code,
//! and stable or supported items do not expose it (`docs/stability.md`,
//! "How experimental is marked").
//!
//! The marker is a rustdoc warning block whose text starts with
//! `Experimental:` ([`MARKER`]): in a crate's `lib.rs` inner docs (the
//! whole crate), a public module's inner docs (the module and every file
//! under it), or an item's outer docs (that item, a method included).
//!
//! The check reads every published crate with `syn` and reports:
//!
//! - a module of a family whose tier is wholly `Experimental` in ADR 0027
//!   without the marker, and a marked module of a family with no
//!   experimental part;
//! - a public item outside the experimental tier (not marked, not under a
//!   marked module or crate) naming an experimental type in its
//!   signature: a function's or method's arguments, return type and
//!   bounds, a struct's public fields, an enum's variants, a trait's
//!   items, a type alias, a constant's type.
//!
//! A type is experimental when it is defined (`struct`, `enum`, `trait`,
//! `type`, `union`) in a marked module or crate, or carries the marker
//! itself. A name in a signature refers to it when its path names the
//! experimental module (or crate), directly (`kit::Kit`) or through the
//! file's `use` lines (`use crate::kit::Kit;` then `Kit`): a textual
//! resolution, not the compiler's, good enough for a boundary check.
//! Exceptions go into [`ALLOW`], one per line with their reason.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

/// The text that marks the experimental tier in rustdoc.
pub const MARKER: &str = "<div class=\"warning\">Experimental:";

/// Exposures accepted on purpose: `item path # reason`.
pub const ALLOW: &str = "xtask/experimental.allow";

/// Crates out of the promise (stability.md, "Not covered").
const NOT_COVERED: &[&str] = &["egui-pop-win32", "egui-pop-wayland", "egui-pop-winit"];

/// Runs the check: `cargo xtask experimental --check`.
pub fn experimental(args: &[String]) -> Result<(), String> {
    let list = match args {
        [a] if a == "--check" => false,
        [a] if a == "--list" => true,
        _ => return Err("usage: cargo xtask experimental --check | --list".into()),
    };
    let root = crate::root();
    let ws = Workspace::read(&root)?;
    if list {
        for unit in ws.units.iter().filter(|u| u.marked) {
            println!("{}", unit.key);
        }
        for item in &ws.marked_items {
            println!("{item}");
        }
        return Ok(());
    }
    let mut problems = ws.tier_problems(&root)?;
    let allow = read_allow(&root.join(ALLOW))?;
    let mut used = BTreeSet::new();
    for (item, names) in ws.exposures() {
        if allow.contains(&item) {
            used.insert(item);
            continue;
        }
        problems.push(format!(
            "`{item}` is outside the experimental tier but names {}",
            names.join(", ")
        ));
    }
    for stale in allow.difference(&used) {
        problems.push(format!(
            "`{stale}` in {ALLOW} exposes nothing now: remove it"
        ));
    }
    if problems.is_empty() {
        println!(
            "{} experimental module(s) and {} item(s) marked; no stable or supported item exposes them",
            ws.units.iter().filter(|u| u.marked).count(),
            ws.marked_items.len()
        );
        return Ok(());
    }
    for p in &problems {
        eprintln!("{p}");
    }
    Err(format!(
        "{} problem(s) with the experimental tier (docs/stability.md)",
        problems.len()
    ))
}

/// The accepted exposures: one per line, `item # reason`, `#` lines skipped.
fn read_allow(path: &Path) -> Result<BTreeSet<String>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut set = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (item, reason) = line
            .split_once(" # ")
            .ok_or_else(|| format!("{ALLOW}: `{line}` has no `# reason`"))?;
        if reason.trim().is_empty() {
            return Err(format!("{ALLOW}: `{item}` has an empty reason"));
        }
        set.insert(item.trim().to_owned());
    }
    Ok(set)
}

/// A crate's root or one of its public modules, with its files.
struct Unit {
    /// `egui-pop-core::kit`, or the crate's name for its root.
    key: String,
    /// The crate's name.
    krate: String,
    /// The module's name, `None` for the crate's root.
    module: Option<String>,
    /// Whether the unit is in the experimental tier.
    marked: bool,
    /// Whether its own file carries the marker (not only its crate).
    own_mark: bool,
    /// Its files, parsed.
    files: Vec<(PathBuf, syn::File)>,
}

struct Workspace {
    units: Vec<Unit>,
    /// Items marked one by one, outside marked units.
    marked_items: Vec<String>,
    /// Experimental type name to the path segments that name its module
    /// or crate (`kit`, `egui_pop_hooks`).
    types: BTreeMap<String, BTreeSet<String>>,
}

impl Workspace {
    fn read(root: &Path) -> Result<Self, String> {
        let mut units = Vec::new();
        let dir = root.join("crates");
        let mut dirs: Vec<PathBuf> = fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        dirs.sort();
        for path in dirs {
            let (Ok(manifest), Ok(lib)) = (
                fs::read_to_string(path.join("Cargo.toml")),
                fs::read_to_string(path.join("src/lib.rs")),
            ) else {
                continue;
            };
            if manifest.lines().any(|l| l.trim() == "publish = false") {
                continue;
            }
            let Some(name) = manifest.lines().find_map(|l| {
                l.strip_prefix("name = \"")
                    .and_then(|r| r.strip_suffix('"'))
            }) else {
                continue;
            };
            if NOT_COVERED.contains(&name) {
                continue;
            }
            units.extend(crate_units(name, &path.join("src"), &lib)?);
        }
        let mut types: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut marked_items = Vec::new();
        let crate_marked: BTreeSet<String> = units
            .iter()
            .filter(|u| u.module.is_none() && u.marked)
            .map(|u| u.krate.clone())
            .collect();
        for unit in &units {
            let ident = unit.krate.replace('-', "_");
            for (_, file) in &unit.files {
                let mut defs = Defs::default();
                defs.visit_file(file);
                for (name, marked) in defs.types {
                    if unit.marked || marked {
                        let set = types.entry(name).or_default();
                        if unit.module.is_none() || crate_marked.contains(&unit.krate) {
                            set.insert(ident.clone());
                        }
                        if let Some(m) = &unit.module {
                            set.insert(m.clone());
                        }
                    }
                }
                if !unit.marked {
                    marked_items.extend(defs.marked.iter().map(|i| format!("{}::{i}", unit.key)));
                }
            }
        }
        // A crate root's `pub use` of an experimental module's item
        // (`egui_pop_core::SystemFocus`) names it through the crate.
        for unit in units.iter().filter(|u| u.module.is_none()) {
            let ident = unit.krate.replace('-', "_");
            let lib = unit.files.iter().find(|(p, _)| p.ends_with("src/lib.rs"));
            let Some((_, lib)) = lib else { continue };
            let experimental_mods: BTreeSet<&str> = units
                .iter()
                .filter(|u| u.krate == unit.krate && u.marked)
                .filter_map(|u| u.module.as_deref())
                .collect();
            for item in &lib.items {
                let syn::Item::Use(u) = item else { continue };
                if !is_pub(&u.vis) {
                    continue;
                }
                let mut paths = BTreeMap::new();
                walk_use(&u.tree, &mut Vec::new(), &mut paths);
                for (alias, full) in paths {
                    let head = full.iter().find(|s| *s != "crate" && *s != "self");
                    if head.is_some_and(|h| experimental_mods.contains(h.as_str()))
                        && full.last().is_some_and(|l| types.contains_key(l))
                    {
                        types.entry(alias).or_default().insert(ident.clone());
                    }
                }
            }
        }
        // The facade's aliases (`egui_pop::views`) name whole crates.
        for set in types.values_mut() {
            let aliases: Vec<String> = set
                .iter()
                .filter_map(|s| s.strip_prefix("egui_pop_"))
                .map(str::to_owned)
                .collect();
            set.extend(aliases);
        }
        marked_items.sort();
        Ok(Self {
            units,
            marked_items,
            types,
        })
    }

    /// Items outside the tier and the experimental names they expose.
    fn exposures(&self) -> BTreeMap<String, Vec<String>> {
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for unit in self.units.iter().filter(|u| !u.marked) {
            // The methods of a type marked one by one share its tier.
            let mut marked_types = BTreeSet::new();
            for (_, file) in &unit.files {
                let mut defs = Defs::default();
                defs.visit_file(file);
                marked_types.extend(defs.types.into_iter().filter(|t| t.1).map(|t| t.0));
            }
            for (_, file) in &unit.files {
                let uses = uses_of(file);
                let mut scan = Scan {
                    types: &self.types,
                    marked_types: &marked_types,
                    uses: &uses,
                    prefix: unit.key.replace('-', "_"),
                    owner: Vec::new(),
                    out: &mut out,
                };
                scan.items(&file.items);
            }
        }
        out
    }

    /// Modules whose mark disagrees with their family's tier in ADR 0027.
    fn tier_problems(&self, root: &Path) -> Result<Vec<String>, String> {
        let text = fs::read_to_string(root.join(crate::families::ADR))
            .map_err(|e| format!("{}: {e}", crate::families::ADR))?;
        let rows = crate::families::parse_table(&text)?;
        let tiers = tiers(&text);
        let marked: BTreeMap<&str, (bool, bool)> = self
            .units
            .iter()
            .map(|u| (u.key.as_str(), (u.marked, u.own_mark)))
            .collect();
        let mut problems = Vec::new();
        for (family, entries) in rows {
            let Some(tier) = tiers.get(&family) else {
                continue;
            };
            let wholly = tier.trim() == "Experimental";
            let partly = tier.contains("Experimental");
            for entry in entries {
                let key = match entry {
                    crate::families::Entry::Crate(k) => k,
                    crate::families::Entry::Module(k, m) => format!("{k}::{m}"),
                };
                let Some(&(is_marked, own_mark)) = marked.get(key.as_str()) else {
                    continue;
                };
                if wholly && !is_marked {
                    problems.push(format!(
                        "`{key}` is in `{family}`, Experimental in ADR 0027, but carries no marker"
                    ));
                }
                // A supported module of an experimental crate (its strings,
                // its mounts) inherits the crate's mark: only its own counts.
                if !partly && own_mark {
                    problems.push(format!(
                        "`{key}` is marked experimental but `{family}` is `{tier}` in ADR 0027"
                    ));
                }
            }
        }
        Ok(problems)
    }
}

/// Each family's Tier cell.
fn tiers(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut in_section = false;
    let mut col = None;
    for line in text.lines() {
        if line.starts_with("## ") {
            in_section = line.trim() == "## Families";
            continue;
        }
        if !in_section || !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        let Some(c) = col else {
            col = cells.iter().position(|c| *c == "Tier");
            continue;
        };
        if let (Some(f), Some(t)) = (cells.first(), cells.get(c)) {
            out.insert((*f).to_owned(), (*t).to_owned());
        }
    }
    out
}

/// A crate's root unit and one unit per public module.
fn crate_units(name: &str, src: &Path, lib: &str) -> Result<Vec<Unit>, String> {
    let crate_file = parse(&src.join("lib.rs"))?;
    let crate_marked = has_marker(&crate_file.attrs);
    let modules = crate::families::public_modules(lib);
    let mut claimed = BTreeSet::new();
    let mut units = Vec::new();
    for m in &modules {
        let main = [src.join(format!("{m}.rs")), src.join(m).join("mod.rs")]
            .into_iter()
            .find(|p| p.is_file());
        let mut paths: Vec<PathBuf> = main.iter().cloned().collect();
        rs_files(&src.join(m), &mut paths);
        paths.sort();
        paths.dedup();
        let mut files = Vec::new();
        let mut own_mark = false;
        for p in paths {
            let file = parse(&p)?;
            if Some(&p) == main.as_ref() {
                own_mark |= has_marker(&file.attrs);
            }
            claimed.insert(p.clone());
            files.push((p, file));
        }
        if main.is_none() {
            // An inline module in lib.rs: its own attributes carry the mark.
            if let Some(syn::Item::Mod(item)) = crate_file
                .items
                .iter()
                .find(|i| matches!(i, syn::Item::Mod(im) if im.ident == m.as_str()))
            {
                own_mark |= has_marker(&item.attrs);
            }
        }
        units.push(Unit {
            key: format!("{name}::{m}"),
            krate: name.to_owned(),
            module: Some(m.clone()),
            marked: crate_marked || own_mark,
            own_mark,
            files,
        });
    }
    let mut rest = Vec::new();
    rs_files(src, &mut rest);
    rest.sort();
    let mut files = Vec::new();
    for p in rest {
        if claimed.contains(&p) || p.components().any(|c| c.as_os_str() == "bin") {
            continue;
        }
        files.push((p.clone(), parse(&p)?));
    }
    units.push(Unit {
        key: name.to_owned(),
        krate: name.to_owned(),
        module: None,
        marked: crate_marked,
        own_mark: crate_marked,
        files,
    });
    Ok(units)
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn parse(path: &Path) -> Result<syn::File, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    syn::parse_file(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Whether doc attributes carry the marker.
pub fn has_marker(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        if !a.path().is_ident("doc") {
            return false;
        }
        match &a.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => s.value().trim_start().starts_with(MARKER),
                _ => false,
            },
            _ => false,
        }
    })
}

fn is_test_or_hidden(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        let text = quote_attr(a);
        (a.path().is_ident("cfg") && text.contains("test"))
            || (a.path().is_ident("doc") && text.contains("hidden"))
    })
}

fn quote_attr(a: &syn::Attribute) -> String {
    match &a.meta {
        syn::Meta::List(l) => l.tokens.to_string(),
        _ => String::new(),
    }
}

fn is_pub(v: &syn::Visibility) -> bool {
    matches!(v, syn::Visibility::Public(_))
}

/// The types a file defines, and the items marked one by one.
#[derive(Default)]
struct Defs {
    types: Vec<(String, bool)>,
    marked: Vec<String>,
}

impl<'a> Visit<'a> for Defs {
    fn visit_item(&mut self, item: &'a syn::Item) {
        let def = match item {
            syn::Item::Struct(i) => Some((&i.ident, &i.attrs)),
            syn::Item::Enum(i) => Some((&i.ident, &i.attrs)),
            syn::Item::Trait(i) => Some((&i.ident, &i.attrs)),
            syn::Item::Type(i) => Some((&i.ident, &i.attrs)),
            syn::Item::Union(i) => Some((&i.ident, &i.attrs)),
            _ => None,
        };
        if let Some((ident, attrs)) = def {
            let marked = has_marker(attrs);
            self.types.push((ident.to_string(), marked));
            if marked {
                self.marked.push(ident.to_string());
            }
        }
        match item {
            syn::Item::Fn(f) if has_marker(&f.attrs) => self.marked.push(f.sig.ident.to_string()),
            syn::Item::Const(c) if has_marker(&c.attrs) => self.marked.push(c.ident.to_string()),
            syn::Item::Impl(i) => {
                let owner = self_name(&i.self_ty);
                for it in &i.items {
                    match it {
                        syn::ImplItem::Fn(f) if has_marker(&f.attrs) => {
                            self.marked.push(format!("{owner}::{}", f.sig.ident));
                        }
                        syn::ImplItem::Const(c) if has_marker(&c.attrs) => {
                            self.marked.push(format!("{owner}::{}", c.ident));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        syn::visit::visit_item(self, item);
    }
}

fn self_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        _ => "_".into(),
    }
}

/// One `use` tree: each name it brings into scope, with its full path.
fn walk_use(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    out: &mut BTreeMap<String, Vec<String>>,
) {
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            walk_use(&p.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            let mut full = prefix.clone();
            full.push(n.ident.to_string());
            out.insert(n.ident.to_string(), full);
        }
        syn::UseTree::Rename(r) => {
            let mut full = prefix.clone();
            full.push(r.ident.to_string());
            out.insert(r.rename.to_string(), full);
        }
        syn::UseTree::Group(g) => {
            for t in &g.items {
                walk_use(t, prefix, out);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}

/// A file's `use` lines: the name in scope to the path it stands for.
fn uses_of(file: &syn::File) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![&file.items];
    while let Some(items) = stack.pop() {
        for item in items {
            match item {
                syn::Item::Use(u) => walk_use(&u.tree, &mut Vec::new(), &mut out),
                syn::Item::Mod(m) => {
                    if let Some((_, inner)) = &m.content {
                        stack.push(inner);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Walks the public items of a unit outside the tier.
struct Scan<'a> {
    types: &'a BTreeMap<String, BTreeSet<String>>,
    marked_types: &'a BTreeSet<String>,
    uses: &'a BTreeMap<String, Vec<String>>,
    prefix: String,
    owner: Vec<String>,
    out: &'a mut BTreeMap<String, Vec<String>>,
}

impl Scan<'_> {
    fn items(&mut self, items: &[syn::Item]) {
        for item in items {
            self.item(item);
        }
    }

    fn report(&mut self, name: &str, found: BTreeSet<String>) {
        if found.is_empty() {
            return;
        }
        let mut path = vec![self.prefix.clone()];
        path.extend(self.owner.iter().cloned());
        path.push(name.to_owned());
        self.out
            .entry(path.join("::"))
            .or_default()
            .extend(found.into_iter().map(|f| format!("`{f}`")));
    }

    fn names<F: FnOnce(&mut Names)>(&self, f: F) -> BTreeSet<String> {
        let mut n = Names {
            types: self.types,
            uses: self.uses,
            found: BTreeSet::new(),
        };
        f(&mut n);
        n.found
    }

    fn item(&mut self, item: &syn::Item) {
        match item {
            syn::Item::Fn(i) if is_pub(&i.vis) && !skip(&i.attrs) => {
                let found = self.names(|n| n.visit_signature(&i.sig));
                self.report(&i.sig.ident.to_string(), found);
            }
            syn::Item::Struct(i) if is_pub(&i.vis) && !skip(&i.attrs) => {
                let found = self.names(|n| {
                    n.visit_generics(&i.generics);
                    for f in &i.fields {
                        if is_pub(&f.vis) && !skip(&f.attrs) {
                            n.visit_type(&f.ty);
                        }
                    }
                });
                self.report(&i.ident.to_string(), found);
            }
            syn::Item::Enum(i) if is_pub(&i.vis) && !skip(&i.attrs) => {
                let found = self.names(|n| {
                    n.visit_generics(&i.generics);
                    for v in &i.variants {
                        if !skip(&v.attrs) {
                            n.visit_fields(&v.fields);
                        }
                    }
                });
                self.report(&i.ident.to_string(), found);
            }
            syn::Item::Trait(i) if is_pub(&i.vis) && !skip(&i.attrs) => {
                let found = self.names(|n| {
                    n.visit_generics(&i.generics);
                    for s in &i.supertraits {
                        n.visit_type_param_bound(s);
                    }
                    for it in &i.items {
                        match it {
                            syn::TraitItem::Fn(f) if !skip(&f.attrs) => n.visit_signature(&f.sig),
                            syn::TraitItem::Const(c) if !skip(&c.attrs) => n.visit_type(&c.ty),
                            syn::TraitItem::Type(t) if !skip(&t.attrs) => {
                                for b in &t.bounds {
                                    n.visit_type_param_bound(b);
                                }
                            }
                            _ => {}
                        }
                    }
                });
                self.report(&i.ident.to_string(), found);
            }
            syn::Item::Type(i) if is_pub(&i.vis) && !skip(&i.attrs) => {
                let found = self.names(|n| n.visit_type(&i.ty));
                self.report(&i.ident.to_string(), found);
            }
            syn::Item::Const(i) if is_pub(&i.vis) && !skip(&i.attrs) => {
                let found = self.names(|n| n.visit_type(&i.ty));
                self.report(&i.ident.to_string(), found);
            }
            syn::Item::Static(i) if is_pub(&i.vis) && !skip(&i.attrs) => {
                let found = self.names(|n| n.visit_type(&i.ty));
                self.report(&i.ident.to_string(), found);
            }
            syn::Item::Impl(i)
                if i.trait_.is_none()
                    && !skip(&i.attrs)
                    && !self.marked_types.contains(&self_name(&i.self_ty)) =>
            {
                self.owner.push(self_name(&i.self_ty));
                for it in &i.items {
                    match it {
                        syn::ImplItem::Fn(f) if is_pub(&f.vis) && !skip(&f.attrs) => {
                            let found = self.names(|n| n.visit_signature(&f.sig));
                            self.report(&f.sig.ident.to_string(), found);
                        }
                        syn::ImplItem::Const(c) if is_pub(&c.vis) && !skip(&c.attrs) => {
                            let found = self.names(|n| n.visit_type(&c.ty));
                            self.report(&c.ident.to_string(), found);
                        }
                        _ => {}
                    }
                }
                self.owner.pop();
            }
            syn::Item::Mod(m) if !skip(&m.attrs) => {
                if let Some((_, inner)) = &m.content {
                    self.owner.push(m.ident.to_string());
                    self.items(inner);
                    self.owner.pop();
                }
            }
            _ => {}
        }
    }
}

/// Tests, hidden items and items marked one by one are not checked.
fn skip(attrs: &[syn::Attribute]) -> bool {
    is_test_or_hidden(attrs) || has_marker(attrs)
}

/// Collects the experimental names a piece of signature refers to.
struct Names<'a> {
    types: &'a BTreeMap<String, BTreeSet<String>>,
    uses: &'a BTreeMap<String, Vec<String>>,
    found: BTreeSet<String>,
}

impl<'a> Visit<'a> for Names<'_> {
    fn visit_path(&mut self, path: &'a syn::Path) {
        let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        // The path as written, its head expanded through `use`.
        let mut full = segs.clone();
        if let Some(head) = segs.first().and_then(|h| self.uses.get(h)) {
            full = head.clone();
            full.extend(segs.iter().skip(1).cloned());
        }
        if let Some((last, before)) = full.split_last()
            && let Some(owners) = self.types.get(last)
            && let Some(o) = before.iter().find(|s| owners.contains(*s))
        {
            self.found.insert(format!("{o}::{last}"));
        }
        syn::visit::visit_path(self, path);
    }

    // A function's body is not its signature.
    fn visit_block(&mut self, _: &'a syn::Block) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_code_keeps_the_experimental_boundary() {
        experimental(&["--check".to_owned()]).expect("the experimental check passes");
    }

    #[test]
    fn a_name_is_resolved_through_the_files_use_lines() {
        let file: syn::File = syn::parse_str(
            "use crate::kit::{Kit, Other as Alias};\n\
             pub fn a(k: &Kit) {}\n\
             pub fn b() -> Option<kit::Kit> { None }\n\
             pub fn c(t: Tone) {}\n\
             pub struct S { pub x: Alias, y: Kit }\n\
             /// <div class=\"warning\">Experimental: x</div>\n\
             pub fn d(k: Kit) {}\n",
        )
        .expect("parses");
        let types = BTreeMap::from([
            ("Kit".to_owned(), BTreeSet::from(["kit".to_owned()])),
            ("Other".to_owned(), BTreeSet::from(["kit".to_owned()])),
            ("Tone".to_owned(), BTreeSet::from(["kit".to_owned()])),
        ]);
        let uses = uses_of(&file);
        let mut out = BTreeMap::new();
        Scan {
            types: &types,
            marked_types: &BTreeSet::new(),
            uses: &uses,
            prefix: "c::m".into(),
            owner: Vec::new(),
            out: &mut out,
        }
        .items(&file.items);
        let keys: Vec<&str> = out.keys().map(String::as_str).collect();
        assert_eq!(keys, ["c::m::S", "c::m::a", "c::m::b"]);
    }
}
