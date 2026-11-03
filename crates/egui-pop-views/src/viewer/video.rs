//! The viewer's video page (ADR 0017): a local video file played in the panel,
//! with Play and Pause, the position, the volume and the time, through a
//! [`VideoBackend`].
//!
//! The backend is GStreamer with the `video` feature, none without it (a local
//! video then opens outside, as any local file; the policy asks
//! [`Viewer::plays_video`]). What is here knows nothing of GStreamer, so a host
//! can give its own player and a test a pretend one.
//!
//! - **Off the interface's thread**: the player decodes on its own threads and
//!   keeps the last frame; each pass takes that one and uploads it, whatever
//!   the player decoded in between.
//! - **Repaints**: the player asks for one when a frame or an event arrives, the
//!   page for one a few times a second while playing, for the time. Paused,
//!   ended or loading, nothing keeps the application awake.
//! - **Stopping**: leaving the page drops the player. So does a panel no longer
//!   drawn (closed, or a tab behind another), which keeps the position: showing
//!   it again opens the file there, paused.
//! - **Keys**: the picture is a Tab stop, its keys are bound on its own surface
//!   ([`VIDEO_SURFACE`]): Space plays or pauses, Left and Right move by
//!   [`SEEK_STEP`], Up and Down change the volume by [`VOLUME_STEP`]. A button
//!   or a slider with the focus keeps its own keys.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use egui::{Align2, Color32, ColorImage, Rect, Sense, TextureHandle, TextureOptions, Ui, vec2};
use egui_pop_api::AppHandle;
use egui_pop_core::lock;
use egui_pop_core::slider::Slider;
use egui_pop_core::strings::{tr, tr_args};
use serde_json::{Value as Json, json};

use super::{Failure, PLUGIN, Status, Viewer};
use crate::strings as txt;

/// The surface the picture's keys are bound on (`viewerVideoFocus`).
pub const VIDEO_SURFACE: &str = "viewerVideo";
/// The context key saying a video page is shown.
pub const VIDEO_CONTEXT: &str = "viewerVideo";
/// What Left and Right move the position by.
pub const SEEK_STEP: Duration = Duration::from_secs(5);
/// What Up and Down change the volume by.
pub const VOLUME_STEP: f32 = 0.05;
/// How often the time is drawn again while playing.
const TIME_REPAINT: Duration = Duration::from_millis(250);
/// The application's passes without the page before its panel counts as
/// hidden: a pass may run before the panel is drawn in it.
const HIDDEN_PASSES: u32 = 3;
/// The texture's name, for egui's debugging.
const TEXTURE: &str = "egui_pop.viewer.video";

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

/// A video being played. A failure's text is shown on the error page.
pub trait VideoPlayer: Send {
    /// Takes what happened since the last call; an error ends the page.
    fn poll(&mut self) -> Result<VideoStatus, String>;
    /// The last frame not taken yet, the older ones forgotten.
    fn frame(&mut self) -> Option<VideoFrame>;
    fn play(&mut self) -> Result<(), String>;
    fn pause(&mut self) -> Result<(), String>;
    fn seek(&mut self, to: Duration) -> Result<(), String>;
    /// From 0 to 1.
    fn set_volume(&mut self, volume: f32);
}

/// What opens videos for the viewer ([`ViewerPlugin::video`](super::ViewerPlugin::video)).
pub trait VideoBackend: Send + Sync {
    /// Its name, in `viewer.state`.
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

/// The backend of this build.
pub(super) fn default_backend() -> Option<Arc<dyn VideoBackend>> {
    #[cfg(all(feature = "video", not(target_arch = "wasm32")))]
    {
        Some(Arc::new(super::gst_player::GStreamer::default()))
    }
    #[cfg(not(all(feature = "video", not(target_arch = "wasm32"))))]
    {
        None
    }
}

/// The video page's state.
struct Session {
    /// The page's address.
    target: String,
    path: PathBuf,
    /// None while its panel is hidden.
    player: Option<Box<dyn VideoPlayer>>,
    status: VideoStatus,
    /// The last position known, kept when the player stops.
    position: Duration,
    texture: Option<TextureHandle>,
    aspect: f32,
    /// The position while the slider is dragged.
    scrub: Option<f64>,
    /// Passes since the page was last drawn.
    unseen: u32,
    /// The picture takes the focus when next drawn.
    focus: bool,
}

/// The viewer's video state: the backend, the volume for the session, the page.
pub(super) struct Video {
    backend: Option<Arc<dyn VideoBackend>>,
    volume: f32,
    session: Option<Session>,
}

impl Video {
    pub(super) fn new(backend: Option<Arc<dyn VideoBackend>>) -> Self {
        Self {
            backend,
            volume: 1.0,
            session: None,
        }
    }

    pub(super) fn set_backend(&mut self, backend: Option<Arc<dyn VideoBackend>>) {
        self.backend = backend;
    }
}

/// What the user asked of the page while it was drawn.
#[derive(Clone, Copy, Debug)]
pub(super) enum Act {
    Toggle,
    SeekTo(f64),
    Volume(f32),
}

impl Viewer {
    /// Whether local video files open here: a backend is there.
    pub fn plays_video(&self) -> bool {
        lock(&self.video).backend.is_some()
    }

    /// Opens the video page, or the error page saying why not.
    pub(super) fn open_video(&self, app: &AppHandle, target: &str, path: &str) {
        let path = PathBuf::from(path);
        if let Err(e) = std::fs::File::open(&path) {
            let failure = if e.kind() == std::io::ErrorKind::NotFound {
                Failure::NotFound
            } else {
                Failure::Playback
            };
            return self.fail(app, target, failure, &e.to_string());
        }
        let opened = self.open_player(app, &path);
        match opened {
            Ok(player) => {
                lock(&self.video).session = Some(Session {
                    target: target.to_string(),
                    path: path.clone(),
                    player: Some(player),
                    status: VideoStatus::default(),
                    position: Duration::ZERO,
                    texture: None,
                    aspect: 16.0 / 9.0,
                    scrub: None,
                    unseen: 0,
                    focus: true,
                });
                app.set_context(VIDEO_CONTEXT, "true");
                self.show_video_page(app, target, path);
            }
            Err(e) => self.fail(app, target, Failure::Playback, &e),
        }
    }

    fn open_player(&self, app: &AppHandle, path: &Path) -> Result<Box<dyn VideoPlayer>, String> {
        let (backend, volume) = {
            let v = lock(&self.video);
            (v.backend.clone(), v.volume)
        };
        let backend = backend.ok_or("this application has no video player")?;
        backend.open(&VideoRequest {
            path: path.to_path_buf(),
            egui: app.egui(),
            volume,
        })
    }

    /// Leaves the video page: the player stops, what it showed goes.
    pub(super) fn stop_video(&self, app: &AppHandle) {
        let session = lock(&self.video).session.take();
        if session.is_some() {
            app.set_context(VIDEO_CONTEXT, "false");
        }
    }

    fn video_failed(&self, app: &AppHandle, target: &str, detail: &str) {
        self.stop_video(app);
        self.fail(app, target, Failure::Playback, detail);
    }

    /// Takes what the player has to say (each pass).
    pub(super) fn poll_video(&self, app: &AppHandle) {
        let failed = {
            let mut v = lock(&self.video);
            let Some(s) = v.session.as_mut() else {
                return;
            };
            let Some(player) = s.player.as_mut() else {
                return;
            };
            match player.poll() {
                Ok(status) => {
                    if let Some(p) = status.position {
                        s.position = p;
                    }
                    s.status = status;
                    None
                }
                Err(e) => Some((s.target.clone(), e)),
            }
        };
        if let Some((target, e)) = failed {
            self.video_failed(app, &target, &e);
        }
    }

    /// Once per pass of the application: stops the player of a page that is
    /// no longer drawn.
    pub(super) fn video_pass(&self, app: &AppHandle) {
        let stopped = {
            let mut v = lock(&self.video);
            let Some(s) = v.session.as_mut() else {
                return;
            };
            if s.player.is_none() {
                return;
            }
            s.unseen += 1;
            if s.unseen < HIDDEN_PASSES {
                // Passes go on until the player is stopped or drawn again,
                // even with nothing else asking for one.
                if s.unseen > 1 {
                    app.egui().request_repaint();
                }
                return;
            }
            s.status.state = VideoState::Paused;
            s.player.take()
        };
        drop(stopped);
        app.publish(PLUGIN.as_str(), None, "viewer.video_stopped", json!({}));
    }

    /// The page is being drawn: its panel is shown, its player opened again
    /// if that stopped it, at the same place, paused.
    pub(super) fn video_in_view(&self, app: &AppHandle) {
        let reopen = {
            let mut v = lock(&self.video);
            let Some(s) = v.session.as_mut() else {
                return;
            };
            s.unseen = 0;
            s.player
                .is_none()
                .then(|| (s.target.clone(), s.path.clone(), s.position))
        };
        let Some((target, path, position)) = reopen else {
            return;
        };
        let opened = self.open_player(app, &path).and_then(|mut player| {
            if !position.is_zero() {
                player.seek(position)?;
            }
            Ok(player)
        });
        match opened {
            Ok(player) => {
                if let Some(s) = lock(&self.video).session.as_mut() {
                    s.player = Some(player);
                }
            }
            Err(e) => self.video_failed(app, &target, &e),
        }
    }

    fn with_player(
        &self,
        app: &AppHandle,
        act: impl FnOnce(&mut Session, &mut dyn VideoPlayer) -> Result<(), String>,
    ) -> Result<Json, String> {
        let done = {
            let mut v = lock(&self.video);
            let s = v.session.as_mut().ok_or("no video is open")?;
            let Some(mut player) = s.player.take() else {
                return Err("the video is stopped while its panel is hidden".into());
            };
            let done = act(s, player.as_mut());
            s.player = Some(player);
            done.map_err(|e| (s.target.clone(), e))
        };
        match done {
            Ok(()) => {
                app.egui().request_repaint();
                Ok(self.video_state())
            }
            Err((target, e)) => {
                self.video_failed(app, &target, &e);
                Err(e)
            }
        }
    }

    /// Plays or pauses; from the end, plays from the start.
    pub fn video_toggle(&self, app: &AppHandle) -> Result<Json, String> {
        self.with_player(app, |s, player| {
            if s.status.state == VideoState::Playing {
                player.pause()?;
                s.status.state = VideoState::Paused;
            } else {
                player.play()?;
                if s.status.state == VideoState::Ended {
                    s.position = Duration::ZERO;
                }
                s.status.state = VideoState::Playing;
            }
            Ok(())
        })
    }

    /// Moves to a position, kept within the video.
    pub fn video_seek_to(&self, app: &AppHandle, seconds: f64) -> Result<Json, String> {
        self.with_player(app, |s, player| {
            let to = Duration::try_from_secs_f64(seconds.max(0.0)).unwrap_or(Duration::ZERO);
            let to = s.status.duration.map_or(to, |d| to.min(d));
            player.seek(to)?;
            s.position = to;
            if s.status.state == VideoState::Ended {
                s.status.state = VideoState::Paused;
            }
            Ok(())
        })
    }

    /// Moves the position forward, or back with a negative number.
    pub fn video_seek_by(&self, app: &AppHandle, seconds: f64) -> Result<Json, String> {
        let from = lock(&self.video)
            .session
            .as_ref()
            .map_or(Duration::ZERO, |s| s.position);
        self.video_seek_to(app, from.as_secs_f64() + seconds)
    }

    /// Sets the volume, from 0 to 1, kept for the session's next videos.
    pub fn video_volume(&self, app: &AppHandle, volume: f32) -> Result<Json, String> {
        let volume = volume.clamp(0.0, 1.0);
        lock(&self.video).volume = volume;
        self.with_player(app, |_, player| {
            player.set_volume(volume);
            Ok(())
        })
    }

    /// Changes the volume by an amount.
    pub fn video_volume_by(&self, app: &AppHandle, by: f32) -> Result<Json, String> {
        let volume = lock(&self.video).volume;
        self.video_volume(app, volume + by)
    }

    /// What the page holds, for `viewer.state`: null without a video page.
    pub(super) fn video_state(&self) -> Json {
        let v = lock(&self.video);
        let Some(s) = v.session.as_ref() else {
            return Json::Null;
        };
        json!({
            "backend": v.backend.as_ref().map(|b| b.name().to_string()),
            "state": s.status.state,
            "stopped": s.player.is_none(),
            "position_ms": s.position.as_millis() as u64,
            "duration_ms": s.status.duration.map(|d| d.as_millis() as u64),
            // In hundredths, not the float's long tail.
            "volume": f64::from((v.volume * 100.0).round()) / 100.0,
            "size": s.texture.as_ref().map(TextureHandle::size),
            "note": s.status.note,
        })
    }

    /// Acts on what the user asked while the page was drawn.
    pub(super) fn video_act(&self, app: &AppHandle, act: Act) {
        let done = match act {
            Act::Toggle => self.video_toggle(app),
            Act::SeekTo(s) => self.video_seek_to(app, s),
            Act::Volume(v) => self.video_volume(app, v),
        };
        // A player that failed has left its reason on the error page; a page
        // whose panel has just gone has nothing to say.
        if let Err(e) = done {
            log::debug!("viewer: {e}");
        }
    }
}

/// Draws the page: the picture, the position, Play or Pause with the time, the
/// volume.
pub(super) fn draw(ui: &mut Ui, v: &Viewer, acts: &mut Vec<Act>) {
    let ctx = ui.ctx().clone();
    let (playing, position, duration, volume, focus, name, note) = {
        let mut video = lock(&v.video);
        let volume = video.volume;
        let Some(s) = video.session.as_mut() else {
            return;
        };
        if let Some(frame) = s.player.as_mut().and_then(|p| p.frame()) {
            s.aspect = frame.aspect;
            match s.texture.as_mut() {
                Some(t) => t.set(frame.image, TextureOptions::LINEAR),
                None => {
                    s.texture =
                        Some(ctx.load_texture(TEXTURE, frame.image, TextureOptions::LINEAR));
                }
            }
        }
        let name = s
            .path
            .file_name()
            .map_or_else(|| s.target.clone(), |n| n.to_string_lossy().into_owned());
        (
            s.status.state == VideoState::Playing,
            s.position,
            s.status.duration,
            volume,
            std::mem::take(&mut s.focus),
            name,
            s.status.note.clone(),
        )
    };
    // Only while it moves: the frames ask for their own repaints, this keeps
    // the time going in a file without pictures.
    if playing {
        ctx.request_repaint_after(TIME_REPAINT);
    }
    let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
    let rows = if note.is_some() { 3.0 } else { 2.0 };
    let height = (ui.available_height() - rows * row).max(row * 2.0);
    let picture = picture(ui, v, height, &name);
    if focus {
        picture.request_focus();
    }
    if picture.clicked() {
        picture.request_focus();
        acts.push(Act::Toggle);
    }
    if let Some(note) = &note {
        ui.weak(note);
    }
    position_row(ui, v, position, duration, acts);
    ui.horizontal(|ui| {
        let label = tr(
            &ctx,
            if playing {
                &txt::VIEWER_VIDEO_PAUSE
            } else {
                &txt::VIEWER_VIDEO_PLAY
            },
        );
        let button = ui.button(label);
        if button.clicked() {
            acts.push(Act::Toggle);
            // By the pointer the keys stay with the picture; by the keyboard
            // with the button.
            if button.clicked_by(egui::PointerButton::Primary) && !button.has_focus() {
                picture.request_focus();
            }
        }
        let total = duration.map_or_else(|| "-:--".to_string(), clock);
        ui.monospace(tr_args(
            &ctx,
            &txt::VIEWER_VIDEO_TIME,
            &[("elapsed", &clock(position)), ("total", &total)],
        ));
        let mut percent = f64::from(volume) * 100.0;
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

/// The picture, its surface around it: its keys hold while it has the focus
/// or was last pressed.
fn picture(ui: &mut Ui, v: &Viewer, height: f32, name: &str) -> egui::Response {
    let ctx = ui.ctx().clone();
    let size = vec2(ui.available_width(), height);
    let rect = Rect::from_min_size(ui.cursor().min, size);
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        egui_pop_core::focus::scope(ui, VIDEO_SURFACE, &[], |ui| {
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            egui_pop_core::a11y::name(
                &response,
                egui::Role::Image,
                &tr_args(&ctx, &txt::VIEWER_VIDEO_PICTURE, &[("name", name)]),
            );
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            let video = lock(&v.video);
            let session = video.session.as_ref();
            match session.and_then(|s| s.texture.as_ref().map(|t| (t, s.aspect))) {
                Some((texture, aspect)) => {
                    let fit = Rect::from_center_size(rect.center(), fitted(rect.size(), aspect));
                    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    painter.image(texture.id(), fit, uv, Color32::WHITE);
                }
                None => {
                    let loading = session.is_some_and(|s| s.status.state == VideoState::Loading);
                    let text = if loading {
                        tr(&ctx, &txt::VIEWER_VIDEO_LOADING)
                    } else {
                        name.to_string()
                    };
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        text,
                        egui::TextStyle::Body.resolve(ui.style()),
                        ui.visuals().weak_text_color(),
                    );
                }
            }
            if response.has_focus() {
                // The arrows are the picture's bindings, not egui's way to
                // move the focus to a neighbour.
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
                painter.rect_stroke(
                    rect,
                    0.0,
                    ui.visuals().selection.stroke,
                    egui::StrokeKind::Inside,
                );
            }
            response
        })
    })
    .inner
}

/// The largest size of that shape within `room`.
fn fitted(room: egui::Vec2, aspect: f32) -> egui::Vec2 {
    let aspect = if aspect > 0.0 { aspect } else { 16.0 / 9.0 };
    if room.x / aspect <= room.y {
        vec2(room.x, room.x / aspect)
    } else {
        vec2(room.y * aspect, room.y)
    }
}

/// The position's slider: it follows the video, except during a drag, whose end
/// seeks. Without a length, there is nowhere to go yet.
fn position_row(
    ui: &mut Ui,
    v: &Viewer,
    position: Duration,
    duration: Option<Duration>,
    acts: &mut Vec<Act>,
) {
    let ctx = ui.ctx().clone();
    let name = tr(&ctx, &txt::VIEWER_VIDEO_POSITION);
    let scrub = lock(&v.video).session.as_ref().and_then(|s| s.scrub);
    let total = duration.map_or(0.0, |d| d.as_secs_f64());
    let mut at = scrub.unwrap_or(position.as_secs_f64());
    let enabled = total > 0.0;
    let out = ui
        .add_enabled_ui(enabled, |ui| {
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
    let scrub = scrub.or(if out.response.dragged() {
        lock(&v.video).session.as_ref().and_then(|s| s.scrub)
    } else {
        None
    });
    if let Some(s) = lock(&v.video).session.as_mut() {
        s.scrub = scrub;
    }
}

impl Viewer {
    fn show_video_page(&self, app: &AppHandle, target: &str, path: PathBuf) {
        let page = Arc::new(super::Page {
            target: target.to_string(),
            content: super::Content::Video(path),
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
            json!({ "target": target, "kind": "video" }),
        );
    }
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
}
