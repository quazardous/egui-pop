//! The custom dialog (ADR 0013): a body of its own drawn every frame,
//! its values merged into the command's arguments, Enter and Escape
//! with the keys the body claims, a mount recorded while it is open, a
//! body that does not fit refused before anything opens, and the debug
//! bridge seeing and answering it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::dialogs::{self, Answer, Backdrop, Confirm, Custom, CustomPass, DialogSpec};
use egui_pop_api::mounts::CUSTOM_DIALOG;
use egui_pop_api::{App, AppHandle, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use egui_pop_model::mount::{ClaimKey, Display, Mountable};
use serde_json::{Value as Json, json};

const GO: CommandId = CommandId::from_static("doc.go");

type Runs = Rc<RefCell<Vec<(Json, Origin)>>>;

struct Doc(Runs);

impl Plugin for Doc {
    fn id(&self) -> PluginId {
        PluginId::from_static("doc")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let runs = self.0.clone();
        ctx.command(CommandSpec::new(GO, "Go", "Doc"), move |call| {
            runs.borrow_mut()
                .push((call.args.clone(), call.origin.clone()));
            Ok(Json::Null)
        })?;
        Ok(())
    }
}

struct Scene {
    h: Harness<'static, App>,
    runs: Runs,
    d: dialogs::Dialogs,
    app: AppHandle,
}

fn scene() -> Scene {
    let runs: Runs = Rc::default();
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Doc(runs.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let handle = app.handle().clone();
    let d = (*app.handle().service(&dialogs::DIALOGS).unwrap()).clone();
    let h = Harness::builder()
        .with_size(egui::vec2(900.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    Scene {
        h,
        runs,
        d,
        app: handle,
    }
}

/// A body drawing a label, counting its frames and the Enter presses it
/// sees, giving a value.
fn counting(
    frames: Rc<Cell<u32>>,
    enters: Rc<Cell<u32>>,
) -> impl FnMut(&mut egui::Ui, &AppHandle, &mut CustomPass) {
    move |ui, _, pass| {
        frames.set(frames.get() + 1);
        ui.label("The body of the dialog");
        if ui.input(|i| i.key_pressed(Key::Enter)) {
            enters.set(enters.get() + 1);
        }
        pass.value("frames", json!(frames.get()));
        pass.describe(json!({ "drawn": frames.get() }));
    }
}

#[test]
fn a_custom_body_is_drawn_every_frame_and_enter_accepts_with_its_values() {
    let mut s = scene();
    let (frames, enters) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    s.d.open_custom(
        &s.app,
        Custom::new("Export preview", "Export 3 images", GO)
            .args(json!({ "what": "export" }))
            .size(400.0, 240.0),
        counting(frames.clone(), enters.clone()),
    )
    .unwrap();
    s.h.run();
    assert!(s.h.query_by_label("The body of the dialog").is_some());
    assert!(s.h.query_by_label("Export 3 images").is_some());
    assert!(s.h.query_by_label("Cancel").is_some());
    let before = frames.get();
    s.h.run();
    assert!(frames.get() > before, "drawn every frame");
    // The debug bridge sees its body's description and values.
    let current = s.d.describe();
    assert_eq!(current["kind"], "custom");
    assert_eq!(current["verb"], "Export 3 images");
    assert!(current["body"]["drawn"].as_u64().unwrap() > 0);
    assert!(current["values"]["frames"].is_number());
    s.h.key_press(Key::Enter);
    s.h.run();
    assert!(s.d.is_empty());
    let runs = s.runs.borrow();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].0["what"], "export");
    assert!(runs[0].0["frames"].is_number(), "{}", runs[0].0);
}

#[test]
fn a_body_claiming_enter_keeps_it_and_ctrl_enter_accepts() {
    let mut s = scene();
    let (frames, enters) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    s.d.open_custom(
        &s.app,
        Custom::new("Rename", "Rename", GO).claims(&[ClaimKey::Enter]),
        counting(frames, enters.clone()),
    )
    .unwrap();
    s.h.run();
    // Enter goes to the body; the dialog stays.
    s.h.key_press(Key::Enter);
    s.h.run();
    assert_eq!(enters.get(), 1);
    assert_eq!(s.d.len(), 1);
    assert!(s.runs.borrow().is_empty());
    // Ctrl+Enter accepts from anywhere.
    s.h.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    s.h.run();
    assert!(s.d.is_empty());
    assert_eq!(s.runs.borrow().len(), 1);
}

#[test]
fn escape_cancels_a_custom_dialog() {
    let mut s = scene();
    let (frames, enters) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    s.d.open_custom(
        &s.app,
        Custom::new("Preview", "Apply", GO),
        counting(frames, enters),
    )
    .unwrap();
    s.h.run();
    s.h.key_press(Key::Escape);
    s.h.run();
    assert!(s.d.is_empty());
    assert!(s.runs.borrow().is_empty());
    assert_eq!(s.d.answered()[0].1, Answer::Cancel);
}

#[test]
fn a_body_claiming_escape_takes_it_while_it_has_something_to_cancel() {
    let mut s = scene();
    let dragging = Rc::new(Cell::new(true));
    let cancelled = Rc::new(Cell::new(0));
    let (d2, c2) = (dragging.clone(), cancelled.clone());
    s.d.open_custom(
        &s.app,
        DialogSpec::new(
            Custom::new("Crop", "Crop", GO).needs(
                Mountable::new("demo.cropper", "Cropper")
                    .claims(&[ClaimKey::Escape])
                    .drag()
                    .fills(),
            ),
        )
        .backdrop(Backdrop::Clear),
        move |ui, _, pass| {
            ui.label("Drag the crop");
            if d2.get() {
                // A drag in progress: this Escape cancels it.
                pass.claim_escape();
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    d2.set(false);
                    c2.set(c2.get() + 1);
                }
            }
        },
    )
    .unwrap();
    s.h.run();
    // Escape cancels the drag, not the dialog.
    s.h.key_press(Key::Escape);
    s.h.run();
    assert_eq!(cancelled.get(), 1);
    assert!(!dragging.get());
    assert_eq!(s.d.len(), 1);
    // Nothing left to cancel: Escape cancels the dialog.
    s.h.key_press(Key::Escape);
    s.h.run();
    assert!(s.d.is_empty());
    assert!(s.runs.borrow().is_empty());
}

#[test]
fn the_body_is_mounted_while_open_and_a_body_that_does_not_fit_opens_nothing() {
    let mut s = scene();
    let needs = Mountable::new("demo.preview", "Preview")
        .display(Display::WideWanted)
        .fills();
    s.d.open_custom(
        &s.app,
        Custom::new("Preview", "Apply", GO).needs(needs),
        |ui, _, _| {
            ui.label("A preview");
        },
    )
    .unwrap();
    s.h.run();
    let mounts = s.app.mounts();
    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0].component, "demo.preview");
    assert_eq!(mounts[0].host, CUSTOM_DIALOG);
    // sRGB first: wide gamut wanted fits, under the sRGB condition.
    assert_eq!(mounts[0].fit.cell(), "if c3");
    s.h.key_press(Key::Escape);
    s.h.run();
    assert!(s.d.is_empty());
    assert!(s.app.mounts().is_empty(), "unmounted when answered");
    // A mastering view requires wide gamut: refused, reported, nothing
    // opens.
    let mastering = Mountable::new("demo.mastering", "Mastering").display(Display::WideRequired);
    let why =
        s.d.open_custom(
            &s.app,
            Custom::new("Master", "Apply", GO).needs(mastering),
            |_, _, _| {},
        )
        .unwrap_err();
    assert!(why.contains("U2"), "{why}");
    assert!(s.d.is_empty());
    assert_eq!(s.app.diagnostics().last().unwrap().code, "mount-refused");
    // Only a custom dialog takes a body.
    assert!(
        s.d.open_custom(&s.app, Confirm::new("Go?", "Go", GO), |_, _, _| {})
            .is_err()
    );
    assert!(s.d.is_empty());
}

#[test]
fn an_agent_answers_a_custom_dialog_and_a_body_not_ready_is_not_accepted() {
    let mut s = scene();
    let ready = Rc::new(Cell::new(false));
    let r2 = ready.clone();
    s.d.open_custom(
        &s.app,
        Custom::new("Export", "Export", GO),
        move |ui, _, pass| {
            ui.label("Choose a size first");
            pass.ready(r2.get());
            pass.value("width", json!(640));
        },
    )
    .unwrap();
    s.h.run();
    // Not ready: Enter does nothing, an agent's accept is refused.
    s.h.key_press(Key::Enter);
    s.h.run();
    assert_eq!(s.d.len(), 1);
    s.d.answer(
        Answer::Accept {
            values: serde_json::Map::new(),
        },
        Origin::Agent("test".into()),
    )
    .unwrap();
    s.h.run();
    assert_eq!(s.d.len(), 1);
    assert!(s.runs.borrow().is_empty());
    ready.set(true);
    s.h.run();
    let mut values = serde_json::Map::new();
    values.insert("height".into(), json!(480));
    s.d.answer(Answer::Accept { values }, Origin::Agent("test".into()))
        .unwrap();
    s.h.run();
    assert!(s.d.is_empty());
    let runs = s.runs.borrow();
    assert_eq!(runs[0].0, json!({ "width": 640, "height": 480 }));
    assert_eq!(runs[0].1, Origin::Agent("test".into()));
}

#[test]
fn a_closing_custom_dialog_has_a_close_button() {
    let mut s = scene();
    s.d.open_custom(&s.app, Custom::closing("About the image"), |ui, _, _| {
        ui.label("4096 by 3072");
    })
    .unwrap();
    s.h.run();
    s.h.get_by_label("Close").click();
    s.h.run();
    assert!(s.d.is_empty());
    assert!(s.runs.borrow().is_empty());
}
