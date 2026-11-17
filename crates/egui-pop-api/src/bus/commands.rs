//! A command run on the bus (ADR 0029): every run of a command, whatever
//! asked it (a key, the palette, a menu, a toolbar, a toast's action, an
//! agent), is asked on `command.run` ([`CommandRun`]) before its handler
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
//! the point is asked, as before.

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

/// A command is about to run (`command.run`): its listeners run in their
/// declared order before the handler; one may refuse the run with a
/// reason. Fail-closed: a listener's failure refuses it too. The value
/// is what the listeners noted, one line each.
#[derive(Debug)]
pub struct CommandRun;
impl Point for CommandRun {
    const KIND: PointKind = PointKind::Request;
    const ID: &'static str = "command.run";
    type Input = CommandAsk;
    type Value = Vec<String>;
}

/// The command points, in the order [`super::order_dump`] prints them.
pub const COMMAND_POINTS: [&str; 1] = [CommandRun::ID];

/// Asks `command.run` for `app`, inline: the refusal that stopped the
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
