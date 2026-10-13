//! A fake host that scripts jobs: a test publishes `job.*` events as a
//! kernel would ([`FakeHost::job`]), without threads or sleeps, and the
//! host answers the job commands (`job.cancel`, `job.pause`,
//! `job.resume`, `job.retry`) now, later or not at all ([`Answer`]).
//!
//! The host is cheap to clone: give one clone to the application
//! (`AppBuilder::host`), keep the other to script.
//!
//! ```
//! use egui_pop_api::App;
//! use egui_pop_api::jobs::{JOBS, JobsPlugin};
//! use egui_pop_test::jobs::FakeHost;
//!
//! let host = FakeHost::new();
//! let mut app = App::builder()
//!     .host(host.clone())
//!     .plugin(JobsPlugin::default())
//!     .build(&egui::Context::default())
//!     .unwrap();
//! let export = host.job("export");
//! export.start("Export", Some(4)).progress(1);
//! app.logic(&egui::Context::default());
//! let jobs = app.handle().service(&JOBS).unwrap();
//! assert_eq!(jobs.job("export").unwrap().fraction, Some(0.25));
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use egui_pop_api::host::{self, Dispatch, Event, Host, Journal, Reply, Resolver, Seq, Waker};
use egui_pop_core::binding::SharedStore;
use egui_pop_model::jobs::{self as model, Request};
use serde_json::{Value as Json, json};

/// How the fake host answers a job command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Answer {
    /// At once: the command succeeds and the job's event follows
    /// (`job.cancelled`, `job.paused`, `job.resumed`, `job.started`).
    #[default]
    Now,
    /// Later: the command waits until [`FakeHost::release`].
    Later,
    /// Never: the command fails with this reason.
    Refuse(String),
}

struct Waiting {
    request: Request,
    id: String,
    resolver: Resolver,
}

#[derive(Default)]
struct Inner {
    journal: Journal,
    store: SharedStore,
    waker: Waker,
    answer: Answer,
    /// The job commands asked, in order: command and arguments.
    asked: Vec<(String, Json)>,
    waiting: Vec<Waiting>,
    /// Each job's `job.started` data, for a retry.
    started: BTreeMap<String, Json>,
}

/// A host whose jobs a test scripts.
#[derive(Clone, Default)]
pub struct FakeHost(Rc<RefCell<Inner>>);

impl fmt::Debug for FakeHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let i = self.0.borrow();
        f.debug_struct("FakeHost")
            .field("events", &i.journal.last())
            .field("answer", &i.answer)
            .field("asked", &i.asked)
            .field("waiting", &i.waiting.len())
            .finish()
    }
}

/// The event that answers a request.
fn answer_event(request: Request) -> &'static str {
    match request {
        Request::Cancel => model::CANCELLED,
        Request::Pause => model::PAUSED,
        Request::Resume => model::RESUMED,
        Request::Retry => model::STARTED,
    }
}

impl FakeHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// How the job commands are answered from now on.
    pub fn answer(&self, answer: Answer) {
        self.0.borrow_mut().answer = answer;
    }

    /// Publishes an event as the host, and wakes the interface.
    pub fn emit(&self, name: &str, data: Json) -> Seq {
        let (seq, waker) = {
            let mut i = self.0.borrow_mut();
            if name == model::STARTED
                && let Some(id) = data["id"].as_str()
            {
                i.started.insert(id.into(), data.clone());
            }
            (i.journal.publish("host", None, name, data), i.waker.clone())
        };
        waker.wake();
        seq
    }

    /// A job to script.
    pub fn job(&self, id: &str) -> JobScript {
        JobScript {
            host: self.clone(),
            id: id.into(),
        }
    }

    /// The job commands asked so far: command and arguments.
    pub fn asked(&self) -> Vec<(String, Json)> {
        self.0.borrow().asked.clone()
    }

    /// How many commands wait for [`FakeHost::release`].
    pub fn waiting(&self) -> usize {
        self.0.borrow().waiting.len()
    }

    /// Answers the waiting commands (they succeed, their jobs' events
    /// follow); how many.
    pub fn release(&self) -> usize {
        let waiting = std::mem::take(&mut self.0.borrow_mut().waiting);
        let n = waiting.len();
        for w in waiting {
            self.follow(w.request, &w.id);
            w.resolver.resolve(Ok(json!({ "id": w.id })));
        }
        self.0.borrow().waker.wake();
        n
    }

    /// Refuses the waiting commands with a reason; how many.
    pub fn refuse_waiting(&self, why: &str) -> usize {
        let waiting = std::mem::take(&mut self.0.borrow_mut().waiting);
        let n = waiting.len();
        for w in waiting {
            w.resolver.resolve(Err(why.into()));
        }
        self.0.borrow().waker.wake();
        n
    }

    /// The event that follows an accepted request.
    fn follow(&self, request: Request, id: &str) {
        let data = match request {
            Request::Retry => self
                .0
                .borrow()
                .started
                .get(id)
                .cloned()
                .unwrap_or_else(|| json!({ "id": id })),
            _ => json!({ "id": id }),
        };
        self.emit(answer_event(request), data);
    }

    /// The events published so far (the journal's).
    pub fn events(&self) -> Vec<Event> {
        self.0.borrow().journal.since(0)
    }
}

impl Host for FakeHost {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        if let Some(r) = d.run_local() {
            return Reply::Done(r);
        }
        let Some(request) = Request::from_command(d.command.as_str()) else {
            return Reply::Done(Err(format!(
                "no handler for `{}` in the fake host",
                d.command
            )));
        };
        let Some(id) = d.args["id"].as_str().map(String::from) else {
            return Reply::Done(Err("`id` is missing".into()));
        };
        let answer = {
            let mut i = self.0.borrow_mut();
            i.asked.push((d.command.to_string(), d.args.clone()));
            i.answer.clone()
        };
        match answer {
            Answer::Now => {
                self.follow(request, &id);
                Reply::Done(Ok(json!({ "id": id })))
            }
            Answer::Later => {
                let (pending, resolver) = host::pending();
                self.0.borrow_mut().waiting.push(Waiting {
                    request,
                    id,
                    resolver,
                });
                Reply::Later(pending)
            }
            Answer::Refuse(why) => Reply::Done(Err(why)),
        }
    }

    fn publish(&mut self, source: &str, cause: Option<Seq>, name: &str, data: Json) -> Option<Seq> {
        Some(
            self.0
                .borrow_mut()
                .journal
                .publish(source, cause, name, data),
        )
    }

    fn since(&self, seq: Seq) -> Option<Vec<Event>> {
        Some(self.0.borrow().journal.since(seq))
    }

    fn last_seq(&self) -> Option<Seq> {
        Some(self.0.borrow().journal.last())
    }

    fn store(&self) -> SharedStore {
        self.0.borrow().store.clone()
    }

    fn set_waker(&mut self, waker: Waker) {
        self.0.borrow_mut().waker = waker;
    }
}

/// One job of a [`FakeHost`], scripted event by event.
#[derive(Clone, Debug)]
pub struct JobScript {
    host: FakeHost,
    id: String,
}

impl JobScript {
    /// Publishes `name` for this job, `data` merged with its id.
    pub fn event(&self, name: &str, data: Json) -> &Self {
        let mut data = match data {
            Json::Object(m) => Json::Object(m),
            _ => json!({}),
        };
        data["id"] = json!(self.id);
        self.host.emit(name, data);
        self
    }

    /// Starts it: a title, a total (none: indeterminate).
    pub fn start(&self, title: &str, total: Option<u64>) -> &Self {
        self.event(model::STARTED, json!({ "title": title, "total": total }))
    }

    /// Starts it with the whole `job.started` data (steps, parent...).
    pub fn start_with(&self, data: Json) -> &Self {
        self.event(model::STARTED, data)
    }

    /// `done` items so far.
    pub fn progress(&self, done: u64) -> &Self {
        self.event(model::PROGRESS, json!({ "done": done }))
    }

    /// `done` items so far, working on `item`.
    pub fn item(&self, done: u64, item: &str) -> &Self {
        self.event(model::PROGRESS, json!({ "done": done, "item": item }))
    }

    /// The current step and how far it is.
    pub fn step(&self, step: &str, done: u64, total: Option<u64>) -> &Self {
        self.event(
            model::PROGRESS,
            json!({ "step": step, "step_done": done, "step_total": total }),
        )
    }

    /// Lines of its log.
    pub fn log(&self, text: &str) -> &Self {
        self.event(model::LOG, json!({ "text": text }))
    }

    pub fn item_failed(&self, item: &str, error: &str) -> &Self {
        self.event(model::ITEM_FAILED, json!({ "item": item, "error": error }))
    }

    pub fn pause(&self) -> &Self {
        self.event(model::PAUSED, Json::Null)
    }

    pub fn resume(&self) -> &Self {
        self.event(model::RESUMED, Json::Null)
    }

    pub fn fail(&self, error: &str) -> &Self {
        self.event(model::FAILED, json!({ "error": error }))
    }

    pub fn finish(&self) -> &Self {
        self.event(model::FINISHED, Json::Null)
    }

    pub fn cancelled(&self) -> &Self {
        self.event(model::CANCELLED, Json::Null)
    }
}
