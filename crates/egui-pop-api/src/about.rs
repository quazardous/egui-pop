//! About the application, as a plugin ([`AboutPlugin`]): Help > About
//! *application* opens a dialog with the application's name, icon,
//! version, one-line description, licences, authors and links, and a
//! "Copy version info" button for bug reports. The host describes the
//! application with an [`AppInfo`].
//!
//! Links open through egui's `Context::open_url` (a new tab on the web,
//! the system's browser on the desktop).
//!
//! "Copy version info" copies plain text ([`AppInfo::version_info`]:
//! the application's version, egui-pop's and egui's, the platform and
//! the renderer when the host gave it) through the `clipboard.copy`
//! step when [`crate::clipboard::ClipboardPlugin`] is connected (the
//! `about.version` endpoint), else straight to the system clipboard.
//!
//! Commands: `about` (Help > About *application*; its title says
//! `{ $application }`), for agents `about.read` (the same as JSON, kept
//! out of the palette), and `help.report` (Help > Report a problem, the
//! palette, the inspect dialog's Report button, the About dialog's link):
//! a new issue opened in the browser, prefilled with the version info,
//! the platform, the page shown and the last inspect pick ([`report`]).
//! `help.report` is declared only when the [`AppInfo`] says where issues
//! go ([`AppInfo::issues`]): a library cannot know where an
//! application's bugs belong, and a command leading nowhere is noise.
//!
//! ```
//! use egui_pop_api::about::{ABOUT, AboutPlugin, AppInfo};
//! use egui_pop_api::App;
//!
//! let info = AppInfo::new("Darkroom", "1.2.0")
//!     .description("A photo editor.")
//!     .licence("GPL-3.0-or-later")
//!     .author("The Darkroom contributors")
//!     .repository("https://example.org/darkroom");
//! let app = App::builder()
//!     .plugin(AboutPlugin::new(info))
//!     .build(&egui::Context::default())
//!     .unwrap();
//! let about = app.handle().service(&ABOUT).unwrap();
//! assert_eq!(about.info().name, "Darkroom");
//! assert!(about.info().version_info().starts_with("Darkroom 1.2.0\n"));
//! ```

use std::fmt;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use egui::Ui;
use egui_pop_core::lock;
use egui_pop_core::strings;
use egui_pop_core::ui::{TextRole, UiPop as _};
use egui_pop_model::clipboard::{ClipItem, TEXT};
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::{Value as Json, json};

pub mod report;

use crate::app::{AppHandle, MENU_HELP};
use crate::clipboard::{self, ClipSource, CopyRequest};
use crate::dialogs::{Custom, DIALOGS};
use crate::services::ServiceToken;
use crate::steps::Outcome;
use crate::strings as txt;
use crate::{Ctx, Plugin, PluginError};

/// The plugin's id.
pub const ABOUT_PLUGIN: PluginId = PluginId::from_static("egui_pop.about");

/// What the application says about itself, as a service.
pub const ABOUT: ServiceToken<About> = ServiceToken::new("egui_pop.about");

/// Help > About *application*.
pub const CMD_ABOUT: CommandId = CommandId::from_static("about");
/// The same, as JSON, for agents.
pub const CMD_ABOUT_READ: CommandId = CommandId::from_static("about.read");
/// Help > Report a problem: a prefilled new issue, in the browser.
pub const CMD_REPORT: CommandId = CommandId::from_static("help.report");

/// The clipboard endpoint "Copy version info" copies from.
pub const VERSION_ENDPOINT: &str = "about.version";

/// egui-pop's version: its application API's.
pub const EGUI_POP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The egui series egui-pop is built against (one egui-pop minor per
/// egui minor); a test keeps it in step with the workspace.
pub const EGUI_VERSION: &str = "0.37";

/// Draws the application's icon in a square of the given side.
pub type IconFn = Arc<dyn Fn(&mut Ui, f32) + Send + Sync>;

/// What the application says about itself: the host fills it.
#[derive(Clone, Default)]
pub struct AppInfo {
    /// Its name; empty: the application's (`AppBuilder::name`).
    pub name: String,
    /// Its version, typically `env!("CARGO_PKG_VERSION")`.
    pub version: String,
    /// What it is, in one line.
    pub description: String,
    /// Its licences, as SPDX expressions (`MIT OR Apache-2.0`).
    pub licences: Vec<String>,
    pub authors: Vec<String>,
    /// Its source code.
    pub repository: Option<String>,
    pub website: Option<String>,
    pub docs: Option<String>,
    /// Where to report a problem: the issue list
    /// (`https://github.com/owner/repo/issues`). Set, `help.report`
    /// opens a new issue under it, prefilled.
    pub issues: Option<String>,
    /// The repository's issue form `help.report` fills (`bug.yml`, in
    /// `.github/ISSUE_TEMPLATE/`), its fields named as
    /// [`report`]'s `FIELD_*`; without it, the report goes in the body.
    pub issue_form: Option<String>,
    /// What draws the interface (`wgpu: Vulkan, llvmpipe`), for the
    /// version info; the host knows it.
    pub renderer: Option<String>,
    /// Its icon, drawn at the top of the dialog.
    pub icon: Option<IconFn>,
}

impl fmt::Debug for AppInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppInfo")
            .field("name", &self.name)
            .field("version", &self.version)
            .field("description", &self.description)
            .field("licences", &self.licences)
            .field("authors", &self.authors)
            .field("repository", &self.repository)
            .field("website", &self.website)
            .field("docs", &self.docs)
            .field("issues", &self.issues)
            .field("issue_form", &self.issue_form)
            .field("renderer", &self.renderer)
            .field("icon", &self.icon.is_some())
            .finish()
    }
}

impl AppInfo {
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            ..Self::default()
        }
    }

    pub fn description(mut self, text: &str) -> Self {
        self.description = text.into();
        self
    }

    /// Adds a licence (an SPDX expression).
    pub fn licence(mut self, spdx: &str) -> Self {
        self.licences.push(spdx.into());
        self
    }

    pub fn author(mut self, author: &str) -> Self {
        self.authors.push(author.into());
        self
    }

    pub fn repository(mut self, url: &str) -> Self {
        self.repository = Some(url.into());
        self
    }

    pub fn website(mut self, url: &str) -> Self {
        self.website = Some(url.into());
        self
    }

    pub fn docs(mut self, url: &str) -> Self {
        self.docs = Some(url.into());
        self
    }

    /// Where to report a problem: the issue list. It turns
    /// `help.report` on.
    pub fn issues(mut self, url: &str) -> Self {
        self.issues = Some(url.into());
        self
    }

    /// The issue form `help.report` fills (`bug.yml`).
    pub fn issue_form(mut self, file: &str) -> Self {
        self.issue_form = Some(file.into());
        self
    }

    pub fn renderer(mut self, renderer: &str) -> Self {
        self.renderer = Some(renderer.into());
        self
    }

    /// The icon, drawn by `f` in a square of the side it is given.
    pub fn icon(mut self, f: impl Fn(&mut Ui, f32) + Send + Sync + 'static) -> Self {
        self.icon = Some(Arc::new(f));
        self
    }

    /// Where the application runs: the operating system and the
    /// architecture, or the web with the browser and its operating
    /// system, read from the user agent (`web (wasm32), Firefox 132 on
    /// Linux`).
    pub fn platform() -> String {
        if cfg!(target_arch = "wasm32") {
            match user_agent() {
                Some(ua) => format!("web (wasm32), {}", report::describe_user_agent(&ua)),
                None => "web (wasm32)".into(),
            }
        } else {
            format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
        }
    }

    /// The version info, as plain text for a bug report: the
    /// application and its version, egui-pop's and egui's, the platform,
    /// the renderer when known.
    pub fn version_info(&self) -> String {
        let mut lines = vec![
            format!("{} {}", self.name, self.version).trim().to_string(),
            format!("egui-pop {EGUI_POP_VERSION}"),
            format!("egui {EGUI_VERSION}"),
            format!("Platform: {}", Self::platform()),
        ];
        if let Some(r) = &self.renderer {
            lines.push(format!("Renderer: {r}"));
        }
        lines.join("\n") + "\n"
    }

    /// Everything, as `about.read` answers it.
    pub fn to_json(&self) -> Json {
        json!({
            "name": self.name,
            "version": self.version,
            "description": self.description,
            "licences": self.licences,
            "authors": self.authors,
            "links": {
                "repository": self.repository,
                "website": self.website,
                "docs": self.docs,
                "issues": self.issues,
            },
            "issue_form": self.issue_form,
            "egui_pop": EGUI_POP_VERSION,
            "egui": EGUI_VERSION,
            "platform": Self::platform(),
            "renderer": self.renderer,
            "version_info": self.version_info(),
        })
    }
}

/// The browser's user agent, on the web.
fn user_agent() -> Option<String> {
    #[cfg(all(target_arch = "wasm32", feature = "eframe"))]
    {
        eframe::web_sys::window().and_then(|w| w.navigator().user_agent().ok())
    }
    #[cfg(not(all(target_arch = "wasm32", feature = "eframe")))]
    {
        None
    }
}

/// What the application says about itself.
#[derive(Debug, Default)]
pub struct About {
    info: Mutex<AppInfo>,
}

impl About {
    /// The application's info.
    pub fn info(&self) -> AppInfo {
        lock(&self.info).clone()
    }

    /// Changes it (the renderer, known once the window is up).
    pub fn update(&self, f: impl FnOnce(&mut AppInfo)) {
        f(&mut lock(&self.info));
    }
}

/// Says which page or view is shown, for a report (`None`: it cannot).
pub type PageFn = dyn Fn(&AppHandle) -> Option<String>;

/// Help > About *application*, `about.read` for agents, and
/// `help.report` when the info says where issues go.
#[derive(Default)]
pub struct AboutPlugin {
    info: AppInfo,
    page: Option<Rc<PageFn>>,
}

impl fmt::Debug for AboutPlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AboutPlugin")
            .field("info", &self.info)
            .field("page", &self.page.is_some())
            .finish()
    }
}

impl AboutPlugin {
    pub fn new(info: AppInfo) -> Self {
        Self { info, page: None }
    }

    /// Says which page or view is shown, for `help.report`'s page field
    /// and title.
    pub fn page(mut self, f: impl Fn(&AppHandle) -> Option<String> + 'static) -> Self {
        self.page = Some(Rc::new(f));
        self
    }
}

/// The page function, for [`report_of`].
#[derive(Default)]
struct Shared {
    page: std::cell::RefCell<Option<Rc<PageFn>>>,
}

/// The version info, as the `about.version` endpoint copies it.
struct VersionSource;

impl ClipSource for VersionSource {
    fn copy(&self, app: &AppHandle) -> Option<ClipItem> {
        let text = app.service(&ABOUT)?.info().version_info();
        Some(ClipItem::new(TEXT, 0, json!(text), "Version info").fallback(TEXT, json!(text)))
    }
}

impl Plugin for AboutPlugin {
    fn id(&self) -> PluginId {
        ABOUT_PLUGIN
    }

    fn title(&self) -> String {
        "About".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        const CAT: &str = "Help";
        let mut info = self.info.clone();
        if info.name.is_empty() {
            info.name = ctx.app().name();
        }
        ctx.service(
            &ABOUT,
            About {
                info: Mutex::new(info),
            },
        )?;
        ctx.clip_endpoint(VERSION_ENDPOINT, Some(Rc::new(VersionSource)), None);
        *ctx.app().local::<Shared>().page.borrow_mut() = self.page.clone();
        if self.info.issues.is_some() {
            ctx.command(
                CommandSpec::new(CMD_REPORT, "Report a problem", CAT)
                    .description("Opens a new issue in the browser, prefilled with the version info, the platform, the page shown and the last element inspected.")
                    .menu(MENU_HELP, "8_report", 0),
                |call| {
                    let url = report_url(call.app)?;
                    open_report(call.app, &url.url)?;
                    Ok(json!({ "url": url.url, "trimmed": url.trimmed }))
                },
            )?;
        }
        ctx.command(
            CommandSpec::new(CMD_ABOUT, "About { $application }", CAT)
                .description("The application's name, version, licence, authors and links, with its version info to copy.")
                .icon("info")
                .menu(MENU_HELP, "9_about", 0),
            |call| open(call.app).map(|_| Json::Null),
        )?;
        ctx.command(
            CommandSpec::new(CMD_ABOUT_READ, "Read about the application", CAT)
                .description("The application's name, version, description, licences, authors and links, egui-pop's and egui's versions, the platform, the renderer and the version info as text.")
                .hidden()
                .read_only(),
            |call| {
                let about = call
                    .app
                    .service(&ABOUT)
                    .ok_or("the about plugin is not connected")?;
                Ok(about.info().to_json())
            },
        )?;
        Ok(())
    }
}

/// The dialog's title, in the interface's language.
pub fn title(ctx: &egui::Context, info: &AppInfo) -> String {
    strings::tr_args(ctx, &txt::ABOUT_TITLE, &[("application", &info.name)])
}

/// Opens the About dialog (it needs [`AboutPlugin`] and the dialogs
/// plugin); returns the dialog's id.
pub fn open(app: &AppHandle) -> Result<u64, String> {
    let about = app
        .service(&ABOUT)
        .ok_or("the about plugin is not connected")?;
    let dialogs = app
        .service(&DIALOGS)
        .ok_or("the dialogs plugin is not connected")?;
    let info = about.info();
    let spec = Custom::closing(&title(&app.egui(), &info)).size(440.0, 300.0);
    let mut copied = false;
    dialogs.open_custom(app, spec, move |ui, app, pass| {
        pass.describe(about.info().to_json());
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| copied |= show(ui, app, copied));
    })
}

/// Copies the version info: through the clipboard's `clipboard.copy`
/// step when its plugin is connected, else to the system clipboard.
pub fn copy_version_info(app: &AppHandle) -> Result<(), String> {
    let connected = app
        .plugins()
        .iter()
        .any(|p| p.id == clipboard::PLUGIN && p.connected);
    if connected {
        return match clipboard::copy(app, CopyRequest::new(VERSION_ENDPOINT), &Origin::User) {
            Outcome::Done(r) => r.map(|_| ()),
            Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
        };
    }
    let about = app
        .service(&ABOUT)
        .ok_or("the about plugin is not connected")?;
    app.egui().copy_text(about.info().version_info());
    Ok(())
}

/// What a report says: the version info, the platform, the page shown
/// (when the application says it), the last inspect pick.
pub fn report_of(app: &AppHandle) -> Result<report::Report, String> {
    let about = app
        .service(&ABOUT)
        .ok_or("the about plugin is not connected")?;
    let info = about.info();
    let platform = AppInfo::platform();
    let version: String = info
        .version_info()
        .lines()
        .filter(|l| !l.starts_with("Platform: "))
        .map(|l| format!("{l}\n"))
        .collect();
    let page = app
        .local::<Shared>()
        .page
        .borrow()
        .clone()
        .and_then(|f| f(app));
    let title = match &page {
        Some(p) => format!("Problem in {p}"),
        None => format!("Problem in {}", info.name),
    };
    let pick = crate::devtools::inspect::last(app).map(|i| i.text());
    Ok(report::Report {
        title,
        version,
        platform,
        page,
        pick,
    })
}

/// The new-issue URL `help.report` opens (it needs [`AboutPlugin`] with
/// [`AppInfo::issues`]).
pub fn report_url(app: &AppHandle) -> Result<report::IssueUrl, String> {
    let about = app
        .service(&ABOUT)
        .ok_or("the about plugin is not connected")?;
    let info = about.info();
    let issues = info
        .issues
        .as_deref()
        .ok_or("the application says nowhere to report issues")?;
    Ok(report_of(app)?.url(issues, info.issue_form.as_deref()))
}

/// Opens the URL: through the links service when its plugin is
/// connected (a new tab on the web, the system's browser on the
/// desktop), else egui's `open_url`.
fn open_report(app: &AppHandle, url: &str) -> Result<(), String> {
    if crate::links::links(app).is_none() {
        app.egui().open_url(egui::OpenUrl::new_tab(url));
        return Ok(());
    }
    let label = strings::tr(&app.egui(), &txt::ABOUT_ISSUES);
    match crate::links::open(app, egui_pop_model::links::Request::new(url).label(&label)) {
        Outcome::Done(r) => r.map(|_| ()),
        Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
    }
}

fn link(ui: &mut Ui, label: &str, url: &Option<String>) {
    if let Some(url) = url {
        ui.hyperlink_to(label, url).on_hover_text(url);
    }
}

/// Draws what the application says about itself (it needs
/// [`AboutPlugin`]): the About dialog's body, also usable in a page.
/// `copied`: the version info was copied (the button says so). Returns
/// whether the version info was copied this pass.
pub fn show(ui: &mut Ui, app: &AppHandle, copied: bool) -> bool {
    let Some(about) = app.service(&ABOUT) else {
        return false;
    };
    let info = about.info();
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        if let Some(icon) = &info.icon {
            icon(ui, 48.0);
        }
        ui.vertical(|ui| {
            ui.text_role(TextRole::Title, &info.name);
            if !info.version.is_empty() {
                ui.text_role(
                    TextRole::Meta,
                    strings::tr_args(&ctx, &txt::ABOUT_VERSION, &[("version", &info.version)]),
                );
            }
        });
    });
    if !info.description.is_empty() {
        ui.add_space(4.0);
        ui.label(&info.description);
    }
    ui.add_space(8.0);
    egui::Grid::new("egui_pop.about.facts")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            if !info.licences.is_empty() {
                ui.text_role(TextRole::Muted, strings::tr(&ctx, &txt::ABOUT_LICENCE));
                ui.label(info.licences.join(", "));
                ui.end_row();
            }
            if !info.authors.is_empty() {
                ui.text_role(TextRole::Muted, strings::tr(&ctx, &txt::ABOUT_AUTHORS));
                ui.label(info.authors.join(", "));
                ui.end_row();
            }
        });
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        link(
            ui,
            &strings::tr(&ctx, &txt::ABOUT_REPOSITORY),
            &info.repository,
        );
        link(ui, &strings::tr(&ctx, &txt::ABOUT_WEBSITE), &info.website);
        link(ui, &strings::tr(&ctx, &txt::ABOUT_DOCS), &info.docs);
        // Reporting goes through `help.report`, prefilled; the bare
        // issue list only when the command is not there.
        let issues = strings::tr(&ctx, &txt::ABOUT_ISSUES);
        if app.command(&CMD_REPORT).is_some() {
            let hover = info.issues.clone().unwrap_or_default();
            if ui.link(&issues).on_hover_text(hover).clicked() {
                app.run(CMD_REPORT, Json::Null);
            }
        } else {
            link(ui, &issues, &info.issues);
        }
    });
    ui.add_space(8.0);
    let mut now = false;
    ui.horizontal(|ui| {
        if ui
            .button(strings::tr(&ctx, &txt::ABOUT_COPY))
            .on_hover_text(info.version_info())
            .clicked()
        {
            match copy_version_info(app) {
                Ok(()) => now = true,
                Err(e) => log::warn!("about: {e}"),
            }
        }
        if copied || now {
            ui.text_role(TextRole::Muted, strings::tr(&ctx, &txt::ABOUT_COPIED));
        }
    });
    now
}
