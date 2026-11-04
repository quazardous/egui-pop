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
//!   request carrying a `Referer` that names the application, as YouTube's
//!   Required Minimum Functionality asks of desktop applications:
//!   `https://<app id>/` ([`referrer_for`], the id of
//!   `AppBuilder::app_id`), or the [`ViewerPlugin`](super::ViewerPlugin)'s
//!   [`embed_referrer`](super::ViewerPlugin::embed_referrer). Without either,
//!   YouTube links open outside. Nothing else loads in it: a link the player
//!   follows (its title, "Watch on YouTube") goes through the links service,
//!   outside.
//! - **Its size**: YouTube's player needs at least [`MIN_PLAYER`] pixels each
//!   way ([`fits_player`]). A panel smaller than that shows a card with Open
//!   outside in place of the web view, which opens (from the start) once the
//!   panel is large enough again; a window of its own is kept at least that
//!   large by the backend.
//! - **Nothing over it**: the page draws nothing on the player's place but a
//!   background and a word under the web view, which covers them.
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
use egui_pop_api::strings as api_txt;
use egui_pop_core::icon::{Icon, Size, Tone};
use egui_pop_core::lock;
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_core::theme::Tokens;
use serde_json::{Value as Json, json};

use super::{Failure, PLUGIN, Status, Viewer};
use crate::strings as txt;

/// The `Referer` the web view sent before it named the application.
#[deprecated(
    since = "0.4.2",
    note = "the `Referer` names the application (`referrer_for`, `AppBuilder::app_id`): YouTube's terms rule out a shared one"
)]
pub const DEFAULT_REFERRER: &str = "https://localhost/";
/// The smallest side of YouTube's player, in its own (CSS) pixels: its
/// Required Minimum Functionality asks for a viewport of at least 200 by 200.
pub const MIN_PLAYER: f32 = 200.0;
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

/// The `Referer` naming the application of id `app_id` (`org.example.App`),
/// as YouTube asks of an application: an `https` URL whose domain is the
/// application's id, `https://org.example.app/`. None for an empty id or one
/// that cannot be a domain name.
pub fn referrer_for(app_id: &str) -> Option<String> {
    let id = app_id.trim();
    let valid = !id.is_empty()
        && !id.starts_with('.')
        && !id.ends_with('.')
        && !id.contains("..")
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    valid.then(|| format!("https://{}/", id.to_ascii_lowercase()))
}

/// Whether a player of `physical` pixels meets YouTube's minimum
/// ([`MIN_PLAYER`] each way). `native_pixels_per_point` is the system's
/// scale: the web view's pixels are the window's physical pixels divided by
/// it, so a 2x screen needs 400 physical pixels. Both counts must reach the
/// minimum.
pub fn fits_player(physical: egui::Vec2, native_pixels_per_point: f32) -> bool {
    let scale = native_pixels_per_point.max(1.0);
    physical.min_elem() / scale >= MIN_PLAYER
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
    /// Whether a web view opened at `place` sits over the panel, so that the
    /// panel's place is the player's size (checked against [`MIN_PLAYER`]);
    /// false: a window of its own, which the backend keeps at least
    /// [`MIN_PLAYER`] each way. By default: a child of an X11 window.
    fn in_panel(&self, place: &Placement) -> bool {
        place.parent_x11.is_some()
    }
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
    /// The panel is too small for the player: the card shows.
    small: bool,
}

/// The viewer's web view state: the backend, the referrer, the page.
pub(super) struct Embed {
    backend: Option<Arc<dyn EmbedBackend>>,
    /// None: the application has no id, YouTube links open outside.
    referrer: Option<String>,
    session: Option<Session>,
}

impl Embed {
    pub(super) fn new(backend: Option<Arc<dyn EmbedBackend>>) -> Self {
        Self {
            backend,
            referrer: None,
            session: None,
        }
    }

    pub(super) fn set_backend(&mut self, backend: Option<Arc<dyn EmbedBackend>>) {
        self.backend = backend;
    }

    pub(super) fn has_backend(&self) -> bool {
        self.backend.is_some()
    }

    pub(super) fn set_referrer(&mut self, referrer: Option<String>) {
        self.referrer = referrer;
    }
}

impl Viewer {
    /// Whether YouTube links open here: a web view backend is there, and a
    /// `Referer` naming the application (its id, or
    /// [`ViewerPlugin::embed_referrer`](super::ViewerPlugin::embed_referrer)).
    pub fn plays_web_video(&self) -> bool {
        let e = lock(&self.embed);
        e.backend.is_some() && e.referrer.is_some()
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
            small: false,
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
    /// there. False: the panel is too small for the player (the card shows,
    /// the web view is closed).
    fn embed_in_view(&self, app: &AppHandle, place: Placement, native_ppp: f32) -> bool {
        let open = {
            let mut e = lock(&self.embed);
            let referrer = e.referrer.clone();
            let backend = e.backend.clone();
            let Some(s) = e.session.as_mut() else {
                return true;
            };
            s.unseen = 0;
            let in_panel = backend.as_ref().is_some_and(|b| b.in_panel(&place));
            let small = in_panel && !fits_player(place.rect.size(), native_ppp);
            let was_small = std::mem::replace(&mut s.small, small);
            if small {
                let dropped = s.view.take();
                let target = s.target.clone();
                drop(e);
                let had = dropped.is_some();
                drop(dropped);
                if !was_small {
                    app.publish(
                        PLUGIN.as_str(),
                        None,
                        "viewer.embed_too_small",
                        json!({ "target": target, "closed": had }),
                    );
                }
                return false;
            }
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
                    self.embed_failed(app, &target, "this application has no web view");
                    return true;
                };
                let Some(referrer) = referrer else {
                    self.embed_failed(app, &target, "this application has no id to give YouTube");
                    return true;
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
        true
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
            "too_small": s.small,
            "min": MIN_PLAYER,
        })
    }
}

/// Draws the page: the web view's place, 16:9 in the panel and at least
/// [`MIN_PLAYER`] tall when the panel allows, the web view put over it (or a
/// word saying it is in its own window); a card with Open outside when the
/// panel is too small for the player. True: Open outside was clicked.
pub(super) fn draw(ui: &mut Ui, app: &AppHandle, v: &Viewer) -> bool {
    let ctx = ui.ctx().clone();
    let (id, own_window, closed) = {
        let e = lock(&v.embed);
        let Some(s) = e.session.as_ref() else {
            return false;
        };
        (s.id.clone(), s.own_window && s.view.is_some(), s.closed)
    };
    let width = ui.available_width();
    let height = (width * 9.0 / 16.0)
        .max(MIN_PLAYER)
        .min(ui.available_height())
        .max(1.0);
    // Where the place will be, decided before it is laid out: a panel too
    // small gets the card in its stead.
    let rect = Rect::from_min_size(ui.cursor().min, vec2(width, height));
    let visible = ui.clip_rect().intersect(rect);
    let ppp = ctx.pixels_per_point();
    let in_main = ctx.viewport_id() == egui::ViewportId::ROOT;
    let place = Placement {
        rect: Rect::from_min_max(
            (visible.min.to_vec2() * ppp).to_pos2(),
            (visible.max.to_vec2() * ppp).to_pos2(),
        ),
        parent_x11: in_main
            .then(|| egui_pop_api::frame::x11_window(&ctx))
            .flatten(),
    };
    let native = ctx.native_pixels_per_point().unwrap_or(1.0);
    if !v.embed_in_view(app, place, native) {
        return too_small(ui, &place, native);
    }
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    egui_pop_core::a11y::name(
        &response,
        egui::Role::Image,
        &tr_args(&ctx, &txt::VIEWER_VIDEO_PICTURE, &[("name", &id)]),
    );
    // Under the web view, which covers it: nothing is drawn over the player.
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
    false
}

/// The card in place of a player the panel is too small for: why, and Open
/// outside. True: Open outside was clicked.
fn too_small(ui: &mut Ui, place: &Placement, native: f32) -> bool {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let size = place.rect.size() / native.max(1.0);
    let min = format!("{MIN_PLAYER:.0}");
    let (w, h) = (
        format!("{:.0}", size.x.max(0.0)),
        format!("{:.0}", size.y.max(0.0)),
    );
    ui.add_space(t.space.m);
    ui.horizontal(|ui| {
        ui.add(Icon::Info.size(Size::Inline).tone(Tone::Info));
        ui.strong(tr(&ctx, &txt::VIEWER_EMBED_TOO_SMALL));
    });
    ui.weak(tr_args(
        &ctx,
        &txt::VIEWER_EMBED_TOO_SMALL_DETAIL,
        &[("min", &min), ("width", &w), ("height", &h)],
    ));
    ui.add_space(t.space.s);
    ui.button(tr(&ctx, &api_txt::LINKS_OPEN_OUTSIDE)).clicked()
}
