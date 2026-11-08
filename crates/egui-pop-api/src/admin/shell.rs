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
use egui_pop_model::admin::{Field, Item, Section, Tone, Tree, template};
use egui_pop_model::fuzzy;
use egui_pop_model::ids::{CommandId, SettingId};
use egui_pop_model::values::{self, Value};

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
        // Escape clears the search, then closes.
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
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
        let sections = tree.sections_of(&scope);
        let current = section
            .clone()
            .filter(|s| sections.iter().any(|x| x.meta.id == *s))
            .or_else(|| sections.first().map(|s| s.meta.id.clone()));
        let height = ui.available_height();
        ui.horizontal_top(|ui| {
            // The left panel: the scope's sections, or the hits.
            let nav = ui
                .vertical(|ui| {
                    ui.set_width(220.0);
                    ui.set_min_height(height);
                    if query.is_empty() {
                        for s in &sections {
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
                        for s in &tree.sections {
                            let hits = hits(&ctx, app, &entries, s, &query).len();
                            if hits == 0 {
                                continue;
                            }
                            let title = format!(
                                "{} › {}  ({hits})",
                                scope_title_of(&ctx, &tree, &s.scope),
                                section_title(&ctx, app, &entries, s)
                            );
                            if ui.add(widgets::SelectableRow::new(false, title)).clicked() {
                                scope = s.scope.clone();
                                section = Some(s.meta.id.clone());
                                filter.clear();
                            }
                        }
                    }
                })
                .response;
            a11y::name(&nav, Role::Tree, &strings::tr(&ctx, &txt::ADMIN_SECTIONS));
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("egui_pop.admin.page")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        if query.is_empty() {
                            if let Some(s) = current
                                .as_deref()
                                .and_then(|c| sections.iter().find(|s| s.meta.id == c))
                            {
                                section_page(ui, app, admin, &entries, s);
                            }
                        } else {
                            let mut any = false;
                            for s in &tree.sections {
                                let list = hits(&ctx, app, &entries, s, &query);
                                if list.is_empty() {
                                    continue;
                                }
                                any = true;
                                ui.strong(format!(
                                    "{} › {}",
                                    scope_title_of(&ctx, &tree, &s.scope),
                                    section_title(&ctx, app, &entries, s)
                                ));
                                ui.add_space(t.space.s);
                                for f in list {
                                    if let Some(e) = entry_of(&entries, f) {
                                        field_row(ui, app, admin, e, f);
                                    }
                                }
                                ui.add_space(t.space.m);
                            }
                            if !any {
                                ui.weak(strings::tr(&ctx, &txt::NOTHING_MATCHES));
                            }
                        }
                    });
                });
        });
        let mut s = lock(&admin.state);
        s.scope = Some(scope);
        s.section = section;
        s.filter = filter;
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
        _ => s.meta.title.clone(),
    }
}

fn entry_of<'a>(entries: &'a [SettingEntry], f: &Field) -> Option<&'a SettingEntry> {
    let id = f.setting_id()?;
    entries.iter().find(|e| e.id.as_str() == id)
}

/// The fields of a section matching the search: label, description,
/// section and id, as shown.
fn hits<'a>(
    ctx: &egui::Context,
    app: &AppHandle,
    entries: &[SettingEntry],
    s: &'a Section,
    query: &str,
) -> Vec<&'a Field> {
    s.groups
        .iter()
        .flat_map(|g| &g.items)
        .filter_map(|i| match i {
            Item::Field(f) => Some(f),
            _ => None,
        })
        .filter(|f| {
            entry_of(entries, f).is_some_and(|e| {
                settings_window::matches(ctx, app, e, query)
                    || query
                        .split_whitespace()
                        .all(|w| fuzzy::score(w, e.id.as_str()).is_some())
            })
        })
        .collect()
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
        ui.weak(&s.meta.description);
    }
    ui.add_space(t.space.s);
    // The values a section's actions take: its settings, by id.
    let mut values = Values::new();
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
            && let Some(x) = lock(&app.store()).get(&e.path)
        {
            values.insert(f.meta.id.clone(), x.value.to_json());
        }
    }
    for g in &s.groups {
        if !g.meta.title.is_empty() {
            ui.strong(&g.meta.title);
        }
        egui::Frame::new()
            .fill(t.surface.raised)
            .stroke(egui::Stroke::new(1.0, t.border.default))
            .corner_radius(egui::CornerRadius::same(
                t.radius.widget.round().clamp(0.0, 255.0) as u8,
            ))
            .inner_margin(egui::Margin::same(t.space.m.round().clamp(0.0, 127.0) as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for item in &g.items {
                    match item {
                        Item::Field(f) => match entry_of(entries, f) {
                            Some(e) => field_row(ui, app, admin, e, f),
                            None => {
                                ui.weak(format!("{}: {}", f.meta.id, f.meta.title));
                            }
                        },
                        Item::Action(a) => {
                            let title = if a.meta.title.is_empty() {
                                app.command(&CommandId::new(&a.command).unwrap_or(super::CMD_OPEN))
                                    .map_or(a.command.clone(), |c| c.title)
                            } else {
                                a.meta.title.clone()
                            };
                            if ui.button(title).clicked()
                                && let Ok(id) = CommandId::new(&a.command)
                            {
                                let (args, _) = template::expand(&a.args, &values);
                                app.run(id, args);
                            }
                        }
                        Item::Status(st) => {
                            let shown = match &st.source {
                                egui_pop_model::admin::StatusSource::Hook { hook } => {
                                    admin.hooks().status(hook)
                                }
                                egui_pop_model::admin::StatusSource::Command { .. } => None,
                            };
                            ui.horizontal(|ui| {
                                ui.label(&st.meta.title);
                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    match shown {
                                        Some((text, tone)) => {
                                            ui.add(Indicator::text(text).tone(kit_tone(tone)));
                                        }
                                        None => {
                                            ui.weak("—");
                                        }
                                    }
                                });
                            });
                        }
                        Item::Callout(c) => {
                            ui.add(Indicator::text(&c.text).tone(kit_tone(c.tone)));
                        }
                        Item::Widget(w) => match admin.widget(&w.kind) {
                            Some(draw) => draw(ui, app, &w.options),
                            None => {
                                ui.weak(strings::tr_args(
                                    &ctx,
                                    &txt::ADMIN_NO_WIDGET,
                                    &[("kind", &w.kind)],
                                ));
                            }
                        },
                    }
                }
            });
        ui.add_space(t.space.m);
    }
}

fn kit_tone(tone: Tone) -> KitTone {
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
    let focus_me = lock(&admin.state).key.as_deref() == Some(id);
    let row = ui
        .horizontal(|ui| {
            let h = ui.spacing().interact_size.y;
            let (bar, _) = ui.allocate_exact_size(egui::vec2(3.0, h), egui::Sense::hover());
            if modified {
                ui.painter().rect_filled(bar, 1.0, t.accent.default);
            }
            let read_only = origin == ValueScope::Run;
            let tail = 260.0;
            ui.add_enabled_ui(!read_only, |ui| {
                ui.allocate_ui(
                    egui::vec2((ui.available_width() - tail).max(160.0), 0.0),
                    |ui| {
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
                    },
                )
                .response
                .on_disabled_hover_text(strings::tr(&ctx, &txt::ADMIN_RUN_READ_ONLY));
            });
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
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
            });
        })
        .response;
    let mut name = d.label.clone();
    if modified {
        name = format!("{name}, {}", strings::tr(&ctx, &txt::ADMIN_MODIFIED));
    }
    if origin != ValueScope::App {
        name = format!("{name}, {}", origin_text(&ctx, origin));
    }
    a11y::name(&row, Role::Group, &name);
    if focus_me {
        row.scroll_to_me(Some(Align::Center));
        lock(&admin.state).key = None;
    }
    let help = if f.help.is_empty() {
        d.description.clone()
    } else {
        f.help.clone()
    };
    if !help.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(3.0 + ui.spacing().item_spacing.x);
            crate::links::Inline::new(&help).weak().show(ui, app);
        });
    }
    ui.add_space(t.space.s);
}
