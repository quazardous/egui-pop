//! The settings window, as a plugin ([`SettingsPlugin`], in
//! `DefaultPlugins`): a search field over a tree of sections on one side
//! and the form on the other, generated from the declared settings and
//! applied as they change, each with its reset; and a shortcuts editor
//! (every command with its keys, changed by pressing the new ones, the
//! conflicts said, each change taken back by its reset).
//!
//! `Ctrl+,` (or File > Settings) opens it; Escape closes it (or stops a
//! capture of keys).
//!
//! The settings have an undo of their own, the store's (`value.undo`,
//! `value.redo`), with its buttons in the window: Edit > Undo and
//! `Ctrl+Z` undo the application's changes, through the host.

use std::sync::{Arc, Mutex};

use egui::{Key, Ui, WidgetType};
use egui_pop_core::a11y;
use egui_pop_core::keys::chord_of;
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Kind, OverlaySpec};
use egui_pop_core::param::{self, Mode};
use egui_pop_core::strings::{self, RESET};
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::fuzzy;
use egui_pop_model::ids::{BlockId, CommandId, PluginId};
use egui_pop_model::keymap::{Action, Keys, Layer};
use egui_pop_model::values::{Change, Set};
use serde_json::json;

use crate::app::{AppHandle, MENU_FILE};
use crate::hooks::SettingEntry;
use crate::services::ServiceToken;
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// Opens and closes the settings window.
pub const CMD_SETTINGS: CommandId = CommandId::from_static("app.settings");

/// The settings window's state, as a service.
pub const SETTINGS_WINDOW: ServiceToken<SettingsWindow> = ServiceToken::new("app.settings_window");

/// The store's own undo and redo, which the settings window's buttons
/// run.
pub const CMD_VALUE_UNDO: CommandId = CommandId::from_static("value.undo");
pub const CMD_VALUE_REDO: CommandId = CommandId::from_static("value.redo");

/// The section holding the shortcuts editor, as [`SettingsWindow::open`]
/// takes it (shown translated).
pub const SHORTCUTS: &str = txt::SETTINGS_SHORTCUTS.fallback;

#[derive(Debug, Default)]
struct State {
    open: bool,
    filter: String,
    /// The section shown when there is no filter.
    section: Option<String>,
    /// The command whose new keys are awaited.
    capture: Option<CommandId>,
    /// Waiting for keys to search by.
    capture_search: bool,
    /// The keys searched for (`Ctrl+K`), exactly.
    key_filter: Option<String>,
    /// Where keys changed go: the user's layer, or the workspace's when
    /// the application offers it.
    workspace_target: bool,
}

/// Open or not, the search, the section shown.
#[derive(Clone, Debug, Default)]
pub struct SettingsWindow(Arc<Mutex<State>>);

impl SettingsWindow {
    pub fn is_open(&self) -> bool {
        lock(&self.0).open
    }

    /// Opens it, on a section if given.
    pub fn open(&self, section: Option<&str>) {
        let mut s = lock(&self.0);
        s.open = true;
        if let Some(sec) = section {
            s.section = Some(sec.into());
        }
    }

    pub fn close(&self) {
        let mut s = lock(&self.0);
        s.open = false;
        s.capture = None;
        s.capture_search = false;
        s.key_filter = None;
        s.filter.clear();
    }

    /// The command whose new keys are awaited.
    pub fn capturing(&self) -> Option<CommandId> {
        lock(&self.0).capture.clone()
    }
}

/// Whether the application's settings window is open.
pub fn is_open(app: &AppHandle) -> bool {
    app.service(&SETTINGS_WINDOW).is_some_and(|w| w.is_open())
}

/// The plugin.
#[derive(Debug, Default)]
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.settings")
    }

    fn title(&self) -> String {
        "Settings".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.service(&SETTINGS_WINDOW, SettingsWindow::default())?;
        ctx.command(
            CommandSpec::new(CMD_SETTINGS, "Settings", "File")
                .description("Every setting, searchable, and the keyboard shortcuts.")
                .key("Ctrl+Comma")
                .menu(MENU_FILE, "8_settings", 0)
                .args(json!({ "section": "" })),
            |call| {
                let w = call
                    .app
                    .service(&SETTINGS_WINDOW)
                    .ok_or("no settings window")?;
                if w.is_open() {
                    w.close();
                } else {
                    let section = call.args.get("section").and_then(|s| s.as_str());
                    w.open(section.filter(|s| !s.is_empty()));
                }
                Ok(json!({ "open": w.is_open() }))
            },
        )?;
        ctx.block(
            zones::OVERLAYS,
            Block::add(BlockId::from_static("egui_pop.settings"), 5, show),
        );
        Ok(())
    }
}

/// The sections, in order: the settings' own, then the shortcuts.
fn sections(entries: &[SettingEntry]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in entries {
        if !out.contains(&e.section) {
            out.push(e.section.clone());
        }
    }
    out.push(SHORTCUTS.into());
    out
}

/// A section's title as shown, in the interface's language.
fn section_title(
    ctx: &egui::Context,
    app: &AppHandle,
    entries: &[SettingEntry],
    sec: &str,
) -> String {
    if sec == SHORTCUTS {
        return strings::tr(ctx, &txt::SETTINGS_SHORTCUTS);
    }
    entries.iter().find(|e| e.section == sec).map_or_else(
        || sec.to_string(),
        |e| app.section_title(ctx, &e.owner, sec),
    )
}

/// A setting's descriptor as shown: its label and description in the
/// interface's language.
fn shown(ctx: &egui::Context, e: &SettingEntry) -> egui_pop_model::values::Descriptor {
    let mut d = e.descriptor.clone();
    let id = e.id.as_str();
    d.label = strings::setting_label(ctx, id, &d.label);
    d.description = strings::setting_description(ctx, id, &d.description);
    d
}

/// Whether a setting matches the search (label, description, section,
/// as shown).
fn matches(ctx: &egui::Context, app: &AppHandle, e: &SettingEntry, filter: &str) -> bool {
    let d = shown(ctx, e);
    let text = format!(
        "{} {} {}",
        app.section_title(ctx, &e.owner, &e.section),
        d.label,
        d.description
    );
    filter
        .split_whitespace()
        .all(|w| fuzzy::score(w, &text).is_some())
}

/// Draws the window while it is open.
fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(w) = app.service(&SETTINGS_WINDOW) else {
        return;
    };
    if !w.is_open() {
        app.capture_keys(false);
        return;
    }
    let ctx = ui.ctx().clone();
    let spec = OverlaySpec::new("egui_pop.settings", "Settings", Kind::Modal)
        .width(860.0)
        .keep_on_escape();
    let mut open = true;
    let mut close = false;
    overlay::show(&ctx, &spec, &mut open, None, |ui| {
        let t = Tokens::of(ui.ctx());
        let capturing = w.capturing().is_some() || lock(&w.0).capture_search;
        if !capturing && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            close = true;
            return;
        }
        let entries = app.settings();
        let mut s = lock(&w.0);
        ui.horizontal(|ui| {
            let title = ui.heading(strings::tr(&ctx, &txt::SETTINGS_TITLE));
            let search = ui
                .add(
                    egui::TextEdit::singleline(&mut s.filter)
                        .hint_text(strings::tr(&ctx, &txt::SETTINGS_SEARCH_HINT))
                        .desired_width(360.0),
                )
                .labelled_by(title.id);
            if search.changed() {
                s.capture = None;
            }
            history(ui, app);
        });
        ui.separator();
        let secs = sections(&entries);
        let current = s
            .section
            .clone()
            .filter(|c| secs.contains(c))
            .unwrap_or_else(|| secs[0].clone());
        let filter = s.filter.trim().to_string();
        let height = (ui.ctx().content_rect().height() * 0.6).max(240.0);
        // The form's width: the window's, less the tree (a scroll area in
        // a modal would otherwise grow without end).
        let form_width = (ui.max_rect().width() - 220.0).max(240.0);
        ui.horizontal_top(|ui| {
            // The tree: sections, those with matches when searching.
            ui.vertical(|ui| {
                ui.set_width(200.0);
                for sec in &secs {
                    let hits = if sec == SHORTCUTS {
                        usize::from(
                            filter.is_empty()
                                || !shortcut_rows(&ctx, app, &filter, None).is_empty(),
                        )
                    } else {
                        entries
                            .iter()
                            .filter(|e| {
                                &e.section == sec
                                    && (filter.is_empty() || matches(&ctx, app, e, &filter))
                            })
                            .count()
                    };
                    let enabled = filter.is_empty() || hits > 0;
                    let r = ui.add_enabled(
                        enabled,
                        egui::Button::selectable(
                            filter.is_empty() && *sec == current,
                            section_title(&ctx, app, &entries, sec),
                        ),
                    );
                    if r.clicked() {
                        s.section = Some(sec.clone());
                        s.filter.clear();
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("egui_pop.settings.form")
                .max_height(height)
                .max_width(form_width)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(form_width - 16.0);
                    // Top-down here (the row around is left to right), so
                    // descriptions wrap.
                    ui.vertical(|ui| {
                        if filter.is_empty() {
                            ui.strong(section_title(&ctx, app, &entries, &current));
                            ui.add_space(t.space.s);
                            if current == SHORTCUTS {
                                let st = &mut *s;
                                shortcuts(ui, app, st, "");
                            } else {
                                for e in entries.iter().filter(|e| e.section == current) {
                                    row(ui, app, e);
                                }
                            }
                        } else {
                            let mut any = false;
                            for sec in &secs {
                                if sec == SHORTCUTS {
                                    if !shortcut_rows(&ctx, app, &filter, None).is_empty() {
                                        any = true;
                                        ui.strong(section_title(&ctx, app, &entries, sec));
                                        let st = &mut *s;
                                        shortcuts(ui, app, st, &filter);
                                    }
                                    continue;
                                }
                                let list: Vec<&SettingEntry> = entries
                                    .iter()
                                    .filter(|e| &e.section == sec && matches(&ctx, app, e, &filter))
                                    .collect();
                                if list.is_empty() {
                                    continue;
                                }
                                any = true;
                                ui.strong(section_title(&ctx, app, &entries, sec));
                                for e in list {
                                    row(ui, app, e);
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
        app.capture_keys(s.capture.is_some() || s.capture_search);
    });
    if close || !open {
        w.close();
        app.capture_keys(false);
        overlay::close(&ctx, spec.id);
    }
}

/// The settings' own undo and redo: the store's, not the application's.
fn history(ui: &mut Ui, app: &AppHandle) {
    let (undo, redo) = {
        let store = app.store();
        let s = lock(&store);
        (
            s.undo_steps().last().map(|st| st.label.clone()),
            s.redo_steps().last().map(|st| st.label.clone()),
        )
    };
    let ctx = ui.ctx().clone();
    for (label, step, command, with_step, nothing, name) in [
        (
            &txt::SETTINGS_UNDO,
            undo,
            CMD_VALUE_UNDO,
            &txt::SETTINGS_UNDO_STEP,
            &txt::SETTINGS_NOTHING_TO_UNDO,
            &txt::SETTINGS_UNDO_NAME,
        ),
        (
            &txt::SETTINGS_REDO,
            redo,
            CMD_VALUE_REDO,
            &txt::SETTINGS_REDO_STEP,
            &txt::SETTINGS_NOTHING_TO_REDO,
            &txt::SETTINGS_REDO_NAME,
        ),
    ] {
        let r = ui
            .add_enabled(
                step.is_some(),
                egui::Button::new(strings::tr(&ctx, label)).small(),
            )
            .on_hover_text(match &step {
                Some(st) => strings::tr_args(&ctx, with_step, &[("change", st)]),
                None => strings::tr(&ctx, nothing),
            });
        a11y::name(&r, WidgetType::Button, &strings::tr(&ctx, name));
        if r.clicked() {
            app.run(command, serde_json::Value::Null);
        }
    }
}

/// One setting: its editor bound to the store, its reset, its
/// description.
fn row(ui: &mut Ui, app: &AppHandle, e: &SettingEntry) {
    let store = app.store();
    let t = Tokens::of(ui.ctx());
    let ctx = ui.ctx().clone();
    let d = shown(&ctx, e);
    ui.horizontal(|ui| {
        let reset_width = 64.0;
        let id = e.id.as_str();
        ui.allocate_ui(
            egui::vec2(ui.available_width() - reset_width - t.space.m, 0.0),
            |ui| {
                param::param_shown(ui, &d, Mode::Bound(&store, &e.path), &|c| {
                    strings::setting_choice(&ctx, id, c)
                })
            },
        );
        let changed = lock(&store)
            .get(&e.path)
            .is_some_and(|x| x.value != x.descriptor.default);
        let reset = strings::tr(ui.ctx(), &RESET);
        let setting = [("setting", d.label.as_str())];
        let r = ui
            .add_enabled(changed, egui::Button::new(&reset).small())
            .on_disabled_hover_text(strings::tr_args(
                ui.ctx(),
                &txt::SETTINGS_AT_DEFAULT,
                &setting,
            ));
        a11y::name(
            &r,
            WidgetType::Button,
            &strings::tr_args(ui.ctx(), &txt::SETTINGS_RESET_SETTING, &setting),
        );
        if r.clicked() {
            let _ = lock(&store).set(Set::new(&e.path, Change::Reset, Origin::User));
        }
    });
    if !d.description.is_empty() {
        ui.weak(&d.description);
    }
    ui.add_space(t.space.s);
}

/// The commands the shortcuts editor lists for a search (and, when
/// searching by key, those bound to exactly these keys): visible ones and
/// those with keys, as `Category: Title` in the interface's language.
fn shortcut_rows(
    ctx: &egui::Context,
    app: &AppHandle,
    filter: &str,
    keys: Option<&str>,
) -> Vec<(CommandSpec, String)> {
    let mut rows: Vec<(CommandSpec, String)> = app
        .commands()
        .into_iter()
        .map(|(s, _)| s)
        .filter(|s| s.palette || !app.keys_for(&s.id).is_empty())
        .map(|s| {
            let label = strings::tr_args(
                ctx,
                &txt::COMMAND_LABEL,
                &[
                    ("category", &strings::category(ctx, &s.category)),
                    ("title", &command_title(ctx, &s)),
                ],
            );
            (s, label)
        })
        .filter(|(s, _)| keys.is_none_or(|k| app.keys_for(&s.id).iter().any(|x| x == k)))
        .filter(|(s, label)| {
            filter.is_empty()
                || filter.split_whitespace().all(|w| {
                    fuzzy::score(w, label).is_some()
                        || app
                            .keys_for(&s.id)
                            .iter()
                            .any(|k| k.to_lowercase().contains(&w.to_lowercase()))
                })
        })
        .collect();
    rows.sort_by(|a, b| a.1.cmp(&b.1));
    rows
}

/// A command's title as shown.
fn command_title(ctx: &egui::Context, spec: &CommandSpec) -> String {
    strings::command_title(ctx, spec.id.as_str(), &spec.title)
}

/// A command's title as shown, by id (the id when not declared).
fn title_of(ctx: &egui::Context, app: &AppHandle, id: &CommandId) -> String {
    app.command(id)
        .map_or_else(|| id.to_string(), |s| command_title(ctx, &s))
}

/// What the command's system-scope keys do outside the window, and
/// whether that is as declared (`false`: they only work inside).
fn system_note(ctx: &egui::Context, app: &AppHandle, id: &CommandId) -> Option<(String, bool)> {
    use crate::system_keys::{KeyStatus, SYSTEM_KEYS, Support};
    let keys: Vec<String> = app
        .system_bindings()
        .into_iter()
        .filter(|b| matches!(&b.action, Action::Run(c) if c == id))
        .map(|b| b.keys.to_string())
        .collect();
    if keys.is_empty() {
        return None;
    }
    let shown = keys.join(", ");
    let Some(service) = app.service(&SYSTEM_KEYS) else {
        return Some((
            strings::tr_args(ctx, &txt::SHORTCUTS_SYSTEM_NO_PLUGIN, &[("keys", &shown)]),
            false,
        ));
    };
    if let Support::Unavailable { reason } = service.support() {
        return Some((
            strings::tr_args(
                ctx,
                &txt::SHORTCUTS_SYSTEM_UNAVAILABLE,
                &[("keys", &shown), ("reason", &reason)],
            ),
            false,
        ));
    }
    // The first binding not registered says why; else all are.
    for k in &keys {
        match service.status(k) {
            Some(KeyStatus::Registered) => {}
            Some(KeyStatus::Failed(reason)) => {
                return Some((
                    strings::tr_args(
                        ctx,
                        &txt::SHORTCUTS_SYSTEM_FAILED,
                        &[("keys", k), ("reason", &reason)],
                    ),
                    false,
                ));
            }
            Some(KeyStatus::Pending) | None => {
                return Some((
                    strings::tr_args(ctx, &txt::SHORTCUTS_SYSTEM_PENDING, &[("keys", k)]),
                    true,
                ));
            }
        }
    }
    Some((
        strings::tr_args(ctx, &txt::SHORTCUTS_SYSTEM_ON, &[("keys", &shown)]),
        true,
    ))
}

/// The first key pressed this frame, with its modifiers, as keys.
fn pressed_keys(ui: &Ui) -> Option<(Key, egui::Modifiers)> {
    ui.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => Some((*key, *modifiers)),
            _ => None,
        })
    })
}

/// The shortcuts editor.
fn shortcuts(ui: &mut Ui, app: &AppHandle, st: &mut State, filter: &str) {
    let t = Tokens::of(ui.ctx());
    let ctx = ui.ctx().clone();
    // Search by key: the next keys pressed.
    ui.horizontal(|ui| {
        if st.capture_search {
            ui.colored_label(
                t.accent.default,
                strings::tr(&ctx, &txt::SHORTCUTS_PRESS_TO_SEARCH),
            );
            match pressed_keys(ui) {
                Some((Key::Escape, m)) if !m.any() => st.capture_search = false,
                Some((key, modifiers)) => {
                    st.key_filter = Some(Keys(vec![chord_of(key, modifiers)]).to_string());
                    st.capture_search = false;
                }
                None => {}
            }
            ui.input_mut(|i| {
                i.events
                    .retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_)))
            });
        } else if ui
            .button(strings::tr(&ctx, &txt::SHORTCUTS_SEARCH_BY_KEY))
            .clicked()
        {
            st.capture_search = true;
            st.capture = None;
        }
        if let Some(k) = st.key_filter.clone() {
            ui.label(strings::tr_args(
                &ctx,
                &txt::SHORTCUTS_BOUND_TO,
                &[("keys", &k)],
            ));
            let clear = ui.small_button(strings::tr(&ctx, &txt::SHORTCUTS_SHOW_ALL));
            if clear.clicked() {
                st.key_filter = None;
            }
        }
    });
    // Conflicts, by scope.
    let by_scope = app.key_conflicts_by_scope();
    if !by_scope.is_empty() {
        ui.add_space(t.space.s);
        ui.strong(strings::tr(&ctx, &txt::SHORTCUTS_CONFLICTS));
        for (scope, list) in &by_scope {
            for (lose, win) in list {
                let name = |b: &egui_pop_model::keymap::Binding| match &b.action {
                    Action::Run(c) => title_of(&ctx, app, c),
                    _ => String::new(),
                };
                ui.colored_label(
                    t.status.warning,
                    strings::tr_args(
                        &ctx,
                        &txt::SHORTCUTS_CONFLICT_IN_SCOPE,
                        &[
                            ("keys", &lose.keys.to_string()),
                            ("scope", &scope.to_string()),
                            ("loser", &name(lose)),
                            ("winner", &name(win)),
                        ],
                    ),
                );
            }
        }
        ui.add_space(t.space.s);
    }
    // Where the keys changed go, when the application keeps a
    // workspace's keys.
    if app.workspace_keys_offered() {
        ui.horizontal(|ui| {
            ui.label(strings::tr(&ctx, &txt::SHORTCUTS_TARGET));
            ui.radio_value(
                &mut st.workspace_target,
                false,
                strings::tr(&ctx, &txt::SHORTCUTS_TARGET_USER),
            );
            ui.radio_value(
                &mut st.workspace_target,
                true,
                strings::tr(&ctx, &txt::SHORTCUTS_TARGET_WORKSPACE),
            );
        });
        ui.add_space(t.space.s);
    } else {
        st.workspace_target = false;
    }
    let target = if st.workspace_target {
        Layer::Workspace
    } else {
        Layer::User
    };
    let capture = &mut st.capture;
    let key_filter = st.key_filter.clone();
    if let Some(c) = capture.clone() {
        let title = app
            .command(&c)
            .map(|s| command_title(&ctx, &s))
            .unwrap_or_default();
        ui.colored_label(
            t.accent.default,
            strings::tr_args(&ctx, &txt::SHORTCUTS_PRESS_NEW_KEYS, &[("command", &title)]),
        );
        match pressed_keys(ui) {
            Some((Key::Escape, m)) if !m.any() => *capture = None,
            Some((Key::Backspace, m)) if !m.any() => {
                app.rebind_in(target, c, None);
                *capture = None;
            }
            Some((key, modifiers)) => {
                let keys = Keys(vec![chord_of(key, modifiers)]).to_string();
                app.rebind_in(target, c, Some(keys));
                *capture = None;
            }
            None => {}
        }
        ui.input_mut(|i| {
            i.events
                .retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_)))
        });
        ui.add_space(t.space.s);
    }
    let conflicts = app.key_conflicts();
    egui::Grid::new("egui_pop.shortcuts")
        .num_columns(4)
        .striped(true)
        .spacing([t.space.m, t.space.xs])
        .show(ui, |ui| {
            for (spec, label) in shortcut_rows(&ctx, app, filter, key_filter.as_deref()) {
                let name = ui.label(&label);
                let keys = app.keys_for(&spec.id);
                // Each key with where it comes from when it is not a default
                // (workspace, user, or the application's own names).
                let sources = app.key_sources(&spec.id);
                ui.monospace(if sources.is_empty() {
                    "—".to_string()
                } else {
                    sources
                        .iter()
                        .map(|b| match app.key_source_name(&ctx, b) {
                            Some(source) => strings::tr_args(
                                &ctx,
                                &txt::SHORTCUTS_KEY_FROM,
                                &[("keys", &b.keys.to_string()), ("source", &source)],
                            ),
                            None => b.keys.to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .labelled_by(name.id);
                // A binding of this command losing to another, or beating one.
                let clash = conflicts.iter().find_map(|(lose, win)| {
                    let mine = |b: &egui_pop_model::keymap::Binding| {
                        matches!(&b.action, Action::Run(c) if *c == spec.id)
                    };
                    let other = |b: &egui_pop_model::keymap::Binding| match &b.action {
                        Action::Run(c) => title_of(&ctx, app, c),
                        _ => String::new(),
                    };
                    if mine(lose) {
                        Some(strings::tr_args(
                            &ctx,
                            &txt::SHORTCUTS_GOES_TO,
                            &[("keys", &lose.keys.to_string()), ("command", &other(win))],
                        ))
                    } else if mine(win) {
                        Some(strings::tr_args(
                            &ctx,
                            &txt::SHORTCUTS_ALSO_BOUND,
                            &[("keys", &win.keys.to_string()), ("command", &other(lose))],
                        ))
                    } else {
                        None
                    }
                });
                let system = system_note(&ctx, app, &spec.id);
                match (clash, system) {
                    (None, None) => {
                        ui.label("");
                    }
                    (clash, system) => {
                        ui.vertical(|ui| {
                            if let Some(c) = clash {
                                ui.colored_label(t.status.warning, c);
                            }
                            if let Some((text, ok)) = system {
                                if ok {
                                    ui.label(text);
                                } else {
                                    ui.colored_label(t.status.warning, text);
                                }
                            }
                        });
                    }
                }
                ui.horizontal(|ui| {
                    let command = [("command", label.as_str())];
                    let change = ui.small_button(strings::tr(&ctx, &txt::SHORTCUTS_CHANGE));
                    a11y::name(
                        &change,
                        WidgetType::Button,
                        &strings::tr_args(&ctx, &txt::SHORTCUTS_CHANGE_NAME, &command),
                    );
                    if change.clicked() {
                        *capture = Some(spec.id.clone());
                    }
                    if !keys.is_empty() {
                        let none = ui.small_button(strings::tr(&ctx, &txt::SHORTCUTS_NO_KEY));
                        a11y::name(
                            &none,
                            WidgetType::Button,
                            &strings::tr_args(&ctx, &txt::SHORTCUTS_NO_KEY_NAME, &command),
                        );
                        if none.clicked() {
                            app.rebind_in(target, spec.id.clone(), None);
                        }
                    }
                    if app.rebound_in(target, &spec.id) {
                        let reset = ui.small_button(strings::tr(ui.ctx(), &RESET));
                        a11y::name(
                            &reset,
                            WidgetType::Button,
                            &strings::tr_args(&ctx, &txt::SHORTCUTS_DEFAULT_NAME, &command),
                        );
                        if reset.clicked() {
                            app.reset_keys_in(target, spec.id.clone());
                        }
                    }
                });
                ui.end_row();
            }
        });
}
