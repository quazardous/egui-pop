//! The window's place followed into the store (no undo step), and the
//! way out of full screen.

use egui::{Pos2, Rect, Vec2, ViewportId, ViewportInfo};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::window::{self, CMD_FULLSCREEN, WindowState};
use egui_pop_api::{App, DefaultPlugins};
use egui_pop_core::lock;

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .build(&egui::Context::default())
        .unwrap();
    Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app)
}

fn viewport(h: &mut Harness<'_, App>, f: impl FnOnce(&mut ViewportInfo)) {
    let info = h.input_mut().viewports.entry(ViewportId::ROOT).or_default();
    f(info);
}

#[test]
fn the_place_is_kept_in_the_store_without_undo_steps() {
    let mut h = harness();
    viewport(&mut h, |v| {
        v.inner_rect = Some(Rect::from_min_size(
            Pos2::new(110.0, 140.0),
            Vec2::new(900.0, 600.0),
        ));
        v.outer_rect = Some(Rect::from_min_size(
            Pos2::new(100.0, 100.0),
            Vec2::new(920.0, 650.0),
        ));
        v.maximized = Some(false);
        v.fullscreen = Some(false);
    });
    h.run();
    let store = h.state().handle().store();
    let doc = lock(&store).export("settings/");
    let state = window::from_settings(&doc).unwrap();
    assert_eq!(
        state,
        WindowState {
            position: Some(Pos2::new(100.0, 100.0)),
            size: Vec2::new(900.0, 600.0),
            maximized: false,
            fullscreen: false,
        }
    );
    // Maximized: the normal place stays as it was.
    viewport(&mut h, |v| {
        v.maximized = Some(true);
        v.inner_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1920.0, 1080.0)));
    });
    h.run();
    let doc = lock(&store).export("settings/");
    let state = window::from_settings(&doc).unwrap();
    assert!(state.maximized);
    assert_eq!(state.size, Vec2::new(900.0, 600.0));
    assert!(lock(&store).undo_steps().is_empty());
}

#[test]
fn full_screen_shows_its_way_out_at_the_top_edge() {
    let mut h = harness();
    viewport(&mut h, |v| v.fullscreen = Some(true));
    h.run();
    assert_eq!(h.state().handle().context().get("windowFullscreen"), "true");
    assert!(h.query_by_label("Exit full screen (F11)").is_none());
    h.hover_at(Pos2::new(450.0, 1.0));
    h.run();
    h.get_by_label("Exit full screen (F11)").click();
    h.run();
    h.run();
    let handle = h.state().handle().clone();
    assert_eq!(handle.history().last().unwrap().id, CMD_FULLSCREEN);
    assert_eq!(
        handle.history().last().unwrap().result,
        Ok(serde_json::json!({ "fullscreen": false }))
    );
}
