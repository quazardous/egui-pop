//! Menus with lists filled when they open, opened as a popup at a point,
//! and context menus passing arguments of the moment to their commands.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::app::OverlayKind;
use egui_pop_api::menus::{self, ListItem, STEP_OPEN};
use egui_pop_api::steps::Before;
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, MenuId, PluginId};
use egui_pop_model::menus::MenuItem;
use serde_json::{Value as Json, json};

const OPEN: CommandId = CommandId::from_static("doc.open");
const RESET: CommandId = CommandId::from_static("doc.reset");
const RECENT: MenuId = MenuId::from_static("doc.recent");
const ROW: MenuId = MenuId::from_static("doc.row");

type Runs = Rc<RefCell<Vec<(CommandId, Json)>>>;

struct Doc {
    runs: Runs,
    asked: Rc<Cell<usize>>,
    veto: Rc<Cell<bool>>,
}

impl Plugin for Doc {
    fn id(&self) -> PluginId {
        PluginId::from_static("doc")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for id in [OPEN, RESET] {
            let runs = self.runs.clone();
            ctx.command(
                CommandSpec::new(id.clone(), id.as_str(), "Doc").when(if id == RESET {
                    "args.path"
                } else {
                    ""
                }),
                move |call| {
                    runs.borrow_mut().push((id.clone(), call.args.clone()));
                    Ok(Json::Null)
                },
            )?;
        }
        ctx.menu_item(MenuItem::list(RECENT, "1", 0, "doc.recent"))?;
        ctx.menu_item(MenuItem::command(ROW, "1", 0, RESET).args(json!({ "hard": true })))?;
        let asked = self.asked.clone();
        ctx.list_provider("doc.recent", move |_, _| {
            asked.set(asked.get() + 1);
            vec![
                ListItem::new("a.txt", OPEN, json!({ "path": "a.txt" })),
                ListItem::new("b.txt", OPEN, json!({ "path": "b.txt" })).checked(true),
            ]
        });
        let veto = self.veto.clone();
        ctx.before(STEP_OPEN, move |_, _| {
            if veto.get() {
                Before::Veto("not now".into())
            } else {
                Before::Go
            }
        });
        ctx.block(
            egui_pop_api::zones::MAIN,
            egui_pop_api::zones::Block::add(BlockId::from_static("doc.rows"), 0, |ui, app| {
                for path in ["node/exposure", "node/crop"] {
                    let r = ui.label(path);
                    menus::context_menu_with(&r, app, &ROW, json!({ "path": path }));
                }
            }),
        );
        Ok(())
    }
}

struct Setup {
    h: Harness<'static, App>,
    runs: Runs,
    asked: Rc<Cell<usize>>,
    veto: Rc<Cell<bool>>,
}

fn setup() -> Setup {
    let runs: Runs = Rc::default();
    let asked = Rc::new(Cell::new(0));
    let veto = Rc::new(Cell::new(false));
    let app = App::builder()
        .plugin(Doc {
            runs: runs.clone(),
            asked: asked.clone(),
            veto: veto.clone(),
        })
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(700.0, 500.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    Setup {
        h,
        runs,
        asked,
        veto,
    }
}

#[test]
fn a_list_is_asked_for_when_its_menu_opens_at_a_point_and_its_entries_run() {
    let Setup {
        mut h, runs, asked, ..
    } = setup();
    assert_eq!(asked.get(), 0, "not asked before it opens");
    let app = h.state().handle().clone();
    assert!(menus::popup_at(&app, &RECENT, egui::pos2(100.0, 120.0)));
    assert_eq!(menus::popup_open(&app), Some(RECENT));
    h.run();
    let after_open = asked.get();
    assert!(after_open >= 1);
    // Open over frames: asked once, not on every frame.
    for _ in 0..5 {
        h.step();
    }
    assert_eq!(asked.get(), after_open);
    let a = h.get_by_label("a.txt").rect();
    assert!(a.left() >= 99.0 && a.top() >= 119.0, "at the point: {a:?}");
    let menu = app
        .overlay_rects()
        .into_iter()
        .find(|o| o.kind == OverlayKind::Menu)
        .expect("the popup is reported");
    assert!(menu.rect.contains_rect(a));
    assert_eq!(menu.id, "doc.recent");
    let snapshot = app.debug_snapshot();
    assert_eq!(snapshot["lists"][0]["name"], "doc.recent");
    assert_eq!(snapshot["lists"][0]["last"][1]["label"], "b.txt");
    h.get_by_label("b.txt").click();
    h.run();
    h.run();
    assert_eq!(*runs.borrow(), [(OPEN, json!({ "path": "b.txt" }))]);
    assert_eq!(menus::popup_open(&app), None, "closed once chosen");
    // Opened again: asked again.
    menus::popup_at(&app, &RECENT, egui::pos2(100.0, 120.0));
    h.run();
    assert!(asked.get() > after_open);
    h.key_press(egui::Key::Escape);
    h.run();
    assert_eq!(menus::popup_open(&app), None, "Escape closes it");
}

#[test]
fn a_context_menu_passes_its_arguments_to_the_command_and_the_when_clause() {
    let Setup {
        mut h, runs, veto, ..
    } = setup();
    h.get_by_label("node/crop").click_secondary();
    h.run();
    // `doc.reset` applies only with `args.path`: the item is enabled.
    h.get_by_label("doc.reset").click();
    h.run();
    h.run();
    assert_eq!(
        *runs.borrow(),
        [(RESET, json!({ "hard": true, "path": "node/crop" }))]
    );
    let app = h.state().handle().clone();
    let trace = app.step_traces().pop().unwrap();
    assert_eq!(trace.args["how"], "context");
    // A hook vetoes the opening: no menu.
    veto.set(true);
    h.get_by_label("node/exposure").click_secondary();
    h.run();
    assert!(h.query_by_label("doc.reset").is_none());
    assert!(!menus::popup_at(&app, &RECENT, egui::pos2(10.0, 10.0)));
    assert_eq!(menus::popup_open(&app), None);
}
