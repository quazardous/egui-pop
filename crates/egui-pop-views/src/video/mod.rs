//! Video: the [`Video`] widget, a local video file played in place, and the
//! [`VideoBackend`] it plays through.
//!
//! ```no_run
//! use egui_pop_views::video::{Controls, Fit, Video};
//!
//! fn ui(ui: &mut egui::Ui) {
//!     // A tutorial, with every control.
//!     Video::new("/usr/share/app/tour.webm").id("tour").show(ui);
//!     // A preview: no controls, playing at once, again and again, silent.
//!     Video::new("/usr/share/app/gesture.webm")
//!         .controls(Controls::None)
//!         .autoplay(true)
//!         .looping(true)
//!         .muted(true)
//!         .fit(Fit::Cover)
//!         .size(egui::vec2(240.0, 135.0))
//!         .show(ui);
//! }
//! ```
//!
//! - **Players**: each widget has its own (one pipeline per video), opened
//!   when it is first drawn and paused on its first picture, unless it plays
//!   at once ([`Video::autoplay`]). The backend is GStreamer with the `video`
//!   feature, none without it or on the web: the widget then draws a
//!   placeholder saying so, with **Open outside**. [`Video::backend`] gives
//!   another (a host's own player, a test's pretend one).
//! - **Off the interface's thread**: the player decodes on its own threads and
//!   keeps the last frame; each pass takes that one and uploads it, whatever
//!   the player decoded in between.
//! - **Repaints**: the player asks for one when a frame or an event arrives,
//!   the widget for one a few times a second while playing, for the time.
//!   Paused, ended or loading, nothing keeps the application awake.
//! - **Out of view**: a widget no longer drawn (a page left, a panel closed, a
//!   tab behind another) has its player stopped after a few passes; drawn
//!   again, it opens the file at the same place, paused (playing, with
//!   [`Video::autoplay`]).
//! - **Controls** ([`Controls`]): `Full` (the position on a row of its own;
//!   Play or Pause, the time and the volume under it), `Compact` (Play or
//!   Pause, the position and the time on one row), `None` (the picture only).
//!   A click on the picture plays or pauses, except without controls.
//! - **Keys**, while the picture has the focus (it is a Tab stop with
//!   controls): Space plays or pauses, Left and Right move by [`SEEK_STEP`],
//!   Up and Down change the volume by [`VOLUME_STEP`]. A key an application
//!   binds on the picture's surface ([`Video::surface`]) runs its command
//!   instead, as the viewer's do.
//! - **Loops** ([`Video::looping`]): the player loops by itself when it can
//!   ([`VideoPlayer::set_looping`]; GStreamer's does, without a gap), else the
//!   widget seeks back to the start at the end.
//! - **Agents**: a widget given an [`id`](Video::id) is listed by the `videos`
//!   collector and driven by the `video.*` commands ([`VideoPlugin`]); its
//!   accessible node is a video named after the file (or [`Video::label`]).

mod plugin;
mod session;

#[cfg(all(feature = "video", not(target_arch = "wasm32")))]
mod gst;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use egui::accesskit::Role;
use egui::load::SizedTexture;
use egui::{
    Align2, Color32, ColorImage, Key, Modifiers, Rect, Response, Sense, Ui, Vec2, pos2, vec2,
};
use egui_pop_api::AppHandle;
use egui_pop_api::links::{self, How, Request, Source};
use egui_pop_api::strings as api_txt;
use egui_pop_core::kit::{Kit, KitCx, Part, State};
use egui_pop_core::lock;
use egui_pop_core::slider::Slider;
use egui_pop_core::strings::{tr, tr_args};

pub use plugin::{
    CMD_MUTE, CMD_PAUSE, CMD_PLAY, CMD_SEEK, CMD_TOGGLE, CMD_VOLUME, PLUGIN, VideoPlugin,
};
pub use session::Videos;

#[cfg(all(feature = "video", not(target_arch = "wasm32")))]
pub use gst::{GStreamer, write_test_clip};

use crate::strings as txt;
use session::{Act, Session};

/// The surface a video's picture opens by default, for key bindings
/// (`videoFocus`).
pub const SURFACE: &str = "video";
/// What Left and Right move the position by.
pub const SEEK_STEP: Duration = Duration::from_secs(5);
/// What Up and Down change the volume by.
pub const VOLUME_STEP: f32 = 0.05;
/// How often the time is drawn again while playing.
const TIME_REPAINT: Duration = Duration::from_millis(250);
/// The shape of a video before its first picture.
const DEFAULT_ASPECT: f32 = 16.0 / 9.0;

/// What a backend is asked to open.
#[derive(Clone, Debug)]
pub struct VideoRequest {
    /// The file, which exists and could be opened a moment ago.
    pub path: PathBuf,
    /// For the player's threads to ask for a repaint.
    pub egui: egui::Context,
    /// From 0 to 1.
    pub volume: f32,
}

/// Where a player is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoState {
    /// Not ready to show anything yet.
    #[default]
    Loading,
    Paused,
    Playing,
    /// At the end, not moving; Play starts again.
    Ended,
}

/// What a player says of itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VideoStatus {
    pub state: VideoState,
    /// The position, when it can say.
    pub position: Option<Duration>,
    /// The length, when it can say.
    pub duration: Option<Duration>,
    /// Something the user should know while it plays on (no sound device, an
    /// audio stream it cannot decode).
    pub note: Option<String>,
}

/// A picture to show.
#[derive(Clone, Debug)]
pub struct VideoFrame {
    pub image: Arc<ColorImage>,
    /// The width over the height it is shown at, the file's pixel shape
    /// included.
    pub aspect: f32,
}

/// A video being played. A failure's text is shown in place of the picture
/// (the viewer's error page, for the viewer's video).
pub trait VideoPlayer: Send {
    /// Takes what happened since the last call; an error ends the player.
    fn poll(&mut self) -> Result<VideoStatus, String>;
    /// The last frame not taken yet, the older ones forgotten.
    fn frame(&mut self) -> Option<VideoFrame>;
    fn play(&mut self) -> Result<(), String>;
    fn pause(&mut self) -> Result<(), String>;
    fn seek(&mut self, to: Duration) -> Result<(), String>;
    /// From 0 to 1.
    fn set_volume(&mut self, volume: f32);
    /// Loops by itself from now on (or no longer), when it can: whether it
    /// does. By default it cannot, and the widget seeks back to the start
    /// when it ends.
    fn set_looping(&mut self, on: bool) -> bool {
        let _ = on;
        false
    }
    /// How many times it went back to the start by itself.
    fn loops(&self) -> u64 {
        0
    }
}

/// What opens videos ([`Video::backend`], and the viewer's).
pub trait VideoBackend: Send + Sync {
    /// Its name, in the widget's state.
    fn name(&self) -> &str;
    /// A player, paused at the start of the file, or why not. It stops when
    /// dropped, without making the interface's thread wait.
    fn open(&self, request: &VideoRequest) -> Result<Box<dyn VideoPlayer>, String>;
}

/// An RGBA picture from rows `stride` bytes apart, for a backend: `None` when
/// the bytes do not hold it.
pub fn rgba_image(data: &[u8], width: usize, height: usize, stride: usize) -> Option<ColorImage> {
    let row = width.checked_mul(4)?;
    if width == 0 || height == 0 || stride < row {
        return None;
    }
    let mut rgba = Vec::with_capacity(row.checked_mul(height)?);
    for y in 0..height {
        let start = y.checked_mul(stride)?;
        rgba.extend_from_slice(data.get(start..start.checked_add(row)?)?);
    }
    Some(ColorImage::from_rgba_unmultiplied([width, height], &rgba))
}

/// A time as a clock: `0:05`, `12:30`, `1:02:03`.
pub fn clock(time: Duration) -> String {
    let s = time.as_secs();
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// The backend of this build: GStreamer with the `video` feature (not on the
/// web), none otherwise.
pub fn default_backend() -> Option<Arc<dyn VideoBackend>> {
    #[cfg(all(feature = "video", not(target_arch = "wasm32")))]
    {
        Some(Arc::new(GStreamer::default()))
    }
    #[cfg(not(all(feature = "video", not(target_arch = "wasm32"))))]
    {
        None
    }
}

/// Which controls a video draws under its picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Controls {
    /// The position on a row of its own; Play or Pause, the time and the
    /// volume under it.
    #[default]
    Full,
    /// Play or Pause, the position and the time on one row.
    Compact,
    /// The picture only: not a Tab stop, a click does nothing; commands and
    /// [`Video::autoplay`] still play it.
    None,
}

/// How the picture fills the widget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// All of it, with bars on the sides or above and below.
    #[default]
    Contain,
    /// All the room, cropping what goes past it.
    Cover,
}

/// Where the backend comes from.
#[derive(Clone)]
enum Backend {
    Default,
    Given(Option<Arc<dyn VideoBackend>>),
}

/// A local video file played in place. Its state (the player, the position,
/// the volume) lives in the context, by [`id`](Self::id), from one pass to
/// the next.
#[must_use = "call `show` to draw it"]
pub struct Video<'a> {
    source: PathBuf,
    id: Option<String>,
    label: Option<String>,
    controls: Controls,
    autoplay: bool,
    looping: bool,
    muted: bool,
    volume: f32,
    fit: Fit,
    poster: Option<SizedTexture>,
    size: Option<Vec2>,
    backend: Backend,
    app: Option<&'a AppHandle>,
    surface: &'a str,
    take_focus: bool,
}

impl std::fmt::Debug for Video<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Video")
            .field("source", &self.source)
            .field("id", &self.id)
            .field("controls", &self.controls)
            .finish_non_exhaustive()
    }
}

/// What a video did this pass.
#[derive(Debug)]
pub struct VideoOutput {
    /// The picture (the placeholder without a player).
    pub response: Response,
    pub state: VideoState,
    pub position: Duration,
    pub duration: Option<Duration>,
    /// From 0 to 1, whether muted or not.
    pub volume: f32,
    pub muted: bool,
    /// Why it cannot play: the player failed to open or while it played.
    pub error: Option<String>,
    /// There is no player: the placeholder is drawn.
    pub unavailable: bool,
    /// **Open outside** was clicked on the placeholder (with an
    /// [`app`](Video::app), the links service has been asked already).
    pub open_outside: bool,
}

impl<'a> Video<'a> {
    /// The file at `source`, with full controls, paused on its first picture.
    pub fn new(source: impl AsRef<Path>) -> Self {
        Self {
            source: source.as_ref().to_path_buf(),
            id: None,
            label: None,
            controls: Controls::Full,
            autoplay: false,
            looping: false,
            muted: false,
            volume: 1.0,
            fit: Fit::Contain,
            poster: None,
            size: None,
            backend: Backend::Default,
            app: None,
            surface: SURFACE,
            take_focus: false,
        }
    }

    /// Its name for agents (the `videos` collector, the `video.*` commands'
    /// `video`) and its key in the context. Without one, the video is keyed by
    /// its file and the `Ui` it is drawn in, and agents do not see it.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Its accessible name, instead of "Video: " and the file's name.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn controls(mut self, controls: Controls) -> Self {
        self.controls = controls;
        self
    }

    /// Plays as soon as it opens, and again when drawn after being out of
    /// view.
    pub fn autoplay(mut self, on: bool) -> Self {
        self.autoplay = on;
        self
    }

    /// Starts again at the end; read every pass.
    pub fn looping(mut self, on: bool) -> Self {
        self.looping = on;
        self
    }

    /// Starts without sound (read when it opens; the volume slider, or
    /// `video.mute`, gives it back).
    pub fn muted(mut self, on: bool) -> Self {
        self.muted = on;
        self
    }

    /// Its volume when it opens, from 0 to 1 (1 by default).
    pub fn volume(mut self, volume: f32) -> Self {
        self.volume = volume.clamp(0.0, 1.0);
        self
    }

    pub fn fit(mut self, fit: Fit) -> Self {
        self.fit = fit;
        self
    }

    /// A picture shown until it first plays or moves, instead of its first
    /// frame (a texture the application loaded: `&TextureHandle` converts).
    pub fn poster(mut self, poster: impl Into<SizedTexture>) -> Self {
        self.poster = Some(poster.into());
        self
    }

    /// Its whole size, controls included. By default, the available width,
    /// and the picture as high as its shape asks.
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// What plays it (read when it opens), instead of this build's
    /// ([`default_backend`]); `None` draws the placeholder.
    pub fn backend(mut self, backend: Option<Arc<dyn VideoBackend>>) -> Self {
        self.backend = Backend::Given(backend);
        self
    }

    /// The application: **Open outside** goes through its links service.
    pub fn app(mut self, app: &'a AppHandle) -> Self {
        self.app = Some(app);
        self
    }

    /// The focus surface around the picture, for key bindings ([`SURFACE`]
    /// by default).
    pub fn surface(mut self, surface: &'a str) -> Self {
        self.surface = surface;
        self
    }

    /// The picture takes the keyboard focus this pass.
    pub fn take_focus(mut self, on: bool) -> Self {
        self.take_focus = on;
        self
    }

    /// Draws it.
    pub fn show(self, ui: &mut Ui) -> VideoOutput {
        let ctx = ui.ctx().clone();
        let videos = Videos::of(&ctx);
        videos.install_sweep();
        let key = match &self.id {
            Some(id) => egui::Id::unique(("egui_pop.video", id)),
            None => ui.scope_id().with(("egui_pop.video", &self.source)),
        };
        let backend = match &self.backend {
            Backend::Default => default_backend(),
            Backend::Given(b) => b.clone(),
        };
        let fresh = || {
            Session::new(
                self.source.clone(),
                backend.clone(),
                self.volume,
                self.muted,
            )
        };
        let name = self.label.clone().unwrap_or_else(|| {
            let file = self.source.file_name().map_or_else(
                || self.source.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            tr_args(&ctx, &txt::VIEWER_VIDEO_PICTURE, &[("name", &file)])
        });
        let mut acts = Vec::new();
        let registry = videos.registry();
        let mut reg = lock(&registry);
        let session = reg.session(key, fresh);
        if session.source != self.source {
            *session = fresh();
        }
        session.name.clone_from(&self.id);
        session.controls = self.controls;
        session.autoplay = self.autoplay;
        session.seen(ctx.cumulative_pass_nr(), ctx.viewport_id());
        session.update(&ctx, self.looping);
        let (response, open_outside) = self.draw(ui, session, &name, &mut acts);
        for act in acts {
            session.act(&ctx, act);
        }
        if session.status.state == VideoState::Playing {
            ctx.request_repaint_after(TIME_REPAINT);
        }
        let out = VideoOutput {
            response,
            state: session.status.state,
            position: session.position,
            duration: session.status.duration,
            volume: session.volume,
            muted: session.muted,
            error: session.error.clone(),
            unavailable: session.unavailable(),
            open_outside,
        };
        if out.response.has_focus()
            && let Some(name) = &self.id
        {
            reg.focused = Some(name.clone());
        }
        drop(reg);
        if open_outside && let Some(app) = self.app {
            let url = file_url(&self.source);
            let _ = links::open(
                app,
                Request::new(&url)
                    .how(How::Outside)
                    .source(Source::Declared),
            );
        }
        out
    }

    /// The picture and the controls; the user's asks into `acts`. Returns the
    /// picture's response and whether Open outside was clicked.
    fn draw(
        &self,
        ui: &mut Ui,
        s: &mut Session,
        name: &str,
        acts: &mut Vec<Act>,
    ) -> (Response, bool) {
        let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
        let playable = !s.unavailable() && s.error.is_none();
        let controls = if playable {
            match self.controls {
                Controls::Full => 2.0 * row + if s.status.note.is_some() { row } else { 0.0 },
                Controls::Compact => row,
                Controls::None => 0.0,
            }
        } else {
            0.0
        };
        let aspect = s.aspect_or(self.poster.map(|p| p.size.x / p.size.y.max(1.0)));
        let width = self.size.map_or(ui.available_width(), |v| v.x).max(1.0);
        let height = match self.size {
            Some(v) => (v.y - controls).max(row),
            None => width / aspect,
        };
        let size = vec2(width, height);
        ui.vertical(|ui| {
            ui.set_width(width);
            if s.unavailable() {
                return placeholder(ui, size, name);
            }
            let mut outside = false;
            let picture = self.picture(ui, s, size, name, acts, &mut outside);
            if !playable {
                return (picture, outside);
            }
            if let Some(note) = &s.status.note
                && self.controls == Controls::Full
            {
                ui.weak(note);
            }
            match self.controls {
                Controls::Full => full_controls(ui, s, &picture, acts),
                Controls::Compact => compact_controls(ui, s, &picture, acts),
                Controls::None => {}
            }
            (picture, false)
        })
        .inner
    }

    /// The picture, its surface around it: its keys hold while it has the
    /// focus.
    fn picture(
        &self,
        ui: &mut Ui,
        s: &mut Session,
        size: Vec2,
        name: &str,
        acts: &mut Vec<Act>,
        outside: &mut bool,
    ) -> Response {
        let ctx = ui.ctx().clone();
        let interactive = self.controls != Controls::None && s.error.is_none();
        egui_pop_core::focus::scope(ui, self.surface, &[], |ui| {
            let sense = if interactive {
                Sense::click()
            } else {
                Sense::hover()
            };
            let (rect, response) = ui.allocate_exact_size(size, sense);
            egui_pop_core::a11y::name(&response, Role::Video, name);
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            if let Some(error) = s.error.clone() {
                let title = tr(&ctx, &txt::VIEWER_VIDEO_FAILED);
                *outside = notice(ui, rect, &title, Some(&error));
                return response;
            }
            let poster = self.poster.filter(|_| !s.started);
            let shown = match poster {
                Some(p) => Some((p.id, p.size.x / p.size.y.max(1.0))),
                None => s.texture.as_ref().map(|t| (t.id(), s.aspect)),
            };
            match shown {
                Some((texture, aspect)) => {
                    let (at, uv) = placed(rect, aspect, self.fit);
                    painter.image(texture, at, uv, Color32::WHITE);
                }
                None if s.status.state == VideoState::Loading => {
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        tr(&ctx, &txt::VIEWER_VIDEO_LOADING),
                        egui::TextStyle::Body.resolve(ui.style()),
                        ui.visuals().weak_text_color(),
                    );
                }
                None => {}
            }
            if !interactive {
                return response;
            }
            if self.take_focus {
                response.request_focus();
            }
            // egui clicks a focused widget on Space too: one toggle for it.
            let clicked = response.clicked();
            if clicked {
                response.request_focus();
                acts.push(Act::Toggle);
            }
            if response.has_focus() {
                // The arrows are the picture's keys, not egui's way to move
                // the focus to a neighbour.
                ui.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        response.id,
                        egui::EventFilter {
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            ..Default::default()
                        },
                    );
                });
                egui_pop_core::focus::takes_keys(&ctx, response.id);
                egui_pop_core::focus::own_ring(&ctx, response.id);
                keys(ui, acts, clicked);
                painter.rect_stroke(
                    rect,
                    0.0,
                    ui.visuals().selection.stroke,
                    egui::StrokeKind::Inside,
                );
            }
            response
        })
    }
}

/// The picture's keys, taken from the input (a binding of the application's
/// on them has run and taken them first). Space is taken without a second
/// toggle when egui made a click of it.
fn keys(ui: &mut Ui, acts: &mut Vec<Act>, clicked: bool) {
    ui.input_mut(|i| {
        let none = Modifiers::NONE;
        let mut toggled = clicked;
        while i.consume_key(none, Key::Space) {
            if !std::mem::replace(&mut toggled, false) {
                acts.push(Act::Toggle);
            }
        }
        while i.consume_key(none, Key::ArrowLeft) {
            acts.push(Act::SeekBy(-SEEK_STEP.as_secs_f64()));
        }
        while i.consume_key(none, Key::ArrowRight) {
            acts.push(Act::SeekBy(SEEK_STEP.as_secs_f64()));
        }
        while i.consume_key(none, Key::ArrowUp) {
            acts.push(Act::VolumeBy(VOLUME_STEP));
        }
        while i.consume_key(none, Key::ArrowDown) {
            acts.push(Act::VolumeBy(-VOLUME_STEP));
        }
    });
}

/// The rectangle a picture of that shape is painted in, and the part of it
/// shown.
fn placed(rect: Rect, aspect: f32, fit: Fit) -> (Rect, Rect) {
    let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let aspect = if aspect > 0.0 { aspect } else { DEFAULT_ASPECT };
    match fit {
        Fit::Contain => (
            Rect::from_center_size(rect.center(), fitted(rect.size(), aspect)),
            whole,
        ),
        Fit::Cover => {
            let room = rect.size();
            if room.y <= 0.0 {
                return (rect, whole);
            }
            let uv = if room.x / room.y > aspect {
                // Wider than the picture: its top and bottom are cut.
                let shown = (room.y / (room.x / aspect)).min(1.0);
                Rect::from_min_max(pos2(0.0, 0.5 - shown / 2.0), pos2(1.0, 0.5 + shown / 2.0))
            } else {
                let shown = (room.x / (room.y * aspect)).min(1.0);
                Rect::from_min_max(pos2(0.5 - shown / 2.0, 0.0), pos2(0.5 + shown / 2.0, 1.0))
            };
            (rect, uv)
        }
    }
}

/// The largest size of that shape within `room`.
fn fitted(room: Vec2, aspect: f32) -> Vec2 {
    let aspect = if aspect > 0.0 { aspect } else { DEFAULT_ASPECT };
    if room.x / aspect <= room.y {
        vec2(room.x, room.x / aspect)
    } else {
        vec2(room.y * aspect, room.y)
    }
}

/// The picture's place without a player: why, and Open outside (whether it
/// was clicked).
fn placeholder(ui: &mut Ui, size: Vec2, name: &str) -> (Response, bool) {
    let ctx = ui.ctx().clone();
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    egui_pop_core::a11y::name(&response, Role::Video, name);
    ui.painter()
        .rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
    let clicked = notice(ui, rect, &tr(&ctx, &txt::VIDEO_NO_PLAYER), None);
    (response, clicked)
}

/// A message centred in `rect`, already allocated, with Open outside: whether
/// it was clicked.
fn notice(ui: &mut Ui, rect: Rect, title: &str, detail: Option<&str>) -> bool {
    let ctx = ui.ctx().clone();
    let inner = rect.shrink(ui.spacing().item_spacing.x * 2.0);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    // Centred as a block of two or three rows.
    let rows = if detail.is_some() { 3.0 } else { 2.0 };
    let block = rows * (child.spacing().interact_size.y + child.spacing().item_spacing.y);
    child.add_space(((inner.height() - block) / 2.0).max(0.0));
    child.label(title);
    if let Some(detail) = detail {
        child.weak(detail);
    }
    child
        .button(tr(&ctx, &api_txt::LINKS_OPEN_OUTSIDE))
        .clicked()
}

/// Play or Pause, a toolbar button of the kit with its mark painted.
fn play_button(ui: &mut Ui, playing: bool) -> Response {
    let ctx = ui.ctx().clone();
    let name = tr(
        &ctx,
        if playing {
            &txt::VIEWER_VIDEO_PAUSE
        } else {
            &txt::VIEWER_VIDEO_PLAY
        },
    );
    let side = ui.spacing().interact_size.y;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
    if ui.is_rect_visible(rect) {
        let kit = Kit::of(&ctx);
        let look = &kit.toolbar_button;
        let state = State::from_response(&response);
        look.paint(
            &KitCx {
                painter: ui.painter(),
                ppp: ctx.pixels_per_point(),
            },
            &Part {
                rect,
                room: rect,
                state,
                label: None,
            },
        );
        let color = look.style(state).text;
        let mark = Rect::from_center_size(rect.center(), Vec2::splat((side * 0.4).round()));
        if playing {
            let bar = mark.width() / 3.0;
            for x in [mark.left(), mark.right() - bar] {
                ui.painter().rect_filled(
                    Rect::from_min_size(pos2(x, mark.top()), vec2(bar, mark.height())),
                    0.0,
                    color,
                );
            }
        } else {
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    mark.left_top(),
                    pos2(mark.right(), mark.center().y),
                    mark.left_bottom(),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
    }
    egui_pop_core::a11y::name(&response, Role::Button, &name);
    response.on_hover_text(name)
}

/// The time, `0:12 / 1:00`.
fn time(ui: &mut Ui, s: &Session) {
    let ctx = ui.ctx().clone();
    let total = s.status.duration.map_or_else(|| "-:--".to_string(), clock);
    ui.monospace(tr_args(
        &ctx,
        &txt::VIEWER_VIDEO_TIME,
        &[("elapsed", &clock(s.position)), ("total", &total)],
    ));
}

/// The position's slider: it follows the video, except during a drag, whose
/// end seeks. Without a length, there is nowhere to go yet.
fn position(ui: &mut Ui, s: &mut Session, acts: &mut Vec<Act>) {
    let ctx = ui.ctx().clone();
    let name = tr(&ctx, &txt::VIEWER_VIDEO_POSITION);
    let total = s.status.duration.map_or(0.0, |d| d.as_secs_f64());
    let mut at = s.scrub.unwrap_or(s.position.as_secs_f64());
    let out = ui
        .add_enabled_ui(total > 0.0, |ui| {
            Slider::new(&mut at, &name, 0.0..=total.max(1.0))
                .hard(0.0..=total.max(1.0))
                .step(1.0)
                .coarse(10.0)
                .precision(0)
                .unit("s")
                .show(ui)
        })
        .inner;
    let mut scrub = None;
    for change in &out.changes {
        if change.done {
            acts.push(Act::SeekTo(change.to));
            scrub = None;
        } else {
            scrub = Some(change.to);
        }
    }
    // Escape during a drag puts it back without a change.
    s.scrub = scrub.or(if out.response.dragged() {
        s.scrub
    } else {
        None
    });
}

/// A click on Play or Pause: by the pointer the keys stay with the picture, by
/// the keyboard with the button.
fn toggled(button: &Response, picture: &Response, acts: &mut Vec<Act>) {
    if button.clicked() {
        acts.push(Act::Toggle);
        if button.clicked_by(egui::PointerButton::Primary) && !button.has_focus() {
            picture.request_focus();
        }
    }
}

fn full_controls(ui: &mut Ui, s: &mut Session, picture: &Response, acts: &mut Vec<Act>) {
    let ctx = ui.ctx().clone();
    position(ui, s, acts);
    ui.horizontal(|ui| {
        let button = play_button(ui, s.status.state == VideoState::Playing);
        toggled(&button, picture, acts);
        time(ui, s);
        let mut percent = f64::from(s.audible_volume()) * 100.0;
        let out = Slider::new(
            &mut percent,
            &tr(&ctx, &txt::VIEWER_VIDEO_VOLUME),
            0.0..=100.0,
        )
        .hard(0.0..=100.0)
        .step(5.0)
        .precision(0)
        .unit("%")
        .show(ui);
        if !out.changes.is_empty() {
            acts.push(Act::Volume((percent / 100.0) as f32));
        }
    });
}

fn compact_controls(ui: &mut Ui, s: &mut Session, picture: &Response, acts: &mut Vec<Act>) {
    ui.horizontal(|ui| {
        let button = play_button(ui, s.status.state == VideoState::Playing);
        toggled(&button, picture, acts);
        // The time at the end, the position in what is left.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            time(ui, s);
            position(ui, s, acts);
        });
    });
}

/// A `file:` URL for a path.
fn file_url(path: &Path) -> String {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clock_grows_its_hours_only_when_it_has_them() {
        assert_eq!(clock(Duration::ZERO), "0:00");
        assert_eq!(clock(Duration::from_millis(5900)), "0:05");
        assert_eq!(clock(Duration::from_secs(750)), "12:30");
        assert_eq!(clock(Duration::from_secs(3723)), "1:02:03");
    }

    #[test]
    fn a_picture_drops_the_padding_of_its_rows() {
        let red = [255, 0, 0, 255];
        let mut data = Vec::new();
        for _ in 0..2 {
            data.extend_from_slice(&red);
            data.extend_from_slice(&red);
            data.extend_from_slice(&[9, 9, 9, 9]);
        }
        let image = rgba_image(&data, 2, 2, 12).unwrap();
        assert_eq!(image.size, [2, 2]);
        assert!(image.pixels.iter().all(|p| *p == Color32::RED));
    }

    #[test]
    fn bytes_that_do_not_hold_a_picture_give_none() {
        assert!(rgba_image(&[0; 4], 2, 2, 8).is_none(), "too short");
        assert!(rgba_image(&[0; 64], 2, 2, 4).is_none(), "rows overlap");
        assert!(rgba_image(&[0; 64], 0, 2, 8).is_none(), "no width");
        assert!(rgba_image(&[0; 64], usize::MAX, 2, 8).is_none(), "overflow");
    }

    #[test]
    fn a_picture_fits_its_room_keeping_its_shape() {
        assert_eq!(fitted(vec2(200.0, 200.0), 2.0), vec2(200.0, 100.0));
        assert_eq!(fitted(vec2(400.0, 100.0), 2.0), vec2(200.0, 100.0));
        assert_eq!(fitted(vec2(160.0, 900.0), 0.0), vec2(160.0, 90.0));
    }

    #[test]
    fn a_covering_picture_fills_its_room_and_crops_the_rest() {
        let room = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 100.0));
        // A square picture in a wide room: its middle half, top to bottom.
        let (at, uv) = placed(room, 1.0, Fit::Cover);
        assert_eq!(at, room);
        assert_eq!((uv.min.y, uv.max.y), (0.25, 0.75));
        assert_eq!((uv.min.x, uv.max.x), (0.0, 1.0));
        // A wide picture in a square room: its middle half, side to side.
        let square = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
        let (_, uv) = placed(square, 2.0, Fit::Cover);
        assert_eq!((uv.min.x, uv.max.x), (0.25, 0.75));
        // Contained, all of it, in the middle.
        let (at, uv) = placed(room, 1.0, Fit::Contain);
        assert_eq!(
            at,
            Rect::from_center_size(room.center(), vec2(100.0, 100.0))
        );
        assert_eq!(uv, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
    }
}
