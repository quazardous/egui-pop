//! `cargo xtask shards`: CI's test shards split by measured duration
//! (`docs/testing.md`, "Shards by duration").
//!
//! The tests' durations are read from nextest's JUnit reports and kept in
//! `ci/test-durations.json`. The split is computed from that file alone,
//! so every shard of a run computes the same one: the tests are grouped
//! into units (a whole test binary, a module of one, or a single test,
//! a group cut finer while it weighs more than a twelfth of a shard),
//! and the units go, longest first, to the lightest shard. The lightest
//! shard at the end takes no list but the complement of the others' (`not
//! (...)`): a test the file does not know yet runs there, and the union of
//! the shards is every test by construction. `shards check` asks nextest
//! to list each shard and checks it anyway.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

pub const USAGE: &str = "\
cargo xtask shards [--shards N] [--filter I]
cargo xtask shards refresh [--run RUN_ID]... [PATH...]
cargo xtask shards check [--shards N]

  (no task)      print the split: each shard's expected time and filter;
                 --filter I prints shard I's nextest filter alone
  refresh        rewrite ci/test-durations.json from nextest JUnit reports:
                 files or folders (searched for *.xml), or the `junit-*`
                 artifacts of CI runs (`gh run download`); a test seen in
                 several reports gets its mean time
  check          list the tests of each shard with nextest (builds the
                 workspace's tests) and fail unless every test is in
                 exactly one shard
  --shards N     the number of shards (default 3)";

/// Where the durations are kept, from the workspace root.
pub const FILE: &str = "ci/test-durations.json";

/// A unit weighing more than this share of a shard is cut finer.
const UNIT_SHARE: f64 = 12.0;

/// The durations file: every test's time in seconds, per binary id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Durations {
    /// Tests run at once by nextest on a runner (its cores).
    pub threads: f64,
    /// Wall seconds of what only the first shard runs besides its tests
    /// (the doctests, the icon set's SVG tests, the shard check).
    pub first_shard_seconds: f64,
    /// Binary id, then test name, then seconds.
    pub tests: BTreeMap<String, BTreeMap<String, f64>>,
}

impl Durations {
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("{FILE}: {e}"))?;
        let num = |k: &str| v.get(k).and_then(Value::as_f64);
        let mut tests = BTreeMap::new();
        for (bin, cases) in v
            .get("tests")
            .and_then(Value::as_object)
            .ok_or(format!("{FILE}: no `tests` object"))?
        {
            let mut m = BTreeMap::new();
            for (name, t) in cases.as_object().ok_or(format!("{FILE}: {bin}"))? {
                m.insert(name.clone(), t.as_f64().unwrap_or(0.0));
            }
            tests.insert(bin.clone(), m);
        }
        Ok(Self {
            threads: num("threads").unwrap_or(4.0).max(1.0),
            first_shard_seconds: num("first_shard_seconds").unwrap_or(0.0),
            tests,
        })
    }

    pub fn to_json(&self, runs: &[String]) -> String {
        let tests: serde_json::Map<String, Value> = self
            .tests
            .iter()
            .map(|(b, m)| {
                let m: serde_json::Map<String, Value> = m
                    .iter()
                    .map(|(n, t)| (n.clone(), json!((t * 100.0).round() / 100.0)))
                    .collect();
                (b.clone(), Value::Object(m))
            })
            .collect();
        let v = json!({
            "about": "Every test's time on CI, in seconds, per nextest binary id: \
                      the input of CI's test shards (cargo xtask shards; \
                      docs/testing.md, \"Shards by duration\"). Refreshed by \
                      `cargo xtask shards refresh`.",
            "runs": runs,
            "threads": self.threads,
            "first_shard_seconds": self.first_shard_seconds,
            "tests": tests,
        });
        serde_json::to_string_pretty(&v).expect("json") + "\n"
    }

    fn total(&self) -> f64 {
        self.tests.values().flat_map(|m| m.values()).sum()
    }
}

/// A group of tests run together: a binary, a module of it (`prefix`),
/// or a single test (`exact`).
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub binary: String,
    pub prefix: Option<String>,
    pub exact: bool,
    pub seconds: f64,
}

impl Unit {
    pub fn filter(&self) -> String {
        let bin = format!("binary_id(={})", self.binary);
        match (&self.prefix, self.exact) {
            (None, _) => bin,
            (Some(p), true) => format!("({bin} & test(={p}))"),
            (Some(p), false) => format!("({bin} & test(/^{}(::|$)/))", regex_escape(p)),
        }
    }
}

fn regex_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if "\\.+*?()|[]{}^$/".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// One shard of the split.
#[derive(Debug, Clone, PartialEq)]
pub struct Shard {
    pub units: Vec<Unit>,
    /// Takes every test the other shards do not.
    pub rest: bool,
    /// Test seconds (summed, as if run one after another).
    pub seconds: f64,
}

/// The split of the tests into `n` shards.
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    pub shards: Vec<Shard>,
    pub threads: f64,
    pub first_shard_seconds: f64,
}

impl Split {
    /// Shard `i` (from 1)'s nextest filter.
    pub fn filter(&self, i: usize) -> String {
        let s = &self.shards[i - 1];
        if s.rest {
            let others: Vec<&Unit> = self
                .shards
                .iter()
                .filter(|o| !o.rest)
                .flat_map(|o| o.units.iter())
                .collect();
            if others.is_empty() {
                "all()".into()
            } else {
                format!("not ({})", render(&others))
            }
        } else if s.units.is_empty() {
            "none()".into()
        } else {
            render(&s.units.iter().collect::<Vec<_>>())
        }
    }

    /// Shard `i` (from 1)'s expected wall seconds: its tests over the
    /// runner's threads, plus the first shard's own steps.
    pub fn wall(&self, i: usize) -> f64 {
        let extra = if i == 1 {
            self.first_shard_seconds
        } else {
            0.0
        };
        self.shards[i - 1].seconds / self.threads + extra
    }
}

/// The filter of a set of units, kept short: per binary, the modules
/// and tests that share a parent module in one regular expression
/// (`^parent::(a|b)(::|$)`), so that the filter does not grow with the
/// name of every test the gallery's pages make.
pub fn render(units: &[&Unit]) -> String {
    // Binary, then parent module, then the names under it.
    let mut prefixes: BTreeMap<&str, BTreeMap<&str, Vec<&str>>> = BTreeMap::new();
    let mut exact: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut whole: BTreeSet<&str> = BTreeSet::new();
    for u in units {
        match (&u.prefix, u.exact) {
            (None, _) => {
                whole.insert(&u.binary);
            }
            (Some(p), true) => exact.entry(&u.binary).or_default().push(p),
            (Some(p), false) => {
                let (parent, name) = match p.rfind("::") {
                    Some(i) => (&p[..i + 2], &p[i + 2..]),
                    None => ("", p.as_str()),
                };
                prefixes
                    .entry(&u.binary)
                    .or_default()
                    .entry(parent)
                    .or_default()
                    .push(name);
            }
        }
    }
    let mut out: Vec<String> = whole.iter().map(|b| format!("binary_id(={b})")).collect();
    let binaries: BTreeSet<&str> = prefixes.keys().chain(exact.keys()).copied().collect();
    for b in binaries {
        let mut tests = Vec::new();
        if let Some(parents) = prefixes.get(b) {
            let alts: Vec<String> = parents
                .iter()
                .map(|(parent, names)| {
                    let names: Vec<String> = names.iter().map(|n| regex_escape(n)).collect();
                    format!("{}(?:{})", regex_escape(parent), names.join("|"))
                })
                .collect();
            tests.push(format!("test(/^(?:{})(::|$)/)", alts.join("|")));
        }
        for e in exact.get(b).into_iter().flatten() {
            tests.push(format!("test(={e})"));
        }
        out.push(format!("(binary_id(={b}) & ({}))", tests.join(" | ")));
    }
    out.join(" | ")
}

/// A node of a binary's test names, cut on `::`.
#[derive(Default)]
struct Node {
    seconds: f64,
    /// A test with exactly this name, and its time.
    test: Option<f64>,
    children: BTreeMap<String, Node>,
}

impl Node {
    fn insert(&mut self, path: &[&str], t: f64) {
        self.seconds += t;
        match path.split_first() {
            None => self.test = Some(t),
            Some((first, rest)) => self
                .children
                .entry(first.to_string())
                .or_default()
                .insert(rest, t),
        }
    }

    fn units(&self, binary: &str, prefix: Option<&str>, limit: f64, out: &mut Vec<Unit>) {
        let whole = Unit {
            binary: binary.into(),
            prefix: prefix.map(String::from),
            exact: false,
            seconds: self.seconds,
        };
        if self.seconds <= limit || self.children.is_empty() {
            out.push(whole);
            return;
        }
        if let (Some(t), Some(p)) = (self.test, prefix) {
            out.push(Unit {
                binary: binary.into(),
                prefix: Some(p.into()),
                exact: true,
                seconds: t,
            });
        } else if self.test.is_some() {
            // A test named like nothing (no prefix) cannot be: a binary
            // has no test with an empty name. Keep it whole, to be safe.
            out.push(whole);
            return;
        }
        for (name, child) in &self.children {
            let p = match prefix {
                Some(p) => format!("{p}::{name}"),
                None => name.clone(),
            };
            child.units(binary, Some(&p), limit, out);
        }
    }
}

/// Split the tests of `d` into `n` shards, longest unit first, each to
/// the lightest shard; the first shard starts with its own steps' time.
pub fn split(d: &Durations, n: usize) -> Split {
    let n = n.max(1);
    let limit = d.total() / n as f64 / UNIT_SHARE;
    let mut units = Vec::new();
    for (bin, tests) in &d.tests {
        let mut root = Node::default();
        for (name, t) in tests {
            let path: Vec<&str> = name.split("::").collect();
            root.insert(&path, *t);
        }
        root.units(bin, None, limit, &mut units);
    }
    units.sort_by(|a, b| {
        b.seconds
            .total_cmp(&a.seconds)
            .then_with(|| a.filter().cmp(&b.filter()))
    });
    let mut shards = vec![
        Shard {
            units: Vec::new(),
            rest: false,
            seconds: 0.0,
        };
        n
    ];
    // The load the greedy fills, in test seconds: the first shard's own
    // steps count as if they were tests.
    let mut load = vec![0.0; n];
    load[0] = d.first_shard_seconds * d.threads;
    for u in units {
        let i = lightest(&load);
        load[i] += u.seconds;
        shards[i].seconds += u.seconds;
        shards[i].units.push(u);
    }
    let rest = lightest(&load);
    shards[rest].rest = true;
    Split {
        shards,
        threads: d.threads,
        first_shard_seconds: d.first_shard_seconds,
    }
}

fn lightest(load: &[f64]) -> usize {
    let mut best = 0;
    for (i, l) in load.iter().enumerate() {
        if *l < load[best] {
            best = i;
        }
    }
    best
}

/// `I/N` read as (I, N), I from 1.
pub fn parse_shard(s: &str) -> Result<(usize, usize), String> {
    let (i, n) = s.split_once('/').ok_or(format!("`{s}`: expected I/N"))?;
    let i: usize = i.parse().map_err(|_| format!("`{s}`: expected I/N"))?;
    let n: usize = n.parse().map_err(|_| format!("`{s}`: expected I/N"))?;
    if i == 0 || i > n {
        return Err(format!("`{s}`: the shard is from 1 to {n}"));
    }
    Ok((i, n))
}

/// The split from the checked-in durations.
pub fn load(n: usize) -> Result<Split, String> {
    let path = super::root().join(FILE);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(split(&Durations::parse(&text)?, n))
}

/// The tests of nextest JUnit reports: binary id, test name, seconds.
pub fn junit_tests(xml: &str) -> Vec<(String, String, f64)> {
    let mut out = Vec::new();
    let mut suite = String::new();
    for tag in xml.split('<').skip(1) {
        let tag = tag.split('>').next().unwrap_or("");
        if let Some(rest) = tag.strip_prefix("testsuite ") {
            suite = attr(rest, "name").unwrap_or_default();
        } else if let Some(rest) = tag.strip_prefix("testcase ")
            && let (Some(name), Some(t)) = (attr(rest, "name"), attr(rest, "time"))
        {
            let class = attr(rest, "classname").unwrap_or_else(|| suite.clone());
            out.push((class, name, t.parse().unwrap_or(0.0)));
        }
    }
    out
}

/// An attribute's value in a tag's attributes (`name="v" time="1"`).
fn attr(tag: &str, key: &str) -> Option<String> {
    let tag = format!(" {tag}");
    let start = tag.find(&format!(" {key}=\"")).map(|i| i + key.len() + 3)?;
    let end = start + tag[start..].find('"')?;
    Some(
        tag[start..end]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&"),
    )
}

/// Durations from several reports: each test's mean time. The previous
/// file's settings (threads, the first shard's steps) are kept.
pub fn merge(previous: &Durations, reports: &[String]) -> Durations {
    let mut sums: BTreeMap<(String, String), (f64, f64)> = BTreeMap::new();
    for xml in reports {
        for (bin, name, t) in junit_tests(xml) {
            let e = sums.entry((bin, name)).or_default();
            e.0 += t;
            e.1 += 1.0;
        }
    }
    let mut tests: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
    for ((bin, name), (sum, count)) in sums {
        tests.entry(bin).or_default().insert(name, sum / count);
    }
    Durations {
        threads: previous.threads,
        first_shard_seconds: previous.first_shard_seconds,
        tests,
    }
}

fn xml_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for p in entries {
            xml_files(&p, out);
        }
    } else if path.extension().is_some_and(|e| e == "xml") {
        out.push(path.to_path_buf());
    }
}

fn refresh(args: &[String]) -> Result<(), String> {
    let root = super::root();
    let mut runs = Vec::new();
    let mut paths = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--run" => runs.push(it.next().ok_or("--run needs a run id")?.clone()),
            p if !p.starts_with("--") => paths.push(PathBuf::from(p)),
            other => return Err(format!("unknown argument `{other}`\n\n{USAGE}")),
        }
    }
    let download = root.join("target/shards-junit");
    for run in &runs {
        let dir = download.join(run);
        let _ = fs::remove_dir_all(&dir);
        let status = Command::new("gh")
            .args(["run", "download", run, "-p", "junit-*", "-D"])
            .arg(&dir)
            .current_dir(&root)
            .status()
            .map_err(|e| format!("gh: {e}"))?;
        if !status.success() {
            return Err(format!("gh run download {run} failed"));
        }
        paths.push(dir);
    }
    if paths.is_empty() {
        return Err(format!("no report given\n\n{USAGE}"));
    }
    let mut files = Vec::new();
    for p in &paths {
        xml_files(p, &mut files);
    }
    let reports: Vec<String> = files
        .iter()
        .map(|f| fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display())))
        .collect::<Result<_, _>>()?;
    let path = root.join(FILE);
    let previous = fs::read_to_string(&path)
        .ok()
        .and_then(|t| Durations::parse(&t).ok())
        .unwrap_or(Durations {
            threads: 4.0,
            ..Durations::default()
        });
    let d = merge(&previous, &reports);
    let count: usize = d.tests.values().map(BTreeMap::len).sum();
    if count == 0 {
        return Err("the reports hold no test".into());
    }
    fs::create_dir_all(path.parent().expect("parent")).map_err(|e| e.to_string())?;
    fs::write(&path, d.to_json(&runs)).map_err(|e| format!("{}: {e}", path.display()))?;
    println!(
        "{FILE}: {count} tests from {} reports, {:.0} test seconds",
        files.len(),
        d.total()
    );
    print_split(&split(&d, 3));
    Ok(())
}

fn print_split(s: &Split) {
    for (i, sh) in s.shards.iter().enumerate() {
        println!(
            "shard {}/{}: {:.0} s expected ({} units{})",
            i + 1,
            s.shards.len(),
            s.wall(i + 1),
            sh.units.len(),
            if sh.rest {
                ", and every unknown test"
            } else {
                ""
            }
        );
    }
}

/// The tests nextest selects with a filter: (binary id, test name).
fn nextest_list(filter: Option<&str>) -> Result<BTreeSet<(String, String)>, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.args([
        "nextest",
        "list",
        "--locked",
        "--workspace",
        "--message-format",
        "json",
    ]);
    if let Some(f) = filter {
        cmd.args(["-E", f]);
    }
    let out = cmd
        .current_dir(super::root())
        .output()
        .map_err(|e| format!("cargo nextest list: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo nextest list failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    listed(&String::from_utf8_lossy(&out.stdout))
}

/// The tests a `nextest list --message-format json` output selects.
pub fn listed(json: &str) -> Result<BTreeSet<(String, String)>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("nextest list: {e}"))?;
    let mut out = BTreeSet::new();
    for (bin, suite) in v
        .get("rust-suites")
        .and_then(Value::as_object)
        .ok_or("nextest list: no rust-suites")?
    {
        for (name, case) in suite
            .get("testcases")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let status = case.pointer("/filter-match/status").and_then(Value::as_str);
            if status == Some("matches") {
                out.insert((bin.clone(), name.clone()));
            }
        }
    }
    Ok(out)
}

/// Every test of `all` in exactly one of `shards`: the problems, if any.
pub fn coverage(
    all: &BTreeSet<(String, String)>,
    shards: &[BTreeSet<(String, String)>],
) -> Vec<String> {
    let mut problems = Vec::new();
    for t in all {
        let n = shards.iter().filter(|s| s.contains(t)).count();
        if n != 1 {
            problems.push(format!("{} {}: in {n} shards", t.0, t.1));
        }
    }
    for (i, s) in shards.iter().enumerate() {
        for t in s.difference(all) {
            problems.push(format!("{} {}: only in shard {}", t.0, t.1, i + 1));
        }
    }
    problems
}

fn check(n: usize) -> Result<(), String> {
    let split = load(n)?;
    let all = nextest_list(None)?;
    let mut shards = Vec::new();
    for i in 1..=n {
        let s = nextest_list(Some(&split.filter(i)))?;
        println!("shard {i}/{n}: {} tests", s.len());
        shards.push(s);
    }
    let problems = coverage(&all, &shards);
    if problems.is_empty() {
        println!(
            "every one of the {} tests is in exactly one shard",
            all.len()
        );
        Ok(())
    } else {
        let shown = problems.iter().take(50).cloned().collect::<Vec<_>>();
        Err(format!(
            "{} tests not in exactly one shard:\n{}",
            problems.len(),
            shown.join("\n")
        ))
    }
}

pub fn shards(args: &[String]) -> Result<(), String> {
    let mut n = 3;
    let mut only: Option<usize> = None;
    let mut task = None;
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "refresh" | "check" if task.is_none() => task = Some(a.clone()),
            "--shards" => {
                n = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|v| *v > 0)
                    .ok_or("--shards needs a number")?;
            }
            "--filter" => {
                only = Some(
                    it.next()
                        .and_then(|v| v.parse().ok())
                        .ok_or("--filter needs a shard number")?,
                );
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => rest.push(a.clone()),
        }
    }
    match task.as_deref() {
        Some("refresh") => refresh(&rest),
        Some("check") => check(n),
        _ => {
            if let Some(a) = rest.first() {
                return Err(format!("unknown argument `{a}`\n\n{USAGE}"));
            }
            let split = load(n)?;
            match only {
                Some(i) if i == 0 || i > n => Err(format!("shard {i}: from 1 to {n}")),
                Some(i) => {
                    println!("{}", split.filter(i));
                    Ok(())
                }
                None => {
                    print_split(&split);
                    for i in 1..=n {
                        println!("\nshard {i}/{n}: {}", split.filter(i));
                    }
                    Ok(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn durations<N: AsRef<str>>(tests: &[(&str, N, f64)], first: f64) -> Durations {
        let mut d = Durations {
            threads: 4.0,
            first_shard_seconds: first,
            ..Durations::default()
        };
        for (b, n, t) in tests {
            d.tests
                .entry(b.to_string())
                .or_default()
                .insert(n.as_ref().to_string(), *t);
        }
        d
    }

    #[test]
    fn junit_reports_are_read_per_binary() {
        let xml = r#"<?xml version="1.0"?>
<testsuites name="nextest-run" tests="2" time="3.5">
    <testsuite name="egui-pop-gallery::it" tests="2">
        <testcase name="gallery::a" classname="egui-pop-gallery::it" time="1.5"/>
        <testcase name="mounts::b&amp;c" classname="egui-pop-gallery::it" time="2">
            <flakyFailure message="x"/>
        </testcase>
    </testsuite>
</testsuites>"#;
        assert_eq!(
            junit_tests(xml),
            vec![
                ("egui-pop-gallery::it".into(), "gallery::a".into(), 1.5),
                ("egui-pop-gallery::it".into(), "mounts::b&c".into(), 2.0),
            ]
        );
    }

    #[test]
    fn several_reports_give_each_test_its_mean() {
        let r = |t: &str| {
            format!(
                r#"<testsuite name="b"><testcase name="x" classname="b" time="{t}"/></testsuite>"#
            )
        };
        let d = merge(&durations::<&str>(&[], 30.0), &[r("1"), r("3")]);
        assert_eq!(d.tests["b"]["x"], 2.0);
        assert_eq!(d.first_shard_seconds, 30.0);
        assert_eq!(Durations::parse(&d.to_json(&[])).unwrap(), d);
    }

    #[test]
    fn a_heavy_binary_is_cut_by_modules_and_light_ones_stay_whole() {
        let mut tests = vec![("small", "t".to_string(), 1.0)];
        for i in 0..40 {
            tests.push(("big", format!("m{}::t{i}", i % 4), 5.0));
        }
        let s = split(&durations(&tests, 0.0), 3);
        let units: Vec<String> = s
            .shards
            .iter()
            .flat_map(|sh| sh.units.iter().map(Unit::filter))
            .collect();
        assert!(units.contains(&"binary_id(=small)".to_string()));
        assert!(!units.contains(&"binary_id(=big)".to_string()));
        assert!(
            units
                .iter()
                .any(|u| u.starts_with("(binary_id(=big) & test("))
        );
        // Every test in exactly one unit.
        let total: f64 = s.shards.iter().map(|sh| sh.seconds).sum();
        assert_eq!(total, 201.0);
    }

    #[test]
    fn the_shards_come_out_balanced() {
        let mut tests = Vec::new();
        for i in 0..300 {
            let t = 1.0 + (i * 37 % 23) as f64;
            tests.push(("it", format!("m{}::t{i}", i % 7), t));
        }
        tests.push(("it", "slow::one".to_string(), 60.0));
        let s = split(&durations(&tests, 20.0), 3);
        let walls: Vec<f64> = (1..=3).map(|i| s.wall(i)).collect();
        let mean = walls.iter().sum::<f64>() / 3.0;
        let max = walls.iter().cloned().fold(0.0, f64::max);
        assert!(max - mean < 0.05 * mean, "{walls:?}");
    }

    #[test]
    fn exactly_one_shard_takes_the_rest_and_the_others_are_left_out_of_it() {
        let d = durations(&[("a", "x", 10.0), ("b", "y", 9.0), ("c", "z", 8.0)], 0.0);
        let s = split(&d, 3);
        assert_eq!(s.shards.iter().filter(|sh| sh.rest).count(), 1);
        let rest = s.shards.iter().position(|sh| sh.rest).unwrap() + 1;
        let f = s.filter(rest);
        assert!(f.starts_with("not ("), "{f}");
        for i in (1..=3).filter(|i| *i != rest) {
            for u in &s.shards[i - 1].units {
                assert!(f.contains(&format!("binary_id(={})", u.binary)), "{f}");
            }
        }
        // The same file gives the same split, in every shard's job.
        assert_eq!(split(&d, 3), s);
    }

    #[test]
    fn a_unit_filter_matches_its_module_and_a_test_of_its_name() {
        let u = Unit {
            binary: "egui-pop-gallery::it".into(),
            prefix: Some("gallery::every_page_renders".into()),
            exact: false,
            seconds: 1.0,
        };
        assert_eq!(
            u.filter(),
            "(binary_id(=egui-pop-gallery::it) & test(/^gallery::every_page_renders(::|$)/))"
        );
        let e = Unit { exact: true, ..u };
        assert_eq!(
            e.filter(),
            "(binary_id(=egui-pop-gallery::it) & test(=gallery::every_page_renders))"
        );
    }

    #[test]
    fn units_under_one_module_share_one_expression() {
        let u = |b: &str, p: Option<&str>, exact| Unit {
            binary: b.into(),
            prefix: p.map(String::from),
            exact,
            seconds: 1.0,
        };
        let units = [
            u("small", None, false),
            u("it", Some("gallery::every_page_renders::tokens"), false),
            u("it", Some("gallery::every_page_renders::kit"), false),
            u("it", Some("mounts"), false),
            u("it", Some("gallery"), true),
        ];
        assert_eq!(
            render(&units.iter().collect::<Vec<_>>()),
            "binary_id(=small) | (binary_id(=it) & (test(/^(?:(?:mounts)|\
             gallery::every_page_renders::(?:tokens|kit))(::|$)/) | test(=gallery)))"
        );
    }

    #[test]
    fn a_module_that_is_also_a_test_keeps_both() {
        let mut tests = vec![("it", "m".to_string(), 50.0)];
        for i in 0..10 {
            tests.push(("it", format!("m::t{i}"), 20.0));
        }
        let s = split(&durations(&tests, 0.0), 2);
        let units: Vec<String> = s
            .shards
            .iter()
            .flat_map(|sh| sh.units.iter().map(Unit::filter))
            .collect();
        assert!(units.contains(&"(binary_id(=it) & test(=m))".to_string()));
        assert_eq!(units.len(), 11);
    }

    #[test]
    fn the_coverage_check_finds_lost_and_doubled_tests() {
        let t = |n: &str| ("b".to_string(), n.to_string());
        let all: BTreeSet<_> = [t("a"), t("b"), t("c")].into();
        let ok = vec![[t("a")].into(), [t("b"), t("c")].into()];
        assert!(coverage(&all, &ok).is_empty());
        let bad = vec![[t("a"), t("b")].into(), [t("b")].into()];
        let p = coverage(&all, &bad);
        assert_eq!(p, vec!["b b: in 2 shards", "b c: in 0 shards"]);
    }

    #[test]
    fn nextest_list_json_counts_the_matching_tests() {
        let json = r#"{"rust-suites":{"b":{"testcases":{
            "x":{"filter-match":{"status":"matches"}},
            "y":{"filter-match":{"status":"mismatch","reason":"expression"}}}}}}"#;
        assert_eq!(
            listed(json).unwrap(),
            [("b".to_string(), "x".to_string())].into()
        );
    }

    #[test]
    fn shards_are_read_as_i_over_n() {
        assert_eq!(parse_shard("2/3"), Ok((2, 3)));
        assert!(parse_shard("0/3").is_err());
        assert!(parse_shard("4/3").is_err());
        assert!(parse_shard("3").is_err());
    }

    #[test]
    fn the_checked_in_durations_split_well() {
        let path = crate::root().join(FILE);
        let d = Durations::parse(&fs::read_to_string(path).unwrap()).unwrap();
        assert!(!d.tests.is_empty());
        let s = split(&d, 3);
        let walls: Vec<f64> = (1..=3).map(|i| s.wall(i)).collect();
        let mean = walls.iter().sum::<f64>() / 3.0;
        let max = walls.iter().cloned().fold(0.0, f64::max);
        assert!(max < 1.1 * mean, "{walls:?}");
    }
}
