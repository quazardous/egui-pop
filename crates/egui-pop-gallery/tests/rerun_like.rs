//! The Rerun-like example (`docs/rerun-like.md`): its kits are clean and
//! readable, its keys clash with nothing, a selection made anywhere shows
//! everywhere, and its two snapshots (`rerun-like-dark`,
//! `rerun-like-light`).

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::api::App;
use egui_pop::api::workbench::{PANEL_INSPECTOR, PANEL_MAIN, PANEL_SIDE_BAR};
use egui_pop::model::commands::Origin;
use egui_pop::theme::ThemeSpec;
use egui_pop::tokens::contrast::{self, Level};
use egui_pop::tokens::{Themes, kit};
use egui_pop_gallery::rerun_like::data::{ENTITIES, View};
use egui_pop_gallery::rerun_like::{self, DARK, Item, LIGHT, Model, RerunLike};
use egui_pop_test::SharedGpu as _;

const SIZE: egui::Vec2 = egui::vec2(1280.0, 800.0);

fn viewer(spec: ThemeSpec) -> (Harness<'static, App>, Rc<RefCell<Model>>) {
    let plugin = RerunLike::default();
    let model = plugin.model();
    let app = rerun_like::builder(plugin)
        .theme(spec)
        .build(&egui::Context::default())
        .expect("the viewer builds");
    let h = Harness::builder()
        .with_size(SIZE)
        .with_max_steps(8)
        .shared_wgpu()
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    egui_pop::core::motion::freeze(&h.ctx, 0.0);
    (h, model)
}

#[test]
fn the_kits_are_clean_and_readable() {
    for k in rerun_like::kits() {
        let problems = kit::check(&k);
        assert!(problems.is_empty(), "{}: {problems:?}", k.name);
        let mut themes = Themes::default();
        themes.add_kit(&k);
        let tokens = themes.resolve(&ThemeSpec::new(k.name.clone()));
        let failures = contrast::check(&tokens, Level::Normal);
        assert!(failures.is_empty(), "{}: {failures:#?}", k.name);
        // Their charts' hues, fitted to their surfaces, stay apart.
        let apart = contrast::check_category(&tokens);
        assert!(apart.is_empty(), "{}: {apart:#?}", k.name);
    }
}

#[test]
fn the_viewer_builds_clean() {
    let (mut h, _) = viewer(ThemeSpec::pair(DARK, LIGHT));
    h.run();
    let app = h.state();
    assert!(
        app.handle().diagnostics().is_empty(),
        "{:#?}",
        app.handle().diagnostics()
    );
    egui_pop_test::assert_keymap_clean(app);
    // The viewer's panels are where Rerun has them; the shell's own are not shown.
    let layout = app.handle().layout();
    assert_eq!(layout.areas.len(), rerun_like::layout().areas.len());
    for (area, node) in &rerun_like::layout().areas {
        let now = layout.areas.iter().find(|(a, _)| a == area).map(|(_, n)| n);
        assert_eq!(now, Some(node), "{area:?}");
    }
    for shell in [PANEL_SIDE_BAR, PANEL_MAIN, PANEL_INSPECTOR] {
        assert!(!app.handle().panel_visible(&shell), "{shell}");
    }
}

#[test]
fn a_selection_in_the_tree_shows_everywhere() {
    let (mut h, model) = viewer(ThemeSpec::pair(DARK, LIGHT));
    h.run();
    // The user picks /camera/detections in the blueprint.
    let key = rerun_like::entity_key(View::Camera, 4);
    model
        .borrow_mut()
        .tree
        .set_selection([key.clone()], Some(key));
    h.run();
    assert_eq!(model.borrow().selected, Some(Item::Entity(4)));
    // The selection panel shows it.
    h.get_by_label("Archetype");
    assert!(h.query_all_by_label("Boxes2D").count() >= 1);
    // A click on a stream of the time panel selects it, and the tree follows.
    let path = ENTITIES[0].path;
    let rows: Vec<_> = h.query_all_by_label(path).collect();
    assert!(!rows.is_empty());
    model.borrow_mut().select(Some(Item::Entity(0)));
    h.run();
    let tree = model.borrow().tree.selection().keys().to_vec();
    assert_eq!(tree, vec![rerun_like::entity_key(View::TimeSeries, 0)]);
}

#[test]
fn playback_runs_from_commands() {
    let (mut h, model) = viewer(ThemeSpec::pair(DARK, LIGHT));
    h.run();
    let start = model.borrow().at();
    let app = h.state_mut();
    let _ = app.run(
        &rerun_like::CMD_STEP_FORWARD,
        serde_json::Value::Null,
        Origin::User,
    );
    assert_eq!(model.borrow().at(), start + 1);
    let _ = app.run(&rerun_like::CMD_PLAY, serde_json::Value::Null, Origin::User);
    assert!(model.borrow().playing);
    let _ = app.run(
        &rerun_like::CMD_RESET,
        serde_json::Value::Null,
        Origin::User,
    );
    assert!(!model.borrow().playing);
    assert_eq!(model.borrow().at(), 0);
}

#[test]
fn snapshots() {
    let mut results = egui_kittest::SnapshotResults::new();
    for (name, spec) in [
        ("rerun-like-dark", ThemeSpec::pair(DARK, LIGHT)),
        ("rerun-like-light", ThemeSpec::pair(LIGHT, DARK)),
    ] {
        let (mut h, model) = viewer(spec);
        h.run();
        // Something selected, as a user would have.
        model.borrow_mut().select(Some(Item::Entity(1)));
        h.run();
        egui_pop_test::assert_all_named(&h);
        results.add(h.try_snapshot(name));
    }
}
