//! The READMEs crates.io shows hold no relative link or image: crates.io
//! resolves them from the crate's folder (`crates/<crate>/`), not from
//! the file's place in the repository, so the repository's README, which
//! the facade publishes, would point at `crates/egui-pop/docs/...`.

use std::path::Path;

/// The relative targets of a Markdown text's links and images, outside
/// code fences, with their line.
fn relative_targets(text: &str) -> Vec<(usize, String)> {
    let absolute = |t: &str| t.starts_with("http://") || t.starts_with("https://");
    let local = |t: &str| t.starts_with('#') || t.starts_with("mailto:");
    let mut found = Vec::new();
    let mut fence = false;
    for (n, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
        if fence {
            continue;
        }
        let mut targets = Vec::new();
        let mut rest = line;
        while let Some(i) = rest.find("](") {
            rest = &rest[i + 2..];
            targets.push(rest.split([')', ' ']).next().unwrap_or(""));
        }
        for attr in ["src=\"", "href=\""] {
            let mut rest = line;
            while let Some(i) = rest.find(attr) {
                rest = &rest[i + attr.len()..];
                targets.push(rest.split('"').next().unwrap_or(""));
            }
        }
        for t in targets {
            if !absolute(t) && !local(t) {
                found.push((n + 1, t.to_string()));
            }
        }
    }
    found
}

#[test]
fn published_readmes_link_absolutely() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = vec![root.join("README.md")];
    for entry in std::fs::read_dir(root.join("crates")).unwrap() {
        let readme = entry.unwrap().path().join("README.md");
        if readme.exists() {
            files.push(readme);
        }
    }
    let mut problems = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for (line, target) in relative_targets(&text) {
            problems.push(format!("{}:{line}: {target}", file.display()));
        }
    }
    assert!(
        problems.is_empty(),
        "relative links, broken on crates.io:\n{}",
        problems.join("\n")
    );
}

#[test]
fn relative_targets_are_found_outside_code() {
    let text =
        "![a](docs/a.png) [b](https://x) [c](#c)\n<img src=\"t.png\">\n```\n[d](e.md)\n```\n";
    assert_eq!(
        relative_targets(text),
        [(1, "docs/a.png".to_string()), (2, "t.png".to_string())]
    );
}
