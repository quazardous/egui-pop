//! The window's own frame: the title bar in the title zone, its regions
//! published in physical pixels, its buttons running steps a plugin may
//! veto, the system menu, and the system's frame back from the setting.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::frame::{
    BLOCK_MENU, FramePlugin, MENU_SYSTEM, SETTING_BAR_IN_FULLSCREEN, STEP_CLOSE,
    STEP_EXIT_FULLSCREEN, STEP_MAXIMIZE, STEP_SYSTEM_MENU,
};
use egui_pop_api::steps::Before;
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::frame::{self, Hit};
use egui_pop_model::ids::{BlockId, PluginId};
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

/// Puts a logo at the start of the title bar, keeping the menu button.
struct Logo;

impl Plugin for Logo {
    fn id(&self) -> PluginId {
        PluginId::from_static("logo")
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![PluginId::from_static("egui_pop.frame")]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.block(
            zones::CAPTION_START,
            Block::replace(BLOCK_MENU, BlockId::from_static("logo"), |ui, app| {
                egui_pop_api::frame::menu_button(ui, app);
                ui.strong("Logo");
            }),
        );
        Ok(())
    }
}

fn harness(ppp: f32) -> Harness<'static, App> {
    harness_with(ppp, false)
}

fn harness_with(ppp: f32, logo: bool) -> Harness<'static, App> {
    let mut b = App::builder()
        .plugins(DefaultPlugins)
        .plugin(FramePlugin::own().name("Demo"))
        .plugin(Guard);
    if logo {
        b = b.plugin(Logo);
    }
    let app = b.build(&egui::Context::default()).unwrap();
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

#[test]
fn the_start_of_the_bar_is_a_zone_an_application_replaces() {
    let mut h = harness_with(1.0, true);
    assert!(h.query_by_label("Logo").is_some());
    assert!(
        h.query_by_label("Demo").is_none(),
        "the default block is replaced"
    );
    // The kept menu button still asks for the system menu, as a step.
    h.get_by_label("Window menu").click();
    h.run();
    let trace = h.state().handle().step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_SYSTEM_MENU);
    assert!(h.query_by_label("Close window").is_some());
    h.key_press(egui::Key::Escape);
    h.run();
    // The bar still follows the setting.
    let store = h.state().handle().store();
    egui_pop_core::lock(&store)
        .set(Set::new(
            "settings/window.frame",
            Change::To(Value::Choice("system".into())),
            egui_pop_model::commands::Origin::User,
        ))
        .unwrap();
    h.run();
    assert!(h.query_by_label("Logo").is_none());
    assert!(frame::regions(&h.ctx).is_none());
}

fn full_screen(h: &mut Harness<'static, App>, on: bool) {
    h.input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .fullscreen = Some(on);
    h.run();
}

/// A double click in one frame.
fn double_click_at(h: &mut Harness<'static, App>, pos: egui::Pos2) {
    let events = &mut h.input_mut().events;
    events.push(egui::Event::PointerMoved(pos));
    for _ in 0..2 {
        for pressed in [true, false] {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    h.run();
}

fn steps_run(h: &Harness<'static, App>, step: &egui_pop_model::ids::StepId) -> usize {
    h.state()
        .handle()
        .step_traces()
        .iter()
        .filter(|t| t.step == *step)
        .count()
}

#[test]
fn full_screen_keeps_the_bar_with_its_way_out_and_no_resize_edges() {
    let mut h = harness(1.0);
    // Out of full screen a double click on the caption maximizes.
    let caption = egui::pos2(300.0, 10.0);
    double_click_at(&mut h, caption);
    assert_eq!(steps_run(&h, &STEP_MAXIMIZE), 1);
    full_screen(&mut h, true);
    // The bar and its zones stay; minimize and maximize go, the way out
    // of full screen takes maximize's place.
    assert!(h.query_by_label("Demo").is_some(), "the caption-start zone");
    assert!(h.query_by_label("Close").is_some());
    assert!(h.query_by_label("Minimize").is_none());
    assert!(h.query_by_label("Maximize").is_none());
    let exit = h.get_by_label("Exit full screen").rect();
    let r = frame::regions(&h.ctx).expect("published");
    assert!(r.fullscreen);
    assert!(r.caption.is_some() && r.close.is_some(), "{r:?}");
    assert!(r.minimize.is_none() && r.maximize.is_none(), "{r:?}");
    assert_eq!(r.grip, 0, "no resize edges");
    assert_eq!(r.hit(0, 300), Hit::Client);
    assert_eq!(r.hit(300, 10), Hit::Caption);
    let c = exit.center();
    assert_eq!(
        r.hit(c.x as i32, c.y as i32),
        Hit::Client,
        "never Snap Layouts"
    );
    // The window plugin's strip is not needed: the bar holds the way out.
    h.hover_at(egui::pos2(400.0, 1.0));
    h.run();
    assert!(h.query_by_label("Exit full screen (F11)").is_none());
    // A double click on the bar asks nothing in full screen.
    double_click_at(&mut h, caption);
    assert_eq!(steps_run(&h, &STEP_MAXIMIZE), 1);
    // The way out is a step.
    h.get_by_label("Exit full screen").click();
    h.run();
    let trace = h.state().handle().step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_EXIT_FULLSCREEN);
    assert_eq!(trace.ran_by.as_deref(), Some("default"));
    assert_eq!(
        trace.result,
        Some(Ok(serde_json::json!({ "fullscreen": false })))
    );
    full_screen(&mut h, false);
    assert!(h.query_by_label("Minimize").is_some());
    assert!(h.query_by_label("Maximize").is_some());
    assert!(h.query_by_label("Exit full screen").is_none());
    let r = frame::regions(&h.ctx).expect("published");
    assert!(!r.fullscreen && r.grip > 0);
}

#[test]
fn the_setting_off_hides_the_bar_in_full_screen() {
    let mut h = harness(1.0);
    let store = h.state().handle().store();
    egui_pop_core::lock(&store)
        .set(Set::new(
            &format!("settings/{SETTING_BAR_IN_FULLSCREEN}"),
            Change::To(Value::Bool(false)),
            egui_pop_model::commands::Origin::User,
        ))
        .unwrap();
    h.run();
    assert!(h.query_by_label("Close").is_some());
    full_screen(&mut h, true);
    assert!(h.query_by_label("Close").is_none());
    assert!(h.query_by_label("Demo").is_none());
    let r = frame::regions(&h.ctx).expect("still published");
    assert!(r.caption.is_none() && r.close.is_none(), "{r:?}");
    assert_eq!(r.hit(400, 10), Hit::Client);
    assert_eq!(r.grip, 0);
    // The window plugin's strip at the top edge is the way out again.
    h.hover_at(egui::pos2(400.0, 1.0));
    h.run();
    assert!(h.query_by_label("Exit full screen (F11)").is_some());
    full_screen(&mut h, false);
    assert!(h.query_by_label("Close").is_some());
}
