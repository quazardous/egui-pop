//! `group`'s contract (docs/history.md, "One step for many writes"):
//! outside a command, with the default host, the writes of a group are
//! one undo step with its label and origin; a failed closure puts them
//! back and leaves no step; `write` carries the group's gesture. The
//! host's own groups and a panic are tested in `host.rs`.

use egui_pop_api::{App, AppHandle, Ctx, Plugin, PluginError};
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::PluginId;
use egui_pop_model::values::{Change, Descriptor, Set, Step, Value};

struct Swatch;

impl Plugin for Swatch {
    fn id(&self) -> PluginId {
        PluginId::from_static("swatch")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.value("swatch/hue", Descriptor::float("Hue", 0.0, 360.0, 200.0))?
            .value("swatch/size", Descriptor::float("Size", 40.0, 300.0, 160.0))?;
        Ok(())
    }
}

fn app() -> App {
    App::builder()
        .plugin(Swatch)
        .build(&egui::Context::default())
        .unwrap()
}

fn steps(h: &AppHandle) -> Vec<Step> {
    let store = h.store();
    egui_pop_core::lock(&store).undo_steps().to_vec()
}

fn to(v: f64) -> Change {
    Change::To(Value::Float(v))
}

#[test]
fn a_group_that_succeeds_is_one_step_with_its_label_and_origin() {
    let app = app();
    let h = app.handle().clone();
    let origin = Origin::Script("tidy".into());
    let out = h.group(&origin, "Tidy", |g| {
        assert_eq!(g.origin(), &origin);
        assert!(g.gesture().is_some(), "the default host groups");
        g.set("swatch/hue", to(30.0))?;
        g.set("swatch/size", to(80.0))?;
        Ok::<_, egui_pop_model::values::Refused>(7)
    });
    assert_eq!(out.unwrap(), 7, "the closure's value comes back");
    assert_eq!(h.value::<f64>("swatch/hue"), Some(30.0));
    assert_eq!(h.value::<f64>("swatch/size"), Some(80.0));
    let steps = steps(&h);
    assert_eq!(steps.len(), 1, "{steps:?}");
    assert_eq!(steps[0].label, "Tidy");
    assert_eq!(steps[0].origin, origin);
    let paths: Vec<_> = steps[0].changes.iter().map(|c| c.0.as_str()).collect();
    assert_eq!(paths, ["swatch/hue", "swatch/size"]);
}

#[test]
fn a_group_that_fails_puts_its_writes_back_and_leaves_no_step() {
    let app = app();
    let h = app.handle().clone();
    let out: Result<(), String> = h.group(&Origin::User, "Half", |g| {
        g.set("swatch/hue", to(10.0)).map_err(|e| e.to_string())?;
        assert_eq!(
            h.value::<f64>("swatch/hue"),
            Some(10.0),
            "written meanwhile"
        );
        Err("stopped".into())
    });
    assert_eq!(out, Err("stopped".into()), "the closure's error comes back");
    assert_eq!(h.value::<f64>("swatch/hue"), Some(200.0));
    assert!(steps(&h).is_empty(), "{:?}", steps(&h));
}

#[test]
fn a_refused_write_inside_ends_the_group_as_the_closure_says() {
    let app = app();
    let h = app.handle().clone();
    // The closure decides: it may go on after a refusal, and the group
    // keeps what was written.
    let out: Result<(), ()> = h.group(&Origin::User, "Partly", |g| {
        assert!(g.set("swatch/nowhere", to(1.0)).is_err());
        g.set("swatch/hue", to(50.0)).map(|_| ()).map_err(|_| ())
    });
    assert_eq!(out, Ok(()));
    assert_eq!(h.value::<f64>("swatch/hue"), Some(50.0));
    let steps = steps(&h);
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].label, "Partly");
}

#[test]
fn write_carries_the_groups_gesture_and_a_group_with_no_write_leaves_no_step() {
    let app = app();
    let h = app.handle().clone();
    let gesture = h
        .group(&Origin::User, "Raw", |g| {
            g.write(Set::new("swatch/hue", to(90.0), Origin::User))?;
            g.write(Set::new("swatch/size", to(90.0), Origin::User))?;
            Ok::<_, egui_pop_model::values::Refused>(g.gesture())
        })
        .unwrap();
    let steps_now = steps(&h);
    assert_eq!(steps_now.len(), 1, "{steps_now:?}");
    assert_eq!(steps_now[0].label, "Raw");
    assert_eq!(steps_now[0].group, gesture.map(|g| g.0));

    h.group(&Origin::User, "Nothing", |_| Ok::<_, ()>(()))
        .unwrap();
    assert_eq!(steps(&h).len(), 1, "an empty group is no step");
}

#[test]
fn groups_one_after_the_other_are_steps_in_order() {
    let app = app();
    let h = app.handle().clone();
    for (label, v) in [("One", 1.0), ("Two", 2.0)] {
        h.group(&Origin::User, label, |g| g.set("swatch/hue", to(v)))
            .unwrap();
    }
    let labels: Vec<_> = steps(&h).into_iter().map(|s| s.label).collect();
    assert_eq!(labels, ["One", "Two"]);
}
