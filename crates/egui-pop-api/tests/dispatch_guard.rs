//! Every command goes through the host's dispatch: undo and redo,
//! `history.*`, the context stack's and the clipboard's commands too, so
//! a host refusing agents in one place refuses them everywhere. An
//! agent's undo is held to the values' writers.

use std::sync::{Arc, Mutex};

use egui_pop_api::app::{CMD_REDO, CMD_UNDO};
use egui_pop_api::clipboard::{self, ClipboardPlugin};
use egui_pop_api::contexts::{self, ContextsPlugin};
use egui_pop_api::devtools::{DevtoolsPlugin, SETTING_DEVTOOLS};
use egui_pop_api::history_view::{self, HistoryPlugin};
use egui_pop_api::host::{Dispatch, Host, Journal, LocalHost, Reply, Waker};
use egui_pop_api::{App, AppBuilder, DefaultPlugins};
use egui_pop_core::binding::SharedStore;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::CommandId;
use egui_pop_model::values::{Change, Set, Value};
use serde_json::{Value as Json, json};

const REFUSAL: &str = "the host runs nothing for agents";

/// The default host, refusing every command an agent asks for, and
/// keeping the commands it saw.
struct Guarded {
    inner: LocalHost,
    seen: Arc<Mutex<Vec<(CommandId, bool)>>>,
}

impl Host for Guarded {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        self.seen
            .lock()
            .unwrap()
            .push((d.command.clone(), d.deferred()));
        if matches!(d.origin, Origin::Agent(_)) {
            return Reply::Done(Err(REFUSAL.into()));
        }
        self.inner.dispatch(d)
    }
    fn journal(&self) -> Option<&Journal> {
        self.inner.journal()
    }
    fn journal_mut(&mut self) -> Option<&mut Journal> {
        self.inner.journal_mut()
    }
    fn store(&self) -> SharedStore {
        self.inner.store()
    }
    fn set_waker(&mut self, waker: Waker) {
        self.inner.set_waker(waker);
    }
}

/// The commands whose run calls the host itself.
const CALLING_THE_HOST: [CommandId; 29] = [
    CMD_UNDO,
    CMD_REDO,
    history_view::CMD_LIST,
    history_view::CMD_READ,
    history_view::CMD_UNDO,
    history_view::CMD_REDO,
    history_view::CMD_JUMP,
    history_view::CMD_SNAPSHOT,
    history_view::CMD_RESTORE,
    history_view::CMD_DROP_SNAPSHOT,
    history_view::CMD_UNDO_STEP,
    contexts::CMD_BACK,
    contexts::CMD_UP,
    contexts::CMD_ENTER,
    contexts::CMD_READ,
    contexts::CMD_REQUEST,
    contexts::CMD_GO,
    contexts::CMD_SET_MODE,
    contexts::CMD_HOLD,
    contexts::CMD_RELEASE,
    clipboard::CMD_CUT,
    clipboard::CMD_COPY,
    clipboard::CMD_PASTE,
    clipboard::CMD_PASTE_SPECIAL,
    clipboard::CMD_CLIP_COPY,
    clipboard::CMD_CLIP_PASTE,
    clipboard::CMD_OFFERS,
    clipboard::CMD_READ,
    clipboard::CMD_WRITE,
];

fn plugins(b: AppBuilder) -> AppBuilder {
    b.plugins(DefaultPlugins)
        .plugin(DevtoolsPlugin::default())
        .plugin(HistoryPlugin::default())
        .plugin(ContextsPlugin::default())
        .plugin(ClipboardPlugin)
}

fn agent() -> Origin {
    Origin::Agent("tester".into())
}

#[test]
fn a_host_refusing_agents_refuses_them_the_commands_that_call_it() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let host = Guarded {
        inner: LocalHost::new(SharedStore::default()),
        seen: seen.clone(),
    };
    let ctx = egui::Context::default();
    let mut app = plugins(App::builder().host(host)).build(&ctx).unwrap();
    app.logic(&ctx);
    let commands = app.handle().commands();
    let mut reached = 0;
    for id in CALLING_THE_HOST {
        let spec = &commands
            .iter()
            .find(|(s, _)| s.id == id)
            .unwrap_or_else(|| panic!("`{id}` is declared"))
            .0;
        seen.lock().unwrap().clear();
        let r = app.run(&id, json!({}), agent()).now().unwrap();
        assert!(r.is_err(), "`{id}` ran for an agent: {r:?}");
        let dispatched = seen.lock().unwrap().clone();
        if dispatched.is_empty() {
            // Refused before the host: its `when` or its flag.
            let e = r.unwrap_err();
            assert!(
                e.contains("not offered to agents") || !spec.when.is_empty(),
                "`{id}`: {e}"
            );
            continue;
        }
        reached += 1;
        assert_eq!(dispatched, vec![(id.clone(), true)], "deferred");
        assert_eq!(r, Err(REFUSAL.into()), "`{id}`");
    }
    assert!(reached >= 15, "{reached} reached the host");
    // The user's go through the same dispatch and run.
    seen.lock().unwrap().clear();
    let r = app
        .run(&history_view::CMD_LIST, Json::Null, Origin::User)
        .now()
        .unwrap();
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(*seen.lock().unwrap(), vec![(history_view::CMD_LIST, true)]);
    // An undo the user may make reaches the host, and the agent's does not
    // pass it.
    let store = app.handle().store();
    egui_pop_core::lock(&store)
        .set(Set::new(
            &format!("settings/{SETTING_DEVTOOLS}"),
            Change::To(Value::Bool(true)),
            Origin::User,
        ))
        .unwrap();
    app.logic(&ctx);
    seen.lock().unwrap().clear();
    let r = app.run(&CMD_UNDO, Json::Null, agent()).now().unwrap();
    assert_eq!(r, Err(REFUSAL.into()));
    assert!(devtools_on(&app));
    let r = app.run(&CMD_UNDO, Json::Null, Origin::User).now().unwrap();
    assert!(r.is_ok(), "{r:?}");
    assert!(!devtools_on(&app));
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .all(|(id, deferred)| *id == CMD_UNDO && *deferred)
    );
}

fn devtools_on(app: &App) -> bool {
    egui_pop_core::lock(&app.handle().store())
        .get(&format!("settings/{SETTING_DEVTOOLS}"))
        .and_then(|e| e.value.as_bool())
        .unwrap()
}

#[test]
fn an_agents_undo_is_held_to_the_writers() {
    let ctx = egui::Context::default();
    let mut app = plugins(App::builder()).build(&ctx).unwrap();
    app.logic(&ctx);
    let store = app.handle().store();
    egui_pop_core::lock(&store)
        .set(Set::new(
            &format!("settings/{SETTING_DEVTOOLS}"),
            Change::To(Value::Bool(true)),
            Origin::User,
        ))
        .unwrap();
    app.logic(&ctx);
    let h = app.handle().clone();
    let before = h.last_event();
    // `edit.undo` for an agent: the setting is its owner's alone.
    let r = app.run(&CMD_UNDO, Json::Null, agent()).now().unwrap();
    assert!(devtools_on(&app), "not undone: {r:?}");
    let events = h.events_since(before);
    assert!(
        events
            .iter()
            .any(|e| e.name == "edit.undo_failed" && e.data.to_string().contains("agent:tester")),
        "{events:?}"
    );
    // `history.undo` and `history.jump`, the same.
    let _ = app.run(&history_view::CMD_UNDO, json!({}), agent());
    let _ = app.run(&history_view::CMD_JUMP, json!({ "to": 0 }), agent());
    assert!(devtools_on(&app));
    // The user's undo takes it back.
    let r = app.run(&CMD_UNDO, Json::Null, Origin::User).now().unwrap();
    assert!(r.is_ok(), "{r:?}");
    assert!(!devtools_on(&app));
}
