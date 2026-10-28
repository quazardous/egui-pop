//! The in-app viewer (ADR 0017): a panel showing one page at a time, the
//! links service's way inside. Feature `markdown`.
//!
//! [`ViewerPlugin`] (opt-in) declares a panel in the end side bar
//! (detachable into its own window, like any panel) and installs itself
//! as the links service's [`Inside`]: `docs:` pages, markdown pages on
//! the application's documentation hosts and local images then open
//! here, through the same policy and the same `link.open` step as every
//! other link.
//!
//! - **Pages**: markdown (rendered by egui_commonmark), images (egui's
//!   loaders, PNG, JPEG and GIF), plain text. `docs:` pages come from the
//!   [`Docs`] the application ships (`include_str!`), on native and on
//!   the web build alike; remote pages and images come through a
//!   [`Fetch`] the host gives (none by default: the viewer then opens
//!   nothing remote and says so). HTML pages are not shown inside yet:
//!   their error page offers Open outside.
//! - **Navigation**: back and forward (the session's history, nothing
//!   kept across runs), reload, an address bar (typing a target opens it
//!   through the links service), anchors (`#install` scrolls to the
//!   heading), links in a page opened through the links service (a link
//!   to another `docs:` page stays inside, a web link goes outside;
//!   Ctrl+click or a middle click opens outside).
//! - **Find in page** (Ctrl+F, F3, Shift+F3, Enter and Shift+Enter in its
//!   field): the matches counted in the page's text, the block holding
//!   the current one marked and scrolled to.
//! - **Zoom** (Ctrl+plus, Ctrl+minus, Ctrl+0 while the viewer has the
//!   focus): the page's text only, in steps.
//! - **Remote images** are never fetched before the user asks: each is
//!   replaced by its alt text, and a bar says "Images from `host` are not
//!   loaded" with a button loading them; the host is remembered in the
//!   `viewer.image_hosts` setting (cleared there). Images on the page's
//!   own host (a documentation host the application declares) load at
//!   once.
//! - **Failures** show an error page in place (not found, no connection,
//!   too large, a type it cannot show) with Retry, Open outside and Copy
//!   link.
//! - **Agents** read the viewer through `viewer.state` and the `viewer`
//!   collector (the page, its status or error, the history, the search,
//!   the zoom, the images waiting and the hosts allowed) and drive it
//!   with its commands; loading a host's images is the user's alone.
//!
//! ```
//! use egui_pop_api::links::{self, LinksPlugin, Request};
//! use egui_pop_api::App;
//! use egui_pop_views::viewer::{Docs, ViewerPlugin, VIEWER};
//!
//! let docs = Docs::new()
//!     .page("index", "# Help\n\nSee [the guide](guide.md).")
//!     .page("guide", "# Guide\n\n## Install\n\nRun it.");
//! let app = App::builder()
//!     .plugin(LinksPlugin::default())
//!     .plugin(ViewerPlugin::default().docs(docs))
//!     .build(&egui::Context::default())
//!     .unwrap();
//! links::open(app.handle(), Request::new("docs:guide#install"));
//! let viewer = app.handle().service(&VIEWER).unwrap();
//! assert_eq!(viewer.state()["target"], "docs:guide#install");
//! assert_eq!(viewer.state()["status"], "shown");
//! ```

mod page;
mod view;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use egui_commonmark::CommonMarkCache;
use egui_pop_api::links::{self, How, Inside, OpenCall, Request, Source, Target};
use egui_pop_api::services::ServiceToken;
use egui_pop_api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PanelId, PluginId, SettingId};
use egui_pop_model::keymap::BindOptions;
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_model::values::{Change, Descriptor, Set, Value};
use serde_json::{Value as Json, json};

pub use page::{Block, ImageRef, Markdown, has_scheme, resolve, slug};

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.viewer");
/// The viewer's panel.
pub const PANEL: PanelId = PanelId::from_static("egui_pop.viewer");
/// The viewer service.
pub const VIEWER: ServiceToken<Viewer> = ServiceToken::new("app.viewer");
/// The focus surface its keys are bound on (`viewerFocus`).
pub const SURFACE: &str = "viewer";
/// The hosts whose images the user allowed, separated by commas.
pub const SETTING_IMAGE_HOSTS: SettingId = SettingId::from_static("viewer.image_hosts");

pub const CMD_BACK: CommandId = CommandId::from_static("viewer.back");
pub const CMD_FORWARD: CommandId = CommandId::from_static("viewer.forward");
pub const CMD_RELOAD: CommandId = CommandId::from_static("viewer.reload");
pub const CMD_FIND: CommandId = CommandId::from_static("viewer.find");
pub const CMD_FIND_NEXT: CommandId = CommandId::from_static("viewer.find_next");
pub const CMD_FIND_PREVIOUS: CommandId = CommandId::from_static("viewer.find_previous");
pub const CMD_ZOOM_IN: CommandId = CommandId::from_static("viewer.zoom_in");
pub const CMD_ZOOM_OUT: CommandId = CommandId::from_static("viewer.zoom_out");
pub const CMD_ZOOM_RESET: CommandId = CommandId::from_static("viewer.zoom_reset");
pub const CMD_OPEN_OUTSIDE: CommandId = CommandId::from_static("viewer.open_outside");
pub const CMD_COPY_LINK: CommandId = CommandId::from_static("viewer.copy_link");
pub const CMD_LOAD_IMAGES: CommandId = CommandId::from_static("viewer.load_images");
pub const CMD_STATE: CommandId = CommandId::from_static("viewer.state");

/// The largest page or image shown, in bytes.
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
/// The page's zoom steps.
pub const ZOOM_STEPS: &[f32] = &[
    0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0,
];

/// The pages and files an application ships for `docs:` links, by path
/// (`guide/start.md`, `img/shot.png`). A link names a page with or
/// without its `.md` (`docs:guide/start`), a folder its `index.md`
/// (`docs:guide/`, and `docs:` alone the root's).
#[derive(Clone, Default)]
pub struct Docs {
    files: BTreeMap<String, Arc<[u8]>>,
}

impl std::fmt::Debug for Docs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.files.keys()).finish()
    }
}

impl Docs {
    pub fn new() -> Self {
        Self::default()
    }

    /// A markdown page (`.md` added when the path has no extension).
    pub fn page(self, path: &str, markdown: &str) -> Self {
        let path = path.trim_matches('/');
        let path = if std::path::Path::new(path).extension().is_some() {
            path.to_string()
        } else {
            format!("{path}.md")
        };
        self.file(&path, markdown.as_bytes().to_vec())
    }

    /// Any file a page uses (an image), as it is.
    pub fn file(mut self, path: &str, bytes: impl Into<Arc<[u8]>>) -> Self {
        self.files
            .insert(path.trim_matches('/').to_string(), bytes.into());
        self
    }

    /// The file a `docs:` page names, with its path.
    pub fn get(&self, page: &str) -> Option<(String, Arc<[u8]>)> {
        let p = page.trim_start_matches('/');
        let folder = p.trim_end_matches('/');
        let candidates = if folder.is_empty() {
            vec!["index.md".to_string()]
        } else if p.ends_with('/') {
            vec![format!("{folder}/index.md")]
        } else {
            vec![p.to_string(), format!("{p}.md"), format!("{p}/index.md")]
        };
        candidates
            .into_iter()
            .find_map(|c| self.files.get(&c).map(|b| (c.clone(), b.clone())))
    }

    /// The names a `docs:` link may use for each file (its path, a page
    /// without `.md`, a folder for its `index.md`).
    pub fn pages(&self) -> Vec<String> {
        let mut out = Vec::new();
        for path in self.files.keys() {
            out.push(path.clone());
            if let Some(page) = path.strip_suffix(".md") {
                out.push(page.to_string());
                if let Some(folder) = page.strip_suffix("index") {
                    out.push(folder.to_string());
                    out.push(folder.trim_end_matches('/').to_string());
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

/// What a fetch gives: the bytes, or why not.
pub type Fetched = Result<Vec<u8>, String>;

/// How the viewer gets a remote page or image, given by the host (an
/// HTTP client of its choice, a cache, the browser's `fetch` on the web
/// build). `done` may be called from any thread, once.
pub trait Fetch: Send + Sync {
    fn fetch(&self, url: &str, done: Box<dyn FnOnce(Fetched) + Send>);
}

/// Why a page is not shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    NotFound,
    NoConnection,
    TooLarge,
    Unsupported,
}

impl Failure {
    fn title(self) -> &'static egui_pop_core::strings::StrKey {
        use crate::strings as s;
        match self {
            Failure::NotFound => &s::VIEWER_NOT_FOUND,
            Failure::NoConnection => &s::VIEWER_NO_CONNECTION,
            Failure::TooLarge => &s::VIEWER_TOO_LARGE,
            Failure::Unsupported => &s::VIEWER_UNSUPPORTED,
        }
    }
}

/// What a page shows.
#[derive(Debug)]
pub enum Content {
    Markdown(Markdown),
    /// An image, by the address its bytes are given to egui under.
    Image(String),
}

/// A page shown.
#[derive(Debug)]
pub struct Page {
    /// Its address, without the anchor.
    pub target: String,
    pub content: Content,
}

impl Page {
    fn kind(&self) -> &'static str {
        match self.content {
            Content::Markdown(_) => "markdown",
            Content::Image(_) => "image",
        }
    }
}

/// Where the viewer is.
#[derive(Clone, Debug)]
pub enum Status {
    Empty,
    Loading(String),
    Shown(Arc<Page>),
    Failed {
        target: String,
        failure: Failure,
        detail: String,
    },
}

/// A remote or shipped image of a page.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Image {
    Ready,
    Loading,
    /// Waiting for the user to allow its host.
    Blocked(String),
    Failed(String),
}

#[derive(Clone, Debug)]
struct Entry {
    target: String,
    scroll: f32,
}

#[derive(Debug, Default)]
struct Find {
    open: bool,
    /// The field takes the keyboard on the next pass.
    focus: bool,
    query: String,
    /// One block per match.
    matches: Vec<usize>,
    at: usize,
}

#[derive(Debug)]
struct Inner {
    back: Vec<Entry>,
    forward: Vec<Entry>,
    current: Option<Entry>,
    status: Status,
    /// Which load an arriving page answers.
    generation: u64,
    find: Find,
    zoom: f32,
    /// A block to bring into view on the next pass.
    scroll_to: Option<usize>,
    /// A scroll offset to put back (back and forward).
    restore: Option<f32>,
    /// The scroll offset now.
    scroll: f32,
    images: BTreeMap<String, Image>,
    allowed: BTreeSet<String>,
    /// The address bar's text while it is edited.
    address: String,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            back: Vec::new(),
            forward: Vec::new(),
            current: None,
            status: Status::Empty,
            generation: 0,
            find: Find::default(),
            zoom: 1.0,
            scroll_to: None,
            restore: None,
            scroll: 0.0,
            images: BTreeMap::new(),
            allowed: BTreeSet::new(),
            address: String::new(),
        }
    }
}

enum Arrival {
    Page { target: String, generation: u64 },
    Image(String),
}

type Inbox = Arc<Mutex<Vec<(Arrival, Fetched)>>>;

/// The viewer: its pages, history, search, zoom and images. Its methods
/// are what its commands and its panel do.
pub struct Viewer {
    docs: Docs,
    fetch: Option<Arc<dyn Fetch>>,
    inner: Mutex<Inner>,
    inbox: Inbox,
    cache: Mutex<CommonMarkCache>,
}

impl std::fmt::Debug for Viewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Viewer")
            .field("docs", &self.docs)
            .field("fetch", &self.fetch.is_some())
            .field("inner", &lock(&self.inner))
            .finish_non_exhaustive()
    }
}

fn split_fragment(target: &str) -> (&str, Option<&str>) {
    match target.split_once('#') {
        Some((base, f)) => (base, Some(f)),
        None => (target, None),
    }
}

fn extension(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or_default();
    let name = name.split(['?', '#']).next().unwrap_or_default();
    match name.rsplit_once('.') {
        Some((_, ext)) => ext.to_ascii_lowercase(),
        None => String::new(),
    }
}

fn host_of(url: &str) -> Option<String> {
    Target::parse(url)
        .ok()
        .and_then(|t| t.host_ascii().map(str::to_ascii_lowercase))
}

/// The hosts in the setting's text.
pub fn parse_hosts(text: &str) -> BTreeSet<String> {
    text.split([',', ' ', '\n'])
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| !h.is_empty())
        .collect()
}

impl Viewer {
    pub fn new(docs: Docs, fetch: Option<Arc<dyn Fetch>>) -> Self {
        Self {
            docs,
            fetch,
            inner: Mutex::new(Inner::default()),
            inbox: Arc::default(),
            cache: Mutex::new(CommonMarkCache::default()),
        }
    }

    /// The pages the application ships.
    pub fn docs(&self) -> &Docs {
        &self.docs
    }

    /// The page's address with its anchor, if any.
    pub fn target(&self) -> Option<String> {
        lock(&self.inner).current.as_ref().map(|e| e.target.clone())
    }

    /// Where it is.
    pub fn status(&self) -> Status {
        lock(&self.inner).status.clone()
    }

    /// The page's zoom.
    pub fn zoom(&self) -> f32 {
        lock(&self.inner).zoom
    }

    /// Opens a target in the viewer, as the links service does once the
    /// policy agreed (`push`: as a new page of the history).
    pub fn navigate(&self, app: &AppHandle, target: &str, push: bool) {
        let (base, fragment) = split_fragment(target);
        let same = {
            let mut i = lock(&self.inner);
            if push {
                if let Some(mut cur) = i.current.take() {
                    cur.scroll = i.scroll;
                    i.back.push(cur);
                }
                i.forward.clear();
            }
            i.current = Some(Entry {
                target: target.to_string(),
                scroll: 0.0,
            });
            i.address = target.to_string();
            match &i.status {
                Status::Shown(p) if p.target == base && push => Some(p.clone()),
                _ => None,
            }
        };
        match same {
            // An anchor on the page shown: scroll there.
            Some(page) => self.scroll_to_anchor(&page, fragment),
            None => self.load(app, base, fragment),
        }
        app.set_panel_visible(&PANEL, true);
        app.egui().request_repaint();
    }

    fn scroll_to_anchor(&self, page: &Page, fragment: Option<&str>) {
        if let (Content::Markdown(md), Some(f)) = (&page.content, fragment) {
            lock(&self.inner).scroll_to = md.anchor(f);
        }
    }

    fn fail(&self, app: &AppHandle, target: &str, failure: Failure, detail: &str) {
        lock(&self.inner).status = Status::Failed {
            target: target.to_string(),
            failure,
            detail: detail.to_string(),
        };
        app.publish(
            PLUGIN.as_str(),
            None,
            "viewer.failed",
            json!({ "target": target, "failure": failure, "detail": detail }),
        );
    }

    fn load(&self, app: &AppHandle, target: &str, fragment: Option<&str>) {
        let generation = {
            let mut i = lock(&self.inner);
            i.generation += 1;
            i.status = Status::Loading(target.to_string());
            i.scroll_to = None;
            i.generation
        };
        let parsed = Target::parse(target).ok();
        let scheme = parsed.as_ref().map(|t| t.scheme().to_string());
        match scheme.as_deref() {
            Some("docs") => {
                let page = parsed
                    .as_ref()
                    .and_then(Target::doc_page)
                    .unwrap_or_default();
                match self.docs.get(&page) {
                    Some((path, bytes)) => {
                        self.finish(app, target, &path, bytes.to_vec(), fragment);
                    }
                    None => self.fail(
                        app,
                        target,
                        Failure::NotFound,
                        &format!("no page `{page}` among the application's pages"),
                    ),
                }
            }
            Some("file") => {
                let path = parsed
                    .as_ref()
                    .and_then(Target::local_path)
                    .unwrap_or_default();
                match read_file(&path) {
                    Ok(bytes) => self.finish(app, target, &path, bytes, fragment),
                    Err((failure, e)) => self.fail(app, target, failure, &e),
                }
            }
            Some("http" | "https") => match &self.fetch {
                Some(fetch) => {
                    let inbox = self.inbox.clone();
                    let ctx = app.egui();
                    // The anchor stays on the entry: looked up once the
                    // page arrives.
                    let t = target.to_string();
                    fetch.fetch(
                        target,
                        Box::new(move |r| {
                            lock(&inbox).push((
                                Arrival::Page {
                                    target: t,
                                    generation,
                                },
                                r,
                            ));
                            ctx.request_repaint();
                        }),
                    );
                }
                None => self.fail(
                    app,
                    target,
                    Failure::NoConnection,
                    "this application does not load pages from the network",
                ),
            },
            _ => self.fail(
                app,
                target,
                Failure::Unsupported,
                "the viewer shows `docs:` pages, web pages and local images",
            ),
        }
    }

    /// A page's bytes are there: read them for what they are.
    fn finish(
        &self,
        app: &AppHandle,
        target: &str,
        name: &str,
        bytes: Vec<u8>,
        fragment: Option<&str>,
    ) {
        if bytes.len() > MAX_BYTES {
            let detail = format!("{} bytes, more than {MAX_BYTES}", bytes.len());
            return self.fail(app, target, Failure::TooLarge, &detail);
        }
        let ext = extension(name);
        let content = match ext.as_str() {
            "" | "md" | "markdown" | "txt" => {
                let text = String::from_utf8_lossy(&bytes);
                let text = if ext == "txt" {
                    format!("```text\n{text}\n```")
                } else {
                    text.into_owned()
                };
                Content::Markdown(Markdown::parse(&text))
            }
            "png" | "jpg" | "jpeg" | "gif" => {
                app.egui().include_bytes(target.to_string(), bytes);
                Content::Image(target.to_string())
            }
            "html" | "htm" | "xhtml" => {
                return self.fail(
                    app,
                    target,
                    Failure::Unsupported,
                    "HTML pages are not shown inside yet",
                );
            }
            other => {
                let detail = format!("`.{other}` files are not shown inside");
                return self.fail(app, target, Failure::Unsupported, &detail);
            }
        };
        let page = Arc::new(Page {
            target: target.to_string(),
            content,
        });
        if let Content::Markdown(md) = &page.content {
            self.prepare_images(app, target, md);
            let mut cache = lock(&self.cache);
            cache.link_hooks_clear();
            for l in md.links() {
                cache.add_link_hook(l);
            }
        }
        {
            let mut i = lock(&self.inner);
            i.status = Status::Shown(page.clone());
            i.restore = i.current.as_ref().map(|e| e.scroll);
            i.scroll_to = None;
            let query = i.find.query.clone();
            if let Content::Markdown(md) = &page.content {
                i.find.matches = md.find(&query);
                i.find.at = 0;
            }
        }
        self.scroll_to_anchor(&page, fragment);
        app.publish(
            PLUGIN.as_str(),
            None,
            "viewer.shown",
            json!({ "target": target, "kind": page.kind() }),
        );
    }

    /// Hands each image of a page to egui, fetches those it may, and
    /// keeps the others waiting for the user.
    fn prepare_images(&self, app: &AppHandle, base: &str, md: &Markdown) {
        let page_host = host_of(base);
        let ctx = app.egui();
        for img in md.blocks.iter().flat_map(|b| &b.images) {
            let uri = resolve(base, &img.dest);
            if lock(&self.inner).images.contains_key(&uri) {
                continue;
            }
            let parsed = Target::parse(&uri).ok();
            let state = match parsed.as_ref().map(Target::scheme) {
                Some("docs") => {
                    let page = parsed
                        .as_ref()
                        .and_then(Target::doc_page)
                        .unwrap_or_default();
                    match self.docs.get(&page) {
                        Some((_, bytes)) => {
                            ctx.include_bytes(uri.clone(), bytes);
                            Image::Ready
                        }
                        None => Image::Failed(format!("no file `{page}`")),
                    }
                }
                Some("file") => {
                    let path = parsed
                        .as_ref()
                        .and_then(Target::local_path)
                        .unwrap_or_default();
                    match read_file(&path) {
                        Ok(bytes) => {
                            ctx.include_bytes(uri.clone(), bytes);
                            Image::Ready
                        }
                        Err((_, e)) => Image::Failed(e),
                    }
                }
                Some("http" | "https") => {
                    let host = host_of(&uri).unwrap_or_default();
                    let allowed =
                        page_host.as_deref() == Some(host.as_str()) || self.allows(app, &host);
                    if allowed {
                        self.fetch_image(app, &uri)
                    } else {
                        Image::Blocked(host)
                    }
                }
                _ => Image::Failed("not an image the viewer loads".into()),
            };
            lock(&self.inner).images.insert(uri, state);
        }
    }

    fn fetch_image(&self, app: &AppHandle, uri: &str) -> Image {
        let Some(fetch) = &self.fetch else {
            return Image::Failed("this application does not load images from the network".into());
        };
        let inbox = self.inbox.clone();
        let ctx = app.egui();
        let u = uri.to_string();
        fetch.fetch(
            uri,
            Box::new(move |r| {
                lock(&inbox).push((Arrival::Image(u), r));
                ctx.request_repaint();
            }),
        );
        Image::Loading
    }

    /// The hosts the user allowed images from (the setting).
    pub fn allowed_hosts(&self, app: &AppHandle) -> BTreeSet<String> {
        let text = lock(&app.store())
            .get(&format!("settings/{SETTING_IMAGE_HOSTS}"))
            .and_then(|e| match &e.value {
                Value::Text(t) => Some(t.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let hosts = parse_hosts(&text);
        lock(&self.inner).allowed = hosts.clone();
        hosts
    }

    fn allows(&self, app: &AppHandle, host: &str) -> bool {
        self.allowed_hosts(app).contains(host)
    }

    /// Loads the images waiting for a host and remembers the host (the
    /// user's choice: kept from agents).
    pub fn load_images(&self, app: &AppHandle, host: &str) -> Result<usize, String> {
        let host = host.trim().to_ascii_lowercase();
        if host.is_empty() {
            return Err("`host` is missing".into());
        }
        let mut hosts = self.allowed_hosts(app);
        if hosts.insert(host.clone()) {
            let text = hosts.into_iter().collect::<Vec<_>>().join(", ");
            lock(&app.store())
                .set(Set::new(
                    &format!("settings/{SETTING_IMAGE_HOSTS}"),
                    Change::To(Value::Text(text)),
                    Origin::User,
                ))
                .map_err(|e| e.to_string())?;
            self.allowed_hosts(app);
        }
        let waiting: Vec<String> = lock(&self.inner)
            .images
            .iter()
            .filter(|(_, s)| **s == Image::Blocked(host.clone()))
            .map(|(u, _)| u.clone())
            .collect();
        for uri in &waiting {
            let state = self.fetch_image(app, uri);
            lock(&self.inner).images.insert(uri.clone(), state);
        }
        app.egui().request_repaint();
        Ok(waiting.len())
    }

    /// Takes what fetches brought (each pass).
    pub fn poll(&self, app: &AppHandle) {
        let arrived: Vec<(Arrival, Fetched)> = std::mem::take(&mut *lock(&self.inbox));
        for (what, result) in arrived {
            match what {
                Arrival::Page { target, generation } => {
                    if lock(&self.inner).generation != generation {
                        continue;
                    }
                    let fragment = self
                        .target()
                        .and_then(|t| split_fragment(&t).1.map(str::to_string));
                    match result {
                        Ok(bytes) => self.finish(app, &target, &target, bytes, fragment.as_deref()),
                        Err(e) => self.fail(app, &target, Failure::NoConnection, &e),
                    }
                }
                Arrival::Image(uri) => {
                    let state = match result {
                        Ok(bytes) if bytes.len() > MAX_BYTES => {
                            Image::Failed(format!("{} bytes, too large", bytes.len()))
                        }
                        Ok(bytes) => {
                            app.egui().include_bytes(uri.clone(), bytes);
                            Image::Ready
                        }
                        Err(e) => Image::Failed(e),
                    };
                    lock(&self.inner).images.insert(uri, state);
                }
            }
        }
    }

    /// The previous page.
    pub fn back(&self, app: &AppHandle) -> bool {
        self.walk(app, true)
    }

    /// The next page.
    pub fn forward(&self, app: &AppHandle) -> bool {
        self.walk(app, false)
    }

    fn walk(&self, app: &AppHandle, back: bool) -> bool {
        let target = {
            let mut i = lock(&self.inner);
            let next = if back { i.back.pop() } else { i.forward.pop() };
            let Some(next) = next else {
                return false;
            };
            if let Some(mut cur) = i.current.take() {
                cur.scroll = i.scroll;
                if back {
                    i.forward.push(cur);
                } else {
                    i.back.push(cur);
                }
            }
            i.address = next.target.clone();
            let t = next.target.clone();
            i.current = Some(next);
            t
        };
        let (base, fragment) = split_fragment(&target);
        self.load(app, base, fragment);
        true
    }

    /// The page again (its remote images fetched again too).
    pub fn reload(&self, app: &AppHandle) -> bool {
        let Some(target) = self.target() else {
            return false;
        };
        {
            let mut i = lock(&self.inner);
            i.images.retain(|_, s| !matches!(s, Image::Failed(_)));
            let scroll = i.scroll;
            if let Some(c) = i.current.as_mut() {
                c.scroll = scroll;
            }
        }
        let (base, fragment) = split_fragment(&target);
        self.load(app, base, fragment);
        true
    }

    /// Opens the search (`query`: searches it and goes to its first
    /// match); returns the number of matches.
    pub fn find(&self, query: Option<&str>) -> usize {
        let mut i = lock(&self.inner);
        i.find.open = true;
        i.find.focus = true;
        if let Some(q) = query {
            i.find.query = q.to_string();
        }
        self.refind(&mut i);
        i.find.matches.len()
    }

    fn refind(&self, i: &mut Inner) {
        let matches = match &i.status {
            Status::Shown(p) => match &p.content {
                Content::Markdown(md) => md.find(&i.find.query),
                Content::Image(_) => Vec::new(),
            },
            _ => Vec::new(),
        };
        i.find.matches = matches;
        i.find.at = 0;
        i.scroll_to = i.find.matches.first().copied();
    }

    /// The next (or previous) match, around the page; its number from
    /// 1, none without matches.
    pub fn find_step(&self, next: bool) -> Option<usize> {
        let mut i = lock(&self.inner);
        let n = i.find.matches.len();
        if n == 0 {
            return None;
        }
        i.find.at = if next {
            (i.find.at + 1) % n
        } else {
            (i.find.at + n - 1) % n
        };
        i.scroll_to = Some(i.find.matches[i.find.at]);
        Some(i.find.at + 1)
    }

    /// Closes the search.
    pub fn close_find(&self) {
        let mut i = lock(&self.inner);
        i.find.open = false;
        i.find.matches.clear();
    }

    /// Moves the zoom by steps (0: back to 100%); returns it.
    pub fn zoom_step(&self, by: i32) -> f32 {
        let mut i = lock(&self.inner);
        i.zoom = if by == 0 {
            1.0
        } else {
            let at = ZOOM_STEPS
                .iter()
                .position(|z| (*z - i.zoom).abs() < 0.001)
                .unwrap_or(5) as i32;
            ZOOM_STEPS[(at + by).clamp(0, ZOOM_STEPS.len() as i32 - 1) as usize]
        };
        i.zoom
    }

    /// The source a page's own links come with: declared for the
    /// application's `docs:` pages, content otherwise.
    fn source_of(target: &str) -> Source {
        if target.starts_with("docs:") {
            Source::Declared
        } else {
            Source::Content
        }
    }

    /// Opens the page outside (a `docs:` page's online copy).
    pub fn open_outside(&self, app: &AppHandle, origin: &Origin) -> Result<Json, String> {
        let target = self.target().ok_or("nothing is open")?;
        let req = Request::new(&target)
            .how(How::Outside)
            .source(Self::source_of(&target))
            .origin(origin.clone());
        match links::open(app, req) {
            egui_pop_api::steps::Outcome::Done(r) => r,
            egui_pop_api::steps::Outcome::Vetoed { by, reason } => {
                Err(format!("vetoed by {by}: {reason}"))
            }
        }
    }

    /// Copies the page's address.
    pub fn copy_link(&self, app: &AppHandle, origin: &Origin) -> Result<Json, String> {
        let target = self.target().ok_or("nothing is open")?;
        links::copy_link(app, &target, origin);
        Ok(json!({ "copied": target }))
    }

    /// Follows a link of the page shown (`dest` as written), outside when
    /// asked: an anchor scrolls, the rest goes through the links service.
    pub fn follow(&self, app: &AppHandle, dest: &str, outside: bool) {
        let Some(page) = (match self.status() {
            Status::Shown(p) => Some(p),
            _ => None,
        }) else {
            return;
        };
        let resolved = resolve(&page.target, dest);
        let (base, fragment) = split_fragment(&resolved);
        if base.is_empty() || (base == page.target && !outside) {
            self.scroll_to_anchor(&page, fragment);
            return;
        }
        let how = if outside { How::Outside } else { How::Auto };
        links::open(
            app,
            Request::new(&resolved)
                .how(how)
                .source(Self::source_of(&page.target)),
        );
    }

    /// Opens what the user typed in the address bar: an address, or a
    /// `docs:` page by its name.
    pub fn go(&self, app: &AppHandle, typed: &str) {
        let typed = typed.trim();
        if typed.is_empty() {
            return;
        }
        let target = if has_scheme(typed) || typed.starts_with('/') || typed.starts_with('\\') {
            typed.to_string()
        } else {
            format!("docs:{typed}")
        };
        if target.starts_with("command:") {
            self.fail(
                app,
                &target,
                Failure::Unsupported,
                "commands do not run from the address bar",
            );
            return;
        }
        links::open(app, Request::new(&target).source(Source::Declared));
    }

    /// What the viewer shows and holds, for agents and the collector.
    pub fn state(&self) -> Json {
        let i = lock(&self.inner);
        let (status, kind, title, error, text, headings) = match &i.status {
            Status::Empty => ("empty", None, None, None, None, Vec::new()),
            Status::Loading(_) => ("loading", None, None, None, None, Vec::new()),
            Status::Shown(p) => {
                let (title, text, headings) = match &p.content {
                    Content::Markdown(md) => (
                        md.title.clone(),
                        Some(md.text().chars().take(4000).collect::<String>()),
                        md.blocks
                            .iter()
                            .filter_map(|b| {
                                Some(json!({ "anchor": b.anchor.clone()?, "level": b.level }))
                            })
                            .collect(),
                    ),
                    Content::Image(_) => (None, None, Vec::new()),
                };
                ("shown", Some(p.kind()), title, None, text, headings)
            }
            Status::Failed {
                failure, detail, ..
            } => (
                "failed",
                None,
                None,
                Some(json!({ "failure": failure, "detail": detail })),
                None,
                Vec::new(),
            ),
        };
        let blocked: BTreeSet<&String> = i
            .images
            .values()
            .filter_map(|s| match s {
                Image::Blocked(h) => Some(h),
                _ => None,
            })
            .collect();
        let images: Vec<Json> = i
            .images
            .iter()
            .map(|(u, s)| {
                let (state, why) = match s {
                    Image::Ready => ("ready", None),
                    Image::Loading => ("loading", None),
                    Image::Blocked(_) => ("blocked", None),
                    Image::Failed(e) => ("failed", Some(e)),
                };
                json!({ "uri": u, "state": state, "error": why })
            })
            .collect();
        json!({
            "target": i.current.as_ref().map(|e| e.target.clone()),
            "status": status,
            "kind": kind,
            "title": title,
            "error": error,
            "back": i.back.iter().map(|e| e.target.clone()).collect::<Vec<_>>(),
            "forward": i.forward.iter().rev().map(|e| e.target.clone()).collect::<Vec<_>>(),
            "find": {
                "open": i.find.open,
                "query": i.find.query,
                "matches": i.find.matches.len(),
                "at": (!i.find.matches.is_empty()).then_some(i.find.at + 1),
            },
            "zoom": i.zoom,
            "images": images,
            "blocked_hosts": blocked,
            "allowed_hosts": i.allowed,
            "headings": headings,
            "text": text,
            "fetch": self.fetch.is_some(),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_file(path: &str) -> Result<Vec<u8>, (Failure, String)> {
    let meta = std::fs::metadata(path).map_err(|e| (Failure::NotFound, e.to_string()))?;
    if meta.len() > MAX_BYTES as u64 {
        return Err((
            Failure::TooLarge,
            format!("{} bytes, more than {MAX_BYTES}", meta.len()),
        ));
    }
    std::fs::read(path).map_err(|e| (Failure::NotFound, e.to_string()))
}

#[cfg(target_arch = "wasm32")]
fn read_file(_: &str) -> Result<Vec<u8>, (Failure, String)> {
    Err((
        Failure::Unsupported,
        "local files are not read on the web".into(),
    ))
}

/// The links service's way inside: the viewer.
#[derive(Debug)]
struct Shows(Arc<Viewer>);

impl Inside for Shows {
    fn name(&self) -> &str {
        "viewer"
    }

    fn show(&self, app: &AppHandle, call: &OpenCall<'_>) -> Result<(), String> {
        self.0.navigate(app, call.target.as_str(), true);
        Ok(())
    }

    fn docs_pages(&self) -> Option<Vec<String>> {
        Some(self.0.docs.pages())
    }
}

/// The viewer as a plugin (opt-in): the panel, the service, the links
/// service's way inside, the commands, the setting and the `viewer`
/// collector.
#[derive(Default)]
pub struct ViewerPlugin {
    docs: Docs,
    fetch: Option<Arc<dyn Fetch>>,
}

impl std::fmt::Debug for ViewerPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewerPlugin")
            .field("docs", &self.docs)
            .field("fetch", &self.fetch.is_some())
            .finish()
    }
}

impl ViewerPlugin {
    /// The `docs:` pages the application ships.
    pub fn docs(mut self, docs: Docs) -> Self {
        self.docs = docs;
        self
    }

    /// How remote pages and images are fetched (none: never).
    pub fn fetch(mut self, fetch: Arc<dyn Fetch>) -> Self {
        self.fetch = Some(fetch);
        self
    }
}

fn surface() -> BindOptions {
    BindOptions::surface(SURFACE)
}

fn viewer(app: &AppHandle) -> Result<Arc<Viewer>, String> {
    app.service(&VIEWER)
        .ok_or_else(|| "the viewer plugin is not connected".to_string())
}

impl Plugin for ViewerPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Viewer".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![links::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let app = ctx.app();
        ctx.service(&VIEWER, Viewer::new(self.docs.clone(), self.fetch.clone()))?;
        let v = app
            .service(&VIEWER)
            .ok_or_else(|| PluginError::Failed("the viewer service is missing".into()))?;
        if let Some(l) = links::links(&app) {
            l.set_inside(Some(Arc::new(Shows(v.clone()))));
        }
        // Decoding only: no file or http loader, the viewer hands the
        // bytes over itself.
        egui_extras::install_image_loaders(&app.egui());
        ctx.setting(
            SETTING_IMAGE_HOSTS,
            Descriptor::text("Images loaded from", "").description(
                "The hosts whose images the viewer loads, each allowed by you once; empty to be asked again.",
            ),
        )?;
        ctx.panel(
            PanelSpec::new(PANEL, "Viewer", Area::EndSide)
                .min_size(240.0)
                .default_size(420.0)
                .icon("help")
                .closed(),
            |ui, app| {
                if let Ok(v) = viewer(app) {
                    view::show(ui, app, &v);
                }
            },
        )?;
        commands(ctx)?;
        let c = v.clone();
        ctx.collector(
            FnCollector::new(
                "viewer",
                "The viewer: the page (its address, status or error, title, headings and text), the history back and forward, the search, the zoom, the images (ready, loading, blocked by host, failed) and the hosts allowed.",
            )
            .snapshot(move |_| c.state()),
        )?;
        ctx.on(egui_pop_api::hooks::On::Pass, |app, _| {
            if let Ok(v) = viewer(app) {
                v.poll(app);
            }
        });
        Ok(())
    }

    fn disconnect(&mut self, ctx: &mut Ctx<'_>) {
        if let Some(l) = links::links(&ctx.app()) {
            l.set_inside(None);
        }
    }
}

fn commands(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    const CAT: &str = "Viewer";
    ctx.command(
        CommandSpec::new(CMD_BACK, "Back", CAT)
            .description("Shows the viewer's previous page.")
            .key_with("Alt+Left", surface()),
        |call| Ok(json!({ "moved": viewer(call.app)?.back(call.app) })),
    )?
    .command(
        CommandSpec::new(CMD_FORWARD, "Forward", CAT)
            .description("Shows the viewer's next page, after going back.")
            .key_with("Alt+Right", surface()),
        |call| Ok(json!({ "moved": viewer(call.app)?.forward(call.app) })),
    )?
    .command(
        CommandSpec::new(CMD_RELOAD, "Reload the page", CAT)
            .description("Loads the viewer's page again.")
            .key_with("F5", surface()),
        |call| Ok(json!({ "reloaded": viewer(call.app)?.reload(call.app) })),
    )?
    .command(
        CommandSpec::new(CMD_FIND, "Find in page", CAT)
            .description("Opens the viewer's search; with `query`, searches the page and goes to the first match.")
            .key_with("Ctrl+F", surface())
            .args(json!({ "query": "install" })),
        |call| {
            let v = viewer(call.app)?;
            let n = v.find(call.args["query"].as_str());
            Ok(json!({ "matches": n }))
        },
    )?
    .command(
        CommandSpec::new(CMD_FIND_NEXT, "Find next in page", CAT)
            .description("Goes to the next match of the viewer's search.")
            .key_with("F3", surface()),
        |call| Ok(json!({ "at": viewer(call.app)?.find_step(true) })),
    )?
    .command(
        CommandSpec::new(CMD_FIND_PREVIOUS, "Find previous in page", CAT)
            .description("Goes to the previous match of the viewer's search.")
            .key_with("Shift+F3", surface()),
        |call| Ok(json!({ "at": viewer(call.app)?.find_step(false) })),
    )?
    .command(
        CommandSpec::new(CMD_ZOOM_IN, "Zoom in the page", CAT)
            .description("Makes the viewer's page larger, one step.")
            .key_with("Ctrl+Plus", surface())
            .key_with("Ctrl+Equals", surface()),
        |call| Ok(json!({ "zoom": viewer(call.app)?.zoom_step(1) })),
    )?
    .command(
        CommandSpec::new(CMD_ZOOM_OUT, "Zoom out the page", CAT)
            .description("Makes the viewer's page smaller, one step.")
            .key_with("Ctrl+Minus", surface()),
        |call| Ok(json!({ "zoom": viewer(call.app)?.zoom_step(-1) })),
    )?
    .command(
        CommandSpec::new(CMD_ZOOM_RESET, "Reset the page zoom", CAT)
            .description("Gives the viewer's page its normal size.")
            .key_with("Ctrl+0", surface()),
        |call| Ok(json!({ "zoom": viewer(call.app)?.zoom_step(0) })),
    )?
    .command(
        CommandSpec::new(CMD_OPEN_OUTSIDE, "Open the page outside", CAT)
            .description("Opens the viewer's page outside the application (a `docs:` page's online copy), through the links service; an agent's request is confirmed by the user."),
        |call| viewer(call.app)?.open_outside(call.app, call.origin),
    )?
    .command(
        CommandSpec::new(CMD_COPY_LINK, "Copy the page's link", CAT)
            .description("Copies the address of the viewer's page."),
        |call| viewer(call.app)?.copy_link(call.app, call.origin),
    )?
    .command(
        CommandSpec::new(CMD_LOAD_IMAGES, "Load images from a host", CAT)
            .description("Loads the images of a host (`host`) the viewer's page shows, and remembers the host; the user's choice alone.")
            .args(json!({ "host": "images.example.org" }))
            .hidden()
            .no_agent(),
        |call| {
            if *call.origin != Origin::User {
                return Err("only the user allows a host's images".into());
            }
            let host = call.args["host"].as_str().ok_or("`host` is missing")?;
            let n = viewer(call.app)?.load_images(call.app, host)?;
            Ok(json!({ "host": host, "loading": n }))
        },
    )?
    .command(
        CommandSpec::new(CMD_STATE, "Read the viewer", CAT)
            .description("The viewer's page (address, status or error, title, headings, text), its history, search, zoom, images and the hosts allowed.")
            .hidden(),
        |call| {
            let v = viewer(call.app)?;
            v.allowed_hosts(call.app);
            Ok(v.state())
        },
    )?;
    Ok(())
}
