//! Undo and redo through the host: egui-pop owns the keys, the commands
//! and the steps; the host says what an undo is; hooks see, stop or
//! replace each run, and the journal says how it went.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_pop_api::app::{CMD_REDO, CMD_UNDO, STEP_UNDO};
use egui_pop_api::host::{Dispatch, Host, Journal, Reply, Waker};
use egui_pop_api::settings_window::CMD_VALUE_UNDO;
use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::binding::SharedStore;
use egui_pop_core::lock;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::PluginId;
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::{Value as Json, json};

/// The application's history: labels of the steps done and undone.
#[derive(Clone, Default)]
struct History {
    done: Arc<Mutex<Vec<String>>>,
    undone: Arc<Mutex<Vec<String>>>,
    calls: Arc<Mutex<Vec<String>>>,
}

#[derive(Default)]
struct Kernel {
    journal: Journal,
    store: SharedStore,
    history: History,
}

impl Host for Kernel {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        Reply::Done(d.run_local().unwrap_or(Err("no handler".into())))
    }
    fn journal(&self) -> &Journal {
        &self.journal
    }
    fn journal_mut(&mut self) -> &mut Journal {
        &mut self.journal
    }
    fn store(&self) -> SharedStore {
        self.store.clone()
    }
    fn set_waker(&mut self, _: Waker) {}
    fn can_undo(&self) -> bool {
        !self.history.done.lock().unwrap().is_empty()
    }
    fn can_redo(&self) -> bool {
        !self.history.undone.lock().unwrap().is_empty()
    }
    fn undo_label(&self) -> Option<String> {
        self.history.done.lock().unwrap().last().cloned()
    }
    fn redo_label(&self) -> Option<String> {
        self.history.undone.lock().unwrap().last().cloned()
    }
    fn undo(&mut self, _: &Origin) -> Result<Option<String>, String> {
        self.history.calls.lock().unwrap().push("host undo".into());
        let step = self.history.done.lock().unwrap().pop();
        if let Some(s) = &step {
            self.history.undone.lock().unwrap().push(s.clone());
        }
        Ok(step)
    }
    fn redo(&mut self, _: &Origin) -> Result<Option<String>, String> {
        let step = self.history.undone.lock().unwrap().pop();
        if let Some(s) = &step {
            self.history.done.lock().unwrap().push(s.clone());
        }
        Ok(step)
    }
}

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

fn kernel(steps: &[&str]) -> (Kernel, History) {
    let k = Kernel::default();
    k.history
        .done
        .lock()
        .unwrap()
        .extend(steps.iter().map(|s| s.to_string()));
    let h = k.history.clone();
    (k, h)
}

fn harness(app: App) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

#[test]
fn ctrl_z_undoes_the_host_s_step_and_the_journal_names_it() {
    let (k, history) = kernel(&["drag exposure"]);
    let app = App::builder()
        .host(k)
        .build(&egui::Context::default())
        .unwrap();
    let a = app.handle().clone();
    assert!(a.can_undo());
    let undo = a.command(&CMD_UNDO).unwrap();
    assert_eq!(a.command_title(&undo), "Undo drag exposure");
    let mut h = harness(app);
    let before = a.last_event();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(*history.undone.lock().unwrap(), ["drag exposure"]);
    let names: Vec<String> = a.events_since(before).into_iter().map(|e| e.name).collect();
    assert!(names.contains(&"edit.undo".to_string()), "{names:?}");
    let undone = a
        .events_since(before)
        .into_iter()
        .find(|e| e.name == "edit.undone")
        .unwrap();
    assert_eq!(undone.data["label"], "drag exposure");
    // Nothing left to undo: the command does not apply; redo does.
    assert!(!a.can_undo());
    assert!(a.when_context().get("canUndo") == "false");
    assert_eq!(
        a.command_title(&a.command(&CMD_REDO).unwrap()),
        "Redo drag exposure"
    );
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run();
    assert_eq!(*history.done.lock().unwrap(), ["drag exposure"]);
    assert!(
        a.events_since(before)
            .iter()
            .any(|e| e.name == "edit.redone" && e.data["label"] == "drag exposure")
    );
}

#[test]
fn hooks_run_in_order_and_a_veto_stops_the_undo() {
    let (k, history) = kernel(&["crop", "rotate"]);
    let calls = history.calls.clone();
    let gesture = Rc::new(RefCell::new(false));
    let (c1, c2, g) = (calls.clone(), calls.clone(), gesture.clone());
    let mut app = App::builder()
        .host(k)
        .plugin(P(
            "watcher",
            Box::new(move |ctx| {
                let (c1, c2, g) = (c1.clone(), c2.clone(), g.clone());
                ctx.before(STEP_UNDO, move |_, args| {
                    c1.lock()
                        .unwrap()
                        .push(format!("before {}", args["label"].as_str().unwrap()));
                    if *g.borrow() {
                        Before::Veto("a drag is in progress".into())
                    } else {
                        Before::Go
                    }
                });
                ctx.after(STEP_UNDO, move |_, _, result| {
                    c2.lock()
                        .unwrap()
                        .push(format!("after {}", result.as_ref().unwrap()["undone"]));
                });
                Ok(())
            }),
        ))
        .build(&egui::Context::default())
        .unwrap();
    let r = app.run(&CMD_UNDO, Json::Null, Origin::User).now().unwrap();
    assert_eq!(r, Ok(json!({ "undone": "rotate" })));
    assert_eq!(
        *calls.lock().unwrap(),
        ["before rotate", "host undo", "after \"rotate\""]
    );
    // A veto: the host is not asked, the command fails with the reason,
    // the journal says so.
    *gesture.borrow_mut() = true;
    calls.lock().unwrap().clear();
    let before = app.handle().last_event();
    let r = app.run(&CMD_UNDO, Json::Null, Origin::User).now().unwrap();
    assert!(r.unwrap_err().contains("a drag is in progress"));
    assert_eq!(*calls.lock().unwrap(), ["before crop"]);
    assert_eq!(*history.done.lock().unwrap(), ["crop"]);
    let failed = app
        .handle()
        .events_since(before)
        .into_iter()
        .find(|e| e.name == "edit.undo_failed")
        .unwrap();
    assert_eq!(failed.data["reason"], "a drag is in progress");
    assert_eq!(failed.data["label"], "crop");
    let trace = app.handle().step_traces().pop().unwrap();
    assert_eq!(trace.vetoed_by.unwrap().0.as_str(), "watcher");
}

#[test]
fn a_run_hook_does_the_undo_instead_of_the_host() {
    let (k, history) = kernel(&["crop"]);
    let mut app = App::builder()
        .host(k)
        .plugin(P(
            "own",
            Box::new(|ctx| {
                ctx.replace(STEP_UNDO, |_, _| Ok(json!({ "undone": "mine" })));
                Ok(())
            }),
        ))
        .build(&egui::Context::default())
        .unwrap();
    let r = app.run(&CMD_UNDO, Json::Null, Origin::User).now().unwrap();
    assert_eq!(r, Ok(json!({ "undone": "mine" })));
    assert!(history.calls.lock().unwrap().is_empty());
    assert_eq!(
        app.handle().step_traces().pop().unwrap().ran_by.as_deref(),
        Some("own")
    );
}

#[test]
fn the_settings_keep_their_own_undo_apart_from_the_host_s() {
    let (k, history) = kernel(&["crop"]);
    let store = k.store.clone();
    lock(&store)
        .declare(
            &egui_pop_model::ids::OwnerId::USER,
            "settings/x",
            Descriptor::bool("X", false),
        )
        .unwrap();
    lock(&store)
        .set(Set::new(
            "settings/x",
            Change::To(Value::Bool(true)),
            Origin::User,
        ))
        .unwrap();
    let mut app = App::builder()
        .host(k)
        .build(&egui::Context::default())
        .unwrap();
    // Edit > Undo is the host's: the setting stays.
    app.run(&CMD_UNDO, Json::Null, Origin::User)
        .now()
        .unwrap()
        .unwrap();
    assert!(history.done.lock().unwrap().is_empty());
    assert_eq!(
        lock(&store).get("settings/x").unwrap().value,
        Value::Bool(true)
    );
    // The settings window's undo is the store's.
    app.run(&CMD_VALUE_UNDO, Json::Null, Origin::User)
        .now()
        .unwrap()
        .unwrap();
    assert_eq!(
        lock(&store).get("settings/x").unwrap().value,
        Value::Bool(false)
    );
}

#[test]
fn the_default_host_undoes_the_store() {
    let mut app = App::builder().build(&egui::Context::default()).unwrap();
    let store = app.handle().store();
    lock(&store)
        .declare(
            &egui_pop_model::ids::OwnerId::USER,
            "settings/n",
            Descriptor::bool("N", false),
        )
        .unwrap();
    lock(&store)
        .set(Set::new(
            "settings/n",
            Change::To(Value::Bool(true)),
            Origin::User,
        ))
        .unwrap();
    assert!(app.handle().can_undo());
    let r = app.run(&CMD_UNDO, Json::Null, Origin::User).now().unwrap();
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(
        lock(&store).get("settings/n").unwrap().value,
        Value::Bool(false)
    );
    assert!(!app.handle().can_undo());
}
