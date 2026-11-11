//! JSON-RPC 2.0, one message per line: reading a request, handing it to
//! the UI thread, writing its response.

use std::io::{BufRead, Write};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use egui_pop_core::debug::Feedback;
use serde_json::{Value as Json, json};

use crate::{DEFAULT_DEADLINE, Job, MAX_DEADLINE, Waker};

/// How long a request other than `wait` waits for the UI thread: it is
/// answered at the next pass, unless the application stopped drawing.
const ANSWER_WITHIN: Duration = Duration::from_secs(30);

/// A JSON-RPC error.
#[derive(Clone, Debug, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Json>,
}

impl RpcError {
    /// The request is not JSON.
    pub const PARSE: i64 = -32700;
    /// Not a JSON-RPC 2.0 request.
    pub const INVALID_REQUEST: i64 = -32600;
    /// No such method.
    pub const NO_METHOD: i64 = -32601;
    /// Its params are wrong.
    pub const INVALID_PARAMS: i64 = -32602;
    /// The bridge refused it (its mode, an `agent` flag, a veto); `data`
    /// is the feedback block.
    pub const REFUSED: i64 = -32001;
    /// The application did not answer in time (it ran no frame).
    pub const NO_ANSWER: i64 = -32002;
    /// A `ui` action found no node, or more than one, to act on.
    pub const UI: i64 = -32003;

    pub(crate) fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub(crate) fn params(message: impl Into<String>) -> Self {
        Self::new(Self::INVALID_PARAMS, message)
    }

    /// The bridge is off: nothing but the user may read or call it.
    pub(crate) fn off() -> Self {
        Self::new(
            Self::REFUSED,
            "the debug bridge is off; only the user may read or call it",
        )
    }

    /// The socket was turned off (its setting) while a client was
    /// connected.
    pub(crate) fn stopped() -> Self {
        Self::new(Self::REFUSED, "the agent socket is off")
    }

    pub(crate) fn refused(f: &Feedback) -> Self {
        Self {
            code: Self::REFUSED,
            message: f.message.clone(),
            data: serde_json::to_value(f).ok(),
        }
    }

    fn to_json(&self) -> Json {
        let mut e = json!({ "code": self.code, "message": self.message });
        if let Some(d) = &self.data {
            e["data"] = d.clone();
        }
        e
    }
}

/// Serves one connection until it closes.
pub(crate) fn serve(reader: impl BufRead, mut writer: impl Write, tx: &Sender<Job>, wake: &Waker) {
    for line in reader.lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle(&line, tx, wake)
            && (writeln!(writer, "{response}").is_err() || writer.flush().is_err())
        {
            return;
        }
    }
}

/// The response to one line; `None` for a notification (no `id`).
pub(crate) fn handle(line: &str, tx: &Sender<Job>, wake: &Waker) -> Option<Json> {
    let request: Json = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return Some(error(
                Json::Null,
                RpcError::new(RpcError::PARSE, e.to_string()),
            ));
        }
    };
    let id = request.get("id").cloned();
    let respond = |result: Result<Json, RpcError>| {
        let id = id.clone()?;
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err(e) => error(id, e),
        })
    };
    if request.get("jsonrpc").and_then(Json::as_str) != Some("2.0") {
        return Some(error(
            id.unwrap_or(Json::Null),
            RpcError::new(
                RpcError::INVALID_REQUEST,
                "a JSON-RPC 2.0 request (`jsonrpc: \"2.0\"`, `method`), one per line",
            ),
        ));
    }
    let Some(method) = request.get("method").and_then(Json::as_str) else {
        return Some(error(
            id.unwrap_or(Json::Null),
            RpcError::new(RpcError::INVALID_REQUEST, "no `method`"),
        ));
    };
    let params = request.get("params").cloned().unwrap_or(json!({}));
    respond(dispatch(method, params, tx, wake))
}

fn error(id: Json, e: RpcError) -> Json {
    json!({ "jsonrpc": "2.0", "id": id, "error": e.to_json() })
}

fn dispatch(method: &str, params: Json, tx: &Sender<Job>, wake: &Waker) -> Result<Json, RpcError> {
    let (reply, rx) = mpsc::sync_channel(1);
    let str_param = |name: &str| -> Result<String, RpcError> {
        params
            .get(name)
            .and_then(Json::as_str)
            .map(String::from)
            .ok_or_else(|| RpcError::params(format!("`{name}` is missing or not a string")))
    };
    // The application window a call or a snapshot is for: the active
    // one without.
    let window = match params.get("window") {
        None | Some(Json::Null) => None,
        Some(_) => Some(str_param("window")?),
    };
    let mut within = ANSWER_WITHIN;
    let job = match method {
        "describe" => Job::Describe { reply },
        "snapshot" => Job::Snapshot {
            collector: match params.get("collector") {
                None | Some(Json::Null) => None,
                Some(_) => Some(str_param("collector")?),
            },
            window,
            reply,
        },
        "call" => Job::Call {
            collector: str_param("collector")?,
            action: str_param("action")?,
            args: params.get("args").cloned().unwrap_or(Json::Null),
            window,
            reply,
        },
        "wait" => {
            let held = params
                .get("deadline_ms")
                .and_then(Json::as_u64)
                .map(Duration::from_millis)
                .unwrap_or(DEFAULT_DEADLINE)
                .min(MAX_DEADLINE);
            // The UI thread answers at the deadline; this one gives up a
            // little later, if no frame ran at all.
            within = held + Duration::from_secs(2);
            Job::Wait {
                args: params,
                deadline: Instant::now() + held,
                reply,
            }
        }
        "ui" => Job::Ui {
            params,
            // The UI thread answers first, so the reason reaches the client.
            deadline: Instant::now() + ANSWER_WITHIN - Duration::from_secs(5),
            reply,
        },
        other => {
            return Err(RpcError::new(
                RpcError::NO_METHOD,
                format!("no method `{other}`: describe, snapshot, call, wait, ui"),
            ));
        }
    };
    if tx.send(job).is_err() {
        return Err(RpcError::new(
            RpcError::NO_ANSWER,
            "the application has closed",
        ));
    }
    wake.wake();
    match rx.recv_timeout(within) {
        Ok(r) => r,
        Err(RecvTimeoutError::Timeout) => Err(RpcError::new(
            RpcError::NO_ANSWER,
            "the application did not answer: no frame ran",
        )),
        Err(RecvTimeoutError::Disconnected) => Err(RpcError::new(
            RpcError::NO_ANSWER,
            "the application has closed",
        )),
    }
}
