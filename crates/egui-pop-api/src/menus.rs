//! Menus drawn from their declarations, with egui's own menus
//! ([`egui::containers::menu::MenuButton`], `SubMenuButton`'s
//! [`egui::containers::menu::SubMenu`], [`egui::Popup`]): egui's
//! keyboard navigation and accessibility come with them.
//!
//! A command item shows the command's title, its first key binding (from
//! the keymap, so a menu never shows a key that does not work) and a tick
//! when its `checked` clause holds; it is greyed, with the reason, where
//! the command does not apply. Clicking runs the command as the user, in
//! the context the menu opened in.
//!
//! - [`bar`] draws a menu as a menu bar (the shell draws `app.main`);
//! - [`show`] draws a menu's items inside an open menu or popup;
//! - [`context_menu`] opens a declared menu on a widget's secondary click.
//!
//! ```
//! use egui_pop_api::{AppHandle, menus};
//! use egui_pop_model::ids::MenuId;
//!
//! const ROW: MenuId = MenuId::from_static("demo.row");
//!
//! fn row(ui: &mut egui::Ui, app: &AppHandle) {
//!     let r = ui.label("Right-click me");
//!     menus::context_menu(&r, app, &ROW);
//! }
//! ```

use egui::containers::menu::{MenuButton, MenuConfig, MenuState, SubMenu};
use egui::{Atom, Button, Id, Response, Ui, vec2};
use egui_pop_core::icon::Icon;
use egui_pop_core::theme::Tokens;
use egui_pop_model::ids::MenuId;
use egui_pop_model::menus::{Item, Shown, show_items};
use egui_pop_model::when::{When, WhenContext};

use crate::app::{AppHandle, Request};

/// How deep submenus may nest before the rest is cut (a menu naming
/// itself as a submenu).
const MAX_DEPTH: u32 = 8;

/// Draws a menu as a bar of menu buttons: each item a submenu opening a
/// popup, a command a button.
pub fn bar(ui: &mut Ui, app: &AppHandle, menu: &MenuId) {
    let cx = app.when_context();
    for shown in show_items(&app.menu(menu), &cx) {
        if let Shown::Item {
            item: Item::Submenu { menu: sub, label },
            enabled,
            ..
        } = shown
        {
            ui.add_enabled_ui(enabled, |ui| {
                MenuButton::new(label).ui(ui, |ui| items(ui, app, &sub, &cx, 0));
            });
        }
    }
}

/// Draws a menu's items in the current `Ui` (an open menu or popup), in
/// the current context.
pub fn show(ui: &mut Ui, app: &AppHandle, menu: &MenuId) {
    let cx = app.when_context();
    show_in(ui, app, menu, &cx);
}

/// Draws a menu's items, evaluated in a given context.
pub fn show_in(ui: &mut Ui, app: &AppHandle, menu: &MenuId, cx: &WhenContext) {
    ui.scope(|ui| {
        egui::containers::menu::menu_style(ui.style_mut());
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
            items(ui, app, menu, cx, 0);
        });
    });
}

/// Opens a declared menu on the widget's secondary click (or a long
/// touch). Its items are evaluated in the context of the moment it
/// opened, and their commands run in it.
pub fn context_menu(response: &Response, app: &AppHandle, menu: &MenuId) {
    let key = response.id.with("egui_pop_menu_context");
    if response.secondary_clicked() {
        let cx = app.when_context();
        response.ctx.data_mut(|d| d.insert_temp(key, cx));
    }
    response.context_menu(|ui| {
        let cx = ui
            .ctx()
            .data(|d| d.get_temp::<WhenContext>(key))
            .unwrap_or_else(|| app.when_context());
        items(ui, app, menu, &cx, 0);
    });
}

fn items(ui: &mut Ui, app: &AppHandle, menu: &MenuId, cx: &WhenContext, depth: u32) {
    if depth > MAX_DEPTH {
        ui.weak(format!("`{menu}` nests too deep"));
        return;
    }
    let list = app.menu(menu);
    if list.is_empty() {
        ui.weak("(empty)");
        return;
    }
    let t = Tokens::of(ui.ctx());
    let side = t.size.icon_inline;
    for shown in show_items(&list, cx) {
        match shown {
            Shown::Separator => {
                ui.separator();
            }
            Shown::Item {
                item,
                enabled,
                checked,
                checkable,
            } => match item {
                Item::Command { id, args, label } => {
                    let spec = app.command(&id);
                    let (title, why) = match &spec {
                        Some(s) => {
                            let applies =
                                s.when.is_empty() || When::parse(&s.when).is_ok_and(|w| w.eval(cx));
                            let why = (!applies)
                                .then(|| format!("Does not apply here ({})", s.when))
                                .or_else(|| (!enabled).then(|| "Not available here".to_string()));
                            (label.clone().unwrap_or_else(|| app.command_title(s)), why)
                        }
                        None => (id.to_string(), Some(format!("No command `{id}`"))),
                    };
                    // A run waiting for its result greys the item.
                    let why = why.or_else(|| app.running(&id).then(|| "Running…".to_string()));
                    let failed = app
                        .history()
                        .iter()
                        .rev()
                        .find(|r| r.id == id)
                        .and_then(|r| r.result.clone().err());
                    let tick = Id::new(("egui_pop_menu_tick", menu.as_str(), id.as_str()));
                    let mut button = Button::new((Atom::custom(tick, vec2(side, side)), title));
                    if let Some(k) = app.keys_for(&id).first() {
                        button = button.shortcut_text(k.clone());
                    }
                    let r = ui
                        .add_enabled_ui(why.is_none(), |ui| button.atom_ui(ui))
                        .inner;
                    if checked && let Some(rect) = r.rect(tick) {
                        let color = if why.is_none() {
                            t.text.default
                        } else {
                            t.text.muted
                        };
                        Icon::Check.paint(ui.painter(), rect, color);
                    }
                    let mut response = r.response;
                    if checkable {
                        let (on, name) =
                            (why.is_none(), label.unwrap_or_else(|| title_of(&spec, &id)));
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::Checkbox,
                                on,
                                checked,
                                &name,
                            )
                        });
                    }
                    if let Some(why) = why {
                        response = response.on_disabled_hover_text(why);
                    } else if let Some(e) = failed {
                        response = response.on_hover_text(format!("Last run failed: {e}"));
                    } else if let Some(s) = spec.filter(|s| !s.description.is_empty()) {
                        response = response.on_hover_text(s.description);
                    }
                    if response.clicked() {
                        app.request_run_in(id, args, cx.clone());
                        ui.close();
                    }
                }
                Item::Submenu { menu: sub, label } => {
                    ui.add_enabled_ui(enabled, |ui| submenu(ui, app, &sub, &label, cx, depth));
                }
                Item::List { name } => {
                    ui.weak(name);
                }
            },
        }
    }
}

fn title_of(
    spec: &Option<egui_pop_model::commands::CommandSpec>,
    id: &egui_pop_model::ids::CommandId,
) -> String {
    spec.as_ref()
        .map_or_else(|| id.to_string(), |s| s.title.clone())
}

/// A submenu button with a painted arrow, opening on hover as egui's
/// `SubMenuButton` does.
fn submenu(ui: &mut Ui, app: &AppHandle, menu: &MenuId, label: &str, cx: &WhenContext, depth: u32) {
    let t = Tokens::of(ui.ctx());
    let side = t.size.icon_inline;
    let my_id = ui.next_auto_id();
    let open = MenuState::from_ui(ui, |state, _| {
        state.open_item == Some(SubMenu::id_from_widget_id(my_id))
    });
    let space = Id::new(("egui_pop_menu_space", menu.as_str()));
    let arrow = Id::new(("egui_pop_menu_arrow", menu.as_str()));
    let button = Button::new((Atom::custom(space, vec2(side, side)), label))
        .right_text(Atom::custom(arrow, vec2(side, side)));
    let inactive = ui.style().visuals.widgets.inactive;
    if open {
        ui.style_mut().visuals.widgets.inactive = ui.style().visuals.widgets.open;
    }
    let r = button.atom_ui(ui);
    ui.style_mut().visuals.widgets.inactive = inactive;
    if let Some(rect) = r.rect(arrow) {
        Icon::ChevronRight.paint(ui.painter(), rect.shrink(rect.width() * 0.15), t.text.muted);
    }
    SubMenu::default()
        .config(MenuConfig::find(ui))
        .show(ui, &r.response, |ui| items(ui, app, menu, cx, depth + 1));
}

impl AppHandle {
    /// Asks for a command to run as the user in a given context (a menu
    /// opened there), before the next frame.
    pub fn request_run_in(
        &self,
        command: egui_pop_model::ids::CommandId,
        args: serde_json::Value,
        context: WhenContext,
    ) {
        self.push_request(Request::RunIn {
            command,
            args,
            origin: egui_pop_model::commands::Origin::User,
            context,
        });
    }
}
