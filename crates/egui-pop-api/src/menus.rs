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
//! A list item (`MenuItem::list`) is filled by the list provider of its
//! name (`Ctx::list_provider`), asked when the menu opens and never on
//! every frame: the recent files, the profiles. Each entry it gives runs
//! a command with its arguments; the `lists` debug collector shows what
//! each provider gave last.
//!
//! - [`bar`] draws a menu as a menu bar (the shell draws `app.main`);
//! - [`show`] draws a menu's items inside an open menu or popup;
//! - [`context_menu`] opens a declared menu on a widget's secondary click,
//!   and [`context_menu_with`] with arguments of the moment (the clicked
//!   row's path), merged into every item's command and seen by the `when`
//!   clauses as `args.<key>`;
//! - [`popup_at`] opens a declared menu as a popup under a rectangle or
//!   at a point (a logo, `Alt+Space`); egui-pop draws it until an item is
//!   chosen, a click lands outside or Escape is pressed.
//!
//! Opening a menu by [`popup_at`] or a context menu runs the
//! [`STEP_OPEN`] step (ADR 0011): a plugin may veto it or change its
//! arguments.
//!
//! ```
//! use egui_pop_api::{AppHandle, menus};
//! use egui_pop_model::ids::MenuId;
//!
//! const ROW: MenuId = MenuId::from_static("demo.row");
//!
//! fn row(ui: &mut egui::Ui, app: &AppHandle, path: &str) {
//!     let r = ui.label("Right-click me");
//!     menus::context_menu_with(&r, app, &ROW, serde_json::json!({ "path": path }));
//! }
//! ```

use egui_pop_core::kit::{Component, Glyph, Kit, Part, State, Tone};
use std::fmt;
use std::rc::Rc;

use egui::containers::menu::{MenuButton, MenuConfig, MenuState, SubMenu};
use egui::{
    Atom, Button, Color32, Context, Id, Pos2, Rect, Response, RichText, Shape, Stroke, Ui, vec2,
};
use egui_pop_core::icon::Icon;
use egui_pop_core::state::Slot;
use egui_pop_core::strings;

/// What a context menu opened with (its `when` context and arguments,
/// `None` when vetoed), per widget.
const CONTEXT_MENU: Slot<Option<(WhenContext, Json)>> = Slot::new("api.menus.context");
/// A dynamic list's items and the pass they were listed on, per menu
/// and list.
const LIST: Slot<(u64, Vec<ListItem>)> = Slot::new("api.menus.list");
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, MenuId, OwnerId, StepId};
use egui_pop_model::menus::{Item, Shown, show_items};
use egui_pop_model::when::{When, WhenContext};
use serde::Serialize;
use serde_json::{Map, Value as Json, json};

use crate::app::{AppHandle, Request};
use crate::steps::Outcome;
use crate::strings as txt;

/// How deep submenus may nest before the rest is cut (a menu naming
/// itself as a submenu).
const MAX_DEPTH: u32 = 8;

/// A menu opens as a popup or a context menu: `{"menu": id, "args":
/// arguments, "how": "popup" | "context"}`. A veto keeps it closed, a
/// redirect changes its arguments.
pub const STEP_OPEN: StepId = StepId::from_static("menu.open");

/// An entry a list provider gives when its menu opens.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListItem {
    pub label: String,
    #[serde(skip)]
    pub icon: Option<Icon>,
    /// What it runs, with its arguments.
    pub command: CommandId,
    pub args: Json,
    /// Shown ticked (the profile in use).
    pub checked: bool,
    /// Greyed when false (with no reason: the label says it).
    pub enabled: bool,
}

impl ListItem {
    pub fn new(label: &str, command: CommandId, args: Json) -> Self {
        Self {
            label: label.into(),
            icon: None,
            command,
            args,
            checked: false,
            enabled: true,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }

    /// Greyed: a placeholder ("No recent file").
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// A list provider: the entries of a list, in the context the menu opens
/// in.
pub type ListFn = Rc<dyn Fn(&AppHandle, &WhenContext) -> Vec<ListItem>>;

/// A registered list provider.
#[derive(Clone)]
pub(crate) struct Provider {
    pub name: String,
    pub f: ListFn,
}

impl fmt::Debug for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "list provider {}", self.name)
    }
}

/// What a list provider gave last, as the `lists` collector shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListTrace {
    pub name: String,
    pub owner: OwnerId,
    pub items: Vec<ListItem>,
}

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
            let label = strings::menu_label(ui.ctx(), sub.as_str(), &label);
            ui.add_enabled_ui(enabled, |ui| {
                MenuButton::new(label).ui(ui, |ui| items(ui, app, &sub, &cx, &Json::Null, 0));
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
    show_with(ui, app, menu, cx, &Json::Null);
}

/// Draws a menu's items in a context, with arguments merged into every
/// command an item runs.
fn show_with(ui: &mut Ui, app: &AppHandle, menu: &MenuId, cx: &WhenContext, args: &Json) {
    ui.scope(|ui| {
        egui::containers::menu::menu_style(ui.style_mut());
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
            items(ui, app, menu, cx, args, 0);
        });
    });
}

/// Opens a declared menu on the widget's secondary click (or a long
/// touch). Its items are evaluated in the context of the moment it
/// opened, and their commands run in it.
pub fn context_menu(response: &Response, app: &AppHandle, menu: &MenuId) {
    context_menu_with(response, app, menu, Json::Null);
}

/// As [`context_menu`], with arguments of this call (the clicked row's
/// path): merged into every item's command arguments, and seen by `when`
/// clauses as `args.<key>`.
pub fn context_menu_with(response: &Response, app: &AppHandle, menu: &MenuId, args: Json) {
    let key = CONTEXT_MENU.keyed(response.id);
    if response.secondary_clicked() {
        let opened = open_step(app, menu, args, "context");
        let state = opened.map(|args| (with_args(&app.when_context(), &args), args));
        key.set(&response.ctx, state);
    }
    response.context_menu(|ui| {
        let state = key
            .get(ui.ctx())
            .unwrap_or_else(|| Some((app.when_context(), Json::Null)));
        match state {
            Some((cx, args)) => items(ui, app, menu, &cx, &args, 0),
            // Vetoed: it closes at once.
            None => ui.close(),
        }
    });
}

/// A menu open as a popup.
#[derive(Clone, Debug)]
pub(crate) struct Popup {
    pub menu: MenuId,
    pub under: Rect,
    pub args: Json,
    pub context: WhenContext,
}

/// Where a popup menu opens: under a rectangle (a button, a logo), or at
/// a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct At(pub Rect);

impl From<Rect> for At {
    fn from(r: Rect) -> Self {
        At(r)
    }
}

impl From<Pos2> for At {
    fn from(p: Pos2) -> Self {
        At(Rect::from_min_size(p, egui::Vec2::ZERO))
    }
}

/// The popup's egui id.
pub(crate) fn popup_id() -> Id {
    Id::new("egui_pop.menu.popup")
}

/// Opens a declared menu as a popup under a rectangle or at a point,
/// evaluated in the context of now; egui-pop draws it from the next
/// frame until an item is chosen, a click lands outside or Escape is
/// pressed. One popup at a time: another replaces it. Returns whether it
/// opened (a hook on [`STEP_OPEN`] may veto it).
pub fn popup_at(app: &AppHandle, menu: &MenuId, at: impl Into<At>) -> bool {
    popup_at_with(app, menu, at, Json::Null)
}

/// As [`popup_at`], with arguments merged into the items' commands.
pub fn popup_at_with(app: &AppHandle, menu: &MenuId, at: impl Into<At>, args: Json) -> bool {
    let Some(args) = open_step(app, menu, args, "popup") else {
        return false;
    };
    let popup = Popup {
        menu: menu.clone(),
        under: at.into().0,
        context: with_args(&app.when_context(), &args),
        args,
    };
    app.set_popup(Some(popup));
    app.egui().request_repaint();
    true
}

/// The menu open as a popup, if one is.
pub fn popup_open(app: &AppHandle) -> Option<MenuId> {
    app.popup().map(|p| p.menu)
}

/// Closes the popup menu, if one is open.
pub fn close_popup(app: &AppHandle) {
    app.set_popup(None);
}

/// Draws the popup menu while it is open (the shell does, each frame).
pub(crate) fn draw_popup(ctx: &Context, app: &AppHandle) {
    let Some(p) = app.popup() else { return };
    let mut open = true;
    egui::Popup::new(
        popup_id(),
        ctx.clone(),
        p.under,
        egui::LayerId::background(),
    )
    .kind(egui::PopupKind::Menu)
    .open_bool(&mut open)
    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
    .show(|ui| {
        ui.set_min_width(200.0);
        show_with(ui, app, &p.menu, &p.context, &p.args);
    });
    if !open {
        app.set_popup(None);
    }
}

/// Runs the opening step; the arguments to open with, or `None` when a
/// hook vetoed it.
fn open_step(app: &AppHandle, menu: &MenuId, args: Json, how: &str) -> Option<Json> {
    let out = app.step(
        &STEP_OPEN,
        json!({ "menu": menu, "args": args, "how": how }),
        |_, a| Ok(a["args"].clone()),
    );
    match out {
        Outcome::Done(Ok(args)) => Some(args),
        Outcome::Done(Err(_)) | Outcome::Vetoed { .. } => None,
    }
}

/// A context with arguments as `args.<key>` keys (scalars only).
fn with_args(cx: &WhenContext, args: &Json) -> WhenContext {
    let mut cx = cx.clone();
    if let Json::Object(m) = args {
        for (k, v) in m {
            let text = match v {
                Json::String(s) => s.clone(),
                Json::Bool(_) | Json::Number(_) => v.to_string(),
                _ => continue,
            };
            cx.set(&format!("args.{k}"), &text);
        }
    }
    cx
}

/// An item's arguments with a call's merged in (the call's win).
fn merge(item: &Json, extra: &Json) -> Json {
    match (item, extra) {
        (_, Json::Null) => item.clone(),
        (Json::Null, _) => extra.clone(),
        (Json::Object(a), Json::Object(b)) => {
            let mut m: Map<String, Json> = a.clone();
            m.extend(b.iter().map(|(k, v)| (k.clone(), v.clone())));
            Json::Object(m)
        }
        _ => item.clone(),
    }
}

fn items(ui: &mut Ui, app: &AppHandle, menu: &MenuId, cx: &WhenContext, args: &Json, depth: u32) {
    if depth > MAX_DEPTH {
        ui.weak(strings::tr_args(
            ui.ctx(),
            &txt::MENU_TOO_DEEP,
            &[("menu", menu.as_str())],
        ));
        return;
    }
    let list = app.menu(menu);
    if list.is_empty() {
        ui.weak(strings::tr(ui.ctx(), &txt::MENU_EMPTY));
        return;
    }
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
                Item::Command {
                    id,
                    args: own,
                    label,
                } => {
                    let row = Row {
                        menu,
                        id: &id,
                        label,
                        icon: None,
                        enabled,
                        checked,
                        checkable,
                    };
                    if command_row(ui, app, cx, row) {
                        app.request_run_in(id.clone(), merge(&own, args), cx.clone());
                        ui.close();
                    }
                }
                Item::Submenu { menu: sub, label } => {
                    let label = strings::menu_label(ui.ctx(), sub.as_str(), &label);
                    ui.add_enabled_ui(enabled, |ui| {
                        submenu(ui, app, &sub, &label, cx, args, depth)
                    });
                }
                Item::List { name } => {
                    for entry in list_entries(ui, app, menu, &name, cx) {
                        let row = Row {
                            menu,
                            id: &entry.command,
                            label: Some(entry.label.clone()),
                            icon: entry.icon,
                            enabled: enabled && entry.enabled,
                            checked: entry.checked,
                            checkable: entry.checked,
                        };
                        if command_row(ui, app, cx, row) {
                            app.request_run_in(
                                entry.command.clone(),
                                merge(&entry.args, args),
                                cx.clone(),
                            );
                            ui.close();
                        }
                    }
                }
            },
        }
    }
}

/// A list's entries: asked of its provider when the menu opens (the
/// first frame it is drawn), kept while it stays open.
fn list_entries(
    ui: &Ui,
    app: &AppHandle,
    menu: &MenuId,
    name: &str,
    cx: &WhenContext,
) -> Vec<ListItem> {
    let key = LIST.keyed((menu.as_str(), name));
    let pass = ui.ctx().cumulative_pass_nr();
    let kept = key.get(ui.ctx()).filter(|(at, _)| at + 1 >= pass);
    let entries = match kept {
        Some((_, entries)) => entries,
        None => app.list_items(name, cx),
    };
    key.set(ui.ctx(), (pass, entries.clone()));
    entries
}

/// A command row as a menu draws it.
struct Row<'a> {
    menu: &'a MenuId,
    id: &'a CommandId,
    label: Option<String>,
    icon: Option<Icon>,
    enabled: bool,
    checked: bool,
    checkable: bool,
}

/// Draws a command row; whether it was clicked.
fn command_row(ui: &mut Ui, app: &AppHandle, cx: &WhenContext, row: Row<'_>) -> bool {
    let t = Tokens::of(ui.ctx());
    let ctx = ui.ctx().clone();
    let side = t.size.icon_inline;
    let Row {
        menu,
        id,
        label,
        icon,
        enabled,
        checked,
        checkable,
    } = row;
    let spec = app.command(id);
    let (title, why) = match &spec {
        Some(s) => {
            let applies = s.when.is_empty() || When::parse(&s.when).is_ok_and(|w| w.eval(cx));
            let why = (!applies)
                .then(|| strings::tr_args(&ctx, &txt::MENU_DOES_NOT_APPLY, &[("when", &s.when)]))
                .or_else(|| (!enabled).then(|| strings::tr(&ctx, &txt::MENU_NOT_AVAILABLE)));
            (
                label.clone().unwrap_or_else(|| app.command_menu_title(s)),
                why,
            )
        }
        None => (
            id.to_string(),
            Some(strings::tr_args(
                &ctx,
                &txt::MENU_NO_COMMAND,
                &[("id", id.as_str())],
            )),
        ),
    };
    // A run waiting for its result greys the item.
    let why = why.or_else(|| {
        app.running(id)
            .then(|| strings::tr(&ctx, &txt::MENU_RUNNING))
    });
    let failed = app
        .history()
        .iter()
        .rev()
        .find(|r| r.id == *id)
        .and_then(|r| r.result.clone().err());
    let tick = Id::new(("egui_pop_menu_tick", menu.as_str(), id.as_str(), &title));
    let kit = Kit::of(&ctx);
    let item = &kit.menu_item;
    let shortcut = app.keys_for(id).first().cloned();
    // egui lays the item out and takes its clicks; the kit paints its box
    // under it (`slot`), in the state egui's button draws with.
    let slot = ui.painter().add(Shape::Noop);
    let (r, state) = ui
        .add_enabled_ui(why.is_none(), |ui| {
            let state = item_state(ui, ui.next_auto_id());
            let label = item_text(ui, item, state, title.as_str());
            let mut button = Button::new((Atom::custom(tick, vec2(side, side)), label))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE);
            if let Some(k) = shortcut {
                button = button.shortcut_text(
                    RichText::new(k).color(item.style_toned(Tone::Muted, state).text),
                );
            }
            (button.atom_ui(ui), state)
        })
        .inner;
    paint_item(ui, item, slot, r.response.rect, state);
    // A run waiting for the host: a spinner in the icon's place, after
    // the delay.
    let busy = crate::busy::busy_shown(app, &ctx, tick, id);
    if app.running(id) {
        ctx.accesskit_node_builder(r.response.id, |node| node.set_busy());
    }
    if let Some(rect) = r.rect(tick) {
        // The mark keeps the item's resting colour (greyed when disabled).
        let color = item
            .style(State::REST.with(State::DISABLED, why.is_some()))
            .text;
        if busy {
            egui_pop_core::progress::paint_spinner(ui, rect, color);
        } else if checked {
            kit.glyph(Glyph::Check).paint(ui.painter(), rect, color);
        } else if let Some(i) = icon {
            i.paint(ui.painter(), rect, color);
        }
    }
    let mut response = r.response;
    if checkable {
        let (on, name) = (
            why.is_none(),
            label.unwrap_or_else(|| title_of(&ctx, &spec, id)),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, on, checked, &name)
        });
    }
    if let Some(why) = why {
        response = response.on_disabled_hover_text(why);
    } else if let Some(e) = failed {
        response = response.on_hover_text(strings::tr_args(
            &ctx,
            &txt::MENU_LAST_RUN_FAILED,
            &[("error", &e)],
        ));
    } else if let Some(s) = spec.filter(|s| !s.description.is_empty()) {
        response = response.on_hover_text(strings::command_description(
            &ctx,
            s.id.as_str(),
            &s.description,
        ));
    }
    response.clicked()
}

/// The state a menu item's egui button draws with this pass: its
/// response from the last one, as egui's `Button` reads it.
fn item_state(ui: &Ui, id: Id) -> State {
    ui.ctx()
        .read_response(id)
        .map_or(State::REST, |r| State::from_response(&r))
        .with(State::DISABLED, !ui.is_enabled())
}

/// A menu item's label in its state's colour. A disabled item keeps its
/// resting colour: egui greys a disabled item itself.
fn item_text(ui: &Ui, item: &Component, state: State, text: &str) -> RichText {
    let colour = if ui.is_enabled() {
        item.style(state).text
    } else {
        item.style(State::REST).text
    };
    RichText::new(text).color(colour)
}

/// Paints a menu item's box under what egui drew of it.
fn paint_item(ui: &Ui, item: &Component, slot: egui::layers::ShapeIdx, rect: Rect, state: State) {
    item.paint_under(
        ui,
        slot,
        &Part {
            rect,
            room: rect,
            state,
            label: None,
        },
        None,
    );
}

/// A command's title as shown, without undo's label nor the hint of its
/// arguments.
fn title_of(ctx: &Context, spec: &Option<CommandSpec>, id: &CommandId) -> String {
    spec.as_ref().map_or_else(
        || id.to_string(),
        |s| {
            strings::without_argument_hint(&strings::command_title(ctx, s.id.as_str(), &s.title))
                .to_string()
        },
    )
}

/// A submenu button with a painted arrow, opening on hover as egui's
/// `SubMenuButton` does.
fn submenu(
    ui: &mut Ui,
    app: &AppHandle,
    menu: &MenuId,
    label: &str,
    cx: &WhenContext,
    args: &Json,
    depth: u32,
) {
    let t = Tokens::of(ui.ctx());
    let side = t.size.icon_inline;
    let my_id = ui.next_auto_id();
    let open = MenuState::from_ui(ui, |state, _| {
        state.open_item == Some(SubMenu::id_from_widget_id(my_id))
    });
    let space = Id::new(("egui_pop_menu_space", menu.as_str()));
    let arrow = Id::new(("egui_pop_menu_arrow", menu.as_str()));
    let kit = Kit::of(ui.ctx());
    let item = &kit.menu_item;
    let slot = ui.painter().add(Shape::Noop);
    // Selected while its submenu is open.
    let state = item_state(ui, my_id).with(State::SELECTED, open);
    let button = Button::new((
        Atom::custom(space, vec2(side, side)),
        item_text(ui, item, state, label),
    ))
    .right_text(Atom::custom(arrow, vec2(side, side)))
    .fill(Color32::TRANSPARENT)
    .stroke(Stroke::NONE);
    let r = button.atom_ui(ui);
    paint_item(ui, item, slot, r.response.rect, state);
    if let Some(rect) = r.rect(arrow) {
        kit.glyph(Glyph::Submenu).paint(
            ui.painter(),
            rect.shrink(rect.width() * 0.15),
            t.text.muted,
        );
    }
    SubMenu::default()
        .config(MenuConfig::find(ui))
        .show(ui, &r.response, |ui| {
            items(ui, app, menu, cx, args, depth + 1)
        });
}

impl AppHandle {
    /// Asks for a command to run as the user in a given context (a menu
    /// opened there), before the next frame.
    pub fn request_run_in(&self, command: CommandId, args: Json, context: WhenContext) {
        self.push_request(Request::RunIn {
            command,
            args,
            origin: egui_pop_model::commands::Origin::User,
            context,
        });
    }
}
