//! `wait`'s contract (docs/runner.md, "Waiting on the application"):
//! the conditions read from and written to `bridge.wait`'s arguments,
//! the refusals, an event matched after `since` and by its cause, and
//! the answer's shape. Never blocks: no frame is run. Settling on idle
//! jobs is the jobs model's (tested with it).

use egui_pop_api::wait::{WaitFor, Waited, args_schema, result_schema};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::{Value as Json, json};

const PING: CommandId = CommandId::from_static("probe.ping");
const FAIL: CommandId = CommandId::from_static("probe.fail");

struct Probe;

impl Plugin for Probe {
    fn id(&self) -> PluginId {
        PluginId::from_static("probe")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(CommandSpec::new(PING, "Ping", "Probe"), |_| {
            Ok(json!("pong"))
        })?
        .command(
            CommandSpec::new(FAIL, "Fail", "Probe"),
            |_| Err("no".into()),
        )?;
        Ok(())
    }
}

fn app() -> App {
    App::builder()
        .plugin(Probe)
        .build(&egui::Context::default())
        .unwrap()
}

#[test]
fn each_condition_reads_back_what_it_writes() {
    let cases = [
        WaitFor::event("command.done"),
        WaitFor::Event {
            name: "command.done".into(),
            cause: Some(7),
        },
        WaitFor::Settled,
        WaitFor::Any(vec![
            WaitFor::event("command.failed"),
            WaitFor::Any(vec![WaitFor::Settled]),
        ]),
    ];
    for w in cases {
        assert_eq!(WaitFor::from_args(&w.to_args()), Ok(w.clone()), "{w:?}");
    }
    // A null cause is no cause.
    assert_eq!(
        WaitFor::from_args(&json!({ "event": "x", "cause": null })),
        Ok(WaitFor::event("x"))
    );
    assert_eq!(
        WaitFor::event("x").to_args(),
        json!({ "event": "x", "cause": null })
    );
}

#[test]
fn malformed_arguments_are_refused_with_the_reason() {
    let refused = |args: Json| WaitFor::from_args(&args).unwrap_err();
    let nothing = "give `event` (with an optional `cause`), `settled: true` or `any`";
    assert_eq!(refused(json!({})), nothing);
    assert_eq!(refused(json!(null)), nothing);
    assert_eq!(refused(json!({ "settled": false })), nothing);
    assert_eq!(refused(json!({ "event": 3 })), "`event` is not a name");
    assert_eq!(
        refused(json!({ "event": "x", "cause": "a" })),
        "`cause` is not a sequence number"
    );
    assert_eq!(
        refused(json!({ "event": "x", "cause": -1 })),
        "`cause` is not a sequence number"
    );
    assert_eq!(refused(json!({ "any": 1 })), "`any` is not a list");
    // One bad member refuses the whole list.
    assert_eq!(
        refused(json!({ "any": [{ "settled": true }, {}] })),
        nothing
    );
    // `any` wins over the other keys given beside it.
    assert_eq!(
        WaitFor::from_args(&json!({ "any": [], "event": "x" })),
        Ok(WaitFor::Any(vec![]))
    );
}

#[test]
fn an_event_matches_only_after_since_and_by_its_cause() {
    let mut app = app();
    let h = app.handle().clone();
    let before = h.last_event();
    let reply = app.run(&PING, Json::Null, Origin::User);
    assert_eq!(reply.now(), Some(Ok(json!("pong"))));
    let after = h.last_event();
    assert!(after > before);

    let done = WaitFor::event("command.done");
    let Some(Waited::Event(e)) = h.wait_met(&done, before) else {
        panic!("command.done after {before}");
    };
    assert_eq!(e.name, "command.done");
    assert!(e.seq > before);
    // Nothing new after the last event.
    assert_eq!(h.wait_met(&done, after), None);

    // By its cause: the dispatch it follows from, and no other.
    let cause = e.cause.expect("a result follows from its dispatch");
    let by_cause = |c| WaitFor::Event {
        name: "command.done".into(),
        cause: Some(c),
    };
    assert_eq!(
        h.wait_met(&by_cause(cause), before),
        Some(Waited::Event(e.clone()))
    );
    assert_eq!(h.wait_met(&by_cause(cause + 1_000), before), None);

    // `any` answers with the first condition that holds, in its order.
    let failed = WaitFor::event("command.failed");
    assert_eq!(h.wait_met(&failed, before), None);
    assert_eq!(
        h.wait_met(&WaitFor::Any(vec![failed.clone(), done.clone()]), before),
        Some(Waited::Event(e.clone()))
    );
    assert_eq!(h.wait_met(&WaitFor::Any(vec![]), before), None);

    // A later failure: `any` still names the first that holds.
    app.run(&FAIL, Json::Null, Origin::User);
    let Some(Waited::Event(f)) = h.wait_met(&WaitFor::Any(vec![failed, done]), before) else {
        panic!("a failure after {before}");
    };
    assert_eq!(f.name, "command.failed");
}

#[test]
fn bridge_wait_answers_met_or_not_with_the_last_sequence_number() {
    let mut app = app();
    let since = app.handle().last_event();
    app.run(&PING, Json::Null, Origin::User);
    let last = app.handle().last_event();

    let f = app.handle().debug_call(
        "bridge",
        "wait",
        &json!({ "event": "command.done", "since": since }),
    );
    assert!(f.ok, "{}", f.message);
    assert_eq!(f.data["met"], true);
    assert_eq!(f.data["by"], "event");
    assert_eq!(f.data["event"]["name"], "command.done");
    assert_eq!(f.data["last"], last);

    // Without `since`, only events after the call itself count.
    let f = app
        .handle()
        .debug_call("bridge", "wait", &json!({ "event": "command.done" }));
    assert!(f.ok, "{}", f.message);
    assert_eq!(f.data["met"], false);
    assert_eq!(f.data["by"], Json::Null);
    assert_eq!(f.data["event"], Json::Null);
    assert_eq!(f.data["last"], last, "{}", f.data);

    // Arguments it cannot read are refused with the reason.
    let f = app
        .handle()
        .debug_call("bridge", "wait", &json!({ "event": 1 }));
    assert!(!f.ok);
    assert!(f.message.contains("`event` is not a name"), "{}", f.message);
}

#[test]
fn the_schemas_name_every_argument_and_the_answer() {
    let args = args_schema();
    assert_eq!(args["type"], "object");
    let props = args["properties"].as_object().unwrap();
    let mut names: Vec<_> = props.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["any", "cause", "deadline_ms", "event", "settled", "since"]
    );
    let result = result_schema();
    assert_eq!(result["required"], json!(["met", "last"]));
    assert_eq!(
        result["properties"]["by"]["enum"],
        json!(["event", "settled", null])
    );
}
