//! The gallery's shell plugin: the page list in the side bar (a tree, its
//! groups foldable), the page in the main zone, its playground in the inspector, the theme switchers in
//! the top bar, and commands to move between pages.

use std::sync::Mutex;

use egui::Ui;
use egui_pop::api::app::MENU_VIEW;
use egui_pop::api::stories::StoryInfo;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::kit::{Kit, Visibility};
use egui_pop::core::lock;
use egui_pop::core::ui::{Stock, Tone, UiPop};
use egui_pop::core::widgets;
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{BlockId, CommandId, PluginId, StoryId};
use egui_pop::prelude::{Block, ServiceToken, zones};
use egui_pop::theme::{self, Density, Tokens};
use egui_pop::views::Mode;
use egui_pop::views::tree::{Node, Signal, Sink, TreeSource, TreeState, TreeView};
use serde_json::{Value as Json, json};

use crate::{PAGES, TITLE};

/// The page shown.
#[derive(Debug)]
pub struct Nav {
    page: Mutex<StoryId>,
    /// The page the list last brought into view.
    in_view: Mutex<Option<StoryId>>,
    /// The list of pages: its fold, its cursor, how far it is scrolled.
    list: Mutex<TreeState<String>>,
}

impl Nav {
    pub fn page(&self) -> StoryId {
        lock(&self.page).clone()
    }

    /// Whether the list has yet to bring the page shown into view: once
    /// after each change of page, however it came.
    fn to_bring_into_view(&self) -> bool {
        let page = self.page();
        let mut in_view = lock(&self.in_view);
        let new = in_view.as_ref() != Some(&page);
        *in_view = Some(page);
        new
    }

    /// Shows a page; also sets the `galleryPage` context key.
    pub fn set(&self, app: &AppHandle, page: StoryId) {
        app.set_context("galleryPage", page.as_str());
        *lock(&self.page) = page;
    }
}

/// The navigation service.
pub const NAV: ServiceToken<Nav> = ServiceToken::new("gallery.nav");

pub const SHELL: PluginId = PluginId::from_static("gallery.shell");
pub const CMD_NEXT: CommandId = CommandId::from_static("gallery.page.next");
pub const CMD_PREVIOUS: CommandId = CommandId::from_static("gallery.page.previous");
pub const CMD_PAGE: CommandId = CommandId::from_static("gallery.page");
pub const CMD_THEME: CommandId = CommandId::from_static("gallery.theme.next");

/// The shell plugin, showing `first` at start-up.
#[derive(Debug)]
pub struct Shell {
    pub first: StoryId,
}

fn nav(app: &AppHandle) -> std::sync::Arc<Nav> {
    app.service(&NAV)
        .expect("the gallery shell provides navigation")
}

/// Moves `by` pages through the stories, wrapping around.
/// The stories as listed: groups in the order of their lightest story.
pub fn ordered(app: &AppHandle) -> Vec<StoryInfo> {
    let all = app.stories();
    let rank = |g: &str| {
        all.iter()
            .filter(|s| s.group == g)
            .map(|s| s.weight)
            .min()
            .unwrap_or(0)
    };
    let mut v = all.clone();
    v.sort_by_key(|s| (rank(&s.group), s.group.clone(), s.weight));
    v
}

fn step(app: &AppHandle, by: isize) -> Json {
    let stories = ordered(app);
    let nav = nav(app);
    let at = stories.iter().position(|s| s.id == nav.page()).unwrap_or(0) as isize;
    let n = stories.len().max(1) as isize;
    let next = stories[((at + by).rem_euclid(n)) as usize].id.clone();
    nav.set(app, next.clone());
    json!({ "page": next })
}

impl Plugin for Shell {
    fn id(&self) -> PluginId {
        SHELL
    }

    fn title(&self) -> String {
        "Gallery".into()
    }

    fn priority(&self) -> i32 {
        100
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let app = ctx.app();
        ctx.service(
            &NAV,
            Nav {
                page: Mutex::new(self.first.clone()),
                in_view: Mutex::new(None),
                list: Mutex::new(TreeState::new()),
            },
        )?;
        app.set_context("galleryPage", self.first.as_str());
        ctx.command(
            CommandSpec::new(CMD_NEXT, "Next page", "Gallery")
                .description("Shows the next page of the gallery.")
                .key("Alt+Down")
                .menu(MENU_VIEW, "5_gallery", 0),
            |call| Ok(step(call.app, 1)),
        )?
        .command(
            CommandSpec::new(CMD_PREVIOUS, "Previous page", "Gallery")
                .description("Shows the previous page of the gallery.")
                .key("Alt+Up")
                .menu(MENU_VIEW, "5_gallery", 1),
            |call| Ok(step(call.app, -1)),
        )?
        .command(
            CommandSpec::new(CMD_PAGE, "Show a page", "Gallery")
                .description("Shows a page by name (`page`).")
                .args(json!({ "page": "overview" }))
                .hidden(),
            |call| {
                let page = call
                    .args
                    .get("page")
                    .and_then(Json::as_str)
                    .ok_or("`page` is missing")?;
                let id = StoryId::new(page).map_err(|e| e.to_string())?;
                if !call.app.stories().iter().any(|s| s.id == id) {
                    return Err(format!(
                        "no page `{page}` (there are: {})",
                        PAGES.join(", ")
                    ));
                }
                nav(call.app).set(call.app, id);
                Ok(json!({ "page": page }))
            },
        )?
        .command(
            CommandSpec::new(CMD_THEME, "Next theme", "Gallery")
                .description("Switches to the next built-in theme.")
                .key("Ctrl+Shift+T")
                .menu(MENU_VIEW, "6_theme", 0),
            |call| {
                let t = call.app.theme();
                let ctx = call.app.egui();
                let all = theme::BUILT_IN_THEMES;
                let shown = t.shown(&ctx);
                let at = all.iter().position(|n| *n == shown).unwrap_or(0);
                let next = all[(at + 1) % all.len()];
                t.pick(&ctx, next);
                Ok(json!({ "theme": next }))
            },
        )?;
        ctx.block(
            zones::TOP_BAR,
            Block::add(BlockId::from_static("gallery.switchers"), 0, |ui, app| {
                switchers(ui, app)
            }),
        )
        .block(
            zones::SIDE_BAR,
            Block::add(BlockId::from_static("gallery.pages"), 0, |ui, app| {
                pages(ui, app)
            }),
        )
        .block(
            zones::MAIN,
            Block::add(BlockId::from_static("gallery.page"), 0, |ui, app| {
                page(ui, app)
            }),
        )
        .block(
            zones::INSPECTOR,
            Block::add(BlockId::from_static("gallery.playground"), 0, |ui, app| {
                playground(ui, app)
            }),
        );
        Ok(())
    }
}

/// The theme switchers.
fn switchers(ui: &mut Ui, app: &AppHandle) {
    let service = app.theme();
    let mut spec = service.spec();
    let before = spec.clone();
    let shown = service.shown(ui.ctx());
    let counterpart = service.counterpart();
    let mut picked = shown.clone();
    ui.strong(TITLE);
    ui.separator();
    let label = ui.label("Theme");
    // The other theme of the pair, the one not shown.
    let other = counterpart.as_ref().map(|c| {
        if *c == shown {
            spec.base.clone()
        } else {
            c.clone()
        }
    });
    let preference = service.preference();
    let mut chosen = preference;
    let combo = widgets::combo_box("theme")
        .selected_text(theme::theme_label(&shown))
        .show_ui(ui, |ui| {
            // The light or dark preference first, choosing within the pair.
            ui.weak("Light or dark");
            ui.add_enabled_ui(other.is_some(), |ui| {
                for p in [
                    egui::ThemePreference::System,
                    egui::ThemePreference::Light,
                    egui::ThemePreference::Dark,
                ] {
                    widgets::selectable_value(ui, &mut chosen, p, preference_label(p))
                        .on_disabled_hover_text("This theme has no light or dark counterpart.");
                }
            });
            ui.separator();
            let names: Vec<String> = theme::BUILT_IN_THEMES
                .iter()
                .map(|s| s.to_string())
                .chain(
                    service
                        .themes()
                        .names()
                        .filter(|n| !theme::BUILT_IN_THEMES.contains(n))
                        .map(String::from),
                )
                .collect();
            for name in names {
                let label = if Some(&name) == other.as_ref() {
                    format!("{} (its pair)", theme::theme_label(&name))
                } else {
                    theme::theme_label(&name)
                };
                widgets::selectable_value(ui, &mut picked, name, label);
            }
        })
        .response
        .labelled_by(label.id);
    combo.on_hover_text(match &other {
        Some(o) => format!(
            "Paired with {}; light or dark: {}",
            theme::theme_label(o),
            preference_label(preference)
        ),
        None => "No light or dark counterpart".to_string(),
    });
    let label = ui.label("Density");
    widgets::combo_box("density")
        .selected_text(spec.density.name())
        .show_ui(ui, |ui| {
            for d in Density::ALL {
                widgets::selectable_value(ui, &mut spec.density, d, d.name());
            }
        })
        .response
        .labelled_by(label.id);
    let label = ui.label("Accent");
    widgets::combo_box("accent")
        .selected_text(spec.accent.as_deref().unwrap_or("theme's own"))
        .show_ui(ui, |ui| {
            widgets::selectable_value(ui, &mut spec.accent, None, "theme's own");
            for scale in service.themes().accents(&spec.base) {
                widgets::selectable_value(ui, &mut spec.accent, Some(scale.clone()), scale);
            }
        })
        .response
        .labelled_by(label.id);
    widgets::checkbox(ui, &mut spec.reduced_motion, "Reduced motion");
    if spec != before {
        service.set(spec);
    }
    if picked != shown {
        service.pick(ui.ctx(), &picked);
    }
    if chosen != preference {
        service.set_preference(chosen);
    }
}

/// A light or dark preference, as the switcher shows it.
fn preference_label(p: egui::ThemePreference) -> &'static str {
    match p {
        egui::ThemePreference::System => "Follow the system",
        egui::ThemePreference::Light => "Always light",
        egui::ThemePreference::Dark => "Always dark",
    }
}

/// The tree's key of a group of pages.
fn group_key(group: &str) -> String {
    format!("group:{group}")
}

/// The tree's key of a page.
fn page_key(page: &StoryId) -> String {
    format!("page:{page}")
}

/// The groups in the order listed.
fn groups(stories: &[StoryInfo]) -> Vec<&str> {
    let mut groups: Vec<&str> = stories.iter().map(|s| s.group.as_str()).collect();
    groups.dedup();
    groups
}

/// The list of pages as a tree: the groups and their pages, known at once.
struct PageTree<'a>(&'a [StoryInfo]);

impl TreeSource<String> for PageTree<'_> {
    fn load(&self, parent: Option<&String>, sink: Sink<String>) {
        sink.deliver(match parent {
            None => groups(self.0)
                .into_iter()
                .map(|g| Node::branch(group_key(g), g))
                .collect(),
            Some(group) => self
                .0
                .iter()
                .filter(|s| group_key(&s.group) == *group)
                .map(|s| Node::leaf(page_key(&s.id), &s.title))
                .collect(),
        });
    }
}

/// Where the folded groups are remembered: with the window's layout, in
/// egui's persisted memory, as the stacked sections' are. A group not named
/// there is open.
fn folded_id() -> egui::Id {
    egui::Id::new("gallery.pages.folded")
}

/// Keeps the groups folded now with the window's layout.
fn remember_folded(ctx: &egui::Context, list: &TreeState<String>, stories: &[StoryInfo]) {
    let folded: Vec<String> = groups(stories)
        .into_iter()
        .filter(|g| !list.model().is_expanded(&group_key(g)))
        .map(String::from)
        .collect();
    ctx.data_mut(|d| d.insert_persisted(folded_id(), folded));
}

/// The page list, a tree of egui-pop's with the groups as folders, its one
/// scroll area filling the side bar: 57 pages do not fit a window. Its bar
/// shows at rest as the sections' stack's does, whatever the theme does for
/// the other bars: nothing else says that the list goes on.
///
/// The selection marks the page shown, and only a page opening changes it;
/// the cursor walks the list on its own, and Enter or a click opens.
fn pages(ui: &mut Ui, app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    let nav = nav(app);
    let bring_into_view = nav.to_bring_into_view();
    let stories = ordered(app);
    let mut list = lock(&nav.list);
    if list.model().is_empty() {
        let folded: Vec<String> = ui
            .ctx()
            .data_mut(|d| d.get_persisted(folded_id()))
            .unwrap_or_default();
        for g in groups(&stories) {
            list.set_expanded(&group_key(g), !folded.iter().any(|f| f == g));
        }
    }
    if bring_into_view {
        let page = nav.page();
        if let Some(shown) = stories.iter().find(|s| s.id == page) {
            let group = group_key(&shown.group);
            if !list.model().is_expanded(&group) {
                list.set_expanded(&group, true);
                remember_folded(ui.ctx(), &list, &stories);
            }
            let key = page_key(&page);
            list.set_selection([key.clone()], Some(key.clone()));
            list.reveal(&key);
        }
    }
    let others = ui.spacing().scroll;
    let mut bar = Kit::of(ui.ctx()).scroll;
    bar.visibility = Visibility::Always;
    // The bar's colours, as `UiPop::scroll_area` gives them.
    let out = ui
        .scoped(Stock::ScrollBar, Tone::Neutral, |ui| {
            let mut style = bar.style(&others);
            // Thin until the pointer is over it, full width to the pointer
            // all along: the list's longest titles reach this far.
            style.floating_width = t.space.s;
            ui.spacing_mut().scroll = style;
            // The side bar is narrow: the two levels indent little.
            ui.spacing_mut().indent = t.space.m;
            TreeView::new("gallery.pages")
                .label("Pages")
                .mode(Mode::None)
                .activate_on_click(true)
                .show(ui, &mut list, &PageTree(&stories))
        })
        .inner;
    for signal in out.signals {
        match signal {
            Signal::Activate { key } => {
                if let Some(s) = stories.iter().find(|s| page_key(&s.id) == key) {
                    nav.set(app, s.id.clone());
                }
            }
            Signal::Expand { .. } => remember_folded(ui.ctx(), &list, &stories),
            _ => {}
        }
    }
}

/// The page shown.
fn page(ui: &mut Ui, app: &AppHandle) {
    let id = nav(app).page();
    let Some(info) = app.stories().into_iter().find(|s| s.id == id) else {
        ui.weak(format!("No page `{id}`."));
        return;
    };
    ui.heading(&info.title);
    ui.weak(&info.description);
    ui.separator();
    app.show_story(ui, &id);
}

/// The page's playground.
fn playground(ui: &mut Ui, app: &AppHandle) {
    ui.heading("Playground");
    ui.separator();
    let id = nav(app).page();
    if !app.show_playground(ui, &id) {
        ui.weak("This page has no options.");
    }
}
