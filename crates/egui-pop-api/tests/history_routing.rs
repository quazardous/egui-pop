//! Focus routing (ADR 0014): under `Routing::Focus`, Ctrl+Z and
//! Ctrl+Shift+Z go to the history of the focused surface, never to the
//! newest step of another; with no such surface focused, to the history
//! focused last; the `pop.history.target` step lets the application
//! redirect it. `Routing::Newest` stays the default.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::history::{
    self, History, Routing, STEP_TARGET, ScopeId, ScopeInfo, StoreHistory,
};
use egui_pop_api::host::{Dispatch, Host, Journal, Reply, Waker};
use egui_pop_api::steps::Before;
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::binding::SharedStore;
use egui_pop_core::lock;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, OwnerId, PluginId};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::json;

const GRAPH: ScopeId = ScopeId::from_static("graph");
const IMAGE: ScopeId = ScopeId::from_static("image");
const NODE: &str = "graph/node";
const EXPOSURE: &str = "image/exposure";

/// A kernel whose history owner has two scopes over its store: the
/// graph's (focus target: the graph surface) and the image's.
struct Kernel {
    journal: Journal,
    store: SharedStore,
    owner: StoreHistory,
}

impl Kernel {
    fn new() -> Self {
        let store = SharedStore::default();
        let scope = |id: ScopeId, title: &str, surface: &str| {
            let mut i = ScopeInfo::new(id, title);
            i.focus = vec![surface.into()];
            i
        };
        let owner = StoreHistory::with_scopes(
            store.clone(),
            vec![
                scope(GRAPH, "Graph", "sheet.graph"),
                scope(IMAGE, "Image", "sheet.image"),
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

/// Two surfaces, the graph's and the image's, each with a button, and a
/// button outside both (where the focus names no history: a menu bar).
fn sheets() -> P {
    P(
        "sheets",
        Box::new(|ctx| {
            ctx.block(
                zones::MAIN,
                Block::add(BlockId::from_static("sheets.block"), 0, |ui, _| {
                    for (surface, label) in [
                        ("sheet.graph", "In the graph"),
                        ("sheet.image", "In the image"),
                    ] {
                        ui.allocate_ui(egui::vec2(300.0, 80.0), |ui| {
                            egui_pop_core::focus::scope(ui, surface, &[], |ui| {
                                ui.add_space(10.0);
                                let _ = ui.button(label);
                                ui.add_space(10.0);
                            });
                        });
                    }
                    ui.add_space(40.0);
                    let _ = ui.button("Outside");
                }),
            );
            let store = ctx.app().store();
            let mut s = lock(&store);
            s.declare(&OwnerId::USER, NODE, Descriptor::int("Node", 0, 99, 0))
                .unwrap();
            s.declare(
                &OwnerId::USER,
                EXPOSURE,
                Descriptor::int("Exposure", 0, 99, 0),
            )
            .unwrap();
            Ok(())
        }),
    )
}

fn app(routing: Option<Routing>, extra: Option<P>) -> (App, AppHandle) {
    let mut b = App::builder().host(Kernel::new()).plugin(sheets());
    if let Some(p) = extra {
        b = b.plugin(p);
    }
    let app = b.build(&egui::Context::default()).unwrap();
    let a = app.handle().clone();
    if let Some(r) = routing {
        history::set_routing(&a, r);
    }
    (app, a)
}

fn harness(app: App) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

/// A step in `scope`: `path` set to `v`, sealed.
fn set(a: &AppHandle, scope: &ScopeId, path: &str, v: i64) {
    let store = a.store();
    let mut s = lock(&store);
    s.set(Set::new(path, Change::To(Value::Int(v)), Origin::User).history(scope))
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

fn redo(h: &mut Harness<'static, App>) {
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run();
}

fn click(h: &mut Harness<'static, App>, label: &str) {
    h.get_by_label(label).click();
    h.run();
}

#[test]
fn ctrl_z_undoes_the_focused_surface_s_history_even_when_another_step_is_newer() {
    let (app, a) = app(Some(Routing::Focus), None);
    set(&a, &GRAPH, NODE, 1);
    set(&a, &IMAGE, EXPOSURE, 5);
    let mut h = harness(app);
    // The graph has the focus: its edit goes, the newer exposure stays.
    click(&mut h, "In the graph");
    assert_eq!(history::focused_scope(&a), Some(GRAPH));
    assert_eq!(history::target(&a, false).unwrap().scope(), GRAPH);
    undo(&mut h);
    assert_eq!(get(&a, NODE), Value::Int(0));
    assert_eq!(get(&a, EXPOSURE), Value::Int(5));
    // The other way round: a graph edit newer than the exposure, the
    // image focused: the exposure goes.
    set(&a, &GRAPH, NODE, 2);
    click(&mut h, "In the image");
    undo(&mut h);
    assert_eq!(get(&a, EXPOSURE), Value::Int(0));
    assert_eq!(get(&a, NODE), Value::Int(2));
}

#[test]
fn with_no_history_focused_ctrl_z_goes_to_the_one_focused_last() {
    let (app, a) = app(Some(Routing::Focus), None);
    let mut h = harness(app);
    click(&mut h, "In the graph");
    set(&a, &GRAPH, NODE, 1);
    set(&a, &IMAGE, EXPOSURE, 5);
    // The focus leaves for a place that names no history.
    click(&mut h, "Outside");
    assert_eq!(history::focused_scope(&a), None);
    assert_eq!(history::last_focused(&a), Some(GRAPH));
    let snap = a.undo_label();
    assert_eq!(
        snap.as_deref(),
        Some("set graph/node"),
        "the Edit menu names the graph's step"
    );
    undo(&mut h);
    assert_eq!(get(&a, NODE), Value::Int(0));
    assert_eq!(get(&a, EXPOSURE), Value::Int(5));
}

#[test]
fn redo_follows_the_same_target_not_the_history_undone_last() {
    let (app, a) = app(Some(Routing::Focus), None);
    set(&a, &GRAPH, NODE, 1);
    set(&a, &IMAGE, EXPOSURE, 5);
    let mut h = harness(app);
    click(&mut h, "In the graph");
    undo(&mut h);
    click(&mut h, "In the image");
    undo(&mut h);
    assert_eq!(
        (get(&a, NODE), get(&a, EXPOSURE)),
        (Value::Int(0), Value::Int(0))
    );
    // The image was undone last; the graph has the focus: its redo.
    click(&mut h, "In the graph");
    assert_eq!(history::target(&a, true).unwrap().scope(), GRAPH);
    redo(&mut h);
    assert_eq!(get(&a, NODE), Value::Int(1));
    assert_eq!(get(&a, EXPOSURE), Value::Int(0));
}

#[test]
fn newest_stays_the_default_routing() {
    let (app, a) = app(None, None);
    assert_eq!(history::routing(&a), Routing::Newest);
    set(&a, &GRAPH, NODE, 1);
    set(&a, &IMAGE, EXPOSURE, 5);
    let mut h = harness(app);
    click(&mut h, "In the graph");
    undo(&mut h);
    assert_eq!(get(&a, EXPOSURE), Value::Int(0), "the newest step goes");
    assert_eq!(get(&a, NODE), Value::Int(1));
}

#[test]
fn a_before_hook_on_pop_history_target_redirects_ctrl_z() {
    let hook = P(
        "redirect",
        Box::new(|ctx| {
            ctx.before(STEP_TARGET, |_, args| {
                if args["rule"] != "focus" {
                    return Before::Go;
                }
                let mut a = args.clone();
                a["scope"] = json!("image");
                Before::Redirect(a)
            });
            Ok(())
        }),
    );
    let (app, a) = app(Some(Routing::Focus), Some(hook));
    set(&a, &IMAGE, EXPOSURE, 5);
    set(&a, &GRAPH, NODE, 1);
    let mut h = harness(app);
    click(&mut h, "In the graph");
    undo(&mut h);
    assert_eq!(get(&a, EXPOSURE), Value::Int(0), "redirected to the image");
    assert_eq!(get(&a, NODE), Value::Int(1));
}

#[test]
fn a_surface_declares_the_history_it_works_on() {
    // The image's surface declares the graph's history: it wins over the
    // image scope's own focus targets.
    let declare = P(
        "declare",
        Box::new(|ctx| {
            ctx.history_target("sheet.image", GRAPH);
            Ok(())
        }),
    );
    let (app, a) = app(Some(Routing::Focus), Some(declare));
    set(&a, &GRAPH, NODE, 1);
    set(&a, &IMAGE, EXPOSURE, 5);
    let mut h = harness(app);
    click(&mut h, "In the image");
    assert_eq!(history::focused_scope(&a), Some(GRAPH));
    undo(&mut h);
    assert_eq!(get(&a, NODE), Value::Int(0));
    assert_eq!(get(&a, EXPOSURE), Value::Int(5));
}
