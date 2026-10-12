//! The settings window, as a plugin ([`SettingsPlugin`], in
//! `DefaultPlugins`): a search field over a tree of sections on one side
//! and the form on the other, generated from the declared settings and
//! applied as they change, each with its reset; and a shortcuts editor
//! (every command with its keys, changed by pressing the new ones, the
//! conflicts said, each change taken back by its reset).
//!
//! `Ctrl+,` (or File > Settings) opens it; Escape closes it (or stops a
//! capture of keys).

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
use egui_pop_model::keymap::{Action, Keys};
use egui_pop_model::values::{Change, Set};
use serde_json::json;

use crate::app::{AppHandle, MENU_FILE};
use crate::hooks::SettingEntry;
use crate::services::ServiceToken;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// Opens and closes the settings window.
pub const CMD_SETTINGS: CommandId = CommandId::from_static("app.settings");

/// The settings window's state, as a service.
pub const SETTINGS_WINDOW: ServiceToken<SettingsWindow> = ServiceToken::new("app.settings_window");

/// The section holding the shortcuts editor.
pub const SHORTCUTS: &str = "Keyboard shortcuts";

#[derive(Debug, Default)]
struct State {
    open: bool,
    filter: String,
    /// The section shown when there is no filter.
    section: Option<String>,
    /// The command whose new keys are awaited.
    capture: Option<CommandId>,
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

/// Whether a setting matches the search (label, description, section).
fn matches(e: &SettingEntry, filter: &str) -> bool {
    let text = format!(
        "{} {} {}",
        e.section, e.descriptor.label, e.descriptor.description
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
        let capturing = w.capturing().is_some();
        if !capturing && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            close = true;
            return;
        }
        let entries = app.settings();
        let mut s = lock(&w.0);
        ui.horizontal(|ui| {
            let title = ui.heading("Settings");
            let search = ui
                .add(
                    egui::TextEdit::singleline(&mut s.filter)
                        .hint_text("Search settings and shortcuts")
                        .desired_width(360.0),
                )
                .labelled_by(title.id);
            if search.changed() {
                s.capture = None;
            }
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
                        usize::from(filter.is_empty() || !shortcut_rows(app, &filter).is_empty())
                    } else {
                        entries
                            .iter()
                            .filter(|e| {
                                &e.section == sec && (filter.is_empty() || matches(e, &filter))
                            })
                            .count()
                    };
                    let enabled = filter.is_empty() || hits > 0;
                    let r = ui.add_enabled(
                        enabled,
                        egui::Button::selectable(
                            filter.is_empty() && *sec == current,
                            sec.as_str(),
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
                            ui.strong(&current);
                            ui.add_space(t.space.s);
                            if current == SHORTCUTS {
                                shortcuts(ui, app, &mut s.capture, "");
                            } else {
                                for e in entries.iter().filter(|e| e.section == current) {
                                    row(ui, app, e);
                                }
                            }
                        } else {
                            let mut any = false;
                            for sec in &secs {
                                if sec == SHORTCUTS {
                                    if !shortcut_rows(app, &filter).is_empty() {
                                        any = true;
                                        ui.strong(sec);
                                        shortcuts(ui, app, &mut s.capture, &filter);
                                    }
                                    continue;
                                }
                                let list: Vec<&SettingEntry> = entries
                                    .iter()
                                    .filter(|e| &e.section == sec && matches(e, &filter))
                                    .collect();
                                if list.is_empty() {
                                    continue;
                                }
                                any = true;
                                ui.strong(sec);
                                for e in list {
                                    row(ui, app, e);
                                }
                                ui.add_space(t.space.m);
                            }
                            if !any {
                                ui.weak("Nothing matches.");
                            }
                        }
                    });
                });
        });
        app.capture_keys(s.capture.is_some());
    });
    if close || !open {
        w.close();
        app.capture_keys(false);
        overlay::close(&ctx, spec.id);
    }
}

/// One setting: its editor bound to the store, its reset, its
/// description.
fn row(ui: &mut Ui, app: &AppHandle, e: &SettingEntry) {
    let store = app.store();
    let t = Tokens::of(ui.ctx());
    ui.horizontal(|ui| {
        let reset_width = 64.0;
        ui.allocate_ui(
            egui::vec2(ui.available_width() - reset_width - t.space.m, 0.0),
            |ui| param::param(ui, &e.descriptor, Mode::Bound(&store, &e.path)),
        );
        let changed = lock(&store)
            .get(&e.path)
            .is_some_and(|x| x.value != x.descriptor.default);
        let reset = strings::tr(ui.ctx(), &RESET);
        let r = ui
            .add_enabled(changed, egui::Button::new(&reset).small())
            .on_disabled_hover_text(format!("{} is at its default", e.descriptor.label));
        a11y::name(
            &r,
            WidgetType::Button,
            &format!("{reset} {}", e.descriptor.label),
        );
        if r.clicked() {
            let _ = lock(&store).set(Set::new(&e.path, Change::Reset, Origin::User));
        }
    });
    if !e.descriptor.description.is_empty() {
        ui.weak(&e.descriptor.description);
    }
    ui.add_space(t.space.s);
}

/// The commands the shortcuts editor lists for a search: visible ones
/// and those with keys, as `Category: Title`.
fn shortcut_rows(app: &AppHandle, filter: &str) -> Vec<(CommandSpec, String)> {
    let mut rows: Vec<(CommandSpec, String)> = app
        .commands()
        .into_iter()
        .map(|(s, _)| s)
        .filter(|s| s.palette || !app.keys_for(&s.id).is_empty())
        .map(|s| {
            let label = format!("{}: {}", s.category, s.title);
            (s, label)
        })
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

/// The shortcuts editor.
fn shortcuts(ui: &mut Ui, app: &AppHandle, capture: &mut Option<CommandId>, filter: &str) {
    let t = Tokens::of(ui.ctx());
    if let Some(c) = capture.clone() {
        let title = app.command(&c).map(|s| s.title).unwrap_or_default();
        ui.colored_label(
            t.accent.default,
            format!("Press the new keys for {title}. Escape: cancel; Backspace: no key."),
        );
        // The first key pressed, with its modifiers.
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        match pressed {
            Some((Key::Escape, m)) if !m.any() => *capture = None,
            Some((Key::Backspace, m)) if !m.any() => {
                app.rebind(c, None);
                *capture = None;
            }
            Some((key, modifiers)) => {
                let keys = Keys(vec![chord_of(key, modifiers)]).to_string();
                app.rebind(c, Some(keys));
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
            for (spec, label) in shortcut_rows(app, filter) {
                let name = ui.label(&label);
                let keys = app.keys_for(&spec.id);
                ui.monospace(if keys.is_empty() {
                    "—".to_string()
                } else {
                    keys.join(", ")
                })
                .labelled_by(name.id);
                // A binding of this command losing to another, or beating one.
                let clash = conflicts.iter().find_map(|(lose, win)| {
                    let mine = |b: &egui_pop_model::keymap::Binding| {
                        matches!(&b.action, Action::Run(c) if *c == spec.id)
                    };
                    let other = |b: &egui_pop_model::keymap::Binding| match &b.action {
                        Action::Run(c) => app.command(c).map_or(c.to_string(), |s| s.title),
                        _ => String::new(),
                    };
                    if mine(lose) {
                        Some(format!("{} goes to {}", lose.keys, other(win)))
                    } else if mine(win) {
                        Some(format!("{} also bound to {}", win.keys, other(lose)))
                    } else {
                        None
                    }
                });
                match clash {
                    Some(c) => {
                        ui.colored_label(t.status.warning, c);
                    }
                    None => {
                        ui.label("");
                    }
                }
                ui.horizontal(|ui| {
                    let change = ui.small_button("Change");
                    a11y::name(
                        &change,
                        WidgetType::Button,
                        &format!("Change the keys of {label}"),
                    );
                    if change.clicked() {
                        *capture = Some(spec.id.clone());
                    }
                    if app.rebound(&spec.id) {
                        let reset = ui.small_button(strings::tr(ui.ctx(), &RESET));
                        a11y::name(
                            &reset,
                            WidgetType::Button,
                            &format!("Default keys of {label}"),
                        );
                        if reset.clicked() {
                            app.reset_keys(spec.id.clone());
                        }
                    }
                });
                ui.end_row();
            }
        });
}
