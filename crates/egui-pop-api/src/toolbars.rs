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
//!   button ([`Glyph::More`](egui_pop_core::kit::Glyph::More)) opening
//!   them as a menu.
//! - The toolbar is one Tab stop: the arrows along it go from tool to
//!   tool, Home and End to the first and the last, and Tab coming back
//!   lands on the tool focused last.
//!
//! egui_stack's [`ToolRow`] lays the tools
//! out, takes their input and moves the focus; the kit paints them (its
//! `DockPainter`). Sizes come from the theme: the icon is
//! `size.icon.toolbar`, padded by `space.xs`, so the density moves them.
//! What each toolbar drew last is [`layout`] (the `toolbars` debug
//! collector), so an agent reads a toolbar's tools, their state and which
//! went into "More tools" without pixels.
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

use egui::{Align, Layout, Popup, Ui};
use egui_pop_core::icon::{Icon, IconRef, Size};
use egui_pop_core::kit::Kit;
use egui_pop_core::state::Slot;
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_model::ids::{CommandId, MenuId};
use egui_pop_model::menus::{Item, Shown, show_items};
use egui_pop_model::when::{When, WhenContext};
use egui_stack::toolbar::{self as row, ToolDisplay, ToolRow};
use serde::Serialize;
use serde_json::Value as Json;

use crate::app::AppHandle;
use crate::menus::{self, ListItem, Mark, RowTrace};
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
        let tools = self.tools(ui, &shown, &cx);
        let t = Tokens::of(&ctx);
        let icon = Size::Toolbar.points(&t);
        let pad = t.space.xs;
        let kit = Kit::of(&ctx);
        // The More button's names, from the strings.
        let mut labels = egui_stack::Labels::ENGLISH;
        labels.more_tools = strings::tr(&ctx, &txt::TOOLBAR_MORE).into();
        if let Some(toolbar) = &self.name {
            labels.more_tools_in =
                strings::tr_args(&ctx, &txt::TOOLBAR_MORE_IN, &[("toolbar", toolbar)]).into();
        }
        // Unique in the application: the row's memory (its Tab stop) and
        // `laid_out` go by it.
        let name = format!("egui-pop.toolbar:{}:{:?}", self.menu, ui.scope_id());
        let mut bar = ToolRow::new(name)
            .display(match self.labels {
                Labels::Icons => ToolDisplay::Icons,
                Labels::Beside => ToolDisplay::Beside,
                Labels::Text => ToolDisplay::Text,
            })
            .icon_size(icon)
            .padding(pad)
            .spacing(pad / 2.0)
            .painter(&*kit)
            .labels(labels);
        if self.orientation == Orientation::Vertical {
            bar = bar.vertical();
        }
        if let Some(title) = &self.name {
            bar = bar.title(title.clone());
        }
        let vertical = self.orientation == Orientation::Vertical;
        let row_tools = tools
            .iter()
            .enumerate()
            .map(|(i, tool)| self.row_tool(&ctx, i, tool, &cx));
        let layout = if vertical {
            Layout::top_down(Align::Min)
        } else {
            Layout::left_to_right(Align::Center)
        };
        let response = ui
            .with_layout(layout, |ui| {
                // A separator's room along the row.
                ui.spacing_mut().item_spacing.x = t.space.s;
                if vertical {
                    return bar.show(ui, row_tools);
                }
                // A row is centred across the line it is in and takes
                // the line's height, as a widget of `ui.horizontal` does:
                // ToolRow starts at the top of its room, so its room is
                // its own height (the tallest of the icon and the button
                // text, padded), centred in the line.
                let line = ui.available_rect_before_wrap();
                let text = ui.text_style_height(&egui::TextStyle::Button);
                let height = (icon.max(text) + 2.0 * pad).round();
                let room = egui::vec2(ui.available_width(), height);
                let out = ui.allocate_ui(room, |ui| bar.show(ui, row_tools)).inner;
                if line.height().is_finite() {
                    ui.expand_to_include_y(line.top());
                    ui.expand_to_include_y(line.bottom());
                }
                out
            })
            .inner;
        if let Some(tool) = response
            .clicked
            .as_deref()
            .and_then(|id| id.parse::<usize>().ok())
            .and_then(|i| tools.get(i))
            && let Some(id) = &tool.trace.command
        {
            app.request_run_in(
                id.clone(),
                menus::merge(&tool.trace.args, &self.args),
                cx.clone(),
            );
        }
        // The row's tools as drawn, a subsequence of ours, in order.
        let mut traces = Vec::with_capacity(response.laid.tools.len());
        let mut ours = tools.iter().enumerate();
        for laid in &response.laid.tools {
            let sep = laid.kind == row::ToolKind::Separator;
            let found = ours.by_ref().find(|(i, t)| {
                if sep {
                    t.trace.kind == ToolKind::Separator
                } else {
                    laid.id == i.to_string()
                }
            });
            if let Some((_, t)) = found {
                let mut trace = t.trace.clone();
                trace.overflow = laid.overflow;
                traces.push(trace);
            }
        }
        // The More menu open: its rows, for agents, as a menu's.
        let more_open = response
            .laid
            .more
            .is_some_and(|more| Popup::is_id_open(&ctx, more.with("popup")));
        if more_open {
            let gutter = icon + 2.0 * pad;
            for tr in traces
                .iter()
                .filter(|t| t.overflow && t.kind != ToolKind::Separator)
            {
                let mark = match (tr.busy, tr.checked, tr.icon.is_some()) {
                    (true, ..) => Mark::Busy,
                    (false, true, true) => Mark::Framed,
                    (false, true, false) if tr.radio => Mark::Radio,
                    (false, true, false) => Mark::Check,
                    (false, false, true) => Mark::Icon,
                    (false, false, false) => Mark::None,
                };
                menus::record_row(
                    &ctx,
                    RowTrace {
                        menu: self.menu.clone(),
                        label: tr.label.clone(),
                        icon: tr.icon.clone(),
                        mark,
                        label_x: gutter,
                        gutter,
                    },
                );
            }
        }
        let out = ToolbarLayout {
            menu: self.menu.clone(),
            orientation: self.orientation,
            labels: self.labels,
            name: self.name.clone(),
            overflow: response.laid.overflow,
            tools: traces,
        };
        app.local::<Layouts>()
            .0
            .borrow_mut()
            .insert(self.menu.to_string(), out.clone());
        out
    }

    /// One of our tools as the row's, named by its index: its tooltip
    /// (title, key, why it is greyed, the last run's error), its icon
    /// recorded where drawn, a submenu drawing the menu's rows.
    fn row_tool<'t>(
        &'t self,
        ctx: &egui::Context,
        i: usize,
        tool: &'t Tool,
        cx: &'t WhenContext,
    ) -> row::Tool<'t> {
        let tr = &tool.trace;
        let id = i.to_string();
        let label = tr.label.clone();
        let mut out = match tr.kind {
            ToolKind::Separator => return row::Tool::separator(),
            ToolKind::Submenu => {
                let (app, args) = (self.app, &self.args);
                let sub = tr.submenu.clone();
                row::Tool::menu(id, label, move |ui| {
                    if let Some(sub) = sub {
                        let shown = show_items(&app.menu(&sub), cx);
                        menus::rows(ui, app, &sub, shown, cx, args, 1);
                    }
                })
            }
            ToolKind::Command => match (tr.checkable, tr.radio) {
                (true, true) => row::Tool::radio(id, label, tr.checked),
                (true, false) => row::Tool::toggle(id, label, tr.checked),
                _ => row::Tool::button(id, label),
            },
        };
        let tip = match &tr.keys {
            Some(keys) => strings::tr_args(
                ctx,
                &txt::TOOLBAR_WITH_KEY,
                &[("action", &tr.label), ("keys", keys)],
            ),
            None => tr.label.clone(),
        };
        let tip = match (&tr.why, &tool.failed) {
            (Some(why), _) => format!("{tip}\n{why}"),
            (None, Some(e)) => format!(
                "{tip}\n{}",
                strings::tr_args(ctx, &txt::MENU_LAST_RUN_FAILED, &[("error", e)])
            ),
            (None, None) => tip,
        };
        out = out.tooltip(tip).enabled(tr.enabled).busy(tr.busy);
        if let Some(keys) = &tr.keys {
            out = out.shortcut(keys.clone());
        }
        if let Some(icon) = tool.icon {
            let (label, name) = (tr.label.as_str(), tr.icon.as_deref());
            out = out.icon_painter(move |painter, rect, color| {
                icon.paint(painter, rect, color);
                if let Some(name) = name {
                    egui_pop_core::icon::registry::record(painter.ctx(), "toolbar", label, name);
                }
            });
        }
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
    /// The last run's error.
    failed: Option<String>,
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
            failed: None,
        }
    }

    fn submenu(menu: MenuId, label: String, enabled: bool) -> Tool {
        let mut tool = Tool::separator();
        tool.trace.kind = ToolKind::Submenu;
        tool.trace.submenu = Some(menu);
        tool.trace.label = label;
        tool.trace.enabled = enabled;
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
            egui::Id::unique(("egui_pop_toolbar_busy", c.id.as_str(), &title)),
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
            failed,
        }
    }
}
