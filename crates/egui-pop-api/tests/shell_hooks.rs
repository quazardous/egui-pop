//! The shell's hooks for applications: the title zone above the menu
//! bar, an edge-to-edge main area, a wrapper around the whole drawing,
//! `on_exit` and the `app.exit` step, the window's place restored.

use std::cell::Cell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::app::STEP_EXIT;
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, AppBuilder, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{BlockId, PluginId};
use serde_json::json;

struct Main;

impl Plugin for Main {
    fn id(&self) -> PluginId {
        PluginId::from_static("main")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("main.body"), 0, |ui, _| {
                ui.label("main body");
            }),
        )
        .block(
            zones::TITLE,
            Block::add(BlockId::from_static("main.title"), 0, |ui, _| {
                ui.label("my own title bar");
            }),
        );
        Ok(())
    }
}

fn harness(b: AppBuilder) -> Harness<'static, App> {
    let app = b.plugin(Main).build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(600.0, 400.0))
        .build_ui_state(|ui, app: &mut App| app.show_wrapped(ui), app);
    h.run();
    h
}

#[test]
fn the_title_zone_is_above_the_menu_bar() {
    let h = harness(App::builder());
    let title = h.get_by_label("my own title bar").rect();
    let file = h.get_by_label("File").rect();
    assert!(title.bottom() <= file.top() + 0.5, "{title:?} {file:?}");
}

#[test]
fn the_main_area_may_be_drawn_edge_to_edge() {
    let framed = harness(App::builder()).get_by_label("main body").rect();
    let edge = harness(App::builder().main_edge_to_edge(true))
        .get_by_label("main body")
        .rect();
    assert!(edge.left() < framed.left(), "{edge:?} {framed:?}");
}

#[test]
fn a_wrapper_draws_around_the_application() {
    let h = harness(App::builder().wrap(|ui, _app, inner| {
        egui::Panel::bottom("wrapper").show(ui, |ui| {
            ui.label("drawn around");
        });
        inner(ui);
    }));
    assert!(h.query_by_label("drawn around").is_some());
    assert!(h.query_by_label("main body").is_some());
}

#[test]
fn exiting_runs_on_exit_then_the_exit_step() {
    let ran = Rc::new(Cell::new(false));
    let r = ran.clone();
    let mut app = App::builder()
        .on_exit(move |_| r.set(true))
        .plugin(Main)
        .build(&egui::Context::default())
        .unwrap();
    let out = app.exit();
    assert!(ran.get());
    assert_eq!(out.result(), Some(&Ok(json!({ "saved": true }))));
    let t = app.handle().step_traces().pop().unwrap();
    assert_eq!(t.step, STEP_EXIT);
}

#[test]
fn the_windows_place_is_restored_from_the_settings_directory() {
    let dir = std::env::temp_dir().join(format!("egui-pop-restore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let app = App::builder()
        .plugin(egui_pop_api::window::WindowPlugin)
        .persist(&dir)
        .build(&egui::Context::default())
        .unwrap();
    egui_pop_core::lock(&app.handle().store())
        .set(
            egui_pop_model::values::Set::new(
                egui_pop_api::window::RECT,
                egui_pop_model::values::Change::To(egui_pop_model::values::Value::Floats(vec![
                    10.0, 20.0, 900.0, 700.0,
                ])),
                egui_pop_model::commands::Origin::User,
            )
            .force(),
        )
        .unwrap();
    app.save().unwrap();
    let mut options = egui_pop_api::eframe::NativeOptions::default();
    let state = egui_pop_api::window::restore(&dir, &mut options).expect("kept");
    assert_eq!(state.size, egui::vec2(900.0, 700.0));
    assert_eq!(options.viewport.inner_size, Some(egui::vec2(900.0, 700.0)));
    assert!(egui_pop_api::window::restore(dir.join("none"), &mut options).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
