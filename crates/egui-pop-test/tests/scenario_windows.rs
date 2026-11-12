//! A scenario's `Window` step: the `Ui` steps and `Expect(Tree)` that
//! follow read and act in one application window. kittest embeds the
//! second window in the main one; each has a button of the same name.

use std::cell::Cell;
use std::rc::Rc;

use egui_pop_api::windows::WindowSpec;
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, AppBuilder, Ctx, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, PluginId};
use egui_pop_test::scenario::{Runner, parse};
use serde_json::Value as Json;

/// How often each window's button was pressed: the main one's, the
/// second one's.
type Pressed = Rc<[Cell<u32>; 2]>;

/// A main window with a "Press" button, and `test.second`, which opens a
/// second window with a "Press" button of its own.
struct TwoWindows(Pressed);

impl Plugin for TwoWindows {
    fn id(&self) -> PluginId {
        PluginId::from_static("test.windows")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let main = self.0.clone();
        let second = self.0.clone();
        ctx.block(
            zones::MAIN,
            Block::add(
                BlockId::from_static("test.windows.main"),
                0,
                move |ui, _| {
                    // Below where egui embeds the second window (top left).
                    ui.add_space(320.0);
                    if ui.button("Press").clicked() {
                        main[0].set(main[0].get() + 1);
                    }
                    ui.label(format!("main pressed {}", main[0].get()));
                },
            ),
        );
        ctx.command(
            CommandSpec::new(
                CommandId::from_static("test.second"),
                "Second window",
                "Test",
            ),
            move |c| {
                let pressed = second.clone();
                c.app.open_window(
                    WindowSpec::new("second", "Second").size([260.0, 140.0]),
                    move |ui, _| {
                        if ui.button("Press").clicked() {
                            pressed[1].set(pressed[1].get() + 1);
                        }
                        ui.label(format!("second pressed {}", pressed[1].get()));
                    },
                );
                Ok(Json::Null)
            },
        )?;
        Ok(())
    }
}

fn runner(pressed: &Pressed) -> Runner {
    let pressed = pressed.clone();
    Runner::new(move || -> AppBuilder { App::builder().plugin(TwoWindows(pressed.clone())) })
}

#[test]
fn scenario_a_window_step_clicks_in_the_second_window_then_in_the_main_one() {
    let pressed = Pressed::default();
    let scenario = parse(
        r#"Scenario(
            name: "two-windows",
            fixtures: [Fixture("test.second", ())],
            steps: [
                Wait(Settled),
                Window("second"),
                Expect(Tree(has: (role: "Window", label: "Second"))),
                Expect(Tree(has: (label: "second pressed 0"), lacks: (label: "main pressed 0"))),
                Ui(Click(role: "Button", label: "Press")),
                Wait(Settled),
                Expect(Tree(has: (label: "second pressed 1"))),
                Window("main"),
                Expect(Tree(has: (label: "main pressed 0"), lacks: (role: "Window", label: "Second"))),
                Expect(Tree(lacks: (label: "second pressed 1"))),
                Ui(Click(role: "Button", label: "Press")),
                Wait(Settled),
                Expect(Tree(has: (label: "main pressed 1"))),
            ],
        )"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let passed = runner(&pressed)
        .run(&scenario)
        .unwrap_or_else(|f| panic!("{f}"));
    assert_eq!(passed.steps, 13);
    assert_eq!((pressed[0].get(), pressed[1].get()), (1, 1));
}

#[test]
fn scenario_a_window_not_open_breaks_its_step_with_the_open_ones() {
    let pressed = Pressed::default();
    let scenario = parse(
        r#"Scenario(
            name: "no-such-window",
            steps: [
                Wait(Settled),
                Window("third"),
                Ui(Click(role: "Button", label: "Press")),
            ],
        )"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let f = runner(&pressed)
        .run(&scenario)
        .expect_err("no window `third`");
    assert_eq!(f.step, Some(3), "{f}");
    assert_eq!(f.broken, "ui", "{f}");
    assert!(
        f.message.contains("third") && f.message.contains("main"),
        "{f}"
    );
    assert_eq!(pressed[0].get(), 0);
}
