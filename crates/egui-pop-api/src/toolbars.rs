//! Toolbars drawn from their declarations (`docs/toolbars.md`): a toolbar
//! is a menu id (`demo.toolbar`) whose items are contributed as any
//! menu's (`Ctx::menu_item`, `CommandSpec::menu`, an alter hook), and
//! drawn as a row or a column of buttons instead of a list.
//!
//! - A command item is a button showing the command's icon
//!   ([`CommandSpec::icon`](egui_pop_model::commands::CommandSpec::icon),
//!   its `icon_on` while checked), its title and first key binding in the
//!   tooltip; a toolbar showing its titles ([`Labels`]) writes the title
//!   beside the icon, and a command without an icon shows its title.
//! - Its `checked` clause draws it on (`toolbar-button`'s `selected`
//!   state, framed as a checked menu icon is; announced as toggled, a
//!   radio item as a chosen radio button); where the command or the item
//!   does not apply it is greyed, the reason in its tooltip; a run waiting
//!   for its result shows the spinner in its icon's place.
//! - Groups are separated by a line, as in a menu; a submenu is a button
//!   with a drop-down mark opening that menu; a list item gives a button
//!   per entry (with the entry's own icon), asked of its provider when
//!   the toolbar is first drawn and again when the context changes.
//! - The tools that do not fit go, from the end, into a "More tools"
//!   button ([`Glyph::More`]) opening them as a menu.
//!
//! Sizes come from the theme: the icon is `size.icon.toolbar`, padded by
//! `space.xs`, so the density moves them. What each toolbar drew last is
//! [`layout`] (the `toolbars` debug collector), so an agent reads a
//! toolbar's tools, their state and which went into "More tools" without
//! pixels.
//!
//! ```
//! use egui_pop_api::AppHandle;
//! use egui_pop_api::toolbars::{Labels, Toolbar};
//! use egui_pop_model::ids::MenuId;
//!
//! const TOOLBAR: MenuId = MenuId::from_static("demo.toolbar");
//!
//! fn bar(ui: &mut egui::Ui, app: &AppHandle) {
//!     Toolbar::new(app, &TOOLBAR).show(ui);
//!     Toolbar::new(app, &TOOLBAR).labels(Labels::Beside).vertical().show(ui);
//! }
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;

use egui::containers::menu::MenuConfig;
use egui::{
    Align, Align2, Color32, FontId, Layout, Rect, Response, Sense, Ui, UiKind, UiStackInfo,
    WidgetInfo, WidgetType, pos2, vec2,
};
use egui_pop_core::icon::{Icon, IconRef, Size};
use egui_pop_core::kit::{Glyph, Kit, KitCx, Part, State};
use egui_pop_core::progress::paint_spinner;
use egui_pop_core::state::Slot;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_model::ids::{CommandId, MenuId};
use egui_pop_model::menus::{Item, Shown, show_items};
use egui_pop_model::when::{When, WhenContext};
use serde::Serialize;
use serde_json::Value as Json;

use crate::app::AppHandle;
use crate::menus::{self, ListItem};
use crate::strings as txt;

/// How a toolbar shows its tools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Labels {
    /// Icons alone, the title in the tooltip; a tool without an icon
    /// shows its title.
    #[default]
    Icons,
    /// Each icon with its title beside it.
    Beside,
    /// Titles alone.
    Text,
}

/// Which way a toolbar runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// A row, from the start.
    #[default]
    Horizontal,
    /// A column, from the top (a tool palette).
    Vertical,
}

/// What a tool is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// A button running a command.
    Command,
    /// A button opening a menu.
    Submenu,
    /// The line between two groups.
    Separator,
}

/// A tool as a toolbar drew it last, for agents and tests.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToolTrace {
    pub kind: ToolKind,
    /// The command it runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandId>,
    /// The arguments it runs the command with.
    #[serde(skip_serializing_if = "Json::is_null")]
    pub args: Json,
    /// The menu it opens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submenu: Option<MenuId>,
    /// Its title (its name for screen readers); empty for a separator.
    pub label: String,
    /// The name of the icon it shows, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// The key binding its tooltip gives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<String>,
    pub enabled: bool,
    /// Why it is greyed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// It has a `checked` clause: a toggle (or a radio choice), on or not.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub checkable: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub checked: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub radio: bool,
    /// Its command runs, waiting for its result.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub busy: bool,
    /// It did not fit: it is in the "More tools" menu.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub overflow: bool,
}

/// A toolbar as it was drawn last.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToolbarLayout {
    pub menu: MenuId,
    pub orientation: Orientation,
    pub labels: Labels,
    /// Its name for screen readers ([`Toolbar::name`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Every tool, in order, those in "More tools" marked `overflow`.
    pub tools: Vec<ToolTrace>,
    /// How many tools went into "More tools" (separators not counted).
    pub overflow: usize,
}

/// A toolbar's list entries and the context they were asked in, per
/// toolbar and list.
const LISTS: Slot<(WhenContext, Vec<ListItem>)> = Slot::new("api.toolbars.lists");

/// Where each toolbar drew its tools last, by menu id.
#[derive(Default)]
struct Layouts(RefCell<BTreeMap<String, ToolbarLayout>>);

/// What a toolbar drew last, if it was drawn.
pub fn layout(app: &AppHandle, menu: &MenuId) -> Option<ToolbarLayout> {
    app.local::<Layouts>()
        .0
        .borrow()
        .get(menu.as_str())
        .cloned()
}

/// Every toolbar's last layout, by menu id (the `toolbars` collector).
pub(crate) fn layouts(app: &AppHandle) -> BTreeMap<String, ToolbarLayout> {
    app.local::<Layouts>().0.borrow().clone()
}

/// A toolbar drawn from a menu's items.
#[derive(Debug)]
#[must_use = "draw it with `show`"]
pub struct Toolbar<'a> {
    app: &'a AppHandle,
    menu: MenuId,
    orientation: Orientation,
    labels: Labels,
    args: Json,
    name: Option<String>,
}

impl<'a> Toolbar<'a> {
    /// The toolbar of a menu's items: a row of icons.
    pub fn new(app: &'a AppHandle, menu: &MenuId) -> Self {
        Self {
            app,
            menu: menu.clone(),
            orientation: Orientation::Horizontal,
            labels: Labels::Icons,
            args: Json::Null,
            name: None,
        }
    }

    /// Its name for screen readers (`Formatting`, `Tools`): its "More
    /// tools" button is then `More tools: <name>`, so two toolbars of a
    /// window never have two controls of one name. Give it whenever more
    /// than one toolbar shows at a time.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// A column instead of a row (a tool palette).
    pub fn vertical(mut self) -> Self {
        self.orientation = Orientation::Vertical;
        self
    }

    /// Which way it runs.
    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Icons alone (the default), icons with their titles, or titles.
    pub fn labels(mut self, labels: Labels) -> Self {
        self.labels = labels;
        self
    }

    /// Arguments of this toolbar (the document it acts on): merged into
    /// every command a tool runs, and seen by `when` clauses as
    /// `args.<key>`, as a context menu's.
    pub fn args(mut self, args: Json) -> Self {
        self.args = args;
        self
    }

    /// Draws it in the room the `Ui` has, the tools that do not fit in
    /// "More tools"; returns what it drew. A column measures the height
    /// its `Ui` offers: inside `ui.horizontal` that is one row's, so give
    /// it room (`ui.allocate_ui`, a panel) rather than a row.
    pub fn show(self, ui: &mut Ui) -> ToolbarLayout {
        let app = self.app;
        let ctx = ui.ctx().clone();
        let cx = menus::with_args(&app.when_context(), &self.args);
        let shown = show_items(&app.menu(&self.menu), &cx);
        let mut tools = self.tools(ui, &shown, &cx);
        let t = Tokens::of(&ctx);
        let m = Metrics::of(&t);
        let font = egui::TextStyle::Button.resolve(ui.style());
        let vertical = self.orientation == Orientation::Vertical;
        for tool in &mut tools {
            tool.measure(&ctx, &m, &font, self.labels);
        }
        // A column is as wide as its widest tool.
        let column = tools
            .iter()
            .filter(|t| t.trace.kind != ToolKind::Separator)
            .map(|t| t.width)
            .fold(m.height, f32::max);
        let extents: Vec<f32> = tools
            .iter()
            .map(|t| match (t.trace.kind, vertical) {
                (ToolKind::Separator, _) => m.separator,
                (_, true) => m.height,
                (_, false) => t.width,
            })
            .collect();
        let room = if vertical {
            ui.available_height()
        } else {
            ui.available_width()
        };
        let kept = fit(&extents, m.gap, m.height, room);
        // No separator at the cut, nor leading the overflow.
        let mut shown_until = kept;
        while shown_until > 0 && tools[shown_until - 1].trace.kind == ToolKind::Separator {
            shown_until -= 1;
        }
        let mut overflow: Vec<usize> = (kept..tools.len()).collect();
        while overflow
            .first()
            .is_some_and(|&i| tools[i].trace.kind == ToolKind::Separator)
        {
            overflow.remove(0);
        }
        for &i in &overflow {
            tools[i].trace.overflow = true;
        }
        let layout = if vertical {
            Layout::top_down(Align::Min)
        } else {
            Layout::left_to_right(Align::Center)
        };
        ui.with_layout(layout, |ui| {
            ui.spacing_mut().item_spacing = vec2(m.gap, m.gap);
            for tool in &tools[..shown_until] {
                let size = match (tool.trace.kind, vertical) {
                    (ToolKind::Separator, true) => vec2(column, m.separator),
                    (ToolKind::Separator, false) => vec2(m.separator, m.height),
                    (_, true) => vec2(column, m.height),
                    (_, false) => vec2(tool.width, m.height),
                };
                self.tool(ui, &m, &font, &cx, tool, size);
            }
            if !overflow.is_empty() {
                let rows: Vec<Shown> = overflow.iter().map(|&i| tools[i].row()).collect();
                self.more(ui, &m, &cx, rows, if vertical { column } else { m.height });
            }
        });
        let out = ToolbarLayout {
            menu: self.menu.clone(),
            orientation: self.orientation,
            labels: self.labels,
            name: self.name.clone(),
            overflow: overflow
                .iter()
                .filter(|&&i| tools[i].trace.kind != ToolKind::Separator)
                .count(),
            tools: tools
                .into_iter()
                .enumerate()
                .filter(|(i, _)| *i < shown_until || overflow.contains(i))
                .map(|(_, t)| t.trace)
                .collect(),
        };
        app.local::<Layouts>()
            .0
            .borrow_mut()
            .insert(self.menu.to_string(), out.clone());
        out
    }

    /// The tools of the shown items: a list item gives one per entry.
    fn tools(&self, ui: &Ui, shown: &[Shown], cx: &WhenContext) -> Vec<Tool> {
        let app = self.app;
        let mut out = Vec::new();
        for s in shown {
            match s {
                Shown::Separator => out.push(Tool::separator()),
                Shown::Item {
                    item,
                    enabled,
                    checked,
                    checkable,
                    radio,
                } => match item {
                    Item::Command { id, args, label } => out.push(Tool::command(
                        ui.ctx(),
                        app,
                        cx,
                        CommandTool {
                            id,
                            args: args.clone(),
                            label: label.clone(),
                            own_icon: None,
                            enabled: *enabled,
                            checked: *checked,
                            checkable: *checkable,
                            radio: *radio,
                        },
                    )),
                    Item::Submenu { menu, label } => {
                        let label = strings::menu_label(ui.ctx(), menu.as_str(), label);
                        out.push(Tool::submenu(menu.clone(), label, *enabled));
                    }
                    Item::List { name } => {
                        for e in self.list_entries(ui, name, cx) {
                            out.push(Tool::command(
                                ui.ctx(),
                                app,
                                cx,
                                CommandTool {
                                    id: &e.command,
                                    args: e.args.clone(),
                                    label: Some(e.label.clone()),
                                    own_icon: e.icon,
                                    enabled: *enabled && e.enabled,
                                    checked: e.checked,
                                    checkable: e.checked,
                                    radio: false,
                                },
                            ));
                        }
                    }
                },
            }
        }
        out
    }

    /// A list item's entries: asked of its provider when the toolbar is
    /// first drawn and again whenever the context changes (a toolbar
    /// stays open, unlike a menu).
    fn list_entries(&self, ui: &Ui, name: &str, cx: &WhenContext) -> Vec<ListItem> {
        let key = LISTS.keyed((self.menu.as_str(), name));
        if let Some((at, entries)) = key.get(ui.ctx())
            && at == *cx
        {
            return entries;
        }
        let entries = self.app.list_items(name, cx);
        key.set(ui.ctx(), (cx.clone(), entries.clone()));
        entries
    }

    /// Draws one tool in a rectangle of `size`.
    fn tool(
        &self,
        ui: &mut Ui,
        m: &Metrics,
        font: &FontId,
        cx: &WhenContext,
        tool: &Tool,
        size: egui::Vec2,
    ) {
        let ctx = ui.ctx().clone();
        let tr = &tool.trace;
        if tr.kind == ToolKind::Separator {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let stroke = egui::Stroke::new(1.0, Tokens::of(&ctx).border.default);
            let line = if size.x > size.y {
                [rect.left_center(), rect.right_center()]
            } else {
                [rect.center_top(), rect.center_bottom()]
            };
            ui.painter().line_segment(line, stroke);
            return;
        }
        let response = ui
            .add_enabled_ui(tr.enabled, |ui| {
                ui.allocate_exact_size(size, Sense::click()).1
            })
            .inner;
        let state = State::from_response(&response)
            .with(State::SELECTED, tr.checked)
            .with(State::DISABLED, !tr.enabled);
        self.paint(ui, m, font, tool, response.rect, state);
        let name = tr.label.clone();
        let enabled = tr.enabled;
        match (tr.kind, tr.checkable, tr.radio) {
            (ToolKind::Command, true, true) => {
                let checked = tr.checked;
                response.widget_info(|| {
                    WidgetInfo::selected(WidgetType::RadioButton, enabled, checked, &name)
                });
            }
            (ToolKind::Command, true, false) => {
                let checked = tr.checked;
                response.widget_info(|| {
                    WidgetInfo::selected(WidgetType::Button, enabled, checked, &name)
                });
            }
            _ => response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, &name)),
        }
        if tr.busy {
            ctx.accesskit_node_builder(response.id, |node| node.set_busy());
        }
        let tip = match &tr.keys {
            Some(keys) => strings::tr_args(
                &ctx,
                &txt::TOOLBAR_WITH_KEY,
                &[("action", &tr.label), ("keys", keys)],
            ),
            None => tr.label.clone(),
        };
        let response = match (&tr.why, &tool.failed) {
            (Some(why), _) => response.on_disabled_hover_text(format!("{tip}\n{why}")),
            (None, Some(e)) => response.on_hover_text(format!(
                "{tip}\n{}",
                strings::tr_args(&ctx, &txt::MENU_LAST_RUN_FAILED, &[("error", e)])
            )),
            (None, None) => response.on_hover_text(tip),
        };
        match tr.kind {
            ToolKind::Command => {
                if response.clicked()
                    && let Some(id) = &tr.command
                {
                    self.app.request_run_in(
                        id.clone(),
                        menus::merge(&tr.args, &self.args),
                        cx.clone(),
                    );
                }
            }
            ToolKind::Submenu => {
                if let Some(sub) = &tr.submenu {
                    self.popup(&response, cx, |ui, app, cx, args| {
                        let shown = show_items(&app.menu(sub), cx);
                        menus::rows(ui, app, sub, shown, cx, args, 1);
                    });
                }
            }
            ToolKind::Separator => {}
        }
    }

    /// Paints a tool's box, icon (or spinner), title and drop-down mark.
    fn paint(&self, ui: &Ui, m: &Metrics, font: &FontId, tool: &Tool, rect: Rect, state: State) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        let ctx = ui.ctx();
        let kit = Kit::of(ctx);
        let look = &kit.toolbar_button;
        let painter = ui.painter();
        look.paint(
            &KitCx {
                painter,
                ppp: ctx.pixels_per_point(),
            },
            &Part {
                rect,
                room: rect,
                state,
                label: None,
            },
        );
        let color = look.style(state).text;
        let (icon, text) = tool.parts(self.labels);
        let square = |x: f32| {
            Rect::from_min_size(
                pos2(x, rect.center().y - m.icon / 2.0),
                vec2(m.icon, m.icon),
            )
        };
        // Alone, the icon is centred; with a title, both start at the
        // padding.
        let alone = icon.is_some() && !text && tool.trace.kind == ToolKind::Command;
        let mut x = rect.left() + m.pad;
        if let Some(i) = icon {
            let at = if alone {
                Rect::from_center_size(rect.center(), vec2(m.icon, m.icon))
            } else {
                square(x)
            };
            if tool.trace.busy {
                paint_spinner(ui, at, color);
            } else {
                i.paint(painter, at, color);
            }
            x += m.icon + m.inner;
        }
        if text {
            painter.text(
                pos2(x, rect.center().y),
                Align2::LEFT_CENTER,
                &tool.trace.label,
                font.clone(),
                color,
            );
        }
        if tool.trace.kind == ToolKind::Submenu {
            let side = m.icon * 0.6;
            let at = Rect::from_center_size(
                pos2(rect.right() - m.pad - side / 2.0, rect.center().y),
                vec2(side, side),
            );
            kit.glyph(Glyph::Dropdown).paint(painter, at, color);
        }
        if let (Some(name), Some(_)) = (&tool.trace.icon, icon) {
            egui_pop_core::icon::registry::record(ctx, "toolbar", &tool.trace.label, name);
        }
    }

    /// The "More tools" button, opening the tools that did not fit.
    fn more(&self, ui: &mut Ui, m: &Metrics, cx: &WhenContext, rows: Vec<Shown>, width: f32) {
        let ctx = ui.ctx().clone();
        let name = match &self.name {
            Some(toolbar) => strings::tr_args(&ctx, &txt::TOOLBAR_MORE_IN, &[("toolbar", toolbar)]),
            None => strings::tr(&ctx, &txt::TOOLBAR_MORE),
        };
        let (rect, response) = ui.allocate_exact_size(vec2(width, m.height), Sense::click());
        if ui.is_rect_visible(rect) {
            let kit = Kit::of(&ctx);
            let look = &kit.toolbar_button;
            let state = State::from_response(&response);
            look.paint(
                &KitCx {
                    painter: ui.painter(),
                    ppp: ctx.pixels_per_point(),
                },
                &Part {
                    rect,
                    room: rect,
                    state,
                    label: None,
                },
            );
            let at = Rect::from_center_size(rect.center(), vec2(m.icon, m.icon));
            kit.glyph(Glyph::More)
                .paint(ui.painter(), at, look.style(state).text);
        }
        let label = name.clone();
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &label));
        let response = response.on_hover_text(name);
        let menu = self.menu.clone();
        self.popup(&response, cx, move |ui, app, cx, args| {
            menus::rows(ui, app, &menu, rows, cx, args, 1);
        });
    }

    /// A menu popup under a tool, opened by its click, drawn by `rows`.
    fn popup(
        &self,
        response: &Response,
        cx: &WhenContext,
        rows: impl FnOnce(&mut Ui, &AppHandle, &WhenContext, &Json),
    ) {
        let ui_config = MenuConfig::default();
        let app = self.app;
        let args = &self.args;
        egui::Popup::menu(response)
            .close_behavior(ui_config.close_behavior)
            .info(
                UiStackInfo::new(UiKind::Menu)
                    .with_tag_value(MenuConfig::MENU_CONFIG_TAG, ui_config),
            )
            .show(|ui| rows(ui, app, cx, args));
    }
}

/// What a toolbar's tools are sized from.
struct Metrics {
    /// The icon's side (`size.icon.toolbar`).
    icon: f32,
    /// Around the icon and the title (`space.xs`).
    pad: f32,
    /// Between the icon and the title.
    inner: f32,
    /// Between tools.
    gap: f32,
    /// A tool's height (a row's) or width (an icon-only column's).
    height: f32,
    /// A separator's room along the toolbar.
    separator: f32,
}

impl Metrics {
    fn of(t: &Tokens) -> Metrics {
        let t_icon = Size::Toolbar.points(t);
        let pad = t.space.xs;
        Metrics {
            icon: t_icon,
            pad,
            inner: t.space.xs,
            gap: t.space.xs / 2.0,
            height: t_icon + 2.0 * pad,
            separator: t.space.s,
        }
    }
}

/// A command item, as a tool is made of it.
struct CommandTool<'i> {
    id: &'i CommandId,
    args: Json,
    label: Option<String>,
    /// A list entry's own icon: never its command's.
    own_icon: Option<Icon>,
    enabled: bool,
    checked: bool,
    checkable: bool,
    radio: bool,
}

/// A tool, worked out for this pass.
struct Tool {
    trace: ToolTrace,
    /// The icon drawn, resolved.
    icon: Option<Icon>,
    /// Its label as declared (an item's own), for the overflow menu.
    own_label: Option<String>,
    /// The last run's error.
    failed: Option<String>,
    /// Its item's own clause holds (the menu row checks the command's
    /// again).
    item_enabled: bool,
    /// Its width in a row.
    width: f32,
}

impl Tool {
    fn separator() -> Tool {
        Tool {
            trace: ToolTrace {
                kind: ToolKind::Separator,
                command: None,
                args: Json::Null,
                submenu: None,
                label: String::new(),
                icon: None,
                keys: None,
                enabled: true,
                why: None,
                checkable: false,
                checked: false,
                radio: false,
                busy: false,
                overflow: false,
            },
            icon: None,
            own_label: None,
            failed: None,
            item_enabled: true,
            width: 0.0,
        }
    }

    fn submenu(menu: MenuId, label: String, enabled: bool) -> Tool {
        let mut tool = Tool::separator();
        tool.trace.kind = ToolKind::Submenu;
        tool.trace.submenu = Some(menu);
        tool.own_label = Some(label.clone());
        tool.trace.label = label;
        tool.trace.enabled = enabled;
        tool.item_enabled = enabled;
        tool
    }

    fn command(ctx: &egui::Context, app: &AppHandle, cx: &WhenContext, c: CommandTool<'_>) -> Tool {
        let spec = app.command(c.id);
        let (title, why) = match &spec {
            Some(s) => {
                let applies = s.when.is_empty() || When::parse(&s.when).is_ok_and(|w| w.eval(cx));
                let why = (!applies)
                    .then(|| strings::tr_args(ctx, &txt::MENU_DOES_NOT_APPLY, &[("when", &s.when)]))
                    .or_else(|| (!c.enabled).then(|| strings::tr(ctx, &txt::MENU_NOT_AVAILABLE)));
                (
                    c.label.clone().unwrap_or_else(|| app.command_menu_title(s)),
                    why,
                )
            }
            None => (
                c.label.clone().unwrap_or_else(|| c.id.to_string()),
                Some(strings::tr_args(
                    ctx,
                    &txt::MENU_NO_COMMAND,
                    &[("id", c.id.as_str())],
                )),
            ),
        };
        let running = app.running(c.id);
        let why = why.or_else(|| running.then(|| strings::tr(ctx, &txt::MENU_RUNNING)));
        let failed = app
            .history()
            .iter()
            .rev()
            .find(|r| r.id == *c.id)
            .and_then(|r| r.result.clone().err());
        let (icon, name) = match c.own_icon {
            Some(i) => (Some(i), Some(i.name().to_string())),
            None => match spec.as_ref().and_then(|s| s.icon_for(c.checked)) {
                Some(n) => (
                    Some(IconRef::from(n).or_placeholder("toolbar")),
                    Some(n.to_string()),
                ),
                None => (None, None),
            },
        };
        let busy = crate::busy::busy_shown(
            app,
            ctx,
            egui::Id::new(("egui_pop_toolbar_busy", c.id.as_str(), &title)),
            c.id,
        );
        Tool {
            trace: ToolTrace {
                kind: ToolKind::Command,
                command: Some(c.id.clone()),
                args: c.args,
                submenu: None,
                label: title,
                icon: name,
                keys: app.keys_for(c.id).first().cloned(),
                enabled: why.is_none(),
                why,
                checkable: c.checkable,
                checked: c.checked,
                radio: c.radio,
                busy,
                overflow: false,
            },
            icon,
            own_label: c.label,
            failed,
            item_enabled: c.enabled,
            width: 0.0,
        }
    }

    /// What it shows: its icon (if the labels show icons) and whether its
    /// title shows.
    fn parts(&self, labels: Labels) -> (Option<Icon>, bool) {
        let icon = self.icon.filter(|_| labels != Labels::Text);
        let text = labels != Labels::Icons || icon.is_none();
        (icon, text)
    }

    /// Works out its width in a row.
    fn measure(&mut self, ctx: &egui::Context, m: &Metrics, font: &FontId, labels: Labels) {
        if self.trace.kind == ToolKind::Separator {
            return;
        }
        let (icon, text) = self.parts(labels);
        let mut w = 2.0 * m.pad;
        if icon.is_some() {
            w += m.icon;
        }
        if text {
            if icon.is_some() {
                w += m.inner;
            }
            w += ctx
                .fonts_mut(|f| {
                    f.layout_no_wrap(self.trace.label.clone(), font.clone(), Color32::PLACEHOLDER)
                })
                .size()
                .x;
        }
        if self.trace.kind == ToolKind::Submenu {
            w += m.inner + m.icon * 0.6;
        }
        self.width = w.max(m.height);
    }

    /// The tool as a menu row (in "More tools").
    fn row(&self) -> Shown {
        let tr = &self.trace;
        match tr.kind {
            ToolKind::Separator => Shown::Separator,
            ToolKind::Submenu => Shown::Item {
                item: Item::Submenu {
                    menu: tr
                        .submenu
                        .clone()
                        .unwrap_or_else(|| MenuId::from_static("none")),
                    label: self.own_label.clone().unwrap_or_default(),
                },
                enabled: self.item_enabled,
                checked: false,
                checkable: false,
                radio: false,
            },
            ToolKind::Command => Shown::Item {
                item: Item::Command {
                    id: tr
                        .command
                        .clone()
                        .unwrap_or_else(|| CommandId::from_static("none")),
                    args: tr.args.clone(),
                    label: self.own_label.clone(),
                },
                // Its item's clause: the row checks the command's again.
                enabled: self.item_enabled,
                checked: tr.checked,
                checkable: tr.checkable,
                radio: tr.radio,
            },
        }
    }
}

/// How many tools of these extents stay in a toolbar of `room` points,
/// `gap` apart, the rest going into a "More tools" button `more` long:
/// all of them when they fit.
fn fit(extents: &[f32], gap: f32, more: f32, room: f32) -> usize {
    let all: f32 = extents.iter().sum::<f32>() + gap * extents.len().saturating_sub(1) as f32;
    if all <= room {
        return extents.len();
    }
    let mut used = more;
    extents
        .iter()
        .take_while(|e| {
            used += *e + gap;
            used <= room
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::fit;

    #[test]
    fn tools_that_do_not_fit_go_into_more_from_the_end() {
        let e = [10.0, 10.0, 10.0];
        // All fit, no "More tools".
        assert_eq!(fit(&e, 1.0, 10.0, 32.0), 3);
        // One goes: "More tools" takes its room too, and a gap before it.
        assert_eq!(fit(&e, 1.0, 8.0, 31.0), 2);
        assert_eq!(fit(&e, 1.0, 10.0, 31.0), 1);
        assert_eq!(fit(&e, 1.0, 10.0, 21.0), 1);
        // Nothing fits besides it.
        assert_eq!(fit(&e, 1.0, 10.0, 5.0), 0);
    }
}
