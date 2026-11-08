//! Live runs: a scenario replayed against a running application over its
//! agent socket (`egui-pop-agent`, `docs/agent-socket.md`), as an agent
//! drives it, rather than in a kittest harness.
//!
//! The same files run both ways. Live, the steps that go through the
//! bridge (`Call`, `Wait`, `Expect(NoError)`, `Expect(Value)`,
//! `Expect(Journal)`, `Shot`) work as in-process; the ones that need the
//! accessibility tree (`Ui`, `Expect(Tree)`) are refused, until the
//! `ui.*` side over eframe's `inspection` exists. After every step the
//! runner checks `NoErrorToast`, `NoFailedCommand`, `NoUnknownCommand`,
//! `NoMissingText`, and `NoPanic` (the application is gone: it crashed or
//! exited); `StableWindows` and `FrameBudget` need the frames, so they
//! are in-process only.
//!
//! A shot keeps every collector's snapshot as `<scenario>-<name>.json` in
//! the output folder, and, when the runner is given a shot capability
//! (`LiveRunner::shot`), calls it with `$path` replaced by
//! `<scenario>-<name>.png` and waits for the file: an application names
//! its own screenshot command.
//!
//! ```no_run
//! use egui_pop_test::scenario::live::LiveRunner;
//!
//! let runner = LiveRunner::new("/run/user/1000/egui-pop-agent-my-app.sock")
//!     .var("folder", "/tmp/photos")
//!     .out("/tmp/out");
//! match runner.run_file("tests/scenarios/import.ron") {
//!     Ok(p) => println!("{} passed, {} steps", p.scenario, p.steps),
//!     Err(f) => eprintln!("{f}"),
//! }
//! ```

pub mod cli;

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value as Json, json};

use super::invariants::{
    NO_ERROR_TOAST, NO_FAILED_COMMAND, NO_MISSING_TEXT, NO_PANIC, NO_UNKNOWN_COMMAND,
};
use super::{
    Expect, Failure, Fixture, Runner, Scenario, Step, Wait, error_toasts, holds, missing_text,
    step_text, value_at,
};

/// Why a request got no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientError {
    /// The connection is lost: the application crashed or exited.
    Gone(String),
    /// The application answered with an error.
    Rpc(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Gone(m) => write!(f, "the application is gone: {m}"),
            ClientError::Rpc(m) => write!(f, "{m}"),
        }
    }
}

/// A blocking JSON-RPC 2.0 client of the agent socket: one request per
/// line, one answer per line.
#[derive(Debug)]
pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next: u64,
}

impl Client {
    /// Connects to a socket.
    pub fn connect(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let writer = UnixStream::connect(path)?;
        let reader = BufReader::new(writer.try_clone()?);
        Ok(Self {
            reader,
            writer,
            next: 1,
        })
    }

    /// Connects, retrying until `within` passes: the application may
    /// still be starting.
    pub fn connect_within(path: impl AsRef<Path>, within: Duration) -> Result<Self, String> {
        let end = Instant::now() + within;
        loop {
            match Self::connect(path.as_ref()) {
                Ok(c) => return Ok(c),
                Err(e) if Instant::now() >= end => {
                    return Err(format!(
                        "no agent socket at {} after {within:?}: {e}",
                        path.as_ref().display()
                    ));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    }

    /// Sends a request and reads its answer's result.
    pub fn request(&mut self, method: &str, params: Json) -> Result<Json, ClientError> {
        let id = self.next;
        self.next += 1;
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let gone = |e: std::io::Error| ClientError::Gone(e.to_string());
        writeln!(self.writer, "{line}").map_err(gone)?;
        self.writer.flush().map_err(gone)?;
        let mut answer = String::new();
        if self.reader.read_line(&mut answer).map_err(gone)? == 0 {
            return Err(ClientError::Gone("the socket closed".into()));
        }
        let answer: Json = serde_json::from_str(&answer)
            .map_err(|e| ClientError::Rpc(format!("an answer that is not JSON: {e}")))?;
        if let Some(e) = answer.get("error") {
            // -32002: no frame answered, or the application has closed.
            if e["code"] == -32002 {
                let m = e["message"].as_str().unwrap_or("no answer");
                return Err(ClientError::Gone(m.to_string()));
            }
            return Err(ClientError::Rpc(format!(
                "{} ({})",
                e["message"].as_str().unwrap_or("an error"),
                e["code"]
            )));
        }
        Ok(answer["result"].clone())
    }

    /// Every collector's snapshot.
    pub fn snapshot(&mut self) -> Result<Json, ClientError> {
        self.request("snapshot", json!({}))
    }

    /// The journal's last sequence number.
    pub fn last_event(&mut self) -> Result<u64, ClientError> {
        let s = self.request("snapshot", json!({ "collector": "journal" }))?;
        Ok(s["last"]
            .as_u64()
            .or_else(|| s["journal"]["last"].as_u64())
            .unwrap_or(0))
    }

    /// The journal's events after `seq`.
    pub fn events_since(&mut self, seq: u64) -> Result<Vec<Json>, ClientError> {
        let f = self.request(
            "call",
            json!({ "collector": "journal", "action": "since", "args": { "seq": seq } }),
        )?;
        Ok(f["data"].as_array().cloned().unwrap_or_default())
    }
}

/// What a passed live scenario did.
#[derive(Clone, Debug, PartialEq)]
pub struct LivePassed {
    pub scenario: String,
    pub steps: usize,
    /// The files its shots kept (snapshots as JSON, pictures).
    pub shots: Vec<PathBuf>,
    /// The journal's events during the scenario.
    pub journal: Vec<Json>,
}

/// Runs scenarios against a running application, over its agent socket.
#[derive(Debug, Clone)]
pub struct LiveRunner {
    socket: PathBuf,
    vars: Vec<(String, String)>,
    out: Option<PathBuf>,
    shot: Option<(String, Json)>,
    deadline: Duration,
    connect: Duration,
}

type StepError = (String, String);

fn gone(e: &ClientError) -> StepError {
    match e {
        ClientError::Gone(_) => (format!("invariant {NO_PANIC}"), e.to_string()),
        ClientError::Rpc(m) => ("rpc".into(), m.clone()),
    }
}

impl LiveRunner {
    /// A runner on the socket at `socket`.
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
            vars: Vec::new(),
            out: None,
            shot: None,
            deadline: Duration::from_secs(10),
            connect: Duration::from_secs(60),
        }
    }

    /// A variable: `$name` in a string argument (of a step or a fixture)
    /// is replaced by `value` (a folder the run made, a file).
    #[must_use]
    pub fn var(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.push((name.into(), value.into()));
        self
    }

    /// Where shots go.
    #[must_use]
    pub fn out(mut self, dir: impl Into<PathBuf>) -> Self {
        self.out = Some(dir.into());
        self
    }

    /// The capability a `Shot` calls for a picture, with its arguments;
    /// `$path` in them is the picture's path, `$name` the shot's name.
    #[must_use]
    pub fn shot(mut self, capability: impl Into<String>, args: Json) -> Self {
        self.shot = Some((capability.into(), args));
        self
    }

    /// How long a `Wait`, or a shot's picture, waits (10 s by default).
    #[must_use]
    pub fn deadline(mut self, d: Duration) -> Self {
        self.deadline = d;
        self
    }

    /// How long the runner tries to connect (60 s by default).
    #[must_use]
    pub fn connect_within(mut self, d: Duration) -> Self {
        self.connect = d;
        self
    }

    /// Runs a scenario's file.
    pub fn run_file(&self, path: impl AsRef<Path>) -> Result<LivePassed, Box<Failure>> {
        let s = Runner::load(path)?;
        self.run(&s)
    }

    /// Runs the `*.ron` files of a folder (not its subfolders), by name.
    pub fn run_dir(
        &self,
        dir: impl AsRef<Path>,
    ) -> Vec<(PathBuf, Result<LivePassed, Box<Failure>>)> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir.as_ref())
            .map(|d| {
                d.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "ron"))
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        files
            .into_iter()
            .map(|p| {
                let r = self.run_file(&p);
                (p, r)
            })
            .collect()
    }

    /// Runs one scenario on a connection of its own.
    pub fn run(&self, scenario: &Scenario) -> Result<LivePassed, Box<Failure>> {
        let failure = |broken: &str, message: String| {
            Box::new(Failure {
                scenario: scenario.name.clone(),
                step: None,
                step_text: String::new(),
                broken: broken.into(),
                message,
                journal: Vec::new(),
                tree: String::new(),
                snapshot: None,
            })
        };
        let client = Client::connect_within(&self.socket, self.connect)
            .map_err(|m| failure("connect", m))?;
        let mut run = LiveRun {
            runner: self,
            scenario,
            client,
            caps: Json::Null,
            start: 0,
            mark: 0,
            missing_before: Vec::new(),
            shots: Vec::new(),
        };
        let setup = (|| -> Result<(), ClientError> {
            run.caps = run.client.request("describe", Json::Null)?;
            run.start = run.client.last_event()?;
            run.mark = run.start;
            run.missing_before = missing_text(&run.client.snapshot()?);
            Ok(())
        })();
        if let Err(e) = setup {
            return Err(failure("connect", e.to_string()));
        }
        let unknown: Vec<&String> = scenario
            .allow
            .iter()
            .filter(|a| !super::invariants::ALL.contains(&a.as_str()))
            .collect();
        if !unknown.is_empty() {
            return Err(run.fail(
                None,
                "allow",
                "load",
                format!("unknown invariants {unknown:?}"),
            ));
        }
        for Fixture(name, args) in &scenario.fixtures {
            let text = format!("Fixture({name:?}, {args})");
            if !name.starts_with("test.") {
                return Err(run.fail(
                    None,
                    &text,
                    "fixture",
                    "a fixture is a `test.*` capability".into(),
                ));
            }
            let args = self.substitute(args);
            if let Err((broken, message)) = run.call(name, &args) {
                return Err(run.fail(None, &text, &broken, message));
            }
        }
        for (i, step) in scenario.steps.iter().enumerate() {
            let text = step_text(step);
            let before = match run.client.last_event() {
                Ok(b) => b,
                Err(e) => {
                    let (broken, message) = gone(&e);
                    return Err(run.fail(Some(i + 1), &text, &broken, message));
                }
            };
            if let Err((broken, message)) = run.step(step) {
                return Err(run.fail(Some(i + 1), &text, &broken, message));
            }
            if let Err((name, message)) = run.invariants(before) {
                return Err(run.fail(Some(i + 1), &text, &name, message));
            }
        }
        let journal = run.client.events_since(run.start).unwrap_or_default();
        Ok(LivePassed {
            scenario: scenario.name.clone(),
            steps: scenario.steps.len(),
            shots: run.shots,
            journal,
        })
    }

    /// `$name` replaced in every string of `args`.
    fn substitute(&self, args: &Json) -> Json {
        self.substitute_with(args, &[])
    }

    fn substitute_with(&self, args: &Json, more: &[(&str, &str)]) -> Json {
        match args {
            Json::String(s) => {
                let mut s = s.clone();
                // Longest names first, so `$folder2` is not `$folder` + 2.
                let mut vars: Vec<(&str, &str)> = self
                    .vars
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .chain(more.iter().copied())
                    .collect();
                vars.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
                for (k, v) in vars {
                    s = s.replace(&format!("${k}"), v);
                }
                Json::String(s)
            }
            Json::Array(a) => {
                Json::Array(a.iter().map(|v| self.substitute_with(v, more)).collect())
            }
            Json::Object(o) => Json::Object(
                o.iter()
                    .map(|(k, v)| (k.clone(), self.substitute_with(v, more)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

struct LiveRun<'r> {
    runner: &'r LiveRunner,
    scenario: &'r Scenario,
    client: Client,
    caps: Json,
    start: u64,
    mark: u64,
    missing_before: Vec<String>,
    shots: Vec<PathBuf>,
}

impl LiveRun<'_> {
    fn allowed(&self, name: &str) -> bool {
        self.scenario.allow.iter().any(|a| a == name)
    }

    fn call(&mut self, name: &str, args: &Json) -> Result<(), StepError> {
        let all = self.caps["capabilities"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let Some(cap) = all.iter().find(|c| c["name"] == name).cloned() else {
            if self.allowed(NO_UNKNOWN_COMMAND) {
                return Ok(());
            }
            return Err((
                format!("invariant {NO_UNKNOWN_COMMAND}"),
                format!("no capability `{name}` (see `bridge.describe`)"),
            ));
        };
        if name == "commands.run" {
            let id = args["id"].as_str().unwrap_or_default();
            let known = all
                .iter()
                .any(|c| c["kind"] == "command" && c["name"] == id);
            if !known && !self.allowed(NO_UNKNOWN_COMMAND) {
                return Err((
                    format!("invariant {NO_UNKNOWN_COMMAND}"),
                    format!("no command `{id}`"),
                ));
            }
        }
        let before = self.client.last_event().map_err(|e| gone(&e))?;
        let params = if cap["kind"] == "command" {
            json!({ "collector": "commands", "action": "run", "args": { "id": name, "args": args } })
        } else {
            json!({
                "collector": cap["call"]["collector"],
                "action": cap["call"]["action"],
                "args": args,
            })
        };
        let f = self.client.request("call", params).map_err(|e| gone(&e))?;
        if f["ok"] != true {
            let failed = self
                .client
                .events_since(before)
                .map_err(|e| gone(&e))?
                .iter()
                .any(|e| e["name"] == "command.failed");
            // A command that ran and failed is NoFailedCommand's.
            if !failed {
                let m = f["message"].as_str().unwrap_or("refused").to_string();
                return Err(("call".into(), m));
            }
        }
        Ok(())
    }

    fn wait_args(w: &Wait) -> Json {
        match w {
            Wait::Settled => json!({ "settled": true }),
            Wait::Event(name) => json!({ "event": name }),
            Wait::Any(any) => json!({ "any": any.iter().map(Self::wait_args).collect::<Vec<_>>() }),
        }
    }

    fn step(&mut self, step: &Step) -> Result<(), StepError> {
        match step {
            Step::Call(name, args) => {
                self.mark = self.client.last_event().map_err(|e| gone(&e))?;
                let args = self.runner.substitute(args);
                self.call(name, &args)
            }
            Step::Wait(w) => {
                let mut args = Self::wait_args(w);
                args["since"] = json!(self.mark);
                args["deadline_ms"] = json!(self.runner.deadline.as_millis() as u64);
                let r = self.client.request("wait", args).map_err(|e| gone(&e))?;
                if r["met"] == true {
                    Ok(())
                } else {
                    Err((
                        "wait".into(),
                        format!("{w:?} not met within {:?}", self.runner.deadline),
                    ))
                }
            }
            Step::Ui(_) => Err((
                "ui".into(),
                "`Ui` steps are not live yet (they need the `ui.*` side): run the scenario in-process, or use commands".into(),
            )),
            Step::Expect(e) => self.expect(e),
            Step::Shot(name) => self.shoot(name),
        }
    }

    fn expect(&mut self, e: &Expect) -> Result<(), StepError> {
        let no = |m: String| ("expectation".to_string(), m);
        match e {
            Expect::NoError => {
                let snapshot = self.client.snapshot().map_err(|e| gone(&e))?;
                let errors = error_toasts(&snapshot);
                if !errors.is_empty() {
                    return Err(no(format!("error toasts: {errors:?}")));
                }
                let failed: Vec<String> = self
                    .client
                    .events_since(self.start)
                    .map_err(|e| gone(&e))?
                    .iter()
                    .filter(|e| e["name"] == "command.failed")
                    .map(|e| e["data"].to_string())
                    .collect();
                if failed.is_empty() {
                    Ok(())
                } else {
                    Err(no(format!("command.failed: {failed:?}")))
                }
            }
            Expect::Tree { .. } => Err(no(
                "`Expect(Tree)` is not live yet (it needs the `ui.*` side)".into(),
            )),
            Expect::Value(path, want) => {
                let snapshot = self.client.snapshot().map_err(|e| gone(&e))?;
                let got = value_at(&snapshot, path).map_err(no)?;
                if got == *want {
                    Ok(())
                } else {
                    Err(no(format!("`{path}` is {got}, not {want}")))
                }
            }
            Expect::Journal { has, data } => {
                let events = self.client.events_since(self.start).map_err(|e| gone(&e))?;
                let hit = events.iter().any(|e| {
                    e["name"] == *has && data.as_ref().is_none_or(|d| holds(&e["data"], d))
                });
                if hit {
                    Ok(())
                } else {
                    let names: Vec<&str> =
                        events.iter().filter_map(|e| e["name"].as_str()).collect();
                    Err(no(match data {
                        Some(d) => {
                            format!("no `{has}` holding {d} on the journal; it has {names:?}")
                        }
                        None => format!("no `{has}` on the journal; it has {names:?}"),
                    }))
                }
            }
        }
    }

    fn shoot(&mut self, name: &str) -> Result<(), StepError> {
        let file = format!("{}-{name}", self.scenario.name);
        let Some(out) = self.runner.out.clone() else {
            return Ok(());
        };
        let io = |e: std::io::Error| ("shot".to_string(), e.to_string());
        std::fs::create_dir_all(&out).map_err(io)?;
        let snapshot = self.client.snapshot().map_err(|e| gone(&e))?;
        let json_path = out.join(format!("{file}.json"));
        let text = serde_json::to_string_pretty(&snapshot).unwrap_or_default();
        std::fs::write(&json_path, text).map_err(io)?;
        self.shots.push(json_path);
        let Some((capability, args)) = self.runner.shot.clone() else {
            return Ok(());
        };
        let png = out.join(format!("{file}.png"));
        let _ = std::fs::remove_file(&png);
        let path = png.to_string_lossy().into_owned();
        let args = self
            .runner
            .substitute_with(&args, &[("path", &path), ("name", name)]);
        self.call(&capability, &args)?;
        // The picture is saved once the application draws its next frame.
        let end = Instant::now() + self.runner.deadline;
        while !png.exists() {
            if Instant::now() >= end {
                return Err((
                    "shot".into(),
                    format!("no picture at {path} within {:?}", self.runner.deadline),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.shots.push(png);
        Ok(())
    }

    /// The invariants after a step: the first broken, by name.
    fn invariants(&mut self, before: u64) -> Result<(), StepError> {
        let allow = &self.scenario.allow;
        let check = |name: &str, r: Result<(), String>| -> Result<(), StepError> {
            match r {
                Err(m) if !allow.iter().any(|a| a == name) => Err((format!("invariant {name}"), m)),
                _ => Ok(()),
            }
        };
        let (events, snapshot) = match (self.client.events_since(before), self.client.snapshot()) {
            (Ok(e), Ok(s)) => (e, s),
            (Err(e), _) | (_, Err(e)) => return check(NO_PANIC, Err(e.to_string())),
        };
        let errors = error_toasts(&snapshot);
        check(
            NO_ERROR_TOAST,
            if errors.is_empty() {
                Ok(())
            } else {
                Err(format!("error toasts: {errors:?}"))
            },
        )?;
        let failed: Vec<String> = events
            .iter()
            .filter(|e| e["name"] == "command.failed")
            .map(|e| e["data"].to_string())
            .collect();
        check(
            NO_FAILED_COMMAND,
            if failed.is_empty() {
                Ok(())
            } else {
                Err(format!("command.failed: {failed:?}"))
            },
        )?;
        let missing: Vec<String> = missing_text(&snapshot)
            .into_iter()
            .filter(|k| !self.missing_before.contains(k))
            .collect();
        check(
            NO_MISSING_TEXT,
            if missing.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "keys shown untranslated during the step: {missing:?}"
                ))
            },
        )
    }

    fn fail(
        &mut self,
        step: Option<usize>,
        text: &str,
        broken: &str,
        message: String,
    ) -> Box<Failure> {
        let journal = self
            .client
            .events_since(self.start)
            .unwrap_or_default()
            .iter()
            .rev()
            .take(12)
            .rev()
            .map(|e| {
                format!(
                    "#{} {} [{}] {}",
                    e["seq"],
                    e["name"].as_str().unwrap_or("?"),
                    e["source"],
                    e["data"]
                )
            })
            .collect();
        Box::new(Failure {
            scenario: self.scenario.name.clone(),
            step,
            step_text: text.into(),
            broken: broken.into(),
            message,
            journal,
            tree: String::new(),
            snapshot: None,
        })
    }
}
