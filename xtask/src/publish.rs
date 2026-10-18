//! `cargo xtask publish`: publishes the workspace's crates to crates.io,
//! in the order of their dependencies, skipping every crate whose version
//! is already there. The list comes from `cargo metadata` (every package
//! whose `publish` is not `false`), so a crate added to the workspace is
//! published without editing anything here.
//!
//! Each crate has its own version: only the crates whose version is not
//! on crates.io yet are published, and each gets a local tag,
//! `<crate>-vX.Y.Z`, on the commit it was published from.
//!
//! crates.io limits how many new crates an account publishes in a short
//! time (HTTP 429). A refused crate is tried again after `--interval`
//! seconds; any other failure stops the run. Running it again resumes
//! where it stopped: what was published is skipped.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead as _, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

/// A workspace package that may be published, and the publishable
/// workspace packages it needs at build time (dev-dependencies are
/// stripped from published manifests, so they do not order anything).
#[derive(Debug, PartialEq)]
struct Package {
    name: String,
    version: String,
    needs: BTreeSet<String>,
}

pub fn publish(args: &[String]) -> Result<(), String> {
    let mut dry_run = false;
    let mut interval = 600u64;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dry-run" => dry_run = true,
            "--interval" => {
                interval = it
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or("--interval takes a number of seconds")?;
            }
            other => return Err(format!("publish: unknown argument `{other}`")),
        }
    }
    let packages = packages()?;
    let order = order(&packages)?;
    println!(
        "{} crates, in the order of their dependencies:",
        order.len()
    );
    let mut tags = Vec::new();
    for p in &order {
        let state = if published(&p.name, &p.version)? {
            "on crates.io, skipped"
        } else if dry_run {
            "to publish"
        } else {
            publish_one(p, interval)?;
            tags.push(tag(p)?);
            "published"
        };
        println!("  {} {}: {state}", p.name, p.version);
    }
    if !tags.is_empty() {
        println!(
            "tagged {}; push them with `git push --tags`",
            tags.join(", ")
        );
    }
    Ok(())
}

/// A crate's release tag, `<crate>-vX.Y.Z`.
fn tag_name(p: &Package) -> String {
    format!("{}-v{}", p.name, p.version)
}

/// Tags the commit a crate was just published from.
fn tag(p: &Package) -> Result<String, String> {
    let name = tag_name(p);
    let status = Command::new("git")
        .args(["tag", "-a", &name, "-m"])
        .arg(format!("{} {}", p.name, p.version))
        .current_dir(super::root())
        .status()
        .map_err(|e| format!("git tag: {e}"))?;
    if !status.success() {
        return Err(format!("git tag {name} failed ({status})"));
    }
    Ok(name)
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

/// The workspace's publishable packages, from `cargo metadata`.
fn packages() -> Result<Vec<Package>, String> {
    let out = Command::new(cargo())
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(super::root())
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let all = meta["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?;
    // `publish` is null when the package may go to any registry, a list of
    // registries otherwise; `false` in the manifest gives an empty list.
    let publishable = |p: &serde_json::Value| {
        p["publish"].is_null()
            || p["publish"]
                .as_array()
                .is_some_and(|r| r.iter().any(|r| r == "crates-io"))
    };
    let names: BTreeSet<String> = all
        .iter()
        .filter(|p| publishable(p))
        .filter_map(|p| p["name"].as_str().map(String::from))
        .collect();
    Ok(all
        .iter()
        .filter(|p| publishable(p))
        .map(|p| Package {
            name: p["name"].as_str().unwrap_or_default().to_string(),
            version: p["version"].as_str().unwrap_or_default().to_string(),
            needs: p["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["kind"].is_null() || d["kind"] == "build")
                .filter_map(|d| d["name"].as_str())
                .filter(|n| names.contains(*n))
                .map(String::from)
                .collect(),
        })
        .collect())
}

/// Every package after the ones it needs; among those ready at the same
/// time, by name, so the order is the same on every machine.
fn order(packages: &[Package]) -> Result<Vec<&Package>, String> {
    let mut left: BTreeMap<&str, &Package> =
        packages.iter().map(|p| (p.name.as_str(), p)).collect();
    let mut done: BTreeSet<&str> = BTreeSet::new();
    let mut order = Vec::new();
    while !left.is_empty() {
        let ready: Vec<&str> = left
            .values()
            .filter(|p| p.needs.iter().all(|n| done.contains(n.as_str())))
            .map(|p| p.name.as_str())
            .collect();
        if ready.is_empty() {
            let names: Vec<&str> = left.keys().copied().collect();
            return Err(format!("a dependency cycle between {}", names.join(", ")));
        }
        for name in ready {
            order.push(left.remove(name).expect("ready packages are left"));
            done.insert(name);
        }
    }
    Ok(order)
}

/// Whether crates.io has this version of the crate.
fn published(name: &str, version: &str) -> Result<bool, String> {
    let url = format!("https://crates.io/api/v1/crates/{name}/{version}");
    let out = Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}"])
        .args(["-A", "egui-pop xtask publish"])
        .arg(&url)
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    match String::from_utf8_lossy(&out.stdout).trim() {
        "200" => Ok(true),
        "404" => Ok(false),
        code => Err(format!("crates.io answered {code} for {name} {version}")),
    }
}

/// `cargo publish -p`, tried again while crates.io refuses it for its
/// rate limit. Cargo waits until the crate is in the index before it
/// returns, so the next crate finds it.
fn publish_one(p: &Package, interval: u64) -> Result<(), String> {
    loop {
        println!("publishing {} {}", p.name, p.version);
        let mut child = Command::new(cargo())
            .args(["publish", "-p", &p.name])
            .current_dir(super::root())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cargo publish: {e}"))?;
        // Shown as it comes, and kept to read the refusal's reason.
        let mut said = String::new();
        if let Some(err) = child.stderr.take() {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                eprintln!("{line}");
                said.push_str(&line);
                said.push('\n');
            }
        }
        let status = child.wait().map_err(|e| format!("cargo publish: {e}"))?;
        if status.success() {
            return Ok(());
        }
        if !rate_limited(&said) {
            return Err(format!("cargo publish -p {} failed ({status})", p.name));
        }
        println!(
            "crates.io's rate limit: trying {} again in {interval} s",
            p.name
        );
        std::thread::sleep(Duration::from_secs(interval));
    }
}

/// Whether `cargo publish`'s output is crates.io refusing for its rate
/// limit rather than for the crate itself.
fn rate_limited(said: &str) -> bool {
    said.contains("429") || said.contains("Too Many Requests")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, needs: &[&str]) -> Package {
        Package {
            name: name.into(),
            version: "0.1.0".into(),
            needs: needs.iter().map(|n| n.to_string()).collect(),
        }
    }

    #[test]
    fn a_crate_comes_after_what_it_needs_and_ties_go_by_name() {
        let all = [
            pkg("facade", &["api", "views"]),
            pkg("views", &["api"]),
            pkg("api", &["core"]),
            pkg("core", &["tokens"]),
            pkg("tokens", &[]),
            pkg("model", &[]),
        ];
        let names: Vec<&str> = order(&all)
            .unwrap()
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["model", "tokens", "core", "api", "views", "facade"]);
    }

    #[test]
    fn a_cycle_is_an_error_naming_its_crates() {
        let all = [pkg("a", &["b"]), pkg("b", &["a"]), pkg("c", &[])];
        assert_eq!(order(&all).unwrap_err(), "a dependency cycle between a, b");
    }

    #[test]
    fn a_tag_names_the_crate_and_its_version() {
        assert_eq!(tag_name(&pkg("egui-pop-core", &[])), "egui-pop-core-v0.1.0");
    }

    #[test]
    fn only_the_rate_limit_is_tried_again() {
        assert!(rate_limited(
            "the remote server responded with an error (status 429 Too Many Requests)"
        ));
        assert!(!rate_limited("error: failed to verify package tarball"));
    }
}
