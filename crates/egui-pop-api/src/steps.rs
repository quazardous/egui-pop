//! Steps: the moments of a feature's life an application can reach
//! (ADR 0011). egui-pop does the plumbing (closing a window, detaching a
//! panel, switching an arrangement, rebinding a key); at each step a
//! plugin may:
//!
//! - **observe**: every step publishes a signal on the host's journal,
//!   named after the step (`frame.close`), with its arguments and result,
//!   or `<step>.vetoed`;
//! - **veto or redirect** with a before-hook ([`Before`]): stop the step
//!   with a reason, or change its arguments;
//! - **replace** with a run-hook: do the step its own way instead of
//!   egui-pop's default (the last plugin to replace a step wins; two are
//!   reported);
//! - **extend** with an after-hook: act on the result.
//!
//! Each is optional; without hooks a step runs its default. Every run is
//! traced (who redirected, vetoed, ran, followed) and shown by the debug
//! bridge's `steps` collector.
//!
//! ```
//! use egui_pop_api::steps::{Before, StepSpec};
//! use egui_pop_api::{App, Ctx, Plugin, PluginError};
//! use egui_pop_model::ids::{PluginId, StepId};
//!
//! const SAVE: StepId = StepId::from_static("demo.save");
//!
//! struct Guard;
//! impl Plugin for Guard {
//!     fn id(&self) -> PluginId { PluginId::from_static("guard") }
//!     fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
//!         ctx.step(StepSpec::new(SAVE, "Saves the demo."))?;
//!         ctx.before(SAVE, |_, args| {
//!             if args["name"] == "" { Before::Veto("no name".into()) } else { Before::Go }
//!         });
//!         Ok(())
//!     }
//! }
//!
//! let app = App::builder().plugin(Guard).build(&egui::Context::default()).unwrap();
//! let h = app.handle();
//! let out = h.step(&SAVE, serde_json::json!({ "name": "" }), |_, _| Ok(serde_json::json!("saved")));
//! assert_eq!(out.vetoed(), Some("no name"));
//! let out = h.step(&SAVE, serde_json::json!({ "name": "a" }), |_, _| Ok(serde_json::json!("saved")));
//! assert_eq!(out.result(), Some(&Ok(serde_json::json!("saved"))));
//! ```

use std::fmt;
use std::rc::Rc;

use egui_pop_model::ids::{OwnerId, StepId};
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::app::AppHandle;

/// What a before-hook decides.
#[derive(Clone, Debug, PartialEq)]
pub enum Before {
    /// The step goes on.
    Go,
    /// The step stops; the reason is traced and published.
    Veto(String),
    /// The step goes on with these arguments instead.
    Redirect(Json),
}

/// A before-hook.
pub type BeforeFn = Rc<dyn Fn(&AppHandle, &Json) -> Before>;
/// A run-hook: does the step instead of the default.
pub type RunFn = Rc<dyn Fn(&AppHandle, &Json) -> Result<Json, String>>;
/// An after-hook: sees the arguments and the result.
pub type AfterFn = Rc<dyn Fn(&AppHandle, &Json, &Result<Json, String>)>;

/// A step as declared: its id, what it does, an example of its
/// arguments.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StepSpec {
    pub id: StepId,
    pub description: String,
    pub args: Json,
}

impl StepSpec {
    pub fn new(id: StepId, description: &str) -> Self {
        Self {
            id,
            description: description.into(),
            args: Json::Null,
        }
    }

    /// An example of its arguments, shown by the debug bridge.
    pub fn args(mut self, example: Json) -> Self {
        self.args = example;
        self
    }
}

/// A declared step with its hooks, as the debug bridge shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StepInfo {
    #[serde(flatten)]
    pub spec: StepSpec,
    pub owner: OwnerId,
    pub hooks: Vec<HookInfo>,
}

/// A hook on a step: its owner and kind (`before`, `replace`, `after`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HookInfo {
    pub owner: OwnerId,
    pub kind: &'static str,
}

/// The kind of a hook.
#[derive(Clone)]
pub(crate) enum HookFn {
    Before(BeforeFn),
    Run(RunFn),
    After(AfterFn),
}

impl HookFn {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            HookFn::Before(_) => "before",
            HookFn::Run(_) => "replace",
            HookFn::After(_) => "after",
        }
    }
}

/// A hook on a step.
#[derive(Clone)]
pub(crate) struct StepHook {
    pub step: StepId,
    pub f: HookFn,
}

impl fmt::Debug for StepHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.f.kind(), self.step)
    }
}

/// How a step ended.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// It ran (by default or by a run-hook): its result.
    Done(Result<Json, String>),
    /// A before-hook stopped it.
    Vetoed { by: OwnerId, reason: String },
}

impl Outcome {
    /// The result, when it ran.
    pub fn result(&self) -> Option<&Result<Json, String>> {
        match self {
            Outcome::Done(r) => Some(r),
            Outcome::Vetoed { .. } => None,
        }
    }

    /// Whether it ran and succeeded.
    pub fn ok(&self) -> bool {
        matches!(self, Outcome::Done(Ok(_)))
    }

    /// The reason, when it was vetoed.
    pub fn vetoed(&self) -> Option<&str> {
        match self {
            Outcome::Vetoed { reason, .. } => Some(reason),
            Outcome::Done(_) => None,
        }
    }
}

/// One run of a step, as the `steps` collector shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StepTrace {
    pub step: StepId,
    /// The arguments it was asked with.
    pub asked: Json,
    /// The arguments it ran with, after redirections.
    pub args: Json,
    /// Who redirected it, in order.
    pub redirected_by: Vec<OwnerId>,
    /// Who vetoed it, and why.
    pub vetoed_by: Option<(OwnerId, String)>,
    /// Who ran it: a plugin's run-hook, or `default`.
    pub ran_by: Option<String>,
    /// Who followed it with an after-hook.
    pub after: Vec<OwnerId>,
    pub result: Option<Result<Json, String>>,
    /// Whether nobody declared the step.
    pub undeclared: bool,
}

/// How many runs the trace keeps.
pub const TRACES: usize = 64;

/// The hooks of one step, taken out of the registry for a run.
pub(crate) fn run(
    app: &AppHandle,
    step: &StepId,
    args: Json,
    hooks: Vec<(OwnerId, HookFn)>,
    declared: bool,
    default: impl FnOnce(&AppHandle, &Json) -> Result<Json, String>,
) -> (Outcome, StepTrace) {
    let mut trace = StepTrace {
        step: step.clone(),
        asked: args.clone(),
        args: Json::Null,
        redirected_by: Vec::new(),
        vetoed_by: None,
        ran_by: None,
        after: Vec::new(),
        result: None,
        undeclared: !declared,
    };
    let mut args = args;
    for (owner, f) in &hooks {
        if let HookFn::Before(f) = f {
            match f(app, &args) {
                Before::Go => {}
                Before::Redirect(new) => {
                    args = new;
                    trace.redirected_by.push(owner.clone());
                }
                Before::Veto(reason) => {
                    trace.args = args.clone();
                    trace.vetoed_by = Some((owner.clone(), reason.clone()));
                    app.publish(
                        "egui_pop",
                        None,
                        &format!("{step}.vetoed"),
                        json!({ "args": args, "by": owner, "reason": reason }),
                    );
                    return (
                        Outcome::Vetoed {
                            by: owner.clone(),
                            reason,
                        },
                        trace,
                    );
                }
            }
        }
    }
    let runner = hooks.iter().rev().find_map(|(o, f)| match f {
        HookFn::Run(f) => Some((o.clone(), f.clone())),
        _ => None,
    });
    let result = match runner {
        Some((owner, f)) => {
            trace.ran_by = Some(owner.to_string());
            f(app, &args)
        }
        None => {
            trace.ran_by = Some("default".into());
            default(app, &args)
        }
    };
    for (owner, f) in &hooks {
        if let HookFn::After(f) = f {
            f(app, &args, &result);
            trace.after.push(owner.clone());
        }
    }
    let data = match &result {
        Ok(v) => json!({ "args": args, "result": v }),
        Err(e) => json!({ "args": args, "error": e }),
    };
    app.publish("egui_pop", None, step.as_str(), data);
    trace.args = args;
    trace.result = Some(result.clone());
    (Outcome::Done(result), trace)
}
