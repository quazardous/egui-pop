//! Focusing a panel focuses the panel, not its first widget: a slider
//! first in a panel stays unfocused, so the wheel over it scrolls the
//! panel; Tab then enters the widgets. A panel that names a widget with
//! `focus::prefer` hands the focus to it.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::focus;
use egui_pop_core::slider::Slider;
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::panels::{Area, PanelSpec};

const PARAMS: PanelId = PanelId::from_static("demo.params");
const SEARCH: PanelId = PanelId::from_static("demo.search");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let mut exposure = 0.0;
        let mut text = String::new();
        ctx.panel(
            PanelSpec::new(PARAMS, "Parameters", Area::EndSide),
            move |ui, _| {
                Slider::new(&mut exposure, "Exposure", -2.0..=2.0).show(ui);
                let _ = ui.button("Reset");
            },
        )?
        .panel(
            PanelSpec::new(SEARCH, "Search", Area::Bottom),
            move |ui, _| {
                let field = ui.text_edit_singleline(&mut text).accessible_name("Find");
                focus::prefer(ui.ctx(), field.id);
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

#[test]
fn focusing_a_panel_focuses_the_panel_not_its_first_slider() {
    let mut h = harness();
    h.state().handle().focus_panel(&PARAMS);
    h.run();
    assert_eq!(h.state().handle().focused_panel(), Some(PARAMS));
    let slider = h.get_by_role_and_label(Role::Slider, "Exposure");
    assert!(!slider.is_focused(), "the slider waits for Tab or a click");
    assert!(
        h.get_by_role_and_label(Role::Pane, "Parameters")
            .is_focused(),
        "the panel holds the focus itself"
    );
    // Tab enters its widgets, the slider first.
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(
        h.get_by_role_and_label(Role::Slider, "Exposure")
            .is_focused()
    );
    assert_eq!(h.state().handle().focused_panel(), Some(PARAMS));
}

#[test]
fn a_panel_that_prefers_a_widget_hands_it_the_focus() {
    let mut h = harness();
    h.state().handle().focus_panel(&SEARCH);
    h.run();
    assert_eq!(h.state().handle().focused_panel(), Some(SEARCH));
    assert!(
        h.get_by_role_and_label(Role::TextInput, "Find")
            .is_focused()
    );
}
