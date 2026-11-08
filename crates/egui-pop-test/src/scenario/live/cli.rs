//! The `egui-pop-scenario` command line, as a function an application
//! can call from a binary of its own (`cli::main`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value as Json, json};

use super::LiveRunner;

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

/// Runs the command line on its arguments (the program's name left
/// out); the exit status: 0 when every scenario passed, 1 when one
/// failed, 2 for a usage error.
pub fn main(args: Vec<String>) -> i32 {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_command_line_reads_its_options() {
        let (_, out, files) = parse(&args(&[
            "--socket",
            "/tmp/a.sock",
            "--out",
            "/tmp/out",
            "--var",
            "folder=/tmp/p",
            "--shot",
            "app.shot",
            r#"{"path": "$path"}"#,
            "a.ron",
        ]))
        .unwrap();
        assert_eq!(out, Some(PathBuf::from("/tmp/out")));
        assert_eq!(files, [PathBuf::from("a.ron")]);
        assert!(parse(&args(&["a.ron"])).is_err(), "no socket");
        assert!(parse(&args(&["--socket", "s"])).is_err(), "no file");
        assert!(parse(&args(&["--socket", "s", "--var", "x", "a.ron"])).is_err());
        assert!(parse(&args(&["--socket", "s", "--nope", "a.ron"])).is_err());
    }
}
