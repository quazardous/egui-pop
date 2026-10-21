//! The settings window, as a plugin ([`SettingsPlugin`], in
//! `DefaultPlugins`): a search field over a tree of sections on one side
//! and the form on the other, generated from the declared settings and
//! applied as they change, each with its reset; and a shortcuts editor
//! (every command with its keys, changed by pressing the new ones, the
//! conflicts said, each change taken back by its reset). Pointer
//! gestures are there too (`docs/pointer-gestures.md`): the profile in
//! force and how many of its gestures the user changed, a filter
//! showing them alone, each gesture's triggers removed one by one or
//! added by doing the gesture on a pad.
//!
//! The Fonts page, under Appearance, holds the text settings
//! (`text`): the text size, a family for the interface and one for
//! numbers and code, picked among the families there are
//! (`egui_pop_core::fonts::families`), and a preview, which draws the
//! family the pointer rests on in the picker before it is picked.
//!
//! Plugins add pages of their own next to the generated ones
//! ([`crate::settings_pages`]): drawn by the plugin, placed in the tree,
//! found by the search, with their reset.
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
use egui_pop_core::fonts::{self, Origin as FaceOrigin, Role};
use egui_pop_core::keys::chord_of;
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Kind, OverlaySpec};
use egui_pop_core::param::{self, Mode};
use egui_pop_core::state::Slot;
use egui_pop_core::strings::{self, RESET};
use egui_pop_core::theme::Tokens;
use egui_pop_core::ui::UiPop as _;
use egui_pop_core::widgets;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::fuzzy;
use egui_pop_model::ids::{BlockId, CommandId, OwnerId, PluginId};
use egui_pop_model::keymap::{Action, Keys, Layer, Pointer};
use egui_pop_model::values::{Change, Set};
use serde_json::json;

use crate::app::{AppHandle, MENU_FILE};
use crate::hooks::SettingEntry;
use crate::services::ServiceToken;
use crate::settings_pages::{self, SettingsPage};
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// Opens and closes the settings window.
pub const CMD_SETTINGS: CommandId = CommandId::from_static("app.settings");

/// Opens the settings window on a custom page (`{"page": "<id>"}`).
pub const CMD_SETTINGS_PAGE: CommandId = CommandId::from_static("app.settings.page");

/// The settings window's state, as a service.
pub const SETTINGS_WINDOW: ServiceToken<SettingsWindow> = ServiceToken::new("app.settings_window");

/// The store's own undo and redo, which the settings window's buttons
/// run.
pub const CMD_VALUE_UNDO: CommandId = CommandId::from_static("value.undo");
pub const CMD_VALUE_REDO: CommandId = CommandId::from_static("value.redo");

/// The section holding the shortcuts editor, as [`SettingsWindow::open`]
/// takes it (shown translated).
pub const SHORTCUTS: &str = txt::SETTINGS_SHORTCUTS.fallback;

/// The section of the text settings, under Appearance (shown
/// translated): the text size, the families and a preview.
pub const FONTS: &str = txt::SETTINGS_FONTS.fallback;

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
    /// Only the pointer gestures.
    pointer_only: bool,
    /// The gesture command a new gesture is awaited for, and the one
    /// done on the pad so far.
    pointer_capture: Option<(CommandId, Option<String>)>,
}

/// Open or not, the search, the section shown.
#[derive(Clone, Debug, Default)]
pub struct SettingsWindow(Arc<Mutex<State>>);

impl SettingsWindow {
    pub fn is_open(&self) -> bool {
        lock(&self.0).open
    }

    /// Opens it, on a section (as the tree names it) or a custom page
    /// (its id) if given.
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
        s.pointer_capture = None;
        s.key_filter = None;
        s.filter.clear();
    }

    /// The command whose new keys are awaited.
    pub fn capturing(&self) -> Option<CommandId> {
        lock(&self.0).capture.clone()
    }

    /// The section or custom page asked for last (shown when there is
    /// no search), and the search.
    pub fn shown(&self) -> (Option<String>, String) {
        let s = lock(&self.0);
        (s.section.clone(), s.filter.clone())
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
        ctx.command(
            CommandSpec::new(CMD_SETTINGS_PAGE, "Settings page", "File")
                .description("Opens the settings window on a custom page (its id).")
                .hidden()
                .args(json!({ "page": "" })),
            |call| {
                let page = call
                    .args
                    .get("page")
                    .and_then(|s| s.as_str())
                    .ok_or("`page` is missing")?;
                if call.app.settings_page(page).is_none() {
                    return Err(format!("no settings page `{page}`"));
                }
                let w = call
                    .app
                    .service(&SETTINGS_WINDOW)
                    .ok_or("no settings window")?;
                w.open(Some(page));
                lock(&w.0).filter.clear();
                Ok(json!({ "open": true, "page": page }))
            },
        )?;
        ctx.block(
            zones::OVERLAYS,
            Block::add(BlockId::from_static("egui_pop.settings"), 5, show),
        );
        let w = ctx.app().downgrade();
        ctx.collector(
            egui_pop_core::debug::FnCollector::new(
                "settings_window",
                "The settings window: open or not, the section or page shown, the search, the tree (sections and custom pages) and the custom pages (id, title, parent, order, keywords, settings, component, reset, changed). `app.settings.page` opens a page.",
            )
            .snapshot(move |ctx| match w.get() {
                Ok(app) => describe(ctx, &app),
                Err(_) => serde_json::Value::Null,
            }),
        )?;
        Ok(())
    }
}

/// What the `settings_window` collector shows.
fn describe(ctx: &egui::Context, app: &AppHandle) -> serde_json::Value {
    let (open, shown, filter) =
        app.service(&SETTINGS_WINDOW)
            .map_or((false, None, String::new()), |w| {
                let (sec, filter) = w.shown();
                (w.is_open(), sec, filter)
            });
    let all = app.settings();
    let pages = app.settings_pages();
    let entries = settings_pages::unclaimed(&all, &pages);
    let tree: Vec<serde_json::Value> = settings_pages::tree(&sections(&entries), &pages)
        .into_iter()
        .map(|n| {
            json!({
                "key": n.key,
                "title": node_title(ctx, app, &entries, &pages, &n.key),
                "depth": n.depth,
                "page": n.page,
            })
        })
        .collect();
    json!({
        "open": open,
        "section": shown,
        "filter": filter,
        "tree": tree,
        "pages": settings_pages::describe(app),
    })
}

/// A line of the tree as shown: a custom page's title, or the section's.
fn node_title(
    ctx: &egui::Context,
    app: &AppHandle,
    entries: &[SettingEntry],
    pages: &[SettingsPage],
    key: &str,
) -> String {
    match pages.iter().find(|p| p.id == key) {
        Some(p) => p.title.clone(),
        None => section_title(ctx, app, entries, key),
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
    if sec == FONTS {
        return strings::tr(ctx, &txt::SETTINGS_FONTS);
    }
    entries.iter().find(|e| e.section == sec).map_or_else(
        || sec.to_string(),
        |e| app.section_title(ctx, &e.owner, sec),
    )
}

/// A setting's descriptor as shown: its label and description in the
/// interface's language.
pub(crate) fn shown(ctx: &egui::Context, e: &SettingEntry) -> egui_pop_model::values::Descriptor {
    let mut d = e.descriptor.clone();
    let id = e.id.as_str();
    d.label = strings::setting_label(ctx, id, &d.label);
    d.description = strings::setting_description(ctx, id, &d.description);
    d
}

/// Whether a setting matches the search (label, description, section,
/// as shown).
pub(crate) fn matches(
    ctx: &egui::Context,
    app: &AppHandle,
    e: &SettingEntry,
    filter: &str,
) -> bool {
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

/// Whether a custom page matches the search.
fn page_matches(
    ctx: &egui::Context,
    app: &AppHandle,
    entries: &[SettingEntry],
    all: &[SettingEntry],
    page: &SettingsPage,
    filter: &str,
) -> bool {
    let parent = page
        .parent
        .as_deref()
        .map(|p| match app.settings_page(p) {
            Some(parent) => parent.title,
            None => section_title(ctx, app, entries, p),
        })
        .unwrap_or_default();
    settings_pages::matches(ctx, page, &parent, all, filter)
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
        let capturing = w.capturing().is_some() || {
            let st = lock(&w.0);
            st.capture_search || st.pointer_capture.is_some()
        };
        if !capturing && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            close = true;
            return;
        }
        let all = app.settings();
        let pages = app.settings_pages();
        // The settings a custom page edits are drawn by the page.
        let entries = settings_pages::unclaimed(&all, &pages);
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
        let nodes = settings_pages::tree(&sections(&entries), &pages);
        let secs: Vec<String> = nodes.iter().map(|n| n.key.clone()).collect();
        let page_of = |key: &str| pages.iter().find(|p| p.id == key);
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
                for node in &nodes {
                    let sec = &node.key;
                    let hits = if let Some(page) = page_of(sec) {
                        usize::from(
                            filter.is_empty()
                                || page_matches(&ctx, app, &entries, &all, page, &filter),
                        )
                    } else if sec == SHORTCUTS {
                        usize::from(
                            filter.is_empty()
                                || !shortcut_rows(&ctx, app, &filter, None, false).is_empty(),
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
                    let item = widgets::SelectableRow::new(
                        filter.is_empty() && *sec == current,
                        node_title(&ctx, app, &entries, &pages, sec),
                    );
                    // A page under a parent, and the Fonts page (Appearance's),
                    // are indented.
                    let r = if node.depth > 0 || sec == FONTS {
                        ui.horizontal(|ui| {
                            ui.add_space(t.space.l);
                            ui.add_enabled(enabled, item)
                        })
                        .inner
                    } else {
                        ui.add_enabled(enabled, item)
                    };
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
                            ui.strong(node_title(&ctx, app, &entries, &pages, &current));
                            ui.add_space(t.space.s);
                            if let Some(page) = page_of(&current) {
                                settings_pages::header(ui, app, page);
                                ui.add_space(t.space.s);
                                settings_pages::draw(ui, app, &page.id);
                            } else if current == SHORTCUTS {
                                let st = &mut *s;
                                shortcuts(ui, app, st, "");
                            } else if current == FONTS {
                                fonts_page(ui, app, &entries);
                            } else {
                                for e in entries.iter().filter(|e| e.section == current) {
                                    row(ui, app, e);
                                }
                            }
                        } else {
                            let mut any = false;
                            let mut open_page = None;
                            for sec in &secs {
                                if let Some(page) = page_of(sec) {
                                    if page_matches(&ctx, app, &entries, &all, page, &filter) {
                                        any = true;
                                        if settings_pages::search_hit(ui, app, page, &all, &filter)
                                        {
                                            open_page = Some(page.id.clone());
                                        }
                                        ui.add_space(t.space.m);
                                    }
                                    continue;
                                }
                                if sec == SHORTCUTS {
                                    if !shortcut_rows(&ctx, app, &filter, None, false).is_empty() {
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
                            if let Some(id) = open_page {
                                s.section = Some(id);
                                s.filter.clear();
                            }
                        }
                    });
                });
        });
        app.capture_keys(s.capture.is_some() || s.capture_search || s.pointer_capture.is_some());
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

/// Draws a declared setting as the settings window does (its editor
/// bound to the store, its reset, its description), for a custom page;
/// `false` when no setting has that id.
pub fn setting_row(ui: &mut Ui, app: &AppHandle, id: &egui_pop_model::ids::SettingId) -> bool {
    match app.settings().into_iter().find(|e| e.id == *id) {
        Some(e) => {
            row(ui, app, &e);
            true
        }
        None => false,
    }
}

/// One setting: its editor bound to the store, its reset, its
/// description.
pub(crate) fn row(ui: &mut Ui, app: &AppHandle, e: &SettingEntry) {
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
        setting_reset_button(ui, app, e, &d.label);
    });
    if !d.description.is_empty() {
        ui.weak(&d.description);
    }
    ui.add_space(t.space.s);
}

/// Whether a command is a pointer gesture.
fn is_gesture(id: &CommandId) -> bool {
    egui_pop_model::pointer::gesture_spec(id).is_some()
}

/// A setting's reset, greyed at its default.
fn setting_reset_button(ui: &mut Ui, app: &AppHandle, e: &SettingEntry, label: &str) {
    let store = app.store();
    let changed = lock(&store)
        .get(&e.path)
        .is_some_and(|x| x.value != x.descriptor.default);
    let reset = strings::tr(ui.ctx(), &RESET);
    let setting = [("setting", label)];
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
        let _ = lock(&store)
            .set(Set::new(&e.path, Change::Reset, Origin::User).by(&OwnerId::FRAMEWORK));
    }
}

/// The Fonts page: the text size, a family per role picked among the
/// families there are, and a preview in the fonts installed, or in the
/// family the pointer rests on in a picker (the preview families).
fn fonts_page(ui: &mut Ui, app: &AppHandle, entries: &[SettingEntry]) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    for e in entries.iter().filter(|e| e.section == FONTS) {
        match Role::ALL
            .into_iter()
            .find(|r| crate::text::font_setting(*r) == e.id)
        {
            Some(role) => family_row(ui, app, e, role),
            None => row(ui, app, e),
        }
    }
    ui.add_space(t.space.m);
    ui.strong(strings::tr(&ctx, &txt::FONTS_PREVIEW));
    egui::Frame::new()
        .fill(t.surface.raised)
        .stroke(egui::Stroke::new(1.0, t.border.default))
        .corner_radius(egui::CornerRadius::same(
            t.radius.widget.round().clamp(0.0, 255.0) as u8,
        ))
        .inner_margin(egui::Margin::same(t.space.m.round().clamp(0.0, 127.0) as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            fonts::with_preview(ui, |ui| {
                ui.heading(strings::tr(&ctx, &txt::THEME_EDITOR_SAMPLE_HEADING));
                crate::theme_editor::font_sample(ui);
            });
            ui.add_space(t.space.s);
            ui.label(strings::tr(&ctx, &txt::FONTS_NUMBERS));
            let cell = egui::vec2(120.0, ui.spacing().interact_size.y);
            for n in ["0.00", "1.11", "-1234.56", "88.88"] {
                ui.allocate_ui_with_layout(
                    cell,
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        ui.label(fonts::number_job(&ctx, n, t.font.body, t.text.default));
                    },
                );
            }
        });
}

/// What an origin is called in the family picker.
fn origin_name(ctx: &egui::Context, origin: FaceOrigin) -> String {
    strings::tr(
        ctx,
        match origin {
            FaceOrigin::Bundled => &txt::FONTS_ORIGIN_BUNDLED,
            FaceOrigin::Registered => &txt::FONTS_ORIGIN_REGISTERED,
            FaceOrigin::File => &txt::FONTS_ORIGIN_FILE,
            FaceOrigin::System => &txt::FONTS_ORIGIN_SYSTEM,
        },
    )
}

/// The family each role's picker previews (`[ui, mono]`), as written
/// to the preview families last.
const FONT_PREVIEW: Slot<[Option<String>; 2]> = Slot::opaque("api.settings.font_preview");

/// A role's family: the theme's, or one of the families there are (only
/// monospace ones for numbers and code), each drawn in its own face but
/// the system's (loading every system font would cost too much).
fn family_row(ui: &mut Ui, app: &AppHandle, e: &SettingEntry, role: Role) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let d = shown(&ctx, e);
    let current = crate::text::font(app, role);
    let themes = strings::tr(&ctx, &txt::FONTS_THEMES);
    let mut pick: Option<Option<String>> = None;
    // The family the pointer rests on, previewed below; the preview
    // stays while the pointer moves between rows, and ends when it
    // leaves them (or the picker closes).
    let mut rested: Option<String> = None;
    let mut over_row = false;
    ui.horizontal(|ui| {
        let reset_width = 64.0;
        ui.allocate_ui(
            egui::vec2(ui.available_width() - reset_width - t.space.m, 0.0),
            |ui| {
                let l = ui.label(&d.label);
                let combo = widgets::combo_box(("egui_pop.fonts", e.id.as_str()))
                    .selected_text(current.clone().unwrap_or_else(|| themes.clone()));
                ui.combo_box(combo, |ui| {
                    if ui.selectable_label(current.is_none(), &themes).clicked() {
                        pick = Some(None);
                    }
                    let families = fonts::families(&ctx)
                        .into_iter()
                        .filter(|f| role == Role::Ui || f.monospace);
                    for f in families {
                        let mut text = egui::RichText::new(&f.name);
                        if f.origin != FaceOrigin::System
                            && let Some(family) =
                                fonts::specimen(&ctx, &f.name, fonts::Weight::REGULAR)
                        {
                            text = text.family(family);
                        }
                        let r = ui
                            .selectable_label(current.as_deref() == Some(f.name.as_str()), text)
                            .on_hover_text(origin_name(&ctx, f.origin));
                        over_row |= r.hovered();
                        if crate::theme_editor::rests_on(&r) {
                            rested = Some(f.name.clone());
                        }
                        if r.clicked() {
                            pick = Some(Some(f.name.clone()));
                        }
                    }
                })
                .response
                .labelled_by(l.id);
            },
        );
        setting_reset_button(ui, app, e, &d.label);
    });
    let at = usize::from(role == Role::Mono);
    let mut previewed = FONT_PREVIEW.get_or_default(&ctx);
    let want = if let Some(p) = pick {
        crate::text::set_font(app, role, p.as_deref());
        None
    } else if rested.is_some() {
        rested
    } else if !over_row {
        None
    } else {
        previewed[at].clone()
    };
    // Written when it changes only: the theme editor's fonts tab, open
    // at the same time, previews through the same families.
    if previewed[at] != want {
        fonts::set_preview(&ctx, role, want.clone().map(|n| vec![n]));
        previewed[at] = want;
        FONT_PREVIEW.set(&ctx, previewed);
    }
    if !d.description.is_empty() {
        ui.weak(&d.description);
    }
    ui.weak(strings::tr(&ctx, &txt::FONTS_PICKER_HINT));
    ui.add_space(t.space.s);
}

/// The commands the shortcuts editor lists for a search (and, when
/// searching by key, those bound to exactly these keys): visible ones,
/// those with keys and the pointer gestures (those alone with
/// `pointer_only`), as `Category: Title` in the interface's language.
fn shortcut_rows(
    ctx: &egui::Context,
    app: &AppHandle,
    filter: &str,
    keys: Option<&str>,
    pointer_only: bool,
) -> Vec<(CommandSpec, String)> {
    let mut rows: Vec<(CommandSpec, String)> = app
        .commands()
        .into_iter()
        .map(|(s, _)| s)
        .filter(|s| s.palette || !app.keys_for(&s.id).is_empty() || is_gesture(&s.id))
        .filter(|s| !pointer_only || is_gesture(&s.id))
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
            widgets::radio_value(
                ui,
                &mut st.workspace_target,
                false,
                strings::tr(&ctx, &txt::SHORTCUTS_TARGET_USER),
            );
            widgets::radio_value(
                ui,
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
    pointer_header(ui, app, st);
    pointer_pad(ui, app, st, target);
    let pointer_only = st.pointer_only;
    let mut pointer_capture: Option<CommandId> = None;
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
            for (spec, label) in
                shortcut_rows(&ctx, app, filter, key_filter.as_deref(), pointer_only)
            {
                let name = ui.label(&label);
                let keys = app.keys_for(&spec.id);
                let gesture = is_gesture(&spec.id);
                // Each key with where it comes from when it is not a default
                // (workspace, user, or the application's own names).
                let sources = app.key_sources(&spec.id);
                if gesture {
                    gesture_keys(ui, app, &ctx, &spec.id, &label, &keys, target, name.id);
                } else {
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
                }
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
                let system = system_note(&ctx, app, &spec.id)
                    .or_else(|| gesture.then(|| alt_note(&ctx, &keys)).flatten());
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
                    if gesture {
                        let add = ui.small_button(strings::tr(&ctx, &txt::SHORTCUTS_POINTER_ADD));
                        a11y::name(
                            &add,
                            WidgetType::Button,
                            &strings::tr_args(&ctx, &txt::SHORTCUTS_POINTER_ADD_NAME, &command),
                        );
                        if add.clicked() {
                            pointer_capture = Some(spec.id.clone());
                        }
                        reset_button(ui, app, &ctx, target, &spec.id, &command);
                        return;
                    }
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
    if let Some(id) = pointer_capture {
        st.pointer_capture = Some((id, None));
        st.capture = None;
    }
    widget_uses(ui, filter, key_filter.as_deref());
}

/// The widgets' own uses of the secondary button, seen or declared
/// (`egui_pop_core::pointer::claim`): listed, not changed here.
fn widget_uses(ui: &mut Ui, filter: &str, key_filter: Option<&str>) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let needle = filter.trim().to_lowercase();
    let uses: Vec<_> = egui_pop_core::pointer::claim::uses(&ctx)
        .into_iter()
        .filter(|u| key_filter.is_none_or(|k| k.eq_ignore_ascii_case(&u.gesture)))
        .filter(|u| {
            needle.is_empty()
                || u.widget.to_lowercase().contains(&needle)
                || u.does.to_lowercase().contains(&needle)
        })
        .collect();
    if uses.is_empty() {
        return;
    }
    ui.add_space(t.space.m);
    ui.strong(strings::tr(&ctx, &txt::SHORTCUTS_WIDGET_USES));
    ui.label(strings::tr(&ctx, &txt::SHORTCUTS_WIDGET_USES_NOTE));
    egui::Grid::new("egui_pop.shortcuts.widget_uses")
        .num_columns(3)
        .striped(true)
        .spacing([t.space.m, t.space.xs])
        .show(ui, |ui| {
            for u in &uses {
                let name = ui.label(&u.widget);
                ui.monospace(&u.gesture).labelled_by(name.id);
                ui.label(&u.does);
                ui.end_row();
            }
        });
}

/// The reset of a command's keys in `target`, when it changed them.
fn reset_button(
    ui: &mut Ui,
    app: &AppHandle,
    ctx: &egui::Context,
    target: Layer,
    id: &CommandId,
    command: &[(&str, &str)],
) {
    if app.rebound_in(target, id) {
        let reset = ui.small_button(strings::tr(ctx, &RESET));
        a11y::name(
            &reset,
            WidgetType::Button,
            &strings::tr_args(ctx, &txt::SHORTCUTS_DEFAULT_NAME, command),
        );
        if reset.clicked() {
            app.reset_keys_in(target, id.clone());
        }
    }
}

/// The pointer profile in force, how many of its gestures the user
/// changed, and the filter showing the gestures alone.
fn pointer_header(ui: &mut Ui, app: &AppHandle, st: &mut State) {
    let ctx = ui.ctx().clone();
    let profile = crate::pointer::profile(app);
    let changed = crate::pointer::overrides(app).len();
    ui.horizontal(|ui| {
        ui.label(if changed == 0 {
            strings::tr_args(
                &ctx,
                &txt::SHORTCUTS_POINTER_PROFILE,
                &[("profile", &profile.title)],
            )
        } else {
            strings::tr_args(
                &ctx,
                &txt::SHORTCUTS_POINTER_PROFILE_CHANGED,
                &[("profile", &profile.title), ("count", &changed.to_string())],
            )
        });
        widgets::checkbox(
            ui,
            &mut st.pointer_only,
            strings::tr(&ctx, &txt::SHORTCUTS_POINTER_ONLY),
        );
    });
}

/// A gesture's triggers, each with its removal.
#[allow(clippy::too_many_arguments)]
fn gesture_keys(
    ui: &mut Ui,
    app: &AppHandle,
    ctx: &egui::Context,
    id: &CommandId,
    label: &str,
    keys: &[String],
    target: Layer,
    name: egui::Id,
) {
    ui.horizontal_wrapped(|ui| {
        if keys.is_empty() {
            ui.monospace("—").labelled_by(name);
        }
        for k in keys {
            ui.monospace(k).labelled_by(name);
            let remove = ui.small_button("✕");
            a11y::name(
                &remove,
                WidgetType::Button,
                &strings::tr_args(
                    ctx,
                    &txt::SHORTCUTS_POINTER_REMOVE_NAME,
                    &[("keys", k), ("command", label)],
                ),
            );
            if remove.clicked() {
                let rest: Vec<&str> = keys
                    .iter()
                    .filter(|x| *x != k)
                    .map(String::as_str)
                    .collect();
                app.rebind_gestures(target, id.clone(), &rest);
            }
        }
    });
}

/// On Linux, a warning for a gesture bound with Alt and a drag or the
/// wheel, which some window managers take.
fn alt_note(ctx: &egui::Context, keys: &[String]) -> Option<(String, bool)> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let risky = keys.iter().filter_map(|k| Keys::parse(k).ok()).any(|k| {
        k.pointer().is_some_and(|(c, p)| {
            c.alt && matches!(p, Pointer::Wheel | Pointer::Scroll | Pointer::Drag(_))
        })
    });
    risky.then(|| (strings::tr(ctx, &txt::SHORTCUTS_POINTER_ALT), false))
}

/// The pad a new gesture is done on, while one is awaited: the gesture
/// done shown, used (added to the command's) or cancelled.
fn pointer_pad(ui: &mut Ui, app: &AppHandle, st: &mut State, target: Layer) {
    use egui_pop_core::pointer::{self, chord};
    let Some((id, mut candidate)) = st.pointer_capture.clone() else {
        return;
    };
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let title = title_of(&ctx, app, &id);
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        st.pointer_capture = None;
        return;
    }
    ui.add_space(t.space.s);
    ui.colored_label(
        t.accent.default,
        strings::tr_args(&ctx, &txt::SHORTCUTS_POINTER_PAD, &[("command", &title)]),
    );
    let size = egui::vec2(ui.available_width().min(420.0), 88.0);
    let (rect, pad) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    a11y::name(
        &pad,
        WidgetType::Other,
        &strings::tr(&ctx, &txt::SHORTCUTS_POINTER_PAD_NAME),
    );
    if pad.hovered() {
        if let Some(input) = pointer::wheel_inputs(&ctx).last() {
            candidate = Some(input.chord().to_string());
        }
        pointer::take_wheel(&ctx);
    }
    let modifiers = ui.input(|i| i.modifiers);
    for b in [
        egui::PointerButton::Primary,
        egui::PointerButton::Secondary,
        egui::PointerButton::Middle,
        egui::PointerButton::Extra1,
        egui::PointerButton::Extra2,
    ] {
        let button = pointer::button(b);
        let done = if pad.double_clicked_by(b) {
            Some(Pointer::DoubleClick(button))
        } else if pad.clicked_by(b) {
            Some(Pointer::Click(button))
        } else if pad.drag_stopped_by(b) {
            Some(Pointer::Drag(button))
        } else {
            None
        };
        if let Some(p) = done {
            candidate = Some(chord(modifiers, p).to_string());
        }
    }
    let stroke = egui::Stroke::new(1.0, t.accent.default);
    ui.painter()
        .rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Inside);
    if let Some(c) = &candidate {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            c,
            egui::TextStyle::Monospace.resolve(ui.style()),
            ui.visuals().text_color(),
        );
    }
    ui.horizontal(|ui| {
        if let Some(c) = &candidate
            && ui
                .button(strings::tr_args(
                    &ctx,
                    &txt::SHORTCUTS_POINTER_USE,
                    &[("keys", c)],
                ))
                .clicked()
        {
            let mut keys = app.keys_for(&id);
            if !keys.contains(c) {
                keys.push(c.clone());
            }
            let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
            app.rebind_gestures(target, id.clone(), &keys);
            candidate = None;
            st.pointer_capture = None;
            return;
        }
        if ui
            .button(strings::tr(&ctx, &egui_pop_core::strings::CANCEL))
            .clicked()
        {
            st.pointer_capture = None;
        }
    });
    if let Some((_, slot)) = st.pointer_capture.as_mut() {
        *slot = candidate;
    }
    ui.add_space(t.space.s);
}
