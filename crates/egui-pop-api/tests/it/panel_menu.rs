//! The panel menu: one context menu for a panel wherever it is. A
//! floating panel's title (and each tab of a floating group) opens the
//! menu a docked tab opens: Detach into a window, Move to group (the
//! docked tab groups, by their titles), Dock back (where it floated
//! from), Close; each entry a workbench command.
//!
//! Every panel floats unless its spec opts out (Float greyed, saying
//! why); a docked tab group floats whole (Float group) and docks back as
//! it was; the user's setting can narrow floating to the palettes.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::workbench::Change;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::layout::Join;
use egui_pop_model::panels::{Area, PanelSpec};

const LOG: PanelId = PanelId::from_static("demo.log");
const OUT: PanelId = PanelId::from_static("demo.out");
const TREE: PanelId = PanelId::from_static("demo.tree");
const NOTES: PanelId = PanelId::from_static("demo.notes");
/// Declared without a word on floating: it floats, as every panel.
const PLAIN: PanelId = PanelId::from_static("demo.plain");
/// Opted out of floating.
const DOC: PanelId = PanelId::from_static("demo.doc");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(
            PanelSpec::new(OUT, "Output", Area::Bottom).floatable(true),
            |ui, _| {
                ui.label("output body");
            },
        )?
        .panel(
            PanelSpec::new(LOG, "Log", Area::Bottom).floatable(true),
            |ui, _| {
                ui.label("log body");
            },
        )?
        .panel(
            PanelSpec::new(TREE, "Tree", Area::EndSide).floatable(true),
            |ui, _| {
                ui.label("tree body");
            },
        )?
        .panel(
            PanelSpec::new(NOTES, "Notes", Area::EndSide).floatable(true),
            |ui, _| {
                ui.label("notes body");
            },
        )?
        .panel(PanelSpec::new(PLAIN, "Plain", Area::StartSide), |ui, _| {
            ui.label("plain body");
        })?
        .panel(
            PanelSpec::new(DOC, "Document", Area::Center).floatable(false),
            |ui, _| {
                ui.label("doc body");
            },
        )?;
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

/// The log floating, its menu opened by a right click on its title.
fn floating_log_menu() -> Harness<'static, App> {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert!(
        app.change_workbench(Change::Float(LOG, [200.0, 120.0]))
            .unwrap()
            .ok()
    );
    h.run();
    h.run();
    assert!(app.layout().floating_of(&LOG).is_some());
    h.get_by_label("Log").click_secondary();
    h.run();
    h
}

/// Clicks the entry `label` of the submenu opened from `submenu`.
fn pick_in_submenu(h: &mut Harness<'static, App>, submenu: &str, label: &str) {
    let at = h.get_by_label(submenu).rect();
    h.get_by_label(submenu).hover();
    h.run();
    let entry = h
        .get_all_by_label(label)
        .map(|n| n.rect())
        .find(|r| {
            (r.center().y - at.center().y).abs() < at.height() * 3.0
                && (r.left() >= at.right() - 8.0 || r.right() <= at.left() + 8.0)
        })
        .unwrap_or_else(|| panic!("the submenu {submenu} lists {label}"));
    h.input_mut()
        .events
        .push(egui::Event::PointerMoved(entry.center()));
    h.run();
    h.get_all_by_label(label)
        .find(|n| n.rect() == entry)
        .unwrap()
        .click();
    h.run();
}

/// The menu's Close entry: wider than the band's ✕ named the same.
fn close_entry(h: &Harness<'static, App>, label: &str) -> egui::Rect {
    h.get_all_by_label(label)
        .map(|n| n.rect())
        .max_by(|a, b| a.width().total_cmp(&b.width()))
        .unwrap_or_else(|| panic!("no {label}"))
}

#[test]
fn a_floating_panels_title_opens_the_panel_menu() {
    let h = floating_log_menu();
    // Dock back shows its key (`Ctrl+Shift+D`) beside it.
    for entry in ["Detach into a window", "Move to group", "Dock back"] {
        assert!(
            h.query_by_label_contains(entry).is_some(),
            "the floating panel's menu lists {entry}"
        );
    }
    // The band's ✕ is named Close Log too: the menu's entry is the other.
    assert_eq!(h.get_all_by_label("Close Log").count(), 2);
}

#[test]
fn detach_from_a_floating_panel_opens_it_in_a_window() {
    let mut h = floating_log_menu();
    h.get_by_label("Detach into a window").click();
    h.run();
    let layout = h.state().handle().layout();
    assert!(layout.floating_of(&LOG).is_none());
    assert_eq!(layout.window_of(&LOG).map(|w| w.panel.clone()), Some(LOG));
}

#[test]
fn move_to_group_docks_a_floating_panel_in_that_group() {
    let mut h = floating_log_menu();
    pick_in_submenu(&mut h, "Move to group", "Tree, Notes");
    let layout = h.state().handle().layout();
    assert!(layout.floating_of(&LOG).is_none());
    assert_eq!(layout.area_of(&LOG), Some(Area::EndSide));
    let (group, shown) = layout.group_of(&LOG).expect("docked as a tab");
    assert_eq!(group, vec![TREE, NOTES, LOG]);
    assert_eq!(group[shown], LOG, "shown in its new group");
}

#[test]
fn dock_back_returns_a_floating_panel_to_the_place_it_floated_from() {
    let mut h = harness();
    let app = h.state().handle().clone();
    // Moved out of its default area first: back means where it was, not
    // where it was declared.
    assert!(
        app.change_workbench(Change::Move(LOG, Area::EndSide))
            .unwrap()
            .ok()
    );
    h.run();
    let (before, _) = app.layout().group_of(&LOG).unwrap();
    assert!(
        app.change_workbench(Change::Float(LOG, [200.0, 120.0]))
            .unwrap()
            .ok()
    );
    h.run();
    h.run();
    h.get_by_label("Log").click_secondary();
    h.run();
    h.get_by_label_contains("Dock back").click();
    h.run();
    let layout = app.layout();
    assert!(layout.floating_of(&LOG).is_none());
    assert_eq!(layout.area_of(&LOG), Some(Area::EndSide));
    let (after, _) = layout.group_of(&LOG).unwrap();
    assert_eq!(after, before, "at its index among the same tabs");
}

#[test]
fn close_from_a_floating_panel_closes_it() {
    let mut h = floating_log_menu();
    let entry = close_entry(&h, "Close Log");
    h.get_all_by_label("Close Log")
        .find(|n| n.rect() == entry)
        .unwrap()
        .click();
    h.run();
    let layout = h.state().handle().layout();
    assert!(layout.floating_of(&LOG).is_none());
    assert!(layout.hidden.contains(&LOG));
}

#[test]
fn a_tab_of_a_floating_group_opens_the_same_menu() {
    let mut h = harness();
    let app = h.state().handle().clone();
    app.change_workbench(Change::Float(LOG, [200.0, 120.0]));
    app.change_workbench(Change::Float(OUT, [400.0, 120.0]));
    assert!(
        app.change_workbench(Change::Group(vec![OUT], LOG, Join::Tab))
            .unwrap()
            .ok()
    );
    h.run();
    h.run();
    // Its tab in the group's band (the shown panel is named Output too).
    let tab = h
        .get_all_by_label("Output")
        .map(|n| n.rect())
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap();
    h.get_all_by_label("Output")
        .find(|n| n.rect() == tab)
        .unwrap()
        .click_secondary();
    h.run();
    assert!(h.get_all_by_label("Close Output").count() >= 2);
    assert!(h.query_by_label_contains("Dock back").is_some());
}

#[test]
fn a_docked_tab_opens_the_same_menu() {
    let mut h = harness();
    h.get_by_role_and_label(egui::accesskit::Role::Tab, "Tree")
        .click_secondary();
    h.run();
    for entry in [
        "Detach into a window",
        "Move to group",
        "Float",
        "Close Tree",
    ] {
        assert!(
            h.query_by_label(entry).is_some(),
            "the docked tab's menu lists {entry}"
        );
    }
    // Docked, it moves to another group of tabs, as a floating one does.
    pick_in_submenu(&mut h, "Move to group", "Output, Log");
    let layout = h.state().handle().layout();
    assert_eq!(layout.area_of(&TREE), Some(Area::Bottom));
}

/// Opens a docked tab's menu by a right click on its tab.
fn tab_menu(h: &mut Harness<'static, App>, tab: &str) {
    h.get_by_role_and_label(egui::accesskit::Role::Tab, tab)
        .click_secondary();
    h.run();
}

/// Sets the setting `id` to `value`, as the user would.
fn set_setting(h: &mut Harness<'static, App>, id: &str, value: egui_pop_model::values::Value) {
    egui_pop_core::lock(&h.state().handle().store())
        .set(egui_pop_model::values::Set::new(
            &format!("settings/{id}"),
            egui_pop_model::values::Change::To(value),
            egui_pop_model::commands::Origin::User,
        ))
        .unwrap();
    h.run();
    h.run();
}

#[test]
fn a_panel_declared_without_a_word_floats_from_its_menu() {
    let mut h = harness();
    tab_menu(&mut h, "Plain");
    h.get_by_label("Float").click();
    h.run();
    h.run();
    let layout = h.state().handle().layout();
    assert!(layout.floating_of(&PLAIN).is_some(), "Plain floats");
    assert_eq!(layout.area_of(&PLAIN), None);
}

#[test]
fn a_panel_that_opts_out_shows_float_greyed_with_its_reason() {
    let mut h = harness();
    tab_menu(&mut h, "Document");
    let entry = h.get_by_label("Float (Document cannot float)");
    assert!(entry.accesskit_node().is_disabled(), "greyed, not hidden");
    assert!(h.query_by_label("Float").is_none());
    let app = h.state().handle().clone();
    let out = app.change_workbench(Change::Float(DOC, [100.0, 100.0]));
    assert!(!out.unwrap().ok(), "the step refuses it too");
    assert!(app.layout().floating_of(&DOC).is_none());
}

#[test]
fn float_group_floats_the_whole_tab_group_and_dock_back_restores_it() {
    let mut h = harness();
    let app = h.state().handle().clone();
    let (before, shown) = app.layout().group_of(&TREE).unwrap();
    assert_eq!(before, vec![TREE, NOTES]);
    tab_menu(&mut h, "Tree");
    h.get_by_label("Float group").click();
    h.run();
    h.run();
    let layout = app.layout();
    let g = layout.floating_of(&TREE).expect("the group floats");
    assert_eq!(g.panels().cloned().collect::<Vec<_>>(), before);
    assert_eq!(g.arrangement, egui_pop_model::layout::Arrangement::Tabs);
    assert_eq!(g.shown(), &before[shown], "its shown tab kept");
    assert_eq!(layout.area_of(&NOTES), None);
    assert!(app.change_workbench(Change::DockBack(NOTES)).unwrap().ok());
    h.run();
    let layout = app.layout();
    assert!(layout.floating_of(&TREE).is_none());
    assert_eq!(layout.area_of(&TREE), Some(Area::EndSide));
    assert_eq!(
        layout.group_of(&TREE),
        Some((before, shown)),
        "the group back as it was"
    );
}

#[test]
fn the_group_menu_floats_the_group() {
    let mut h = harness();
    // A right click on the empty part of the bottom tab bar, after Log.
    let tab = h
        .get_by_role_and_label(egui::accesskit::Role::Tab, "Log")
        .rect();
    let at = egui::pos2(tab.right() + 120.0, tab.center().y);
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.run();
    h.input_mut().events.push(button(true));
    h.input_mut().events.push(button(false));
    h.run();
    h.get_by_label("Float group").click();
    h.run();
    h.run();
    let layout = h.state().handle().layout();
    let g = layout.floating_of(&LOG).expect("the bottom group floats");
    assert!(g.holds(&OUT) && g.holds(&LOG));
}

#[test]
fn the_setting_narrows_floating_to_the_palettes() {
    use egui_pop_api::workbench::SETTING_FLOAT_PANELS;
    use egui_pop_model::values::Value;
    let mut h = harness();
    set_setting(
        &mut h,
        SETTING_FLOAT_PANELS.as_str(),
        Value::Choice("palettes".into()),
    );
    tab_menu(&mut h, "Plain");
    assert!(
        h.get_by_label("Float (only palettes float)")
            .accesskit_node()
            .is_disabled()
    );
    h.key_press(egui::Key::Escape);
    h.run();
    let app = h.state().handle().clone();
    assert!(
        !app.change_workbench(Change::Float(PLAIN, [100.0, 100.0]))
            .unwrap()
            .ok()
    );
    assert!(
        !app.change_workbench(Change::FloatGroup(TREE, [100.0, 100.0]))
            .unwrap()
            .ok()
    );
    // Back to any: it floats again.
    set_setting(
        &mut h,
        SETTING_FLOAT_PANELS.as_str(),
        Value::Choice("any".into()),
    );
    assert!(
        app.change_workbench(Change::Float(PLAIN, [100.0, 100.0]))
            .unwrap()
            .ok()
    );
}
