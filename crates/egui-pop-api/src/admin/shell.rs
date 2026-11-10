//! The administration app's shell: the full page (an overlay of the main
//! window, or a window of its own), the scope tabs and search on top, the
//! sections on the left, the page generated from the section's nodes.
//! Every brick is one of egui-pop's widgets: selectable rows, the
//! settings' editors (`param`), combo boxes, indicators, buttons.

use egui::accesskit::Role;
use egui::{Align, Key, Modifiers, Ui};
use egui_pop_core::a11y;
use egui_pop_core::indicator::Indicator;
use egui_pop_core::kit::Tone as KitTone;
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Kind, OverlaySpec};
use egui_pop_core::param::{self, Mode};
use egui_pop_core::strings::{self, RESET};
use egui_pop_core::tab::{self, ScopeKind};
use egui_pop_core::theme::Tokens;
use egui_pop_core::ui::UiPop as _;
use egui_pop_core::widgets;
use egui_pop_model::admin::hooks::Values;
use egui_pop_model::admin::layers::ValueScope;
use egui_pop_model::admin::{
    Action, ActionStyle, CellKind, Column, Field, FieldKind, Item, RecordSource, Records, RowField,
    Section, StatusSource, Tone, Tree, cell_text, form_context, holds, meter_share, row_context,
    template, tone_named,
};
use egui_pop_model::fuzzy;
use egui_pop_model::ids::{CommandId, SettingId};
use egui_pop_model::values::{self, Value};
use serde_json::Value as Json;

use super::{ADMIN, Admin, SURFACE, in_window};
use crate::app::AppHandle;
use crate::hooks::SettingEntry;
use crate::settings_window;
use crate::strings as txt;

/// Draws the app while it is open, in the mode the `admin.window`
/// setting says.
pub(super) fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(admin) = app.service(&ADMIN) else {
        return;
    };
    if !admin.is_open() {
        return;
    }
    let ctx = ui.ctx().clone();
    if in_window(app) {
        let title = strings::tr(&ctx, &txt::SETTINGS_TITLE);
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([1000.0, 700.0]);
        let mut close = false;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("egui_pop.admin"),
            builder,
            |ui, _class| {
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    close = true;
                }
                egui::CentralPanel::default().show(ui, |ui| {
                    tab::scope(ui, "egui_pop.admin", ScopeKind::Panel, |ui| {
                        if page(ui, app, &admin) {
                            close = true;
                        }
                    });
                });
            },
        );
        if close {
            admin.close();
        }
        return;
    }
    let spec = OverlaySpec::new("egui_pop.admin", "Settings", Kind::Modal).keep_on_escape();
    let mut open = true;
    let rect = ctx.content_rect();
    let mut close = false;
    overlay::hold(&ctx, &spec, &mut open, None, rect, || {
        let id = egui::Id::unique("egui_pop.admin.page");
        egui::Modal::new(id)
            .area(
                egui::Area::new(id)
                    .order(egui::Order::Foreground)
                    .fixed_pos(rect.min),
            )
            .backdrop_color(egui::Color32::TRANSPARENT)
            .frame(egui::Frame::NONE)
            .show(&ctx, |ui| {
                let t = Tokens::of(ui.ctx());
                ui.painter().rect_filled(rect, 0.0, t.surface.canvas);
                let margin = t.space.l;
                let inner = rect.shrink(margin);
                ui.set_min_size(rect.size());
                ui.set_max_size(rect.size());
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                tab::scope(&mut child, "egui_pop.admin", ScopeKind::Modal, |ui| {
                    if page(ui, app, &admin) {
                        close = true;
                    }
                });
            });
    });
    if close || !open {
        admin.close();
        overlay::close(&ctx, spec.id);
    }
}

/// The page: tabs and search, the sections, the section's page. `true`
/// to close.
fn page(ui: &mut Ui, app: &AppHandle, admin: &Admin) -> bool {
    let _history = values::enter_history(&egui_pop_model::history::SETTINGS);
    let mut close = false;
    egui_pop_core::focus::scope(ui, SURFACE, &[], |ui| {
        // A wizard running fills the page.
        if admin.running().is_some() {
            super::wizard::page(ui, app, admin);
            return;
        }
        let ctx = ui.ctx().clone();
        let t = Tokens::of(&ctx);
        let tree = admin.tree(app);
        let entries = app.settings();
        let scopes = tree.shown_scopes();
        if scopes.is_empty() {
            ui.weak(strings::tr(&ctx, &txt::NOTHING_MATCHES));
            return;
        }
        let (scope, mut section, mut filter) = admin.shown();
        // A search hit chosen: the item to land on.
        let mut landed: Option<String> = None;
        // Escape clears the search, then closes; while a shortcuts editor
        // captures keys, it is the editor's.
        if !app.keys_captured() && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            if filter.is_empty() {
                close = true;
            } else {
                filter.clear();
            }
        }
        let mut scope = scope
            .filter(|s| scopes.iter().any(|x| x.meta.id == *s))
            .unwrap_or_else(|| scopes[0].meta.id.clone());
        // Ctrl+PageDown and Ctrl+PageUp move between scopes.
        let at = scopes.iter().position(|s| s.meta.id == scope).unwrap_or(0);
        let step = ui.input_mut(|i| {
            if i.consume_key(Modifiers::COMMAND, Key::PageDown) {
                1
            } else if i.consume_key(Modifiers::COMMAND, Key::PageUp) {
                -1
            } else {
                0
            }
        });
        if step != 0 {
            let n = scopes.len() as i32;
            scope = scopes[((at as i32 + step).rem_euclid(n)) as usize]
                .meta
                .id
                .clone();
            section = None;
        }
        // The tab row: the scopes, then the search, pop out and close.
        let mut window = in_window(app);
        ui.horizontal(|ui| {
            let tabs = ui
                .horizontal(|ui| {
                    for s in &scopes {
                        let selected = filter.is_empty() && s.meta.id == scope;
                        let r = ui.add(widgets::SelectableRow::new(
                            selected,
                            scope_title(&ctx, &s.meta.id, &s.meta.title),
                        ));
                        if r.clicked() {
                            scope = s.meta.id.clone();
                            section = None;
                            filter.clear();
                        }
                    }
                })
                .response;
            a11y::name(&tabs, Role::TabList, &strings::tr(&ctx, &txt::ADMIN_SCOPES));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let r = ui.button(strings::tr(&ctx, &txt::ADMIN_CLOSE));
                if r.clicked() {
                    close = true;
                }
                let (label, tip) = if window {
                    (&txt::ADMIN_POP_IN, &txt::ADMIN_POP_IN)
                } else {
                    (&txt::ADMIN_POP_OUT, &txt::ADMIN_POP_OUT_TIP)
                };
                if ui
                    .button(strings::tr(&ctx, label))
                    .on_hover_text(strings::tr(&ctx, tip))
                    .clicked()
                {
                    window = !window;
                    let _ = admin.set_in(
                        app,
                        ValueScope::App,
                        &super::WINDOW,
                        serde_json::json!(window),
                    );
                }
                let hint = strings::tr(&ctx, &txt::ADMIN_SEARCH_HINT);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut filter)
                        .hint_text(&hint)
                        .desired_width(240.0),
                );
                a11y::name(&r, Role::SearchInput, &hint);
                if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F)) {
                    r.request_focus();
                }
            });
        });
        ui.separator();
        let query = filter.trim().to_string();
        let found = if query.is_empty() {
            Vec::new()
        } else {
            search(&ctx, app, admin, &entries, &tree, &query)
        };
        let sections = tree.sections_of(&scope);
        let current = section
            .clone()
            .filter(|s| sections.iter().any(|x| x.meta.id == *s))
            .or_else(|| sections.first().map(|s| s.meta.id.clone()));
        // Answers read again after the events the scope's nodes name.
        let patterns: Vec<String> = tree
            .sections
            .iter()
            .flat_map(|s| s.groups.iter().flat_map(|g| &g.items))
            .flat_map(|i| match i {
                Item::Status(st) => st.refresh.clone(),
                Item::List(r) | Item::Table(r) => r.refresh.clone(),
                _ => Vec::new(),
            })
            .collect();
        admin.watch(app, &patterns);
        let height = ui.available_height();
        ui.horizontal_top(|ui| {
            // The left panel: the scope's sections, or the hits.
            let nav = ui
                .vertical(|ui| {
                    ui.set_width(220.0);
                    ui.set_min_height(height);
                    if query.is_empty() {
                        for s in &sections {
                            let cx = form_context(app.when_context(), &values_of(app, &entries, s));
                            if !holds(s.meta.when.as_deref(), &cx) {
                                continue;
                            }
                            let n = modified(app, admin, &entries, s);
                            let mut title = section_title(&ctx, app, &entries, s);
                            if n > 0 {
                                title = format!("{title}  ({n})");
                            }
                            let selected = current.as_deref() == Some(s.meta.id.as_str());
                            let r = ui.add(widgets::SelectableRow::new(selected, title));
                            if r.clicked() {
                                section = Some(s.meta.id.clone());
                            }
                        }
                    } else {
                        // Each section found, then the items found in it:
                        // choosing one lands on it.
                        for (s, list) in &found {
                            let mut title = format!(
                                "{} › {}",
                                scope_title_of(&ctx, &tree, &s.scope),
                                section_title(&ctx, app, &entries, s)
                            );
                            if !list.is_empty() {
                                title = format!("{title}  ({})", list.len());
                            }
                            let mut chosen = ui
                                .add(widgets::SelectableRow::new(false, title))
                                .clicked()
                                .then_some(None);
                            for h in list.iter().take(NAV_HITS) {
                                let r = ui
                                    .horizontal(|ui| {
                                        ui.add_space(t.space.l);
                                        ui.add(widgets::SelectableRow::new(
                                            false,
                                            h.title(&ctx, &entries),
                                        ))
                                    })
                                    .inner;
                                if r.clicked() {
                                    chosen = Some(Some(h.key().to_string()));
                                }
                            }
                            if let Some(key) = chosen {
                                scope = s.scope.clone();
                                section = Some(s.meta.id.clone());
                                filter.clear();
                                landed = key;
                            }
                        }
                    }
                })
                .response;
            a11y::name(&nav, Role::Tree, &strings::tr(&ctx, &txt::ADMIN_SECTIONS));
            ui.separator();
            // The page's width, kept: nothing in it pushes its right edge
            // out (rows wrap, their controls stay inside).
            let width = ui.available_width();
            egui::ScrollArea::vertical()
                .id_salt("egui_pop.admin.page")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let width = (width - ui.spacing().scroll.allocated_width()).max(0.0);
                    ui.set_max_width(width);
                    ui.vertical(|ui| {
                        ui.set_max_width(width);
                        if query.is_empty() {
                            if let Some(s) = current
                                .as_deref()
                                .and_then(|c| sections.iter().find(|s| s.meta.id == c))
                            {
                                section_page(ui, app, admin, &entries, s);
                            }
                        } else {
                            for (s, list) in &found {
                                let crumb = format!(
                                    "{} › {}",
                                    scope_title_of(&ctx, &tree, &s.scope),
                                    section_title(&ctx, app, &entries, s)
                                );
                                // A section found by its title or keywords:
                                // the way to it.
                                let mut open = false;
                                ui.horizontal(|ui| {
                                    ui.strong(&crumb);
                                    if list.is_empty() {
                                        let r = ui.small_button(strings::tr(
                                            &ctx,
                                            &txt::SETTINGS_OPEN_PAGE,
                                        ));
                                        a11y::name(
                                            &r,
                                            Role::Button,
                                            &strings::tr_args(
                                                &ctx,
                                                &txt::SETTINGS_OPEN_PAGE_NAME,
                                                &[("page", &crumb)],
                                            ),
                                        );
                                        open = r.clicked();
                                    }
                                });
                                ui.add_space(t.space.s);
                                for h in list {
                                    match h {
                                        Hit::Setting(f) => {
                                            if let Some(e) = entry_of(&entries, f) {
                                                field_row(ui, app, admin, e, f);
                                            }
                                        }
                                        Hit::Item(key, title) => {
                                            if item_hit(ui, title) {
                                                landed = Some(key.clone());
                                                open = true;
                                            }
                                        }
                                    }
                                }
                                if open {
                                    scope = s.scope.clone();
                                    section = Some(s.meta.id.clone());
                                    filter.clear();
                                }
                                ui.add_space(t.space.m);
                            }
                            if found.is_empty() {
                                ui.weak(strings::tr(&ctx, &txt::NOTHING_MATCHES));
                            }
                        }
                    });
                });
        });
        let moved = {
            let mut s = lock(&admin.state);
            let moved = s.section != section || s.scope.as_deref() != Some(scope.as_str());
            if moved {
                s.row = None;
                s.mark = None;
            }
            if let Some(key) = landed {
                s.mark = Some(key.clone());
                s.key = Some(key);
            }
            s.scope = Some(scope);
            s.section = section;
            s.filter = filter;
            moved
        };
        if moved {
            admin.refresh();
        }
    });
    close
}

/// A scope's title as shown: the generated ones translated.
fn scope_title(ctx: &egui::Context, id: &str, title: &str) -> String {
    let key = match id {
        "general" => &txt::ADMIN_SCOPE_GENERAL,
        "interface" => &txt::ADMIN_SCOPE_INTERFACE,
        "input" => &txt::ADMIN_SCOPE_INPUT,
        "developer" => &txt::ADMIN_SCOPE_DEVELOPER,
        _ => return title.to_string(),
    };
    strings::tr(ctx, key)
}

fn scope_title_of(ctx: &egui::Context, tree: &Tree, id: &str) -> String {
    let title = tree
        .scopes
        .iter()
        .find(|s| s.meta.id == id)
        .map_or(id, |s| s.meta.title.as_str());
    scope_title(ctx, id, title)
}

/// A section's title as shown: a generated one by its settings' owner
/// (translated), a declared one as declared.
fn section_title(
    ctx: &egui::Context,
    app: &AppHandle,
    entries: &[SettingEntry],
    s: &Section,
) -> String {
    let first = s
        .groups
        .iter()
        .flat_map(|g| &g.items)
        .find_map(|i| match i {
            Item::Field(f) => entry_of(entries, f),
            _ => None,
        });
    match first {
        Some(e) if e.section == s.meta.title => app.section_title(ctx, &e.owner, &e.section),
        _ => builtin_text(ctx, &s.meta.title),
    }
}

/// A text of the pages egui-pop declares itself (the shortcuts, gestures,
/// fonts and Advanced pages), in the interface's language; any other as
/// declared.
fn builtin_text(ctx: &egui::Context, text: &str) -> String {
    [
        &txt::SETTINGS_SHORTCUTS,
        &txt::SETTINGS_FONTS,
        &txt::ADMIN_SECTION_GESTURES,
        &txt::ADMIN_SECTION_ADVANCED,
        &txt::ADMIN_ADVANCED_ABOUT,
        &txt::ADMIN_ADVANCED_OPEN,
    ]
    .into_iter()
    .find(|k| k.fallback == text)
    .map_or_else(|| text.to_string(), |k| strings::tr(ctx, k))
}

fn entry_of<'a>(entries: &'a [SettingEntry], f: &Field) -> Option<&'a SettingEntry> {
    let id = f.setting_id()?;
    entries.iter().find(|e| e.id.as_str() == id)
}

/// How many items found a section lists under it in the left panel
/// (its count says them all; the page shows them all).
const NAV_HITS: usize = 8;

/// What the search found inside a section.
enum Hit {
    /// A setting: its row, editable in place.
    Setting(Box<Field>),
    /// An item inside a widget (a command of the shortcuts editor): its
    /// key and its title.
    Item(String, String),
}

impl Hit {
    /// What landing on it marks: the setting id, or the item's key.
    fn key(&self) -> &str {
        match self {
            Hit::Setting(f) => f.setting_id().unwrap_or_default(),
            Hit::Item(key, _) => key,
        }
    }

    fn title(&self, ctx: &egui::Context, entries: &[SettingEntry]) -> String {
        match self {
            Hit::Setting(f) => entry_of(entries, f).map_or_else(
                || f.meta.id.clone(),
                |e| settings_window::shown(ctx, e).label,
            ),
            Hit::Item(_, title) => title.clone(),
        }
    }
}

/// An item found inside a widget, on the search's page: its title and
/// the button landing on it. `true` when chosen.
fn item_hit(ui: &mut Ui, title: &str) -> bool {
    let ctx = ui.ctx().clone();
    ui.horizontal_wrapped(|ui| {
        ui.label(title);
        let r = ui.small_button(strings::tr(&ctx, &txt::ADMIN_SHOW_ITEM));
        a11y::name(
            &r,
            Role::Button,
            &strings::tr_args(&ctx, &txt::ADMIN_SHOW_ITEM_NAME, &[("item", title)]),
        );
        r.clicked()
    })
    .inner
}

/// What a section holds matching the search: its fields and the
/// settings its widgets edit (label, description, section and id, as
/// shown), and the items a widget kind's search finds in its widgets.
fn hits(
    ctx: &egui::Context,
    app: &AppHandle,
    admin: &Admin,
    entries: &[SettingEntry],
    s: &Section,
    query: &str,
) -> Vec<Hit> {
    let found = |id: &str| {
        entries
            .iter()
            .find(|e| e.id.as_str() == id)
            .is_some_and(|e| {
                settings_window::matches(ctx, app, e, query)
                    || query
                        .split_whitespace()
                        .all(|w| fuzzy::score(w, e.id.as_str()).is_some())
            })
    };
    let mut out = Vec::new();
    for item in s.groups.iter().flat_map(|g| &g.items) {
        match item {
            Item::Field(f) if f.setting_id().is_some_and(found) => {
                out.push(Hit::Setting(Box::new(f.clone())));
            }
            Item::Widget(w) => {
                out.extend(
                    w.settings()
                        .filter(|id| found(id))
                        .map(|id| Hit::Setting(Box::new(Field::setting(id)))),
                );
                if let Some(search) = admin.search_of(&w.kind) {
                    out.extend(
                        search(ctx, app, &w.options, query)
                            .into_iter()
                            .map(|(key, title)| Hit::Item(key, title)),
                    );
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether a section's title, description or keywords hold every word
/// of the search.
fn section_found(
    ctx: &egui::Context,
    app: &AppHandle,
    entries: &[SettingEntry],
    s: &Section,
    query: &str,
) -> bool {
    let text = format!(
        "{} {} {}",
        section_title(ctx, app, entries, s),
        builtin_text(ctx, &s.meta.description),
        s.meta.keywords.join(" ")
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|w| text.contains(&w.to_lowercase()))
}

/// The search's hits by section: a setting placed in several sections
/// shown once, at its first place, as an item is; a section found by its
/// title or keywords listed even with nothing found inside it.
fn search<'a>(
    ctx: &egui::Context,
    app: &AppHandle,
    admin: &Admin,
    entries: &[SettingEntry],
    tree: &'a Tree,
    query: &str,
) -> Vec<(&'a Section, Vec<Hit>)> {
    let mut seen: Vec<(bool, String)> = Vec::new();
    let mut out = Vec::new();
    for s in &tree.sections {
        let list: Vec<Hit> = hits(ctx, app, admin, entries, s, query)
            .into_iter()
            .filter(|h| {
                let k = (matches!(h, Hit::Item(..)), h.key().to_string());
                let fresh = !seen.contains(&k);
                if fresh {
                    seen.push(k);
                }
                fresh
            })
            .collect();
        if !list.is_empty() || section_found(ctx, app, entries, s, query) {
            out.push((s, list));
        }
    }
    out
}

/// How many of a section's settings hold a value other than their
/// default, in any layer.
fn modified(app: &AppHandle, admin: &Admin, entries: &[SettingEntry], s: &Section) -> usize {
    let store = app.store();
    let st = lock(&store);
    let layers = admin.layers();
    s.groups
        .iter()
        .flat_map(|g| &g.items)
        .filter_map(|i| match i {
            Item::Field(f) => entry_of(entries, f),
            _ => None,
        })
        .filter(|e| {
            st.get(&e.path).is_some_and(|x| {
                !layers
                    .holders(&e.path, &x.value.to_json(), &x.descriptor.default.to_json())
                    .is_empty()
            })
        })
        .count()
}

/// Set while a widget of a page draws.
const IN_WIDGET: &str = "egui_pop.admin.in_widget";

/// A setting a custom page draws ([`settings_window::setting_row`]),
/// while the Settings app draws the page: the app's own row (its value
/// scopes, origin, modified bar, and the mark a search hit leaves).
/// `false` elsewhere, or when no setting has that id.
pub(super) fn setting_row(ui: &mut Ui, app: &AppHandle, id: &SettingId) -> bool {
    if !ui
        .ctx()
        .data(|d| d.get_temp::<bool>(egui::Id::unique(IN_WIDGET)))
        .unwrap_or(false)
    {
        return false;
    }
    let Some(admin) = app.service(&ADMIN) else {
        return false;
    };
    let entries = app.settings();
    let Some(e) = entries.iter().find(|e| e.id == *id) else {
        return false;
    };
    field_row(ui, app, &admin, e, &Field::setting(id.as_str()));
    true
}

/// A section's page: its title and "Reset section", its description,
/// its groups in inset boxes.
fn section_page(
    ui: &mut Ui,
    app: &AppHandle,
    admin: &Admin,
    entries: &[SettingEntry],
    s: &Section,
) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let n = modified(app, admin, entries, s);
    ui.horizontal(|ui| {
        ui.heading(section_title(&ctx, app, entries, s));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            let r = ui.add_enabled(
                n > 0,
                egui::Button::new(strings::tr(&ctx, &txt::ADMIN_RESET_SECTION)),
            );
            if n > 0 {
                ui.weak(strings::tr_args(
                    &ctx,
                    &txt::ADMIN_MODIFIED_COUNT,
                    &[("count", &n.to_string())],
                ));
            }
            if r.clicked() {
                let layers = admin.layers();
                for f in s
                    .groups
                    .iter()
                    .flat_map(|g| &g.items)
                    .filter_map(|i| match i {
                        Item::Field(f) => Some(f),
                        _ => None,
                    })
                {
                    if let Some(e) = entry_of(entries, f) {
                        let origin = layers.origin(&e.path);
                        if origin.editable() {
                            let _ = admin.clear_in(app, origin, &e.id);
                        }
                    }
                }
            }
        });
    });
    if !s.meta.description.is_empty() {
        ui.weak(builtin_text(&ctx, &s.meta.description));
    }
    ui.add_space(t.space.s);
    let values = values_of(app, entries, s);
    let cx = form_context(app.when_context(), &values);
    for g in &s.groups {
        if !holds(g.meta.when.as_deref(), &cx) {
            continue;
        }
        if !g.meta.title.is_empty() {
            ui.strong(&g.meta.title);
        }
        inset(ui, |ui| {
            for item in &g.items {
                let meta = item.meta();
                if !holds(meta.when.as_deref(), &cx) {
                    continue;
                }
                let on = holds(meta.enabled.as_deref(), &cx);
                let r = ui
                    .add_enabled_ui(on, |ui| item_row(ui, app, admin, entries, item, &values))
                    .response;
                if !on && let Some(reason) = &meta.reason {
                    r.on_disabled_hover_text(reason);
                }
            }
        });
        ui.add_space(t.space.m);
    }
}

/// A group's inset box (the `form-group` brick).
pub(super) fn inset(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let t = Tokens::of(ui.ctx());
    egui::Frame::new()
        .fill(t.surface.raised)
        .stroke(egui::Stroke::new(1.0, t.border.default))
        .corner_radius(egui::CornerRadius::same(
            t.radius.widget.round().clamp(0.0, 255.0) as u8,
        ))
        .inner_margin(egui::Margin::same(t.space.m.round().clamp(0.0, 127.0) as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// The values a section's clauses and actions read: its settings, by
/// field id.
fn values_of(app: &AppHandle, entries: &[SettingEntry], s: &Section) -> Values {
    let mut values = Values::new();
    let store = app.store();
    let st = lock(&store);
    for f in s
        .groups
        .iter()
        .flat_map(|g| &g.items)
        .filter_map(|i| match i {
            Item::Field(f) => Some(f),
            _ => None,
        })
    {
        if let Some(e) = entry_of(entries, f)
            && let Some(x) = st.get(&e.path)
        {
            values.insert(f.meta.id.clone(), x.value.to_json());
        }
    }
    values
}

/// One item of a group, as its brick draws it.
pub(super) fn item_row(
    ui: &mut Ui,
    app: &AppHandle,
    admin: &Admin,
    entries: &[SettingEntry],
    item: &Item,
    values: &Values,
) {
    let ctx = ui.ctx().clone();
    match item {
        Item::Field(f) => match entry_of(entries, f) {
            Some(e) => field_row(ui, app, admin, e, f),
            None => {
                ui.weak(format!("{}: {}", f.meta.id, f.meta.title));
            }
        },
        Item::Action(a) => {
            action_button(ui, app, admin, a, values);
        }
        Item::Status(st) => {
            let shown = match &st.source {
                StatusSource::Hook { hook } => admin.hooks().status(hook).map(Ok),
                StatusSource::Command { command, pointer } => admin
                    .read(app, command, &Json::Null)
                    .map(|r| r.map(|answer| status_of(&answer, pointer))),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(&st.meta.title);
                ui.with_layout(
                    egui::Layout::right_to_left(Align::Center),
                    |ui| match shown {
                        Some(Ok((text, tone))) => {
                            ui.add(Indicator::text(text).tone(kit_tone(tone)));
                        }
                        Some(Err(e)) => {
                            ui.weak("—").on_hover_text(e);
                        }
                        None => {
                            ui.weak("…");
                        }
                    },
                );
            });
        }
        Item::Callout(c) => {
            ui.horizontal_wrapped(|ui| {
                ui.add(Indicator::text(&c.text).tone(kit_tone(c.tone)));
                if let Some(a) = &c.action {
                    action_button(ui, app, admin, a, values);
                }
            });
        }
        Item::Widget(w) => match admin.widget(&w.kind) {
            Some(draw) => {
                // A custom page's settings drawn inside it take the
                // Settings app's rows ([`setting_row`]).
                let id = egui::Id::unique(IN_WIDGET);
                let before = ctx.data(|d| d.get_temp::<bool>(id));
                ctx.data_mut(|d| d.insert_temp(id, true));
                draw(ui, app, &w.options);
                ctx.data_mut(|d| match before {
                    Some(b) => {
                        d.insert_temp(id, b);
                    }
                    None => d.remove::<bool>(id),
                });
            }
            None => {
                ui.weak(strings::tr_args(
                    &ctx,
                    &txt::ADMIN_NO_WIDGET,
                    &[("kind", &w.kind)],
                ));
            }
        },
        Item::List(r) => records(ui, app, admin, r, values, false),
        Item::Table(r) => records(ui, app, admin, r, values, true),
    }
}

/// A status's value and tone in a command's answer: at the pointer, a
/// value written out, or an object's `text` and `tone`.
fn status_of(answer: &Json, pointer: &str) -> (String, Tone) {
    let v = answer.pointer(pointer).unwrap_or(&Json::Null);
    match v.get("text") {
        Some(text) => (
            cell_text(text),
            v["tone"].as_str().and_then(tone_named).unwrap_or_default(),
        ),
        None => (cell_text(v), Tone::Info),
    }
}

/// An action's title: its own, else its command's.
fn action_title(app: &AppHandle, a: &Action) -> String {
    if !a.meta.title.is_empty() {
        return a.meta.title.clone();
    }
    CommandId::new(&a.command)
        .ok()
        .and_then(|id| app.command(&id))
        .map_or(a.command.clone(), |c| c.title)
}

/// An action as its button: runs its command with its arguments filled
/// from the values, after its confirmation when it asks one; answers
/// read are read again after it.
fn action_button(ui: &mut Ui, app: &AppHandle, admin: &Admin, a: &Action, values: &Values) {
    let title = builtin_text(ui.ctx(), &action_title(app, a));
    let r = match a.style {
        ActionStyle::Link => ui.link(title),
        ActionStyle::Primary => ui.add(egui::Button::new(title).selected(true)),
        ActionStyle::Danger => ui.add(egui::Button::new(
            egui::RichText::new(title).color(Tokens::of(ui.ctx()).status.danger),
        )),
        ActionStyle::Secondary => ui.button(title),
    };
    if r.clicked()
        && let Ok(id) = CommandId::new(&a.command)
    {
        let (args, _) = template::expand(&a.args, values);
        let asked = a
            .confirmation
            .as_ref()
            .map(|c| c.filled(values))
            .map(|c| (c.title, c.text, c.verb, c.danger))
            .or_else(|| {
                a.confirm
                    .as_ref()
                    .and_then(|h| admin.hooks().confirm(h, values))
                    .map(|c| (c.message.clone(), String::new(), c.verb, c.danger))
            });
        match asked {
            Some((title, text, verb, danger)) => {
                let Some(dialogs) = app.service(&crate::dialogs::DIALOGS) else {
                    log::warn!(
                        "`{}` asks a confirmation and there are no dialogs: not run",
                        a.command
                    );
                    return;
                };
                let mut c = crate::dialogs::Confirm::new(&title, &verb, id)
                    .detail(&text)
                    .args(args);
                if danger {
                    c = c.danger();
                }
                dialogs.open(c);
            }
            None => {
                app.run(id, args);
                admin.refresh();
            }
        }
    }
}

/// A row field's cell: the record's value as a toggle, a text or a
/// choice; a change runs its command with the record and the value.
#[allow(clippy::too_many_arguments)]
fn row_field(
    ui: &mut Ui,
    app: &AppHandle,
    admin: &Admin,
    r: &Records,
    f: &RowField,
    row: &Json,
    values: &Values,
    labelled: bool,
) {
    let value = &row[f.key.as_str()];
    let row_id = cell_text(&row[r.key.as_str()]);
    let mut wrote = None;
    let resp = match f.kind {
        FieldKind::Toggle => {
            let mut on = value.as_bool().unwrap_or(false);
            let text = if labelled { f.meta.title.as_str() } else { "" };
            let resp = ui.checkbox(&mut on, text);
            if resp.changed() {
                wrote = Some(Json::Bool(on));
            }
            a11y::name(&resp, Role::CheckBox, &f.meta.title);
            resp
        }
        FieldKind::Choice => {
            let choices = f.choices_of(row);
            let now = cell_text(value);
            let shown = choices
                .iter()
                .find(|c| cell_text(&c.value) == now)
                .map_or(now.clone(), |c| c.label.clone());
            if labelled {
                ui.label(&f.meta.title);
            }
            let mut picked = None;
            let resp = ui
                .combo_box(
                    widgets::combo_box((
                        "egui_pop.admin.row_field",
                        &r.meta.id,
                        &row_id,
                        &f.meta.id,
                    ))
                    .selected_text(shown),
                    |ui| {
                        for c in &choices {
                            if ui
                                .add(widgets::SelectableRow::new(
                                    cell_text(&c.value) == now,
                                    &c.label,
                                ))
                                .clicked()
                            {
                                picked = Some(c.value.clone());
                            }
                        }
                    },
                )
                .response;
            a11y::name(&resp, Role::ComboBox, &f.meta.title);
            if let Some(p) = picked.filter(|p| cell_text(p) != now) {
                wrote = Some(p);
            }
            resp
        }
        _ => {
            // A text: typed into a buffer, written when left or on Enter.
            let id = egui::Id::unique(("egui_pop.admin.row_text", &r.meta.id, &row_id, &f.meta.id));
            let now = cell_text(value);
            let mut text = ui
                .data(|d| d.get_temp::<String>(id))
                .unwrap_or_else(|| now.clone());
            if labelled {
                ui.label(&f.meta.title);
            }
            let resp = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .id(id.with("edit"))
                    .desired_width(180.0),
            );
            a11y::name(&resp, Role::TextInput, &f.meta.title);
            if resp.has_focus() {
                ui.data_mut(|d| d.insert_temp(id, text.clone()));
            } else {
                ui.data_mut(|d| d.remove::<String>(id));
                if resp.lost_focus() && text.trim() != now && !text.trim().is_empty() {
                    wrote = Some(Json::String(text.trim().to_string()));
                }
            }
            resp
        }
    };
    if !f.meta.description.is_empty() {
        resp.on_hover_text(&f.meta.description);
    }
    if let Some(v) = wrote
        && let Ok(id) = CommandId::new(&f.command)
    {
        app.run(id, f.write_args(values, v));
        admin.refresh();
    }
}

/// A row's field, when its `when` holds, greyed when its `enabled` does
/// not.
#[allow(clippy::too_many_arguments)]
fn row_field_shown(
    ui: &mut Ui,
    app: &AppHandle,
    admin: &Admin,
    r: &Records,
    f: &RowField,
    row: &Json,
    values: &Values,
    cx: &egui_pop_model::when::WhenContext,
    labelled: bool,
) {
    if !holds(f.meta.when.as_deref(), cx) {
        if !labelled {
            ui.label("");
        }
        return;
    }
    let on = holds(f.meta.enabled.as_deref(), cx);
    let resp = ui
        .add_enabled_ui(on, |ui| {
            row_field(ui, app, admin, r, f, row, values, labelled);
        })
        .response;
    if !on && let Some(reason) = &f.meta.reason {
        resp.on_disabled_hover_text(reason);
    }
}

/// A row's actions whose `when` holds, greyed when their `enabled` does
/// not.
fn row_actions(
    ui: &mut Ui,
    app: &AppHandle,
    admin: &Admin,
    actions: &mut dyn Iterator<Item = &Action>,
    values: &Values,
    cx: &egui_pop_model::when::WhenContext,
) {
    for a in actions {
        if !holds(a.meta.when.as_deref(), cx) {
            continue;
        }
        let on = holds(a.meta.enabled.as_deref(), cx);
        let resp = ui
            .add_enabled_ui(on, |ui| action_button(ui, app, admin, a, values))
            .response;
        if !on && let Some(reason) = &a.meta.reason {
            resp.on_disabled_hover_text(reason);
        }
    }
}

/// A column's cell: its text, or its meter.
fn cell(ui: &mut Ui, c: &Column, row: &Json, tone: Option<Tone>) -> egui::Response {
    let v = &row[c.key.as_str()];
    if c.kind == CellKind::Meter
        && let Some(share) = meter_share(v)
    {
        let text = c
            .text
            .as_ref()
            .map(|k| cell_text(&row[k.as_str()]))
            .unwrap_or_else(|| format!("{:.0} %", share * 100.0));
        let resp = ui.add(
            egui::ProgressBar::new(share)
                .desired_width(160.0)
                .text(text.clone()),
        );
        a11y::name(
            &resp,
            Role::ProgressIndicator,
            &format!("{}: {text}", c.title),
        );
        return resp;
    }
    let text = match (c.kind, &c.text) {
        (CellKind::Meter, Some(k)) => cell_text(&row[k.as_str()]),
        _ => cell_text(v),
    };
    match tone {
        Some(tone) => ui.add(Indicator::text(text).tone(kit_tone(tone))),
        None => ui.label(text),
    }
}

/// A record list or table: its rows read from its source, each with its
/// fields and actions; its empty state; what went wrong reading it. The
/// row a deep link names (`scope/section/<row key>`) is marked and
/// scrolled to.
fn records(ui: &mut Ui, app: &AppHandle, admin: &Admin, r: &Records, values: &Values, table: bool) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    if !r.meta.title.is_empty() {
        ui.label(egui::RichText::new(&r.meta.title).strong());
    }
    let rows = match &r.source {
        RecordSource::Hook { hook } => admin
            .hooks()
            .records(hook)
            .ok_or_else(|| format!("no records hook `{hook}`")),
        RecordSource::Command {
            command,
            args,
            pointer,
        } => match admin.read(app, command, &template::expand(args, values).0) {
            None => {
                ui.weak("…");
                return;
            }
            Some(answer) => answer.map(|a| Records::rows_of(&a, pointer)),
        },
    };
    let rows = match rows {
        Ok(rows) => rows,
        Err(e) => {
            ui.add(Indicator::text(e).tone(KitTone::Danger));
            return;
        }
    };
    if rows.is_empty() {
        ui.horizontal_wrapped(|ui| {
            let text = r.empty.as_ref().map_or_else(
                || strings::tr(&ctx, &txt::ADMIN_NO_RECORDS),
                |e| e.text.clone(),
            );
            ui.weak(text);
            if let Some(a) = r.empty.as_ref().and_then(|e| e.action.as_ref()) {
                action_button(ui, app, admin, a, values);
            }
        });
        return;
    }
    let base = form_context(app.when_context(), values);
    let tone_of = |row: &Json| {
        r.tone_key
            .as_ref()
            .and_then(|k| row[k.as_str()].as_str())
            .and_then(tone_named)
    };
    // The row a deep link named: marked while the page shows, scrolled
    // to once.
    let (selected, scroll) = {
        let mut s = lock(&admin.state);
        let hit = s
            .key
            .clone()
            .filter(|k| rows.iter().any(|row| cell_text(&row[r.key.as_str()]) == *k));
        if let Some(k) = &hit {
            s.row = Some(k.clone());
            s.key = None;
        }
        (s.row.clone(), hit.is_some())
    };
    let is_selected =
        |row: &Json| selected.as_deref() == Some(cell_text(&row[r.key.as_str()]).as_str());
    if table {
        let marked = rows.iter().position(&is_selected);
        let extra: Vec<&RowField> = r.extra_fields().collect();
        egui::ScrollArea::horizontal()
            .id_salt(("egui_pop.admin.table", &r.meta.id))
            .show(ui, |ui| {
                let selection = t.accent.default.gamma_multiply(0.25);
                egui::Grid::new(("egui_pop.admin.table", &r.meta.id))
                    .with_row_color(move |i, style| {
                        if marked.is_some_and(|m| m + 1 == i) {
                            Some(selection)
                        } else if i % 2 == 1 {
                            Some(style.visuals.faint_bg_color)
                        } else {
                            None
                        }
                    })
                    .spacing([t.space.l, t.space.s])
                    .show(ui, |ui| {
                        for c in &r.columns {
                            ui.label(egui::RichText::new(&c.title).strong());
                        }
                        for f in &extra {
                            ui.label(egui::RichText::new(&f.meta.title).strong());
                        }
                        if !r.row_actions.is_empty() {
                            ui.label("");
                        }
                        ui.end_row();
                        for row in &rows {
                            let tone = tone_of(row);
                            let v = Records::row_values(values, row);
                            let cx = row_context(base.clone(), row);
                            for (i, c) in r.columns.iter().enumerate() {
                                match r.field_for(&c.key) {
                                    Some(f) => {
                                        row_field_shown(ui, app, admin, r, f, row, &v, &cx, false);
                                    }
                                    None => {
                                        let resp = cell(ui, c, row, tone.filter(|_| i == 0));
                                        if i == 0 && scroll && is_selected(row) {
                                            resp.scroll_to_me(Some(Align::Center));
                                        }
                                    }
                                }
                            }
                            for f in &extra {
                                row_field_shown(ui, app, admin, r, f, row, &v, &cx, false);
                            }
                            if !r.row_actions.is_empty() {
                                ui.horizontal(|ui| {
                                    row_actions(ui, app, admin, &mut r.row_actions.iter(), &v, &cx);
                                });
                            }
                            ui.end_row();
                        }
                    });
            });
        return;
    }
    for row in &rows {
        let v = Records::row_values(values, row);
        let cx = row_context(base.clone(), row);
        let width = ui.available_width();
        let frame = if is_selected(row) {
            egui::Frame::NONE.fill(t.accent.default.gamma_multiply(0.25))
        } else {
            egui::Frame::NONE
        };
        let resp = frame
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(width, ui.spacing().interact_size.y),
                    egui::Layout::right_to_left(Align::Center),
                    |ui| {
                        ui.set_max_width(width);
                        row_actions(ui, app, admin, &mut r.row_actions.iter().rev(), &v, &cx);
                        for c in r.columns.iter().rev() {
                            if c.kind == CellKind::Meter {
                                cell(ui, c, row, None);
                                continue;
                            }
                            let text = cell_text(&row[c.key.as_str()]);
                            if !text.is_empty() {
                                ui.weak(text);
                            }
                        }
                        ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                            ui.vertical(|ui| {
                                let title = r.title_key.as_ref().map_or_else(
                                    || cell_text(&row[r.key.as_str()]),
                                    |k| cell_text(&row[k.as_str()]),
                                );
                                match tone_of(row) {
                                    Some(tone) => {
                                        ui.add(Indicator::text(title).tone(kit_tone(tone)));
                                    }
                                    None => {
                                        ui.label(title);
                                    }
                                }
                                if let Some(k) = &r.subtitle_key {
                                    let sub = cell_text(&row[k.as_str()]);
                                    if !sub.is_empty() {
                                        ui.weak(sub);
                                    }
                                }
                            });
                        });
                    },
                );
                // The row's fields, under its title, wrapped.
                if !r.row_fields.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.set_max_width(width);
                        for f in &r.row_fields {
                            row_field_shown(ui, app, admin, r, f, row, &v, &cx, true);
                        }
                    });
                }
            })
            .response;
        if scroll && is_selected(row) {
            resp.scroll_to_me(Some(Align::Center));
        }
        ui.add_space(t.space.xs);
    }
}

pub(super) fn kit_tone(tone: Tone) -> KitTone {
    match tone {
        Tone::Info => KitTone::Info,
        Tone::Success => KitTone::Success,
        Tone::Warning => KitTone::Warning,
        Tone::Danger => KitTone::Danger,
    }
}

fn origin_text(ctx: &egui::Context, scope: ValueScope) -> String {
    strings::tr(
        ctx,
        match scope {
            ValueScope::Library => &txt::ADMIN_ORIGIN_LIBRARY,
            ValueScope::Workspace => &txt::ADMIN_ORIGIN_WORKSPACE,
            _ => &txt::ADMIN_ORIGIN_RUN,
        },
    )
}

fn for_text(ctx: &egui::Context, scope: ValueScope) -> String {
    strings::tr(
        ctx,
        match scope {
            ValueScope::App => &txt::ADMIN_FOR_APP,
            ValueScope::Library => &txt::ADMIN_FOR_LIBRARY,
            ValueScope::Workspace => &txt::ADMIN_FOR_WORKSPACE,
            ValueScope::Run => &txt::ADMIN_ORIGIN_RUN,
        },
    )
}

/// One setting's row: the modified bar, its editor (the settings form's,
/// writing to the scope chosen), the origin tag, "Set for", its reset
/// and its help under it. The row is a named group for assistive
/// technology: its label, "modified" and its origin.
fn field_row(ui: &mut Ui, app: &AppHandle, admin: &Admin, e: &SettingEntry, f: &Field) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let store = app.store();
    let Some((value, default)) = lock(&store)
        .get(&e.path)
        .map(|x| (x.value.to_json(), x.descriptor.default.to_json()))
    else {
        return;
    };
    let layers = admin.layers();
    let origin = layers.origin(&e.path);
    let offered = admin.offered();
    let allowed: Vec<ValueScope> = offered
        .iter()
        .copied()
        .filter(|s| f.scopes.is_empty() || *s == ValueScope::App || f.scopes.contains(s))
        .collect();
    let target = admin
        .target(&e.path)
        .filter(|s| allowed.contains(s))
        .unwrap_or(if allowed.contains(&origin) {
            origin
        } else {
            ValueScope::App
        });
    let modified = !layers.holders(&e.path, &value, &default).is_empty();
    let d = settings_window::shown(&ctx, e);
    let id = e.id.as_str();
    let focus_me = admin.key_is(id);
    // The item a search hit or a deep link landed on: marked behind.
    let marked = admin.marked().as_deref() == Some(id);
    let behind = ui.painter().add(egui::Shape::Noop);
    // The row: its right-hand controls laid first, from the page's right
    // edge, then the bar and the editor in what is left; nothing goes
    // past the page.
    let width = ui.available_width();
    let row = ui
        .allocate_ui_with_layout(
            egui::vec2(width, ui.spacing().interact_size.y),
            egui::Layout::right_to_left(Align::Center),
            |ui| {
                ui.set_max_width(width);
                // The reset, of the layer the value comes from.
                let can_reset = match origin {
                    ValueScope::App => value != default,
                    ValueScope::Run => false,
                    _ => true,
                };
                let setting = [("setting", d.label.as_str())];
                let r = ui.add_enabled(
                    can_reset,
                    egui::Button::new(strings::tr(&ctx, &RESET)).small(),
                );
                a11y::name(
                    &r,
                    Role::Button,
                    &strings::tr_args(&ctx, &txt::SETTINGS_RESET_SETTING, &setting),
                );
                if r.clicked() {
                    let _ = admin.clear_in(app, origin, &e.id);
                }
                // "Set for", when it may be set in more than one scope.
                if allowed.len() > 1 {
                    let mut picked = target;
                    let combo = widgets::combo_box(("egui_pop.admin.set_for", id))
                        .selected_text(for_text(&ctx, target));
                    let r = ui
                        .combo_box(combo, |ui| {
                            for s in &allowed {
                                widgets::selectable_value(ui, &mut picked, *s, for_text(&ctx, *s));
                            }
                        })
                        .response;
                    a11y::name(
                        &r,
                        Role::ComboBox,
                        &strings::tr_args(&ctx, &txt::ADMIN_SET_FOR_NAME, &setting),
                    );
                    if picked != target {
                        admin.set_target(&e.path, picked);
                    }
                }
                // The origin tag.
                if origin != ValueScope::App {
                    let text = origin_text(&ctx, origin);
                    ui.add(
                        Indicator::text(&text)
                            .tone(KitTone::Accent)
                            .name(&text)
                            .tooltip(strings::tr_args(
                                &ctx,
                                &txt::ADMIN_ORIGIN_TIP,
                                &[("origin", &text)],
                            )),
                    );
                }
                ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                    let h = ui.spacing().interact_size.y;
                    let (bar, _) = ui.allocate_exact_size(egui::vec2(3.0, h), egui::Sense::hover());
                    if modified {
                        ui.painter().rect_filled(bar, 1.0, t.accent.default);
                    }
                    let read_only = origin == ValueScope::Run;
                    ui.add_enabled_ui(!read_only, |ui| {
                        ui.set_max_width(ui.available_width());
                        let shown = |c: &str| strings::setting_choice(&ctx, id, c);
                        if target == ValueScope::App && origin == ValueScope::App {
                            param::param_shown(ui, &d, Mode::Bound(&store, &e.path), &shown);
                        } else {
                            let now = layers
                                .value_in(target, &e.path, &value)
                                .unwrap_or(value.clone());
                            let mut v = Value::from_json(d.ty, &now).unwrap_or(d.default.clone());
                            let before = v.clone();
                            param::param_shown(ui, &d, Mode::Plain(&mut v), &shown);
                            if v != before
                                && let Ok(sid) = SettingId::new(id)
                            {
                                let _ = admin.set_in(app, target, &sid, v.to_json());
                            }
                        }
                    })
                    .response
                    .on_disabled_hover_text(strings::tr(&ctx, &txt::ADMIN_RUN_READ_ONLY));
                });
            },
        )
        .response;
    let mut name = d.label.clone();
    if modified {
        name = format!("{name}, {}", strings::tr(&ctx, &txt::ADMIN_MODIFIED));
    }
    if origin != ValueScope::App {
        name = format!("{name}, {}", origin_text(&ctx, origin));
    }
    if marked {
        ui.painter().set(
            behind,
            egui::epaint::RectShape::filled(
                row.rect.expand(t.space.xs),
                t.radius.widget,
                t.accent.default.gamma_multiply(0.25),
            ),
        );
        // Said selected to assistive technology.
        row.widget_info(move || {
            let mut info = egui::WidgetInfo::labeled(Role::Group, true, &name);
            info.selected = Some(true);
            info
        });
    } else {
        a11y::name(&row, Role::Group, &name);
    }
    if focus_me && admin.take_key(id) {
        row.scroll_to_me(Some(Align::Center));
    }
    let help = if f.help.is_empty() {
        d.description.clone()
    } else {
        f.help.clone()
    };
    if !help.is_empty() {
        // Under the label, wrapped to the page's width.
        let indent = (3.0 + ui.spacing().item_spacing.x)
            .round()
            .clamp(0.0, 127.0) as i8;
        egui::Frame::NONE
            .inner_margin(egui::Margin {
                left: indent,
                ..egui::Margin::ZERO
            })
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                crate::links::Inline::new(&help).weak().show(ui, app);
            });
    }
    ui.add_space(t.space.s);
}
