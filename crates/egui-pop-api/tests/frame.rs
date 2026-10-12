//! The window's own frame: the title bar in the title zone, its regions
//! published in physical pixels, its buttons running steps a plugin may
//! veto, the system menu, and the system's frame back from the setting.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::frame::{FramePlugin, MENU_SYSTEM, STEP_CLOSE, STEP_MAXIMIZE};
use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::frame::{self, Hit};
use egui_pop_model::ids::PluginId;
use egui_pop_model::values::{Change, Set, Value};

struct Guard;

impl Plugin for Guard {
    fn id(&self) -> PluginId {
        PluginId::from_static("guard")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_CLOSE, |app, _| {
            if app.context().get("unsaved") == "true" {
                Before::Veto("unsaved work".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn harness(ppp: f32) -> Harness<'static, App> {
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(FramePlugin::own().name("Demo"))
        .plugin(Guard)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .with_pixels_per_point(ppp)
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

#[test]
fn the_title_bar_publishes_its_regions_in_physical_pixels() {
    for ppp in [1.0, 1.25, 1.5, 2.0] {
        let h = harness(ppp);
        assert!(h.query_by_label("Minimize").is_some());
        assert!(h.query_by_label("Maximize").is_some());
        assert!(h.query_by_label("Close").is_some());
        assert!(h.query_by_label("Demo").is_some());
        let r = frame::regions(&h.ctx).expect("published");
        assert_eq!(
            r.size,
            [(800.0 * ppp).round() as i32, (600.0 * ppp).round() as i32]
        );
        let close = r.close.unwrap();
        let caption = r.caption.unwrap();
        assert_eq!(close.x + close.w, caption.x + caption.w, "{ppp}");
        assert!(r.grip >= frame::MIN_GRIP_PX as i32);
        let mid = |c: frame::RectPx| (c.x + c.w / 2, c.y + c.h / 2);
        let (x, y) = mid(close);
        assert_eq!(r.hit(x, y), Hit::Close);
        let (x, y) = mid(r.maximize.unwrap());
        assert_eq!(r.hit(x, y), Hit::Maximize);
        let (cx, cy) = (caption.x + caption.w / 2, caption.y + caption.h / 2);
        assert_eq!(r.hit(cx, cy), Hit::Caption, "{ppp}");
        // The menu button is a hole in the caption.
        let hole = r.holes.first().copied().expect("the menu button");
        let (x, y) = mid(hole);
        assert_eq!(r.hit(x, y), Hit::Client);
        // The debug bridge answers the same.
        let fb =
            h.state()
                .handle()
                .debug_call("frame", "hit", &serde_json::json!({ "x": cx, "y": cy }));
        assert_eq!(fb.data, serde_json::json!("caption"));
    }
}

#[test]
fn the_buttons_run_steps_a_plugin_may_veto() {
    let mut h = harness(1.0);
    let mark = h.state().handle().last_event();
    h.state().handle().set_context("unsaved", "true");
    h.get_by_label("Close").click();
    h.run();
    let names: Vec<String> = h
        .state()
        .handle()
        .events_since(mark)
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(
        names.contains(&"frame.close.vetoed".to_string()),
        "{names:?}"
    );
    h.state().handle().set_context("unsaved", "false");
    h.get_by_label("Close").click();
    h.run();
    let trace = h.state().handle().step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_CLOSE);
    assert_eq!(trace.ran_by.as_deref(), Some("default"));
    h.get_by_label("Maximize").click();
    h.run();
    let trace = h.state().handle().step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_MAXIMIZE);
    assert_eq!(trace.args["maximized"], true);
}

#[test]
fn alt_space_opens_the_system_menu_and_the_setting_gives_the_system_frame_back() {
    let mut h = harness(1.0);
    assert!(!h.state().handle().menu(&MENU_SYSTEM).is_empty());
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::Space);
    h.run();
    // The bar's button and the menu's item.
    assert_eq!(h.query_all_by_label("Minimize").count(), 2);
    assert!(
        h.query_by_label("Close window").is_some(),
        "the system menu is open"
    );
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.query_by_label("Close window").is_none());
    let store = h.state().handle().store();
    egui_pop_core::lock(&store)
        .set(Set::new(
            "settings/window.frame",
            Change::To(Value::Choice("system".into())),
            egui_pop_model::commands::Origin::User,
        ))
        .unwrap();
    h.run();
    assert!(h.query_by_label("Minimize").is_none());
    assert!(frame::regions(&h.ctx).is_none());
}
