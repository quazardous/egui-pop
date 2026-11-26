//! A command's live text (`pop.command.label`): the application's title
//! and tooltip for a command whose text follows its state, shown by its
//! menu item and tooltip; without an answer, the declared title and
//! description.

use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::bus::bus_contract::ListenOpts;
use egui_pop_api::bus::{self, COMMAND_LABEL_POINTS, CommandLabel, CommandText};
use egui_pop_api::menus;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, MenuId, PluginId};
use egui_pop_model::menus::MenuItem;
use serde_json::Value as Json;

const ZEN: CommandId = CommandId::from_static("demo.zen");
const SAVE: CommandId = CommandId::from_static("demo.save");
const MENU: MenuId = MenuId::from_static("demo.menu");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for (i, (id, title, description)) in [
            (ZEN, "Zen", "Shows one panel alone."),
            (SAVE, "Save", "Saves the document."),
        ]
        .into_iter()
        .enumerate()
        {
            ctx.command(
                CommandSpec::new(id.clone(), title, "Demo").description(description),
                |_| Ok(Json::Null),
            )?;
            ctx.menu_item(MenuItem::command(MENU, "1", i as i32, id))?;
        }
        Ok(())
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Demo)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(700.0, 500.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

/// Whether a node named `label` is drawn.
fn shown(h: &Harness<'static, App>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

/// Opens the menu, hovers `item` and says whether `tip` shows.
fn tooltip_of(h: &mut Harness<'static, App>, item: &str, tip: &str) -> bool {
    let app = h.state().handle().clone();
    assert!(menus::popup_at(&app, &MENU, egui::pos2(100.0, 120.0)));
    h.run();
    h.get_by_label(item).hover();
    for _ in 0..4 {
        h.run();
    }
    let seen = shown(h, tip);
    h.key_press(egui::Key::Escape);
    h.run();
    seen
}

#[test]
fn without_an_answer_a_command_shows_its_declared_text() {
    let mut h = harness();
    let app = h.state().handle().clone();
    assert_eq!(app.command_text(&ZEN), CommandText::default());
    let zen = app.command(&ZEN).unwrap();
    assert_eq!(app.command_title(&zen), "Zen");
    assert_eq!(app.command_tooltip(&zen), "Shows one panel alone.");
    assert!(tooltip_of(&mut h, "Zen", "Shows one panel alone."));
}

#[test]
fn the_applications_answer_names_the_menu_item_and_its_tooltip() {
    let mut h = harness();
    let app = h.state().handle().clone();
    let target = Arc::new(Mutex::new("Compare".to_string()));
    let answer = target.clone();
    app.bus()
        .listen::<CommandLabel>("demo.zen.label", ListenOpts::new(), move |id, text| {
            if *id == ZEN {
                let panel = answer.lock().unwrap().clone();
                *text = CommandText::default()
                    .title(format!("Zen: {panel}"))
                    .tooltip(format!("Shows {panel} alone."));
            }
            Ok(())
        })
        .unwrap();
    let zen = app.command(&ZEN).unwrap();
    assert_eq!(app.command_title(&zen), "Zen: Compare");
    assert_eq!(app.command_menu_title(&zen), "Zen: Compare");
    assert_eq!(app.command_tooltip(&zen), "Shows Compare alone.");
    assert!(tooltip_of(&mut h, "Zen: Compare", "Shows Compare alone."));
    // The others keep their declared text.
    let save = app.command(&SAVE).unwrap();
    assert_eq!(app.command_title(&save), "Save");
    assert!(tooltip_of(&mut h, "Save", "Saves the document."));
    // Live: asked each time it is drawn.
    *target.lock().unwrap() = "Viewer".into();
    assert!(tooltip_of(&mut h, "Zen: Viewer", "Shows Viewer alone."));
}

#[test]
fn the_command_label_point_is_declared() {
    for p in COMMAND_LABEL_POINTS {
        assert!(
            bus::CONTRACT.iter().any(|d| d.id == p),
            "{p} in the contract"
        );
        assert!(bus::POINTS.contains(&p), "{p} in the points");
    }
    assert_eq!(
        <CommandLabel as bus::bus_contract::Point>::ID,
        "pop.command.label"
    );
}
