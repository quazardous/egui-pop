//! List items and property rows, driven as a user would.

use std::cell::RefCell;
use std::rc::Rc;

use egui::accesskit;
use egui::{Key, vec2};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_core::icon::Icon;
use egui_pop_core::theme::{Density, ThemeSpec};
use egui_pop_test::walk::{self, SHIFT_TAB, TAB};
use egui_pop_test::{assert_all_named, themed};
use egui_pop_views::item::{ItemAction, ListItem, row_height};
use egui_pop_views::property::PropertyRow;
use egui_pop_views::tree::{Node, Sink, TreeSource, TreeState, TreeView};

/// A click at a point.
fn click_at(h: &mut egui_kittest::Harness<'_>, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run();
}

/// What the rows reported.
#[derive(Debug, Default)]
struct Seen {
    actions: Vec<String>,
    toggled: usize,
    clicked: Vec<String>,
}

fn eye(name: &str) -> ItemAction {
    ItemAction::new("hide", Icon::Visible, format!("Hide {name}"))
}

fn remove(name: &str) -> ItemAction {
    ItemAction::new("remove", Icon::Close, format!("Remove {name}"))
}

/// Two items with an eye and a cross each, the second's eye pinned.
fn two_items(seen: &Rc<RefCell<Seen>>) -> egui_kittest::Harness<'static> {
    let seen = seen.clone();
    themed(&ThemeSpec::default(), vec2(300.0, 120.0), move |ui| {
        for (name, pinned) in [("Alpha", false), ("Beta", true)] {
            let out = ListItem::new(name)
                .icon(Icon::File)
                .trailing("3 items")
                .action(remove(name))
                .action(eye(name).pinned(pinned))
                .id_salt(name)
                .show(ui);
            let mut s = seen.borrow_mut();
            if let Some(a) = out.action() {
                s.actions.push(format!("{a} {name}"));
            }
            if out.response.clicked() {
                s.clicked.push(name.to_string());
            }
        }
    })
}

#[test]
fn an_item_shows_its_actions_on_hover_and_reports_a_click() {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let mut h = two_items(&seen);
    h.run();
    // At rest: only the pinned eye.
    assert!(h.query_by_label("Hide Alpha").is_none());
    assert!(h.query_by_label("Remove Alpha").is_none());
    assert!(h.query_by_label("Hide Beta").is_some());
    assert!(h.query_by_label("Remove Beta").is_none());
    let row = h.get_by_label("Alpha").rect();
    h.hover_at(row.center());
    h.run();
    assert!(h.query_by_label("Remove Alpha").is_some());
    h.get_by_label("Hide Alpha").click();
    h.run();
    assert_eq!(seen.borrow().actions, ["hide Alpha"]);
    // The click on the button is not the row's.
    assert!(seen.borrow().clicked.is_empty());
    h.get_by_label("Beta").click();
    h.run();
    assert_eq!(seen.borrow().clicked, ["Beta"]);
    assert_all_named(&h);
}

#[test]
fn tab_walks_from_an_item_through_its_buttons() {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let mut h = two_items(&seen);
    h.run();
    let w = walk::walk(&mut h, TAB, 10);
    assert_eq!(
        w.names(),
        [
            "Alpha",
            "Remove Alpha",
            "Hide Alpha",
            "Beta",
            "Remove Beta",
            "Hide Beta"
        ]
    );
    // Back from a button, through the ones before it, to its row; the
    // buttons of a row behind are hidden.
    walk::reach(&mut h, TAB, "Hide Alpha", 10).expect("Hide Alpha");
    let back = walk::press(&mut h, SHIFT_TAB).expect("a stop");
    assert_eq!(back.name, "Remove Alpha");
    let back = walk::press(&mut h, SHIFT_TAB).expect("a stop");
    assert_eq!(back.name, "Alpha");
    walk::reach(&mut h, TAB, "Hide Alpha", 10).expect("Hide Alpha");
    // Enter on a focused button runs its action.
    walk::press(&mut h, walk::Press::key(Key::Enter));
    assert_eq!(seen.borrow().actions, ["hide Alpha"]);
}

#[test]
fn screen_readers_find_an_item_s_actions_on_the_row() {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let mut h = two_items(&seen);
    h.run();
    let row = h.get_by_label("Alpha");
    let node = row.accesskit_node();
    let names: Vec<String> = node
        .data()
        .custom_actions()
        .iter()
        .map(|a| a.description.clone())
        .collect();
    assert_eq!(names, ["Remove Alpha", "Hide Alpha"]);
    let (target_node, target_tree) = node.locate();
    h.event(egui::Event::AccessKitActionRequest(
        accesskit::ActionRequest {
            action: accesskit::Action::CustomAction,
            target_tree,
            target_node,
            data: Some(accesskit::ActionData::CustomAction(0)),
        },
    ));
    h.run();
    assert_eq!(seen.borrow().actions, ["remove Alpha"]);
}

#[test]
fn a_collapsing_item_toggles_from_its_arrow_and_the_keys() {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let open = Rc::new(RefCell::new(false));
    let (s, o) = (seen.clone(), open.clone());
    let mut h = themed(&ThemeSpec::default(), vec2(300.0, 80.0), move |ui| {
        let is_open = *o.borrow();
        let out = ListItem::new("world")
            .collapsing(is_open)
            .id_salt("world")
            .show(ui);
        if out.toggled {
            s.borrow_mut().toggled += 1;
            *o.borrow_mut() = !is_open;
        }
        if out.response.clicked() {
            s.borrow_mut().clicked.push("world".into());
        }
    });
    h.run();
    let rect = h.get_by_label("world").rect();
    // The arrow's column is at the row's left end.
    click_at(&mut h, egui::pos2(rect.left() + 4.0, rect.center().y));
    assert!(*open.borrow());
    assert_eq!(seen.borrow().toggled, 1);
    assert!(
        h.get_by_label("world")
            .accesskit_node()
            .data()
            .is_expanded()
            == Some(true),
        "announced open"
    );
    // A click on the label is the row's, not the arrow's.
    h.get_by_label("world").click();
    h.run();
    assert_eq!(seen.borrow().toggled, 1);
    // Left closes the focused row, Right opens it.
    h.get_by_label("world").focus();
    h.run();
    h.key_press(Key::ArrowLeft);
    h.run();
    assert!(!*open.borrow());
    h.key_press(Key::ArrowRight);
    h.run();
    assert!(*open.borrow());
    assert_eq!(seen.borrow().toggled, 3);
}

#[test]
fn a_disabled_item_is_not_clicked() {
    let clicked = Rc::new(RefCell::new(0));
    let c = clicked.clone();
    let mut h = themed(&ThemeSpec::default(), vec2(300.0, 60.0), move |ui| {
        if ListItem::new("Archived")
            .disabled(true)
            .show(ui)
            .response
            .clicked()
        {
            *c.borrow_mut() += 1;
        }
    });
    h.run();
    h.get_by_label("Archived").click();
    h.run();
    assert_eq!(*clicked.borrow(), 0);
    assert!(h.get_by_label("Archived").accesskit_node().is_disabled());
}

#[test]
fn the_row_height_is_its_own_token() {
    let height = |spec: ThemeSpec| {
        let got = Rc::new(RefCell::new(0.0));
        let g = got.clone();
        let mut h = themed(&spec, vec2(200.0, 60.0), move |ui| {
            *g.borrow_mut() = row_height(ui);
        });
        h.run();
        *got.borrow()
    };
    // By default, the controls' height at each density.
    assert_eq!(height(ThemeSpec::default()), 24.0);
    assert_eq!(height(ThemeSpec::default().density(Density::Compact)), 20.0);
    // A theme may make rows denser than its controls.
    let dense = ThemeSpec::default().override_token(
        "size.row.height",
        serde_json::json!({"value": 20, "unit": "px"}),
    );
    assert_eq!(height(dense), 20.0);
}

#[test]
fn property_rows_line_their_values_up() {
    let radius = Rc::new(RefCell::new(1.0_f32));
    let r = radius.clone();
    let mut h = themed(&ThemeSpec::default(), vec2(400.0, 120.0), move |ui| {
        PropertyRow::new("Path").text(ui, "world/points");
        PropertyRow::new("A much longer name").text(ui, "Points3D");
        PropertyRow::new("Radius").widget(ui, egui::DragValue::new(&mut *r.borrow_mut()));
    });
    h.run();
    h.run();
    let path = h.get_by_label("world/points").rect();
    let kind = h.get_by_label("Points3D").rect();
    assert_eq!(path.left(), kind.left());
    assert!(path.left() > h.get_by_label("A much longer name").rect().right());
    // The value widget is named by its property.
    let named = h
        .query_all_by_label("Radius")
        .filter(|n| n.accesskit_node().role() == accesskit::Role::SpinButton)
        .count();
    assert_eq!(named, 1, "the value, named by its property");
    assert_all_named(&h);
}

#[test]
fn a_read_only_property_s_widgets_are_disabled() {
    let mut on = true;
    let mut h = themed(&ThemeSpec::default(), vec2(300.0, 60.0), move |ui| {
        PropertyRow::new("Visible")
            .read_only(true)
            .widget(ui, egui::Checkbox::without_text(&mut on));
    });
    h.run();
    let boxes: Vec<_> = h
        .query_all_by_label("Visible")
        .filter(|n| n.accesskit_node().role() == accesskit::Role::CheckBox)
        .collect();
    assert_eq!(boxes.len(), 1);
    assert!(boxes[0].accesskit_node().is_disabled());
}

/// A branch and three leaves.
struct Layers;

impl TreeSource<String> for Layers {
    fn load(&self, parent: Option<&String>, sink: Sink<String>) {
        sink.deliver(match parent {
            None => vec![Node::branch("root".into(), "Root")],
            Some(_) => ["sky", "ground", "sea"]
                .iter()
                .map(|k| Node::leaf(k.to_string(), k.to_string()))
                .collect(),
        });
    }
}

fn layers_tree(seen: &Rc<RefCell<Seen>>) -> egui_kittest::Harness<'static> {
    let seen = seen.clone();
    let mut state = TreeState::<String>::new();
    state.set_expanded(&"root".to_string(), true);
    themed(&ThemeSpec::default(), vec2(300.0, 300.0), move |ui| {
        let _ = ui.button("Before");
        let s = seen.clone();
        TreeView::<String>::new("layers")
            .label("Layers")
            .height(120.0)
            .guides(true)
            .trailing(|n| (!n.branch).then(|| "layer".to_string()))
            .actions(|n| {
                if n.branch {
                    Vec::new()
                } else {
                    vec![eye(&n.label)]
                }
            })
            .on_action(move |key, action| s.borrow_mut().actions.push(format!("{action} {key}")))
            .show(ui, &mut state, &Layers);
        let _ = ui.button("After");
    })
}

#[test]
fn a_tree_row_s_actions_show_on_hover_and_run_their_step() {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let mut h = layers_tree(&seen);
    h.run();
    h.run();
    assert!(h.query_by_label("Hide ground").is_none());
    let row = h.get_by_label("ground").rect();
    h.hover_at(row.center());
    h.run();
    h.get_by_label("Hide ground").click();
    h.run();
    assert_eq!(seen.borrow().actions, ["hide ground"]);
    assert_all_named(&h);
}

#[test]
fn tab_from_a_tree_reaches_the_cursor_s_actions() {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let mut h = layers_tree(&seen);
    h.run();
    h.run();
    walk::reach(&mut h, TAB, "Layers", 5).expect("the tree");
    // The cursor on `sky`.
    h.key_press(Key::ArrowDown);
    h.run();
    h.key_press(Key::ArrowDown);
    h.run();
    let stop = walk::press(&mut h, TAB).expect("a stop");
    assert_eq!(stop.name, "Hide sky");
    // Back from the buttons to the tree.
    let tree = walk::press(&mut h, SHIFT_TAB).expect("a stop");
    assert_eq!(tree.name, "Layers");
    // On from the last button, out of the tree; back from there, to the
    // tree (its buttons hide once neither has the focus).
    walk::press(&mut h, TAB).expect("a stop");
    let after = walk::press(&mut h, TAB).expect("a stop");
    assert_eq!(after.name, "After");
    let back = walk::press(&mut h, SHIFT_TAB).expect("a stop");
    assert_eq!(back.name, "Layers");
    // Enter on the cursor's button runs its action.
    walk::press(&mut h, TAB);
    walk::press(&mut h, walk::Press::key(Key::Enter));
    assert_eq!(seen.borrow().actions, ["hide sky"]);
}
