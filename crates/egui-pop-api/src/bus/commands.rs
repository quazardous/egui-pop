//! A command run on the bus (ADR 0029): every run of a command, whatever
//! asked it (a key, the palette, a menu, a toolbar, a toast's action, an
//! agent), is asked on `pop.command.run` ([`CommandRun`]) before its handler
//! runs. The point is declared fail-closed: a listener may refuse the run
//! with a reason (`HookError::refuse`), and a listener that fails, panics
//! or misses its deadline refuses it too. A refused run never reaches the
//! host; it is said as `command.refused` with the listener that refused
//! it (`by`) and its reason, and the caller gets the refusal as its
//! error.
//!
//! egui-pop registers no listener on it: with nobody listening, a run
//! goes on as before, to the host's dispatch and its handler, said as
//! `command.dispatched` then `command.done` or `command.failed`. A
//! command not offered to agents, unknown or disabled is refused before
//! the point is asked, as before; so is a run whose arguments do not
//! match its command's JSON Schema (`by: "pop.command.args"`,
//! [`ARGS_VALIDATOR`]).

use bus_contract::{Point, PointKind, Refusal};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::CommandId;
use serde_json::Value as Json;

use super::{ASKING, Asking};
use crate::app::AppHandle;

/// A command about to run: which, with what, for whom, and in which
/// window.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CommandAsk {
    /// The command.
    pub command: CommandId,
    /// Its arguments.
    pub args: Json,
    /// Who asks: the person, an agent, a script.
    pub origin: Origin,
    /// The application window it was asked in, when it was not the main
    /// window.
    pub window: Option<String>,
}

impl CommandAsk {
    pub fn new(command: CommandId, args: Json, origin: Origin) -> Self {
        Self {
            command,
            args,
            origin,
            window: None,
        }
    }

    /// The application window it was asked in.
    pub fn window(mut self, window: impl Into<String>) -> Self {
        self.window = Some(window.into());
        self
    }
}

/// A command is about to run (`pop.command.run`): its listeners run in their
/// declared order before the handler; one may refuse the run with a
/// reason. Fail-closed: a listener's failure refuses it too. The value
/// is what the listeners noted, one line each.
#[derive(Debug)]
pub struct CommandRun;
impl Point for CommandRun {
    const KIND: PointKind = PointKind::Request;
    const ID: &'static str = "pop.command.run";
    type Input = CommandAsk;
    type Value = Vec<String>;
}

/// Who refuses a run whose arguments do not match its command's JSON
/// Schema (`CommandSpec::schema`): the `by` of its `command.refused`. The
/// check is not a listener: it runs before `pop.command.run` is asked,
/// for every origin.
pub const ARGS_VALIDATOR: &str = "pop.command.args";

/// Why `args` do not match `spec`'s JSON Schema, `None` when they do or
/// when its arguments are only described (an example, a text). `null`
/// arguments are checked as `{}` against an object's schema.
pub(crate) fn args_refusal(
    spec: &egui_pop_model::commands::CommandSpec,
    args: &Json,
) -> Option<String> {
    if !egui_pop_core::debug::is_schema(&spec.args) {
        return None;
    }
    let empty = Json::Object(Default::default());
    let args = if args.is_null() && spec.args.get("type").and_then(Json::as_str) == Some("object") {
        &empty
    } else {
        args
    };
    egui_pop_core::schema::check(&spec.args, args)
        .err()
        .map(|e| format!("its arguments do not match its schema: {e}"))
}

/// The command points, in the order [`super::order_dump`] prints them.
pub const COMMAND_POINTS: [&str; 1] = [CommandRun::ID];

/// Asks `pop.command.run` for `app`, inline: the refusal that stopped the
/// run, if one did; the listeners' failures logged.
pub(crate) fn ask(app: &AppHandle, ask: CommandAsk) -> Option<Refusal> {
    ASKING.with_borrow_mut(|a| a.push(app.downgrade()));
    let _asking = Asking;
    let out = app.bus().alter::<CommandRun>(ask, Vec::new());
    for f in &out.failures {
        log::error!(target: "egui_pop", "{f}");
    }
    out.refused
}

/// A refusal as the run's error and the journal's reason: "refused by
/// `listener`: reason", or "refused, `listener` failed: reason".
pub(crate) fn reason(refusal: &Refusal) -> String {
    if refusal.failed {
        format!("refused, `{}` failed: {}", refusal.listener, refusal.reason)
    } else {
        format!("refused by `{}`: {}", refusal.listener, refusal.reason)
    }
}
