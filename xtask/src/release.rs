//! Changesets and releases (`docs/releasing.md`).
//!
//! A change users notice comes with a changeset, a Markdown file in
//! `.changeset/` whose front matter names the crates it changes and the
//! bump each takes (`patch`, `minor`, `major`), followed by the text of
//! the changelog entry. knope (`knope.toml`) reads them at release time
//! to raise the versions; the tasks here do the rest:
//!
//! - `changeset-check` checks the pending changesets (known crates, known
//!   bumps, a break travelling up to the crates that depend on the broken
//!   one) and, given the base of a pull request, that a pull request
//!   changing a published crate adds one;
//! - `release-notes` runs after knope's `PrepareRelease`: it writes the
//!   release into `CHANGELOG.md` from the changesets knope consumed (and
//!   from an `Unreleased` section written by hand, if one is left), then
//!   commits the release, signed off;
//! - `tag` tags a published minor of the facade, `vX.Y.0`, locally.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::publish::{self, Package};

/// Where the changesets are, from the workspace's root.
const DIR: &str = ".changeset";

/// The facade: its minors are tagged, and name the releases they are in.
const FACADE: &str = "egui-pop";

/// How far a change moves a crate's version. Before 1.0, knope applies
/// `major` to the middle number (0.2.3 to 0.3.0) and both `minor` and
/// `patch` to the last one (0.2.3 to 0.2.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

impl Bump {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "patch" => Some(Self::Patch),
            "minor" => Some(Self::Minor),
            "major" => Some(Self::Major),
            _ => None,
        }
    }

    /// The changelog section an entry of this bump goes to.
    fn section(self) -> &'static str {
        match self {
            Self::Major => "Breaking changes",
            Self::Minor => "Features",
            Self::Patch => "Fixes",
        }
    }
}

/// One changeset file.
#[derive(Debug, PartialEq)]
pub struct Changeset {
    /// Its file name, for messages.
    pub file: String,
    pub bumps: BTreeMap<String, Bump>,
    /// The changelog entry's text; empty for a changeset that only
    /// declares bumps (their text is written elsewhere).
    pub text: String,
}

impl Changeset {
    /// The highest bump it declares, which places its entry.
    fn bump(&self) -> Bump {
        self.bumps.values().copied().max().unwrap_or(Bump::Patch)
    }
}

/// Reads a changeset: `---`, one `crate: bump` per line, `---`, the text.
pub fn parse(file: &str, source: &str) -> Result<Changeset, String> {
    let source = source.replace("\r\n", "\n");
    let rest = source
        .strip_prefix("---\n")
        .ok_or_else(|| format!("{file}: starts with `---` and the crates it changes"))?;
    let rest = format!("\n{rest}");
    let (front, text) = match rest.split_once("\n---") {
        Some((front, text)) => (front, text.strip_prefix('\n').unwrap_or(text)),
        None => return Err(format!("{file}: the front matter has no closing `---`")),
    };
    let mut bumps = BTreeMap::new();
    for line in front.lines().filter(|l| !l.trim().is_empty()) {
        let (name, bump) = line
            .split_once(':')
            .ok_or_else(|| format!("{file}: `{line}` is not `crate: bump`"))?;
        let name = name.trim().trim_matches(|c| c == '"' || c == '\'');
        let bump = bump.trim().trim_matches(|c| c == '"' || c == '\'');
        let bump = Bump::parse(bump)
            .ok_or_else(|| format!("{file}: `{bump}` for {name} is not patch, minor or major"))?;
        if bumps.insert(name.to_string(), bump).is_some() {
            return Err(format!("{file}: {name} is named twice"));
        }
    }
    if bumps.is_empty() {
        return Err(format!("{file}: names no crate"));
    }
    Ok(Changeset {
        file: file.to_string(),
        bumps,
        text: text.trim().to_string(),
    })
}

/// The pending changesets, by file name.
fn pending(root: &Path) -> Result<Vec<Changeset>, String> {
    let dir = root.join(DIR);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| {
            let name = format!(
                "{DIR}/{}",
                p.file_name().unwrap_or_default().to_string_lossy()
            );
            let source = fs::read_to_string(p).map_err(|e| format!("{name}: {e}"))?;
            parse(&name, &source)
        })
        .collect()
}

/// What is wrong with a set of changesets: crates that are not
/// published, and breaks that do not travel up (a crate taking a
/// `major` makes every published crate depending on it take one too,
/// since they re-export its types; `docs/stability.md`).
pub fn problems(changesets: &[Changeset], packages: &[Package]) -> Vec<String> {
    let known: BTreeSet<&str> = packages.iter().map(|p| p.name.as_str()).collect();
    let mut out = Vec::new();
    let mut total: BTreeMap<&str, Bump> = BTreeMap::new();
    for c in changesets {
        for (name, bump) in &c.bumps {
            if !known.contains(name.as_str()) {
                out.push(format!(
                    "{}: `{name}` is not a published crate of the workspace",
                    c.file
                ));
            }
            let e = total.entry(name.as_str()).or_insert(*bump);
            *e = (*e).max(*bump);
        }
    }
    for (name, bump) in &total {
        if *bump != Bump::Major {
            continue;
        }
        let missing: Vec<&str> = dependents(packages, name)
            .into_iter()
            .filter(|d| total.get(d) != Some(&Bump::Major))
            .collect();
        if !missing.is_empty() {
            out.push(format!(
                "{name} takes a major (a break): the crates depending on it take one too, \
                 add `major` for {}",
                missing.join(", ")
            ));
        }
    }
    out
}

/// The published crates depending on `name`, directly or not.
fn dependents<'p>(packages: &'p [Package], name: &str) -> BTreeSet<&'p str> {
    let mut found: BTreeSet<&str> = BTreeSet::new();
    let mut todo = vec![name.to_string()];
    while let Some(n) = todo.pop() {
        for p in packages {
            if p.needs.contains(&n) && found.insert(p.name.as_str()) {
                todo.push(p.name.clone());
            }
        }
    }
    found
}

/// What a pull request's changed files say about changesets.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    /// No published crate changed.
    NoCrate,
    /// Changesets come with it; the crates changed but named by none.
    Covered { unnamed: Vec<String> },
    /// A release: it consumes changesets and adds none.
    Release,
    /// Labelled internal: nothing users notice.
    Internal,
    /// Crates changed without a changeset.
    Missing { crates: Vec<String> },
}

/// Judges a pull request from its changed files (`git diff
/// --name-status`: a status letter and a path).
pub fn judge(
    changes: &[(char, String)],
    packages: &[Package],
    pending: &[Changeset],
    internal: bool,
) -> Verdict {
    let mut crates: BTreeSet<String> = BTreeSet::new();
    let mut added = false;
    let mut removed = false;
    for (status, path) in changes {
        if let Some(name) = path.strip_prefix(&format!("{DIR}/")) {
            if name.ends_with(".md") {
                match status {
                    'D' => removed = true,
                    _ => added = true,
                }
            }
            continue;
        }
        let owner = packages
            .iter()
            .filter(|p| path.starts_with(&format!("{}/", p.dir)))
            .max_by_key(|p| p.dir.len());
        if let Some(p) = owner {
            let inner = &path[p.dir.len() + 1..];
            // Tests, examples and benchmarks are not what users get.
            let shipped = !(inner.starts_with("tests/")
                || inner.starts_with("examples/")
                || inner.starts_with("benches/"));
            if shipped {
                crates.insert(p.name.clone());
            }
        }
    }
    if crates.is_empty() {
        return Verdict::NoCrate;
    }
    if added {
        let named: BTreeSet<&String> = pending.iter().flat_map(|c| c.bumps.keys()).collect();
        let unnamed = crates.into_iter().filter(|c| !named.contains(c)).collect();
        return Verdict::Covered { unnamed };
    }
    if removed {
        return Verdict::Release;
    }
    if internal {
        return Verdict::Internal;
    }
    Verdict::Missing {
        crates: crates.into_iter().collect(),
    }
}

/// `cargo xtask changeset-check [--base REV] [--internal]`.
pub fn check(args: &[String]) -> Result<(), String> {
    let mut base = None;
    let mut internal = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--base" => base = Some(it.next().ok_or("--base takes a revision")?.clone()),
            "--internal" => internal = true,
            other => return Err(format!("changeset-check: unknown argument `{other}`")),
        }
    }
    let root = crate::root();
    let packages = publish::packages()?;
    let pending = pending(&root)?;
    let problems = problems(&pending, &packages);
    if !problems.is_empty() {
        return Err(format!("the changesets:\n  {}", problems.join("\n  ")));
    }
    println!("{} pending changeset(s), consistent", pending.len());
    let Some(base) = base else {
        return Ok(());
    };
    let diff = git(
        &root,
        &[
            "diff",
            "--name-status",
            "--no-renames",
            &format!("{base}...HEAD"),
        ],
    )?;
    let changes: Vec<(char, String)> = diff
        .lines()
        .filter_map(|l| {
            let (s, p) = l.split_once('\t')?;
            Some((s.chars().next()?, p.to_string()))
        })
        .collect();
    match judge(&changes, &packages, &pending, internal) {
        Verdict::NoCrate => println!("no published crate changed: no changeset needed"),
        Verdict::Release => println!("a release: it consumes changesets"),
        Verdict::Internal => println!("labelled internal: no changeset needed"),
        Verdict::Covered { unnamed } => {
            println!("a changeset comes with the change");
            if !unnamed.is_empty() {
                println!(
                    "note: changed but named by no changeset (fine if users notice nothing \
                     there): {}",
                    unnamed.join(", ")
                );
            }
        }
        Verdict::Missing { crates } => {
            return Err(format!(
                "published crates changed ({}) without a changeset.\n\
                 Add one in {DIR}/ (`knope document-change`, or by hand: see \
                 CONTRIBUTING.md, \"Changesets\"), or label the pull request `internal` \
                 when users notice nothing.",
                crates.join(", ")
            ));
        }
    }
    Ok(())
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

/// The `[package]` version of a manifest's text.
fn manifest_version(toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package
            && let Some(v) = line.strip_prefix("version")
            && let Some(v) = v.trim_start().strip_prefix('=')
        {
            return Some(v.trim().trim_matches('"').to_string());
        }
    }
    None
}

/// A crate released: its name and its new version.
type Released = (String, String);

/// The release's section of the changelog.
pub fn section(
    released: &[Released],
    changesets: &[Changeset],
    unreleased: &str,
    date: &str,
) -> String {
    let mut s = String::new();
    match released.iter().find(|(n, _)| n == FACADE) {
        Some((_, v)) => {
            let _ = writeln!(s, "## {v} ({date})\n");
        }
        None => {
            let _ = writeln!(s, "## {date}\n");
        }
    }
    let list: Vec<String> = released.iter().map(|(n, v)| format!("{n} {v}")).collect();
    let _ = writeln!(s, "Published: {}.\n", list.join(", "));
    for bump in [Bump::Major, Bump::Minor, Bump::Patch] {
        let entries: Vec<&Changeset> = changesets
            .iter()
            .filter(|c| !c.text.is_empty() && c.bump() == bump)
            .collect();
        if entries.is_empty() {
            continue;
        }
        let _ = writeln!(s, "### {}\n", bump.section());
        for c in entries {
            s.push_str(&entry(c));
        }
        s.push('\n');
    }
    let unreleased = unreleased.trim();
    if !unreleased.is_empty() {
        s.push_str(unreleased);
        s.push_str("\n\n");
    }
    s
}

/// A changeset as a list item: the crates it changes, then its text,
/// continued lines indented under the bullet.
fn entry(c: &Changeset) -> String {
    let crates: Vec<&str> = c.bumps.keys().map(String::as_str).collect();
    let mut s = format!("- {}: ", crates.join(", "));
    for (i, line) in c.text.lines().enumerate() {
        if i > 0 {
            s.push('\n');
            if !line.trim().is_empty() {
                s.push_str("  ");
            }
        }
        s.push_str(line.trim_end());
    }
    s.push('\n');
    s
}

/// Puts the release's section into the changelog, in place of its
/// `## Unreleased` section if it has one, before the latest release
/// otherwise. Returns the new text and what the Unreleased section held.
pub fn split_unreleased(changelog: &str) -> (String, String, String) {
    let lines: Vec<&str> = changelog.split_inclusive('\n').collect();
    let first = lines
        .iter()
        .position(|l| l.starts_with("## "))
        .unwrap_or(lines.len());
    let head: String = lines[..first].concat();
    if lines
        .get(first)
        .is_some_and(|l| l.trim_end() == "## Unreleased")
    {
        let end = lines[first + 1..]
            .iter()
            .position(|l| l.starts_with("## "))
            .map_or(lines.len(), |i| first + 1 + i);
        let unreleased: String = lines[first + 1..end].concat();
        let rest: String = lines[end..].concat();
        (head, unreleased, rest)
    } else {
        (head, String::new(), lines[first..].concat())
    }
}

/// Today's date (UTC), `YYYY-MM-DD`.
fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    civil(days)
}

/// A day count since 1970-01-01 as a date (Howard Hinnant's algorithm).
fn civil(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// `cargo xtask release-notes [--no-commit]`: after knope's
/// `PrepareRelease` (versions raised, changesets deleted, both staged).
pub fn notes(args: &[String]) -> Result<(), String> {
    let mut commit = true;
    for a in args {
        match a.as_str() {
            "--no-commit" => commit = false,
            other => return Err(format!("release-notes: unknown argument `{other}`")),
        }
    }
    let root = crate::root();
    let packages = publish::packages()?;
    // The crates whose version moved since the last commit.
    let mut released: Vec<Released> = Vec::new();
    for p in &packages {
        let before = git(&root, &["show", &format!("HEAD:{}/Cargo.toml", p.dir)])
            .ok()
            .and_then(|t| manifest_version(&t));
        if before.as_deref() != Some(p.version.as_str()) {
            released.push((p.name.clone(), p.version.clone()));
        }
    }
    if released.is_empty() {
        return Err("no crate's version moved: run `knope release`, not this task".into());
    }
    // The changesets knope consumed: deleted in the index, read from HEAD.
    let deleted = git(
        &root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=D",
            "--",
            DIR,
        ],
    )?;
    let mut changesets = Vec::new();
    for file in deleted.lines().filter(|f| f.ends_with(".md")) {
        let source = git(&root, &["show", &format!("HEAD:{file}")])?;
        changesets.push(parse(file, &source)?);
    }
    let path = root.join("CHANGELOG.md");
    let changelog = fs::read_to_string(&path).map_err(|e| format!("CHANGELOG.md: {e}"))?;
    let (head, unreleased, rest) = split_unreleased(&changelog);
    let section = section(&released, &changesets, &unreleased, &today());
    fs::write(&path, format!("{head}{section}{rest}")).map_err(|e| format!("CHANGELOG.md: {e}"))?;
    git(&root, &["add", "CHANGELOG.md"])?;
    let label = match released.iter().find(|(n, _)| n == FACADE) {
        Some((_, v)) => v.clone(),
        None => released
            .iter()
            .map(|(n, v)| format!("{n} {v}"))
            .collect::<Vec<_>>()
            .join(", "),
    };
    println!("CHANGELOG.md: the release of {label}");
    if commit {
        git(
            &root,
            &["commit", "-s", "-m", &format!("chore(release): {label}")],
        )?;
        println!("committed: chore(release): {label}");
    }
    Ok(())
}

/// `cargo xtask tag`: tags the facade's minor, `vX.Y.0`, on HEAD, once it
/// is on crates.io. Patches and sub-crates are not tagged. The tag is
/// local: pushing it is left to the person releasing.
pub fn tag(args: &[String]) -> Result<(), String> {
    if let Some(a) = args.first() {
        return Err(format!("tag: unknown argument `{a}`"));
    }
    let root = crate::root();
    let packages = publish::packages()?;
    let facade = packages
        .iter()
        .find(|p| p.name == FACADE)
        .ok_or("no egui-pop package")?;
    let version = &facade.version;
    if !version.ends_with(".0") {
        println!("{FACADE} {version} is a patch: patches are not tagged");
        return Ok(());
    }
    let name = format!("v{version}");
    if !git(&root, &["tag", "--list", &name])?.trim().is_empty() {
        println!("{name} exists already");
        return Ok(());
    }
    if !publish::published(FACADE, version)? {
        return Err(format!(
            "{FACADE} {version} is not on crates.io yet: publish first (`cargo xtask publish`)"
        ));
    }
    git(
        &root,
        &["tag", "-a", &name, "-m", &format!("{FACADE} {version}")],
    )?;
    println!("tagged {name}; push it with `git push origin {name}`");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, dir: &str, needs: &[&str]) -> Package {
        Package {
            name: name.into(),
            version: "0.2.0".into(),
            dir: dir.into(),
            needs: needs.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn workspace() -> Vec<Package> {
        vec![
            pkg("tokens", "crates/tokens", &[]),
            pkg("core", "crates/core", &["tokens"]),
            pkg("api", "crates/api", &["core"]),
            pkg("facade", "crates/facade", &["api", "core"]),
            pkg("router", "crates/router", &[]),
        ]
    }

    fn cs(file: &str, bumps: &[(&str, Bump)], text: &str) -> Changeset {
        Changeset {
            file: file.into(),
            bumps: bumps.iter().map(|(n, b)| (n.to_string(), *b)).collect(),
            text: text.into(),
        }
    }

    #[test]
    fn a_changeset_is_read_with_quoted_or_bare_names() {
        let c = parse(
            "a.md",
            "---\n\"core\": major\napi: minor\n---\n\nThe text,\non two lines.\n",
        )
        .unwrap();
        assert_eq!(c.bumps["core"], Bump::Major);
        assert_eq!(c.bumps["api"], Bump::Minor);
        assert_eq!(c.text, "The text,\non two lines.");
        assert_eq!(c.bump(), Bump::Major);
    }

    #[test]
    fn a_changeset_may_only_declare_bumps() {
        let c = parse("a.md", "---\ncore: patch\n---\n").unwrap();
        assert!(c.text.is_empty());
    }

    #[test]
    fn a_wrong_changeset_says_what_is_wrong() {
        assert!(
            parse("a.md", "core: major\n")
                .unwrap_err()
                .contains("starts with")
        );
        assert!(
            parse("a.md", "---\ncore: huge\n---\nx")
                .unwrap_err()
                .contains("huge")
        );
        assert!(
            parse("a.md", "---\n---\nx")
                .unwrap_err()
                .contains("names no crate")
        );
        assert!(
            parse("a.md", "---\ncore: major\n")
                .unwrap_err()
                .contains("closing")
        );
    }

    #[test]
    fn a_break_travels_up_to_every_dependent() {
        let ws = workspace();
        let only = [cs("a.md", &[("core", Bump::Major)], "x")];
        let p = problems(&only, &ws);
        assert_eq!(p.len(), 1);
        assert!(p[0].contains("api, facade"), "{p:?}");
        let all = [
            cs("a.md", &[("core", Bump::Major)], "x"),
            cs("b.md", &[("api", Bump::Major), ("facade", Bump::Major)], ""),
        ];
        assert!(problems(&all, &ws).is_empty());
        // A feature does not travel.
        let feature = [cs("a.md", &[("tokens", Bump::Minor)], "x")];
        assert!(problems(&feature, &ws).is_empty());
    }

    #[test]
    fn an_unknown_crate_is_named() {
        let p = problems(&[cs("a.md", &[("nope", Bump::Patch)], "x")], &workspace());
        assert!(p[0].contains("`nope`"));
    }

    fn changed(paths: &[(char, &str)]) -> Vec<(char, String)> {
        paths.iter().map(|(s, p)| (*s, p.to_string())).collect()
    }

    #[test]
    fn a_crate_changed_without_changeset_is_refused_unless_internal() {
        let ws = workspace();
        let c = changed(&[('M', "crates/core/src/lib.rs")]);
        assert_eq!(
            judge(&c, &ws, &[], false),
            Verdict::Missing {
                crates: vec!["core".into()]
            }
        );
        assert_eq!(judge(&c, &ws, &[], true), Verdict::Internal);
    }

    #[test]
    fn tests_docs_and_tools_need_no_changeset() {
        let c = changed(&[
            ('M', "crates/core/tests/keys.rs"),
            ('A', "crates/api/examples/demo.rs"),
            ('M', "docs/guide.md"),
            ('M', "xtask/src/main.rs"),
        ]);
        assert_eq!(judge(&c, &workspace(), &[], false), Verdict::NoCrate);
    }

    #[test]
    fn a_changeset_covers_the_change_and_unnamed_crates_are_noted() {
        let c = changed(&[
            ('M', "crates/core/src/lib.rs"),
            ('M', "crates/api/README.md"),
            ('A', ".changeset/core-fix.md"),
        ]);
        let pending = [cs(".changeset/core-fix.md", &[("core", Bump::Patch)], "x")];
        assert_eq!(
            judge(&c, &workspace(), &pending, false),
            Verdict::Covered {
                unnamed: vec!["api".into()]
            }
        );
    }

    #[test]
    fn a_release_consumes_changesets() {
        let c = changed(&[
            ('M', "crates/core/Cargo.toml"),
            ('D', ".changeset/core-fix.md"),
        ]);
        assert_eq!(judge(&c, &workspace(), &[], false), Verdict::Release);
    }

    #[test]
    fn the_package_version_is_read_from_its_table() {
        let t = "[package]\nname = \"a\"\nversion = \"0.3.1\"\n[dependencies]\nversion = \"9\"\n";
        assert_eq!(manifest_version(t).as_deref(), Some("0.3.1"));
        assert_eq!(manifest_version("[workspace]\nversion = \"1\"\n"), None);
    }

    #[test]
    fn the_unreleased_section_is_taken_out() {
        let log =
            "# Changelog\n\nIntro.\n\n## Unreleased\n\n### Added\n\n- a\n\n## 0.2.0 (d)\n\n- old\n";
        let (head, un, rest) = split_unreleased(log);
        assert_eq!(head, "# Changelog\n\nIntro.\n\n");
        assert_eq!(un, "\n### Added\n\n- a\n\n");
        assert_eq!(rest, "## 0.2.0 (d)\n\n- old\n");
        let (head, un, rest) = split_unreleased("# C\n\n## 0.2.0 (d)\n");
        assert_eq!(
            (head.as_str(), un.as_str(), rest.as_str()),
            ("# C\n\n", "", "## 0.2.0 (d)\n")
        );
    }

    #[test]
    fn a_release_section_groups_entries_by_bump() {
        let released = vec![
            ("egui-pop".to_string(), "0.3.0".to_string()),
            ("core".to_string(), "0.3.0".to_string()),
        ];
        let sets = [
            cs(
                "a.md",
                &[("core", Bump::Patch)],
                "Fixed a thing,\nfor good.\n\nAgain.",
            ),
            cs(
                "b.md",
                &[("core", Bump::Major), ("egui-pop", Bump::Major)],
                "Broke it.",
            ),
            cs("c.md", &[("core", Bump::Major)], ""),
        ];
        let s = section(&released, &sets, "\n### Added\n\n- by hand\n", "2026-11-03");
        assert_eq!(
            s,
            "## 0.3.0 (2026-11-03)\n\n\
             Published: egui-pop 0.3.0, core 0.3.0.\n\n\
             ### Breaking changes\n\n\
             - core, egui-pop: Broke it.\n\n\
             ### Fixes\n\n\
             - core: Fixed a thing,\n  for good.\n\n  Again.\n\n\
             ### Added\n\n- by hand\n\n"
        );
        let s = section(&[("core".into(), "0.3.1".into())], &[], "", "2026-11-04");
        assert_eq!(s, "## 2026-11-04\n\nPublished: core 0.3.1.\n\n");
    }

    #[test]
    fn days_become_dates() {
        assert_eq!(civil(0), "1970-01-01");
        assert_eq!(civil(20_760), "2026-11-03");
        assert_eq!(civil(11_016), "2000-02-29");
    }
}
