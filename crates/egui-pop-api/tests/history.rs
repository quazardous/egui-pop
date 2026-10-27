//! The history contract (ADR 0014): `StoreHistory` over the value store,
//! local scopes picked by focus, the application's history otherwise,
//! undo, redo and jump as vetoable steps, merging sealed by the focus,
//! the settings window's own history.

use std::sync::{Arc, Mutex};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::history::{
    self, APP, History, Label, Line, LocalScope, Refusal, RefusalKind, Request, SETTINGS, ScopeId,
    ScopeInfo, StepInfo, StoreHistory, Target,
};
use egui_pop_api::steps::Before;
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_model::clock::ManualClock;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, OwnerId, PluginId};
use egui_pop_model::values::{self, Change, Descriptor, Set, Value};
use serde_json::{Value as Json, json};

const N: &str = "doc/n";
const PAD: &str = "pad";
const PAD_SCOPE: ScopeId = ScopeId::from_static("pad");

/// A sketch pad's own history: strokes done and undone.
#[derive(Clone, Default)]
struct Strokes(Arc<Mutex<(Vec<String>, Vec<String>)>>);

impl Strokes {
    fn with(done: &[&str]) -> Self {
        let s = Self::default();
        s.0.lock().unwrap().0 = done.iter().map(|d| d.to_string()).collect();
        s
    }

    fn done(&self) -> Vec<String> {
        self.0.lock().unwrap().0.clone()
    }
}

impl LocalScope for Strokes {
    fn info(&self) -> ScopeInfo {
        let mut i = ScopeInfo::new(PAD_SCOPE, "Pad");
        i.local = true;
        i
    }

    fn line(&self, _: &egui_pop_api::AppHandle) -> Line {
        let (done, undone) = self.0.lock().unwrap().clone();
        let step = |id: usize, l: &String| {
            StepInfo::new(id as u64, PAD_SCOPE, Label::text(l.as_str()), Origin::User)
        };
        Line {
            undo: done
                .iter()
                .enumerate()
                .map(|(i, l)| step(i + 1, l))
                .collect(),
            redo: undone
                .iter()
                .enumerate()
                .map(|(j, l)| step(done.len() + undone.len() - j, l))
                .collect(),
            rev: (done.len() * 100 + undone.len()) as u64,
        }
    }

    fn undo(&self, app: &egui_pop_api::AppHandle, _: &Origin) -> Result<Option<StepInfo>, Refusal> {
        let head = self.line(app).head().cloned();
        let mut s = self.0.lock().unwrap();
        if let Some(l) = s.0.pop() {
            s.1.push(l);
        }
        Ok(head)
    }

    fn redo(&self, app: &egui_pop_api::AppHandle, _: &Origin) -> Result<Option<StepInfo>, Refusal> {
        let next = self.line(app).next().cloned();
        let mut s = self.0.lock().unwrap();
        if let Some(l) = s.1.pop() {
            s.0.push(l);
        }
        Ok(next)
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

/// A pad (a surface with its local scope, a field and a button in it)
/// and a button outside it; `doc/n` in the store.
fn pad(strokes: Strokes) -> P {
    P(
        "pad",
        Box::new(move |ctx| {
            ctx.local_scope(PAD, strokes.clone());
            ctx.block(
                zones::MAIN,
                Block::add(BlockId::from_static("pad.block"), 0, |ui, _| {
                    egui_pop_core::focus::scope(ui, PAD, &[], |ui| {
                        ui.add_space(20.0);
                        let _ = ui.button("In the pad");
                        let mut text = String::from("note");
                        ui.add(egui::TextEdit::singleline(&mut text).id_salt("pad.note"));
                        ui.add_space(20.0);
                    });
                    ui.add_space(40.0);
                    let _ = ui.button("Outside");
                }),
            );
            lock(&ctx.app().store())
                .declare(&OwnerId::USER, N, Descriptor::int("N", 0, 99, 0))
                .unwrap();
            Ok(())
        }),
    )
}

fn harness(app: App) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn set(app: &egui_pop_api::AppHandle, n: i64) {
    lock(&app.store())
        .set(Set::new(N, Change::To(Value::Int(n)), Origin::User))
        .unwrap();
}

fn n(app: &egui_pop_api::AppHandle) -> Value {
    lock(&app.store()).get(N).unwrap().value.clone()
}

fn events(app: &egui_pop_api::AppHandle, since: u64, name: &str) -> Vec<Json> {
    app.events_since(since)
        .into_iter()
        .filter(|e| e.name == name)
        .map(|e| e.data)
        .collect()
}

#[test]
fn ctrl_z_goes_to_the_focused_local_scope_else_the_application() {
    let strokes = Strokes::with(&["stroke"]);
    let app = App::builder()
        .plugin(pad(strokes.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let a = app.handle().clone();
    set(&a, 3);
    let mut h = harness(app);
    // Focus in the pad: Ctrl+Z is the pad's, Edit > Undo says so.
    h.get_by_label("In the pad").click();
    h.run();
    assert!(matches!(history::target(&a, false), Some(Target::Local(_))));
    assert_eq!(a.undo_label().as_deref(), Some("stroke"));
    let undo = a.command(&egui_pop_api::app::CMD_UNDO).unwrap();
    assert_eq!(a.command_title(&undo), "Undo stroke");
    let before = a.last_event();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert!(strokes.done().is_empty());
    assert_eq!(n(&a), Value::Int(3), "the application's step is left");
    let undone = events(&a, before, "edit.undone");
    assert_eq!(undone[0]["scope"], "pad");
    // The pad has nothing left: Ctrl+Z there does nothing, it never
    // falls through to the application.
    assert!(!a.can_undo());
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(n(&a), Value::Int(3));
    // Ctrl+Shift+Z redoes the stroke.
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run();
    assert_eq!(strokes.done(), ["stroke"]);
    // Out of the pad: the application's history.
    h.get_by_label("Outside").click();
    h.run();
    assert!(matches!(history::target(&a, false), Some(Target::Owner(s)) if s == APP));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(n(&a), Value::Int(0));
    assert_eq!(strokes.done(), ["stroke"]);
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run();
    assert_eq!(n(&a), Value::Int(3));
}

#[test]
fn a_text_field_keeps_its_own_undo_inside_a_local_scope() {
    let strokes = Strokes::with(&["stroke"]);
    let app = App::builder()
        .plugin(pad(strokes.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let a = app.handle().clone();
    let mut h = harness(app);
    h.get_by_role(egui::accesskit::Role::TextInput).click();
    h.run();
    assert_eq!(a.when_context().get("textInputFocus"), "true");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(strokes.done(), ["stroke"], "Edit > Undo does not apply");
}

#[test]
fn a_gesture_in_progress_or_a_hook_refuses_and_the_journal_says_why() {
    let app = App::builder()
        .plugin(pad(Strokes::default()))
        .plugin(P(
            "guard",
            Box::new(|ctx| {
                ctx.before(history::STEP_JUMP, |_, args| {
                    if args["to"] == 0 {
                        Before::Veto("not to the start".into())
                    } else {
                        Before::Go
                    }
                });
                Ok(())
            }),
        ))
        .build(&egui::Context::default())
        .unwrap();
    let a = app.handle().clone();
    set(&a, 1);
    set(&a, 2);
    // A drag going on the value: undo waits for it.
    let g = lock(&a.store()).begin_gesture(Origin::User, "drag n");
    lock(&a.store())
        .set(Set::new(N, Change::To(Value::Int(5)), Origin::User).gesture(g))
        .unwrap();
    let before = a.last_event();
    let out = history::undo_in(&a, &APP, &Origin::User);
    let err = out.result().unwrap().clone().unwrap_err();
    assert!(err.contains("drag n"), "{err}");
    let refused = events(&a, before, "history.refused");
    assert_eq!(refused[0]["kind"], "held");
    assert_eq!(refused[0]["scope"], "app");
    lock(&a.store()).cancel(g);
    // A before-hook vetoes a jump: nothing moves, the journal says so.
    let before = a.last_event();
    let out = history::jump(&a, &APP, 0, &Origin::User);
    assert_eq!(out.vetoed(), Some("not to the start"));
    assert_eq!(n(&a), Value::Int(2));
    assert_eq!(events(&a, before, "history.refused")[0]["kind"], "vetoed");
}

#[test]
fn a_jump_walks_to_a_step_and_publishes_the_change() {
    let strokes = Strokes::with(&["a", "b", "c"]);
    let app = App::builder()
        .plugin(pad(strokes.clone()))
        .build(&egui::Context::default())
        .unwrap();
    let a = app.handle().clone();
    for v in 1..=3 {
        set(&a, v);
    }
    let line = history::line(&a, &APP).unwrap();
    let first = line.undo[0].id;
    let before = a.last_event();
    let out = history::jump(&a, &APP, first, &Origin::User);
    let r = out.result().unwrap().clone().unwrap();
    assert_eq!(r["steps"].as_array().unwrap().len(), 2);
    assert_eq!(n(&a), Value::Int(1));
    let changed = events(&a, before, "history.changed");
    assert_eq!(changed[0]["scope"], "app");
    assert_eq!(changed[0]["head"], first);
    // Redo goes on from there, in the scope jumped in.
    assert!(a.redo(&Origin::User).ok());
    assert_eq!(n(&a), Value::Int(2));
    // A local scope jumps too: back to its start, then to its last.
    history::jump(&a, &PAD_SCOPE, 0, &Origin::User);
    assert!(strokes.done().is_empty());
    let last = history::line(&a, &PAD_SCOPE).unwrap().redo[0].id;
    history::jump(&a, &PAD_SCOPE, last, &Origin::User);
    assert_eq!(strokes.done(), ["a", "b", "c"]);
    // To a step that is not there: refused, nothing moves.
    let out = history::jump(&a, &APP, 999, &Origin::User);
    assert!(out.result().unwrap().is_err());
    assert_eq!(n(&a), Value::Int(2));
}

#[test]
fn a_before_hook_redirects_an_undo_to_another_scope() {
    let strokes = Strokes::with(&["stroke"]);
    let app = App::builder()
        .plugin(pad(strokes.clone()))
        .plugin(P(
            "redirect",
            Box::new(|ctx| {
                ctx.before(egui_pop_api::app::STEP_UNDO, |_, args| {
                    let mut a = args.clone();
                    a["scope"] = json!("pad");
                    Before::Redirect(a)
                });
                Ok(())
            }),
        ))
        .build(&egui::Context::default())
        .unwrap();
    let a = app.handle().clone();
    set(&a, 4);
    let out = a.undo(&Origin::User);
    assert_eq!(out.result().unwrap().clone().unwrap()["scope"], "pad");
    assert!(strokes.done().is_empty());
    assert_eq!(n(&a), Value::Int(4));
}

#[test]
fn quick_repeats_merge_and_the_focus_moving_seals_them() {
    let app = App::builder()
        .plugin(pad(Strokes::default()))
        .build(&egui::Context::default())
        .unwrap();
    let a = app.handle().clone();
    let clock = ManualClock::new(10.0);
    lock(&a.store()).set_clock(Some(clock.shared()));
    let mut h = harness(app);
    let bump = |v: i64| {
        lock(&a.store())
            .set(Set::new(N, Change::To(Value::Int(v)), Origin::User).merge(N))
            .unwrap();
    };
    bump(1);
    clock.advance(0.3);
    bump(2);
    assert_eq!(history::line(&a, &APP).unwrap().undo.len(), 1);
    // The focus moves (into the note): the next repeat is a new step.
    h.get_by_role(egui::accesskit::Role::TextInput).click();
    h.run();
    clock.advance(0.3);
    bump(3);
    assert_eq!(history::line(&a, &APP).unwrap().undo.len(), 2);
    // The application keeps the store's own clock.
    h.run();
    assert_eq!(lock(&a.store()).now(), clock.shared().now());
}

#[test]
fn the_application_gives_the_store_a_clock() {
    let app = App::builder()
        .plugin(pad(Strokes::default()))
        .build(&egui::Context::default())
        .unwrap();
    let a = app.handle().clone();
    let _h = harness(app);
    assert!(lock(&a.store()).has_clock());
}

#[test]
fn the_store_owner_answers_dry_runs_and_stale_requests() {
    let store = egui_pop_core::binding::SharedStore::default();
    lock(&store)
        .declare(&OwnerId::USER, N, Descriptor::int("N", 0, 99, 0))
        .unwrap();
    lock(&store)
        .set(Set::new(N, Change::To(Value::Int(7)), Origin::User))
        .unwrap();
    let mut owner = StoreHistory::new(store.clone());
    assert_eq!(owner.scopes()[0].id, APP);
    assert_eq!(owner.scopes()[0].cap, values::HISTORY_CAP);
    let rev = owner.rev(&APP);
    // A dry run names the step and changes nothing.
    let history::Answer::Done(Ok(w)) = owner.undo(&Request::new(APP).dry_run(), &Origin::User)
    else {
        panic!("answered now")
    };
    assert!(w.dry_run);
    assert_eq!(w.steps.len(), 1);
    assert_eq!(lock(&store).get(N).unwrap().value, Value::Int(7));
    assert_eq!(owner.rev(&APP), rev);
    // A request from an older revision is refused.
    let history::Answer::Done(Err(r)) =
        owner.undo(&Request::new(APP).if_rev(rev + 5), &Origin::User)
    else {
        panic!("refused")
    };
    assert_eq!(r.kind, RefusalKind::Stale);
    // An unknown scope is refused; a real undo moves the revision.
    let history::Answer::Done(Err(r)) = owner.redo(&Request::new(SETTINGS), &Origin::User) else {
        panic!("refused")
    };
    assert_eq!(r.kind, RefusalKind::Unknown);
    let history::Answer::Done(Ok(w)) = owner.undo(&Request::new(APP).if_rev(rev), &Origin::User)
    else {
        panic!("done")
    };
    assert_eq!(w.head, 0);
    assert!(owner.rev(&APP) > rev);
    assert_eq!(lock(&store).get(N).unwrap().value, Value::Int(0));
    // A group is one step.
    let g = owner.group(&Origin::User, &Label::text("Import")).unwrap();
    {
        let mut s = lock(&store);
        for v in [1, 2, 3] {
            s.set(
                Set::new(N, Change::To(Value::Int(v)), Origin::User).gesture(values::GestureId(g)),
            )
            .unwrap();
        }
    }
    owner.close(g, true);
    let line = owner.line(&APP).unwrap();
    assert_eq!(line.undo.len(), 1);
    assert_eq!(line.undo[0].label.text, "Import");
    assert_eq!(line.undo[0].group, Some(g));
    assert_eq!(owner.tips(&APP).unwrap().undo, line.undo);
}

#[test]
fn a_store_write_entered_in_a_history_is_undone_there_alone() {
    let app = App::builder()
        .plugin(pad(Strokes::default()))
        .build(&egui::Context::default())
        .unwrap();
    let a = app.handle().clone();
    set(&a, 1);
    {
        let _settings = values::enter_history(&SETTINGS);
        set(&a, 2);
    }
    // The owner's routing never sees the SETTINGS history: its undo
    // takes back the application's step alone (1 back to 0).
    assert_eq!(history::line(&a, &APP).unwrap().undo.len(), 1);
    assert!(a.undo(&Origin::User).ok());
    assert_eq!(n(&a), Value::Int(0));
    assert_eq!(lock(&a.store()).undo_steps_in(&SETTINGS).len(), 1);
}
