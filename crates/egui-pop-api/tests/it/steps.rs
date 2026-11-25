//! Steps (ADR 0011): before-hooks veto or redirect, a run-hook replaces
//! the default, after-hooks follow, the journal and the trace see it all,
//! and hooks go with their plugin.

use std::cell::RefCell;
use std::rc::Rc;

use egui_pop_api::steps::{Before, Outcome, StepSpec};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{OwnerId, PluginId, StepId};
use serde_json::{Value as Json, json};

const STEP: StepId = StepId::from_static("demo.close");

type Connect = Box<dyn FnMut(&mut Ctx<'_>) -> Result<(), PluginError>>;

struct P(&'static str, Connect);

impl Plugin for P {
    fn id(&self) -> PluginId {
        PluginId::new(self.0).unwrap()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        (self.1)(ctx)
    }
}

fn plugin(id: &'static str, f: impl FnMut(&mut Ctx<'_>) -> Result<(), PluginError> + 'static) -> P {
    P(id, Box::new(f))
}

fn owner(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    ctx.step(StepSpec::new(STEP, "Closes the demo.").args(json!({ "force": false })))?;
    Ok(())
}

fn default(_: &egui_pop_api::AppHandle, args: &Json) -> Result<Json, String> {
    Ok(json!({ "closed": true, "force": args["force"] }))
}

#[test]
fn without_hooks_a_step_runs_its_default_and_says_so_on_the_journal() {
    let app = App::builder()
        .plugin(plugin("owner", owner))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    let before = h.last_event();
    let out = h.step(&STEP, json!({ "force": false }), default);
    assert_eq!(
        out,
        Outcome::Done(Ok(json!({ "closed": true, "force": false })))
    );
    let events = h.events_since(before);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "demo.close");
    assert_eq!(events[0].data["result"]["closed"], true);
    let trace = h.step_traces().pop().unwrap();
    assert_eq!(trace.ran_by.as_deref(), Some("default"));
    assert!(!trace.undeclared);
}

#[test]
fn before_hooks_veto_or_redirect_a_run_hook_replaces_after_hooks_follow() {
    let seen = Rc::new(RefCell::new(Vec::<String>::new()));
    let s1 = seen.clone();
    let app = App::builder()
        .plugin(plugin("owner", owner))
        .plugin(plugin("guard", |ctx| {
            ctx.before(STEP, |_, args| {
                if args["unsaved"] == true {
                    Before::Veto("unsaved work".into())
                } else {
                    Before::Redirect(json!({ "force": true }))
                }
            });
            Ok(())
        }))
        .plugin(plugin("own-way", |ctx| {
            ctx.replace(STEP, |_, args| {
                Ok(json!({ "own": true, "force": args["force"] }))
            });
            Ok(())
        }))
        .plugin(plugin("watcher", move |ctx| {
            let s = s1.clone();
            ctx.after(STEP, move |_, args, result| {
                s.borrow_mut().push(format!("{args} -> {result:?}"));
            });
            Ok(())
        }))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    assert!(h.diagnostics().is_empty(), "{:?}", h.diagnostics());
    // Vetoed: nothing runs, nothing follows, the journal says why.
    let mark = h.last_event();
    let out = h.step(&STEP, json!({ "unsaved": true }), default);
    assert_eq!(
        out,
        Outcome::Vetoed {
            by: OwnerId::new("guard").unwrap(),
            reason: "unsaved work".into()
        }
    );
    assert!(seen.borrow().is_empty());
    let events = h.events_since(mark);
    assert_eq!(events[0].name, "demo.close.vetoed");
    assert_eq!(events[0].data["reason"], "unsaved work");
    // Redirected, then run by the replacing plugin, then followed.
    let out = h.step(&STEP, json!({ "force": false }), default);
    assert_eq!(
        out.result(),
        Some(&Ok(json!({ "own": true, "force": true })))
    );
    assert_eq!(seen.borrow().len(), 1);
    let trace = h.step_traces().pop().unwrap();
    assert_eq!(trace.asked, json!({ "force": false }));
    assert_eq!(trace.args, json!({ "force": true }));
    assert_eq!(trace.redirected_by, [OwnerId::new("guard").unwrap()]);
    assert_eq!(trace.ran_by.as_deref(), Some("own-way"));
    assert_eq!(trace.after, [OwnerId::new("watcher").unwrap()]);
    // The debug bridge lists the step and its hooks.
    let snap = h.debug_snapshot();
    let step = snap["steps"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "demo.close")
        .unwrap();
    let kinds: Vec<&str> = step["hooks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["before", "replace", "after"]);
    assert_eq!(snap["steps"]["runs"].as_array().unwrap().len(), 2);
}

#[test]
fn hooks_on_unknown_steps_and_double_replacements_are_reported() {
    let mut app = App::builder()
        .plugin(plugin("owner", owner))
        .plugin(plugin("a", |ctx| {
            ctx.replace(STEP, |_, _| Ok(json!("a")));
            ctx.after(StepId::from_static("nobody.declares"), |_, _, _| {});
            Ok(())
        }))
        .plugin(plugin("b", |ctx| {
            ctx.replace(STEP, |_, _| Ok(json!("b")));
            Ok(())
        }))
        .build(&egui::Context::default())
        .unwrap();
    let codes: Vec<&str> = app.handle().diagnostics().iter().map(|d| d.code).collect();
    assert!(codes.contains(&"unknown-step"), "{codes:?}");
    assert!(codes.contains(&"step-replaced-twice"), "{codes:?}");
    let out = app.handle().step(&STEP, Json::Null, default);
    assert_eq!(out.result(), Some(&Ok(json!("b"))));
    // `b` gone: `a` runs it, and the double replacement is no longer said.
    app.disconnect(&PluginId::from_static("b"));
    app.rebuild();
    let codes: Vec<&str> = app.handle().diagnostics().iter().map(|d| d.code).collect();
    assert!(!codes.contains(&"step-replaced-twice"), "{codes:?}");
    let out = app.handle().step(&STEP, Json::Null, default);
    assert_eq!(out.result(), Some(&Ok(json!("a"))));
    // The owner gone: the step is undeclared, still runs, and says so.
    app.disconnect(&PluginId::from_static("owner"));
    app.handle().step(&STEP, Json::Null, default);
    assert!(app.handle().step_traces().pop().unwrap().undeclared);
}
