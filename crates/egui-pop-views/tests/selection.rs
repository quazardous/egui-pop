//! A tree and a list bound to the application-wide selection: what one
//! selects or hovers, the other shows; Alt+Left and Alt+Right walk the
//! past selections from either, without taking the context stack's keys
//! elsewhere.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::contexts::ContextsPlugin;
use egui_pop_api::selection::{SELECTION, SelectionItem, SelectionPlugin, SelectionService};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{BlockId, PluginId};
use egui_pop_views::list::{ListRows, ListState, ListView};
use egui_pop_views::tree::{Node, Sink, TreeSource, TreeState, TreeView};
use egui_pop_views::{Mode, ViewsPlugin};

const NAMES: [&str; 4] = ["a", "b", "c", "d"];

/// The roots `a` to `d`, by name.
struct Letters;

impl TreeSource<String> for Letters {
    fn load(&self, parent: Option<&String>, sink: Sink<String>) {
        let nodes = match parent {
            None => NAMES
                .iter()
                .map(|n| Node::leaf(n.to_string(), format!("Tree {n}")))
                .collect(),
            Some(_) => Vec::new(),
        };
        sink.deliver(nodes);
    }
}

/// The same letters as a list keyed by number, their items by name.
struct Rows;

impl ListRows<u32> for Rows {
    fn len(&self) -> usize {
        NAMES.len()
    }

    fn key(&self, row: usize) -> u32 {
        row as u32 * 10
    }

    fn label(&self, row: usize) -> String {
        format!("List {}", NAMES[row])
    }

    fn item(&self, row: usize) -> SelectionItem {
        SelectionItem::from(NAMES[row])
    }
}

#[derive(Default)]
struct Views {
    tree: TreeState<String>,
    list: ListState<u32>,
}

type Shared = Rc<RefCell<Views>>;

struct Demo(Shared);

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let views = self.0.clone();
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("demo.views"), 0, move |ui, app| {
                let service = app.service(&SELECTION).expect("the selection plugin");
                let mut v = views.borrow_mut();
                let Views { tree, list } = &mut *v;
                ui.columns(2, |cols| {
                    TreeView::new("tree")
                        .label("Letters tree")
                        .mode(Mode::Multiple)
                        .app(app)
                        .shared(&service)
                        .height(200.0)
                        .show(&mut cols[0], tree, &Letters);
                    ListView::new("list")
                        .label("Letters list")
                        .mode(Mode::Multiple)
                        .app(app)
                        .shared(&service)
                        .height(200.0)
                        .show(&mut cols[1], list, &mut Rows);
                });
            }),
        );
        Ok(())
    }
}

fn harness() -> (Harness<'static, App>, Shared) {
    let views = Shared::default();
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(ContextsPlugin::default())
        .plugin(ViewsPlugin)
        .plugin(SelectionPlugin)
        .plugin(Demo(views.clone()))
        .build(&egui::Context::default())
        .unwrap();
    assert!(
        app.handle().diagnostics().is_empty(),
        "{:?}",
        app.handle().diagnostics()
    );
    let mut h = Harness::builder()
        .with_size(egui::vec2(700.0, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    (h, views)
}

fn service(h: &Harness<'_, App>) -> std::sync::Arc<SelectionService> {
    h.state().handle().service(&SELECTION).unwrap()
}

fn items(h: &Harness<'_, App>) -> Vec<String> {
    service(h).items().iter().map(ToString::to_string).collect()
}

fn context(h: &Harness<'_, App>, key: &str) -> String {
    h.state().handle().context().get(key).to_string()
}

#[test]
fn a_selection_and_a_hover_made_in_one_view_show_in_the_other() {
    let (mut h, views) = harness();
    assert_eq!(context(&h, "hasSelection"), "false");
    h.get_by_label("List b").click();
    h.run();
    assert_eq!(items(&h), ["b"]);
    h.run();
    assert_eq!(views.borrow().tree.selection().keys(), ["b".to_string()]);
    assert_eq!(context(&h, "hasSelection"), "true");
    assert_eq!(context(&h, "selectionCount"), "1");
    // Ctrl in the tree adds to what the list selected.
    h.get_by_label("Tree d").click_modifiers(Modifiers::COMMAND);
    h.run();
    h.run();
    assert_eq!(items(&h), ["b", "d"]);
    assert_eq!(views.borrow().list.selection().keys(), [10, 30]);
    assert_eq!(context(&h, "selectionCount"), "2");
    // A plain click replaces the whole.
    h.get_by_label("Tree a").click();
    h.run();
    h.run();
    assert_eq!(items(&h), ["a"]);
    assert_eq!(views.borrow().list.selection().keys(), [0]);
    // Hovering a list row hovers its item everywhere.
    h.get_by_label("List c").hover();
    h.run();
    assert_eq!(service(&h).hovered(), Some(SelectionItem::from("c")));
    h.get_by_label("Tree b").hover();
    h.run();
    assert_eq!(service(&h).hovered(), Some(SelectionItem::from("b")));
    // On the journal, for agents.
    let journal = h.state().handle().events_since(0);
    assert!(
        journal.iter().any(|e| e.name == "selection.changed"),
        "{journal:?}"
    );
}

#[test]
fn shift_selects_a_range_and_ctrl_toggles() {
    let (mut h, _) = harness();
    h.get_by_label("List a").click();
    h.run();
    h.get_by_label("List c").click_modifiers(Modifiers::SHIFT);
    h.run();
    assert_eq!(items(&h), ["a", "b", "c"]);
    h.get_by_label("List b").click_modifiers(Modifiers::COMMAND);
    h.run();
    assert_eq!(items(&h), ["a", "c"]);
    // In the tree, from its own anchor, in its order.
    h.get_by_label("Tree d").click();
    h.run();
    h.get_by_label("Tree b").click_modifiers(Modifiers::SHIFT);
    h.run();
    let mut got = items(&h);
    got.sort();
    assert_eq!(got, ["b", "c", "d"]);
}

#[test]
fn alt_arrows_walk_the_history_in_a_bound_view() {
    let (mut h, views) = harness();
    for row in ["List a", "List b", "List c"] {
        h.get_by_label(row).click();
        h.run();
    }
    assert_eq!(items(&h), ["c"]);
    h.run();
    assert_eq!(context(&h, "selectionCanBack"), "true");
    // The list has the focus: Alt+Left is the selection's back, not the
    // context stack's.
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowLeft);
    h.run();
    h.run();
    assert_eq!(items(&h), ["b"]);
    assert_eq!(views.borrow().list.selection().keys(), [10]);
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowLeft);
    h.run();
    assert_eq!(items(&h), ["a"]);
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowRight);
    h.run();
    h.run();
    assert_eq!(items(&h), ["b"]);
    assert_eq!(context(&h, "selectionCanForward"), "true");
    // From the tree too.
    h.get_by_label("Tree d").click();
    h.run();
    h.run();
    assert_eq!(context(&h, "selectionCanForward"), "false");
    h.key_press_modifiers(Modifiers::ALT, Key::ArrowLeft);
    h.run();
    h.run();
    assert_eq!(items(&h), ["b"]);
    assert_eq!(views.borrow().tree.selection().keys(), ["b".to_string()]);
}

#[test]
fn the_selection_keys_do_not_clash() {
    let (h, _) = harness();
    egui_pop_test::assert_keymap_clean(h.state());
}
