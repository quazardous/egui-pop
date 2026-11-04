//! The viewer's video page (ADR 0017): a local video file played in the panel
//! by the [`Video`](crate::video::Video) widget, with its full controls,
//! under the id [`VIDEO_ID`].
//!
//! The backend is the build's (GStreamer with the `video` feature), none
//! without it: a local video then opens outside, as any local file (the policy
//! asks [`Viewer::plays_video`]).
//!
//! - **Stopping**: leaving the page forgets the widget, whose player stops. A
//!   panel no longer drawn (closed, or a tab behind another) stops the player
//!   too and keeps the position: showing it again opens the file there,
//!   paused.
//! - **Keys**: the picture is a Tab stop, its keys are the viewer's commands,
//!   bound on its own surface ([`VIDEO_SURFACE`]): Space plays or pauses, Left
//!   and Right move by [`SEEK_STEP`](crate::video::SEEK_STEP), Up and Down
//!   change the volume by [`VOLUME_STEP`](crate::video::VOLUME_STEP).
//! - **Failures**: the player's failure, opening or playing, ends on the
//!   viewer's error page.

use std::path::PathBuf;
use std::sync::Arc;

use egui::Ui;
use egui_pop_api::AppHandle;
use egui_pop_core::lock;
use serde_json::{Value as Json, json};

use super::{Failure, PLUGIN, Status, Viewer};
use crate::video::{Controls, VideoBackend, Videos};

/// The surface the picture's keys are bound on (`viewerVideoFocus`).
pub const VIDEO_SURFACE: &str = "viewerVideo";
/// The context key saying a video page is shown.
pub const VIDEO_CONTEXT: &str = "viewerVideo";
/// The id of the viewer's video, for the `video.*` commands and the `videos`
/// collector.
pub const VIDEO_ID: &str = "egui_pop.viewer";

/// The page shown.
struct Page {
    target: String,
    path: PathBuf,
    /// The context its widget lives in.
    egui: egui::Context,
    /// The picture takes the focus when next drawn.
    focus: bool,
    /// The journal was told its player stopped out of view.
    stopped_told: bool,
}

/// The viewer's video state: the backend, the volume for the session, the page.
pub(super) struct Video {
    backend: Option<Arc<dyn VideoBackend>>,
    volume: f32,
    page: Option<Page>,
}

impl Video {
    pub(super) fn new(backend: Option<Arc<dyn VideoBackend>>) -> Self {
        Self {
            backend,
            volume: 1.0,
            page: None,
        }
    }

    pub(super) fn set_backend(&mut self, backend: Option<Arc<dyn VideoBackend>>) {
        self.backend = backend;
    }
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
        if !self.plays_video() {
            return self.fail(
                app,
                target,
                Failure::Playback,
                "this application has no video player",
            );
        }
        // A new page: a new player, from the start.
        Videos::of(&app.egui()).forget(VIDEO_ID);
        lock(&self.video).page = Some(Page {
            target: target.to_string(),
            path: path.clone(),
            egui: app.egui(),
            focus: true,
            stopped_told: false,
        });
        app.set_context(VIDEO_CONTEXT, "true");
        self.show_video_page(app, target, path);
    }

    /// Leaves the video page: the player stops, what it showed goes.
    pub(super) fn stop_video(&self, app: &AppHandle) {
        let page = lock(&self.video).page.take();
        if page.is_some() {
            Videos::of(&app.egui()).forget(VIDEO_ID);
            app.set_context(VIDEO_CONTEXT, "false");
        }
    }

    pub(super) fn video_failed(&self, app: &AppHandle, target: &str, detail: &str) {
        self.stop_video(app);
        self.fail(app, target, Failure::Playback, detail);
    }

    /// Once per pass of the application: tells the journal when the page's
    /// player stopped out of view (`viewer.video_stopped`).
    pub(super) fn video_pass(&self, app: &AppHandle) {
        let stopped = {
            let mut v = lock(&self.video);
            let Some(page) = v.page.as_mut() else {
                return;
            };
            let now = Videos::of(&page.egui)
                .state(VIDEO_ID)
                .is_some_and(|s| s["stopped"] == true);
            let tell = now && !page.stopped_told;
            page.stopped_told = now;
            tell
        };
        if stopped {
            app.publish(PLUGIN.as_str(), None, "viewer.video_stopped", json!({}));
        }
    }

    /// Runs `act` on the page's video; a player that fails there ends on the
    /// error page.
    fn on_video(
        &self,
        app: &AppHandle,
        act: impl FnOnce(&Videos) -> Result<Json, String>,
    ) -> Result<Json, String> {
        let target = lock(&self.video)
            .page
            .as_ref()
            .map(|p| p.target.clone())
            .ok_or("no video is open")?;
        let videos = Videos::of(&app.egui());
        match act(&videos) {
            Ok(_) => Ok(self.video_state()),
            Err(e) => {
                let failed = videos
                    .state(VIDEO_ID)
                    .and_then(|s| s["error"].as_str().map(str::to_string));
                if let Some(detail) = failed {
                    self.video_failed(app, &target, &detail);
                }
                Err(e)
            }
        }
    }

    /// Plays or pauses; from the end, plays from the start.
    pub fn video_toggle(&self, app: &AppHandle) -> Result<Json, String> {
        self.on_video(app, |v| v.toggle(Some(VIDEO_ID)))
    }

    /// Moves to a position, kept within the video.
    pub fn video_seek_to(&self, app: &AppHandle, seconds: f64) -> Result<Json, String> {
        self.on_video(app, |v| v.seek_to(Some(VIDEO_ID), seconds))
    }

    /// Moves the position forward, or back with a negative number.
    pub fn video_seek_by(&self, app: &AppHandle, seconds: f64) -> Result<Json, String> {
        self.on_video(app, |v| v.seek_by(Some(VIDEO_ID), seconds))
    }

    /// Sets the volume, from 0 to 1, kept for the session's next videos.
    pub fn video_volume(&self, app: &AppHandle, volume: f32) -> Result<Json, String> {
        let volume = volume.clamp(0.0, 1.0);
        lock(&self.video).volume = volume;
        self.on_video(app, |v| v.set_volume(Some(VIDEO_ID), volume))
    }

    /// Changes the volume by an amount.
    pub fn video_volume_by(&self, app: &AppHandle, by: f32) -> Result<Json, String> {
        let volume = lock(&self.video).volume;
        self.video_volume(app, volume + by)
    }

    /// What the page holds, for `viewer.state`: null without a video page.
    pub(super) fn video_state(&self) -> Json {
        let v = lock(&self.video);
        let Some(page) = v.page.as_ref() else {
            return Json::Null;
        };
        let backend = v.backend.as_ref().map(|b| b.name().to_string());
        // In hundredths, not the float's long tail.
        let volume = f64::from((v.volume * 100.0).round()) / 100.0;
        match Videos::of(&page.egui).state(VIDEO_ID) {
            Some(mut s) => {
                s["volume"] = json!(volume);
                s
            }
            // Not drawn yet.
            None => json!({
                "video": VIDEO_ID,
                "source": page.path.display().to_string(),
                "backend": backend,
                "state": "loading",
                "stopped": false,
                "position_ms": 0,
                "duration_ms": null,
                "volume": volume,
                "size": null,
                "note": null,
            }),
        }
    }

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

/// Draws the page: the widget with its full controls. Its failure, with the
/// page's target, for the error page.
pub(super) fn draw(ui: &mut Ui, app: &AppHandle, v: &Viewer) -> Option<(String, String)> {
    let (target, path, backend, volume, focus) = {
        let mut video = lock(&v.video);
        let (backend, volume) = (video.backend.clone(), video.volume);
        let page = video.page.as_mut()?;
        (
            page.target.clone(),
            page.path.clone(),
            backend,
            volume,
            std::mem::take(&mut page.focus),
        )
    };
    let out = crate::video::Video::new(&path)
        .id(VIDEO_ID)
        .surface(VIDEO_SURFACE)
        .controls(Controls::Full)
        .backend(backend)
        .volume(volume)
        .size(ui.available_size())
        .take_focus(focus)
        .app(app)
        .show(ui);
    if !out.muted {
        // The slider's volume is the session's.
        lock(&v.video).volume = out.volume;
    }
    out.error.map(|e| (target, e))
}
