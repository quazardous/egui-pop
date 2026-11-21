//! Panel instances inherit their kind's keys and history routing: the
//! context key `focusedPanelKind` is the focused panel's kind, so a
//! clause `focusedPanelKind == '<kind>'` fires in every instance, and a
//! history scope whose focus target names a kind is active and routed to
//! while one of its instances `<kind>#<n>` is drawn or focused. A target
//! naming one panel exactly still names that panel only.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_pop_api::bus::{OpenPlace, PanelOpenAsk};
use egui_pop_api::history::{self, History, Routing, ScopeId, ScopeInfo, StoreHistory};
use egui_pop_api::host::{Dispatch, Host, Journal, Reply, Waker};
use egui_pop_api::workbench::Change;
use egui_pop_api::{App, AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::binding::SharedStore;
use egui_pop_core::lock;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, OwnerId, PanelId, PluginId};
use egui_pop_model::keymap::BindOptions;
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_model::values::{Change as ValueChange, Descriptor, Set, Value};
use serde_json::Value as Json;

/// Declared with several.
const GAL: PanelId = PanelId::from_static("demo.gallery");
/// Declared once.
const LOG: PanelId = PanelId::from_static("demo.log");
/// Focused on the gallery's kind.
const KIND: ScopeId = ScopeId::from_static("kind");
/// Focused on the log exactly.
const EXACT: ScopeId = ScopeId::from_static("exact");
/// Focused on the gallery's third instance exactly.
const THIRD: ScopeId = ScopeId::from_static("third");
const PICK: &str = "demo/pick";
const LINE: &str = "demo/line";
const SHOT: &str = "demo/shot";

struct Kernel {
    journal: Journal,
    store: SharedStore,
    owner: StoreHistory,
}

impl Kernel {
    fn new() -> Self {
        let store = SharedStore::default();
        let scope = |id: ScopeId, focus: &str| {
            let mut i = ScopeInfo::new(id.clone(), id.as_str());
            i.focus = vec![focus.into()];
            i
        };
        let owner = StoreHistory::with_scopes(
            store.clone(),
            vec![
                scope(KIND, GAL.as_str()),
                scope(EXACT, LOG.as_str()),
                scope(THIRD, "demo.gallery#3"),
            ],
        );
        Self {
            journal: Journal::default(),
            store,
            owner,
        }
    }
}

impl Host for Kernel {
    fn dispatch(&mut self, d: &Dispatch<'_>) -> Reply {
        Reply::Done(d.run_local().unwrap_or(Err("no handler".into())))
    }
    fn journal(&self) -> Option<&Journal> {
        Some(&self.journal)
    }
    fn journal_mut(&mut self) -> Option<&mut Journal> {
        Some(&mut self.journal)
    }
    fn store(&self) -> SharedStore {
        self.store.clone()
    }
    fn set_waker(&mut self, _: Waker) {}
    fn history(&mut self) -> Option<&mut dyn History> {
        Some(&mut self.owner)
    }
}

type Ran = Rc<RefCell<Vec<String>>>;

struct Demo(Ran);

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.instance_panel(
            PanelSpec::new(GAL, "Gallery", Area::Center),
            |ui, _, inst| {
                let _ = ui.button(format!("In gallery #{}", inst.number));
            },
        )?;
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            let _ = ui.button("In the log");
        })?;
        let ran = self.0.clone();
        ctx.command(
            CommandSpec::new(CommandId::from_static("demo.pick"), "Pick", "Demo")
                .key_with("F3", BindOptions::global())
                .when("focusedPanelKind == 'demo.gallery'"),
            move |c| {
                let focused = c.app.focused_panel();
                ran.borrow_mut()
                    .push(focused.map_or(String::new(), |p| p.to_string()));
                Ok(Json::Null)
            },
        )?;
        let store = ctx.app().store();
        let mut s = lock(&store);
        for path in [PICK, LINE, SHOT] {
            s.declare(&OwnerId::USER, path, Descriptor::int(path, 0, 99, 0))
                .unwrap();
        }
        Ok(())
    }
}

fn harness(routing: Routing) -> (Harness<'static, App>, AppHandle, Ran) {
    let ran = Ran::default();
    let app = App::builder()
        .host(Kernel::new())
        .plugin(Demo(ran.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let a = app.handle().clone();
    history::set_routing(&a, routing);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    (h, a, ran)
}

/// Opens the gallery's next instance in an area; its id.
fn open_new(h: &mut Harness<'static, App>, a: &AppHandle, area: Area) -> PanelId {
    a.open_panel(PanelOpenAsk::new(GAL.as_str(), OpenPlace::Area(area), "test").instance("new"))
        .unwrap();
    h.run();
    h.run();
    a.panel_instances(&GAL).last().cloned().unwrap()
}

fn focus(h: &mut Harness<'static, App>, a: &AppHandle, panel: &PanelId) {
    assert!(a.focus_panel(panel).is_some_and(|o| o.ok()));
    h.run();
    h.run();
    assert_eq!(a.focused_panel().as_ref(), Some(panel));
}

/// A step in `scope`: `path` set to `v`, sealed.
fn set(a: &AppHandle, scope: &ScopeId, path: &str, v: i64) {
    let store = a.store();
    let mut s = lock(&store);
    s.set(Set::new(path, ValueChange::To(Value::Int(v)), Origin::User).history(scope))
        .unwrap();
    s.seal();
}

fn get(a: &AppHandle, path: &str) -> Value {
    lock(&a.store()).get(path).unwrap().value.clone()
}

fn undo(h: &mut Harness<'static, App>) {
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
}

#[test]
fn an_instance_has_its_kind_s_keys_and_undo_scope() {
    let (mut h, a, ran) = harness(Routing::Focus);
    let second = open_new(&mut h, &a, Area::Bottom);
    assert_eq!(second.as_str(), "demo.gallery#2");
    focus(&mut h, &a, &second);
    let cx = a.when_context();
    assert_eq!(cx.get("focusedPanel"), "demo.gallery#2");
    assert_eq!(cx.get("focusedPanelKind"), "demo.gallery");
    // The kind's key fires in the instance.
    h.key_press(Key::F3);
    h.run();
    assert_eq!(*ran.borrow(), vec!["demo.gallery#2".to_string()]);
    // Edit › Undo routes to the kind's scope, though the log's step is
    // newer.
    set(&a, &KIND, PICK, 1);
    set(&a, &EXACT, LINE, 2);
    assert_eq!(history::focused_scope(&a), Some(KIND));
    undo(&mut h);
    assert_eq!(get(&a, PICK), Value::Int(0));
    assert_eq!(get(&a, LINE), Value::Int(2));
    // The log focused: its key does not fire, its exact scope routes.
    focus(&mut h, &a, &LOG);
    assert_eq!(a.when_context().get("focusedPanelKind"), "demo.log");
    h.key_press(Key::F3);
    h.run();
    assert_eq!(ran.borrow().len(), 1, "{:?}", ran.borrow());
    assert_eq!(history::focused_scope(&a), Some(EXACT));
}

#[test]
fn a_kind_s_scope_is_active_while_only_an_instance_is_drawn() {
    let (mut h, a, _) = harness(Routing::Newest);
    let second = open_new(&mut h, &a, Area::Bottom);
    // Only the instance shows: the declared gallery and the log closed.
    for p in [GAL, LOG] {
        assert!(a.change_workbench(Change::Hide(p)).is_some_and(|o| o.ok()));
    }
    h.run();
    h.run();
    assert!(a.layout().shows(&second));
    set(&a, &KIND, PICK, 1);
    // Newer, in scopes whose exact targets are not drawn: inactive.
    set(&a, &EXACT, LINE, 2);
    set(&a, &THIRD, SHOT, 3);
    assert_eq!(history::target(&a, false).unwrap().scope(), KIND);
    undo(&mut h);
    assert_eq!(get(&a, PICK), Value::Int(0));
    assert_eq!(
        (get(&a, LINE), get(&a, SHOT)),
        (Value::Int(2), Value::Int(3))
    );
    // The third instance opened: its exact scope becomes active, and the
    // newest.
    let third = open_new(&mut h, &a, Area::EndSide);
    assert_eq!(third.as_str(), "demo.gallery#3");
    assert_eq!(history::target(&a, false).unwrap().scope(), THIRD);
}
