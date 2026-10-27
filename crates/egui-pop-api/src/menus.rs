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
//! Every row starts with the **gutter** (`docs/icons.md`, section 5): a
//! column of fixed width ([`Kit::menu_gutter`], scaled by the density)
//! holding the command's icon ([`CommandSpec::icon`], or `icon_on` while
//! checked), the tick of a checked item without an icon, the dot of a
//! chosen radio item, or the spinner of a running command; a checked
//! command with an icon shows it framed (`menu-item-gutter`'s `selected`
//! state), as Word does. Labels start after it whether a row has an icon
//! or not, so they align; shortcuts keep a right-aligned column, before
//! the submenu arrows' column. The [`SETTING_ICONS`] setting turns the
//! icons off (the gutter is then kept only in a menu with a check or radio
//! item) or follows the platform. The gutter is decorative: AccessKit
//! reads the title, and a check or radio item as checked or not.
//!
//! A list item (`MenuItem::list`) is filled by the list provider of its
//! name (`Ctx::list_provider`), asked when the menu opens and never on
//! every frame: the recent files, the profiles. Each entry it gives runs
//! a command with its arguments; the `lists` debug collector shows what
//! each provider gave last.
//!
//! - [`bar`] draws a menu as a menu bar (the shell draws `app.main`),
//!   moving the menus that do not fit into a "…" menu;
//! - [`show`] draws a menu's items inside an open menu or popup;
//! - [`context_menu`] opens a declared menu on a widget's secondary click,
//!   and [`context_menu_with`] with arguments of the moment (the clicked
//!   row's path), merged into every item's command and seen by the `when`
//!   clauses as `args.<key>`; not where a widget claimed the secondary
//!   click for itself (`egui_pop_core::pointer::claim`);
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
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use egui::AtomExt as _;
use egui::containers::menu::{MenuButton, MenuConfig, MenuState, SubMenu};
use egui::{
    Atom, Button, Color32, Context, Id, Pos2, Rect, Response, RichText, Shape, Stroke, Ui, vec2,
};
use egui_pop_core::icon::{Icon, IconRef};
use egui_pop_core::state::Slot;
use egui_pop_core::strings;

/// What a context menu opened with (its `when` context and arguments,
/// `None` when vetoed), per widget.
const CONTEXT_MENU: Slot<Option<(WhenContext, Json)>> = Slot::new("api.menus.context");
/// A dynamic list's items and the pass they were listed on, per menu
/// and list.
const LIST: Slot<(u64, Vec<ListItem>)> = Slot::new("api.menus.list");
use egui_pop_core::theme::{ScopedTheme, Tokens};
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, MenuId, OwnerId, SettingId, StepId};
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

/// The setting: whether menus show the commands' icons in their gutter,
/// `on` (the default, Word's look), `off`, or `platform` ([`MenuIcons`]).
pub const SETTING_ICONS: SettingId = SettingId::from_static("menus.icons");

/// What the [`SETTING_ICONS`] setting asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuIcons {
    /// The icons the commands declare, in a gutter every row keeps.
    #[default]
    On,
    /// No icons; ticks and radio dots keep the gutter, reserved only in a
    /// menu that holds a check or radio item.
    Off,
    /// `on` on Windows, macOS and KDE Plasma (following its
    /// `ShowIconsInMenuItems`), `off` on GNOME.
    Platform,
}

impl MenuIcons {
    /// The setting's choices, in order.
    pub const CHOICES: [&'static str; 3] = ["on", "off", "platform"];

    pub fn as_str(self) -> &'static str {
        match self {
            MenuIcons::On => "on",
            MenuIcons::Off => "off",
            MenuIcons::Platform => "platform",
        }
    }

    pub fn parse(s: &str) -> Option<MenuIcons> {
        [MenuIcons::On, MenuIcons::Off, MenuIcons::Platform]
            .into_iter()
            .find(|m| m.as_str() == s)
    }

    /// Whether menus show icons: `platform` asks the desktop (read once).
    pub fn shows(self) -> bool {
        match self {
            MenuIcons::On => true,
            MenuIcons::Off => false,
            MenuIcons::Platform => {
                static PLATFORM: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
                *PLATFORM.get_or_init(|| {
                    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
                    platform_shows_icons(&desktop, || read_config("kdeglobals"))
                })
            }
        }
    }
}

/// Whether the platform shows icons in menus, from the desktop
/// (`XDG_CURRENT_DESKTOP`) and KDE's `kdeglobals`: not on GNOME; on KDE
/// Plasma as its `ShowIconsInMenuItems` (group `KDE`) says, yes when it
/// does not say; yes elsewhere (Windows, macOS, other desktops).
pub fn platform_shows_icons(desktop: &str, kdeglobals: impl FnOnce() -> Option<String>) -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        return true;
    }
    let desktop = desktop.to_uppercase();
    if desktop.split(':').any(|d| d == "KDE") {
        return kdeglobals().and_then(|text| kde_show_icons(&text)) != Some(false);
    }
    !desktop.split(':').any(|d| d == "GNOME")
}

/// `ShowIconsInMenuItems` in the `[KDE]` group of a `kdeglobals` text.
fn kde_show_icons(text: &str) -> Option<bool> {
    let mut group = "";
    let mut found = None;
    for line in text.lines().map(str::trim) {
        if let Some(g) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            group = g;
        } else if group == "KDE"
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "ShowIconsInMenuItems"
        {
            found = match value.trim() {
                "true" => Some(true),
                "false" => Some(false),
                _ => found,
            };
        }
    }
    found
}

/// A file of the user's configuration directory, as text.
fn read_config(name: &str) -> Option<String> {
    let config = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|c| !c.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::Path::new(&h).join(".config"))
        })?;
    std::fs::read_to_string(config.join(name)).ok()
}

/// What the [`SETTING_ICONS`] setting says now.
pub fn menu_icons(app: &AppHandle) -> MenuIcons {
    egui_pop_core::lock(&app.store())
        .get(&format!("settings/{SETTING_ICONS}"))
        .and_then(|e| e.value.as_choice().and_then(MenuIcons::parse))
        .unwrap_or_default()
}

/// An entry a list provider gives when its menu opens.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListItem {
    pub label: String,
    /// Its icon; agents read its name (the `lists` collector).
    #[serde(skip_serializing_if = "Option::is_none")]
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

    /// Its icon, by value or by name (`"history"`, `"phosphor:file-image"`,
    /// `"my-plugin:album"`). A name that does not resolve is reported (the
    /// `icons` collector) and the entry shows no icon.
    pub fn icon(mut self, icon: impl Into<egui_pop_core::icon::IconRef>) -> Self {
        self.icon = icon.into().resolve("menu list item");
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
/// popup. The menus that do not fit go into a "…" menu (see
/// [`bar_within`]).
pub fn bar(ui: &mut Ui, app: &AppHandle, menu: &MenuId) {
    bar_within(ui, app, menu, 0.0);
}

/// Draws a menu as a bar of menu buttons, keeping `reserve` points free
/// at its end (what the bar shows after its menus).
///
/// The menus that fit stay in the bar, from the start; the others become
/// submenus of a "…" entry ([`Icon::More`]), with their titles and their
/// `when` clauses. When no menu fits besides it, the whole menu is one
/// entry ([`Icon::Menu`]). Each label is measured once, again only when
/// the language, the kit or the scale changes.
///
/// Each menu has an access key: the first letter of its label not taken
/// by a menu before it. Alt and that letter open the menu, in the bar or
/// inside "…"; a key binding on the same keys wins. Where each menu went,
/// and its access key, is [`bar_layout`] (the `menu_bar` debug
/// collector).
pub fn bar_within(ui: &mut Ui, app: &AppHandle, menu: &MenuId, reserve: f32) {
    let cx = app.when_context();
    let ctx = ui.ctx().clone();
    let shown: Vec<(MenuId, String, bool)> = show_items(&app.menu(menu), &cx)
        .into_iter()
        .filter_map(|shown| match shown {
            Shown::Item {
                item: Item::Submenu { menu: sub, label },
                enabled,
                ..
            } => Some((sub, label, enabled)),
            _ => None,
        })
        .collect();
    let font = egui::TextStyle::Button.resolve(ui.style());
    let caches = app.local::<BarCaches>();
    let mut caches = caches.0.borrow_mut();
    let cache = caches.entry(menu.clone()).or_default();
    cache.prepare(&ctx, font, &shown);
    let pad = ui.spacing().button_padding.x * 2.0;
    let gap = ui.spacing().item_spacing.x;
    let side = Tokens::of(&ctx).size.icon_inline;
    let widths: Vec<f32> = cache.menus.iter().map(|m| m.width + pad).collect();
    let available = ui.available_width();
    let kept = fit(&widths, gap, side + pad, available - reserve);
    cache.fitted = Some(Fitted {
        widths,
        gap,
        more: side + pad,
        available,
        kept,
    });
    // Alt and an access key: the menu it opens, if enabled.
    let pressed = ui.input_mut(|i| {
        cache.menus.iter().position(|m| {
            m.enabled
                && m.key
                    .and_then(|k| egui::Key::from_name(&k.to_ascii_uppercase().to_string()))
                    .is_some_and(|k| i.consume_key(egui::Modifiers::ALT, k))
        })
    });
    for (i, m) in cache.menus[..kept].iter().enumerate() {
        ui.add_enabled_ui(m.enabled, |ui| {
            let id = ui.next_auto_id();
            if pressed == Some(i) {
                egui::Popup::open_id(&ctx, id.with("popup"));
            }
            let (r, _) = MenuButton::new(m.label.as_str())
                .ui(ui, |ui| items(ui, app, &m.menu, &cx, &Json::Null, 0));
            access_key(&ctx, r.id, m.key);
        });
    }
    let hidden = &cache.menus[kept..];
    if !hidden.is_empty() {
        let (icon, name) = if kept == 0 {
            (Icon::Menu, cache.main_name.clone())
        } else {
            (Icon::More, cache.more_name.clone())
        };
        let open = pressed.and_then(|i| i.checked_sub(kept));
        let id = ui.next_auto_id();
        if open.is_some() {
            egui::Popup::open_id(&ctx, id.with("popup"));
        }
        let mark = Id::new(("egui_pop_menu_more", menu.as_str()));
        let button = Button::new(Atom::custom(mark, vec2(side, side)));
        let (r, _) = MenuButton::from_button(button).ui(ui, |ui| {
            for (i, m) in hidden.iter().enumerate() {
                ui.add_enabled_ui(m.enabled, |ui| {
                    let id = ui.next_auto_id();
                    if open == Some(i) {
                        // Opened by its access key: its submenu shows at once.
                        let sub = SubMenu::id_from_widget_id(id);
                        MenuState::mark_shown(ui.ctx(), sub);
                        MenuState::from_ui(ui, |state, _| state.open_item = Some(sub));
                    }
                    submenu(ui, app, &m.menu, &m.label, &cx, &Json::Null, 0, &TITLES);
                    access_key(ui.ctx(), id, m.key);
                });
            }
        });
        let rect = Rect::from_center_size(r.rect.center(), vec2(side, side));
        icon.paint(ui.painter(), rect, ui.style().interact(&r).text_color());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
        r.on_hover_text(name);
    }
    let layout = BarLayout {
        bar: cache.menus[..kept].iter().map(|m| m.menu.clone()).collect(),
        overflow: hidden.iter().map(|m| m.menu.clone()).collect(),
        collapsed: kept == 0 && !hidden.is_empty(),
        keys: cache
            .menus
            .iter()
            .filter_map(|m| Some((m.menu.to_string(), m.key?.to_string())))
            .collect(),
    };
    drop(caches);
    app.local::<BarLayouts>()
        .0
        .borrow_mut()
        .insert(menu.to_string(), layout);
}

/// Where a menu bar drew its menus last.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct BarLayout {
    /// The menus in the bar, in order.
    pub bar: Vec<MenuId>,
    /// The menus inside "…" (or the single entry), in order.
    pub overflow: Vec<MenuId>,
    /// No menu fit besides "…": the whole menu is one entry.
    pub collapsed: bool,
    /// Each menu's access key (Alt and the letter), by menu id.
    pub keys: BTreeMap<String, String>,
}

/// Where a menu drawn as a bar ([`bar`], the shell's `app.main`) put its
/// menus last, if it was drawn.
pub fn bar_layout(app: &AppHandle, menu: &MenuId) -> Option<BarLayout> {
    app.local::<BarLayouts>()
        .0
        .borrow()
        .get(menu.as_str())
        .cloned()
}

/// Every menu bar's last layout, by menu id (the `menu_bar` collector).
pub(crate) fn bar_layouts(app: &AppHandle) -> BTreeMap<String, BarLayout> {
    app.local::<BarLayouts>().0.borrow().clone()
}

#[derive(Default)]
struct BarLayouts(RefCell<BTreeMap<String, BarLayout>>);

#[derive(Default)]
struct BarCaches(RefCell<HashMap<MenuId, BarCache>>);

/// A menu bar's menus, translated and measured: laid out again only when
/// their labels, the kit, the scale or the strings provider change.
#[derive(Default)]
struct BarCache {
    /// The kit the labels were measured for, kept: compared by address,
    /// which a kit dropped could otherwise lend to the next one.
    kit: Option<Arc<Kit>>,
    /// The pixels per point, the strings provider and the font the labels
    /// were measured for.
    key: Option<(u32, Option<usize>, egui::FontId)>,
    menus: Vec<BarMenu>,
    /// The names of "…" and of the single entry.
    more_name: String,
    main_name: String,
    /// The last layout's measures.
    fitted: Option<Fitted>,
}

/// What a bar's last layout was made from.
struct Fitted {
    widths: Vec<f32>,
    gap: f32,
    more: f32,
    available: f32,
    kept: usize,
}

struct BarMenu {
    menu: MenuId,
    /// The label as declared: translated again only when it changes.
    english: String,
    label: String,
    enabled: bool,
    /// The label's width, without the button's padding.
    width: f32,
    key: Option<char>,
}

impl BarCache {
    fn prepare(&mut self, ctx: &Context, font: egui::FontId, shown: &[(MenuId, String, bool)]) {
        let kit = Kit::of(ctx);
        let key = (
            ctx.pixels_per_point().to_bits(),
            strings::provider_id(ctx),
            font.clone(),
        );
        let same = self.key.as_ref() == Some(&key)
            && self.kit.as_ref().is_some_and(|k| Arc::ptr_eq(k, &kit))
            && self.menus.len() == shown.len()
            && self
                .menus
                .iter()
                .zip(shown)
                .all(|(m, (id, english, _))| m.menu == *id && m.english == *english);
        if same {
            for (m, (_, _, enabled)) in self.menus.iter_mut().zip(shown) {
                m.enabled = *enabled;
            }
            return;
        }
        self.key = Some(key);
        self.kit = Some(kit);
        // Asked with the labels, so that they all come in the same pass.
        self.more_name = strings::tr(ctx, &txt::MENU_MORE);
        self.main_name = strings::tr(ctx, &txt::MENU_MAIN);
        let mut taken = Vec::new();
        self.menus = shown
            .iter()
            .map(|(menu, english, enabled)| {
                let label = strings::menu_label(ctx, menu.as_str(), english);
                let width = ctx
                    .fonts_mut(|f| {
                        f.layout_no_wrap(label.clone(), font.clone(), Color32::PLACEHOLDER)
                    })
                    .size()
                    .x;
                let key = access_letter(&label, &taken);
                taken.extend(key);
                BarMenu {
                    menu: menu.clone(),
                    english: english.clone(),
                    label,
                    enabled: *enabled,
                    width,
                    key,
                }
            })
            .collect();
    }
}

/// Whether a bar drawn last pass would keep another number of menus
/// with `reserve` points kept free at its end.
pub(crate) fn refit_needed(app: &AppHandle, menu: &MenuId, reserve: f32) -> bool {
    let caches = app.local::<BarCaches>();
    let caches = caches.0.borrow();
    caches
        .get(menu)
        .and_then(|c| c.fitted.as_ref())
        .is_some_and(|f| fit(&f.widths, f.gap, f.more, f.available - reserve) != f.kept)
}

/// A label's access key: its first letter or digit not taken, lower case.
fn access_letter(label: &str, taken: &[char]) -> Option<char> {
    label
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .find(|c| !taken.contains(c))
}

/// Gives a widget its access key in the accessibility tree.
fn access_key(ctx: &Context, id: Id, key: Option<char>) {
    if let Some(k) = key {
        ctx.accesskit_node_builder(id, |node| node.set_access_key(k.to_string()));
    }
}

/// How many menus of these widths stay in a bar of `room` points, items
/// `gap` apart, the rest going into a "…" entry `more` wide: all of them
/// when they fit, 0 when none fits besides "…".
fn fit(widths: &[f32], gap: f32, more: f32, room: f32) -> usize {
    let all: f32 = widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;
    if all <= room {
        return widths.len();
    }
    let mut used = more;
    widths
        .iter()
        .take_while(|w| {
            used += *w + gap;
            used <= room
        })
        .count()
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
/// opened, and their commands run in it. It does not open where a widget
/// inside claimed the secondary click (`egui_pop_core::pointer::claim`).
pub fn context_menu(response: &Response, app: &AppHandle, menu: &MenuId) {
    context_menu_with(response, app, menu, Json::Null);
}

/// As [`context_menu`], with arguments of this call (the clicked row's
/// path): merged into every item's command arguments, and seen by `when`
/// clauses as `args.<key>`.
pub fn context_menu_with(response: &Response, app: &AppHandle, menu: &MenuId, args: Json) {
    let key = CONTEXT_MENU.keyed(response.id);
    if egui_pop_core::pointer::claim::opens_context_menu(response) {
        let opened = open_step(app, menu, args, "context");
        let state = opened.map(|args| (with_args(&app.when_context(), &args), args));
        key.set(&response.ctx, state);
    }
    egui_pop_core::pointer::claim::context_menu(response, |ui| {
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
    /// The scoped theme it was opened in (a theme editor's preview): it
    /// is drawn in it, though drawn later, out of the scope.
    pub scope: Option<ScopedTheme>,
    /// The window it opens in: the active one when it opened.
    pub window: egui::ViewportId,
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
/// pressed. One popup at a time: another replaces it. It opens in the
/// window the user works in ([`AppHandle::active_window`]: a detached
/// panel's window when it was opened from there), the rectangle in that
/// window's points, and goes to the main window if that window closes.
/// Opened while a [`ScopedTheme`] draws (a theme editor's preview), it
/// is drawn in that theme. Returns whether it opened (a hook on
/// [`STEP_OPEN`] may veto it).
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
        scope: ScopedTheme::current(&app.egui()),
        window: app.active_window(),
    };
    app.set_popup(Some(popup));
    app.egui().request_repaint();
    true
}

/// The menu open as a popup, if one is.
pub fn popup_open(app: &AppHandle) -> Option<MenuId> {
    app.popup().map(|p| p.menu)
}

/// The window the popup menu shows in, if one is open: the one it was
/// opened in, or the main window once that one is closed.
pub fn popup_window(app: &AppHandle) -> Option<egui::ViewportId> {
    app.popup().map(|p| app.open_window(p.window))
}

/// Closes the popup menu, if one is open.
pub fn close_popup(app: &AppHandle) {
    app.set_popup(None);
}

/// Draws the popup menu while it is open, in the window it belongs to
/// (the shell calls it in each window's pass).
pub(crate) fn draw_popup(ctx: &Context, app: &AppHandle) {
    let Some(p) = app.popup() else { return };
    if app.open_window(p.window) != ctx.viewport_id() {
        return;
    }
    let draw = || {
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
        open
    };
    let open = match &p.scope {
        Some(scope) => scope.within(ctx, draw),
        None => draw(),
    };
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
    let shown = show_items(&list, cx);
    // The lists' entries first: the columns depend on them.
    let lists: HashMap<String, Vec<ListItem>> = shown
        .iter()
        .filter_map(|s| match s {
            Shown::Item {
                item: Item::List { name },
                ..
            } => Some((name.clone(), list_entries(ui, app, menu, name, cx))),
            _ => None,
        })
        .collect();
    let columns = Columns::of(ui.ctx(), app, &shown, &lists);
    for shown in shown {
        match shown {
            Shown::Separator => {
                ui.separator();
            }
            Shown::Item {
                item,
                enabled,
                checked,
                checkable,
                radio,
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
                        icon: RowIcon::Command,
                        enabled,
                        checked,
                        checkable,
                        radio,
                    };
                    if command_row(ui, app, cx, &columns, row) {
                        app.request_run_in(id.clone(), merge(&own, args), cx.clone());
                        ui.close();
                    }
                }
                Item::Submenu { menu: sub, label } => {
                    let label = strings::menu_label(ui.ctx(), sub.as_str(), &label);
                    let label_x = ui
                        .add_enabled_ui(enabled, |ui| {
                            submenu(ui, app, &sub, &label, cx, args, depth, &columns)
                        })
                        .inner;
                    record_row(
                        ui.ctx(),
                        RowTrace {
                            menu: menu.clone(),
                            label,
                            icon: None,
                            mark: Mark::None,
                            label_x,
                            gutter: columns.gutter,
                        },
                    );
                }
                Item::List { name } => {
                    for entry in lists.get(&name).cloned().unwrap_or_default() {
                        let row = Row {
                            menu,
                            id: &entry.command,
                            label: Some(entry.label.clone()),
                            icon: RowIcon::Own(entry.icon),
                            enabled: enabled && entry.enabled,
                            checked: entry.checked,
                            checkable: entry.checked,
                            radio: false,
                        };
                        if command_row(ui, app, cx, &columns, row) {
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

/// How the rows of one menu line up: worked out once for the whole menu,
/// so every row starts its label at the same x.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Columns {
    /// The gutter's width; zero when the menu keeps none.
    gutter: f32,
    /// The commands' icons are shown.
    icons: bool,
    /// A column at the end for submenu arrows (blank on other rows).
    arrow: bool,
}

impl Columns {
    fn of(
        ctx: &Context,
        app: &AppHandle,
        shown: &[Shown],
        lists: &HashMap<String, Vec<ListItem>>,
    ) -> Columns {
        let icons = menu_icons(app).shows();
        let mut marks = false;
        let mut arrow = false;
        for s in shown {
            let Shown::Item {
                item, checkable, ..
            } = s
            else {
                continue;
            };
            match item {
                Item::Command { id, .. } => marks |= *checkable || app.running(id),
                Item::Submenu { .. } => arrow = true,
                Item::List { name } => {
                    marks |= lists
                        .get(name)
                        .is_some_and(|l| l.iter().any(|e| e.checked || app.running(&e.command)));
                }
            }
        }
        let t = Tokens::of(ctx);
        let gutter = if icons || marks {
            Kit::of(ctx).menu_gutter(&t)
        } else {
            0.0
        };
        Columns {
            gutter,
            icons,
            arrow,
        }
    }

    /// The gutter's atom, first in a row; none when the menu keeps no
    /// gutter.
    fn gutter_atom(&self, id: Id, side: f32) -> Option<Atom<'static>> {
        (self.gutter > 0.0).then(|| Atom::custom(id, vec2(self.gutter, side)))
    }
}

/// The overflow's rows: the menu bar's titles, without a gutter.
const TITLES: Columns = Columns {
    gutter: 0.0,
    icons: false,
    arrow: true,
};

/// Where a row's icon comes from.
#[derive(Clone, Copy, Debug)]
enum RowIcon {
    /// The command's (`icon`, `icon_on` while checked).
    Command,
    /// Its own (a list entry's): never its command's.
    Own(Option<Icon>),
}

/// What a row's gutter holds, as agents read it ([`rows_drawn`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    /// Nothing: no icon, not checked.
    None,
    /// The command's icon.
    Icon,
    /// The icon, framed: a checked command with an icon.
    Framed,
    /// A tick: a checked item without an icon.
    Check,
    /// A dot: the chosen item of a radio group, without an icon.
    Radio,
    /// The spinner of a running command.
    Busy,
}

/// A menu row as it was drawn, for agents and tests.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RowTrace {
    /// The menu it is in.
    pub menu: MenuId,
    /// Its label.
    pub label: String,
    /// The name of the icon it shows, if any (framed or not).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub mark: Mark,
    /// Where its label starts, in points from the row's left edge.
    pub label_x: f32,
    /// The gutter's width (zero when the menu keeps none).
    pub gutter: f32,
}

/// The rows drawn this pass and the last complete pass's.
const ROWS: Slot<(u64, Vec<RowTrace>, Vec<RowTrace>)> = Slot::new("api.menus.rows");

/// The most rows kept per pass.
const ROWS_MAX: usize = 512;

fn record_row(ctx: &Context, row: RowTrace) {
    let now = ctx.cumulative_pass_nr();
    ROWS.update(ctx, |(pass, building, last)| {
        if *pass != now {
            *last = if *pass + 1 == now {
                std::mem::take(building)
            } else {
                Vec::new()
            };
            building.clear();
            *pass = now;
        }
        if building.len() < ROWS_MAX {
            building.push(row);
        }
    });
}

/// The menu rows drawn in the last complete pass (every open menu's,
/// and menus drawn in place): their labels, icons, gutter marks and
/// where their labels start, so an agent or a test reads what a menu
/// shows without pixels.
pub fn rows_drawn(ctx: &Context) -> Vec<RowTrace> {
    let now = ctx.cumulative_pass_nr();
    ROWS.update(ctx, |(pass, building, last)| {
        if *pass + 1 == now {
            building.clone()
        } else if *pass == now {
            last.clone()
        } else {
            Vec::new()
        }
    })
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
    icon: RowIcon,
    enabled: bool,
    checked: bool,
    checkable: bool,
    radio: bool,
}

/// Draws a command row; whether it was clicked.
fn command_row(
    ui: &mut Ui,
    app: &AppHandle,
    cx: &WhenContext,
    columns: &Columns,
    row: Row<'_>,
) -> bool {
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
        radio,
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
    // The icon shown: the command's (its "on" one while checked), or a
    // list entry's own; none when the setting hides icons.
    let icon: Option<(Icon, String)> = if columns.icons {
        match icon {
            RowIcon::Command => spec
                .as_ref()
                .and_then(|s| s.icon_for(checked))
                .map(|name| (IconRef::from(name).or_placeholder("menu"), name.to_string())),
            RowIcon::Own(i) => i.map(|i| (i, i.name().to_string())),
        }
    } else {
        None
    };
    let tick = Id::new(("egui_pop_menu_tick", menu.as_str(), id.as_str(), &title));
    let text_id = tick.with("label");
    let blank = tick.with("arrow");
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
            let mut atoms: Vec<Atom<'_>> = Vec::with_capacity(5);
            atoms.extend(columns.gutter_atom(tick, side));
            atoms.push(label.atom_id(text_id));
            if shortcut.is_some() || columns.arrow {
                atoms.push(Atom::grow());
            }
            if let Some(k) = shortcut {
                atoms.push(
                    RichText::new(k)
                        .color(item.style_toned(Tone::Muted, state).text)
                        .into(),
                );
            }
            if columns.arrow {
                atoms.push(Atom::custom(blank, vec2(side, side)));
            }
            let button = Button::new(egui::Atoms::from(atoms))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE);
            (button.atom_ui(ui), state)
        })
        .inner;
    paint_item(ui, item, slot, r.response.rect, state);
    // A run waiting for the host: a spinner in the gutter, after the
    // delay.
    let busy = crate::busy::busy_shown(app, &ctx, tick, id);
    if app.running(id) {
        ctx.accesskit_node_builder(r.response.id, |node| node.set_busy());
    }
    let mark = match (busy, checked, &icon) {
        (true, ..) => Mark::Busy,
        (false, true, Some(_)) => Mark::Framed,
        (false, true, None) if radio => Mark::Radio,
        (false, true, None) => Mark::Check,
        (false, false, Some(_)) => Mark::Icon,
        (false, false, None) => Mark::None,
    };
    if let Some(rect) = r.rect(tick) {
        // The mark keeps the item's resting colour (greyed when
        // disabled), never a tone: high contrast keeps the text's.
        let disabled = why.is_some();
        let color = item.style(State::REST.with(State::DISABLED, disabled)).text;
        let square = Rect::from_center_size(rect.center(), vec2(side, side));
        let painter = ui.painter();
        if mark == Mark::Framed {
            let frame = Rect::from_center_size(
                rect.center(),
                vec2(rect.width(), rect.width().min(r.response.rect.height())),
            );
            kit.menu_item_gutter.paint(
                &egui_pop_core::kit::KitCx {
                    painter,
                    ppp: ctx.pixels_per_point(),
                },
                &Part {
                    rect: frame,
                    room: frame,
                    state: State::SELECTED.with(State::DISABLED, disabled),
                    label: None,
                },
            );
        }
        match mark {
            Mark::Busy => egui_pop_core::progress::paint_spinner(ui, square, color),
            Mark::Check => kit.glyph(Glyph::Check).paint(painter, square, color),
            Mark::Radio => kit.glyph(Glyph::Radio).paint(painter, square, color),
            Mark::Icon | Mark::Framed => {
                if let Some((i, _)) = &icon {
                    i.paint(painter, square, color);
                }
            }
            Mark::None => {}
        }
    }
    if let Some((_, name)) = &icon {
        egui_pop_core::icon::registry::record(&ctx, "menu", &title, name);
    }
    record_row(
        &ctx,
        RowTrace {
            menu: menu.clone(),
            label: title.clone(),
            icon: icon.as_ref().map(|(_, n)| n.clone()),
            mark,
            label_x: r
                .rect(text_id)
                .map_or(0.0, |l| l.left() - r.response.rect.left()),
            gutter: columns.gutter,
        },
    );
    let mut response = r.response;
    if checkable {
        let (on, name) = (
            why.is_none(),
            label.unwrap_or_else(|| title_of(&ctx, &spec, id)),
        );
        let kind = if radio {
            egui::WidgetType::RadioButton
        } else {
            egui::WidgetType::Checkbox
        };
        response.widget_info(|| egui::WidgetInfo::selected(kind, on, checked, &name));
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

/// A submenu button with a painted arrow in the menu's last column,
/// opening on hover as egui's `SubMenuButton` does; where its label
/// starts in the row.
#[allow(clippy::too_many_arguments)]
fn submenu(
    ui: &mut Ui,
    app: &AppHandle,
    menu: &MenuId,
    label: &str,
    cx: &WhenContext,
    args: &Json,
    depth: u32,
    columns: &Columns,
) -> f32 {
    let t = Tokens::of(ui.ctx());
    let side = t.size.icon_inline;
    let my_id = ui.next_auto_id();
    let open = MenuState::from_ui(ui, |state, _| {
        state.open_item == Some(SubMenu::id_from_widget_id(my_id))
    });
    let space = Id::new(("egui_pop_menu_space", menu.as_str()));
    let arrow = Id::new(("egui_pop_menu_arrow", menu.as_str()));
    let text_id = space.with("label");
    let kit = Kit::of(ui.ctx());
    let item = &kit.menu_item;
    let slot = ui.painter().add(Shape::Noop);
    // Selected while its submenu is open.
    let state = item_state(ui, my_id).with(State::SELECTED, open);
    let mut atoms: Vec<Atom<'_>> = Vec::with_capacity(4);
    atoms.extend(columns.gutter_atom(space, side));
    atoms.push(item_text(ui, item, state, label).atom_id(text_id));
    atoms.push(Atom::grow());
    atoms.push(Atom::custom(arrow, vec2(side, side)));
    let button = Button::new(egui::Atoms::from(atoms))
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
    r.rect(text_id)
        .map_or(0.0, |l| l.left() - r.response.rect.left())
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
