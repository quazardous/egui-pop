//! Zen mode: the centre alone, through a step, Escape bringing the
//! whole workbench back, the saved layout left as it was; the way out
//! shown on entering and at the top edge; the command palette in it.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::frame::FramePlugin;
use egui_pop_api::workbench::{CMD_ZEN, STEP_ZEN};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_core::theme::ThemeSpec;
use egui_pop_model::ids::{BlockId, PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};

const LOG: PanelId = PanelId::from_static("demo.log");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(PanelSpec::new(LOG, "Log", Area::Bottom), |ui, _| {
            ui.label("log body");
        })?;
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("demo.main"), 0, |ui, _| {
                ui.label("main body");
            }),
        )
        .block(
            zones::SIDE_BAR,
            Block::add(BlockId::from_static("demo.side"), 0, |ui, _| {
                ui.label("side body");
            }),
        )
        .block(
            zones::STATUS_BAR,
            Block::add(BlockId::from_static("demo.status"), 0, |ui, _| {
                ui.label("status body");
            }),
        );
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    harness_with(ThemeSpec::new("pop-dark"))
}

fn harness_with(theme: ThemeSpec) -> Harness<'static, App> {
    let app = App::builder()
        .theme(theme)
        .plugins(DefaultPlugins)
        .plugin(FramePlugin::own().name("Demo"))
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

/// What zen mode hides, and the centre it keeps.
const AROUND: [&str; 5] = ["Demo", "Minimize", "side body", "log body", "status body"];

fn shown(h: &Harness<'static, App>, label: &str) -> bool {
    h.query_by_label(label).is_some()
}

#[test]
fn zen_mode_hides_the_bars_and_areas_and_escape_brings_them_back() {
    let mut h = harness();
    for label in AROUND {
        assert!(shown(&h, label), "{label} before zen mode");
    }
    assert!(shown(&h, "File"), "the menu bar");
    assert!(shown(&h, "main body"));
    let handle = h.state().handle().clone();
    // Shift+F11 by default.
    let entry = handle
        .palette()
        .into_iter()
        .find(|e| e.command == CMD_ZEN)
        .expect("zen mode in the palette");
    assert_eq!(entry.keys, ["Shift+F11"]);
    let layout = serde_json::to_value(handle.layout()).unwrap();
    let mark = handle.last_event();

    handle.run(CMD_ZEN, serde_json::json!(null));
    h.run();
    assert!(handle.zen());
    for label in AROUND {
        assert!(!shown(&h, label), "{label} in zen mode");
    }
    assert!(!shown(&h, "File"), "the menu bar in zen mode");
    assert!(shown(&h, "main body"), "the centre stays");
    // No caption published while the bar is hidden.
    let r = egui_pop_core::frame::regions(&h.ctx).expect("published");
    assert!(r.caption.is_none() && r.close.is_none(), "{r:?}");
    // A step, on the journal; the saved layout is not touched.
    let trace = handle.step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_ZEN);
    assert_eq!(trace.args["zen"], true);
    assert!(
        handle
            .events_since(mark)
            .iter()
            .any(|e| e.name == "workbench.zen" && e.data["args"]["zen"] == true),
        "the journal says so"
    );
    assert_eq!(serde_json::to_value(handle.layout()).unwrap(), layout);

    // Escape leaves it.
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!handle.zen());
    for label in AROUND {
        assert!(shown(&h, label), "{label} after zen mode");
    }
    assert!(shown(&h, "File"));
    let trace = handle.step_traces().pop().unwrap();
    assert_eq!(trace.step, STEP_ZEN);
    assert_eq!(trace.args["zen"], false);
    assert!(
        egui_pop_core::frame::regions(&h.ctx)
            .and_then(|r| r.caption)
            .is_some()
    );
    // Escape does nothing more out of zen mode.
    let traces = handle.step_traces().len();
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(handle.step_traces().len(), traces);
}

#[test]
fn a_plugin_may_veto_zen_mode() {
    struct Keep;
    impl Plugin for Keep {
        fn id(&self) -> PluginId {
            PluginId::from_static("keep")
        }
        fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
            ctx.before(STEP_ZEN, |_, _| {
                egui_pop_api::steps::Before::Veto("the inspector must stay".into())
            });
            Ok(())
        }
    }
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Demo)
        .plugin(Keep)
        .build(&egui::Context::default())
        .unwrap();
    let out = app.handle().set_zen(true).expect("ran at once");
    assert!(out.vetoed().is_some());
    assert!(!app.handle().zen());
}

const HINT: &str = "Zen mode: Escape to leave";
const LEAVE: &str = "Leave zen mode";

/// Zen mode on, with the motion clock frozen at 0 before it starts.
fn in_zen(theme: ThemeSpec) -> (Harness<'static, App>, egui_pop_model::clock::ManualClock) {
    let mut h = harness_with(theme);
    let clock = egui_pop_core::motion::freeze(&h.ctx, 0.0);
    h.run();
    h.state().handle().run(CMD_ZEN, serde_json::json!(null));
    h.run();
    assert!(h.state().handle().zen());
    (h, clock)
}

#[test]
fn entering_zen_mode_says_how_to_leave_then_the_hint_fades() {
    let (mut h, clock) = in_zen(ThemeSpec::new("pop-dark"));
    // At the top centre: the key that leaves it, and a button.
    let hint = h.get_by_label(HINT);
    let bounds = hint.rect();
    assert!(bounds.top() < 40.0, "{bounds:?}");
    assert!(h.query_by_label(LEAVE).is_some());
    // Screen readers hear it once.
    let announced = |h: &Harness<'_, App>| {
        egui_pop_core::a11y::announcements(&h.ctx)
            .iter()
            .filter(|a| a.text == HINT)
            .count()
    };
    assert_eq!(announced(&h), 1);
    clock.set(2.9);
    h.run();
    assert!(h.query_by_label(HINT).is_some(), "three seconds");
    // Then it fades over `motion.duration`, and goes.
    let fade = f64::from(egui_pop_core::theme::Tokens::of(&h.ctx).motion_duration);
    assert!(fade > 0.0);
    clock.set(3.0 + fade / 2.0);
    h.run();
    assert!(h.query_by_label(HINT).is_some(), "fading");
    clock.set(3.0 + fade + 0.01);
    h.run();
    assert!(h.query_by_label(HINT).is_none(), "gone");
    assert!(h.query_by_label(LEAVE).is_none());
    assert!(h.state().handle().zen());
    assert_eq!(announced(&h), 1);
}

#[test]
fn under_reduced_motion_the_hint_goes_without_fading() {
    let (mut h, clock) = in_zen(ThemeSpec::new("pop-dark").reduced_motion(true));
    assert!(h.query_by_label(HINT).is_some());
    clock.set(3.0);
    h.run();
    assert!(h.query_by_label(HINT).is_none());
}

#[test]
fn the_top_edge_shows_the_way_out_and_its_button_leaves_zen_mode() {
    let (mut h, clock) = in_zen(ThemeSpec::new("pop-dark"));
    clock.set(10.0);
    h.run();
    assert!(h.query_by_label(LEAVE).is_none());
    // At the top edge: the strip comes back.
    h.hover_at(egui::pos2(450.0, 1.0));
    h.run();
    assert!(h.query_by_label(HINT).is_some());
    assert!(h.query_by_label(LEAVE).is_some());
    // Away from it: it goes.
    h.hover_at(egui::pos2(450.0, 300.0));
    h.run();
    assert!(h.query_by_label(LEAVE).is_none());
    // Back, and its button leaves zen mode.
    h.hover_at(egui::pos2(450.0, 1.0));
    h.run();
    h.get_by_label(LEAVE).click();
    h.run();
    assert!(!h.state().handle().zen());
    assert!(shown(&h, "side body"));
    assert!(h.query_by_label(LEAVE).is_none());
}

#[test]
fn the_command_palette_works_in_zen_mode() {
    use egui::{Key, Modifiers};
    let (mut h, _clock) = in_zen(ThemeSpec::new("pop-dark"));
    let handle = h.state().handle().clone();
    // Escape closes the palette first, then leaves zen mode.
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::P);
    h.run();
    assert!(egui_pop_api::palette::is_open(&handle));
    h.key_press(Key::Escape);
    h.run();
    assert!(!egui_pop_api::palette::is_open(&handle));
    assert!(handle.zen(), "the palette took that Escape");
    // "Leave zen mode" is offered, and runs.
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::P);
    h.run();
    h.get_by_role(egui::accesskit::Role::TextInput)
        .type_text("leave zen");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert!(!egui_pop_api::palette::is_open(&handle));
    assert!(!handle.zen());
    assert!(shown(&h, "side body"));
}

#[test]
fn shift_f11_toggles_zen_mode() {
    let mut h = harness();
    let handle = h.state().handle().clone();
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::F11);
    h.run();
    assert!(handle.zen());
    assert!(!shown(&h, "side body"));
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::F11);
    h.run();
    assert!(!handle.zen());
    assert!(shown(&h, "side body"));
}

#[test]
fn an_application_binds_its_own_key_and_the_hint_names_the_key_leaving() {
    use egui_pop_model::keymap::Layer;
    struct Keys;
    impl Plugin for Keys {
        fn id(&self) -> PluginId {
            PluginId::from_static("keys")
        }
        fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
            // VS Code's key in place of the default one, and Escape
            // taken away (an unbinding acts on the layers below its own).
            ctx.keybinding("Ctrl+K Z", CMD_ZEN, "", Layer::Default)?
                .unbind("Shift+F11", Layer::Workspace)?
                .unbind("Escape", Layer::Workspace)?;
            Ok(())
        }
    }
    let app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(Demo)
        .plugin(Keys)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    let handle = h.state().handle().clone();
    assert_eq!(handle.keys_for(&CMD_ZEN), ["Ctrl+K Z"]);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    h.key_press(egui::Key::Z);
    h.run();
    assert!(handle.zen());
    // No key leaves it but the toggle: the hint names that one.
    assert!(h.query_by_label("Zen mode: Ctrl+K Z to leave").is_some());
}
