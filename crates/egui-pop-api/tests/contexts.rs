//! The context stack (ADR 0012): the owner applies or refuses, the
//! interface projects what the owner says (layouts per place and mode,
//! context keys, keys by frame), every window shows the same stack, and
//! the stack and its view come back after a restart.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::contexts::{
    self, CONTEXTS, ContextsPlugin, Envelope, FrameSpec, Request, STEP_REQUEST, Verdict,
};
use egui_pop_api::steps::{Before, Outcome};
use egui_pop_api::toasts::TOASTS;
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, AppBuilder, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{BlockId, CommandId, FrameId, GroupId, PanelId, PluginId};
use egui_pop_model::layout::Layout;
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::{Value as Json, json};

const TREE: PanelId = PanelId::from_static("demo.tree");
const LOG: PanelId = PanelId::from_static("demo.log");
const SECOND: PanelId = PanelId::from_static("demo.second");
const CRUMBS: PanelId = PanelId::from_static("demo.crumbs");

const ALBUM: FrameId = FrameId::from_static("demo.album");
const PHOTO: FrameId = FrameId::from_static("demo.photo");
const CULLING: FrameId = FrameId::from_static("demo.culling");
const REVIEW: FrameId = FrameId::from_static("demo.review");
const COMPARE: FrameId = FrameId::from_static("demo.compare");
const AGENT: FrameId = FrameId::from_static("demo.agent");
const ORIENTATION: GroupId = GroupId::from_static("orientation");

const PICK: CommandId = CommandId::from_static("demo.pick");
const APPROVE: CommandId = CommandId::from_static("demo.approve");

/// The key the interface's own guard reads: unsaved typed input.
const UNSAVED: &str = "demoUnsaved";

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![contexts::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(TREE, "Tree", Area::StartSide), |ui, _| {
            ui.label("tree body");
        })?
        .panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?
        .panel(
            PanelSpec::new(SECOND, "Second image", Area::Bottom).closed(),
            |ui, _| {
                ui.label("second body");
            },
        )?
        .panel(
            PanelSpec::new(CRUMBS, "Crumbs", Area::EndSide),
            |ui, app| {
                contexts::breadcrumb(ui, app);
                contexts::mode_chips(ui, app);
            },
        )?;
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("demo.main"), 0, |ui, app| {
                contexts::breadcrumb(ui, app);
                ui.label(format!("last key: {}", app.context().get("demoLast")));
            }),
        );
        ctx.arrangement(
            "Album view",
            Layout {
                hidden: vec![TREE],
                ..Layout::default()
            },
        );
        let album = FrameSpec::place(ALBUM, "Album").arrangement("Album view");
        let photo = FrameSpec::place(PHOTO, "Photo").can_leave(|c| {
            if c.payload == &json!("locked") {
                Verdict::Refuse("the photo is locked".into())
            } else {
                Verdict::Go
            }
        });
        let culling = FrameSpec::mode(CULLING, ORIENTATION, "Culling").key("demoRating", "true");
        let review = FrameSpec::mode(REVIEW, ORIENTATION, "Review");
        let compare =
            FrameSpec::mode(COMPARE, GroupId::from_static("compare"), "Compare").show(SECOND);
        let agent = FrameSpec::mode(AGENT, GroupId::from_static("driver"), "Agent").transient();
        ctx.command(CommandSpec::new(PICK, "Pick", "Demo"), |call| {
            call.app.set_context("demoLast", "pick");
            Ok(Json::Null)
        })?
        .command(CommandSpec::new(APPROVE, "Approve", "Demo"), |call| {
            call.app.set_context("demoLast", "approve");
            Ok(Json::Null)
        })?;
        // Each orientation has its own keys: P picks while culling,
        // approves while reviewing.
        ctx.frame_keybinding(&culling, "P", PICK)?
            .frame_keybinding(&review, "P", APPROVE)?;
        ctx.frame(album)?
            .frame(photo)?
            .frame(culling)?
            .frame(review)?
            .frame(compare)?
            .frame(agent)?;
        // The interface's own guard: unsaved typed input keeps the user
        // where they are.
        ctx.before(STEP_REQUEST, |app, _| {
            if app.context().get(UNSAVED) == "true" {
                Before::Veto("the typed name is not saved".into())
            } else {
                Before::Go
            }
        });
        ctx.setting(
            egui_pop_model::ids::SettingId::from_static("demo.n"),
            Descriptor::int("N", 0, 100, 0),
        )?;
        Ok(())
    }
}

fn builder() -> AppBuilder {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(ContextsPlugin::default())
        .plugin(Demo)
}

fn harness(builder: AppBuilder) -> Harness<'static, App> {
    let app = builder.build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 640.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn key(h: &Harness<'_, App>, k: &str) -> String {
    h.state().handle().context().get(k).to_string()
}

fn rev(h: &Harness<'_, App>) -> u64 {
    contexts::with_owner(h.state().handle(), |c| c.rev()).unwrap()
}

#[test]
fn entering_and_leaving_restore_the_projection() {
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    assert_eq!(key(&h, "place"), "root");
    assert_eq!(key(&h, "placeDepth"), "0");
    // The root's layout, changed by the user.
    app.move_panel(&TREE, Area::EndSide);
    h.run();
    let root = app.layout();
    // Entering the album: its arrangement, the tree hidden.
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    h.run();
    assert_eq!(key(&h, "place"), "demo.album");
    assert_eq!(key(&h, "placeDepth"), "1");
    assert!(!app.panel_visible(&TREE));
    assert!(h.query_all_by_label("Album").next().is_some());
    // Back: the root's layout exactly as it was.
    assert!(contexts::back(&app).ok());
    h.run();
    assert_eq!(key(&h, "place"), "root");
    assert_eq!(app.layout(), root);
    // A pushed mode shows its panel; a change made in it is its own and
    // goes when it is left.
    assert!(contexts::push_mode(&app, COMPARE).ok());
    h.run();
    assert!(app.panel_visible(&SECOND));
    assert_eq!(key(&h, "mode.compare"), "demo.compare");
    assert_eq!(key(&h, "modePushed"), "true");
    app.move_panel(&LOG, Area::StartSide);
    h.run();
    // Escape pops it.
    h.key_press(Key::Escape);
    h.run();
    assert_eq!(key(&h, "mode.compare"), "");
    assert!(!app.panel_visible(&SECOND));
    assert_eq!(app.layout(), root);
    // Coming back to compare finds the change made there.
    assert!(contexts::push_mode(&app, COMPARE).ok());
    h.run();
    assert_eq!(app.layout().area_of(&LOG), Some(Area::StartSide));
    // The journal says what changed, then what was left and entered.
    let names: Vec<String> = app
        .events_since(0)
        .into_iter()
        .map(|e| e.name)
        .filter(|n| n.starts_with("context."))
        .collect();
    for n in [
        "context.request",
        "context.changed",
        "context.entered",
        "context.left",
    ] {
        assert!(names.iter().any(|x| x == n), "{n} in {names:?}");
    }
    assert!(app.diagnostics().is_empty(), "{:?}", app.diagnostics());
}

#[test]
fn a_refusal_leaves_the_display_unchanged() {
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    assert!(
        contexts::enter(&app, PHOTO, json!("locked")).ok(),
        "entering a locked photo is allowed"
    );
    h.run();
    let layout = app.layout();
    let before = rev(&h);
    // The owner's guard refuses: nothing moves, the user is told why.
    let out = contexts::leave(&app);
    assert!(
        matches!(&out, Outcome::Done(Err(e)) if e.contains("the photo is locked")),
        "{out:?}"
    );
    h.run();
    assert_eq!(rev(&h), before);
    assert_eq!(key(&h, "place"), "demo.photo");
    assert_eq!(app.layout(), layout);
    let toasts = app.service(&TOASTS).unwrap();
    assert!(
        toasts
            .shown()
            .iter()
            .any(|t| t.toast.text.contains("the photo is locked")),
        "{:?}",
        toasts.shown()
    );
    assert!(
        app.events_since(0)
            .iter()
            .any(|e| e.name == "context.refused" && e.data["kind"] == "guard")
    );
    // The interface's own guard: the request is never sent.
    app.set_context(UNSAVED, "true");
    let out = contexts::set_mode(&app, CULLING);
    assert_eq!(out.vetoed(), Some("the typed name is not saved"));
    assert_eq!(rev(&h), before);
    app.set_context(UNSAVED, "false");
    // A hold refuses with its reason; released, the same request goes.
    let token = contexts::hold(&app, vec![ORIENTATION], "a drag is in progress", "user").unwrap();
    let out = contexts::request(
        &app,
        Envelope::new(Request::set(CULLING)),
        &Origin::Agent("test".into()),
    );
    assert!(
        matches!(&out, Outcome::Done(Err(e)) if e.contains("a drag is in progress")),
        "{out:?}"
    );
    assert!(contexts::release(&app, token));
    // A stale revision, a dry run.
    let stale = contexts::request(
        &app,
        Envelope::new(Request::set(CULLING)).if_rev(before + 7),
        &Origin::Agent("test".into()),
    );
    assert!(matches!(stale, Outcome::Done(Err(_))));
    let dry = contexts::request(
        &app,
        Envelope::new(Request::set(CULLING)).dry_run(),
        &Origin::Agent("test".into()),
    );
    assert!(dry.ok());
    assert_eq!(rev(&h), before);
    assert!(contexts::set_mode(&app, CULLING).ok());
    assert_eq!(rev(&h), before + 1);
}

#[test]
fn two_windows_stay_consistent() {
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    // The crumbs panel in a window of its own.
    app.detach_panel(&CRUMBS);
    h.run();
    h.run();
    assert_eq!(app.layout().auxiliary.len(), 1);
    // The main window and the detached one show the same stack.
    assert_eq!(h.query_all_by_label("Home").count(), 2);
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    assert!(contexts::set_mode(&app, CULLING).ok());
    h.run();
    assert_eq!(h.query_all_by_label("Album").count(), 2);
    // The chip is in the detached window; leaving the mode there leaves
    // it everywhere.
    h.get_by_label("Leave Culling").click();
    h.run();
    assert_eq!(key(&h, "mode.orientation"), "");
    // A crumb of the main window jumps back for both.
    h.query_all_by_label("Home").next().unwrap().click();
    h.run();
    assert_eq!(key(&h, "place"), "root");
    assert_eq!(h.query_all_by_label("Album").count(), 0);
}

#[test]
fn keys_follow_the_context() {
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    h.key_press(Key::P);
    h.run();
    assert_eq!(key(&h, "demoLast"), "", "P does nothing without a mode");
    assert!(contexts::set_mode(&app, CULLING).ok());
    h.run();
    assert_eq!(key(&h, "demoRating"), "true");
    h.key_press(Key::P);
    h.run();
    assert_eq!(key(&h, "demoLast"), "pick");
    assert!(contexts::set_mode(&app, REVIEW).ok());
    h.run();
    assert_eq!(
        key(&h, "demoRating"),
        "",
        "the frame's own key goes with it"
    );
    h.key_press(Key::P);
    h.run();
    assert_eq!(key(&h, "demoLast"), "approve");
    // Alt+Left goes back up; at the root it does nothing.
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    h.run();
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowLeft);
    h.run();
    assert_eq!(key(&h, "place"), "root");
    assert_no_key_conflicts(&h);
}

/// The keymap has no conflict: the orientations' P never compete.
fn assert_no_key_conflicts(h: &Harness<'_, App>) {
    assert!(
        h.state().handle().key_conflicts().is_empty(),
        "{:?}",
        h.state().handle().key_conflicts()
    );
}

#[test]
fn the_places_of_undo_steps_follow_the_oldest_dropped_by_the_cap() {
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    let set = |n: i64| {
        lock(&app.store())
            .set(Set::new(
                "settings/demo.n",
                Change::To(Value::Int(n)),
                Origin::User,
            ))
            .unwrap();
    };
    lock(&app.store()).set_history_cap(2);
    // A step in Photo 3, then two at the root: the cap drops the first.
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    assert!(contexts::enter(&app, PHOTO, json!(3)).ok());
    h.run();
    set(5);
    h.run();
    assert!(contexts::jump(&app, 0).ok());
    h.run();
    set(6);
    h.run();
    set(7);
    h.run();
    assert_eq!(lock(&app.store()).undo_steps().len(), 2);
    // Both steps left were made here: no toast says otherwise.
    let toasts = app.service(&TOASTS).unwrap();
    for _ in 0..2 {
        assert!(app.undo(&Origin::User).ok());
        h.run();
    }
    let shown = toasts.shown();
    assert!(
        !shown.iter().any(|t| t.toast.text.starts_with("Undone in")),
        "{shown:?}"
    );
    // A lower cap drops the oldest stamps too: one step in Photo 3, one
    // at the root, then a cap of one keeps the root's.
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    assert!(contexts::enter(&app, PHOTO, json!(3)).ok());
    h.run();
    set(8);
    h.run();
    assert!(contexts::jump(&app, 0).ok());
    h.run();
    set(9);
    h.run();
    lock(&app.store()).set_history_cap(1);
    h.run();
    assert!(app.undo(&Origin::User).ok());
    h.run();
    let shown = toasts.shown();
    assert!(
        !shown.iter().any(|t| t.toast.text.starts_with("Undone in")),
        "{shown:?}"
    );
}

#[test]
fn an_undo_out_of_sight_says_where_and_goes_there() {
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    assert!(contexts::enter(&app, PHOTO, json!(3)).ok());
    h.run();
    lock(&app.store())
        .set(Set::new(
            "settings/demo.n",
            Change::To(Value::Int(5)),
            Origin::User,
        ))
        .unwrap();
    h.run();
    assert!(contexts::jump(&app, 0).ok());
    h.run();
    assert_eq!(key(&h, "place"), "root");
    let out = app.undo(&Origin::User);
    assert!(out.ok(), "{out:?}");
    let toasts = app.service(&TOASTS).unwrap();
    let shown = toasts.shown();
    let toast = shown
        .iter()
        .find(|t| t.toast.text == "Undone in Photo 3")
        .unwrap_or_else(|| panic!("{shown:?}"));
    let action = toast.toast.action.clone().unwrap();
    assert_eq!(action.label, "Go there");
    // Its action goes back where the change was made.
    let _ = h
        .state_mut()
        .run(&action.command, action.args.clone(), Origin::User);
    h.run();
    assert_eq!(key(&h, "place"), "demo.photo");
    let state = contexts::state(&app).unwrap();
    assert_eq!(state.places[1].payload, json!(3));
    // An undo made where the user is says nothing.
    let n = toasts.shown().len();
    assert!(app.redo(&Origin::User).ok());
    assert_eq!(toasts.shown().len(), n);
}

#[test]
fn the_stack_and_its_view_come_back_after_a_restart() {
    let dir = std::env::temp_dir().join(format!("egui-pop-contexts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    {
        let mut h = harness(builder().persist(&dir));
        let app = h.state().handle().clone();
        assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
        assert!(contexts::set_mode(&app, CULLING).ok());
        assert!(contexts::push_mode(&app, COMPARE).ok());
        h.run();
        // The stack is written inside each transition: nothing waits
        // for the exit.
        assert!(dir.join("context.json").exists());
        contexts::set_view_value(&app, "list.scroll", json!(120.0));
        assert!(app.pending_documents().contains(&"view".to_string()));
        // An agent drives: a transient mode, never restored.
        assert!(contexts::set_mode(&app, AGENT).ok());
        h.run();
        // A vetoed exit writes nothing more; the documents were offered
        // to the debounced writer, written by `save`.
        h.state().save().unwrap();
        assert!(dir.join("view.json").exists());
    }
    let h = harness(builder().persist(&dir));
    let app = h.state().handle().clone();
    // Places and modes back, the pushed compare too; the transient agent
    // is not.
    assert_eq!(key(&h, "place"), "demo.album");
    assert_eq!(key(&h, "mode.orientation"), "demo.culling");
    assert_eq!(key(&h, "mode.compare"), "demo.compare");
    assert_eq!(key(&h, "mode.driver"), "");
    assert!(app.panel_visible(&SECOND));
    assert_eq!(
        contexts::view_value(&app, "list.scroll"),
        Some(json!(120.0))
    );
    assert!(
        app.events_since(0)
            .iter()
            .any(|e| e.name == "context.restored" && e.data["dropped"][0]["frame"] == "demo.agent")
    );
    // The album is gone since: the path is cut, the modes stay.
    drop(h);
    let h = harness(
        App::builder()
            .plugins(DefaultPlugins)
            .plugin(ContextsPlugin::default().valid(|frame, _| {
                (frame.as_str() == "demo.album").then(|| "it was deleted".to_string())
            }))
            .plugin(Demo)
            .persist(&dir),
    );
    assert_eq!(key(&h, "place"), "root");
    assert_eq!(key(&h, "mode.orientation"), "demo.culling");
    let app = h.state().handle().clone();
    assert!(
        app.service(&TOASTS)
            .unwrap()
            .shown()
            .iter()
            .any(|t| t.toast.text.contains("it was deleted"))
    );
    assert!(app.service(&CONTEXTS).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}
