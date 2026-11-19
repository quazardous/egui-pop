//! Destructive calls: an agent's call that destroys or discards what
//! cannot easily be had back (`commands.run` of a command declared
//! `CommandSpec::destructive`, an action whose collector says so) waits
//! for the user to confirm it.
//!
//! - The call is held, as a `hello` is, and a `dialogs::Confirm` names
//!   the agent, what it asks and its arguments, in the danger style.
//! - Accepted, the call runs as the agent asked it, with its origin, and
//!   its answer is the call's.
//! - Declined (Cancel, or the dialog closed), or past its deadline, it is
//!   refused: the agent hears why, and the journal says it
//!   (`command.refused` for a command, `debug.call.refused` for another
//!   action, `by` [`BY`]).
//!
//! Only the user answers: the answer command ([`CMD_CONFIRM`], shared
//! with pairing) is kept from agents, and an agent's accept through
//! `dialogs.answer` is vetoed (its cancel goes through). The user's own
//! calls are never asked.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use egui_pop_api::AppHandle;
use egui_pop_api::dialogs::{Confirm, DIALOGS};
use egui_pop_core::strings;
use egui_pop_model::commands::Origin;
use serde_json::{Value as Json, json};

use crate::Reply;
use crate::pairing::CMD_CONFIRM;
use crate::rpc::RpcError;
use crate::strings as txt;

/// Who refuses a destructive call the user declined or did not confirm
/// in time: the `by` of its `command.refused` (or `debug.call.refused`).
pub const BY: &str = "pop.agent.confirm";
/// How long a destructive call waits for the user's answer when it
/// gives no `deadline_ms`.
pub const CONFIRM_DEADLINE: Duration = Duration::from_secs(60);
/// The longest it may wait.
pub const MAX_CONFIRM_DEADLINE: Duration = Duration::from_secs(120);
/// The arguments shown in the dialog, at most, in characters.
const SHOWN_ARGS: usize = 240;

/// A bridge call, as an agent asked it.
#[derive(Debug)]
pub(crate) struct Call {
    pub collector: String,
    pub action: String,
    pub args: Json,
    pub window: Option<String>,
    pub origin: Origin,
}

impl Call {
    /// What it asks, for the dialog and the agent: the command's id for
    /// `commands.run`, `collector.action` otherwise.
    fn what(&self) -> String {
        match self.command() {
            Some(id) => id.to_string(),
            None => format!("{}.{}", self.collector, self.action),
        }
    }

    /// The command it runs, for `commands.run`.
    fn command(&self) -> Option<&str> {
        (self.collector == "commands" && self.action == "run")
            .then(|| self.args.get("id").and_then(Json::as_str))
            .flatten()
    }

    /// Its arguments, shown: the command's own for `commands.run`.
    fn shown_args(&self) -> String {
        let args = match self.command() {
            Some(_) => self.args.get("args").cloned().unwrap_or(Json::Null),
            None => self.args.clone(),
        };
        let text = match args {
            Json::Null => "none".to_string(),
            a => a.to_string(),
        };
        if text.chars().count() <= SHOWN_ARGS {
            text
        } else {
            let cut: String = text.chars().take(SHOWN_ARGS).collect();
            format!("{cut}…")
        }
    }
}

/// A call waiting for the user's answer.
struct Held {
    call: Call,
    deadline: Instant,
    reply: Reply,
    dialog: u64,
    /// The dialog was answered with its verb: its command runs next.
    answered: bool,
    /// The user accepted: it runs at the next pass.
    accepted: bool,
}

#[derive(Default)]
struct State {
    next: Cell<u64>,
    held: RefCell<BTreeMap<u64, Held>>,
}

fn state(app: &AppHandle) -> Rc<State> {
    app.local::<State>()
}

/// Whether an agent's call must wait for the user: it is destructive,
/// it is not the user's, and the bridge would let it through (a call
/// refused anyway is not asked about).
pub(crate) fn needed(app: &AppHandle, call: &Call) -> bool {
    call.origin != Origin::User
        && app.debug_destructive(&call.collector, &call.action, &call.args)
        && app
            .debug_refusal(&call.collector, &call.action, &call.args, &call.origin)
            .is_none()
}

/// Holds `call` until the user answers, through a confirmation dialog.
pub(crate) fn hold(app: &AppHandle, call: Call, deadline: Instant, reply: Reply) {
    let Some(dialogs) = app.service(&DIALOGS) else {
        refuse(
            app,
            &call,
            reply,
            RpcError::new(
                RpcError::REFUSED,
                format!(
                    "`{}` is destructive and needs the user's confirmation, and the dialogs plugin is not connected",
                    call.what()
                ),
            ),
        );
        return;
    };
    let st = state(app);
    let n = st.next.get() + 1;
    st.next.set(n);
    let ctx = app.egui();
    let agent = match &call.origin {
        Origin::Agent(a) => a.clone(),
        o => o.to_string(),
    };
    let what = call.what();
    let title = strings::tr_args(
        &ctx,
        &txt::CONFIRM_TITLE,
        &[("agent", &agent), ("what", &what)],
    );
    let detail = strings::tr_args(&ctx, &txt::CONFIRM_DETAIL, &[("args", &call.shown_args())]);
    let question = Confirm::new(&title, &strings::tr(&ctx, &txt::CONFIRM_VERB), CMD_CONFIRM)
        .detail(&detail)
        .danger()
        .args(json!({ "run": n }));
    let dialog = dialogs.open(question);
    st.held.borrow_mut().insert(
        n,
        Held {
            call,
            deadline,
            reply,
            dialog,
            answered: false,
            accepted: false,
        },
    );
    ctx.request_repaint();
}

/// The user's answer command accepted held call `n`: it runs at the next
/// pass.
pub(crate) fn accept(app: &AppHandle, n: u64) -> Result<Json, String> {
    let st = state(app);
    let mut held = st.held.borrow_mut();
    let h = held
        .get_mut(&n)
        .ok_or_else(|| format!("no agent's call waits for confirmation under {n}"))?;
    h.accepted = true;
    let what = h.call.what();
    drop(held);
    app.egui().request_repaint();
    Ok(json!({ "confirmed": what }))
}

/// The dialog's answer: its verb (the command runs next), or Cancel.
pub(crate) fn answered(app: &AppHandle, n: u64, cancel: bool) {
    if cancel {
        let what = state(app)
            .held
            .borrow()
            .get(&n)
            .map(|h| h.call.what())
            .unwrap_or_default();
        end(app, n, declined_error(&what));
    } else if let Some(h) = state(app).held.borrow_mut().get_mut(&n) {
        h.answered = true;
    }
}

fn declined_error(what: &str) -> RpcError {
    RpcError::new(
        RpcError::REFUSED,
        format!("the user declined `{what}`: it is destructive and needs the user's confirmation"),
    )
}

/// Each pass: the calls accepted run; those past their deadline, or
/// whose dialog closed without an answer, are refused.
pub(crate) fn pass(app: &AppHandle) {
    let st = state(app);
    if st.held.borrow().is_empty() {
        return;
    }
    let now = Instant::now();
    let open = app.service(&DIALOGS);
    let mut run = Vec::new();
    let mut ended = Vec::new();
    for (n, h) in st.held.borrow().iter() {
        if h.accepted {
            run.push(*n);
        } else if now >= h.deadline {
            ended.push((
                *n,
                RpcError::new(
                    RpcError::NO_ANSWER,
                    format!(
                        "the user did not confirm `{}` in time: it is destructive and needs the user's confirmation",
                        h.call.what()
                    ),
                ),
            ));
        } else if !h.answered && open.as_ref().is_some_and(|d| !d.is_open(h.dialog)) {
            ended.push((*n, declined_error(&h.call.what())));
        }
    }
    for n in run {
        let Some(h) = st.held.borrow_mut().remove(&n) else {
            continue;
        };
        let c = h.call;
        let f = app.debug_call_in(
            c.window.as_deref(),
            &c.collector,
            &c.action,
            &c.args,
            c.origin,
        );
        let _ = h
            .reply
            .send(Ok(serde_json::to_value(&f).unwrap_or(Json::Null)));
    }
    for (n, e) in ended {
        end(app, n, e);
    }
}

/// A held call refused: the agent hears why, the journal says it.
fn end(app: &AppHandle, n: u64, error: RpcError) {
    let Some(h) = state(app).held.borrow_mut().remove(&n) else {
        return;
    };
    refuse(app, &h.call, h.reply, error);
}

/// `call` refused with `error`, on the journal, from its origin.
fn refuse(app: &AppHandle, call: &Call, reply: Reply, error: RpcError) {
    let source = call.origin.to_string();
    match call.command() {
        Some(id) => app.publish(
            &source,
            None,
            "command.refused",
            json!({ "command": id, "origin": source, "reason": error.message, "by": BY }),
        ),
        None => app.publish(
            &source,
            None,
            "debug.call.refused",
            json!({
                "collector": call.collector,
                "action": call.action,
                "args": call.args,
                "reason": error.message,
                "by": BY,
            }),
        ),
    }
    let _ = reply.send(Err(error));
}

/// The socket is off: the calls held end.
pub(crate) fn refuse_all(app: &AppHandle) {
    let held: Vec<u64> = state(app).held.borrow().keys().copied().collect();
    for n in held {
        end(app, n, RpcError::stopped());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(collector: &str, action: &str, args: Json) -> Call {
        Call {
            collector: collector.into(),
            action: action.into(),
            args,
            window: None,
            origin: Origin::Agent("bot".into()),
        }
    }

    #[test]
    fn a_command_is_shown_by_its_id_and_its_own_arguments() {
        let c = call(
            "commands",
            "run",
            json!({ "id": "files.trash", "args": { "path": "a" } }),
        );
        assert_eq!(c.what(), "files.trash");
        assert_eq!(c.shown_args(), r#"{"path":"a"}"#);
        let c = call("files", "purge", Json::Null);
        assert_eq!(c.what(), "files.purge");
        assert_eq!(c.shown_args(), "none");
        let c = call("files", "purge", json!("x".repeat(400)));
        assert_eq!(c.shown_args().chars().count(), SHOWN_ARGS + 1);
    }
}
