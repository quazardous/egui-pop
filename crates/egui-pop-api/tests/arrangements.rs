//! Arrangements: saved in the store under a name, switched to (a saved
//! one or a template), reset to the template, deleted; switching is a
//! step a plugin may veto; a saved arrangement is repaired.

use egui_pop_api::arrangements::{self, ARRANGEMENTS, STEP_SWITCH};
use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::layout::{Layout, Node};
use egui_pop_model::panels::{Area, PanelSpec};

const LOG: PanelId = PanelId::from_static("demo.log");
const TREE: PanelId = PanelId::from_static("demo.tree");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |_, _| {})?
            .panel(PanelSpec::new(TREE, "Tree", Area::StartSide), |_, _| {})?;
        ctx.arrangement(
            "Focus",
            Layout {
                areas: vec![(Area::EndSide, Node::tab(TREE))],
                hidden: vec![LOG],
                ..Layout::default()
            },
        );
        ctx.before(STEP_SWITCH, |app, args| {
            if app.context().get("locked") == "true" {
                Before::Veto(format!("locked, not switching to {}", args["name"]))
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn app() -> App {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap()
}

#[test]
fn arrangements_are_saved_switched_reset_and_deleted() {
    let app = app();
    let h = app.handle();
    assert_eq!(arrangements::current(h), arrangements::DEFAULT);
    assert!(arrangements::names(h).contains(&"Focus".to_string()));
    // A template.
    assert!(arrangements::switch(h, "Focus").ok());
    assert_eq!(h.layout().area_of(&TREE), Some(Area::EndSide));
    assert!(!h.panel_visible(&LOG));
    assert_eq!(arrangements::current(h), "Focus");
    // Changed, and saved under a name of its own: kept in the store.
    h.move_panel(&TREE, Area::Bottom);
    assert!(arrangements::save(h, "Mine").ok());
    let text = egui_pop_core::lock(&h.store())
        .get(ARRANGEMENTS)
        .unwrap()
        .value
        .as_text()
        .unwrap()
        .to_string();
    assert!(text.contains("Mine"), "{text}");
    // Elsewhere, then back.
    assert!(arrangements::switch(h, "Focus").ok());
    assert_eq!(h.layout().area_of(&TREE), Some(Area::EndSide));
    assert!(arrangements::switch(h, "Mine").ok());
    assert_eq!(h.layout().area_of(&TREE), Some(Area::Bottom));
    // Reset: the template's, or every panel in its default area.
    assert!(arrangements::reset(h).ok());
    assert_eq!(h.layout().area_of(&TREE), Some(Area::StartSide));
    assert!(!arrangements::saved(h).contains_key("Mine"));
    // Delete.
    assert!(arrangements::save(h, "Other").ok());
    assert!(arrangements::delete(h, "Other").ok());
    assert!(!arrangements::delete(h, "Other").ok());
    assert!(!arrangements::switch(h, "Nowhere").ok());
}

#[test]
fn switching_is_a_step_a_plugin_may_veto() {
    let app = app();
    let h = app.handle();
    h.set_context("locked", "true");
    let out = arrangements::switch(h, "Focus");
    assert_eq!(out.vetoed(), Some("locked, not switching to \"Focus\""));
    assert_eq!(h.layout().area_of(&TREE), Some(Area::StartSide));
    let ev = h
        .events_since(0)
        .into_iter()
        .any(|e| e.name == "arrangement.switch.vetoed");
    assert!(ev);
}

#[test]
fn a_saved_arrangement_is_repaired_when_switched_to() {
    let app = app();
    let h = app.handle();
    // Saved by an earlier version, with a panel since gone.
    let mut layout = h.layout();
    layout
        .areas
        .push((Area::Center, Node::tab(PanelId::from_static("gone.panel"))));
    let doc = egui_pop_model::layout::schema().save(&layout).unwrap();
    let text = format!("{{\"Old\": {doc}}}");
    egui_pop_core::lock(&h.store())
        .set(egui_pop_model::values::Set::new(
            ARRANGEMENTS,
            egui_pop_model::values::Change::To(egui_pop_model::values::Value::Text(text)),
            egui_pop_model::commands::Origin::User,
        ))
        .unwrap();
    let out = arrangements::switch(h, "Old");
    let repaired = out.result().unwrap().as_ref().unwrap()["repaired"].clone();
    assert!(repaired.to_string().contains("gone.panel"), "{repaired}");
}
