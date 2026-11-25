//! A command's origin goes along the chain: what a handler starts while
//! running for an agent runs for that agent, the handler may not claim
//! the user's origin, and a command kept from agents never runs for one.

use std::cell::RefCell;
use std::rc::Rc;

use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::{Value as Json, json};

const OUTER: CommandId = CommandId::from_static("chain.outer");
const ESCALATE: CommandId = CommandId::from_static("chain.escalate");
const REACH: CommandId = CommandId::from_static("chain.reach");
const INNER: CommandId = CommandId::from_static("chain.inner");
const PRIVATE: CommandId = CommandId::from_static("chain.private");

type Seen = Rc<RefCell<Vec<(CommandId, Origin)>>>;

struct Chain(Seen);

impl Plugin for Chain {
    fn id(&self) -> PluginId {
        PluginId::from_static("chain")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let record = |seen: &Seen, id: CommandId| {
            let seen = seen.clone();
            move |call: &egui_pop_api::app::Call<'_>| {
                seen.borrow_mut().push((id.clone(), call.origin.clone()));
                assert_eq!(call.app.acting_origin(), *call.origin);
                Ok::<_, String>(Json::Null)
            }
        };
        let rec = record(&self.0, OUTER);
        ctx.command(CommandSpec::new(OUTER, "Outer", "Chain"), move |call| {
            rec(call)?;
            // No origin said: the one it runs for.
            call.app.run(INNER, Json::Null);
            Ok(Json::Null)
        })?;
        let rec = record(&self.0, ESCALATE);
        ctx.command(
            CommandSpec::new(ESCALATE, "Escalate", "Chain"),
            move |call| {
                rec(call)?;
                call.app.run_as(INNER, Json::Null, Origin::User);
                Ok(Json::Null)
            },
        )?;
        let rec = record(&self.0, REACH);
        ctx.command(CommandSpec::new(REACH, "Reach", "Chain"), move |call| {
            rec(call)?;
            call.app.run(PRIVATE, Json::Null);
            Ok(Json::Null)
        })?;
        ctx.command(
            CommandSpec::new(INNER, "Inner", "Chain"),
            record(&self.0, INNER),
        )?;
        ctx.command(
            CommandSpec::new(PRIVATE, "Private", "Chain").no_agent(),
            record(&self.0, PRIVATE),
        )?;
        Ok(())
    }
}

fn agent() -> Origin {
    Origin::Agent("tester".into())
}

fn app() -> (App, Seen) {
    let seen: Seen = Rc::default();
    let mut app = App::builder()
        .plugin(Chain(seen.clone()))
        .build(&egui::Context::default())
        .unwrap();
    app.logic(&egui::Context::default());
    (app, seen)
}

fn settle(app: &mut App) {
    app.logic(&egui::Context::default());
    app.logic(&egui::Context::default());
}

#[test]
fn what_a_handler_starts_runs_for_the_same_origin() {
    let (mut app, seen) = app();
    assert_eq!(app.handle().acting_origin(), Origin::User);
    assert!(app.run(&OUTER, Json::Null, agent()).now().unwrap().is_ok());
    settle(&mut app);
    assert_eq!(*seen.borrow(), vec![(OUTER, agent()), (INNER, agent())]);
    // The user's chain stays the user's.
    seen.borrow_mut().clear();
    let _ = app.run(&OUTER, Json::Null, Origin::User);
    settle(&mut app);
    assert_eq!(
        *seen.borrow(),
        vec![(OUTER, Origin::User), (INNER, Origin::User)]
    );
    assert_eq!(app.handle().acting_origin(), Origin::User);
}

#[test]
fn a_handler_running_for_an_agent_may_not_claim_the_user() {
    let (mut app, seen) = app();
    let h = app.handle().clone();
    let before = h.last_event();
    let _ = app.run(&ESCALATE, Json::Null, agent());
    settle(&mut app);
    assert_eq!(*seen.borrow(), vec![(ESCALATE, agent())]);
    let refused = h
        .events_since(before)
        .into_iter()
        .find(|e| e.name == "command.refused")
        .unwrap();
    assert_eq!(refused.source, "agent:tester");
    assert_eq!(refused.data["command"], "chain.inner");
    assert_eq!(refused.data["origin"], "user");
    assert!(
        refused.data["reason"]
            .as_str()
            .unwrap()
            .contains("may not run another as the user")
    );
    // The user's handler may.
    seen.borrow_mut().clear();
    let _ = app.run(&ESCALATE, Json::Null, Origin::User);
    settle(&mut app);
    assert_eq!(
        *seen.borrow(),
        vec![(ESCALATE, Origin::User), (INNER, Origin::User)]
    );
    // Outside a command, code names the origin it likes.
    seen.borrow_mut().clear();
    h.run_as(INNER, Json::Null, Origin::Script("s".into()));
    settle(&mut app);
    assert_eq!(*seen.borrow(), vec![(INNER, Origin::Script("s".into()))]);
}

#[test]
fn a_command_kept_from_agents_never_runs_for_one() {
    let (mut app, seen) = app();
    let h = app.handle().clone();
    // `App::run`: refused, journaled.
    let before = h.last_event();
    let r = app.run(&PRIVATE, Json::Null, agent()).now().unwrap();
    assert_eq!(r, Err("`chain.private` is not offered to agents".into()));
    let refused = h
        .events_since(before)
        .into_iter()
        .find(|e| e.name == "command.refused")
        .unwrap();
    assert_eq!(refused.source, "agent:tester");
    // `AppHandle::run_as`, queued: refused when it runs.
    h.run_as(PRIVATE, json!(null), agent());
    settle(&mut app);
    // Through a handler running for an agent: it inherits the agent.
    let _ = app.run(&REACH, Json::Null, agent());
    settle(&mut app);
    assert_eq!(*seen.borrow(), vec![(REACH, agent())]);
    // The user runs it.
    let r = app.run(&PRIVATE, Json::Null, Origin::User).now().unwrap();
    assert!(r.is_ok());
    let _ = app.run(&REACH, Json::Null, Origin::User);
    settle(&mut app);
    assert_eq!(
        seen.borrow()
            .iter()
            .filter(|(id, _)| *id == PRIVATE)
            .count(),
        2
    );
}
