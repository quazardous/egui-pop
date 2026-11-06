//! The views driven as a user would: pointer and keys through kittest,
//! their steps through an application.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::Before;
use egui_pop_api::{App, AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::icon::Icon;
use egui_pop_model::ids::{OwnerId, PluginId};
use egui_pop_views::list::{ListRows, ListState, ListView};
use egui_pop_views::steps::{TABLE_SORT, TREE_DROP};
use egui_pop_views::table::{self, Column, DataTable, TableRows, TableState};
use egui_pop_views::tree::{
    self, FilterField, Node, Position, Sink, TreeSource, TreeState, TreeView,
};
use egui_pop_views::{Mode, ViewsPlugin};

/// Folders `a` to `e`, each with three files; `slow` answers only when
/// told to, from a thread.
struct Source {
    asked: Arc<Mutex<Vec<Option<String>>>>,
    slow: Option<mpsc::Sender<Sink<String>>>,
}

fn children(parent: Option<&String>) -> Vec<Node<String>> {
    match parent {
        None => ["a", "b", "c", "d", "e"]
            .iter()
            .map(|k| Node::branch(k.to_string(), format!("Folder {}", k.to_uppercase())))
            .collect(),
        Some(p) => (1..=3)
            .map(|i| Node::leaf(format!("{p}/{i}"), format!("File {p}{i}")))
            .collect(),
    }
}

impl TreeSource<String> for Source {
    fn load(&self, parent: Option<&String>, sink: Sink<String>) {
        self.asked.lock().unwrap().push(parent.cloned());
        match (&self.slow, parent) {
            (Some(tx), Some(_)) => tx.send(sink).unwrap(),
            _ => sink.deliver(children(parent)),
        }
    }
}

/// Vetoes drops into `e`.
struct Guard;

impl Plugin for Guard {
    fn id(&self) -> PluginId {
        PluginId::from_static("guard")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(TREE_DROP, |_, args| {
            if args["target"] == "e" {
                Before::Veto("e is read-only".into())
            } else {
                Before::Go
            }
        });
        ctx.before(TABLE_SORT, |_, args| {
            if args["column"] == "size" {
                Before::Veto("not by size".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

struct TreeApp {
    app: Option<App>,
    state: TreeState<String>,
    source: Source,
    signals: Vec<tree::Signal<String>>,
}

impl TreeApp {
    fn handle(&self) -> AppHandle {
        self.app.as_ref().unwrap().handle().clone()
    }
}

fn tree_harness(slow: Option<mpsc::Sender<Sink<String>>>) -> Harness<'static, TreeApp> {
    let app = TreeApp {
        app: None,
        state: TreeState::new(),
        source: Source {
            asked: Arc::default(),
            slow,
        },
        signals: Vec::new(),
    };
    Harness::builder()
        .with_size(egui::vec2(400.0, 600.0))
        .build_ui_state(
            |ui, t: &mut TreeApp| {
                if t.app.is_none() {
                    t.app = Some(
                        App::builder()
                            .plugin(ViewsPlugin)
                            .plugin(Guard)
                            .build(ui.ctx())
                            .unwrap(),
                    );
                }
                let h = t.handle();
                let out = TreeView::new("files")
                    .label("Files")
                    .mode(Mode::Multiple)
                    .drag_drop(true)
                    .rename(true)
                    .app(&h)
                    .show(ui, &mut t.state, &t.source);
                t.signals.extend(out.signals);
            },
            app,
        )
}

#[test]
fn branches_load_on_demand_and_show_loading_meanwhile() {
    let (tx, rx) = mpsc::channel();
    let mut h = tree_harness(Some(tx));
    h.run();
    assert!(h.query_by_label("Folder A").is_some());
    assert_eq!(*h.state().source.asked.lock().unwrap(), vec![None]);
    // Double click opens a branch: its children are asked for once.
    egui_pop_test_double_click(&mut h, "Folder B");
    h.run();
    assert!(h.query_by_label("Loading…").is_some());
    assert!(h.state().state.model().is_expanded(&"b".into()));
    let sink = rx.try_recv().expect("asked");
    assert_eq!(sink.parent(), Some(&"b".to_string()));
    std::thread::spawn(move || sink.deliver(children(Some(&"b".into()))))
        .join()
        .unwrap();
    h.run();
    assert!(h.query_by_label("Loading…").is_none());
    assert!(h.query_by_label("File b2").is_some());
    assert_eq!(
        h.state().source.asked.lock().unwrap().len(),
        2,
        "asked once per branch"
    );
    // A failure shows its reason and a retry.
    h.state().state.handle().reload(Some("b".into()));
    h.run();
    let sink = rx.try_recv().expect("asked again");
    sink.fail("offline");
    h.run();
    assert!(h.query_by_label("Could not load: offline").is_some());
    h.get_by_label("Retry").click();
    h.run();
    rx.try_recv()
        .expect("retried")
        .deliver(children(Some(&"b".into())));
    h.run();
    assert!(h.query_by_label("File b3").is_some());
}

/// Double-clicks a node by its label (kittest queues events one step
/// apart, beyond egui's double-click delay: all four in one step).
fn egui_pop_test_double_click(h: &mut Harness<'_, TreeApp>, label: &str) {
    let pos = h.get_by_label(label).rect().center();
    let events = &mut h.input_mut().events;
    events.push(egui::Event::PointerMoved(pos));
    for _ in 0..2 {
        for pressed in [true, false] {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
    }
}

#[test]
fn changes_under_an_expanded_tree_keep_selection_and_expansion() {
    let mut h = tree_harness(None);
    h.run();
    h.state_mut().state.set_expanded(&"a".into(), true);
    h.run();
    h.get_by_label("File a2").click();
    h.run();
    h.get_by_label("File a3")
        .click_modifiers(Modifiers::COMMAND);
    h.run();
    assert_eq!(h.state().state.selection().keys(), &["a/2", "a/3"]);
    let edit = h.state().state.handle();
    // From another thread, at any time.
    std::thread::spawn(move || {
        edit.insert(
            Some("a".into()),
            Some(0),
            Node::leaf("a/0".into(), "File a0"),
        );
        edit.rename("a/3".into(), "File a3 renamed");
        edit.remove("a/2".into());
        edit.move_to("c".into(), None, Some(0));
    })
    .join()
    .unwrap();
    h.run();
    let s = &h.state().state;
    assert_eq!(
        s.selection().keys(),
        &["a/3"],
        "the removed one left the selection"
    );
    assert!(s.model().is_expanded(&"a".into()));
    let labels: Vec<String> = s
        .model()
        .rows_now()
        .iter()
        .filter_map(|r| r.key())
        .map(|k| s.model().node(k).unwrap().label.clone())
        .collect();
    assert_eq!(
        labels,
        [
            "Folder C",
            "Folder A",
            "File a0",
            "File a1",
            "File a3 renamed",
            "Folder B",
            "Folder D",
            "Folder E"
        ]
    );
}

#[test]
fn a_tree_not_drawn_drops_its_edits_past_the_cap_and_loads_again() {
    let mut h = tree_harness(None);
    h.run();
    h.state_mut().state.set_expanded(&"a".into(), true);
    h.run();
    let asked = h.state().source.asked.lock().unwrap().len();
    // Its host stops drawing it (no pass); a worker keeps renaming.
    let edit = h.state().state.handle();
    std::thread::spawn(move || {
        for i in 0..tree::EDITS_WAITING + 100 {
            edit.rename("a/1".into(), format!("File a1 v{i}"));
        }
    })
    .join()
    .unwrap();
    // The queue stayed bounded: the edits past the cap gave way to
    // loading everything again.
    assert_eq!(h.state().state.waiting(), 99);
    assert_eq!(h.state().state.dropped(), tree::EDITS_WAITING as u64 + 1);
    // Drawn again: every branch shown is asked for again, and agents are
    // told on the journal.
    h.run();
    let again = h.state().source.asked.lock().unwrap()[asked..].to_vec();
    assert!(
        again.contains(&None) && again.contains(&Some("a".into())),
        "{again:?}"
    );
    assert_eq!(h.state().state.waiting(), 0);
    let told = h
        .state()
        .handle()
        .events_since(0)
        .into_iter()
        .find(|e| e.name == "tree.overflowed")
        .expect("a tree.overflowed event");
    assert_eq!(told.data["tree"], "files");
    assert_eq!(told.data["dropped"], tree::EDITS_WAITING as u64 + 1);
    assert!(h.query_by_label("Folder A").is_some());
}

#[test]
fn the_keyboard_moves_opens_closes_and_searches() {
    let mut h = tree_harness(None);
    h.run();
    h.get_by_label("Folder A").click();
    h.run();
    let cursor = |h: &Harness<'_, TreeApp>| h.state().state.selection().cursor().cloned();
    h.key_press(Key::ArrowDown);
    h.run();
    assert_eq!(cursor(&h).as_deref(), Some("b"));
    h.key_press(Key::ArrowRight);
    h.run();
    assert!(h.state().state.model().is_expanded(&"b".into()));
    h.key_press(Key::ArrowRight);
    h.run();
    assert_eq!(cursor(&h).as_deref(), Some("b/1"));
    h.key_press(Key::ArrowLeft);
    h.run();
    assert_eq!(cursor(&h).as_deref(), Some("b"), "to the parent");
    h.key_press(Key::End);
    h.run();
    assert_eq!(cursor(&h).as_deref(), Some("e"));
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowUp);
    h.run();
    assert_eq!(h.state().state.selection().keys(), &["d", "e"]);
    h.key_press(Key::Home);
    h.run();
    assert_eq!(cursor(&h).as_deref(), Some("a"));
    // Type-ahead: "fi" finds the first file.
    h.event(egui::Event::Text("f".into()));
    h.event(egui::Event::Text("i".into()));
    h.run();
    assert_eq!(cursor(&h).as_deref(), Some("b/1"));
    // F2 renames in place; Enter commits.
    h.key_press(Key::F2);
    h.run();
    assert_eq!(h.state().state.renaming(), Some(&"b/1".to_string()));
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(egui::Event::Text("Notes".into()));
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        h.state().state.model().node(&"b/1".into()).unwrap().label,
        "Notes"
    );
    assert!(h.state().signals.iter().any(|s| matches!(s,
        tree::Signal::Rename { key, from, to } if key == "b/1" && from == "File b1" && to == "Notes")));
}

fn drag<S>(h: &mut Harness<'_, S>, from: &str, to: &str, at: f32) {
    let a = h.get_by_label(from).rect().center();
    let r = h.get_by_label(to).rect();
    let b = egui::pos2(r.center().x, r.top() + r.height() * at);
    h.drag_at(a);
    h.run();
    for i in 1..=4 {
        h.hover_at(a + (b - a) * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(b);
    h.run();
}

#[test]
fn drops_move_nodes_unless_vetoed() {
    let mut h = tree_harness(None);
    h.run();
    // Into a branch not loaded, a node waits for it to load: open it.
    h.state_mut().state.set_expanded(&"c".into(), true);
    h.run();
    drag(&mut h, "Folder A", "Folder C", 0.5);
    let s = &h.state().state;
    assert_eq!(s.model().parent(&"a".into()), Some(Some(&"c".to_string())));
    assert!(h.state().signals.iter().any(|s| matches!(s,
        tree::Signal::Drop { keys, target, position: Position::Inside } if keys == &["a"] && target == "c")));
    // Into `e`: vetoed by a before-hook.
    drag(&mut h, "Folder B", "Folder E", 0.5);
    assert_eq!(h.state().state.model().parent(&"b".into()), Some(None));
    assert!(
        h.state().signals.iter().any(|s| matches!(s,
        tree::Signal::Vetoed { step, reason } if *step == TREE_DROP && reason == "e is read-only"))
    );
    let traces = h.state().handle().step_traces();
    assert!(
        traces
            .iter()
            .any(|t| t.step == TREE_DROP && t.vetoed_by.is_some())
    );
    // After a node.
    drag(&mut h, "Folder D", "Folder B", 0.9);
    let roots = h.state().state.model().loaded(None).unwrap().to_vec();
    assert_eq!(roots, ["b", "d", "c", "e"]);
    let under_c = h
        .state()
        .state
        .model()
        .loaded(Some(&"c".into()))
        .unwrap()
        .to_vec();
    assert_eq!(under_c, ["c/1", "c/2", "c/3", "a"]);
}

/// 100 000 nodes: only a screenful is built.
struct Big;

impl TreeSource<u32> for Big {
    fn load(&self, parent: Option<&u32>, sink: Sink<u32>) {
        match parent {
            None => sink.deliver(
                (0..100)
                    .map(|i| Node::branch(i, format!("Group {i}")))
                    .collect(),
            ),
            Some(&g) => sink.deliver(
                (0..999)
                    .map(|j| Node::leaf(1000 + g * 1000 + j, format!("Item {g}.{j}")))
                    .collect(),
            ),
        }
    }
}

#[test]
fn a_hundred_thousand_nodes_build_a_screenful() {
    let mut state = TreeState::<u32>::new();
    for g in 0..100 {
        state.set_expanded(&g, true);
    }
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 600.0))
        .build_ui_state(
            |ui, state: &mut TreeState<u32>| {
                TreeView::new("big").show(ui, state, &Big);
            },
            state,
        );
    h.run();
    assert_eq!(h.state().model().rows_now().len(), 100_000);
    let built = h.query_all_by_label_contains("Item ").count();
    assert!(built > 10 && built < 60, "{built} rows built");
    let start = std::time::Instant::now();
    for _ in 0..10 {
        h.step();
    }
    let per = start.elapsed() / 10;
    assert!(
        per < std::time::Duration::from_millis(100),
        "{per:?} a pass"
    );
}

#[test]
fn a_selection_of_nodes_not_loaded_waits_for_their_branch() {
    let (tx, rx) = mpsc::channel();
    let mut h = tree_harness(Some(tx));
    h.run();
    // `b/2` is under a branch never opened: queued.
    let s = &mut h.state_mut().state;
    s.set_expanded(&"b".into(), true);
    s.set_selection(["a".to_string(), "b/2".to_string()], Some("b/2".into()));
    assert_eq!(s.selection().keys(), &["a"], "what is known, at once");
    h.run();
    let sink = rx.try_recv().expect("b asked for");
    // Other edits come first: still queued.
    h.state()
        .state
        .handle()
        .rename("c".into(), "Folder C renamed");
    h.run();
    assert!(h.state().state.pending_selection().is_some());
    sink.deliver(children(Some(&"b".into())));
    h.run();
    let s = &h.state().state;
    assert_eq!(s.selection().keys(), &["a", "b/2"]);
    assert_eq!(s.selection().cursor().map(String::as_str), Some("b/2"));
    assert!(s.pending_selection().is_none());
    // The user's selection drops what is queued.
    h.state_mut()
        .state
        .set_selection(["d/1".to_string()], Some("d/1".into()));
    h.get_by_label("Folder E").click();
    h.run();
    let s = &h.state().state;
    assert!(s.pending_selection().is_none());
    assert_eq!(s.selection().keys(), &["e"]);
}

/// A tree whose rows are links: the selection marks the page shown, the
/// cursor moves on its own, a click or Enter opens.
fn links_harness(state: TreeState<String>, size: egui::Vec2) -> Harness<'static, TreeApp> {
    let app = TreeApp {
        app: None,
        state,
        source: Source {
            asked: Arc::default(),
            slow: None,
        },
        signals: Vec::new(),
    };
    Harness::builder().with_size(size).build_ui_state(
        |ui, t: &mut TreeApp| {
            let out = TreeView::new("links")
                .label("Links")
                .mode(Mode::None)
                .activate_on_click(true)
                .show(ui, &mut t.state, &t.source);
            t.signals.extend(out.signals);
        },
        app,
    )
}

fn activated(h: &mut Harness<'_, TreeApp>) -> Vec<String> {
    let signals = std::mem::take(&mut h.state_mut().signals);
    signals
        .into_iter()
        .filter_map(|s| match s {
            tree::Signal::Activate { key } => Some(key),
            _ => None,
        })
        .collect()
}

#[test]
fn a_click_activates_when_asked_and_the_selection_stays_the_applications() {
    let mut state = TreeState::new();
    state.set_expanded(&"a".to_string(), true);
    let mut h = links_harness(state, egui::vec2(400.0, 600.0));
    h.run();
    let shown = "a/1".to_string();
    h.state_mut()
        .state
        .set_selection([shown.clone()], Some(shown.clone()));
    // A click on a leaf opens it, and only moves the cursor.
    h.get_by_label("File a2").click();
    h.run();
    assert_eq!(activated(&mut h), ["a/2"]);
    let s = &h.state().state;
    assert_eq!(s.selection().keys(), std::slice::from_ref(&shown));
    assert_eq!(s.selection().cursor().map(String::as_str), Some("a/2"));
    // A click on a branch opens or closes it.
    h.get_by_label("Folder B").click();
    h.run();
    assert!(h.state().state.model().is_expanded(&"b".into()));
    h.get_by_label("Folder B").click();
    h.run();
    assert!(!h.state().state.model().is_expanded(&"b".into()));
    assert!(activated(&mut h).is_empty());
    // The keys move the cursor alone; Enter opens.
    h.get_by_label("File a3").click();
    h.run();
    assert_eq!(activated(&mut h), ["a/3"]);
    h.key_press(Key::ArrowUp);
    h.run();
    assert!(activated(&mut h).is_empty(), "the arrows open nothing");
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(activated(&mut h), ["a/2"]);
}

#[test]
fn a_node_revealed_comes_into_view_from_the_first_pass() {
    let mut state = TreeState::new();
    for k in ["a", "b", "c", "d", "e"] {
        state.set_expanded(&k.to_string(), true);
    }
    // Before the view has any height.
    state.reveal(&"e/3".to_string());
    let mut h = links_harness(state, egui::vec2(400.0, 120.0));
    h.run();
    assert!(h.query_by_label("File e3").is_some());
    assert!(h.query_by_label("Folder A").is_none(), "it scrolled");
    // And later, back up.
    h.state_mut().state.reveal(&"a".to_string());
    h.run();
    assert!(h.query_by_label("Folder A").is_some());
}

/// Files labelled with their size, named apart.
struct Decorated(Source);

impl TreeSource<String> for Decorated {
    fn load(&self, parent: Option<&String>, sink: Sink<String>) {
        let nodes = children(parent)
            .into_iter()
            .map(|n| {
                if n.branch {
                    n
                } else {
                    let name = n.label.clone();
                    Node::leaf(n.key, format!("{name} · 2 KB")).with_name(name)
                }
            })
            .collect();
        self.0.asked.lock().unwrap().push(parent.cloned());
        sink.deliver(nodes);
    }

    fn can_drop(&self, _: &[String], target: &String, position: Position) -> Result<(), String> {
        if target == "b" && position == Position::Inside {
            Err("b takes no more".into())
        } else {
            Ok(())
        }
    }
}

/// A tree whose rows carry an icon and their own widgets, and whose
/// files are decorated.
struct RichApp {
    state: TreeState<String>,
    source: Decorated,
    hidden: std::collections::HashSet<String>,
    deleted: Vec<String>,
    signals: Vec<tree::Signal<String>>,
}

fn rich_harness() -> Harness<'static, RichApp> {
    let app = RichApp {
        state: TreeState::new(),
        source: Decorated(Source {
            asked: Arc::default(),
            slow: None,
        }),
        hidden: Default::default(),
        deleted: Vec::new(),
        signals: Vec::new(),
    };
    Harness::builder()
        .with_size(egui::vec2(400.0, 600.0))
        .build_ui_state(
            |ui, t: &mut RichApp| {
                let hidden = &mut t.hidden;
                let deleted = &mut t.deleted;
                let out = TreeView::<String>::new("rich")
                    .label("Rich")
                    .rename(true)
                    .drag_drop(true)
                    .can_drop(|_, target, position| {
                        if target == "d" && position == Position::Inside {
                            Err("d is full".into())
                        } else {
                            Ok(())
                        }
                    })
                    .icon(|n| Some(if n.branch { Icon::Folder } else { Icon::File }.into()))
                    .row_widgets(|ui, n| {
                        let on = hidden.contains(&n.key);
                        let name = format!("Hide {}", n.label);
                        let r = ui.add(egui::Button::selectable(
                            on,
                            if on { "hidden" } else { "shown" },
                        ));
                        r.widget_info(|| {
                            egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, &name)
                        });
                        if r.clicked() && !hidden.remove(&n.key) {
                            hidden.insert(n.key.clone());
                        }
                    })
                    .context_ui(|ui, n| {
                        if ui.button(format!("Delete {}", n.label)).clicked() {
                            deleted.push(n.key.clone());
                            ui.close();
                        }
                    })
                    .show(ui, &mut t.state, &t.source);
                t.signals.extend(out.signals);
            },
            app,
        )
}

#[test]
fn row_widgets_act_on_their_row_without_selecting_it() {
    let mut h = rich_harness();
    h.run();
    h.get_by_label("Folder A").click();
    h.run();
    h.get_by_label("Hide Folder C").click();
    h.run();
    assert!(h.state().hidden.contains("c"));
    assert_eq!(h.state().state.selection().keys(), &["a"], "still Folder A");
    assert!(!h.state().signals.iter().any(
        |s| matches!(s, tree::Signal::Select { keys, .. } if keys.contains(&"c".to_string()))
    ));
    // The label is cut before the widgets: the toggle stays clickable
    // and the row's name stays the label.
    let label = h.get_by_label("Folder C").rect();
    let toggle = h.get_by_label("Hide Folder C").rect();
    assert!(toggle.right() <= label.right() + 0.5 && toggle.left() > label.left());
    h.get_by_label("Hide Folder C").click();
    h.run();
    assert!(h.state().hidden.is_empty());
}

#[test]
fn a_rename_starts_from_the_name_not_the_decorated_label() {
    let mut h = rich_harness();
    h.run();
    h.state_mut().state.set_expanded(&"a".into(), true);
    h.run();
    h.get_by_label("File a1 · 2 KB").click();
    h.run();
    h.key_press(Key::F2);
    h.run();
    h.key_press(Key::End);
    h.event(egui::Event::Text("0".into()));
    h.key_press(Key::Enter);
    h.run();
    assert!(h.state().signals.iter().any(|s| matches!(s,
        tree::Signal::Rename { key, from, to } if key == "a/1" && from == "File a1" && to == "File a10")));
    let node = h.state().state.model().node(&"a/1".into()).unwrap().clone();
    assert_eq!(node.name(), "File a10");
    assert_eq!(
        node.label, "File a1 · 2 KB",
        "until the application updates it"
    );
    // Type-ahead searches names.
    h.get_by_label("Folder A").click();
    h.run();
    h.event(egui::Event::Text("file a3".into()));
    h.run();
    assert_eq!(
        h.state().state.selection().cursor().map(String::as_str),
        Some("a/3")
    );
}

#[test]
fn a_context_menu_is_for_the_row_clicked_not_the_selection() {
    let mut h = rich_harness();
    h.run();
    h.get_by_label("Folder A").click();
    h.run();
    h.get_by_label("Folder C").click_secondary();
    h.run();
    assert!(h.state().signals.iter().any(|s| matches!(s,
        tree::Signal::Context { key } if key == "c")));
    h.get_by_label("Delete Folder C").click();
    h.run();
    assert_eq!(h.state().deleted, ["c"]);
    assert_eq!(h.state().state.selection().keys(), &["a"], "still Folder A");
    assert!(h.query_by_label("Delete Folder C").is_none(), "closed");
}

#[test]
fn the_context_menu_key_opens_the_cursor_rows_menu() {
    let mut h = rich_harness();
    h.run();
    h.get_by_label("Folder A").click();
    h.run();
    for _ in 0..2 {
        h.key_press(Key::ArrowDown);
        h.run();
    }
    assert_eq!(
        h.state().state.selection().cursor().map(String::as_str),
        Some("c")
    );
    h.event(egui::Event::Key {
        key: Key::F10,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    });
    h.run();
    h.run();
    assert!(h.state().signals.iter().any(|s| matches!(s,
        tree::Signal::Context { key } if key == "c")));
    let row = h.get_by_label("Folder C").rect();
    let item = h.get_by_label("Delete Folder C").rect();
    assert!(item.top() >= row.bottom() - 1.0, "under the row: {item:?}");
}

#[test]
fn drops_are_refused_by_the_call_site_and_by_the_source() {
    let mut h = rich_harness();
    h.run();
    let vetoed = |h: &Harness<'_, RichApp>, why: &str| {
        h.state().signals.iter().any(|s| {
            matches!(s,
            tree::Signal::Vetoed { step, reason } if *step == TREE_DROP && reason == why)
        })
    };
    // Beside `d` is fine, into it the call site refuses.
    drag(&mut h, "Folder A", "Folder D", 0.5);
    assert!(vetoed(&h, "d is full"));
    assert_eq!(h.state().state.model().parent(&"a".into()), Some(None));
    // Into `b` the source refuses.
    drag(&mut h, "Folder A", "Folder B", 0.5);
    assert!(vetoed(&h, "b takes no more"));
    assert_eq!(h.state().state.model().parent(&"a".into()), Some(None));
    // After `d`: both allow it.
    drag(&mut h, "Folder A", "Folder D", 0.9);
    let roots = h.state().state.model().loaded(None).unwrap().to_vec();
    assert_eq!(roots, ["b", "c", "d", "a", "e"]);
}

struct Rows(Vec<(u32, String, u32)>);

impl ListRows<u32> for Rows {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn key(&self, row: usize) -> u32 {
        self.0[row].0
    }
    fn label(&self, row: usize) -> String {
        self.0[row].1.clone()
    }
}

impl TableRows<u32> for Rows {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn key(&self, row: usize) -> u32 {
        self.0[row].0
    }
    fn label(&self, row: usize) -> String {
        self.0[row].1.clone()
    }
    fn cell(&mut self, ui: &mut egui::Ui, row: usize, column: usize, _: bool) {
        match column {
            0 => ui.label(&self.0[row].1),
            _ => ui.label(format!("{} KB", self.0[row].2)),
        };
    }
}

fn rows(n: u32) -> Rows {
    Rows(
        (0..n)
            .map(|i| (i, format!("Row {i:05}"), (i * 7919) % 1000))
            .collect(),
    )
}

#[test]
fn a_list_selects_with_the_pointer_and_the_keyboard() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 400.0))
        .build_ui_state(
            |ui, (state, rows): &mut (ListState<u32>, Rows)| {
                ListView::new("rows")
                    .mode(Mode::Multiple)
                    .show(ui, state, rows);
            },
            (ListState::new(), rows(50_000)),
        );
    h.run();
    assert!(h.query_by_label("Row 49999").is_none(), "virtualised");
    row(&h, "Row 00002").click();
    h.run();
    h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    h.run();
    assert_eq!(h.state().0.selection().keys(), &[2, 3]);
    h.key_press(Key::End);
    h.run();
    assert_eq!(h.state().0.selection().keys(), &[49_999]);
    assert!(
        h.query_by_label("Row 49999").is_some(),
        "scrolled to the end"
    );
    h.event(egui::Event::Text("row 001".into()));
    h.run();
    assert_eq!(h.state().0.selection().cursor(), Some(&100));
    // Rows that go leave the selection.
    h.state_mut().1.0.retain(|r| r.0 != 100);
    h.run();
    assert!(h.state().0.selection().is_empty());
}

/// Over `at`, a wheel's lines down. In one gesture, as a touchpad's, so that egui
/// does not smooth it over many passes.
fn wheel<S>(h: &mut Harness<'_, S>, at: egui::Pos2, lines: f32) {
    let event = |delta, phase| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, delta),
        modifiers: Modifiers::NONE,
        phase,
        source: egui::MouseWheelSource::Wheel,
    };
    h.hover_at(at);
    h.run();
    h.event(event(0.0, egui::TouchPhase::Start));
    h.event(event(-lines, egui::TouchPhase::Move));
    h.run();
    h.event(event(0.0, egui::TouchPhase::End));
    h.run();
}

/// In a panel that scrolls: the view above a tall page, and where the panel is.
struct InPanel {
    list: ListState<u32>,
    table: TableState<u32>,
    rows: Rows,
    use_table: bool,
    panel: f32,
}

fn in_panel(rows_in_view: u32, use_table: bool) -> Harness<'static, InPanel> {
    Harness::builder()
        .with_size(egui::vec2(400.0, 400.0))
        .build_ui_state(
            |ui, p: &mut InPanel| {
                let out = egui::ScrollArea::vertical()
                    .id_salt("panel")
                    .show(ui, |ui| {
                        if p.use_table {
                            let columns = COLUMNS();
                            DataTable::new("files", &columns).height(150.0).show(
                                ui,
                                &mut p.table,
                                &mut p.rows,
                            );
                        } else {
                            ListView::new("rows")
                                .height(150.0)
                                .show(ui, &mut p.list, &mut p.rows);
                        }
                        ui.allocate_space(egui::vec2(10.0, 2000.0));
                    });
                p.panel = out.state.clamped_offset().y;
            },
            InPanel {
                list: ListState::new(),
                table: TableState::new(),
                rows: rows(rows_in_view),
                use_table,
                panel: 0.0,
            },
        )
}

#[test]
fn a_list_keeps_the_wheel_at_its_end_in_a_panel_that_scrolls() {
    let mut h = in_panel(30, false);
    h.run();
    assert!(h.query_by_label("Row 00029").is_none());
    for _ in 0..12 {
        wheel(&mut h, egui::pos2(100.0, 75.0), 4.0);
    }
    assert!(h.query_by_label("Row 00029").is_some(), "at its end");
    assert_eq!(h.state().panel, 0.0);
}

#[test]
fn a_list_with_nothing_to_scroll_hands_the_wheel_on() {
    let mut h = in_panel(2, false);
    h.run();
    wheel(&mut h, egui::pos2(100.0, 75.0), 4.0);
    assert!(h.state().panel > 0.0);
}

#[test]
fn a_table_keeps_the_wheel_at_its_end_in_a_panel_that_scrolls() {
    let mut h = in_panel(30, true);
    h.run();
    for _ in 0..12 {
        wheel(&mut h, egui::pos2(100.0, 75.0), 4.0);
    }
    // The row and its cell both carry the name.
    assert!(
        h.query_all_by_label("Row 00029").next().is_some(),
        "at its end"
    );
    assert_eq!(h.state().panel, 0.0);
}

#[test]
fn a_table_with_nothing_to_scroll_hands_the_wheel_on() {
    let mut h = in_panel(2, true);
    h.run();
    wheel(&mut h, egui::pos2(100.0, 75.0), 4.0);
    assert!(h.state().panel > 0.0);
}

/// A table row by its name (its cells carry their own labels).
fn row<'h, S>(h: &'h Harness<'_, S>, label: &'h str) -> egui_kittest::Node<'h> {
    h.get_by_role_and_label(egui::accesskit::Role::Button, label)
}

struct TableApp {
    app: Option<App>,
    state: TableState<u32>,
    rows: Rows,
    sorted: usize,
}

const COLUMNS: fn() -> Vec<Column> = || {
    vec![
        Column::new("name", "Name").width(160.0),
        Column::new("size", "Size").width(80.0),
    ]
};

#[test]
fn a_table_sorts_through_the_store_and_its_step() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 400.0))
        .build_ui_state(
            |ui, t: &mut TableApp| {
                let app = t.app.get_or_insert_with(|| {
                    let app = App::builder()
                        .plugin(ViewsPlugin)
                        .plugin(Guard)
                        .build(ui.ctx())
                        .unwrap();
                    table::declare(
                        &app.handle().store(),
                        &OwnerId::FRAMEWORK,
                        "files",
                        &COLUMNS(),
                    )
                    .unwrap();
                    app
                });
                let h = app.handle().clone();
                let store = h.store();
                let columns = COLUMNS();
                let out = DataTable::new("files", &columns)
                    .app(&h)
                    .store(&store, "files")
                    .mode(Mode::Multiple)
                    .show(ui, &mut t.state, &mut t.rows);
                if out.sorted {
                    t.sorted += 1;
                    let desc = t.state.sort().is_some_and(|s| s.descending);
                    t.rows
                        .0
                        .sort_by(|a, b| if desc { b.1.cmp(&a.1) } else { a.1.cmp(&b.1) });
                }
            },
            TableApp {
                app: None,
                state: TableState::new(),
                rows: rows(10_000),
                sorted: 0,
            },
        );
    h.run();
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Button, "Row 09999")
            .is_none(),
        "virtualised"
    );
    row(&h, "Row 00003").click();
    h.run();
    assert_eq!(h.state().state.selection().keys(), &[3]);
    h.get_by_label("Name").click();
    h.run();
    h.get_by_label("Name, sorted ascending").click();
    h.run();
    assert_eq!(h.state().sorted, 2);
    let store = h.state().app.as_ref().unwrap().handle().store();
    assert_eq!(
        store.lock().unwrap().get("files/sort").unwrap().value,
        egui_pop_model::values::Value::Text("-name".into())
    );
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Button, "Row 09999")
            .is_some(),
        "descending: the last first"
    );
    assert_eq!(h.state().state.selection().keys(), &[3], "kept by key");
    // By size: vetoed.
    h.get_by_label("Size").click();
    h.run();
    assert_eq!(
        h.state().state.sort().map(|s| s.column.as_str()),
        Some("name")
    );
    // Widths written to the store reach the table.
    store
        .lock()
        .unwrap()
        .set(egui_pop_model::values::Set::new(
            "files/widths",
            egui_pop_model::values::Change::To(egui_pop_model::values::Value::Floats(vec![
                220.0, 90.0,
            ])),
            egui_pop_model::commands::Origin::User,
        ))
        .unwrap();
    h.run();
    assert_eq!(h.state().state.widths(), &[220.0, 90.0]);
    let name = h.get_by_label("Name, sorted descending").rect();
    assert!((name.width() - 220.0).abs() < 1.0, "{name:?}");
    // Keyboard: Down moves from the cursor.
    row(&h, "Row 09999").click();
    h.run();
    h.key_press(Key::ArrowDown);
    h.run();
    assert_eq!(h.state().state.selection().keys(), &[9998]);
}

#[test]
fn every_step_is_declared() {
    let app = App::builder()
        .plugin(ViewsPlugin)
        .build(&egui::Context::default())
        .unwrap();
    let declared: Vec<String> = app
        .handle()
        .steps()
        .into_iter()
        .map(|s| s.spec.id.to_string())
        .collect();
    for s in [
        "tree.load",
        "tree.expand",
        "tree.select",
        "tree.drop",
        "tree.rename",
        "tree.activate",
        "tree.context",
        "list.select",
        "table.select",
        "table.sort",
        "table.resize",
    ] {
        assert!(declared.iter().any(|d| d == s), "{s}");
    }
}

/// The tree of [`tree_harness`] with a filter field above it; `a` open,
/// `b` loaded then folded.
fn filter_harness() -> Harness<'static, TreeApp> {
    let app = TreeApp {
        app: None,
        state: TreeState::new(),
        source: Source {
            asked: Arc::default(),
            slow: None,
        },
        signals: Vec::new(),
    };
    let mut h = Harness::builder()
        .with_size(egui::vec2(400.0, 600.0))
        .build_ui_state(
            |ui, t: &mut TreeApp| {
                if t.app.is_none() {
                    t.app = Some(App::builder().plugin(ViewsPlugin).build(ui.ctx()).unwrap());
                }
                let h = t.handle();
                FilterField::new("files").show(ui, &mut t.state);
                let out = TreeView::new("files")
                    .label("Files")
                    .mode(Mode::Multiple)
                    .app(&h)
                    .show(ui, &mut t.state, &t.source);
                t.signals.extend(out.signals);
            },
            app,
        );
    h.run();
    h.state_mut().state.set_expanded(&"a".into(), true);
    h.state_mut().state.set_expanded(&"b".into(), true);
    h.run();
    h.state_mut().state.set_expanded(&"b".into(), false);
    h.run();
    h
}

/// The labels of the rows a tree reports to agents.
fn shown(h: &Harness<'_, TreeApp>) -> Vec<String> {
    let trees = tree::filter::drawn(&h.ctx);
    trees["files"]["shown"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["label"].as_str().unwrap_or("").to_string())
        .collect()
}

fn filter_field<'h>(h: &'h Harness<'_, TreeApp>) -> egui_kittest::Node<'h> {
    h.get_by_role_and_label(egui::accesskit::Role::SearchInput, "Filter")
}

#[test]
fn a_filter_shows_the_matches_with_their_ancestors_and_moves_the_cursor() {
    let mut h = filter_harness();
    filter_field(&h).click();
    h.run();
    filter_field(&h).type_text("file B2");
    h.run();
    h.run();
    // `b` is folded: it opens for its match, its fold kept.
    assert_eq!(shown(&h), ["Folder B", "File b2"]);
    let model = h.state().state.model();
    assert!(model.is_open(&"b".into()) && !model.is_expanded(&"b".into()));
    assert_eq!(
        h.state().state.selection().cursor().map(String::as_str),
        Some("b/2"),
        "the cursor goes to the first match"
    );
    assert!(model.is_match(&"b/2".into()) && !model.is_match(&"b".into()));
    // Accents and case aside, every word anywhere.
    h.state_mut().state.set_filter("FÏLE 1");
    h.run();
    assert_eq!(shown(&h), ["Folder A", "File a1", "Folder B", "File b1"]);
}

#[test]
fn the_filter_field_hands_the_keyboard_to_the_tree_and_back() {
    let mut h = filter_harness();
    filter_field(&h).click();
    h.run();
    filter_field(&h).type_text("b2");
    h.run();
    h.run();
    // Down: into the tree, its cursor on the match; Enter opens it.
    h.key_press(Key::ArrowDown);
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert!(
        h.state()
            .signals
            .iter()
            .any(|s| matches!(s, tree::Signal::Activate { key } if key == "b/2")),
        "{:?}",
        h.state().signals
    );
    // Escape clears the field and keeps it: what is typed next filters.
    filter_field(&h).click();
    h.run();
    h.key_press(Key::Escape);
    h.run();
    assert_eq!(h.state().state.filter(), "");
    filter_field(&h).type_text("a3");
    h.run();
    h.run();
    assert_eq!(h.state().state.filter(), "a3");
    assert_eq!(shown(&h), ["Folder A", "File a3"]);
    // Twice more: cleared, then the tree has the keyboard again.
    h.key_press(Key::Escape);
    h.run();
    h.key_press(Key::Escape);
    h.run();
    h.run();
    assert_eq!(h.state().state.filter(), "");
    h.key_press(Key::ArrowDown);
    h.run();
    assert_eq!(
        h.state().state.selection().cursor().map(String::as_str),
        Some("b"),
        "the tree moves from where the cursor was"
    );
}

#[test]
fn nothing_matching_shows_no_match_and_clearing_keeps_the_fold() {
    let mut h = filter_harness();
    let before = shown(&h);
    h.state_mut().state.set_filter("zzz");
    h.run();
    h.run();
    // Beside the field and as the tree's one row.
    assert_eq!(h.query_all_by_label("No match").count(), 2);
    let trees = tree::filter::drawn(&h.ctx);
    assert_eq!(trees["files"]["no_match"], true);
    assert_eq!(trees["files"]["matches"], 0);
    // Opened and closed while filtering: the fold kept is as it was.
    h.state_mut().state.set_filter("file b");
    h.run();
    h.get_by_label("Folder B").click();
    h.run();
    h.key_press(Key::ArrowLeft);
    h.run();
    h.state_mut().state.set_filter("");
    h.run();
    h.run();
    assert_eq!(shown(&h), before);
    let model = h.state().state.model();
    assert!(model.is_expanded(&"a".into()) && !model.is_expanded(&"b".into()));
    assert!(
        !h.state()
            .signals
            .iter()
            .any(|s| matches!(s, tree::Signal::Expand { .. })),
        "no tree.expand while filtering"
    );
}

#[test]
fn the_match_count_is_shown_announced_and_reported() {
    let mut h = filter_harness();
    h.state_mut().state.set_filter("file a");
    h.run();
    h.run();
    // Five folders and six files loaded (`a`'s and `b`'s).
    assert_eq!(h.state().state.filter_count(), Some((3, 11)));
    h.get_by_label("3 of 11");
    let said: Vec<String> = egui_pop_core::a11y::announcements(&h.ctx)
        .into_iter()
        .map(|a| a.text)
        .collect();
    assert!(
        said.iter().any(|t| t == "3 of 11 match the filter"),
        "{said:?}"
    );
    let bridge = h.state().handle().debug_snapshot();
    assert_eq!(bridge["trees"]["files"]["filter"], "file a");
    assert_eq!(bridge["trees"]["files"]["matches"], 3);
    assert_eq!(bridge["trees"]["files"]["field"], true);
    egui_pop_test::assert_all_named(&h);
}

/// A list of 500 rows, 200 points high, under the theme.
fn tall_list() -> Harness<'static, (ListState<u32>, Rows)> {
    let h = Harness::builder()
        .with_size(egui::vec2(400.0, 400.0))
        .build_ui_state(
            |ui, (state, rows): &mut (ListState<u32>, Rows)| {
                ListView::new("rows")
                    .label("Files")
                    .height(200.0)
                    .show(ui, state, rows);
                let _ = ui.button("After");
            },
            (ListState::new(), rows(500)),
        );
    egui_pop_core::theme::install(&h.ctx, &egui_pop_core::theme::ThemeSpec::new("pop-dark"));
    h
}

/// The list's area answers the scroll area's keys (`UiPop::scroll_area`) under
/// the pointer, with nothing focused, and leaves the selection alone.
#[test]
fn a_list_under_the_pointer_takes_the_page_keys() {
    let mut h = tall_list();
    h.run();
    h.hover_at(egui::pos2(100.0, 75.0));
    h.run();
    assert!(h.query_by_label("Row 00000").is_some());
    h.key_press(Key::PageDown);
    h.run();
    assert!(h.query_by_label("Row 00000").is_none(), "a page down");
    h.key_press(Key::End);
    h.run();
    assert!(h.query_by_label("Row 00499").is_some(), "at its end");
    h.key_press(Key::Home);
    h.run();
    assert!(h.query_by_label("Row 00000").is_some(), "back at its start");
    assert!(h.state().0.selection().is_empty());
}

/// The list is the Tab stop: its scroll area does not become another.
#[test]
fn a_list_is_one_tab_stop_and_its_area_none() {
    let mut h = tall_list();
    h.run();
    h.run();
    assert!(h.query_by_label("Scrollable content").is_none());
    h.key_press(Key::Tab);
    h.run();
    assert!(h.query_by_label("Files").unwrap().is_focused());
    h.key_press(Key::Tab);
    h.run();
    assert!(h.query_by_label("After").unwrap().is_focused());
}
