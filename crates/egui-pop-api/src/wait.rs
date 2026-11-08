//! Waiting on the application: until a journal event matches, or until
//! the application is settled. The `bridge.wait` capability and the test
//! kit's frame-stepping wait (`egui_pop_test::wait_until`) share it.
//!
//! The check never blocks: [`AppHandle::wait_met`] says whether the
//! condition holds now. Who waits holds the call and checks again after
//! each frame until it holds or a deadline passes: a kittest harness
//! steps frames, a transport re-checks on the UI thread. The UI thread
//! itself never sleeps on it.
//!
//! ```
//! use egui_pop_api::about::{AboutPlugin, AppInfo};
//! use egui_pop_api::{App, DefaultPlugins};
//! use egui_pop_api::wait::{WaitFor, Waited};
//! use serde_json::json;
//!
//! let mut app = App::builder()
//!     .plugins(DefaultPlugins)
//!     .plugin(AboutPlugin::new(AppInfo::new("Demo", "1.0.0")))
//!     .build(&egui::Context::default())
//!     .unwrap();
//! let since = app.handle().last_event();
//! let f = app.debug_call("commands", "run", &json!({ "id": "about.read" }));
//! assert!(f.ok, "{}", f.message);
//! let done = app.handle().wait_met(&WaitFor::event("command.done"), since);
//! assert!(matches!(done, Some(Waited::Event(e)) if e.data["command"] == "about.read"));
//! // The same through the bridge, as an agent asks it.
//! let f = app.debug_call(
//!     "bridge",
//!     "wait",
//!     &json!({ "event": "command.done", "since": since }),
//! );
//! assert_eq!(f.data["met"], true);
//! ```

use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::host::{Event, Seq};

/// What a wait waits for.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitFor {
    /// A journal event after the wait's start named `name` and, when
    /// given, following from `cause` (a command's dispatch).
    Event { name: String, cause: Option<Seq> },
    /// The application is settled: no command queued or running, every
    /// job idle, and no repaint asked for.
    Settled,
    /// The first of these that holds.
    Any(Vec<WaitFor>),
}

impl WaitFor {
    /// An event by name, whatever its cause.
    pub fn event(name: &str) -> Self {
        Self::Event {
            name: name.into(),
            cause: None,
        }
    }

    /// Reads `bridge.wait`'s arguments: `{ "event", "cause" }`,
    /// `{ "settled": true }` or `{ "any": [..] }`.
    pub fn from_args(args: &Json) -> Result<Self, String> {
        if let Some(any) = args.get("any") {
            let any = any.as_array().ok_or("`any` is not a list")?;
            return any
                .iter()
                .map(Self::from_args)
                .collect::<Result<Vec<_>, _>>()
                .map(Self::Any);
        }
        if let Some(name) = args.get("event") {
            let name = name.as_str().ok_or("`event` is not a name")?;
            let cause = match args.get("cause") {
                None | Some(Json::Null) => None,
                Some(c) => Some(c.as_u64().ok_or("`cause` is not a sequence number")?),
            };
            return Ok(Self::Event {
                name: name.into(),
                cause,
            });
        }
        if args.get("settled").and_then(Json::as_bool) == Some(true) {
            return Ok(Self::Settled);
        }
        Err("give `event` (with an optional `cause`), `settled: true` or `any`".into())
    }

    /// As [`WaitFor::from_args`] reads it.
    pub fn to_args(&self) -> Json {
        match self {
            Self::Event { name, cause } => json!({ "event": name, "cause": cause }),
            Self::Settled => json!({ "settled": true }),
            Self::Any(any) => json!({ "any": any.iter().map(Self::to_args).collect::<Vec<_>>() }),
        }
    }
}

/// What ended a wait.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Waited {
    /// This event matched.
    Event(Event),
    /// The application settled.
    Settled,
}

/// The JSON Schema of `bridge.wait`'s arguments.
pub fn args_schema() -> Json {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "description": "Wait for a journal event after `since` (`event`, optional `cause`), for the application to settle (`settled: true`), or for the first of several (`any`).",
        "properties": {
            "event": { "type": "string", "description": "The event's name (`command.done`)." },
            "cause": { "type": ["integer", "null"], "minimum": 0, "description": "The event it follows from." },
            "settled": { "type": "boolean" },
            "any": { "type": "array", "items": { "type": "object" } },
            "since": { "type": "integer", "minimum": 0, "description": "Only events after this sequence number; by default the call's own." },
            "deadline_ms": { "type": "integer", "minimum": 0, "description": "How long the transport holds the call; in-process the call answers at once." }
        }
    })
}

/// The JSON Schema of `bridge.wait`'s result.
pub fn result_schema() -> Json {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["met", "last"],
        "properties": {
            "met": { "type": "boolean" },
            "by": { "enum": ["event", "settled", null] },
            "event": { "type": ["object", "null"] },
            "last": { "type": "integer", "description": "The journal's last sequence number: the next check's `since`." }
        }
    })
}

impl AppHandle {
    /// Whether the application is settled: no command queued for the
    /// next frame or running (a host's later reply), every job idle,
    /// and no repaint asked for.
    pub fn settled(&self) -> bool {
        self.queued_requests() == 0
            && !self.commands().iter().any(|(s, _)| self.running(&s.id))
            && self.service(&crate::jobs::JOBS).is_none_or(|j| j.idle())
            && !self.egui().has_requested_repaint()
    }

    /// Whether `until` holds now, for events after `since`; what made
    /// it hold. Never blocks (see the [module](self)).
    pub fn wait_met(&self, until: &WaitFor, since: Seq) -> Option<Waited> {
        match until {
            WaitFor::Event { name, cause } => self
                .events_since(since)
                .into_iter()
                .find(|e| e.name == *name && cause.is_none_or(|c| e.cause == Some(c)))
                .map(Waited::Event),
            WaitFor::Settled => self.settled().then_some(Waited::Settled),
            WaitFor::Any(any) => any.iter().find_map(|w| self.wait_met(w, since)),
        }
    }

    /// `bridge.wait`'s answer: `{ "met", "by", "event", "last" }`.
    pub(crate) fn wait_answer(&self, args: &Json) -> Result<Json, String> {
        let until = WaitFor::from_args(args)?;
        let last = self.last_event();
        let since = args.get("since").and_then(Json::as_u64).unwrap_or(last);
        Ok(match self.wait_met(&until, since) {
            Some(Waited::Event(e)) => {
                json!({ "met": true, "by": "event", "event": e, "last": last })
            }
            Some(Waited::Settled) => {
                json!({ "met": true, "by": "settled", "event": null, "last": last })
            }
            None => json!({ "met": false, "by": null, "event": null, "last": last }),
        })
    }
}
