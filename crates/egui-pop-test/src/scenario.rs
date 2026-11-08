//! Scenarios: tests written as data, replayed headless with invariants
//! checked after every step (see `docs/scenarios.md`).
//!
//! A scenario is a RON file of steps. Each step calls a capability that
//! `bridge.describe` lists (a collector action or a command), waits,
//! acts on the interface through egui_kittest, checks an expectation, or
//! keeps a shot. After every step the runner checks the invariants
//! (no error toast, no failed command...), unless the scenario lifts one
//! by name (`allow: [..]`). A failure names the step, what broke, the
//! journal's last events and the accessibility tree.
//!
//! ```no_run
//! use egui_pop_api::{App, DefaultPlugins};
//!
//! egui_pop_test::scenario::run_dir(
//!     || App::builder().plugins(DefaultPlugins),
//!     "tests/scenarios",
//! );
//! ```

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Key, Modifiers, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable as _};
use egui_pop_api::host::{Event, Seq};
use egui_pop_api::wait::WaitFor;
use egui_pop_api::{App, AppBuilder};
use serde::Deserialize;
use serde_json::{Value as Json, json};

use crate::wait::{Deadline, wait_until};

#[cfg(unix)]
pub mod live;

/// A scenario, as read from its RON file.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Its name, in reports and shots.
    pub name: String,
    /// What it shows, for the reader.
    #[serde(default)]
    pub description: String,
    /// `test.*` capabilities called before the steps, to set up the
    /// world (an application registers them in its test builds).
    #[serde(default)]
    pub fixtures: Vec<Fixture>,
    /// Invariants lifted for this scenario, by name (`NoErrorToast`).
    #[serde(default)]
    pub allow: Vec<String>,
    /// What it does, in order.
    pub steps: Vec<Step>,
}

/// A fixture: a capability's name and its arguments.
#[derive(Clone, Debug, Deserialize)]
pub struct Fixture(pub String, pub Json);

/// One step of a scenario.
#[derive(Clone, Debug, Deserialize)]
pub enum Step {
    /// Calls a capability `bridge.describe` lists, with its arguments:
    /// a collector action (`journal.since`) or a command (`about.read`),
    /// as the agent.
    Call(String, Json),
    /// Steps frames until the condition holds, for the events since the
    /// last `Call` or `Ui` step began.
    Wait(Wait),
    /// Acts on the interface, as a person would.
    Ui(Ui),
    /// Checks something.
    Expect(Expect),
    /// Keeps a shot: a snapshot image when the runner renders
    /// (`Runner::snapshots`, feature `wgpu`), the accessibility tree in the report
    /// otherwise.
    Shot(String),
}

/// What a `Wait` step waits for.
#[derive(Clone, Debug, Deserialize)]
pub enum Wait {
    /// No command queued or running, every job idle, no repaint asked.
    Settled,
    /// A journal event by name.
    Event(String),
    /// The first of several.
    Any(Vec<Wait>),
}

impl Wait {
    fn to_wait_for(&self) -> WaitFor {
        match self {
            Wait::Settled => WaitFor::Settled,
            Wait::Event(name) => WaitFor::event(name),
            Wait::Any(any) => WaitFor::Any(any.iter().map(Wait::to_wait_for).collect()),
        }
    }
}

/// A node of the accessibility tree, by its label, its role
/// (`Button`, `Window`, `CheckBox`..., any case), or both.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Node {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.label, &self.role) {
            (Some(l), Some(r)) => write!(f, "{r} \"{l}\""),
            (Some(l), None) => write!(f, "\"{l}\""),
            (None, Some(r)) => write!(f, "{r}"),
            (None, None) => write!(f, "any node"),
        }
    }
}

/// An action on the interface, through egui_kittest.
#[derive(Clone, Debug, Deserialize)]
pub enum Ui {
    /// Clicks a node.
    Click {
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        role: Option<String>,
    },
    /// Focuses a node and types text into it.
    Type {
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        role: Option<String>,
        text: String,
    },
    /// Presses a key, with modifiers: `Escape`, `Ctrl+K`, `Shift+Tab`.
    Key(String),
    /// Scrolls a node, down unless `up`.
    Scroll {
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        up: bool,
    },
}

/// A check.
#[derive(Clone, Debug, Deserialize)]
pub enum Expect {
    /// No error toast on screen and no failed command since the start,
    /// whatever `allow` lifts.
    NoError,
    /// The tree has a node (`has`), and lacks one (`lacks`).
    Tree {
        #[serde(default)]
        has: Option<Node>,
        #[serde(default)]
        lacks: Option<Node>,
    },
    /// A collector's snapshot value: `collector.key.0.key` (the
    /// collector's name may hold dots), compared for equality.
    Value(String, Json),
    /// A journal event since the start named `has`, its data holding
    /// `data` (every key given, compared for equality).
    Journal {
        has: String,
        #[serde(default)]
        data: Option<Json>,
    },
}

/// The invariants the runner checks after every step.
pub mod invariants {
    /// An error toast shows.
    pub const NO_ERROR_TOAST: &str = "NoErrorToast";
    /// `command.failed` is on the journal.
    pub const NO_FAILED_COMMAND: &str = "NoFailedCommand";
    /// A capability or command named that is not registered.
    pub const NO_UNKNOWN_COMMAND: &str = "NoUnknownCommand";
    /// A step panicked.
    pub const NO_PANIC: &str = "NoPanic";
    /// A window or an area moves or resizes once the application is
    /// settled.
    pub const STABLE_WINDOWS: &str = "StableWindows";
    /// A frame took longer than the budget.
    pub const FRAME_BUDGET: &str = "FrameBudget";
    /// An i18n key showed untranslated.
    pub const NO_MISSING_TEXT: &str = "NoMissingText";
    /// Every built-in invariant.
    pub const ALL: [&str; 7] = [
        NO_ERROR_TOAST,
        NO_FAILED_COMMAND,
        NO_UNKNOWN_COMMAND,
        NO_PANIC,
        STABLE_WINDOWS,
        FRAME_BUDGET,
        NO_MISSING_TEXT,
    ];
}
use invariants::*;

/// What an application's invariant reads: the harness, the step just
/// run and the journal's events during it.
pub struct Check<'a> {
    pub harness: &'a Harness<'static, App>,
    pub step: &'a Step,
    pub events: &'a [Event],
}

impl fmt::Debug for Check<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Check")
            .field("step", &self.step)
            .field("events", &self.events)
            .finish()
    }
}

type InvariantFn = Box<dyn Fn(&Check<'_>) -> Result<(), String>>;

/// Why a scenario failed, with what a reader needs to see why.
#[derive(Clone, Debug)]
pub struct Failure {
    /// The scenario's name (or its file, when it does not load).
    pub scenario: String,
    /// The step (from 1), `None` for loading or a fixture.
    pub step: Option<usize>,
    /// The step, as written.
    pub step_text: String,
    /// What broke: `invariant NoFailedCommand`, `expectation`, `call`,
    /// `ui`, `wait`, `fixture`, `load`.
    pub broken: String,
    /// Why.
    pub message: String,
    /// The journal's last events.
    pub journal: Vec<String>,
    /// The accessibility tree at the failure.
    pub tree: String,
    /// The snapshot image kept at the failure, if any.
    pub snapshot: Option<PathBuf>,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "scenario `{}` failed", self.scenario)?;
        match self.step {
            Some(n) => writeln!(f, "  step {n}: {}", self.step_text)?,
            None if !self.step_text.is_empty() => writeln!(f, "  at: {}", self.step_text)?,
            None => {}
        }
        writeln!(f, "  broken: {}", self.broken)?;
        writeln!(f, "  why: {}", self.message)?;
        if let Some(p) = &self.snapshot {
            writeln!(f, "  snapshot: {}", p.display())?;
        }
        if !self.journal.is_empty() {
            writeln!(f, "  journal (last {}):", self.journal.len())?;
            for e in &self.journal {
                writeln!(f, "    {e}")?;
            }
        }
        if !self.tree.is_empty() {
            writeln!(f, "  tree:")?;
            for l in self.tree.lines() {
                writeln!(f, "    {l}")?;
            }
        }
        Ok(())
    }
}

/// What a passed scenario did.
#[derive(Clone, Debug, PartialEq)]
pub struct Passed {
    pub scenario: String,
    pub steps: usize,
    /// The longest frame.
    pub slowest_frame: Duration,
    /// The shots kept as trees (no renderer), by name.
    pub trees: Vec<(String, String)>,
}

/// Runs scenarios against a fresh application each.
pub struct Runner {
    build: Box<dyn Fn() -> AppBuilder>,
    size: Vec2,
    budget: Duration,
    deadline: Deadline,
    invariants: Vec<(String, InvariantFn)>,
    snapshots: bool,
}

impl fmt::Debug for Runner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runner")
            .field("size", &self.size)
            .field("budget", &self.budget)
            .field("deadline", &self.deadline)
            .field(
                "invariants",
                &self.invariants.iter().map(|(n, _)| n).collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// Runs every `*.ron` scenario of `dir` (not its subfolders), each on a
/// fresh application from `build`; panics with every failure's report.
pub fn run_dir(build: impl Fn() -> AppBuilder + 'static, dir: impl AsRef<Path>) -> Vec<Passed> {
    Runner::new(build).assert_dir(dir)
}

impl Runner {
    /// A runner building each scenario's application with `build`, in a
    /// 1100 × 720 window, a frame budget of 2 seconds (generous: a guard
    /// against loops, in debug builds), and waits of
    /// [`Deadline::default`].
    pub fn new(build: impl Fn() -> AppBuilder + 'static) -> Self {
        Self {
            build: Box::new(build),
            size: egui::vec2(1100.0, 720.0),
            budget: Duration::from_secs(2),
            deadline: Deadline::default(),
            invariants: Vec::new(),
            snapshots: false,
        }
    }

    /// The window's size.
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = size;
        self
    }

    /// The longest a frame may take (`FrameBudget`).
    pub fn frame_budget(mut self, budget: Duration) -> Self {
        self.budget = budget;
        self
    }

    /// How long a `Wait` step waits.
    pub fn deadline(mut self, deadline: Deadline) -> Self {
        self.deadline = deadline;
        self
    }

    /// Adds an application's invariant, checked after every step unless
    /// a scenario allows `name`.
    pub fn invariant(
        mut self,
        name: &str,
        check: impl Fn(&Check<'_>) -> Result<(), String> + 'static,
    ) -> Self {
        self.invariants.push((name.into(), Box::new(check)));
        self
    }

    /// Renders with wgpu, so a `Shot` is a snapshot image compared like
    /// the kittest snapshots (feature `wgpu`).
    #[cfg(feature = "wgpu")]
    pub fn snapshots(mut self, on: bool) -> Self {
        self.snapshots = on;
        self
    }

    /// Reads a scenario file.
    pub fn load(path: impl AsRef<Path>) -> Result<Scenario, Box<Failure>> {
        let path = path.as_ref();
        let fail = |message: String| Failure {
            scenario: path.display().to_string(),
            step: None,
            step_text: String::new(),
            broken: "load".into(),
            message,
            journal: Vec::new(),
            tree: String::new(),
            snapshot: None,
        };
        let text = std::fs::read_to_string(path).map_err(|e| Box::new(fail(e.to_string())))?;
        parse(&text).map_err(|e| Box::new(fail(e)))
    }

    /// Runs a scenario file.
    pub fn run_file(&self, path: impl AsRef<Path>) -> Result<Passed, Box<Failure>> {
        self.run(&Self::load(path)?)
    }

    /// Runs every `*.ron` file of `dir`, in name order.
    pub fn run_dir(&self, dir: impl AsRef<Path>) -> Vec<(PathBuf, Result<Passed, Box<Failure>>)> {
        let dir = dir.as_ref();
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "ron"))
            .collect();
        files.sort();
        files
            .into_iter()
            .map(|p| {
                let r = self.run_file(&p);
                (p, r)
            })
            .collect()
    }

    /// [`Runner::run_dir`], panicking with every failure's report; at
    /// least one scenario must be there.
    pub fn assert_dir(&self, dir: impl AsRef<Path>) -> Vec<Passed> {
        let dir = dir.as_ref();
        let results = self.run_dir(dir);
        assert!(!results.is_empty(), "no scenario in {}", dir.display());
        let mut passed = Vec::new();
        let mut failed = Vec::new();
        for (p, r) in results {
            match r {
                Ok(ok) => passed.push(ok),
                Err(f) => failed.push(format!("{}:\n{f}", p.display())),
            }
        }
        assert!(
            failed.is_empty(),
            "{} of {} scenarios failed\n\n{}",
            failed.len(),
            failed.len() + passed.len(),
            failed.join("\n")
        );
        passed
    }

    /// Runs one scenario on a fresh application.
    pub fn run(&self, scenario: &Scenario) -> Result<Passed, Box<Failure>> {
        let unknown: Vec<&String> = scenario
            .allow
            .iter()
            .filter(|a| !ALL.contains(&a.as_str()) && !self.invariants.iter().any(|(n, _)| n == *a))
            .collect();
        let builder = (self.build)();
        let mut h = self.harness(builder);
        let mut run = Run {
            runner: self,
            scenario,
            start: h.state().handle().last_event(),
            mark: 0,
            slowest: Duration::ZERO,
            trees: Vec::new(),
            missing_before: Vec::new(),
            stable: None,
        };
        run.mark = run.start;
        if !unknown.is_empty() {
            return Err(run.fail(
                &h,
                None,
                "allow",
                "load",
                format!("unknown invariants {unknown:?}; there are {ALL:?} and the runner's"),
            ));
        }
        run.frames(&mut h);
        // Keys already untranslated at the start are the application's
        // state, not the scenario's finding (the `i18n` collector lists
        // them).
        run.missing_before = missing_text(&h.state().handle().debug_snapshot());
        for Fixture(name, args) in &scenario.fixtures {
            let text = format!("Fixture({name:?}, {args})");
            if !name.starts_with("test.") {
                return Err(run.fail(
                    &h,
                    None,
                    &text,
                    "fixture",
                    "a fixture is a `test.*` capability".into(),
                ));
            }
            if let Err((broken, message)) = run.call(&mut h, name, args) {
                return Err(run.fail(&h, None, &text, &broken, message));
            }
        }
        for (i, step) in scenario.steps.iter().enumerate() {
            let before = h.state().handle().last_event();
            let text = step_text(step);
            let done = catch_unwind(AssertUnwindSafe(|| run.step(&mut h, step)));
            let outcome = match done {
                Ok(r) => r,
                Err(panic) => {
                    let why = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "a panic".into());
                    if scenario.allow.iter().any(|a| a == NO_PANIC) {
                        Ok(())
                    } else {
                        Err((format!("invariant {NO_PANIC}"), why))
                    }
                }
            };
            if let Err((broken, message)) = outcome {
                return Err(run.fail(&h, Some(i + 1), &text, &broken, message));
            }
            if let Err((name, message)) = run.invariants(&h, step, before) {
                return Err(run.fail(
                    &h,
                    Some(i + 1),
                    &text,
                    &format!("invariant {name}"),
                    message,
                ));
            }
        }
        Ok(Passed {
            scenario: scenario.name.clone(),
            steps: scenario.steps.len(),
            slowest_frame: run.slowest,
            trees: run.trees,
        })
    }

    fn harness(&self, builder: AppBuilder) -> Harness<'static, App> {
        #[cfg(feature = "wgpu")]
        if self.snapshots {
            return crate::app_wgpu(builder, self.size);
        }
        let _ = self.snapshots;
        crate::app(builder, self.size)
    }
}

/// Reads a scenario from RON text (`Option` fields written bare).
pub fn parse(text: &str) -> Result<Scenario, String> {
    ron::Options::default()
        .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
        .from_str(text)
        .map_err(|e| e.to_string())
}

/// One scenario's run.
struct Run<'r> {
    runner: &'r Runner,
    scenario: &'r Scenario,
    /// The journal's last event before the scenario.
    start: Seq,
    /// The journal's last event before the last `Call` or `Ui` step.
    mark: Seq,
    slowest: Duration,
    trees: Vec<(String, String)>,
    /// The areas that moved in the frames after the last settle.
    stable: Option<Vec<String>>,
    /// The keys untranslated before the first step, `locale: key`.
    missing_before: Vec<String>,
}

type StepError = (String, String);

impl Run<'_> {
    fn allowed(&self, name: &str) -> bool {
        self.scenario.allow.iter().any(|a| a == name)
    }

    /// Steps frames until the application asks for no more (at most
    /// kittest's `max_steps`), timing each.
    fn frames(&mut self, h: &mut Harness<'static, App>) {
        let t = Instant::now();
        let n = h.try_run().unwrap_or(4).max(1);
        let per = t.elapsed() / n as u32;
        self.slowest = self.slowest.max(per);
    }

    fn call(
        &mut self,
        h: &mut Harness<'static, App>,
        name: &str,
        args: &Json,
    ) -> Result<(), StepError> {
        let caps = h.state().handle().debug_capabilities();
        let cap = caps["capabilities"]
            .as_array()
            .and_then(|c| c.iter().find(|c| c["name"] == name))
            .cloned();
        let Some(cap) = cap else {
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
            let known = caps["capabilities"]
                .as_array()
                .is_some_and(|c| c.iter().any(|c| c["kind"] == "command" && c["name"] == id));
            if !known && !self.allowed(NO_UNKNOWN_COMMAND) {
                return Err((
                    format!("invariant {NO_UNKNOWN_COMMAND}"),
                    format!("no command `{id}`"),
                ));
            }
        }
        let before = h.state().handle().last_event();
        let f = if cap["kind"] == "command" {
            h.state_mut()
                .debug_call("commands", "run", &json!({ "id": name, "args": args }))
        } else {
            let c = cap["call"]["collector"].as_str().unwrap_or_default();
            let a = cap["call"]["action"].as_str().unwrap_or_default();
            h.state_mut().debug_call(c, a, args)
        };
        self.frames(h);
        if !f.ok {
            let failed = h
                .state()
                .handle()
                .events_since(before)
                .iter()
                .any(|e| e.name == "command.failed");
            // A command that ran and failed is NoFailedCommand's.
            if !failed {
                return Err(("call".into(), f.message));
            }
        }
        Ok(())
    }

    fn step(&mut self, h: &mut Harness<'static, App>, step: &Step) -> Result<(), StepError> {
        match step {
            Step::Call(name, args) => {
                self.mark = h.state().handle().last_event();
                self.call(h, name, args)
            }
            Step::Wait(w) => {
                let until = w.to_wait_for();
                wait_until(h, &until, self.mark, self.runner.deadline)
                    .map_err(|e| ("wait".to_string(), e))?;
                if matches!(w, Wait::Settled) {
                    // Settled, the windows stay put for a few frames more.
                    let settled = areas(h);
                    for _ in 0..3 {
                        h.step();
                    }
                    let now = areas(h);
                    self.stable = Some(
                        settled
                            .iter()
                            .filter_map(|(id, r)| {
                                let n = now.iter().find(|(i, _)| i == id)?.1;
                                (n != *r).then(|| format!("{:?}: {r:?} -> {n:?}", id.id))
                            })
                            .collect(),
                    );
                }
                Ok(())
            }
            Step::Ui(ui) => {
                self.mark = h.state().handle().last_event();
                self.ui(h, ui).map_err(|e| ("ui".to_string(), e))?;
                self.frames(h);
                Ok(())
            }
            Step::Expect(e) => self
                .expect(h, e)
                .map_err(|m| ("expectation".to_string(), m)),
            Step::Shot(name) => {
                let name = format!("{}-{name}", self.scenario.name);
                #[cfg(feature = "wgpu")]
                if self.runner.snapshots {
                    return h
                        .try_snapshot(&name)
                        .map_err(|e| ("shot".to_string(), e.to_string()));
                }
                self.trees.push((name, crate::a11y_tree(h)));
                Ok(())
            }
        }
    }

    fn ui(&mut self, h: &mut Harness<'static, App>, ui: &Ui) -> Result<(), String> {
        match ui {
            Ui::Click { label, role } => {
                let node = Node {
                    label: label.clone(),
                    role: role.clone(),
                };
                find(h, &node)?.click();
            }
            Ui::Type { label, role, text } => {
                let node = Node {
                    label: label.clone(),
                    role: role.clone(),
                };
                find(h, &node)?.focus();
                h.run_ok();
                find(h, &node)?.type_text(text);
            }
            Ui::Key(chord) => {
                let (mods, key) = parse_chord(chord)?;
                h.key_press_modifiers(mods, key);
            }
            Ui::Scroll { label, role, up } => {
                let node = Node {
                    label: label.clone(),
                    role: role.clone(),
                };
                let n = find(h, &node)?;
                if *up {
                    n.scroll_up();
                } else {
                    n.scroll_down();
                }
            }
        }
        Ok(())
    }

    fn expect(&self, h: &Harness<'static, App>, e: &Expect) -> Result<(), String> {
        let handle = h.state().handle();
        match e {
            Expect::NoError => {
                let errors = error_toasts(&handle.debug_snapshot());
                if !errors.is_empty() {
                    return Err(format!("error toasts: {errors:?}"));
                }
                let failed: Vec<String> = handle
                    .events_since(self.start)
                    .iter()
                    .filter(|e| e.name == "command.failed")
                    .map(|e| e.data.to_string())
                    .collect();
                if !failed.is_empty() {
                    return Err(format!("failed commands: {failed:?}"));
                }
                Ok(())
            }
            Expect::Tree { has, lacks } => {
                if let Some(n) = has
                    && count(h, n) == 0
                {
                    return Err(format!("the tree has no {n}"));
                }
                if let Some(n) = lacks
                    && count(h, n) > 0
                {
                    return Err(format!("the tree has {n}"));
                }
                Ok(())
            }
            Expect::Value(path, want) => {
                let got = value_at(&handle.debug_snapshot(), path)?;
                if got == *want {
                    Ok(())
                } else {
                    Err(format!("`{path}` is {got}, not {want}"))
                }
            }
            Expect::Journal { has, data } => {
                let events = handle.events_since(self.start);
                let hit = events
                    .iter()
                    .any(|e| e.name == *has && data.as_ref().is_none_or(|d| holds(&e.data, d)));
                if hit {
                    Ok(())
                } else {
                    let names: Vec<&str> = events.iter().map(|e| e.name.as_str()).collect();
                    Err(match data {
                        Some(d) => {
                            format!("no `{has}` holding {d} on the journal; it has {names:?}")
                        }
                        None => format!("no `{has}` on the journal; it has {names:?}"),
                    })
                }
            }
        }
    }

    /// The invariants after a step: the first broken, by name.
    fn invariants(
        &mut self,
        h: &Harness<'static, App>,
        step: &Step,
        before: Seq,
    ) -> Result<(), (String, String)> {
        let handle = h.state().handle();
        let events = handle.events_since(before);
        let snapshot = handle.debug_snapshot();
        let allow = &self.scenario.allow;
        let check = |name: &str, r: Result<(), String>| -> Result<(), (String, String)> {
            match r {
                Err(m) if !allow.iter().any(|a| a == name) => Err((name.to_string(), m)),
                _ => Ok(()),
            }
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
            .filter(|e| e.name == "command.failed")
            .map(|e| e.data.to_string())
            .collect();
        check(
            NO_FAILED_COMMAND,
            if failed.is_empty() {
                Ok(())
            } else {
                Err(format!("command.failed: {failed:?}"))
            },
        )?;
        check(
            FRAME_BUDGET,
            if self.slowest <= self.runner.budget {
                Ok(())
            } else {
                Err(format!(
                    "a frame took {:?}, over the budget of {:?}",
                    self.slowest, self.runner.budget
                ))
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
        )?;
        if let Some(moved) = self.stable.take() {
            check(
                STABLE_WINDOWS,
                if moved.is_empty() {
                    Ok(())
                } else {
                    Err(format!("areas moved once settled: {moved:?}"))
                },
            )?;
        }
        for (name, f) in &self.runner.invariants {
            let c = Check {
                harness: h,
                step,
                events: &events,
            };
            check(name, f(&c))?;
        }
        Ok(())
    }

    fn fail(
        &self,
        h: &Harness<'static, App>,
        step: Option<usize>,
        text: &str,
        broken: &str,
        message: String,
    ) -> Box<Failure> {
        let handle = h.state().handle();
        let events = handle.events_since(self.start);
        let journal = events
            .iter()
            .rev()
            .take(12)
            .rev()
            .map(|e| format!("#{} {} [{}] {}", e.seq, e.name, e.source, e.data))
            .collect();
        Box::new(Failure {
            scenario: self.scenario.name.clone(),
            step,
            step_text: text.into(),
            broken: broken.into(),
            message,
            journal,
            tree: crate::a11y_tree(h),
            snapshot: None,
        })
    }
}

/// A step as written, for reports.
fn step_text(step: &Step) -> String {
    match step {
        Step::Call(name, args) => format!("Call({name:?}, {args})"),
        other => format!("{other:?}"),
    }
}

fn by_node(n: &Node) -> By<'_> {
    let mut by = By::new();
    if let Some(l) = &n.label {
        by = by.label(l);
    }
    if let Some(r) = &n.role {
        let r = r.clone();
        by = by.predicate(move |node| format!("{:?}", node.role()).eq_ignore_ascii_case(&r));
    }
    by
}

fn count(h: &Harness<'static, App>, n: &Node) -> usize {
    h.query_all(by_node(n)).count()
}

fn find<'h>(h: &'h Harness<'static, App>, n: &'h Node) -> Result<egui_kittest::Node<'h>, String> {
    let mut all = h.query_all(by_node(n));
    let first = all.next().ok_or_else(|| format!("no {n} in the tree"))?;
    if all.next().is_some() {
        return Err(format!(
            "more than one {n} in the tree: name it more closely"
        ));
    }
    Ok(first)
}

/// `Ctrl+Shift+K` as modifiers and a key.
fn parse_chord(chord: &str) -> Result<(Modifiers, Key), String> {
    let mut mods = Modifiers::NONE;
    let parts: Vec<&str> = chord.split('+').map(str::trim).collect();
    let (key, held) = parts.split_last().ok_or("an empty key")?;
    for m in held {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods.ctrl = true,
            "shift" => mods.shift = true,
            "alt" | "option" => mods.alt = true,
            "cmd" | "command" => mods.command = true,
            "mac" | "meta" => mods.mac_cmd = true,
            other => return Err(format!("no modifier `{other}` in `{chord}`")),
        }
    }
    if mods.ctrl {
        mods.command = true;
    }
    let key = Key::from_name(key).ok_or_else(|| format!("no key `{key}` in `{chord}`"))?;
    Ok((mods, key))
}

/// The keys the `i18n` collector reports untranslated, `locale: key`,
/// English locales left out.
fn missing_text(snapshot: &Json) -> Vec<String> {
    snapshot["i18n"]["missing"]
        .as_object()
        .map(|m| {
            m.iter()
                // English is the source language: a key without an
                // English entry shows its English text.
                .filter(|(locale, _)| *locale != "en" && !locale.starts_with("en-"))
                .flat_map(|(locale, keys)| {
                    keys.as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|k| k.as_str())
                        .map(move |k| format!("{locale}: {k}"))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The error toasts on screen, by text.
fn error_toasts(snapshot: &Json) -> Vec<String> {
    snapshot["toasts"]["shown"]
        .as_array()
        .map(|s| {
            s.iter()
                .filter(|t| t["toast"]["level"] == "error" || t["level"] == "error")
                .map(|t| {
                    t["toast"]["text"]
                        .as_str()
                        .or(t["text"].as_str())
                        .unwrap_or_default()
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A snapshot's value at `collector.key.0.key`: the collector is the
/// longest prefix of the path naming one.
fn value_at(snapshot: &Json, path: &str) -> Result<Json, String> {
    let map = snapshot.as_object().ok_or("no snapshot")?;
    let parts: Vec<&str> = path.split('.').collect();
    let (collector, rest) = (1..=parts.len())
        .rev()
        .find_map(|n| {
            let name = parts[..n].join(".");
            map.get(&name).map(|v| (v, &parts[n..]))
        })
        .ok_or_else(|| format!("no collector for `{path}`"))?;
    let mut v = collector;
    for (i, key) in rest.iter().enumerate() {
        let next = match v {
            Json::Array(a) => key.parse::<usize>().ok().and_then(|n| a.get(n)),
            Json::Object(o) => o.get(*key),
            _ => None,
        };
        v = next.ok_or_else(|| {
            format!(
                "`{path}`: nothing at `{}`",
                parts[..parts.len() - rest.len() + i + 1].join(".")
            )
        })?;
    }
    Ok(v.clone())
}

/// Whether `data` holds every key of `want`, recursively.
fn holds(data: &Json, want: &Json) -> bool {
    match (data, want) {
        (Json::Object(d), Json::Object(w)) => w
            .iter()
            .all(|(k, wv)| d.get(k).is_some_and(|dv| holds(dv, wv))),
        _ => data == want,
    }
}

/// The visible areas (windows, popups, modals) and their rects.
fn areas(h: &Harness<'static, App>) -> Vec<(egui::LayerId, egui::Rect)> {
    h.ctx.memory(|m| {
        m.areas()
            .visible_layer_ids()
            .into_iter()
            .filter(|l| l.order != egui::Order::Background && l.order != egui::Order::Tooltip)
            .filter_map(|l| m.area_rect(l.id).map(|r| (l, r)))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scenario_reads_from_ron() {
        let s = parse(
            r#"Scenario(
                name: "about",
                fixtures: [Fixture("test.folder", { "jpeg": 3 })],
                allow: ["NoErrorToast"],
                steps: [
                    Call("about.read", ()),
                    Call("commands.run", { "id": "about.read", "args": null }),
                    Wait(Any([Event("command.done"), Settled])),
                    Ui(Click(label: "About")),
                    Ui(Type(role: "TextInput", text: "hello")),
                    Ui(Key("Ctrl+K")),
                    Ui(Scroll(label: "Pages", up: true)),
                    Expect(NoError),
                    Expect(Tree(has: (role: "Window", label: "About"))),
                    Expect(Value("journal.last", 3)),
                    Expect(Journal(has: "command.done", data: { "command": "about.read" })),
                    Shot("after"),
                ],
            )"#,
        )
        .unwrap();
        assert_eq!(s.steps.len(), 12);
        assert_eq!(s.fixtures[0].1, json!({ "jpeg": 3 }));
        assert!(matches!(&s.steps[0], Step::Call(n, Json::Null) if n == "about.read"));
        assert!(
            matches!(&s.steps[3], Step::Ui(Ui::Click { label: Some(l), role: None }) if l == "About")
        );
        assert!(matches!(
            &s.steps[8],
            Step::Expect(Expect::Tree {
                has: Some(Node {
                    role: Some(_),
                    label: Some(_)
                }),
                lacks: None
            })
        ));
    }

    #[test]
    fn a_value_path_finds_its_collector_by_the_longest_prefix() {
        let snap = json!({ "gallery.viewport": { "zoom": [1, 2] }, "journal": { "last": 4 } });
        assert_eq!(
            value_at(&snap, "gallery.viewport.zoom.1").unwrap(),
            json!(2)
        );
        assert_eq!(value_at(&snap, "journal.last").unwrap(), json!(4));
        assert!(
            value_at(&snap, "journal.first")
                .unwrap_err()
                .contains("journal.first")
        );
        assert!(value_at(&snap, "nope.x").is_err());
    }

    #[test]
    fn chords_parse() {
        assert_eq!(
            parse_chord("Escape").unwrap(),
            (Modifiers::NONE, Key::Escape)
        );
        let (m, k) = parse_chord("Ctrl+Shift+K").unwrap();
        assert!(m.ctrl && m.shift && m.command);
        assert_eq!(k, Key::K);
        assert!(parse_chord("Hyper+K").is_err());
    }

    #[test]
    fn data_holds_the_keys_wanted() {
        let d = json!({ "command": "a", "result": { "x": 1, "y": 2 } });
        assert!(holds(&d, &json!({ "result": { "x": 1 } })));
        assert!(!holds(&d, &json!({ "command": "b" })));
    }
}
