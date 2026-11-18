//! The commit the gallery is built from, for its version
//! (`EGUI_POP_GALLERY_COMMIT`, read by `pages::about`): git's short hash,
//! else the `EGUI_POP_GALLERY_COMMIT` variable, else nothing (a build
//! without git still works).
//!
//! And the page list as a macro for the tests (`$OUT_DIR/pages.rs`,
//! `for_each_page!`): a test per page, generated from `PAGES` in
//! `src/lib.rs`, so a page added there gets its tests with no list to
//! keep by hand.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    page_macro();
    println!("cargo:rerun-if-env-changed=EGUI_POP_GALLERY_COMMIT");
    let commit = git(&["rev-parse", "--short", "HEAD"])
        .inspect(|_| watch_git())
        .or_else(|| std::env::var("EGUI_POP_GALLERY_COMMIT").ok())
        .unwrap_or_default();
    println!("cargo:rustc-env=EGUI_POP_GALLERY_COMMIT={}", commit.trim());
}

/// Runs git in the crate's folder; `None` when git is missing or fails.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let text = text.trim();
    (out.status.success() && !text.is_empty()).then(|| text.to_string())
}

/// Builds again when HEAD moves: HEAD itself, the branch it names and the
/// packed refs (paths from git, so a worktree's work too).
fn watch_git() {
    let mut paths = vec!["HEAD".to_string(), "packed-refs".to_string()];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        paths.push(branch);
    }
    for path in paths {
        if let Some(file) = git(&["rev-parse", "--git-path", &path]) {
            let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file);
            if file.exists() {
                println!("cargo:rerun-if-changed={}", file.display());
            }
        }
    }
}

/// Writes `$OUT_DIR/pages.rs`: `for_each_page!(callback! { args })` calls
/// `callback! { { args } ident "page"; ... }` with every page of `PAGES`,
/// in order, each with a Rust name for its test (`-` made `_`). The tests
/// check the list it gives is `PAGES` itself.
fn page_macro() {
    let lib = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
    println!("cargo:rerun-if-changed={}", lib.display());
    let source = std::fs::read_to_string(&lib).expect("src/lib.rs");
    let start = source
        .find("pub const PAGES")
        .expect("src/lib.rs declares `pub const PAGES`");
    let list = &source[start..];
    let list = &list[list.find('=').expect("PAGES' value")..];
    let list = &list[..list.find("];").expect("the end of PAGES")];
    let mut pages = String::new();
    // Every string of the list, comments aside, however rustfmt lays it out.
    let names = list
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .flat_map(|line| line.split('"').skip(1).step_by(2));
    for page in names {
        let mut ident: String = page
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        if ident.starts_with(|c: char| c.is_ascii_digit()) || KEYWORDS.contains(&ident.as_str()) {
            ident.insert_str(0, "page_");
        }
        writeln!(pages, "            {ident} \"{page}\";").unwrap();
    }
    assert!(!pages.is_empty(), "no page found in PAGES");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let text = format!(
        "// Written by build.rs from `PAGES` in src/lib.rs.\n\
         #[allow(unused_macros)]\n\
         macro_rules! for_each_page {{\n    \
         ($callback:ident ! {{ $($args:tt)* }}) => {{\n        \
         $callback! {{\n            {{ $($args)* }}\n{pages}        }}\n    \
         }};\n}}\n"
    );
    std::fs::write(out.join("pages.rs"), text).expect("pages.rs written");
}

/// Rust's keywords a page's name could collide with.
const KEYWORDS: [&str; 38] = [
    "as", "async", "await", "box", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod",
    "move", "mut", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true",
    "type", "unsafe", "use", "where",
];
