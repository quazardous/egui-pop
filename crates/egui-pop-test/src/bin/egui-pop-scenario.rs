//! Runs scenarios against a running application over its agent socket
//! (`egui_pop_test::scenario::live`, `docs/scenarios.md`).
//!
//! ```text
//! egui-pop-scenario --socket PATH [--out DIR] [--var NAME=VALUE]...
//!                   [--shot CAPABILITY ARGS_JSON] [--deadline-ms N]
//!                   [--connect-ms N] FILE_OR_DIR...
//! ```
//!
//! One JSON line per scenario on stdout (`{"scenario", "passed", ...}`);
//! with `--out`, each scenario's journal as `<scenario>-journal.json` and
//! a failure's report as `<scenario>-failure.txt`. Exits 1 when one fails.

#[cfg(unix)]
fn main() {
    std::process::exit(unix::main());
}

#[cfg(not(unix))]
fn main() {
    eprintln!("egui-pop-scenario: live runs need a Unix socket (not yet on this system)");
    std::process::exit(2);
}

#[cfg(unix)]
mod unix {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use egui_pop_test::scenario::live::LiveRunner;
    use serde_json::{Value as Json, json};

    const USAGE: &str = "usage: egui-pop-scenario --socket PATH [--out DIR] [--var NAME=VALUE]... [--shot CAPABILITY ARGS_JSON] [--deadline-ms N] [--connect-ms N] FILE_OR_DIR...";

    fn parse(args: &[String]) -> Result<(LiveRunner, Option<PathBuf>, Vec<PathBuf>), String> {
        let mut socket = None;
        let mut out = None;
        let mut vars = Vec::new();
        let mut shot = None;
        let mut deadline = None;
        let mut connect = None;
        let mut files = Vec::new();
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
            match a.as_str() {
                "--socket" => socket = Some(value("--socket")?),
                "--out" => out = Some(PathBuf::from(value("--out")?)),
                "--var" => {
                    let v = value("--var")?;
                    let (k, v) = v.split_once('=').ok_or("--var takes NAME=VALUE")?;
                    vars.push((k.to_string(), v.to_string()));
                }
                "--shot" => {
                    let cap = value("--shot")?;
                    let args: Json = serde_json::from_str(&value("--shot")?)
                        .map_err(|e| format!("--shot's arguments: {e}"))?;
                    shot = Some((cap, args));
                }
                "--deadline-ms" => {
                    deadline = Some(
                        value("--deadline-ms")?
                            .parse::<u64>()
                            .map_err(|e| e.to_string())?,
                    );
                }
                "--connect-ms" => {
                    connect = Some(
                        value("--connect-ms")?
                            .parse::<u64>()
                            .map_err(|e| e.to_string())?,
                    );
                }
                "-h" | "--help" => return Err(USAGE.into()),
                f if f.starts_with("--") => return Err(format!("unknown option {f}\n{USAGE}")),
                f => files.push(PathBuf::from(f)),
            }
        }
        let socket = socket.ok_or(USAGE)?;
        if files.is_empty() {
            return Err(USAGE.into());
        }
        let mut runner = LiveRunner::new(socket);
        for (k, v) in vars {
            runner = runner.var(k, v);
        }
        if let Some(o) = &out {
            runner = runner.out(o.clone());
        }
        if let Some((cap, args)) = shot {
            runner = runner.shot(cap, args);
        }
        if let Some(ms) = deadline {
            runner = runner.deadline(Duration::from_millis(ms));
        }
        if let Some(ms) = connect {
            runner = runner.connect_within(Duration::from_millis(ms));
        }
        Ok((runner, out, files))
    }

    fn write(out: Option<&Path>, name: &str, text: &str) {
        if let Some(dir) = out {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(dir.join(name), text);
        }
    }

    pub fn main() -> i32 {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let (runner, out, files) = match parse(&args) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return 2;
            }
        };
        let mut failed = false;
        for f in files {
            let results = if f.is_dir() {
                runner.run_dir(&f)
            } else {
                let r = runner.run_file(&f);
                vec![(f.clone(), r)]
            };
            for (path, r) in results {
                let line = match r {
                    Ok(p) => {
                        let journal = serde_json::to_string_pretty(&p.journal).unwrap_or_default();
                        write(
                            out.as_deref(),
                            &format!("{}-journal.json", p.scenario),
                            &journal,
                        );
                        json!({
                            "scenario": p.scenario,
                            "file": path,
                            "passed": true,
                            "steps": p.steps,
                            "shots": p.shots,
                        })
                    }
                    Err(e) => {
                        failed = true;
                        let report = e.to_string();
                        eprintln!("{report}");
                        write(
                            out.as_deref(),
                            &format!("{}-failure.txt", e.scenario),
                            &report,
                        );
                        json!({
                            "scenario": e.scenario,
                            "file": path,
                            "passed": false,
                            "step": e.step,
                            "broken": e.broken,
                            "why": e.message,
                        })
                    }
                };
                println!("{line}");
            }
        }
        i32::from(failed)
    }
}
