//! Menus with lists filled when they open, opened as a popup at a point,
//! context menus passing arguments of the moment to their commands, and
//! the main menu bar overflowing into "…" when the window is narrow.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::app::{MENU_FILE, MENU_HELP, MENU_MAIN, OverlayKind};
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
                ListItem::new("b.txt", OPEN, json!({ "path": "b.txt" }))
                    .checked(true)
                    .icon("history"),
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
    // Agents read an entry's icon by name.
    assert_eq!(snapshot["lists"][0]["last"][1]["icon"], "history");
    assert!(snapshot["lists"][0]["last"][0].get("icon").is_none());
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

const EXPORT: CommandId = CommandId::from_static("doc.export");
const ABOUT: CommandId = CommandId::from_static("doc.about");

/// A command in File, one in Help.
struct Items;

impl Plugin for Items {
    fn id(&self) -> PluginId {
        PluginId::from_static("items")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(EXPORT, "Export", "File").menu(MENU_FILE, "1", 0),
            |_| Ok(Json::Null),
        )?;
        ctx.command(
            CommandSpec::new(ABOUT, "About", "Help").menu(MENU_HELP, "1", 0),
            |_| Ok(Json::Null),
        )?;
        Ok(())
    }
}

fn bar_harness(width: f32) -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Items)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(width, 300.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn ids(menus: &[MenuId]) -> Vec<&str> {
    menus.iter().map(MenuId::as_str).collect()
}

/// The menus' widths as the wide bar draws them: File and Edit fit in a
/// window as wide as Edit's right edge, plus "…" and the margins.
fn middle_width() -> f32 {
    let h = bar_harness(700.0);
    h.get_by_label("Edit").rect().right() + 50.0
}

#[test]
fn the_menus_that_do_not_fit_go_into_a_more_menu() {
    let all = [
        "app.menu.file",
        "app.menu.edit",
        "app.menu.view",
        "app.menu.help",
    ];
    // Wide: every menu in the bar.
    let h = bar_harness(700.0);
    let app = h.state().handle().clone();
    let wide = menus::bar_layout(&app, &MENU_MAIN).expect("drawn");
    assert_eq!(ids(&wide.bar), all);
    assert!(wide.overflow.is_empty() && !wide.collapsed);
    assert!(h.query_by_label("More menus").is_none());
    assert_eq!(wide.keys["app.menu.file"], "f");
    assert_eq!(wide.keys["app.menu.help"], "h");
    // Middle: File and Edit stay, View and Help go into "…".
    let mut h = bar_harness(middle_width());
    let app = h.state().handle().clone();
    let middle = menus::bar_layout(&app, &MENU_MAIN).unwrap();
    assert_eq!(ids(&middle.bar), all[..2]);
    assert_eq!(ids(&middle.overflow), all[2..]);
    assert!(!middle.collapsed);
    assert!(h.query_by_label("View").is_none(), "hidden while closed");
    let snapshot = app.debug_snapshot();
    assert_eq!(
        snapshot["menu_bar"]["app.main"]["overflow"],
        json!(["app.menu.view", "app.menu.help"])
    );
    // The accessibility tree names "…" and, open, its children.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "More menus")
        .click();
    h.run();
    assert!(h.query_by_label("View").is_some());
    assert!(h.query_by_label("Help").is_some());
    // Narrow: one entry for the whole menu.
    let mut h = bar_harness(40.0);
    let app = h.state().handle().clone();
    let narrow = menus::bar_layout(&app, &MENU_MAIN).unwrap();
    assert!(narrow.bar.is_empty() && narrow.collapsed);
    assert_eq!(ids(&narrow.overflow), all);
    assert!(h.query_by_label("More menus").is_none());
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Main menu")
        .click();
    h.run();
    for label in ["File", "Edit", "View", "Help"] {
        assert!(h.query_by_label(label).is_some(), "{label} in the menu");
    }
}

#[test]
fn alt_and_a_hidden_menus_key_opens_it_inside_the_more_menu() {
    let mut h = bar_harness(middle_width());
    assert!(h.query_by_label("About").is_none());
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::H);
    h.run();
    assert!(
        h.query_by_label("About").is_some(),
        "Help opened inside \"…\""
    );
    assert!(h.query_by_label("View").is_some(), "\"…\" is open");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.query_by_label("About").is_none());
    // A menu in the bar opens the same way.
    h.key_press_modifiers(egui::Modifiers::ALT, egui::Key::F);
    h.run();
    assert!(h.query_by_label("Export").is_some(), "File opened");
}

/// A block in the top bar, after the menus.
struct TopBlock;

impl Plugin for TopBlock {
    fn id(&self) -> PluginId {
        PluginId::from_static("top")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.block(
            egui_pop_api::zones::TOP_BAR,
            egui_pop_api::zones::Block::add(BlockId::from_static("top.status"), 0, |ui, _| {
                ui.label("Top status");
            }),
        );
        Ok(())
    }
}

fn top_harness(width: f32) -> Harness<'static, App> {
    let app = App::builder()
        .plugin(Items)
        .plugin(TopBlock)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(width, 300.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

#[test]
fn the_top_bars_blocks_keep_their_room_after_the_menus() {
    // Wide: the menus and the block side by side.
    let h = top_harness(700.0);
    let app = h.state().handle().clone();
    let wide = menus::bar_layout(&app, &MENU_MAIN).unwrap();
    assert!(wide.overflow.is_empty(), "{wide:?}");
    let help = h.get_by_label("Help").rect().right();
    let block = h.get_by_label("Top status").rect();
    // As wide as the menus alone: the block's room pushes some into "…".
    let mut h = top_harness(help + 20.0);
    h.run();
    let app = h.state().handle().clone();
    let narrow = menus::bar_layout(&app, &MENU_MAIN).unwrap();
    assert!(!narrow.overflow.is_empty(), "{narrow:?}");
    let shown = h.get_by_label("Top status").rect();
    assert!(shown.right() <= help + 20.0, "the block is not cut");
    assert_eq!(shown.width(), block.width());
}

const CARD: MenuId = MenuId::from_static("swatches.card");
const CARD_RENAME: CommandId = CommandId::from_static("swatches.rename");

/// A card with a declared context menu, holding a plain label and a
/// swatch that takes the right click for itself (it resets) and senses
/// drags only, so egui gives its clicks to the card under it.
struct Swatches {
    resets: Rc<Cell<usize>>,
}

impl Plugin for Swatches {
    fn id(&self) -> PluginId {
        PluginId::from_static("swatches")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CARD_RENAME, "swatches.rename", "Swatches"),
            |_| Ok(Json::Null),
        )?;
        ctx.menu_item(MenuItem::command(CARD, "1", 0, CARD_RENAME))?;
        let resets = self.resets.clone();
        ctx.block(
            egui_pop_api::zones::MAIN,
            egui_pop_api::zones::Block::add(
                BlockId::from_static("swatches.card"),
                0,
                move |ui, app| {
                    use egui_pop_core::pointer::claim::{self, SecondaryClaim};
                    let rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(300.0, 120.0));
                    let card = ui.interact(rect, ui.id().with("card"), egui::Sense::click());
                    ui.add(egui::Label::new("plain").selectable(false));
                    let swatch = ui.allocate_response(egui::vec2(40.0, 40.0), egui::Sense::drag());
                    swatch.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "swatch")
                    });
                    claim::claim_secondary(
                        &swatch,
                        SecondaryClaim::new("Swatch").click("Resets the colour"),
                    );
                    if ui.rect_contains_pointer(swatch.rect)
                        && ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary))
                    {
                        resets.set(resets.get() + 1);
                    }
                    menus::context_menu(&card, app, &CARD);
                },
            ),
        );
        Ok(())
    }
}

#[test]
fn a_widget_that_claims_the_right_click_keeps_it_from_the_menu_around_it() {
    let resets = Rc::new(Cell::new(0));
    let app = App::builder()
        .plugin(Swatches {
            resets: resets.clone(),
        })
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(700.0, 500.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    // An ordinary widget: the card's menu opens.
    h.get_by_label("plain").click_secondary();
    h.run();
    assert!(
        h.query_by_label("swatches.rename").is_some(),
        "the menu opens"
    );
    // A click on the card, away from the menu, closes it.
    let away = h.get_by_label("plain").rect().left_top() + egui::vec2(250.0, 100.0);
    h.event(egui::Event::PointerMoved(away));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: away,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run();
    assert!(
        h.query_by_label("swatches.rename").is_none(),
        "the menu closed"
    );
    // The swatch: its own action, and no menu.
    let before = resets.get();
    h.get_by_label("swatch").click_secondary();
    h.run();
    assert_eq!(resets.get(), before + 1, "the swatch resets");
    assert!(
        h.query_by_label("swatches.rename").is_none(),
        "no menu over the swatch"
    );
    // Agents see the claim, and the use it said.
    let snapshot = h.state().handle().debug_snapshot();
    let pointer = &snapshot["pointer"];
    assert!(
        pointer["claims"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["widget"] == "Swatch" && c["click"] == "Resets the colour"),
        "{pointer}"
    );
    assert_eq!(
        pointer["uses"],
        json!([{ "widget": "Swatch", "gesture": "SecondaryClick", "does": "Resets the colour" }])
    );
}
