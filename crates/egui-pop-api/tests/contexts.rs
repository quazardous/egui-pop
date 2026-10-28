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
/// A mode with an arrangement of its own.
const FOCUS: FrameId = FrameId::from_static("demo.focus");
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
        let focus = FrameSpec::mode(FOCUS, GroupId::from_static("focus"), "Focus")
            .arrangement("Album view");
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
            .frame(agent)?
            .frame(focus)?;
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
fn a_step_carries_its_place_and_a_jump_out_of_sight_says_where() {
    use egui_pop_api::history::{self, APP};
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    assert!(contexts::enter(&app, PHOTO, json!(3)).ok());
    h.run();
    lock(&app.store())
        .set(Set::new(
            "settings/demo.n",
            Change::To(Value::Int(4)),
            Origin::User,
        ))
        .unwrap();
    h.run();
    // The step knows where it was made: the album, then the photo.
    let line = history::line(&app, &APP).unwrap();
    let step = line.head().unwrap().clone();
    assert_eq!(step.place.len(), 2);
    assert_eq!(step.place[1].frame, PHOTO);
    assert_eq!(step.place[1].payload, json!(3));
    assert!(contexts::jump(&app, 0).ok());
    h.run();
    let toasts = app.service(&TOASTS).unwrap();
    // A jump to the start, from the root, walks it back: it says where.
    assert!(history::jump(&app, &APP, 0, &Origin::User).ok());
    h.run();
    assert!(
        toasts
            .shown()
            .iter()
            .any(|t| t.toast.text == "Undone in Photo 3"),
        "{:?}",
        toasts.shown()
    );
    // A jump forward again redoes it.
    assert!(history::jump(&app, &APP, step.id, &Origin::User).ok());
    h.run();
    assert!(
        toasts
            .shown()
            .iter()
            .any(|t| t.toast.text == "Redone in Photo 3"),
        "{:?}",
        toasts.shown()
    );
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

#[test]
fn frame_titles_are_translated_like_other_declarations() {
    use egui_pop_core::strings::{self, MapStrings};
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    strings::install(
        &app.egui(),
        std::sync::Arc::new(
            MapStrings::new("fr")
                .with("egui-pop-context-root", "Accueil")
                .with("frame-demo-album.title", "Album photo")
                .with("frame-demo-culling.title", "Tri")
                .with("egui-pop-context-leave-mode", "Quitter { $mode }"),
        ),
    );
    assert!(contexts::enter(&app, ALBUM, json!("Vacances")).ok());
    assert!(contexts::set_mode(&app, CULLING).ok());
    h.run();
    // The crumb and the chip read the catalogue, under keys derived from
    // the frames' ids; the payload stays as it is.
    assert!(h.query_all_by_label("Accueil").next().is_some());
    assert!(
        h.query_all_by_label("Album photo Vacances")
            .next()
            .is_some()
    );
    assert!(h.query_all_by_label("Quitter Tri").next().is_some());
    // A frame without a translation keeps its English.
    assert!(contexts::push_mode(&app, COMPARE).ok());
    h.run();
    assert!(h.query_all_by_label("Quitter Compare").next().is_some());
    assert_eq!(
        strings::frame_title(&app.egui(), "demo.album", "Album"),
        "Album photo"
    );
}

#[test]
fn the_current_arrangement_belongs_to_the_place_shown() {
    use egui_pop_api::arrangements;
    let mut h = harness(builder());
    let app = h.state().handle().clone();
    // The root's arrangement, saved by the user.
    app.move_panel(&TREE, Area::EndSide);
    assert!(arrangements::save(&app, "Mine").ok());
    assert_eq!(arrangements::current(&app), "Mine");
    // A place the user never arranged: the arrangement it declares.
    assert!(contexts::enter(&app, ALBUM, json!(1)).ok());
    h.run();
    assert_eq!(arrangements::current(&app), "Album view");
    assert!(!app.panel_visible(&TREE));
    // Switched there: the place's, not the root's.
    assert!(arrangements::switch(&app, arrangements::DEFAULT).ok());
    h.run();
    assert!(app.panel_visible(&TREE));
    assert_eq!(
        arrangements::by_frame(&app)
            .get("demo.album")
            .map(String::as_str),
        Some(arrangements::DEFAULT)
    );
    assert!(contexts::back(&app).ok());
    h.run();
    assert_eq!(arrangements::current(&app), "Mine");
    assert_eq!(app.layout().area_of(&TREE), Some(Area::EndSide));
    // Another album starts from the album's arrangement as the user chose it.
    assert!(contexts::enter(&app, ALBUM, json!(2)).ok());
    h.run();
    assert_eq!(arrangements::current(&app), arrangements::DEFAULT);
    assert_eq!(app.layout().area_of(&TREE), Some(Area::StartSide));
    assert!(contexts::back(&app).ok());
    h.run();
    // A mode with an arrangement of its own: shown while it is on, gone
    // when it is left.
    assert!(contexts::push_mode(&app, FOCUS).ok());
    h.run();
    assert_eq!(arrangements::current(&app), "Album view");
    assert!(!app.panel_visible(&TREE));
    assert!(contexts::back(&app).ok());
    h.run();
    assert_eq!(arrangements::current(&app), "Mine");
    assert_eq!(app.layout().area_of(&TREE), Some(Area::EndSide));
    // Deleting an arrangement a frame was on puts the frame back on what
    // it declares.
    assert!(contexts::enter(&app, ALBUM, json!(3)).ok());
    assert!(arrangements::save(&app, "Album mine").ok());
    assert_eq!(arrangements::current(&app), "Album mine");
    assert!(arrangements::delete(&app, "Album mine").ok());
    assert_eq!(arrangements::current(&app), "Album view");
}

#[test]
fn a_settings_file_written_before_keeps_its_arrangement_as_the_roots() {
    use egui_pop_api::arrangements;
    use egui_pop_model::values::{SettingsDoc, settings_schema};
    let dir = std::env::temp_dir().join(format!("egui-pop-contexts-arr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Written before arrangements were kept per place: one flat name.
    let mut doc = SettingsDoc::default();
    doc.values
        .insert(arrangements::CURRENT.into(), json!("Album view"));
    std::fs::write(
        dir.join("settings.json"),
        settings_schema().save(&doc).unwrap(),
    )
    .unwrap();
    let mut h = harness(builder().persist(&dir));
    let app = h.state().handle().clone();
    assert_eq!(arrangements::current(&app), "Album view");
    assert!(arrangements::by_frame(&app).is_empty());
    // Places start from their own.
    assert!(contexts::enter(&app, PHOTO, Json::Null).ok());
    h.run();
    assert_eq!(arrangements::current(&app), arrangements::DEFAULT);
    assert!(arrangements::switch(&app, "Album view").ok());
    h.state().save().unwrap();
    drop(h);
    // Both come back: the root's in its flat setting, the photo's by frame.
    let h = harness(builder().persist(&dir));
    let app = h.state().handle().clone();
    assert_eq!(key(&h, "place"), "demo.photo");
    assert_eq!(arrangements::current(&app), "Album view");
    assert!(contexts::back(&app).ok());
    assert_eq!(arrangements::current(&app), "Album view");
    let text = std::fs::read_to_string(dir.join("settings.json")).unwrap();
    assert!(text.contains("workbench.arrangement.frames"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

const CHEAT: CommandId = CommandId::from_static("cheat.photo");

/// Sets the stack's keys by hand instead of declaring frames.
struct Cheat;

impl Plugin for Cheat {
    fn id(&self) -> PluginId {
        PluginId::from_static("cheat")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![contexts::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CHEAT, "Pretend a photo", "Cheat"),
            |call| {
                call.app.set_context("place", "demo.photo");
                Ok(Json::Null)
            },
        )?;
        Ok(())
    }
}

#[test]
fn a_plugin_setting_the_stacks_keys_by_hand_is_reported() {
    let mut h = harness(builder().plugin(Cheat));
    let app = h.state().handle().clone();
    let svc = app.service(&CONTEXTS).unwrap();
    // The projection's own writes are not.
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    assert!(contexts::set_mode(&app, CULLING).ok());
    h.run();
    assert!(svc.hand_writes().is_empty());
    // A command of the plugin, then a write from anywhere.
    let reply = h.state_mut().run(&CHEAT, Json::Null, Origin::User);
    assert!(matches!(reply, egui_pop_api::host::Reply::Done(Ok(_))));
    app.set_context("mode.orientation", "demo.review");
    app.set_context("mode.orientation", "demo.review");
    let writes = svc.hand_writes();
    assert_eq!(writes.len(), 2, "{writes:?}");
    assert_eq!(writes[0].key, "place");
    assert_eq!(writes[0].command, Some(CHEAT));
    assert!(
        writes[0]
            .owner
            .as_ref()
            .is_some_and(|o| o.to_string().contains("cheat")),
        "{writes:?}"
    );
    assert_eq!(writes[1].key, "mode.orientation");
    assert_eq!(writes[1].times, 2);
    assert_eq!(writes[1].command, None);
    // A diagnostic per key, and the collector's warnings.
    let said: Vec<_> = app
        .diagnostics()
        .into_iter()
        .filter(|d| d.code == "context-key-by-hand")
        .collect();
    assert_eq!(said.len(), 2, "{said:?}");
    assert!(said[0].message.contains("declare a frame"), "{said:?}");
    let read = h
        .state_mut()
        .run(&contexts::CMD_READ, Json::Null, Origin::User);
    let egui_pop_api::host::Reply::Done(Ok(read)) = read else {
        panic!("{read:?}");
    };
    let warnings = read["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 2, "{read}");
    assert!(
        warnings[0].as_str().unwrap().contains("cheat.photo"),
        "{read}"
    );
    // The next transition puts the stack's truth back.
    assert!(contexts::set_mode(&app, REVIEW).ok());
    h.run();
    assert_eq!(key(&h, "place"), "demo.album");
    assert_eq!(svc.hand_writes().len(), 2);
}

/// A host keeping one layout per workspace.
struct Workspaces {
    inner: egui_pop_api::host::LocalHost,
    document: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

impl egui_pop_api::host::Host for Workspaces {
    fn dispatch(&mut self, d: &egui_pop_api::host::Dispatch<'_>) -> egui_pop_api::host::Reply {
        self.inner.dispatch(d)
    }
    fn journal(&self) -> Option<&egui_pop_api::host::Journal> {
        self.inner.journal()
    }
    fn journal_mut(&mut self) -> Option<&mut egui_pop_api::host::Journal> {
        self.inner.journal_mut()
    }
    fn store(&self) -> egui_pop_core::binding::SharedStore {
        self.inner.store()
    }
    fn set_waker(&mut self, waker: egui_pop_api::host::Waker) {
        self.inner.set_waker(waker);
    }
    fn layout_document(&self) -> Option<String> {
        self.document.lock().unwrap().clone()
    }
}

#[test]
fn each_workspace_keeps_its_own_stack_beside_its_layout() {
    let dir = std::env::temp_dir().join(format!("egui-pop-contexts-ws-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let document = std::sync::Arc::new(std::sync::Mutex::new(Some(
        "workspaces/a/layout".to_string(),
    )));
    let host = || Workspaces {
        inner: egui_pop_api::host::LocalHost::default(),
        document: document.clone(),
    };
    let mut h = harness(builder().persist(&dir).host(host()));
    let app = h.state().handle().clone();
    assert_eq!(
        contexts::documents(&app),
        (
            "workspaces/a/context".to_string(),
            "workspaces/a/view".to_string()
        )
    );
    assert!(contexts::enter(&app, ALBUM, json!("a")).ok());
    assert!(contexts::set_mode(&app, CULLING).ok());
    h.run();
    assert!(dir.join("workspaces/a/context.json").exists());
    // Another workspace in the same window: its own stack, empty.
    *document.lock().unwrap() = Some("workspaces/b/layout".into());
    contexts::reload(&app);
    h.run();
    assert_eq!(key(&h, "place"), "root");
    assert_eq!(key(&h, "mode.orientation"), "");
    assert!(contexts::enter(&app, PHOTO, json!(7)).ok());
    h.run();
    h.state().save().unwrap();
    // Back to the first: its stack as it was left.
    *document.lock().unwrap() = Some("workspaces/a/layout".into());
    contexts::reload(&app);
    h.run();
    assert_eq!(key(&h, "place"), "demo.album");
    assert_eq!(key(&h, "mode.orientation"), "demo.culling");
    drop(h);
    // And after a restart, each its own.
    *document.lock().unwrap() = Some("workspaces/b/layout".into());
    let h = harness(builder().persist(&dir).host(host()));
    assert_eq!(key(&h, "place"), "demo.photo");
    assert_eq!(key(&h, "mode.orientation"), "");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A place whose payload is an object, `{"id": "…"}` (a sub-workspace):
/// its crumb says the sub-workspace's name, its arrangement depends on
/// which one, a key binds in one of them only.
const SPACE: FrameId = FrameId::from_static("demo.space");

struct Spaces;

impl Plugin for Spaces {
    fn id(&self) -> PluginId {
        PluginId::from_static("spaces")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![contexts::PLUGIN, PluginId::from_static("demo")]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let space = FrameSpec::place(SPACE, "Space")
            .label(|c| match c.payload["id"].as_str() {
                Some("dark") => Some("Darkroom".into()),
                Some(id) => Some(format!("{} {id}", c.title)),
                None => None,
            })
            .arrangement_for(|p| (p["id"] == "dark").then(|| "Album view".to_string()));
        ctx.frame(space)?.keybinding(
            "K",
            PICK,
            "place == 'demo.space' && payload.id == 'dark'",
            egui_pop_model::keymap::Layer::Default,
        )?;
        Ok(())
    }
}

fn spaces() -> AppBuilder {
    App::builder()
        .plugins(DefaultPlugins)
        .plugin(ContextsPlugin::default().root("Main"))
        .plugin(Demo)
        .plugin(Spaces)
}

#[test]
fn a_place_labels_its_crumb_from_its_payload() {
    let mut h = harness(spaces());
    let app = h.state().handle().clone();
    // The root's crumb says what the application names it.
    assert!(h.query_all_by_label("Main").next().is_some());
    assert!(contexts::enter(&app, SPACE, json!({ "id": "dark" })).ok());
    h.run();
    assert!(h.query_all_by_label("Darkroom").next().is_some());
    assert!(
        h.query_all_by_label("Space {\"id\":\"dark\"}")
            .next()
            .is_none()
    );
    // `context.read` says the crumbs the same way.
    let read = h
        .state_mut()
        .run(&contexts::CMD_READ, Json::Null, Origin::User);
    let egui_pop_api::host::Reply::Done(Ok(read)) = read else {
        panic!("{read:?}");
    };
    assert_eq!(
        read["projection"]["crumbs"],
        json!(["Main", "Darkroom"]),
        "{read}"
    );
    // The label gets the translated title; `None` keeps the default.
    assert!(
        contexts::go(
            &app,
            vec![contexts::Entry {
                frame: SPACE,
                payload: json!({ "id": "map" }),
            }]
        )
        .ok()
    );
    h.run();
    assert!(h.query_all_by_label("Space map").next().is_some());
    assert!(contexts::enter(&app, SPACE, json!(7)).ok());
    h.run();
    assert!(h.query_all_by_label("Space 7").next().is_some());
    // A crumb still jumps back to its level.
    h.query_all_by_label("Main").next().unwrap().click();
    h.run();
    assert_eq!(key(&h, "place"), "root");
}

#[test]
fn a_place_starts_from_the_arrangement_its_payload_chooses() {
    use egui_pop_api::arrangements;
    let mut h = harness(spaces());
    let app = h.state().handle().clone();
    assert!(app.panel_visible(&TREE));
    // First entry of the dark one: the arrangement its payload chooses.
    assert!(contexts::enter(&app, SPACE, json!({ "id": "dark" })).ok());
    h.run();
    assert!(!app.panel_visible(&TREE));
    assert_eq!(arrangements::current(&app), "Album view");
    // Changed there, then left.
    app.move_panel(&LOG, Area::EndSide);
    h.run();
    assert!(contexts::back(&app).ok());
    h.run();
    // Another one, which chooses nothing: the frame's arrangement (none
    // declared: the layout shown).
    assert!(contexts::enter(&app, SPACE, json!({ "id": "map" })).ok());
    h.run();
    assert!(app.panel_visible(&TREE));
    assert_eq!(app.layout().area_of(&LOG), Some(Area::Bottom));
    assert!(contexts::back(&app).ok());
    h.run();
    // Back in the dark one: the layout it was left with wins.
    assert!(contexts::enter(&app, SPACE, json!({ "id": "dark" })).ok());
    h.run();
    assert!(!app.panel_visible(&TREE));
    assert_eq!(app.layout().area_of(&LOG), Some(Area::EndSide));
}

#[test]
fn a_key_binds_on_a_payload_field() {
    let mut h = harness(spaces());
    let app = h.state().handle().clone();
    assert!(contexts::enter(&app, SPACE, json!({ "id": "map" })).ok());
    h.run();
    assert_eq!(key(&h, "payload.id"), "map");
    h.key_press(Key::K);
    h.run();
    assert_eq!(key(&h, "demoLast"), "", "K does nothing in another space");
    assert!(contexts::back(&app).ok());
    assert!(contexts::enter(&app, SPACE, json!({ "id": "dark" })).ok());
    h.run();
    h.key_press(Key::K);
    h.run();
    assert_eq!(key(&h, "demoLast"), "pick");
    // Left: the payload's keys go with it.
    assert!(contexts::back(&app).ok());
    h.run();
    assert_eq!(key(&h, "payload.id"), "");
}

/// A place that declares no arrangement (the photo), entered for the
/// first time: its layout does not depend on where the user came from.
#[test]
fn a_place_without_arrangement_looks_the_same_from_anywhere() {
    let photo = || {
        vec![contexts::Entry {
            frame: PHOTO,
            payload: json!("p1"),
        }]
    };
    // Entered from the root: the root's layout.
    let mut a = harness(builder());
    let app = a.state().handle().clone();
    assert!(contexts::go(&app, photo()).ok());
    a.run();
    let from_root = app.layout();
    assert!(app.panel_visible(&TREE));
    // Entered from the album, whose arrangement hides the tree: the
    // root's layout too, not the album's.
    let mut b = harness(builder());
    let app = b.state().handle().clone();
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    b.run();
    assert!(!app.panel_visible(&TREE));
    assert!(contexts::go(&app, photo()).ok());
    b.run();
    assert_eq!(app.layout(), from_root);
    assert!(app.panel_visible(&TREE));
    // Under the album, it starts from the album's layout, whether the user
    // comes from the album or from the root.
    let under = |payload: &str| {
        vec![
            contexts::Entry {
                frame: ALBUM,
                payload: Json::Null,
            },
            contexts::Entry {
                frame: PHOTO,
                payload: json!(payload),
            },
        ]
    };
    assert!(contexts::go(&app, under("p2")).ok());
    b.run();
    let from_album = app.layout();
    assert!(!app.panel_visible(&TREE));
    let mut c = harness(builder());
    let app = c.state().handle().clone();
    assert!(contexts::go(&app, under("p2")).ok());
    c.run();
    assert!(!app.panel_visible(&TREE));
    assert_eq!(app.layout().area_of(&LOG), from_album.area_of(&LOG));
}

/// The root crumb names what is open (a workspace), from the
/// application's own state, translated, and follows it when it changes.
#[test]
fn the_root_crumb_says_what_the_application_names_it() {
    use egui_pop_core::strings::{self, MapStrings, StrKey};
    const WORKSPACE: StrKey = StrKey::new("demo-workspace", "Workspace { $name }");
    const LIBRARY: StrKey = StrKey::new("demo-library", "Library");
    let open = std::sync::Arc::new(std::sync::Mutex::new(Some("Holidays".to_string())));
    let name = open.clone();
    let mut h = harness(
        App::builder()
            .plugins(DefaultPlugins)
            .plugin(
                ContextsPlugin::default()
                    .root_key(LIBRARY)
                    .root_label(move |c| {
                        let name = lock(&name).clone()?;
                        Some(strings::tr_args(c.ctx, &WORKSPACE, &[("name", &name)]))
                    }),
            )
            .plugin(Demo),
    );
    let app = h.state().handle().clone();
    let read = |h: &mut Harness<'_, App>| {
        let read = h
            .state_mut()
            .run(&contexts::CMD_READ, Json::Null, Origin::User);
        let egui_pop_api::host::Reply::Done(Ok(read)) = read else {
            panic!("{read:?}");
        };
        read["projection"]["crumbs"].clone()
    };
    // The breadcrumb, the status bar and `context.read` say the label.
    assert!(h.query_all_by_label("Workspace Holidays").next().is_some());
    assert_eq!(contexts::crumbs(&app), vec!["Workspace Holidays"]);
    assert_eq!(read(&mut h), json!(["Workspace Holidays"]));
    // Another workspace opened: the crumb follows.
    *lock(&open) = Some("Portraits".into());
    h.run();
    assert!(h.query_all_by_label("Workspace Portraits").next().is_some());
    assert!(h.query_all_by_label("Workspace Holidays").next().is_none());
    // Translated, with the catalogue's words.
    strings::install(
        &app.egui(),
        std::sync::Arc::new(
            MapStrings::new("fr")
                .with("demo-workspace", "Espace { $name }")
                .with("demo-library", "Bibliothèque"),
        ),
    );
    assert!(contexts::enter(&app, ALBUM, Json::Null).ok());
    h.run();
    assert_eq!(read(&mut h), json!(["Espace Portraits", "Album"]));
    assert!(h.query_all_by_label("Espace Portraits").next().is_some());
    // Nothing open: the label gives way to the title, translated.
    *lock(&open) = None;
    h.run();
    assert_eq!(contexts::crumbs(&app), vec!["Bibliothèque", "Album"]);
    assert!(h.query_all_by_label("Bibliothèque").next().is_some());
    // The root's crumb still jumps back to the root.
    h.query_all_by_label("Bibliothèque").next().unwrap().click();
    h.run();
    assert_eq!(key(&h, "place"), "root");
}
