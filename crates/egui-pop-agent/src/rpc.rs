//! JSON-RPC 2.0, one message per line: reading a request, handing it to
//! the UI thread, writing its response.

use std::io::{BufRead, Read as _, Write};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use egui_pop_core::debug::Feedback;
use egui_pop_model::commands::Origin;
use serde_json::{Value as Json, json};

use crate::confirm::{CONFIRM_DEADLINE, MAX_CONFIRM_DEADLINE};
use crate::limits::MAX_REQUEST_BYTES;
use crate::pairing::{HELLO_DEADLINE, MAX_NAME_CHARS};
use crate::{AGENT, DEFAULT_DEADLINE, Job, MAX_DEADLINE, Waker};

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
    /// A transport limit was reached: too many clients connected, or a
    /// request longer than [`crate::MAX_REQUEST_BYTES`]; the connection
    /// is closed after it.
    pub const LIMIT: i64 = -32004;

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

    /// The client is not a process of the user the application runs as.
    #[cfg(unix)]
    pub(crate) fn not_the_user() -> Self {
        Self::new(
            Self::REFUSED,
            "only the user the application runs as may connect",
        )
    }

    /// Too many clients are connected.
    pub(crate) fn too_many_connections() -> Self {
        Self::new(
            Self::LIMIT,
            format!(
                "too many clients connected (at most {}): try again once one closes",
                crate::MAX_CONNECTIONS
            ),
        )
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

/// Who a connection is: the paired agent it said `hello` as, if any.
#[derive(Debug, Default)]
pub(crate) struct Session {
    agent: Option<String>,
}

impl Session {
    /// Its calls' origin: `Origin::Agent("<id>")` once paired,
    /// `Origin::Agent("agent-socket")` before.
    pub(crate) fn origin(&self) -> Origin {
        Origin::Agent(self.agent.as_deref().unwrap_or(AGENT).into())
    }
}

/// Serves one connection until it closes, or until it sends a request
/// longer than [`MAX_REQUEST_BYTES`]; a paired agent's closing is told
/// to the UI thread (`agent.disconnected`).
pub(crate) fn serve(reader: impl BufRead, writer: impl Write, tx: &Sender<Job>, wake: &Waker) {
    let mut session = Session::default();
    serve_session(reader, writer, tx, wake, &mut session);
    if let Some(agent) = session.agent
        && tx.send(Job::Gone { agent }).is_ok()
    {
        wake.wake();
    }
}

fn serve_session(
    mut reader: impl BufRead,
    mut writer: impl Write,
    tx: &Sender<Job>,
    wake: &Waker,
    session: &mut Session,
) {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        // One byte more than the cap and its `\n`: a line that fills it
        // is too long.
        let limit = MAX_REQUEST_BYTES as u64 + 2;
        match (&mut reader).take(limit).read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if buf.last() == Some(&b'\n') {
            buf.pop();
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
        }
        if buf.len() > MAX_REQUEST_BYTES {
            let e = RpcError::new(
                RpcError::LIMIT,
                format!("a request longer than {MAX_REQUEST_BYTES} bytes: the connection closes"),
            );
            log::warn!(target: "egui_pop_agent", "agent client closed: {}", e.message);
            refuse(&mut writer, e);
            return;
        }
        let response = match std::str::from_utf8(&buf) {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => handle(line, tx, wake, session),
            Err(e) => Some(error(
                Json::Null,
                RpcError::new(RpcError::PARSE, format!("not UTF-8: {e}")),
            )),
        };
        if let Some(response) = response
            && (writeln!(writer, "{response}").is_err() || writer.flush().is_err())
        {
            return;
        }
    }
}

/// Answers a connection that is closed before any request is read (a
/// transport limit, a client that is not the user's) with `e`, `id`
/// null.
pub(crate) fn refuse(mut writer: impl Write, e: RpcError) {
    let _ = writeln!(writer, "{}", error(Json::Null, e));
    let _ = writer.flush();
}

/// The response to one line; `None` for a notification (no `id`).
pub(crate) fn handle(
    line: &str,
    tx: &Sender<Job>,
    wake: &Waker,
    session: &mut Session,
) -> Option<Json> {
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
    respond(dispatch(method, params, tx, wake, session))
}

fn error(id: Json, e: RpcError) -> Json {
    json!({ "jsonrpc": "2.0", "id": id, "error": e.to_json() })
}

fn dispatch(
    method: &str,
    params: Json,
    tx: &Sender<Job>,
    wake: &Waker,
    session: &mut Session,
) -> Result<Json, RpcError> {
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
    let origin = session.origin();
    let mut within = ANSWER_WITHIN;
    let mut hello = false;
    let job = match method {
        "hello" => {
            if let Some(agent) = &session.agent {
                return Err(RpcError::params(format!(
                    "this connection said `hello` already, as `{agent}`"
                )));
            }
            let name = str_param("name")?;
            if name.trim().is_empty() || name.chars().count() > MAX_NAME_CHARS {
                return Err(RpcError::params(format!(
                    "`name` is 1 to {MAX_NAME_CHARS} characters"
                )));
            }
            let token = match params.get("token") {
                None | Some(Json::Null) => None,
                Some(_) => Some(str_param("token")?),
            };
            let held = params
                .get("deadline_ms")
                .and_then(Json::as_u64)
                .map_or(HELLO_DEADLINE, Duration::from_millis)
                .min(HELLO_DEADLINE);
            within = held + Duration::from_secs(2);
            hello = true;
            Job::Hello {
                name,
                token,
                deadline: Instant::now() + held,
                reply,
            }
        }
        "describe" => Job::Describe { origin, reply },
        "snapshot" => Job::Snapshot {
            collector: match params.get("collector") {
                None | Some(Json::Null) => None,
                Some(_) => Some(str_param("collector")?),
            },
            window,
            reply,
        },
        "call" => {
            // A destructive call is held until the user confirms it, at
            // most this long.
            let held = params
                .get("deadline_ms")
                .and_then(Json::as_u64)
                .map_or(CONFIRM_DEADLINE, Duration::from_millis)
                .min(MAX_CONFIRM_DEADLINE);
            within = within.max(held + Duration::from_secs(2));
            Job::Call {
                collector: str_param("collector")?,
                action: str_param("action")?,
                args: params.get("args").cloned().unwrap_or(Json::Null),
                window,
                origin,
                deadline: Instant::now() + held,
                reply,
            }
        }
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
                origin,
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
                format!("no method `{other}`: hello, describe, snapshot, call, wait, ui"),
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
        Ok(r) => {
            if hello && let Ok(result) = &r {
                session.agent = result["agent"].as_str().map(String::from);
            }
            r
        }
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
