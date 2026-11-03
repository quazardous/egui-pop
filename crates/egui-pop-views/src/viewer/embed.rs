//! The viewer's YouTube page (ADR 0017): a YouTube link played in a web view,
//! through an [`EmbedBackend`].
//!
//! The backend is wry over WebKitGTK with the `webview` feature on Linux, none
//! elsewhere or without it (YouTube links then open outside, as any web video;
//! the policy asks [`Viewer::plays_web_video`]). What is here knows nothing of
//! wry, so a host can give its own web view and a test a pretend one.
//!
//! - **What loads**: YouTube's privacy-enhanced player,
//!   `https://www.youtube-nocookie.com/embed/<id>`, as the page itself, its
//!   request carrying a `Referer` (the [`ViewerPlugin`](super::ViewerPlugin)'s
//!   [`embed_referrer`](super::ViewerPlugin::embed_referrer), `https://localhost/`
//!   by default): without one YouTube's player refuses to play (its error 153).
//!   Nothing else loads in it: a link the player follows (its title, "Watch on
//!   YouTube") goes through the links service, outside.
//! - **Where**: inside the panel when the panel is in the main window and that
//!   window is an X11 one (a child window over the panel's rectangle, following
//!   it as it moves and resizes); in a window of its own otherwise (Wayland, a
//!   detached panel). A child web view is a native window above egui: egui's
//!   popups over the panel are drawn under it.
//! - **Its loop**: a web view needs its toolkit's loop turned on the interface's
//!   thread; the page asks for a pass every [`PUMP`] while a web view is open and
//!   turns that loop once per pass, without blocking.
//! - **Stopping**: leaving the page closes the web view. So does a panel no
//!   longer drawn (closed, or a tab behind another), or the user closing its own
//!   window: drawing the page again opens it again, from the start.

use std::sync::Arc;
use std::time::Duration;

use egui::{Align2, Rect, Sense, Ui, vec2};
use egui_pop_api::AppHandle;
use egui_pop_api::links::{self, How, Request, Source};
use egui_pop_core::lock;
use egui_pop_core::strings::{tr, tr_args};
use serde_json::{Value as Json, json};

use super::{Failure, PLUGIN, Status, Viewer};
use crate::strings as txt;

/// What the web view's request says it comes from, unless the application
/// gives its own address ([`ViewerPlugin::embed_referrer`](super::ViewerPlugin::embed_referrer)).
pub const DEFAULT_REFERRER: &str = "https://localhost/";
/// How often the web view's loop is turned while it is open.
pub const PUMP: Duration = Duration::from_millis(15);
/// The application's passes without the page before its panel counts as
/// hidden: a pass may run before the panel is drawn in it.
const HIDDEN_PASSES: u32 = 3;

/// The player's address for a YouTube video: the privacy-enhanced host, the
/// video starting on its own.
pub fn embed_url(id: &str) -> String {
    format!("https://www.youtube-nocookie.com/embed/{id}?autoplay=1&playsinline=1&rel=0")
}

/// Where a web view goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// In the window's physical pixels, from its inner top-left corner.
    pub rect: Rect,
    /// The main window's X11 id when the page is drawn in it: the web view is
    /// made its child. None: a window of its own.
    pub parent_x11: Option<u64>,
}

/// What a backend is asked to open.
#[derive(Clone, Debug)]
pub struct EmbedRequest {
    /// The page to load ([`embed_url`]).
    pub url: String,
    /// The `Referer` its request carries.
    pub referrer: String,
    /// The title of its own window, when it has one.
    pub title: String,
    pub place: Placement,
    /// For the backend to ask for a pass.
    pub egui: egui::Context,
}

/// A web view open. A failure's text is shown on the error page.
pub trait Embedded: Send {
    /// Moves and sizes it (a child; its own window ignores this).
    fn place(&mut self, place: &Placement) -> Result<(), String>;
    /// Turns the web view's loop once, without blocking.
    fn pump(&mut self) -> Result<(), String>;
    /// Whether it is in a window of its own.
    fn own_window(&self) -> bool;
    /// Whether the user closed its own window.
    fn closed(&self) -> bool;
    /// The addresses it was asked to go to and did not: opened outside, through
    /// the links service.
    fn take_links(&mut self) -> Vec<String>;
}

/// What opens web views for the viewer ([`ViewerPlugin::web_video`](super::ViewerPlugin::web_video)).
pub trait EmbedBackend: Send + Sync {
    /// Its name, in `viewer.state`.
    fn name(&self) -> &str;
    /// A web view loading the request's page, or why not. It closes when
    /// dropped.
    fn open(&self, request: &EmbedRequest) -> Result<Box<dyn Embedded>, String>;
}

/// The backend of this build.
pub(super) fn default_backend() -> Option<Arc<dyn EmbedBackend>> {
    #[cfg(all(feature = "webview", target_os = "linux"))]
    {
        Some(Arc::new(super::wry_view::Wry))
    }
    #[cfg(not(all(feature = "webview", target_os = "linux")))]
    {
        None
    }
}

/// The YouTube page's state.
struct Session {
    target: String,
    id: String,
    /// None before the page is first drawn, while its panel is hidden, and
    /// once its own window was closed.
    view: Option<Box<dyn Embedded>>,
    own_window: bool,
    /// The user closed its own window: it opens again on a click only.
    closed: bool,
    /// Passes since the page was last drawn.
    unseen: u32,
}

/// The viewer's web view state: the backend, the referrer, the page.
pub(super) struct Embed {
    backend: Option<Arc<dyn EmbedBackend>>,
    referrer: String,
    session: Option<Session>,
}

impl Embed {
    pub(super) fn new(backend: Option<Arc<dyn EmbedBackend>>) -> Self {
        Self {
            backend,
            referrer: DEFAULT_REFERRER.to_string(),
            session: None,
        }
    }

    pub(super) fn set_backend(&mut self, backend: Option<Arc<dyn EmbedBackend>>) {
        self.backend = backend;
    }

    pub(super) fn set_referrer(&mut self, referrer: String) {
        self.referrer = referrer;
    }
}

impl Viewer {
    /// Whether YouTube links open here: a web view backend is there.
    pub fn plays_web_video(&self) -> bool {
        lock(&self.embed).backend.is_some()
    }

    /// Replaces what opens web views (none: YouTube links open outside).
    pub fn set_embed_backend(&self, backend: Option<Arc<dyn EmbedBackend>>) {
        lock(&self.embed).set_backend(backend);
    }

    /// Shows the YouTube page; its web view opens when the page is drawn.
    pub(super) fn open_embed(&self, app: &AppHandle, target: &str, id: String) {
        lock(&self.embed).session = Some(Session {
            target: target.to_string(),
            id: id.clone(),
            view: None,
            own_window: false,
            closed: false,
            unseen: 0,
        });
        let page = Arc::new(super::Page {
            target: target.to_string(),
            content: super::Content::YouTube(id),
            html: false,
            dropped: Vec::new(),
        });
        {
            let mut i = lock(&self.inner);
            i.status = Status::Shown(page);
            i.restore = None;
            i.scroll_to = None;
            i.find.matches.clear();
            i.find.at = 0;
        }
        app.publish(
            PLUGIN.as_str(),
            None,
            "viewer.shown",
            json!({ "target": target, "kind": "youtube" }),
        );
    }

    /// Leaves the YouTube page: the web view closes.
    pub(super) fn stop_embed(&self) {
        let session = lock(&self.embed).session.take();
        drop(session);
    }

    fn embed_failed(&self, app: &AppHandle, target: &str, detail: &str) {
        self.stop_embed();
        self.fail(app, target, Failure::Playback, detail);
    }

    /// Once per pass of the application: turns the web view's loop, opens
    /// outside what it was asked to go to, closes it when its page is no
    /// longer drawn.
    pub(super) fn embed_pass(&self, app: &AppHandle) {
        let (failed, links, hidden) = {
            let mut e = lock(&self.embed);
            let Some(s) = e.session.as_mut() else {
                return;
            };
            let Some(view) = s.view.as_mut() else {
                return;
            };
            s.unseen += 1;
            if s.unseen >= HIDDEN_PASSES {
                (None, Vec::new(), s.view.take())
            } else {
                let failed = view.pump().err().map(|e| (s.target.clone(), e));
                let links = view.take_links();
                if failed.is_none() && view.closed() {
                    s.closed = true;
                    s.view = None;
                }
                (failed, links, None)
            }
        };
        if hidden.is_some() {
            drop(hidden);
            app.publish(PLUGIN.as_str(), None, "viewer.embed_stopped", json!({}));
            return;
        }
        for url in links {
            links::open(
                app,
                Request::new(&url).source(Source::Content).how(How::Outside),
            );
        }
        if let Some((target, e)) = failed {
            return self.embed_failed(app, &target, &e);
        }
        if lock(&self.embed)
            .session
            .as_ref()
            .is_some_and(|s| s.view.is_some())
        {
            app.egui().request_repaint_after(PUMP);
        }
    }

    /// The page is drawn at `place`: its web view opened (again) or moved
    /// there.
    fn embed_in_view(&self, app: &AppHandle, place: Placement) {
        let open = {
            let mut e = lock(&self.embed);
            let referrer = e.referrer.clone();
            let backend = e.backend.clone();
            let Some(s) = e.session.as_mut() else {
                return;
            };
            s.unseen = 0;
            match s.view.as_mut() {
                Some(view) => {
                    if let Err(err) = view.place(&place) {
                        Some(Err((s.target.clone(), err)))
                    } else {
                        None
                    }
                }
                None if s.closed => None,
                None => Some(Ok((s.target.clone(), s.id.clone(), referrer, backend))),
            }
        };
        match open {
            None => {}
            Some(Err((target, e))) => self.embed_failed(app, &target, &e),
            Some(Ok((target, id, referrer, backend))) => {
                let Some(backend) = backend else {
                    return self.embed_failed(app, &target, "this application has no web view");
                };
                let request = EmbedRequest {
                    url: embed_url(&id),
                    referrer,
                    title: format!("YouTube: {id}"),
                    place,
                    egui: app.egui(),
                };
                match backend.open(&request) {
                    Ok(view) => {
                        if let Some(s) = lock(&self.embed).session.as_mut() {
                            s.own_window = view.own_window();
                            s.view = Some(view);
                        }
                        app.publish(
                            PLUGIN.as_str(),
                            None,
                            "viewer.embed_opened",
                            json!({ "target": target, "own_window": place.parent_x11.is_none() }),
                        );
                        app.egui().request_repaint_after(PUMP);
                    }
                    Err(e) => self.embed_failed(app, &target, &e),
                }
            }
        }
    }

    /// Opens again the web view whose own window the user closed.
    pub fn embed_reopen(&self, app: &AppHandle) {
        if let Some(s) = lock(&self.embed).session.as_mut() {
            s.closed = false;
        }
        app.egui().request_repaint();
    }

    /// What the page holds, for `viewer.state`: null without a YouTube page.
    pub(super) fn embed_state(&self) -> Json {
        let e = lock(&self.embed);
        let Some(s) = e.session.as_ref() else {
            return Json::Null;
        };
        json!({
            "backend": e.backend.as_ref().map(|b| b.name().to_string()),
            "id": s.id,
            "url": embed_url(&s.id),
            "referrer": e.referrer,
            "open": s.view.is_some(),
            "own_window": s.own_window,
            "closed": s.closed,
        })
    }
}

/// Draws the page: the web view's place, 16:9 in the panel, the web view put
/// over it (or a word saying it is in its own window).
pub(super) fn draw(ui: &mut Ui, app: &AppHandle, v: &Viewer) {
    let ctx = ui.ctx().clone();
    let (id, own_window, closed) = {
        let e = lock(&v.embed);
        let Some(s) = e.session.as_ref() else {
            return;
        };
        (s.id.clone(), s.own_window && s.view.is_some(), s.closed)
    };
    let width = ui.available_width();
    let height = (width * 9.0 / 16.0).min(ui.available_height()).max(1.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    egui_pop_core::a11y::name(
        &response,
        egui::WidgetType::Image,
        &tr_args(&ctx, &txt::VIEWER_VIDEO_PICTURE, &[("name", &id)]),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
    let text = if own_window || closed {
        tr(&ctx, &txt::VIEWER_EMBED_OWN_WINDOW)
    } else {
        tr(&ctx, &txt::VIEWER_VIDEO_LOADING)
    };
    // Wrapped to the place's width: a narrow panel keeps the whole sentence.
    let galley = ui.painter().layout(
        text,
        egui::TextStyle::Body.resolve(ui.style()),
        ui.visuals().weak_text_color(),
        (rect.width() - 2.0 * ui.spacing().item_spacing.x).max(1.0),
    );
    let at = Align2::CENTER_CENTER
        .anchor_size(rect.center(), galley.size())
        .min;
    painter.galley(at, galley, ui.visuals().weak_text_color());
    if closed && ui.button(tr(&ctx, &txt::VIEWER_EMBED_SHOW)).clicked() {
        v.embed_reopen(app);
    }
    let visible = ui.clip_rect().intersect(rect);
    let in_main = ctx.viewport_id() == egui::ViewportId::ROOT;
    let place = Placement {
        rect: Rect::from_min_max(
            (visible.min.to_vec2() * ctx.pixels_per_point()).to_pos2(),
            (visible.max.to_vec2() * ctx.pixels_per_point()).to_pos2(),
        ),
        parent_x11: in_main
            .then(|| egui_pop_api::frame::x11_window(&ctx))
            .flatten(),
    };
    v.embed_in_view(app, place);
}
