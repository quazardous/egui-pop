//! `cargo xtask affected`: what a change needs, from the files it
//! touches and the workspace's dependency graph (`docs/testing.md`,
//! "Selection").
//!
//! The changed files come from git (against the merge base with a base
//! ref, the working tree included) or from `--files`. Each file selects
//! tests (whole crates with the crates that depend on them, or single
//! test binaries) and CI jobs. A file no rule names selects everything:
//! a missing rule costs time, never safety. Critical tests
//! (`#[egui_pop_test::critical]`) are added to every selection.
//!
//! Nothing is built to compute the selection: `cargo metadata
//! --no-deps`, `git` and a scan of the sources for the critical mark.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const USAGE: &str = "\
cargo xtask affected [--base REF] [--files PATH...] [--full]
                     [--plan | --run] [--github] [-- NEXTEST_ARGS...]

  --base REF     compare with the merge base of REF and HEAD, the working
                 tree included (default: origin/main)
  --files P...   take these paths (relative to the workspace) as the
                 change instead of asking git
  --full         select everything (as on main, or with the ci:full label)
  --plan         print the cargo commands --run would run
  --run          run the selected tests: cargo nextest for the tests,
                 cargo test --doc for the doctests
  --github       write the jobs to $GITHUB_OUTPUT and the report to
                 $GITHUB_STEP_SUMMARY
  -- ARGS        passed to every cargo nextest run (e.g. --profile ci)";

/// The nextest filter of every critical test, and of those that render.
pub const CRITICAL: &str = "test(/(^|::)critical_/)";
pub const CRITICAL_GPU: &str = "test(/(^|::)critical_gpu_/)";

/// Paths that select every test and every job.
const EVERYTHING: &[&str] = &[
    ".github/",
    ".config/",
    ".cargo/",
    "xtask/",
    // The test kit and its attributes: every crate's tests use them.
    "crates/egui-pop-test/",
    "crates/egui-pop-test-macros/",
    // Token files are read everywhere: snapshots, contrast, kits.
    "crates/egui-pop-tokens/themes/",
];

/// Test binaries that read the documentation, besides `consumer-smoke`
/// (its doctests are the guides' code).
const DOC_READERS: &[(&str, &str)] = &[
    ("egui-pop", "readme_links"),
    ("egui-pop-gallery", "gallery_doc"),
    ("egui-pop-gallery", "hosts_doc"),
];

/// Data outside the crates, and the test binaries that read it.
const DATA_READERS: &[(&str, &[(&str, &str)])] = &[
    ("template/", &[("egui-pop", "minimal")]),
    (
        "examples/",
        &[("egui-pop-tokens", "kits"), ("egui-pop-gallery", "gallery")],
    ),
];

/// A workspace package, as `cargo metadata --no-deps` gives it.
#[derive(Clone, Debug, Default)]
pub struct Package {
    pub name: String,
    /// Its folder, relative to the workspace, with a trailing `/`.
    pub dir: String,
    /// Workspace packages it depends on, dev-dependencies included.
    pub deps: BTreeSet<String>,
    /// The same, without dev-dependencies: what its build needs.
    pub build_deps: BTreeSet<String>,
    pub has_lib: bool,
    /// Its integration test binaries: name and source, relative.
    pub tests: Vec<(String, String)>,
}

/// One changed file: git's status letter and its path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub status: char,
    pub path: String,
}

/// A test with the critical mark, found in the sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Critical {
    pub package: String,
    /// The test binary, `None` for the library's unit tests.
    pub binary: Option<String>,
    pub name: String,
    pub gpu: bool,
}

/// What a change needs.
#[derive(Debug, Default)]
pub struct Selection {
    pub full: bool,
    /// Why, for each reason, in order: path and rule.
    pub reasons: Vec<String>,
    /// Packages whose every test runs, doctests included.
    pub packages: BTreeSet<String>,
    /// Single test binaries, outside `packages`.
    pub binaries: BTreeSet<(String, String)>,
    /// Critical tests that run besides, outside what is above.
    pub critical: Vec<Critical>,
    /// The change touches nothing but documentation.
    pub docs_only: bool,
    pub jobs: Jobs,
}

/// The jobs of `.github/workflows/ci.yml` a change needs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Jobs {
    pub fmt: bool,
    pub clippy: bool,
    /// `cargo doc` of the guides (consumer-smoke), when clippy (which
    /// documents the whole workspace) does not run.
    pub doc: bool,
    pub test: bool,
    pub deny: bool,
    pub wasm: bool,
    pub package: bool,
    pub semver: bool,
}

impl Jobs {
    const ALL: Jobs = Jobs {
        fmt: true,
        clippy: true,
        doc: false,
        test: true,
        deny: true,
        wasm: true,
        package: true,
        semver: true,
    };

    pub fn list(&self) -> [(&'static str, bool); 8] {
        [
            ("fmt", self.fmt),
            ("clippy", self.clippy),
            ("doc", self.doc),
            ("test", self.test),
            ("deny", self.deny),
            ("wasm", self.wasm),
            ("package", self.package),
            ("semver", self.semver),
        ]
    }
}

pub fn affected(args: &[String]) -> Result<(), String> {
    let mut base = "origin/main".to_string();
    let mut files: Option<Vec<String>> = None;
    let mut full = false;
    let mut run = false;
    let mut show_plan = false;
    let mut github = false;
    let mut nextest_args = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--base" => base = it.next().ok_or("--base needs a ref")?.clone(),
            "--files" => files = Some(Vec::new()),
            "--full" => full = true,
            "--run" => run = true,
            "--plan" => show_plan = true,
            "--github" => github = true,
            "--" => {
                nextest_args.extend(it.by_ref().cloned());
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            p if files.is_some() && !p.starts_with("--") => {
                files.as_mut().expect("files").push(p.to_string());
            }
            other => return Err(format!("unknown argument `{other}`\n\n{USAGE}")),
        }
    }
    let root = super::root();
    let packages = packages(&root)?;
    let changes = match files {
        Some(f) => f
            .into_iter()
            .map(|path| Change { status: 'M', path })
            .collect(),
        None => changes(&root, &base)?,
    };
    let critical = scan_critical(&root, &packages);
    let sel = select_in(Some(&root), &packages, &changes, &critical, full);
    let report = report(&sel, &packages, &changes);
    println!("{report}");
    if github {
        write_github(&sel, &report)?;
    }
    if show_plan {
        for Step(args) in plan(&sel, &packages, &nextest_args) {
            println!("cargo {}", shell_words(&args));
        }
    }
    if run {
        run_tests(&sel, &packages, &nextest_args)?;
    }
    Ok(())
}

/// The workspace's packages, from `cargo metadata --no-deps` (no build,
/// no network).
fn packages(root: &Path) -> Result<Vec<Package>, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    // The workspace root as cargo sees it (symbolic links resolved).
    let ws_root = meta["workspace_root"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.to_path_buf());
    let rel = |p: &str| -> String {
        let p = Path::new(p);
        p.strip_prefix(&ws_root)
            .or_else(|_| p.strip_prefix(root))
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let all = meta["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?;
    let names: BTreeSet<&str> = all.iter().filter_map(|p| p["name"].as_str()).collect();
    let mut packages = Vec::new();
    for p in all {
        let manifest = rel(p["manifest_path"].as_str().unwrap_or_default());
        let dir = match manifest.rsplit_once('/') {
            Some((d, _)) => format!("{d}/"),
            None => String::new(),
        };
        let mut pkg = Package {
            name: p["name"].as_str().unwrap_or_default().to_string(),
            dir,
            ..Package::default()
        };
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let Some(n) = d["name"].as_str().filter(|n| names.contains(n)) else {
                continue;
            };
            pkg.deps.insert(n.to_string());
            if d["kind"].as_str() != Some("dev") {
                pkg.build_deps.insert(n.to_string());
            }
        }
        for t in p["targets"].as_array().into_iter().flatten() {
            let kinds: Vec<&str> = t["kind"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|k| k.as_str())
                .collect();
            if kinds.iter().any(|k| *k == "lib" || *k == "proc-macro") {
                pkg.has_lib = true;
            }
            if kinds.contains(&"test") {
                pkg.tests.push((
                    t["name"].as_str().unwrap_or_default().to_string(),
                    rel(t["src_path"].as_str().unwrap_or_default()),
                ));
            }
        }
        packages.push(pkg);
    }
    Ok(packages)
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The files changed since the merge base of `base` and `HEAD`, the
/// working tree and untracked files included.
fn changes(root: &Path, base: &str) -> Result<Vec<Change>, String> {
    let merge_base = git(root, &["merge-base", base, "HEAD"])?;
    let merge_base = merge_base.trim();
    let mut found = Vec::new();
    let diff = git(
        root,
        &[
            "diff",
            "--name-status",
            "--no-renames",
            "--no-color",
            merge_base,
        ],
    )?;
    for line in diff.lines() {
        let mut parts = line.split('\t');
        let status = parts.next().and_then(|s| s.chars().next()).unwrap_or('M');
        for path in parts {
            found.push(Change {
                status,
                path: path.to_string(),
            });
        }
    }
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard"])?;
    for path in untracked.lines() {
        found.push(Change {
            status: 'A',
            path: path.to_string(),
        });
    }
    Ok(found)
}

/// Every test carrying the critical mark, from a scan of the packages'
/// `src/` and `tests/` folders.
fn scan_critical(root: &Path, packages: &[Package]) -> Vec<Critical> {
    let mut found = Vec::new();
    for p in packages {
        for sub in ["src", "tests"] {
            let mut files = Vec::new();
            rs_files(&root.join(&p.dir).join(sub), &mut files);
            files.sort();
            for file in files {
                let Ok(text) = fs::read_to_string(&file) else {
                    continue;
                };
                let rel = file
                    .strip_prefix(root)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");
                let binary = if sub == "src" {
                    None
                } else {
                    Some(binary_of(p, &rel).unwrap_or_else(|| {
                        // A module of a test binary: the binary is not
                        // known, the package's tests are scanned whole.
                        String::new()
                    }))
                };
                for (name, gpu) in critical_in(&text) {
                    found.push(Critical {
                        package: p.name.clone(),
                        binary: binary.clone(),
                        name,
                        gpu,
                    });
                }
            }
        }
    }
    found
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// The marked test functions of a source: name (with its prefix) and
/// whether it renders.
pub fn critical_in(text: &str) -> Vec<(String, bool)> {
    let mut found = Vec::new();
    let mut pending: Option<bool> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("//") {
            continue;
        }
        if let Some(rest) = l
            .strip_prefix("#[egui_pop_test::critical")
            .or_else(|| l.strip_prefix("#[egui_pop_test_macros::critical"))
            .or_else(|| l.strip_prefix("#[critical"))
            && (rest.starts_with(']') || rest.starts_with('('))
        {
            pending = Some(rest.starts_with("(gpu)"));
            continue;
        }
        if let Some(gpu) = pending
            && let Some(i) = l.find("fn ")
        {
            let name: String = l[i + 3..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let prefix = if gpu { "critical_gpu_" } else { "critical_" };
            found.push((format!("{prefix}{name}"), gpu));
            pending = None;
        }
    }
    found
}

/// The test binary whose root is this file.
fn binary_of(p: &Package, path: &str) -> Option<String> {
    p.tests
        .iter()
        .find(|(_, src)| src == path)
        .map(|(name, _)| name.clone())
}

/// The package a path belongs to: the one with the longest folder
/// containing it.
fn package_of<'p>(packages: &'p [Package], path: &str) -> Option<&'p Package> {
    packages
        .iter()
        .filter(|p| !p.dir.is_empty() && path.starts_with(&p.dir))
        .max_by_key(|p| p.dir.len())
}

/// The packages depending on any of `start`, through `edges`, `start`
/// included.
fn closure(
    packages: &[Package],
    start: &BTreeSet<String>,
    edges: impl Fn(&Package) -> &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut set = start.clone();
    loop {
        let before = set.len();
        for p in packages {
            if edges(p).iter().any(|d| set.contains(d)) {
                set.insert(p.name.clone());
            }
        }
        if set.len() == before {
            return set;
        }
    }
}

/// The workspace packages `name` needs to build, itself included.
fn build_closure(packages: &[Package], name: &str) -> BTreeSet<String> {
    let mut set = BTreeSet::from([name.to_string()]);
    loop {
        let before = set.len();
        for p in packages {
            if set.contains(&p.name) {
                set.extend(p.build_deps.iter().cloned());
            }
        }
        if set.len() == before {
            return set;
        }
    }
}

fn is_doc(path: &str) -> bool {
    path.starts_with("docs/") || (!path.contains('/') && path.ends_with(".md"))
}

/// Whether another file of the package's `tests/` folder may include
/// this test file as a module (`mod name;` or a `#[path]`): then the
/// binary alone is not enough.
fn included_elsewhere(root: Option<&Path>, p: &Package, path: &str) -> bool {
    let Some(root) = root else {
        return false;
    };
    let stem = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let file = Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut files = Vec::new();
    rs_files(&root.join(&p.dir).join("tests"), &mut files);
    files.iter().any(|f| {
        !f.ends_with(path)
            && fs::read_to_string(f).is_ok_and(|t| {
                t.contains(&format!("mod {stem};")) || t.contains(&format!("{file}\""))
            })
    })
}

/// What a change needs, without looking at the files themselves.
#[cfg(test)]
fn select(
    packages: &[Package],
    changes: &[Change],
    critical: &[Critical],
    full: bool,
) -> Selection {
    select_in(None, packages, changes, critical, full)
}

fn select_in(
    root: Option<&Path>,
    packages: &[Package],
    changes: &[Change],
    critical: &[Critical],
    full: bool,
) -> Selection {
    let mut sel = Selection {
        full,
        ..Selection::default()
    };
    if full {
        sel.reasons.push("full run requested".into());
    }
    let mut changed: BTreeSet<String> = BTreeSet::new();
    let mut alone: BTreeSet<String> = BTreeSet::new();
    let mut binaries: BTreeSet<(String, String)> = BTreeSet::new();
    let mut jobs = Jobs {
        test: true,
        ..Jobs::default()
    };
    let mut docs = false;
    let mut code = false;
    let wasm = build_closure(packages, "egui-pop-gallery");
    let api = build_closure(packages, "egui-pop");

    for c in changes {
        let path = c.path.as_str();
        if path.ends_with(".rs") {
            jobs.fmt = true;
        }
        if path.ends_with("Cargo.toml") || path == "Cargo.lock" || path == "deny.toml" {
            jobs.deny = true;
            jobs.package = true;
        }
        if let Some(prefix) = EVERYTHING.iter().find(|p| path.starts_with(**p)) {
            sel.full = true;
            sel.reasons
                .push(format!("{path}: under {prefix}, runs everything"));
            continue;
        }
        if is_doc(path) {
            docs = true;
            for (p, b) in DOC_READERS {
                binaries.insert((p.to_string(), b.to_string()));
            }
            alone.insert("consumer-smoke".into());
            continue;
        }
        if let Some((prefix, readers)) = DATA_READERS.iter().find(|(p, _)| path.starts_with(*p)) {
            code = true;
            for (p, b) in *readers {
                binaries.insert((p.to_string(), b.to_string()));
            }
            sel.reasons
                .push(format!("{path}: data under {prefix}, read by named tests"));
            continue;
        }
        let Some(p) = package_of(packages, path) else {
            sel.full = true;
            sel.reasons
                .push(format!("{path}: no rule names it, runs everything"));
            continue;
        };
        code = true;
        let inner = &path[p.dir.len()..];
        if matches!(c.status, 'A' | 'D' | 'R' | 'C') {
            // The package's file list changes.
            jobs.package = true;
        }
        if inner.starts_with("tests/snapshots/") {
            // An image updated alone: the package's tests, nothing else.
            alone.insert(p.name.clone());
            continue;
        }
        if inner.ends_with("README.md") {
            binaries.insert(("egui-pop".into(), "readme_links".into()));
        }
        if let Some(bin) = binary_of(p, path)
            && !included_elsewhere(root, p, path)
        {
            // A test file: its own binary, nothing depends on it.
            jobs.clippy = true;
            binaries.insert((p.name.clone(), bin));
            continue;
        }
        jobs.clippy = true;
        let build_input = !(inner.starts_with("tests/")
            || inner.starts_with("examples/")
            || inner.starts_with("benches/"));
        if build_input && wasm.contains(&p.name) {
            jobs.wasm = true;
        }
        if build_input && api.contains(&p.name) {
            jobs.semver = true;
        }
        changed.insert(p.name.clone());
    }

    if sel.full {
        sel.jobs = Jobs::ALL;
        sel.packages = packages.iter().map(|p| p.name.clone()).collect();
        return sel;
    }

    let deps = closure(packages, &changed, |p| &p.deps);
    for name in &changed {
        let dependents: Vec<&String> = deps
            .iter()
            .filter(|d| {
                !changed.contains(*d)
                    && packages
                        .iter()
                        .any(|p| &p.name == *d && p.deps.contains(name))
            })
            .collect();
        let mut line = format!("{name}: changed");
        if !dependents.is_empty() {
            let _ = write!(
                line,
                ", with its dependents {}",
                dependents
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        sel.reasons.push(line);
    }
    sel.packages = deps.into_iter().chain(alone).collect();
    sel.binaries = binaries
        .into_iter()
        .filter(|(p, _)| !sel.packages.contains(p))
        .collect();
    sel.docs_only = docs && !code;
    if docs && !jobs.clippy {
        jobs.doc = true;
    }
    sel.critical = critical
        .iter()
        .filter(|c| !sel.packages.contains(&c.package))
        .filter(|c| {
            c.binary
                .as_ref()
                .is_none_or(|b| !sel.binaries.contains(&(c.package.clone(), b.clone())))
        })
        .filter(|c| !(sel.docs_only && c.gpu))
        .cloned()
        .collect();
    sel.jobs = jobs;
    sel
}

/// The human report: what changed, what runs, what is skipped and why.
fn report(sel: &Selection, packages: &[Package], changes: &[Change]) -> String {
    let mut r = String::new();
    let _ = writeln!(r, "## Selection\n");
    let _ = writeln!(r, "{} changed file(s).\n", changes.len());
    if sel.full {
        let _ = writeln!(r, "**Everything runs.**\n");
    }
    for reason in sel.reasons.iter().take(30) {
        let _ = writeln!(r, "- {reason}");
    }
    if sel.reasons.len() > 30 {
        let _ = writeln!(r, "- ... and {} more", sel.reasons.len() - 30);
    }
    if sel.docs_only {
        let _ = writeln!(r, "- documentation only: critical tests that render wait");
    }
    let _ = writeln!(r, "\n### Jobs\n");
    for (job, on) in sel.jobs.list() {
        let _ = writeln!(r, "- {job}: {}", if on { "runs" } else { "skipped" });
    }
    if !sel.full {
        let _ = writeln!(r, "\n### Tests\n");
        let list = |s: &BTreeSet<String>| {
            if s.is_empty() {
                "none".to_string()
            } else {
                s.iter().cloned().collect::<Vec<_>>().join(", ")
            }
        };
        let _ = writeln!(r, "- crates, every test: {}", list(&sel.packages));
        let bins: BTreeSet<String> = sel
            .binaries
            .iter()
            .map(|(p, b)| format!("{p}::{b}"))
            .collect();
        let _ = writeln!(r, "- single test binaries: {}", list(&bins));
        let skipped: BTreeSet<String> = packages
            .iter()
            .map(|p| p.name.clone())
            .filter(|n| !sel.packages.contains(n))
            .collect();
        let _ = writeln!(r, "- crates skipped: {}", list(&skipped));
        let crit: BTreeSet<String> = sel
            .critical
            .iter()
            .map(|c| match &c.binary {
                Some(b) if !b.is_empty() => format!("{}::{b} {}", c.package, c.name),
                _ => format!("{} {}", c.package, c.name),
            })
            .collect();
        let _ = writeln!(r, "- critical tests added: {}", list(&crit));
    }
    r
}

fn write_github(sel: &Selection, report: &str) -> Result<(), String> {
    let append = |var: &str, text: &str| -> Result<(), String> {
        let Ok(path) = std::env::var(var) else {
            return Ok(());
        };
        use std::io::Write as _;
        let mut f = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .map_err(|e| format!("{var}: {e}"))?;
        f.write_all(text.as_bytes())
            .map_err(|e| format!("{var}: {e}"))
    };
    let mut out = format!("full={}\n", sel.full);
    for (job, on) in sel.jobs.list() {
        let _ = writeln!(out, "{job}={on}");
    }
    append("GITHUB_OUTPUT", &out)?;
    append("GITHUB_STEP_SUMMARY", report)
}

/// One `cargo` run of the plan.
#[derive(Debug, PartialEq, Eq)]
pub struct Step(pub Vec<String>);

/// What runs of one package outside the whole packages.
#[derive(Debug, Default)]
struct Narrow {
    /// Binaries run whole.
    binaries: BTreeSet<String>,
    /// Targets holding critical tests: a binary, or the library.
    critical: BTreeSet<Option<String>>,
    /// A critical test in a module of an unknown binary.
    all_tests: bool,
}

/// The cargo runs a selection makes.
pub fn plan(sel: &Selection, packages: &[Package], extra: &[String]) -> Vec<Step> {
    let s = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let nextest = |args: Vec<String>| {
        let mut v = s(&["nextest", "run", "--locked", "--no-tests=warn"]);
        v.extend(args);
        v.extend(extra.iter().cloned());
        Step(v)
    };
    if sel.full {
        return vec![
            nextest(s(&["--workspace"])),
            Step(s(&["test", "--locked", "--workspace", "--doc"])),
        ];
    }
    let mut steps = Vec::new();
    if !sel.packages.is_empty() {
        steps.push(nextest(
            sel.packages
                .iter()
                .flat_map(|p| ["-p".to_string(), p.clone()])
                .collect(),
        ));
        let with_lib: Vec<String> = sel
            .packages
            .iter()
            .filter(|n| packages.iter().any(|p| &p.name == *n && p.has_lib))
            .flat_map(|p| ["-p".to_string(), p.clone()])
            .collect();
        if !with_lib.is_empty() {
            let mut v = s(&["test", "--locked", "--doc"]);
            v.extend(with_lib);
            steps.push(Step(v));
        }
    }
    // Per package: its single binaries, whole, and its critical tests.
    let mut per: BTreeMap<&str, Narrow> = BTreeMap::new();
    for (p, b) in &sel.binaries {
        per.entry(p).or_default().binaries.insert(b.clone());
    }
    for c in &sel.critical {
        let e = per.entry(&c.package).or_default();
        match &c.binary {
            // A critical test in a module whose binary is unknown: every
            // test binary of the package is built, the filter picks.
            Some(b) if b.is_empty() => e.all_tests = true,
            b => {
                e.critical.insert(b.clone());
            }
        }
    }
    for (p, n) in per {
        let Narrow {
            binaries: bins,
            critical: crit,
            all_tests,
        } = n;
        let mut args = s(&["-p", p]);
        let mut filter = Vec::new();
        for b in &bins {
            args.extend(s(&["--test", b]));
            filter.push(format!("binary_id({p}::{b})"));
        }
        for b in &crit {
            match b {
                Some(b) if !bins.contains(b) => args.extend(s(&["--test", b])),
                Some(_) => {}
                None => args.push("--lib".into()),
            }
        }
        if all_tests {
            args.extend(s(&["--tests"]));
        }
        if !crit.is_empty() || all_tests {
            let critical = if sel.docs_only {
                format!("({CRITICAL} - {CRITICAL_GPU})")
            } else {
                CRITICAL.to_string()
            };
            filter.push(critical);
        }
        args.push("-E".into());
        args.push(filter.join(" | "));
        steps.push(nextest(args));
    }
    steps
}

/// Arguments as a shell would need them typed.
fn shell_words(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.contains(['(', ' ', '|', '/', '^']) {
                format!("'{a}'")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn run_tests(sel: &Selection, packages: &[Package], extra: &[String]) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut failed = Vec::new();
    for Step(args) in plan(sel, packages, extra) {
        let shown = shell_words(&args);
        eprintln!("\n$ cargo {shown}");
        let status = Command::new(&cargo)
            .args(&args)
            .current_dir(super::root())
            .status()
            .map_err(|e| format!("cargo: {e}"))?;
        if !status.success() {
            failed.push(format!("cargo {shown} ({status})"));
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("failed:\n{}", failed.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, dir: &str, deps: &[&str], dev: &[&str], tests: &[&str]) -> Package {
        let build_deps: BTreeSet<String> = deps.iter().map(|s| s.to_string()).collect();
        let mut all = build_deps.clone();
        all.extend(dev.iter().map(|s| s.to_string()));
        Package {
            name: name.into(),
            dir: format!("{dir}/"),
            deps: all,
            build_deps,
            has_lib: true,
            tests: tests
                .iter()
                .map(|t| (t.to_string(), format!("{dir}/tests/{t}.rs")))
                .collect(),
        }
    }

    fn workspace() -> Vec<Package> {
        vec![
            pkg(
                "egui-pop-tokens",
                "crates/egui-pop-tokens",
                &[],
                &[],
                &["kits"],
            ),
            pkg(
                "egui-pop-model",
                "crates/egui-pop-model",
                &[],
                &[],
                &["shell"],
            ),
            pkg(
                "egui-pop-core",
                "crates/egui-pop-core",
                &["egui-pop-tokens", "egui-pop-model"],
                &[],
                &[],
            ),
            pkg(
                "egui-pop-viewport",
                "crates/egui-pop-viewport",
                &["egui-pop-core"],
                &[],
                &["view"],
            ),
            pkg(
                "egui-pop-i18n",
                "crates/egui-pop-i18n",
                &["egui-pop-core"],
                &["egui-pop-viewport"],
                &[],
            ),
            pkg(
                "egui-pop",
                "crates/egui-pop",
                &["egui-pop-core", "egui-pop-viewport", "egui-pop-i18n"],
                &[],
                &["readme_links", "minimal"],
            ),
            pkg(
                "egui-pop-gallery",
                "crates/egui-pop-gallery",
                &["egui-pop"],
                &[],
                &["gallery", "gallery_doc", "hosts_doc"],
            ),
            pkg(
                "consumer-smoke",
                "crates/consumer-smoke",
                &["egui-pop"],
                &[],
                &[],
            ),
        ]
    }

    fn critical() -> Vec<Critical> {
        vec![
            Critical {
                package: "egui-pop-tokens".into(),
                binary: Some("kits".into()),
                name: "critical_the_default_kit_is_a_kit_folder".into(),
                gpu: false,
            },
            Critical {
                package: "egui-pop-gallery".into(),
                binary: Some("gallery".into()),
                name: "critical_gpu_every_node_is_named".into(),
                gpu: true,
            },
        ]
    }

    fn changed(paths: &[&str]) -> Vec<Change> {
        paths
            .iter()
            .map(|p| Change {
                status: 'M',
                path: p.to_string(),
            })
            .collect()
    }

    fn names(s: &BTreeSet<String>) -> Vec<&str> {
        s.iter().map(String::as_str).collect()
    }

    #[test]
    fn a_docs_only_change_runs_the_doc_readers_and_cheap_critical_tests() {
        let ws = workspace();
        let sel = select(
            &ws,
            &changed(&["docs/kit.md", "README.md"]),
            &critical(),
            false,
        );
        assert!(!sel.full && sel.docs_only);
        assert_eq!(names(&sel.packages), ["consumer-smoke"]);
        assert!(
            sel.binaries
                .contains(&("egui-pop-gallery".into(), "gallery_doc".into()))
        );
        assert!(
            sel.binaries
                .contains(&("egui-pop".into(), "readme_links".into()))
        );
        assert_eq!(sel.critical.len(), 1, "{:?}", sel.critical);
        assert!(!sel.critical[0].gpu);
        let j = sel.jobs;
        assert!(j.test && j.doc);
        assert!(!j.fmt && !j.clippy && !j.wasm && !j.package && !j.semver && !j.deny);
    }

    #[test]
    fn a_source_change_selects_its_crate_and_dependents() {
        let ws = workspace();
        let sel = select(
            &ws,
            &changed(&["crates/egui-pop-viewport/src/view.rs"]),
            &critical(),
            false,
        );
        assert!(!sel.full);
        assert_eq!(
            names(&sel.packages),
            [
                "consumer-smoke",
                "egui-pop",
                "egui-pop-gallery",
                "egui-pop-i18n",
                "egui-pop-viewport"
            ]
        );
        // The gallery is selected whole: its critical test is in it.
        assert_eq!(sel.critical.len(), 1);
        assert_eq!(sel.critical[0].package, "egui-pop-tokens");
        let j = sel.jobs;
        assert!(j.fmt && j.clippy && j.test && j.wasm && j.semver);
        assert!(!j.deny && !j.package && !j.doc);
    }

    #[test]
    fn a_test_file_selects_its_binary_alone() {
        let ws = workspace();
        let sel = select(
            &ws,
            &changed(&["crates/egui-pop-viewport/tests/view.rs"]),
            &critical(),
            false,
        );
        assert!(sel.packages.is_empty());
        assert_eq!(
            sel.binaries.iter().collect::<Vec<_>>(),
            [&("egui-pop-viewport".to_string(), "view".to_string())]
        );
        assert!(sel.jobs.fmt && sel.jobs.clippy && !sel.jobs.semver && !sel.jobs.wasm);
        assert_eq!(sel.critical.len(), 2, "not docs only: the GPU ones run too");
    }

    #[test]
    fn a_snapshot_alone_runs_its_crate_only() {
        let ws = workspace();
        let sel = select(
            &ws,
            &changed(&["crates/egui-pop-gallery/tests/snapshots/x.png"]),
            &critical(),
            false,
        );
        assert_eq!(names(&sel.packages), ["egui-pop-gallery"]);
        let j = sel.jobs;
        assert!(j.test && !j.fmt && !j.clippy && !j.wasm && !j.semver);
    }

    #[test]
    fn workspace_files_and_unknown_paths_run_everything() {
        let ws = workspace();
        for path in [
            "Cargo.lock",
            "Cargo.toml",
            "rust-toolchain.toml",
            ".github/workflows/ci.yml",
            "xtask/src/main.rs",
            "crates/egui-pop-tokens/themes/pop-dark.yaml",
            "crates/egui-pop-test/src/lib.rs",
            "somewhere/new.txt",
        ] {
            let sel = select(&ws, &changed(&[path]), &critical(), false);
            assert!(sel.full, "{path}");
            assert_eq!(sel.jobs, Jobs::ALL, "{path}");
        }
    }

    #[test]
    fn a_crate_manifest_runs_deny_and_packaging() {
        let ws = workspace();
        let sel = select(
            &ws,
            &changed(&["crates/egui-pop-model/Cargo.toml"]),
            &critical(),
            false,
        );
        assert!(!sel.full);
        assert!(sel.jobs.deny && sel.jobs.package && sel.jobs.clippy);
        assert!(sel.packages.contains("egui-pop-core"));
    }

    #[test]
    fn a_new_file_in_a_crate_runs_packaging() {
        let ws = workspace();
        let changes = [Change {
            status: 'A',
            path: "crates/egui-pop-model/src/new.rs".into(),
        }];
        assert!(select(&ws, &changes, &critical(), false).jobs.package);
    }

    #[test]
    fn data_outside_the_crates_runs_its_readers() {
        let ws = workspace();
        let sel = select(
            &ws,
            &changed(&["examples/kits/flavour/kit.yaml"]),
            &critical(),
            false,
        );
        assert!(sel.packages.is_empty());
        assert!(
            sel.binaries
                .contains(&("egui-pop-tokens".into(), "kits".into()))
        );
        assert!(
            sel.binaries
                .contains(&("egui-pop-gallery".into(), "gallery".into()))
        );
        // Both critical tests sit in selected binaries: none is added.
        assert!(sel.critical.is_empty());
    }

    #[test]
    fn the_plan_builds_only_what_runs() {
        let ws = workspace();
        let sel = select(&ws, &changed(&["docs/kit.md"]), &critical(), false);
        let steps: Vec<String> = plan(&sel, &ws, &[])
            .into_iter()
            .map(|s| s.0.join(" "))
            .collect();
        assert_eq!(
            steps,
            [
                "nextest run --locked --no-tests=warn -p consumer-smoke",
                "test --locked --doc -p consumer-smoke",
                "nextest run --locked --no-tests=warn -p egui-pop --test readme_links -E binary_id(egui-pop::readme_links)",
                "nextest run --locked --no-tests=warn -p egui-pop-gallery --test gallery_doc --test hosts_doc -E binary_id(egui-pop-gallery::gallery_doc) | binary_id(egui-pop-gallery::hosts_doc)",
                "nextest run --locked --no-tests=warn -p egui-pop-tokens --test kits -E (test(/(^|::)critical_/) - test(/(^|::)critical_gpu_/))",
            ]
        );
        let full = select(&ws, &changed(&["Cargo.lock"]), &critical(), false);
        assert_eq!(plan(&full, &ws, &[]).len(), 2);
    }

    #[test]
    fn the_mark_is_found_with_its_function() {
        let src = "#[egui_pop_test::critical]\n#[test]\nfn a() {}\n\
                   // #[critical]\n#[test]\nfn b() {}\n\
                   #[egui_pop_test_macros::critical(gpu)]\n#[test]\npub fn c() {}\n";
        assert_eq!(
            critical_in(src),
            [
                ("critical_a".to_string(), false),
                ("critical_gpu_c".to_string(), true)
            ]
        );
    }
}
