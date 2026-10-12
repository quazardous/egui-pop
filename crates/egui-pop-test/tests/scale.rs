//! Scale: the interface zoom in steps from its keys (egui's own zoom
//! keys off), a step a plugin may veto, the scale-changed signal, and
//! the test kit's helpers at several scales.

use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::Before;
use egui_pop_api::zoom::{self, STEP_ZOOM};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::frame;
use egui_pop_model::ids::PluginId;
use egui_pop_test::{SCALES, app_at, at_scales, rects_of, set_native_scale};

struct Cap;

impl Plugin for Cap {
    fn id(&self) -> PluginId {
        PluginId::from_static("cap")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // Never above 150 %.
        ctx.before(STEP_ZOOM, |_, args| {
            if args["to"].as_f64().unwrap_or(1.0) > 1.5 {
                Before::Veto("150 % at most here".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn press(h: &mut egui_kittest::Harness<'_, App>, m: egui::Modifiers, k: egui::Key) {
    h.key_press_modifiers(m, k);
    h.run();
}

#[test]
fn the_interface_zoom_moves_in_steps_from_its_keys_only() {
    let mut h = app_at(
        App::builder().plugins(DefaultPlugins).plugin(Cap),
        egui::vec2(800.0, 600.0),
        1.0,
    );
    h.run();
    let ctrl_alt = egui::Modifiers::COMMAND | egui::Modifiers::ALT;
    press(&mut h, ctrl_alt, egui::Key::Equals);
    assert_eq!(zoom::current(h.state().handle()), 1.1);
    assert!((h.ctx.zoom_factor() - 1.1).abs() < 1e-4);
    // egui's own Ctrl+= is off: it is the application's.
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Equals);
    assert!((h.ctx.zoom_factor() - 1.1).abs() < 1e-4);
    press(&mut h, ctrl_alt, egui::Key::Minus);
    press(&mut h, ctrl_alt, egui::Key::Minus);
    assert_eq!(zoom::current(h.state().handle()), 0.9);
    press(&mut h, ctrl_alt, egui::Key::Num0);
    assert_eq!(zoom::current(h.state().handle()), 1.0);
    // The plugin's veto holds the zoom at 150 %.
    for _ in 0..5 {
        press(&mut h, ctrl_alt, egui::Key::Equals);
    }
    assert_eq!(zoom::current(h.state().handle()), 1.5);
    let vetoed = h
        .state()
        .handle()
        .step_traces()
        .into_iter()
        .filter(|t| t.step == STEP_ZOOM && t.vetoed_by.is_some())
        .count();
    assert!(vetoed > 0);
    // Each change of scale is on the journal.
    let changes = h
        .state()
        .handle()
        .events_since(0)
        .into_iter()
        .filter(|e| e.name == "scale.changed")
        .count();
    assert!(changes >= 4, "{changes}");
}

#[test]
fn a_window_moved_to_another_screen_changes_scale_but_not_its_layout() {
    let mut h = app_at(
        App::builder().plugins(DefaultPlugins),
        egui::vec2(800.0, 600.0),
        1.0,
    );
    h.run();
    let before = rects_of(&h, "File");
    set_native_scale(&mut h, 1.5);
    h.run();
    assert!((h.ctx.pixels_per_point() - 1.5).abs() < 1e-4);
    assert!(
        (h.ctx.zoom_factor() - 1.0).abs() < 1e-4,
        "the zoom is untouched"
    );
    let after = rects_of(&h, "File");
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        assert!(
            (a.min - b.min).length() < 0.05 && (a.size() - b.size()).length() < 0.05,
            "{a:?} {b:?}"
        );
    }
    let changed = h
        .state()
        .handle()
        .events_since(0)
        .into_iter()
        .find(|e| e.name == "scale.changed")
        .expect("signalled");
    assert_eq!(changed.data["to"], 1.5);
}

#[test]
fn the_frame_stays_crisp_and_its_edges_wide_enough_at_every_scale() {
    assert_eq!(SCALES, [1.0, 1.25, 1.5, 2.0]);
    at_scales(|s| {
        let mut h = app_at(
            App::builder()
                .plugins(DefaultPlugins)
                .plugin(egui_pop_api::frame::FramePlugin::own()),
            egui::vec2(800.0, 600.0),
            s,
        );
        h.run();
        let r = frame::regions(&h.ctx).expect("published");
        assert!(r.grip >= frame::MIN_GRIP_PX as i32);
        assert!(h.query_by_label("Close").is_some());
        // Strokes are whole pixels.
        let w = egui_pop_core::scale::stroke(&h.ctx, 1.0) * s;
        assert!((w - w.round()).abs() < 1e-4);
        let style = h.ctx.global_style();
        let px = style.visuals.widgets.inactive.bg_stroke.width * s;
        assert!((px - px.round()).abs() < 1e-4, "{px} at {s}");
    });
}
