//! Status items (`docs/status-bar.md`): alignment and priority, live
//! states (busy, error and warning counts), a popover per item, rich
//! hover cards, and what agents read and run.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::host::Reply;
use egui_pop_api::strips::{
    self, Alignment, CMD_HOVER_ITEM, CMD_OPEN_ITEM, Indicator, IndicatorState, StripsPlugin, Tone,
};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::a11y;
use egui_pop_core::hover_card::Card;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::{Value as Json, json};

const CLEAR: CommandId = CommandId::from_static("demo.problems.clear");
const RECONNECT: CommandId = CommandId::from_static("demo.node.reconnect");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CLEAR, "Clear the problems", "Demo"),
            |call| {
                call.app.publish("demo", None, "problem.clear", Json::Null);
                Ok(Json::Null)
            },
        )?
        .command(CommandSpec::new(RECONNECT, "Reconnect", "Demo"), |call| {
            call.app.set_context("demoReconnected", "true");
            Ok(Json::Null)
        })?;
        ctx.indicator(
            Indicator::new("demo.problems", "Problems in the catalogue", "problem.")
                .align(Alignment::Start)
                .reduce(|state, e| {
                    match e.name.as_str() {
                        "problem.error" => state.errors += 1,
                        "problem.warning" => state.warnings += 1,
                        "problem.clear" => {
                            state.errors = 0;
                            state.warnings = 0;
                        }
                        _ => {}
                    }
                    state.tone = if state.errors > 0 {
                        Tone::Danger
                    } else if state.warnings > 0 {
                        Tone::Warning
                    } else {
                        Tone::Neutral
                    };
                })
                .popover(|ui, app| {
                    ui.label("The problems, one per line");
                    if ui.button("Dismiss all").clicked() {
                        app.run(CLEAR, Json::Null);
                    }
                })
                .hover(|state, _| {
                    Card::new()
                        .text("**Problems** in the catalogue")
                        .row("Errors", state.errors.to_string())
                        .link("Clear them", CLEAR, Json::Null)
                }),
        )
        .indicator(
            Indicator::new("demo.node", "Node", "node.")
                .priority(10)
                .weight(5)
                .hover(|_, _| {
                    Card::new().text("Connected to `node-1`").link(
                        "Reconnect",
                        RECONNECT,
                        Json::Null,
                    )
                }),
        )
        .indicator(Indicator::new(
            "demo.light",
            "A rather long light item",
            "light.",
        ))
        .indicator(
            Indicator::new("demo.sync", "Syncing", "sync.")
                .priority(-1)
                .initial(IndicatorState {
                    text: "Syncing".into(),
                    busy: Some(0.0),
                    progress: Some(0.4),
                    ..IndicatorState::default()
                }),
        );
        Ok(())
    }
}

fn harness(width: f32) -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(StripsPlugin::status_only())
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(width, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn entry(app: &egui_pop_api::AppHandle, id: &str) -> strips::StatusEntry {
    strips::status(app)
        .into_iter()
        .find(|e| e.id == id)
        .unwrap()
}

#[test]
fn start_items_follow_the_bars_start_and_end_items_sit_at_its_end() {
    let h = harness(1000.0);
    let problems = h.get_by_label("Problems in the catalogue").rect();
    let light = h.get_by_label("A rather long light item").rect();
    assert!(problems.center().x < 500.0, "{problems:?}");
    assert!(light.center().x > 500.0, "{light:?}");
    let app = h.state().handle().clone();
    assert_eq!(entry(&app, "demo.problems").align, Alignment::Start);
    assert_eq!(entry(&app, "demo.light").align, Alignment::End);
    // Busy, with progress: a state agents read too.
    let sync = entry(&app, "demo.sync");
    assert!(sync.busy);
    assert_eq!(sync.progress, Some(0.4));
}

#[test]
fn error_and_warning_counts_show_with_their_icons_and_turning_to_errors_is_announced_once() {
    let mut h = harness(1000.0);
    let app = h.state().handle().clone();
    app.publish("host", None, "problem.error", Json::Null);
    app.publish("host", None, "problem.error", Json::Null);
    app.publish("host", None, "problem.warning", Json::Null);
    h.run();
    // Never colour alone: an icon named by its count, and the count.
    assert!(h.query_by_label("Errors: 2").is_some());
    assert!(h.query_by_label("Warnings: 1").is_some());
    let item = h.get_by_label("Problems in the catalogue");
    let description = item.accesskit_node().description().unwrap_or_default();
    assert!(description.contains("Errors: 2"), "{description}");
    assert!(description.contains("Warnings: 1"), "{description}");
    let problems = entry(&app, "demo.problems");
    assert_eq!((problems.errors, problems.warnings), (2, 1));
    assert_eq!(problems.tone, Tone::Danger);
    let announced = |h: &Harness<'_, App>| {
        a11y::announcements(&h.ctx)
            .iter()
            .filter(|a| a.text.starts_with("Problems in the catalogue:"))
            .count()
    };
    assert_eq!(announced(&h), 1);
    // More errors: no new announcement.
    app.publish("host", None, "problem.error", Json::Null);
    h.run();
    assert!(h.query_by_label("Errors: 3").is_some());
    assert_eq!(announced(&h), 1);
}

#[test]
fn a_click_opens_the_items_popover_and_escape_or_a_click_outside_closes_it() {
    let mut h = harness(1000.0);
    let app = h.state().handle().clone();
    app.publish("host", None, "problem.error", Json::Null);
    h.run();
    h.get_by_label("Problems in the catalogue").click();
    h.run();
    assert!(h.query_by_label("Dismiss all").is_some());
    assert_eq!(strips::open_item(&app).as_deref(), Some("demo.problems"));
    assert!(entry(&app, "demo.problems").open);
    // Its content acts.
    h.get_by_label("Dismiss all").click();
    h.run();
    h.run();
    assert_eq!(entry(&app, "demo.problems").errors, 0);
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.query_by_label("Dismiss all").is_none());
    assert_eq!(strips::open_item(&app), None);
    // A second click closes it as well: a press on the item, the
    // popover's anchor, is not a click outside it (which would close it
    // on the press for the click to open it again).
    h.get_by_label("Problems in the catalogue").click();
    h.run();
    assert!(h.query_by_label("Dismiss all").is_some());
    h.get_by_label("Problems in the catalogue").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Dismiss all").is_none());
    // A click outside.
    h.get_by_label("Problems in the catalogue").click();
    h.run();
    h.get_by_label("A rather long light item").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Dismiss all").is_none());
}

#[test]
fn the_keyboard_opens_the_popover_and_tab_goes_into_it() {
    let mut h = harness(1000.0);
    h.get_by_label("Problems in the catalogue").focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    assert!(h.query_by_label("Dismiss all").is_some());
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.get_by_label("Dismiss all").is_focused());
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.query_by_label("Dismiss all").is_none());
}

#[test]
fn on_a_narrow_bar_items_give_way_by_priority_into_a_menu() {
    let mut h = harness(150.0);
    let app = h.state().handle().clone();
    // The node's priority keeps it; the others are in `…`.
    assert!(h.query_by_label("Node").is_some());
    assert!(h.query_by_label("A rather long light item").is_none());
    assert_eq!(
        entry(&app, "demo.light").hidden_by,
        Some(strips::HiddenBy::Narrow)
    );
    assert!(entry(&app, "demo.node").hidden_by.is_none());
    h.get_by_label("More status items").click();
    h.run();
    // An entry acts as a click on its item: the popover opens.
    h.get_by_label("Problems in the catalogue").click();
    h.run();
    h.run();
    assert!(h.query_by_label("Dismiss all").is_some());
    assert_eq!(strips::open_item(&app).as_deref(), Some("demo.problems"));
}

#[test]
fn the_hover_card_opens_from_the_keyboard_and_its_links_run_commands() {
    let mut h = harness(1000.0);
    let app = h.state().handle().clone();
    // The card is the item's description, before any hover.
    let node = h.get_by_label("Node");
    let description = node.accesskit_node().description().unwrap_or_default();
    assert!(description.contains("Connected to node-1"), "{description}");
    // Ctrl+K Ctrl+I on the focused item.
    h.get_by_label("Node").focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::I);
    h.run();
    h.run();
    assert!(h.query_by_label("Reconnect").is_some());
    assert!(h.get_by_label("Reconnect").is_focused());
    h.get_by_label("Reconnect").click();
    h.run();
    h.run();
    assert_eq!(app.context().get("demoReconnected"), "true");
    assert!(h.query_by_label("Reconnect").is_none());
}

#[test]
fn agents_read_the_states_and_cards_and_open_popovers() {
    let mut h = harness(1000.0);
    let app = h.state().handle().clone();
    app.publish("host", None, "problem.warning", Json::Null);
    h.run();
    let snapshot = app.debug_snapshot()["strips"].clone();
    let problems = snapshot["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "demo.problems")
        .cloned()
        .unwrap();
    assert_eq!(problems["warnings"], 1);
    assert_eq!(problems["align"], "start");
    assert_eq!(problems["popover"], true);
    assert_eq!(problems["card"]["blocks"][1]["rows"][0]["value"], "0");
    assert_eq!(
        problems["card"]["blocks"][2]["links"][0]["command"],
        "demo.problems.clear"
    );
    let run = |h: &mut Harness<'_, App>, command: &CommandId, args: Json| {
        h.state_mut()
            .run(command, args, Origin::Agent("test".into()))
    };
    let reply = run(&mut h, &CMD_OPEN_ITEM, json!({ "id": "demo.problems" }));
    assert!(matches!(reply, Reply::Done(Ok(_))));
    h.run();
    h.run();
    assert!(h.query_by_label("Dismiss all").is_some());
    assert_eq!(app.debug_snapshot()["strips"]["open"], "demo.problems");
    let reply = run(
        &mut h,
        &CMD_OPEN_ITEM,
        json!({ "id": "demo.problems", "open": false }),
    );
    assert!(matches!(reply, Reply::Done(Ok(_))));
    h.run();
    h.run();
    assert!(h.query_by_label("Dismiss all").is_none());
    // An item without a popover, or a card, says so.
    let reply = run(&mut h, &CMD_OPEN_ITEM, json!({ "id": "demo.light" }));
    assert!(matches!(reply, Reply::Done(Err(_))));
    let reply = run(&mut h, &CMD_HOVER_ITEM, json!({ "id": "demo.light" }));
    assert!(matches!(reply, Reply::Done(Err(_))));
    // An agent opens a card by id.
    let reply = run(&mut h, &CMD_HOVER_ITEM, json!({ "id": "demo.problems" }));
    assert!(matches!(reply, Reply::Done(Ok(_))));
    h.run();
    h.run();
    assert!(h.query_by_label("Clear them").is_some());
}
