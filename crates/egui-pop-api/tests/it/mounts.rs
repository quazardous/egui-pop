//! The mount contract in the application (ADR 0013): where a component
//! can go, a mount recorded with its conditions or refused with its
//! rule (reported, nothing mounted), components and hosts declared by
//! plugins, panels refused by their content, and the `mounts` collector.

use egui_pop_api::mounts::{self, JOBS_CENTRE, PANEL, POPOVER, STATUS_BAR};
use egui_pop_api::steps::Outcome;
use egui_pop_api::workbench::Change;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::mount::{HostOffer, HostSize, Mountable, Rule};
use egui_pop_model::panels::{Area, PanelHost, PanelSpec, Place};
use serde_json::json;

const LOG: PanelId = PanelId::from_static("demo.log");
const ASK: PanelId = PanelId::from_static("demo.ask");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.mountable(Mountable::new("demo.console", "Console").session().fills());
        ctx.host_offer(HostOffer::new("demo.ticker", "T", "Ticker").size(HostSize::Row));
        ctx.panel(
            PanelSpec::new(LOG, "Log", Area::Bottom)
                .floatable(true)
                .content(Mountable::new("demo.console", "Console").session().fills()),
            |ui, _| {
                ui.label("log body");
            },
        )?;
        ctx.panel(
            PanelSpec::new(ASK, "Question", Area::EndSide)
                .floatable(true)
                .content(Mountable::new("demo.question", "Question").asks()),
            |ui, _| {
                ui.label("Sure?");
            },
        )?;
        Ok(())
    }
}

/// Declares a component of an id `Demo` has: ignored, said.
struct Other;

impl Plugin for Other {
    fn id(&self) -> PluginId {
        PluginId::from_static("other")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![PluginId::from_static("demo")]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.mountable(Mountable::new("demo.console", "Console again"));
        // A built-in id is not taken either.
        ctx.host_offer(HostOffer::new("panel", "X", "Another panel"));
        Ok(())
    }
}

fn app() -> App {
    App::builder()
        .plugin(Demo)
        .plugin(Other)
        .build(&egui::Context::default())
        .unwrap()
}

#[test]
fn where_can_gives_every_host_and_its_verdict() {
    let app = app();
    let h = app.handle();
    let verdicts = h.where_can(JOBS_CENTRE).unwrap();
    let hosts: Vec<&str> = verdicts.iter().map(|v| v.host.as_str()).collect();
    assert_eq!(&hosts[..3], [PANEL, mounts::PALETTE, mounts::WINDOW]);
    assert!(hosts.contains(&"demo.ticker"), "a plugin's host: {hosts:?}");
    let of = |host: &str| verdicts.iter().find(|v| v.host == host).unwrap().fit.cell();
    assert_eq!(of(PANEL), "if c7");
    assert_eq!(of(mounts::PALETTE), "if c1 c7");
    assert_eq!(of(STATUS_BAR), "D1");
    assert_eq!(of(mounts::DIALOG), "D6");
    assert!(h.where_can("nothing").is_err());
}

#[test]
fn a_mount_is_recorded_with_its_conditions_or_refused_with_its_rule() {
    let app = app();
    let h = app.handle();
    let fit = h.mount(JOBS_CENTRE, POPOVER).unwrap();
    assert_eq!(fit.cell(), "if c1 c7");
    assert_eq!(h.mounts().len(), 1);
    assert_eq!(h.mounts()[0].host, POPOVER);
    // The same mount again is one mount.
    h.mount(JOBS_CENTRE, POPOVER).unwrap();
    assert_eq!(h.mounts().len(), 1);
    // A session never goes in a popover: refused, reported, not mounted.
    let before = h.diagnostics().len();
    let why = h.mount("demo.console", POPOVER).unwrap_err();
    assert!(why.contains("R3, D3"), "{why}");
    let d = h.diagnostics();
    assert_eq!(d.len(), before + 1);
    assert_eq!(d[before].code, "mount-refused");
    assert_eq!(d[before].subject, "demo.console in popover");
    assert_eq!(h.mounts().len(), 1);
    // A host or a component nobody declared.
    assert!(h.mount("demo.console", "nowhere").is_err());
    assert_eq!(h.diagnostics().last().unwrap().code, "mount-unknown");
    assert!(h.unmount(JOBS_CENTRE, POPOVER));
    assert!(!h.unmount(JOBS_CENTRE, POPOVER));
    assert!(h.mounts().is_empty());
}

#[test]
fn a_second_declaration_of_a_component_is_ignored_and_said() {
    let app = app();
    let h = app.handle();
    let consoles: Vec<Mountable> = h
        .mountables()
        .into_iter()
        .filter(|m| m.id == "demo.console")
        .collect();
    assert_eq!(consoles.len(), 1);
    assert_eq!(consoles[0].name, "Console");
    assert!(
        h.diagnostics()
            .iter()
            .any(|d| d.code == "duplicate-id" && d.subject == "demo.console")
    );
    assert!(
        h.diagnostics()
            .iter()
            .any(|d| d.code == "duplicate-id" && d.subject == "panel")
    );
    assert_eq!(
        h.host_offers().iter().filter(|o| o.id == "panel").count(),
        1
    );
}

#[test]
fn a_panel_is_refused_a_host_its_content_does_not_fit() {
    let app = app();
    let h = app.handle();
    // A question floats nowhere: palettes are never modal (R4).
    let out = h
        .change_workbench(Change::Float(ASK, [40.0, 40.0]))
        .unwrap();
    assert!(
        matches!(out, Outcome::Done(Err(ref e)) if e.starts_with("Question cannot go in floating palette:") && e.contains("R4, D4")),
        "{out:?}"
    );
    assert!(h.layout().floating_of(&ASK).is_none());
    // The console holds a session: fine in a palette, never in a popover.
    let console = PanelSpec::new(LOG, "Log", Area::Bottom)
        .floatable(true)
        .content(Mountable::new("demo.console", "Console").session().fills());
    assert_eq!(console.refuses(&mounts::panel_host(Place::Floating)), None);
    let popover = PanelHost {
        place: Place::Floating,
        offer: mounts::popover(),
    };
    assert!(console.refuses(&popover).unwrap().contains("R3, D3"));
    assert_eq!(mounts::panel_host(Place::Window).offer.id, mounts::WINDOW);
}

#[test]
fn the_mounts_collector_shows_declarations_and_answers_verdicts() {
    let mut app = app();
    app.handle().mount(JOBS_CENTRE, PANEL).unwrap();
    let snapshot = app.handle().debug_snapshot();
    let m = &snapshot["mounts"];
    assert!(
        m["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "demo.console")
    );
    assert_eq!(m["mounted"][0]["component"], JOBS_CENTRE);
    let f = app.debug_call("mounts", "where_can", &json!({ "component": "sliders" }));
    assert!(f.ok, "{}", f.message);
    assert!(f.data.as_array().unwrap().len() >= 9);
    let f = app.debug_call(
        "mounts",
        "check",
        &json!({ "component": "demo.console", "host": "popover" }),
    );
    assert!(f.ok, "{}", f.message);
    assert_eq!(f.data["cell"], "D3");
    assert_eq!(f.data["fit"]["refusal"]["rule"], json!(Rule::Session));
    assert!(f.data["reason"].as_str().unwrap().contains("Instead"));
    let f = app.debug_call("mounts", "where_can", &json!({}));
    assert!(!f.ok);
}
