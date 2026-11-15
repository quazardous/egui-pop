//! `families --check`: every public module of every published crate is in
//! exactly one row of the component families' table (ADR 0027,
//! `docs/adr/0027-component-families.md`).
//!
//! The table's Modules column names modules in backticks:
//! `crate::module`, `crate::{a, b}`, or a crate without public modules by
//! its name alone. A crate's public modules are the `pub mod` lines at the
//! first column of its `src/lib.rs` (whatever their `cfg`); a crate is
//! published unless its `Cargo.toml` says `publish = false`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// The ADR holding the table, from the workspace root.
pub const ADR: &str = "docs/adr/0027-component-families.md";

/// Runs the check: `cargo xtask families --check`.
pub fn families(args: &[String]) -> Result<(), String> {
    match args {
        [a] if a == "--check" => {}
        _ => return Err("usage: cargo xtask families --check".into()),
    }
    let root = crate::root();
    let problems = check(&root)?;
    if problems.is_empty() {
        println!("every public module is in one family ({ADR})");
        return Ok(());
    }
    for p in &problems {
        eprintln!("{p}");
    }
    Err(format!(
        "{} problem(s) with the families' table in {ADR}",
        problems.len()
    ))
}

/// The problems found, empty when the table covers every module once.
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let path = root.join(ADR);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let rows = parse_table(&text)?;
    let crates = published_crates(root)?;
    Ok(compare(&rows, &crates))
}

/// What one crate exposes: its public modules, possibly none.
pub type Crates = BTreeMap<String, Vec<String>>;

/// One entry of a row's Modules column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A crate without public modules, by its name.
    Crate(String),
    /// A crate's public module.
    Module(String, String),
}

/// The table's rows: the family's name and its entries.
pub fn parse_table(text: &str) -> Result<Vec<(String, Vec<Entry>)>, String> {
    let mut in_section = false;
    let mut rows = Vec::new();
    let mut modules_col = None;
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
        if modules_col.is_none() {
            modules_col = cells.iter().position(|c| *c == "Modules");
            if modules_col.is_none() {
                return Err(format!("{ADR}: the table has no Modules column"));
            }
            continue;
        }
        if cells
            .iter()
            .all(|c| c.chars().all(|ch| ch == '-' || ch == ':'))
        {
            continue;
        }
        let col = modules_col.unwrap_or_default();
        let family = cells.first().copied().unwrap_or_default().to_owned();
        let cell = cells
            .get(col)
            .ok_or_else(|| format!("{ADR}: row `{family}` has no Modules cell"))?;
        rows.push((family, parse_entries(cell)?));
    }
    if rows.is_empty() {
        return Err(format!("{ADR}: no row under `## Families`"));
    }
    Ok(rows)
}

/// The entries of one Modules cell: its backtick spans.
fn parse_entries(cell: &str) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    for (i, span) in cell.split('`').enumerate() {
        if i % 2 == 0 {
            continue;
        }
        match span.split_once("::") {
            None => entries.push(Entry::Crate(span.to_owned())),
            Some((krate, rest)) => {
                let list = match rest.strip_prefix('{') {
                    Some(inner) => inner
                        .strip_suffix('}')
                        .ok_or_else(|| format!("`{span}`: an unclosed brace"))?,
                    None => rest,
                };
                for m in list.split(',').map(str::trim).filter(|m| !m.is_empty()) {
                    entries.push(Entry::Module(krate.to_owned(), m.to_owned()));
                }
            }
        }
    }
    Ok(entries)
}

/// The published crates under `crates/`, each with its public modules.
pub fn published_crates(root: &Path) -> Result<Crates, String> {
    let mut crates = Crates::new();
    let dir = root.join("crates");
    for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let (Ok(manifest), Ok(lib)) = (
            fs::read_to_string(path.join("Cargo.toml")),
            fs::read_to_string(path.join("src/lib.rs")),
        ) else {
            continue;
        };
        if manifest.lines().any(|l| l.trim() == "publish = false") {
            continue;
        }
        let name = manifest
            .lines()
            .find_map(|l| {
                l.strip_prefix("name = \"")
                    .and_then(|r| r.strip_suffix('"'))
            })
            .ok_or_else(|| format!("{}: no package name", path.display()))?;
        crates.insert(name.to_owned(), public_modules(&lib));
    }
    Ok(crates)
}

/// The `pub mod` lines at the first column of a `lib.rs`.
pub fn public_modules(lib: &str) -> Vec<String> {
    lib.lines()
        .filter_map(|l| l.strip_prefix("pub mod "))
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect()
        })
        .collect()
}

/// Every module in no row, in two rows, or named but unknown.
pub fn compare(rows: &[(String, Vec<Entry>)], crates: &Crates) -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (family, entries) in rows {
        for entry in entries {
            let key = match entry {
                Entry::Crate(k) => match crates.get(k) {
                    None => {
                        problems.push(format!("`{family}`: `{k}` is not a published crate"));
                        continue;
                    }
                    Some(mods) if !mods.is_empty() => {
                        problems.push(format!(
                            "`{family}`: `{k}` has public modules, list them instead of the crate"
                        ));
                        continue;
                    }
                    Some(_) => k.clone(),
                },
                Entry::Module(k, m) => {
                    match crates.get(k) {
                        None => {
                            problems.push(format!("`{family}`: `{k}` is not a published crate"));
                            continue;
                        }
                        Some(mods) if !mods.contains(m) => {
                            problems.push(format!("`{family}`: `{k}::{m}` is not a public module"));
                            continue;
                        }
                        Some(_) => {}
                    }
                    format!("{k}::{m}")
                }
            };
            seen.entry(key).or_default().push(family);
        }
    }
    for (krate, mods) in crates {
        let keys: Vec<String> = if mods.is_empty() {
            vec![krate.clone()]
        } else {
            mods.iter().map(|m| format!("{krate}::{m}")).collect()
        };
        for key in keys {
            match seen.get(&key).map(Vec::as_slice) {
                None | Some([]) => problems.push(format!("`{key}` is in no family")),
                Some([_]) => {}
                Some(many) => problems.push(format!(
                    "`{key}` is in several families: {}",
                    many.join(", ")
                )),
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "\
## Families

| Family | Modules |
|---|---|
| A | `a::{x, y}`; `solo` |
| B | `a::y`; `a::nope` |

## Next
| Not | `a::x` |
";

    #[test]
    fn a_missing_a_doubled_and_an_unknown_module_are_reported() {
        let rows = parse_table(TABLE).expect("the table parses");
        let crates = Crates::from([
            ("a".to_owned(), vec!["x".into(), "y".into(), "z".into()]),
            ("solo".to_owned(), vec![]),
        ]);
        let problems = compare(&rows, &crates);
        assert_eq!(
            problems,
            [
                "`B`: `a::nope` is not a public module",
                "`a::y` is in several families: A, B",
                "`a::z` is in no family",
            ]
        );
    }

    #[test]
    fn the_committed_table_covers_every_public_module_once() {
        let problems = check(&crate::root()).expect("the table and the crates read");
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }
}
