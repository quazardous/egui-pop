//! Popups opened inside a `ScopedTheme` (a theme editor's preview) are
//! drawn in the scoped theme, egui's (a combo box, a tooltip) and
//! egui-pop's (a context menu, a popup menu drawn after the scope has
//! ended); opened outside it, in the theme installed.
//!
//! Each theme gets a popup fill of its own (`surface.overlay`, egui's
//! `window_fill`), so the fill under a popup's entry tells which theme
//! drew it.

use egui::Color32;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::menus;
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_core::state::Slot;
use egui_pop_core::theme::{ScopedTheme, Tokens};
use egui_pop_core::tokens::{ThemeSpec, Themes};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{BlockId, CommandId, MenuId, PluginId};
use egui_pop_model::menus::MenuItem;
use serde_json::{Value as Json, json};

const ACT: CommandId = CommandId::from_static("scoped.act");
const ROW: MenuId = MenuId::from_static("scoped.row");

/// The popup fill of the theme installed.
const INSTALLED: Color32 = Color32::from_rgb(0x00, 0xc0, 0x40);
/// The popup fill of the draft drawn in the scope.
const DRAFT: Color32 = Color32::from_rgb(0xff, 0x00, 0xff);

fn hex(c: Color32) -> Json {
    json!(format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b()))
}

/// The draft, kept across frames (a theme editor keeps its own).
const SCOPE: Slot<ScopedTheme> = Slot::opaque("test.scoped_popups.scope");

/// The same popups twice: outside the scope, then inside it.
struct Preview;

impl Plugin for Preview {
    fn id(&self) -> PluginId {
        PluginId::from_static("preview")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(CommandSpec::new(ACT, "Act", "Preview"), |_| Ok(Json::Null))?;
        ctx.menu_item(MenuItem::command(ROW, "1", 0, ACT))?;
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("preview.sides"), 0, |ui, app| {
                let scope = SCOPE.get_or_insert_with(ui.ctx(), || {
                    let spec =
                        ThemeSpec::new("pop-light").override_token("surface.overlay", hex(DRAFT));
                    ScopedTheme::new(ui.ctx(), Tokens::new(Themes::default().resolve(&spec)))
                });
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| popups(ui, app, "outside"));
                    ui.add_space(200.0);
                    scope.show(ui, |ui| ui.vertical(|ui| popups(ui, app, "inside")));
                });
            }),
        );
        Ok(())
    }
}

/// A combo box, a tooltip, a context menu and a popup menu, named after
/// their side.
fn popups(ui: &mut egui::Ui, app: &egui_pop_api::AppHandle, side: &str) {
    egui::ComboBox::from_label(format!("{side} combo"))
        .selected_text("one")
        .show_ui(ui, |ui| {
            let _ = ui.selectable_label(false, format!("{side} choice"));
        });
    let _ = ui
        .button(format!("{side} tip"))
        .on_hover_text(format!("{side} tooltip"));
    let r = ui.button(format!("{side} row"));
    menus::context_menu(&r, app, &ROW);
    let r = ui.button(format!("{side} popup"));
    if r.clicked() {
        // Drawn by the shell after the block, out of the scope.
        menus::popup_at(app, &ROW, r.rect);
    }
}

fn harness() -> Harness<'static, App> {
    let app = App::builder()
        .theme(ThemeSpec::new("pop-dark").override_token("surface.overlay", hex(INSTALLED)))
        .plugin(Preview)
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    h
}

/// The fills of the rectangles painted under the centre of the node
/// labelled `label`, in the last pass.
fn fills_under(h: &Harness<'_, App>, label: &str) -> Vec<Color32> {
    fn walk(shape: &egui::Shape, at: egui::Pos2, out: &mut Vec<Color32>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, at, out)),
            egui::Shape::Rect(r) if r.rect.contains(at) => out.push(r.fill),
            _ => {}
        }
    }
    let at = h.get_by_label(label).rect().center();
    let mut out = Vec::new();
    for c in &h.output().shapes {
        walk(&c.shape, at, &mut out);
    }
    out
}

/// Asserts that the popup holding `label` is drawn in the theme of
/// `side`.
fn assert_drawn_in(h: &Harness<'_, App>, label: &str, side: &str) {
    let fills = fills_under(h, label);
    let (wanted, other) = if side == "inside" {
        (DRAFT, INSTALLED)
    } else {
        (INSTALLED, DRAFT)
    };
    assert!(
        fills.contains(&wanted) && !fills.contains(&other),
        "{label}: drawn {side} the scope with {wanted:?}, got {fills:?}"
    );
}

#[test]
fn a_combo_box_list_is_drawn_in_the_theme_of_its_side() {
    for side in ["outside", "inside"] {
        let mut h = harness();
        h.get_by_role_and_label(egui::accesskit::Role::ComboBox, &format!("{side} combo"))
            .click();
        h.run();
        assert_drawn_in(&h, &format!("{side} choice"), side);
    }
}

#[test]
fn a_tooltip_is_drawn_in_the_theme_of_its_side() {
    for side in ["outside", "inside"] {
        let mut h = harness();
        h.get_by_label(&format!("{side} tip")).hover();
        h.run();
        h.run_steps(4);
        h.run();
        assert_drawn_in(&h, &format!("{side} tooltip"), side);
    }
}

#[test]
fn an_egui_pop_context_menu_is_drawn_in_the_theme_of_its_side() {
    for side in ["outside", "inside"] {
        let mut h = harness();
        h.get_by_label(&format!("{side} row")).click_secondary();
        h.run();
        assert_drawn_in(&h, "Act", side);
    }
}

#[test]
fn an_egui_pop_popup_menu_drawn_after_its_scope_keeps_its_theme() {
    for side in ["outside", "inside"] {
        let mut h = harness();
        h.get_by_label(&format!("{side} popup")).click();
        h.run();
        let app = h.state().handle().clone();
        assert_eq!(menus::popup_open(&app), Some(ROW));
        assert_drawn_in(&h, "Act", side);
        // The theme installed is back for the rest of the window.
        assert!(h.ctx.global_style().visuals.dark_mode);
        assert_eq!(h.ctx.global_style().visuals.window_fill, INSTALLED);
    }
}
