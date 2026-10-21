//! Custom pages in the settings window (ADR 0013): a page a plugin
//! draws itself, next to the pages generated from declared settings.
//!
//! A page has an id, a title, a place in the tree (under a section or
//! another page, in an order), keywords the settings search finds it
//! by, and the settings it edits: those leave the generated sections,
//! are searched with the page, and its reset takes them back to their
//! defaults as one undo step, with a hook of its own for what is not a
//! setting. It is mounted through the mount contract in the settings
//! page host ([`crate::mounts::SETTINGS_PAGE`]): what its component
//! needs is checked first, and a refusal is reported (ADR 0003) with
//! nothing added. Its body is drawn in a focus scope and a fault
//! boundary, only while it is the page shown.
//!
//! Agents see the pages in the `settings_window` collector, and open one
//! with the `app.settings.page` command (`{"page": "<id>"}`).
//!
//! ```
//! use egui_pop_api::settings_pages::SettingsPage;
//! use egui_pop_api::{App, Ctx, Plugin, PluginError, DefaultPlugins};
//! use egui_pop_model::ids::PluginId;
//!
//! struct Compute;
//!
//! impl Plugin for Compute {
//!     fn id(&self) -> PluginId {
//!         PluginId::from_static("compute")
//!     }
//!
//!     fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
//!         ctx.settings_page(
//!             SettingsPage::new("compute.devices", "Compute")
//!                 .keywords(&["GPU", "OpenCL", "threads"])
//!                 .order(10),
//!             |ui, _app| {
//!                 ui.label("The devices found, and which one runs what.");
//!             },
//!         );
//!         Ok(())
//!     }
//! }
//!
//! let app = App::builder()
//!     .plugins(DefaultPlugins)
//!     .plugin(Compute)
//!     .build(&egui::Context::default())
//!     .unwrap();
//! let pages = app.handle().settings_pages();
//! assert_eq!(pages[0].title, "Compute");
//! assert!(app.handle().mounts().iter().any(|m| m.host == "settings.page"));
//! ```
//!
//! A body is drawn while the settings window holds its own state: it
//! moves the window by commands (`app.settings`, `app.settings.page`),
//! which run before the next frame, not through the
//! [`crate::settings_window::SettingsWindow`] service directly.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use egui::{Ui, WidgetType};
use egui_pop_core::a11y;
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_model::commands::Origin;
use egui_pop_model::diagnostics::{Diagnostic, Level};
use egui_pop_model::fuzzy;
use egui_pop_model::ids::{OwnerId, SettingId};
use egui_pop_model::mount::{
    ClaimKey, Dismiss, FocusPolicy, HostOffer, HostSize, Lifetime, Mountable,
};
use egui_pop_model::values::{Change, Set};
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::app::AppHandle;
use crate::ctx::Ctx;
use crate::hooks::SettingEntry;
use crate::strings as txt;

/// A page's body: drawn every frame the page is shown.
pub type PageFn = Box<dyn FnMut(&mut Ui, &AppHandle)>;

/// A page's own reset, for what it shows that is not a setting.
pub type ResetFn = Rc<dyn Fn(&AppHandle)>;

/// Whether a page's own state is away from its defaults.
pub type ChangedFn = Rc<dyn Fn(&AppHandle) -> bool>;

/// A custom page of the settings window: its place, what finds it, what
/// it edits and what it needs.
///
/// ```
/// use egui_pop_api::settings_pages::SettingsPage;
/// use egui_pop_model::ids::SettingId;
///
/// let page = SettingsPage::new("app.compute", "Compute")
///     .under("Performance")
///     .order(20)
///     .keywords(&["GPU", "OpenCL"])
///     .settings(&[SettingId::from_static("app.threads")]);
/// assert_eq!(page.parent.as_deref(), Some("Performance"));
/// assert!(page.has_reset());
/// ```
#[derive(Clone, Serialize)]
pub struct SettingsPage {
    /// A stable id: what `app.settings.page` and
    /// [`crate::settings_window::SettingsWindow::open`] take.
    pub id: String,
    /// As shown in the tree and over the page.
    pub title: String,
    /// The section (as the tree names it) or the page (its id) it goes
    /// under; `None`, or one that does not exist: a page of its own
    /// after the generated sections.
    pub parent: Option<String>,
    /// Its order among the pages of the same parent (lower first, then
    /// by title).
    pub order: i32,
    /// Words the settings search finds it by, besides its title, its
    /// description and its settings.
    pub keywords: Vec<String>,
    /// Said under its title, and searched.
    pub description: String,
    /// The declared settings it edits: they leave the generated
    /// sections, the search finds the page by them, and its reset takes
    /// them back to their defaults.
    pub settings: Vec<SettingId>,
    /// What its component needs, checked against the settings page
    /// host's offer; `None`: a plain widget (`settings-page:<id>`).
    pub needs: Option<Mountable>,
    #[serde(skip)]
    reset: Option<ResetFn>,
    #[serde(skip)]
    changed: Option<ChangedFn>,
}

impl fmt::Debug for SettingsPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsPage")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("parent", &self.parent)
            .field("order", &self.order)
            .field("keywords", &self.keywords)
            .field("settings", &self.settings)
            .field("reset", &self.reset.is_some())
            .finish_non_exhaustive()
    }
}

impl SettingsPage {
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            parent: None,
            order: 0,
            keywords: Vec::new(),
            description: String::new(),
            settings: Vec::new(),
            needs: None,
            reset: None,
            changed: None,
        }
    }

    /// Under a section (as the tree names it: "Appearance") or a page
    /// (its id).
    pub fn under(mut self, parent: &str) -> Self {
        self.parent = Some(parent.into());
        self
    }

    pub fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    pub fn keywords(mut self, words: &[&str]) -> Self {
        self.keywords.extend(words.iter().map(|w| (*w).to_string()));
        self
    }

    pub fn description(mut self, text: &str) -> Self {
        self.description = text.into();
        self
    }

    /// The declared settings it edits.
    pub fn settings(mut self, ids: &[SettingId]) -> Self {
        for id in ids {
            if !self.settings.contains(id) {
                self.settings.push(id.clone());
            }
        }
        self
    }

    /// What its component needs (checked when it is mounted).
    pub fn needs(mut self, needs: Mountable) -> Self {
        self.needs = Some(needs);
        self
    }

    /// Its own reset, for what it shows that is not a setting (run with
    /// its settings' resets, in the same undo step when it writes to the
    /// store), and whether that is away from its defaults.
    pub fn reset_with(
        mut self,
        changed: impl Fn(&AppHandle) -> bool + 'static,
        reset: impl Fn(&AppHandle) + 'static,
    ) -> Self {
        self.changed = Some(Rc::new(changed));
        self.reset = Some(Rc::new(reset));
        self
    }

    /// Whether it has a reset (settings, or a reset of its own).
    pub fn has_reset(&self) -> bool {
        !self.settings.is_empty() || self.reset.is_some()
    }

    /// Whether what it edits is away from its defaults.
    pub fn is_changed(&self, app: &AppHandle) -> bool {
        let store = app.store();
        let settings = {
            let s = lock(&store);
            self.settings.iter().any(|id| {
                s.get(&format!("settings/{id}"))
                    .is_some_and(|x| x.value != x.descriptor.default)
            })
        };
        settings || self.changed.as_ref().is_some_and(|f| f(app))
    }

    /// Takes what it edits back to its defaults: its settings, then its
    /// own reset, as one undo step of the settings.
    pub fn reset(&self, app: &AppHandle) {
        let store = app.store();
        let group = lock(&store).begin_group(Origin::User, &format!("Reset {}", self.title));
        for id in &self.settings {
            let path = format!("settings/{id}");
            let _ = lock(&store)
                .set(Set::new(&path, Change::Reset, Origin::User).by(&OwnerId::FRAMEWORK));
        }
        if let Some(f) = &self.reset {
            f(app);
        }
        lock(&store).end(group);
    }

    /// What its component needs: its own, or a plain widget.
    pub fn component(&self) -> Mountable {
        self.needs.clone().unwrap_or_else(|| {
            Mountable::new(&format!("settings-page:{}", self.id), &self.title)
                .needs_app()
                .role("group")
        })
    }
}

/// What the settings page host offers a page (ADR 0013): the form's
/// width and the content's height in the settings window, a modal
/// (Escape closes the window), transient, the main window, a focus scope
/// and a fault boundary, sRGB; its content is not drawn while another
/// page is shown.
pub fn host() -> HostOffer {
    HostOffer::new(crate::mounts::SETTINGS_PAGE, "T", "Settings page")
        .source("Ctx::settings_page, SettingsPage")
        .size(HostSize::Content)
        .focus(FocusPolicy::ModalExclusive)
        .owns_keys(&[ClaimKey::Escape])
        .dismissed_by(&[Dismiss::Escape])
        .lifetime(Lifetime::Transient)
        .hides_content(true)
}

struct Entry {
    owner: OwnerId,
    page: SettingsPage,
    /// Taken out while it is drawn.
    body: Option<PageFn>,
}

/// The pages declared, in declaration order.
#[derive(Default)]
pub(crate) struct Pages(RefCell<Vec<Entry>>);

impl Ctx<'_> {
    /// Mounts a custom page in the settings window, drawn by `body`
    /// (see [`crate::settings_pages`]). The page's component is checked
    /// against the settings page host first; a refusal, an empty id or
    /// an id another plugin took is reported as a diagnostic and adds
    /// nothing. Declared again by the same plugin, it replaces its own.
    pub fn settings_page(
        &mut self,
        page: SettingsPage,
        body: impl FnMut(&mut Ui, &AppHandle) + 'static,
    ) -> &mut Self {
        let app = self.app.clone();
        if page.id.trim().is_empty() {
            app.report(Diagnostic {
                level: Level::Error,
                code: "settings-page-id",
                owner: Some(self.owner.clone()),
                subject: page.title.clone(),
                message: "a settings page needs an id; nothing is added".into(),
            });
            return self;
        }
        let pages = app.local::<Pages>();
        let taken = pages.0.borrow().iter().any(|e| {
            e.page.id == page.id && e.owner != self.owner && app.owner_connected(&e.owner)
        });
        if taken {
            app.report(Diagnostic {
                level: Level::Warning,
                code: "duplicate-id",
                owner: Some(self.owner.clone()),
                subject: page.id.clone(),
                message:
                    "a settings page of this id is declared already; this declaration is ignored"
                        .into(),
            });
            return self;
        }
        if app.mount_needs(&page.component(), &host()).is_err() {
            return self;
        }
        let mut list = pages.0.borrow_mut();
        list.retain(|e| e.page.id != page.id);
        list.push(Entry {
            owner: self.owner.clone(),
            page,
            body: Some(Box::new(body)),
        });
        self
    }
}

impl AppHandle {
    /// The custom pages of the settings window, of the connected
    /// plugins, in declaration order.
    pub fn settings_pages(&self) -> Vec<SettingsPage> {
        self.local::<Pages>()
            .0
            .borrow()
            .iter()
            .filter(|e| self.owner_connected(&e.owner))
            .map(|e| e.page.clone())
            .collect()
    }

    /// A custom page by id.
    pub fn settings_page(&self, id: &str) -> Option<SettingsPage> {
        self.settings_pages().into_iter().find(|p| p.id == id)
    }
}

/// Draws a page's body in a focus scope and a fault boundary; `false`
/// when no page has that id.
pub(crate) fn draw(ui: &mut Ui, app: &AppHandle, id: &str) -> bool {
    let pages = app.local::<Pages>();
    let taken = {
        let mut list = pages.0.borrow_mut();
        list.iter_mut()
            .find(|e| e.page.id == id)
            .and_then(|e| e.body.take().map(|b| (b, e.page.title.clone())))
    };
    let Some((mut body, title)) = taken else {
        return false;
    };
    egui_pop_core::focus::scope(ui, "settingsPage", &[], |ui| {
        egui_pop_core::fault::boundary(ui, ("egui_pop.settings.page", id), &title, |ui| {
            body(ui, app);
        });
    });
    if let Some(e) = pages.0.borrow_mut().iter_mut().find(|e| e.page.id == id) {
        e.body.get_or_insert(body);
    }
    true
}

/// A line of the settings tree: a section or a page, indented under its
/// parent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Node {
    /// The section's name, or the page's id.
    pub key: String,
    /// 0 at the top, 1 under a parent.
    pub depth: usize,
    /// A custom page (not a generated section).
    pub page: bool,
}

/// The tree: each section (the shortcuts last) followed by the pages
/// under it, then the pages of their own each followed by its own; the
/// pages of a parent by order, then title. A page goes under a section
/// or a page of its own; under anything else it is a page of its own.
pub(crate) fn tree(sections: &[String], pages: &[SettingsPage]) -> Vec<Node> {
    let mut sorted: Vec<&SettingsPage> = pages.iter().collect();
    sorted.sort_by(|a, b| (a.order, &a.title, &a.id).cmp(&(b.order, &b.title, &b.id)));
    // A page under no section and no page is a page of its own.
    let own = |q: &SettingsPage| {
        q.parent.as_ref().is_none_or(|parent| {
            !sections.contains(parent) && !pages.iter().any(|r| r.id == *parent)
        })
    };
    // A page goes under a section or a page of its own (one level only).
    let resolved: Vec<(&SettingsPage, Option<&str>)> = sorted
        .iter()
        .map(|p| {
            let parent = p.parent.as_deref().filter(|parent| {
                sections.iter().any(|s| s == parent)
                    || pages
                        .iter()
                        .any(|q| q.id == *parent && q.id != p.id && own(q))
            });
            (*p, parent)
        })
        .collect();
    let children = |key: &str| -> Vec<Node> {
        resolved
            .iter()
            .filter(|(_, parent)| *parent == Some(key))
            .map(|(p, _)| Node {
                key: p.id.clone(),
                depth: 1,
                page: true,
            })
            .collect()
    };
    let mut out = Vec::new();
    let (last, firsts) = sections
        .split_last()
        .map_or((None, sections), |(l, f)| (Some(l), f));
    for s in firsts {
        out.push(Node {
            key: s.clone(),
            depth: 0,
            page: false,
        });
        out.extend(children(s));
    }
    for (p, _) in resolved.iter().filter(|(_, parent)| parent.is_none()) {
        out.push(Node {
            key: p.id.clone(),
            depth: 0,
            page: true,
        });
        out.extend(children(&p.id));
    }
    if let Some(s) = last {
        out.push(Node {
            key: s.clone(),
            depth: 0,
            page: false,
        });
        out.extend(children(s));
    }
    out
}

/// The settings no page claims: those the generated sections show.
pub(crate) fn unclaimed(entries: &[SettingEntry], pages: &[SettingsPage]) -> Vec<SettingEntry> {
    entries
        .iter()
        .filter(|e| !pages.iter().any(|p| p.settings.contains(&e.id)))
        .cloned()
        .collect()
}

/// A page's settings, as declared (those that are).
pub(crate) fn page_entries<'a>(
    page: &SettingsPage,
    entries: &'a [SettingEntry],
) -> Vec<&'a SettingEntry> {
    page.settings
        .iter()
        .filter_map(|id| entries.iter().find(|e| e.id == *id))
        .collect()
}

/// Whether a page matches the search: every word in its title, parent,
/// description, keywords or one of its settings as shown.
pub(crate) fn matches(
    ctx: &egui::Context,
    page: &SettingsPage,
    parent_title: &str,
    entries: &[SettingEntry],
    filter: &str,
) -> bool {
    let mut text = format!(
        "{} {} {} {}",
        page.title,
        parent_title,
        page.description,
        page.keywords.join(" ")
    );
    for e in page_entries(page, entries) {
        let d = crate::settings_window::shown(ctx, e);
        text.push(' ');
        text.push_str(&d.label);
        text.push(' ');
        text.push_str(&d.description);
    }
    filter
        .split_whitespace()
        .all(|w| fuzzy::score(w, &text).is_some())
}

/// The page's reset, greyed at its defaults, and its description.
pub(crate) fn header(ui: &mut Ui, app: &AppHandle, page: &SettingsPage) {
    let ctx = ui.ctx().clone();
    if page.has_reset() {
        let name = [("page", page.title.as_str())];
        let changed = page.is_changed(app);
        let r = ui
            .add_enabled(
                changed,
                egui::Button::new(strings::tr(&ctx, &txt::SETTINGS_RESET_PAGE)).small(),
            )
            .on_disabled_hover_text(strings::tr_args(
                &ctx,
                &txt::SETTINGS_PAGE_AT_DEFAULT,
                &name,
            ));
        a11y::name(
            &r,
            WidgetType::Button,
            &strings::tr_args(&ctx, &txt::SETTINGS_RESET_PAGE_NAME, &name),
        );
        if r.clicked() {
            page.reset(app);
        }
    }
    if !page.description.is_empty() {
        ui.weak(&page.description);
    }
}

/// A page found by the search: its title, the button opening it, and
/// its settings that match. Returns whether it was asked open.
pub(crate) fn search_hit(
    ui: &mut Ui,
    app: &AppHandle,
    page: &SettingsPage,
    entries: &[SettingEntry],
    filter: &str,
) -> bool {
    let ctx = ui.ctx().clone();
    let mut open = false;
    ui.horizontal(|ui| {
        ui.strong(&page.title);
        let r = ui.small_button(strings::tr(&ctx, &txt::SETTINGS_OPEN_PAGE));
        a11y::name(
            &r,
            WidgetType::Button,
            &strings::tr_args(
                &ctx,
                &txt::SETTINGS_OPEN_PAGE_NAME,
                &[("page", &page.title)],
            ),
        );
        open = r.clicked();
    });
    if !page.description.is_empty() {
        ui.weak(&page.description);
    }
    for e in page_entries(page, entries) {
        if crate::settings_window::matches(&ctx, app, e, filter) {
            crate::settings_window::row(ui, app, e);
        }
    }
    open
}

/// The pages as the `settings_window` collector shows them.
pub(crate) fn describe(app: &AppHandle) -> Json {
    json!(
        app.settings_pages()
            .iter()
            .map(|p| json!({
                "id": p.id,
                "title": p.title,
                "parent": p.parent,
                "order": p.order,
                "keywords": p.keywords,
                "description": p.description,
                "settings": p.settings,
                "component": p.component().id,
                "reset": p.has_reset(),
                "changed": p.is_changed(app),
            }))
            .collect::<Vec<_>>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(nodes: &[Node]) -> Vec<(String, usize)> {
        nodes.iter().map(|n| (n.key.clone(), n.depth)).collect()
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn pages_go_under_their_parent_by_order_and_title() {
        let sections = s(&["Appearance", "Editor", "Keyboard shortcuts"]);
        let pages = [
            SettingsPage::new("b", "Beta").under("Appearance").order(2),
            SettingsPage::new("a", "Alpha").under("Appearance").order(2),
            SettingsPage::new("z", "Zeta").under("Appearance").order(1),
            SettingsPage::new("k", "Keys").under("Keyboard shortcuts"),
        ];
        let t = tree(&sections, &pages);
        assert_eq!(
            keys(&t),
            [
                ("Appearance".into(), 0),
                ("z".into(), 1),
                ("a".into(), 1),
                ("b".into(), 1),
                ("Editor".into(), 0),
                ("Keyboard shortcuts".into(), 0),
                ("k".into(), 1),
            ]
        );
        assert!(t[1].page && !t[0].page);
    }

    #[test]
    fn pages_of_their_own_come_before_the_shortcuts_with_their_children() {
        let sections = s(&["Appearance", "Keyboard shortcuts"]);
        let pages = [
            SettingsPage::new("compute", "Compute").order(5),
            SettingsPage::new("devices", "Devices").under("compute"),
            SettingsPage::new("lost", "Lost").under("Nowhere"),
            SettingsPage::new("deep", "Deep").under("devices"),
        ];
        assert_eq!(
            keys(&tree(&sections, &pages)),
            [
                ("Appearance".into(), 0),
                ("deep".into(), 0),
                ("lost".into(), 0),
                ("compute".into(), 0),
                ("devices".into(), 1),
                ("Keyboard shortcuts".into(), 0),
            ]
        );
    }

    #[test]
    fn a_page_without_settings_or_a_reset_of_its_own_has_no_reset() {
        let page = SettingsPage::new("p", "Page");
        assert!(!page.has_reset());
        let page = page.reset_with(|_| true, |_| {});
        assert!(page.has_reset());
        assert_eq!(page.component().id, "settings-page:p");
    }
}
