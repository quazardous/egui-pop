//! `bus --check`: no library crate constructs the bus it publishes on
//! (ADR 0030, `docs/adr/0030-the-bus-is-a-contract.md`). The application
//! supplies it (`AppBuilder::bus`); only the default path in
//! `egui-pop-api`'s `bus.rs` and the kernel's own crate make one.
//!
//! A construction is `Kernel::new(`, `Kernel::with_default_deadline(` or
//! `Kernel::default(` in a crate's `src/`, outside comments. Tests,
//! examples, benches and the applications (the gallery, the smoke
//! consumer, the templates) may.

use std::fs;
use std::path::Path;

/// Where a kernel may be constructed, from the workspace root: the
/// kernel's crate and the builder's default.
const ALLOWED: &[&str] = &["crates/egui-pop-hooks/", "crates/egui-pop-api/src/bus.rs"];

/// The applications of the workspace: they choose their bus.
const APPLICATIONS: &[&str] = &["crates/egui-pop-gallery/", "crates/consumer-smoke/"];

const CONSTRUCTORS: &[&str] = &[
    "Kernel::new(",
    "Kernel::with_default_deadline(",
    "Kernel::default(",
];

/// Runs the check: `cargo xtask bus --check`.
pub fn bus(args: &[String]) -> Result<(), String> {
    match args {
        [a] if a == "--check" => {}
        _ => return Err("usage: cargo xtask bus --check".into()),
    }
    let root = crate::root();
    let found = check(&root)?;
    if found.is_empty() {
        println!("no library crate constructs a bus (ADR 0030)");
        return Ok(());
    }
    for f in &found {
        eprintln!("{f}");
    }
    Err(format!(
        "{} construction(s) of a bus in a library crate: take it from the application (ADR 0030)",
        found.len()
    ))
}

/// Every construction outside the allowed places, as `path:line: text`.
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let crates = root.join("crates");
    let mut found = Vec::new();
    let entries = fs::read_dir(&crates).map_err(|e| format!("{}: {e}", crates.display()))?;
    let mut dirs: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    dirs.sort();
    for dir in dirs {
        let src = dir.join("src");
        if src.is_dir() {
            scan(root, &src, &mut found)?;
        }
    }
    Ok(found)
}

fn scan(root: &Path, dir: &Path, found: &mut Vec<String>) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            scan(root, &path, found)?;
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if ALLOWED
            .iter()
            .chain(APPLICATIONS)
            .any(|a| rel.starts_with(a))
        {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("{rel}: {e}"))?;
        found.extend(constructions(&rel, &text));
    }
    Ok(())
}

/// The constructions in one file's text, comments skipped.
pub fn constructions(rel: &str, text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter_map(|(n, line)| {
            let code = line.split("//").next().unwrap_or("");
            CONSTRUCTORS
                .iter()
                .any(|c| {
                    code.match_indices(c).any(|(i, _)| {
                        // `ReadKernel::new(` is not the kernel.
                        !code[..i]
                            .chars()
                            .next_back()
                            .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
                    })
                })
                .then(|| format!("{rel}:{}: {}", n + 1, line.trim()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_library_constructing_a_kernel_is_found() {
        let text = "let bus = Arc::new(egui_pop_hooks::Kernel::new());\n\
                    let k = Kernel::default();\n\
                    // Kernel::new() in a comment\n\
                    let r = ReadKernel::new(x);\n\
                    let d = Kernel::with_default_deadline(t);\n";
        assert_eq!(
            constructions("crates/x/src/lib.rs", text),
            [
                "crates/x/src/lib.rs:1: let bus = Arc::new(egui_pop_hooks::Kernel::new());",
                "crates/x/src/lib.rs:2: let k = Kernel::default();",
                "crates/x/src/lib.rs:5: let d = Kernel::with_default_deadline(t);",
            ]
        );
    }

    #[test]
    fn the_workspace_holds_none() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(check(root).unwrap(), Vec::<String>::new());
    }
}
