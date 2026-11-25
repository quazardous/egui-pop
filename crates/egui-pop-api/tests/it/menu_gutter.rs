//! The menu gutter (`docs/icons.md`, section 5): a column of fixed width
//! on every row's left holding the command's icon (framed while the
//! command is on), the tick of a checked item without an icon, the dot of
//! a chosen radio item; labels with and without an icon start at the same
//! x, in both densities and in high contrast; the `menus.icons` setting
//! hides the icons and keeps the gutter only where a mark needs it.

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_pop_api::menus::{self, Mark, MenuIcons, RowTrace, SETTING_ICONS};
use egui_pop_api::{App, AppBuilder, Ctx, Plugin, PluginError};
use egui_pop_core::theme::{Density, ThemeSpec};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, CommandId, MenuId, PluginId};
use egui_pop_model::menus::MenuItem;
use egui_pop_model::values::{Change, Set, Value};
use serde_json::{Value as Json, json};

/// A menu with every kind of row.
const MENU: MenuId = MenuId::from_static("demo.menu");
/// A menu of plain commands, without icons nor marks.
const PLAIN: MenuId = MenuId::from_static("demo.plain");
const SUB: MenuId = MenuId::from_static("demo.sub");

const SAVE: CommandId = CommandId::from_static("demo.save");
const LOCK: CommandId = CommandId::from_static("demo.lock");
const RULERS: CommandId = CommandId::from_static("demo.rulers");
const DELETE: CommandId = CommandId::from_static("demo.delete");
const ZOOM: CommandId = CommandId::from_static("demo.zoom");
const ABOUT: CommandId = CommandId::from_static("demo.about");

struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let app = ctx.app();
        app.set_context("locked", "true");
        app.set_context("rulers", "true");
        app.set_context("zoom", "100");
        ctx.command(
            CommandSpec::new(SAVE, "Save", "Demo")
                .icon("save")
                .key("Ctrl+S")
                .menu(MENU, "1", 0),
            |_| Ok(Json::Null),
        )?
        .command(
            CommandSpec::new(LOCK, "Lock", "Demo")
                .icon("unlock")
                .icon_on("lock")
                .checked("locked")
                .menu(MENU, "1", 1),
            |_| Ok(Json::Null),
        )?
        .command(
            CommandSpec::new(RULERS, "Rulers", "Demo")
                .checked("rulers")
                .menu(MENU, "1", 2),
            |_| Ok(Json::Null),
        )?
        .command(
            CommandSpec::new(DELETE, "Delete", "Demo")
                .icon("delete")
                .when("never")
                .menu(MENU, "1", 3),
            |_| Ok(Json::Null),
        )?
        .command(
            CommandSpec::new(ZOOM, "Zoom", "Demo").args(json!({ "percent": 100 })),
            |_| Ok(Json::Null),
        )?
        .command(
            CommandSpec::new(ABOUT, "About", "Demo").menu(PLAIN, "1", 0),
            |_| Ok(Json::Null),
        )?
        .submenu(MENU, "3", 0, SUB, "More")?;
        for (i, p) in ["50", "100"].into_iter().enumerate() {
            ctx.menu_item(
                MenuItem::command(MENU, "2", i as i32, ZOOM)
                    .args(json!({ "percent": p }))
                    .label(&format!("{p}%"))
                    .radio(&format!("zoom == '{p}'")),
            )?;
        }
        ctx.menu_item(MenuItem::command(SUB, "1", 0, ABOUT))?;
        ctx.block(
            egui_pop_api::zones::MAIN,
            egui_pop_api::zones::Block::add(BlockId::from_static("demo.menus"), 0, |ui, app| {
                for menu in [MENU, PLAIN] {
                    egui::Frame::menu(ui.style()).show(ui, |ui| {
                        ui.set_max_width(240.0);
                        menus::show(ui, app, &menu);
                    });
                }
            }),
        );
        Ok(())
    }
}

fn builder() -> AppBuilder {
    App::builder().plugin(Demo)
}

fn harness(builder: AppBuilder) -> Harness<'static, App> {
    let app = builder.build(&egui::Context::default()).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(700.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

fn rows(h: &Harness<'_, App>, menu: &MenuId) -> Vec<RowTrace> {
    menus::rows_drawn(&h.ctx)
        .into_iter()
        .filter(|r| r.menu == *menu)
        .collect()
}

fn row<'a>(rows: &'a [RowTrace], label: &str) -> &'a RowTrace {
    rows.iter()
        .find(|r| r.label == label)
        .unwrap_or_else(|| panic!("no row {label} in {rows:?}"))
}

/// Every row of the menu starts its label at the same x, after a gutter
/// of `gutter` points.
fn assert_aligned(rows: &[RowTrace], gutter: f32) {
    assert!(!rows.is_empty());
    let x = rows[0].label_x;
    for r in rows {
        assert_eq!(r.gutter, gutter, "{r:?}");
        assert!((r.label_x - x).abs() < 0.01, "{r:?} against {x}");
    }
    assert!(x >= gutter, "the label starts after the gutter: {x}");
}

fn set_icons(h: &mut Harness<'_, App>, icons: MenuIcons) {
    egui_pop_core::lock(&h.state().handle().store())
        .set(Set::new(
            &format!("settings/{SETTING_ICONS}"),
            Change::To(Value::Choice(icons.as_str().into())),
            Origin::User,
        ))
        .unwrap();
    h.run();
}

#[test]
fn labels_with_and_without_an_icon_start_at_the_same_x() {
    let h = harness(builder());
    let rows = rows(&h, &MENU);
    assert_eq!(rows.len(), 7, "{rows:?}");
    // The comfortable density: the inline icon (16) and 4 on each side.
    assert_aligned(&rows, 24.0);
    assert_eq!(row(&rows, "Save").mark, Mark::Icon);
    assert_eq!(row(&rows, "Save").icon.as_deref(), Some("save"));
    assert_eq!(row(&rows, "More").mark, Mark::None);
    // A menu without icons nor marks keeps the gutter too: Word's look.
    let plain = self::rows(&h, &PLAIN);
    assert_aligned(&plain, 24.0);
}

#[test]
fn a_checked_command_frames_its_icon_and_one_without_an_icon_is_ticked() {
    let h = harness(builder());
    let rows = rows(&h, &MENU);
    // The "on" icon, framed.
    let lock = row(&rows, "Lock");
    assert_eq!(lock.mark, Mark::Framed);
    assert_eq!(lock.icon.as_deref(), Some("lock"));
    assert_eq!(row(&rows, "Rulers").mark, Mark::Check);
    // The chosen zoom has the dot, the other nothing.
    assert_eq!(row(&rows, "100%").mark, Mark::Radio);
    assert_eq!(row(&rows, "50%").mark, Mark::None);
    // Unchecked, the command's own icon comes back, unframed.
    let app = h.state().handle().clone();
    app.set_context("locked", "false");
    let mut h = h;
    h.run();
    let lock = self::rows(&h, &MENU);
    let lock = row(&lock, "Lock");
    assert_eq!(lock.mark, Mark::Icon);
    assert_eq!(lock.icon.as_deref(), Some("unlock"));
}

#[test]
fn a_disabled_entry_keeps_its_icon_greyed_and_its_reason() {
    let h = harness(builder());
    let rows = rows(&h, &MENU);
    let delete = row(&rows, "Delete");
    assert_eq!(delete.mark, Mark::Icon);
    assert_eq!(delete.icon.as_deref(), Some("delete"));
    assert!(
        h.query_all_by_label("Delete")
            .all(|n| n.accesskit_node().is_disabled())
    );
}

#[test]
fn the_gutter_is_decorative_check_and_radio_items_say_their_state() {
    let h = harness(builder());
    let lock = h.get_by_label("Lock");
    assert_eq!(lock.accesskit_node().role(), Role::CheckBox);
    assert_eq!(lock.accesskit_node().toggled(), Some(Toggled::True));
    let chosen = h.get_by_label("100%");
    assert_eq!(chosen.accesskit_node().role(), Role::RadioButton);
    assert_eq!(chosen.accesskit_node().toggled(), Some(Toggled::True));
    let other = h.get_by_label("50%");
    assert_eq!(other.accesskit_node().role(), Role::RadioButton);
    assert_eq!(other.accesskit_node().toggled(), Some(Toggled::False));
    // The icon adds no node: the row is one button named by its title.
    let save: Vec<_> = h.query_all_by_label_contains("Save").collect();
    assert_eq!(save.len(), 1, "one node for the Save row");
    assert_eq!(save[0].accesskit_node().role(), Role::Button);
}

#[test]
fn the_compact_density_narrows_the_gutter_and_keeps_labels_aligned() {
    let h = harness(builder().theme(ThemeSpec::new("pop-dark").density(Density::Compact)));
    // The inline icon (12) and 2 on each side.
    assert_aligned(&rows(&h, &MENU), 16.0);
}

#[test]
fn high_contrast_keeps_the_gutter_the_frame_and_the_alignment() {
    let h = harness(builder().theme(ThemeSpec::new("pop-high-contrast")));
    let rows = rows(&h, &MENU);
    assert_aligned(&rows, 24.0);
    assert_eq!(row(&rows, "Lock").mark, Mark::Framed);
    let kit = egui_pop_core::kit::Kit::of(&h.ctx);
    let frame = kit
        .menu_item_gutter
        .style(egui_pop_core::kit::State::SELECTED);
    assert!(frame.border_width >= 1.0, "the on frame keeps its border");
    assert_ne!(frame.border_color, egui::Color32::TRANSPARENT);
}

#[test]
fn icons_off_hides_them_and_keeps_the_gutter_only_for_marks() {
    let mut h = harness(builder());
    set_icons(&mut h, MenuIcons::Off);
    let rows = rows(&h, &MENU);
    assert_aligned(&rows, 24.0);
    assert!(rows.iter().all(|r| r.icon.is_none()), "{rows:?}");
    // Without its icon, a checked command is ticked.
    assert_eq!(row(&rows, "Lock").mark, Mark::Check);
    assert_eq!(row(&rows, "Save").mark, Mark::None);
    // A menu with no check or radio item keeps no gutter.
    let plain = self::rows(&h, &PLAIN);
    assert_aligned(&plain, 0.0);
    assert!(plain[0].label_x < rows[0].label_x - 20.0, "{plain:?}");
    set_icons(&mut h, MenuIcons::On);
    assert_eq!(self::rows(&h, &PLAIN)[0].gutter, 24.0);
}

#[test]
fn the_setting_is_declared_with_its_three_choices_on_by_default() {
    let h = harness(builder());
    let app = h.state().handle().clone();
    let setting = app
        .settings()
        .into_iter()
        .find(|s| s.id == SETTING_ICONS)
        .expect("menus.icons is declared");
    assert_eq!(setting.descriptor.default.as_choice(), Some("on"));
    assert_eq!(menus::menu_icons(&app), MenuIcons::On);
    for c in MenuIcons::CHOICES {
        assert_eq!(MenuIcons::parse(c).map(MenuIcons::as_str), Some(c));
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn platform_follows_gnome_and_kde_plasma() {
    use menus::platform_shows_icons as shows;
    assert!(!shows("GNOME", || None));
    assert!(!shows("ubuntu:GNOME", || None));
    assert!(shows("KDE", || None));
    assert!(shows("KDE", || Some(
        "[KDE]\nShowIconsInMenuItems=true\n".into()
    )));
    assert!(!shows("KDE", || Some(
        "[General]\nShowIconsInMenuItems=true\n[KDE]\nShowIconsInMenuItems=false\n".into()
    )));
    // Only the KDE group counts.
    assert!(shows("KDE", || Some(
        "[General]\nShowIconsInMenuItems=false\n".into()
    )));
    assert!(shows("XFCE", || None));
    assert!(shows("", || None));
}
