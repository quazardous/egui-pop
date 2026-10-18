//! Compiles the shipped token files (`themes/**/*.yaml`) to JSON in
//! `OUT_DIR`, so the binary reads JSON at start: YAML is the files'
//! format in the repository, for comments, but reading it costs several
//! times JSON's. A value YAML would misread (an unquoted `#colour` or
//! `{alias}`) fails the build, with its file and line.
//!
//! Also writes `kit_files.rs`, the default kit's component files, one per
//! `themes/kit/<component>.tokens.yaml`: adding a component's file is
//! enough to ship it.

use std::fs;
use std::path::{Path, PathBuf};

#[path = "src/yaml_trap.rs"]
mod yaml_trap;

fn main() {
    let themes = Path::new("themes");
    println!("cargo:rerun-if-changed=themes");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let mut kit_files = Vec::new();
    for source in yaml_files(themes) {
        let text = fs::read_to_string(&source).expect("a shipped token file reads");
        if let Some((line, message)) = yaml_trap::trap(&text) {
            panic!("{}:{line}: {message}", source.display());
        }
        let tree: serde_json::Value = serde_saphyr::from_str(&text)
            .unwrap_or_else(|e| panic!("{}: not valid YAML: {e}", source.display()));
        let target = out.join(source.with_extension("json"));
        fs::create_dir_all(target.parent().expect("a file has a folder"))
            .expect("OUT_DIR is writable");
        fs::write(&target, tree.to_string()).expect("OUT_DIR is writable");
        let name = source.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if source.parent() == Some(&themes.join("kit"))
            && let Some(component) = name.strip_suffix(".tokens.yaml")
        {
            kit_files.push((component.to_string(), target));
        }
    }
    kit_files.sort();
    let mut list = String::from("&[\n");
    for (component, target) in kit_files {
        list.push_str(&format!(
            "    ({component:?}, include_str!({:?})),\n",
            target.display().to_string()
        ));
    }
    list.push_str("]\n");
    fs::write(out.join("kit_files.rs"), list).expect("OUT_DIR is writable");
}

/// Every `.yaml` file under `dir`, sorted.
fn yaml_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .expect("the themes folder reads")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            found.extend(yaml_files(&path));
        } else if path.extension().is_some_and(|e| e == "yaml") {
            found.push(path);
        }
    }
    found
}
