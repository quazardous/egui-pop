//! A video's state from one pass to the next (its player, position, volume,
//! texture), the context's registry of them, and the sweep stopping the
//! players of videos no longer drawn.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Context, Id, TextureHandle, TextureOptions, ViewportId};
use egui_pop_core::lock;
use egui_pop_core::state::Slot;
use serde_json::{Value as Json, json};

use super::{
    Controls, DEFAULT_ASPECT, VideoBackend, VideoPlayer, VideoRequest, VideoState, VideoStatus,
};

/// The passes a video may go undrawn before its player stops: a pass may run
/// before the part holding it is drawn in it.
const HIDDEN_PASSES: u64 = 3;
/// The texture's name, for egui's debugging.
const TEXTURE: &str = "egui_pop.video";

/// What the user asked of a video while it was drawn.
#[derive(Clone, Copy, Debug)]
pub(super) enum Act {
    Toggle,
    SeekTo(f64),
    SeekBy(f64),
    Volume(f32),
    VolumeBy(f32),
}

/// One video's state.
pub(super) struct Session {
    pub(super) source: PathBuf,
    /// Its id, for agents.
    pub(super) name: Option<String>,
    /// None: the placeholder.
    backend: Option<Arc<dyn VideoBackend>>,
    /// None before it opens, while out of view, and after a failure.
    player: Option<Box<dyn VideoPlayer>>,
    pub(super) status: VideoStatus,
    /// The last position known, kept when the player stops.
    pub(super) position: Duration,
    pub(super) volume: f32,
    pub(super) muted: bool,
    pub(super) autoplay: bool,
    pub(super) controls: Controls,
    looping: bool,
    /// What the player was told of looping, and whether it loops by itself.
    looping_told: Option<bool>,
    native_loop: bool,
    /// The times the widget sought back to the start itself.
    loops: u64,
    pub(super) texture: Option<TextureHandle>,
    pub(super) aspect: f32,
    /// The position while the slider is dragged.
    pub(super) scrub: Option<f64>,
    /// When it was last drawn, and in which window.
    pass: u64,
    viewport: ViewportId,
    pub(super) error: Option<String>,
    /// It played or moved once: the poster gives way to the frames.
    pub(super) started: bool,
    /// Its player was stopped while it was out of view.
    pub(super) stopped: bool,
}

impl Session {
    pub(super) fn new(
        source: PathBuf,
        backend: Option<Arc<dyn VideoBackend>>,
        volume: f32,
        muted: bool,
    ) -> Self {
        Self {
            source,
            name: None,
            backend,
            player: None,
            status: VideoStatus::default(),
            position: Duration::ZERO,
            volume: volume.clamp(0.0, 1.0),
            muted,
            autoplay: false,
            controls: Controls::Full,
            looping: false,
            looping_told: None,
            native_loop: false,
            loops: 0,
            texture: None,
            aspect: DEFAULT_ASPECT,
            scrub: None,
            pass: 0,
            viewport: ViewportId::ROOT,
            error: None,
            started: false,
            stopped: false,
        }
    }

    /// No player for it: the placeholder is drawn.
    pub(super) fn unavailable(&self) -> bool {
        self.backend.is_none()
    }

    /// Its shape: the frame's, else the poster's, else 16:9.
    pub(super) fn aspect_or(&self, poster: Option<f32>) -> f32 {
        let aspect = if self.texture.is_some() {
            self.aspect
        } else {
            poster.unwrap_or(DEFAULT_ASPECT)
        };
        if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            DEFAULT_ASPECT
        }
    }

    /// The volume heard: none while muted.
    pub(super) fn audible_volume(&self) -> f32 {
        if self.muted { 0.0 } else { self.volume }
    }

    pub(super) fn seen(&mut self, pass: u64, viewport: ViewportId) {
        self.pass = pass;
        self.viewport = viewport;
    }

    /// Each pass it is drawn: opens the player (again, after being out of
    /// view), takes what it says and its last frame, loops.
    pub(super) fn update(&mut self, ctx: &Context, looping: bool) {
        self.looping = looping;
        if self.error.is_some() {
            return;
        }
        let Some(backend) = self.backend.clone() else {
            return;
        };
        if self.player.is_none() {
            let opened = backend.open(&VideoRequest {
                path: self.source.clone(),
                egui: ctx.clone(),
                volume: self.audible_volume(),
            });
            let mut player = match opened {
                Ok(p) => p,
                Err(e) => return self.fail(e),
            };
            if !self.position.is_zero()
                && let Err(e) = player.seek(self.position)
            {
                return self.fail(e);
            }
            self.player = Some(player);
            self.looping_told = None;
            self.stopped = false;
            if self.autoplay
                && let Err(e) = self.play()
            {
                return self.fail(e);
            }
        }
        let Some(player) = self.player.as_mut() else {
            return;
        };
        if self.looping_told != Some(self.looping) {
            self.native_loop = player.set_looping(self.looping) && self.looping;
            self.looping_told = Some(self.looping);
        }
        let status = match player.poll() {
            Ok(s) => s,
            Err(e) => return self.fail(e),
        };
        if let Some(p) = status.position {
            self.position = p;
        }
        self.status = status;
        if let Some(frame) = player.frame() {
            self.aspect = frame.aspect;
            match self.texture.as_mut() {
                Some(t) => t.set(frame.image, TextureOptions::LINEAR),
                None => {
                    self.texture =
                        Some(ctx.load_texture(TEXTURE, frame.image, TextureOptions::LINEAR));
                }
            }
        }
        if self.status.state == VideoState::Ended && self.looping && !self.native_loop {
            // The player cannot loop: back to the start, with the gap a
            // flushing seek makes.
            self.loops += 1;
            if let Err(e) = self.seek_to(0.0).and_then(|()| self.play()) {
                self.fail(e);
            }
        }
    }

    /// The player failed: it stops, the picture says why.
    fn fail(&mut self, error: String) {
        self.player = None;
        self.texture = None;
        self.error = Some(error);
    }

    /// Runs `f` on the player; a failure ends it.
    fn with_player(
        &mut self,
        f: impl FnOnce(&mut Self, &mut dyn VideoPlayer) -> Result<(), String>,
    ) -> Result<(), String> {
        if let Some(e) = &self.error {
            return Err(format!("the video failed: {e}"));
        }
        if self.unavailable() {
            return Err("this application has no video player".into());
        }
        let Some(mut player) = self.player.take() else {
            return Err(if self.stopped {
                "the video is stopped while out of view".into()
            } else {
                "the video has not been drawn yet".into()
            });
        };
        let done = f(self, player.as_mut());
        self.player = Some(player);
        if let Err(e) = &done {
            self.fail(e.clone());
        }
        done
    }

    /// What the user asked while it was drawn; a failure is shown in place.
    pub(super) fn act(&mut self, ctx: &Context, act: Act) {
        let _ = match act {
            Act::Toggle => self.toggle(),
            Act::SeekTo(s) => self.seek_to(s),
            Act::SeekBy(by) => self.seek_to(self.position.as_secs_f64() + by),
            Act::Volume(v) => self.set_volume(v),
            Act::VolumeBy(by) => self.set_volume(self.audible_volume() + by),
        };
        ctx.request_repaint();
    }

    /// Plays or pauses; from the end, plays from the start.
    pub(super) fn toggle(&mut self) -> Result<(), String> {
        if self.status.state == VideoState::Playing {
            self.pause()
        } else {
            self.play()
        }
    }

    pub(super) fn play(&mut self) -> Result<(), String> {
        self.with_player(|s, player| {
            player.play()?;
            if s.status.state == VideoState::Ended {
                s.position = Duration::ZERO;
            }
            s.status.state = VideoState::Playing;
            s.started = true;
            Ok(())
        })
    }

    pub(super) fn pause(&mut self) -> Result<(), String> {
        self.with_player(|s, player| {
            player.pause()?;
            if s.status.state != VideoState::Ended {
                s.status.state = VideoState::Paused;
            }
            Ok(())
        })
    }

    /// Moves to a position, kept within the video.
    pub(super) fn seek_to(&mut self, seconds: f64) -> Result<(), String> {
        self.with_player(|s, player| {
            let to = Duration::try_from_secs_f64(seconds.max(0.0)).unwrap_or(Duration::ZERO);
            let to = s.status.duration.map_or(to, |d| to.min(d));
            player.seek(to)?;
            s.position = to;
            s.started = true;
            if s.status.state == VideoState::Ended {
                s.status.state = VideoState::Paused;
            }
            Ok(())
        })
    }

    /// Sets the volume, from 0 to 1, and gives the sound back.
    pub(super) fn set_volume(&mut self, volume: f32) -> Result<(), String> {
        self.volume = volume.clamp(0.0, 1.0);
        self.muted = false;
        let volume = self.volume;
        self.with_player(|_, player| {
            player.set_volume(volume);
            Ok(())
        })
    }

    pub(super) fn set_muted(&mut self, muted: bool) -> Result<(), String> {
        self.muted = muted;
        let volume = self.audible_volume();
        self.with_player(|_, player| {
            player.set_volume(volume);
            Ok(())
        })
    }

    /// What it holds, for the `videos` collector and the commands.
    pub(super) fn json(&self) -> Json {
        let ms = |d: Duration| u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
        let loops = self.loops + self.player.as_ref().map_or(0, |p| p.loops());
        json!({
            "video": self.name,
            "source": self.source.display().to_string(),
            "backend": self.backend.as_ref().map(|b| b.name().to_string()),
            "controls": self.controls,
            "state": self.status.state,
            "playing": self.status.state == VideoState::Playing,
            "stopped": self.stopped,
            "position_ms": ms(self.position),
            "duration_ms": self.status.duration.map(ms),
            // In hundredths, not the float's long tail.
            "volume": f64::from((self.volume * 100.0).round()) / 100.0,
            "muted": self.muted,
            "looping": self.looping,
            "loops": loops,
            "size": self.texture.as_ref().map(TextureHandle::size),
            "note": self.status.note,
            "error": self.error,
        })
    }
}

/// The videos of a context.
#[derive(Default)]
pub(super) struct Registry {
    sessions: HashMap<Id, Session>,
    /// The named video whose picture last had the focus.
    pub(super) focused: Option<String>,
    swept: bool,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("videos", &self.sessions.len())
            .field("focused", &self.focused)
            .finish()
    }
}

impl Registry {
    pub(super) fn session(&mut self, key: Id, fresh: impl FnOnce() -> Session) -> &mut Session {
        self.sessions.entry(key).or_insert_with(fresh)
    }

    fn named(&mut self, name: &str) -> Option<&mut Session> {
        self.sessions
            .values_mut()
            .find(|s| s.name.as_deref() == Some(name))
    }

    /// The video a command acts on: `video`, else the one that last had the
    /// focus, else the only one with an id.
    fn target(&mut self, video: Option<&str>) -> Result<&mut Session, String> {
        let name = match video {
            Some(v) => v.to_string(),
            None => match self.focused.clone() {
                Some(f) => f,
                None => {
                    let mut names = self.sessions.values().filter_map(|s| s.name.clone());
                    match (names.next(), names.next()) {
                        (Some(only), None) => only,
                        (None, _) => return Err("no video with an id is drawn".into()),
                        _ => {
                            return Err(
                                "several videos have an id and none had the focus; name one with `video`"
                                    .into(),
                            );
                        }
                    }
                }
            },
        };
        self.named(&name)
            .ok_or_else(|| format!("no video `{name}`"))
    }
}

/// The registry in the context's memory.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Registry>>);

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        lock(&self.0).fmt(f)
    }
}

const SLOT: Slot<Shared> = Slot::new("views.video");

/// The videos drawn in a context, by id: what the `video.*` commands and the
/// `videos` collector reach, and the viewer's video page.
#[derive(Clone)]
pub struct Videos {
    ctx: Context,
    registry: Arc<Mutex<Registry>>,
}

impl std::fmt::Debug for Videos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        lock(&self.registry).fmt(f)
    }
}

impl Videos {
    /// The context's videos.
    pub fn of(ctx: &Context) -> Self {
        Self {
            ctx: ctx.clone(),
            registry: SLOT.get_or_insert_with(ctx, Shared::default).0,
        }
    }

    pub(super) fn registry(&self) -> Arc<Mutex<Registry>> {
        self.registry.clone()
    }

    /// Stops the players of videos no longer drawn, once per pass.
    pub(super) fn install_sweep(&self) {
        let mut reg = lock(&self.registry);
        if !reg.swept {
            reg.swept = true;
            drop(reg);
            self.ctx.add_plugin(Sweep);
        }
    }

    /// The ids of the videos drawn with one.
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = lock(&self.registry)
            .sessions
            .values()
            .filter_map(|s| s.name.clone())
            .collect();
        ids.sort();
        ids
    }

    /// What a video holds: its state, position (`position_ms`), length
    /// (`duration_ms`), volume, whether it plays, loops or is muted, its
    /// picture's size, a note or an error.
    pub fn state(&self, video: &str) -> Option<Json> {
        lock(&self.registry).named(video).map(|s| s.json())
    }

    /// What the `videos` collector shows: every video with an id, the one
    /// that last had the focus, and how many have none.
    pub fn describe(&self) -> Json {
        let reg = lock(&self.registry);
        let mut named = serde_json::Map::new();
        let mut unnamed = 0;
        for s in reg.sessions.values() {
            match &s.name {
                Some(n) => {
                    named.insert(n.clone(), s.json());
                }
                None => unnamed += 1,
            }
        }
        json!({ "videos": named, "focused": reg.focused, "without_id": unnamed })
    }

    /// Runs `f` on a video (see [`Self::toggle`] for which), then says what it
    /// holds.
    fn run(
        &self,
        video: Option<&str>,
        f: impl FnOnce(&mut Session) -> Result<(), String>,
    ) -> Result<Json, String> {
        let done = {
            let mut reg = lock(&self.registry);
            let s = reg.target(video)?;
            f(s).map(|()| s.json())
        };
        self.ctx.request_repaint();
        done
    }

    /// Plays or pauses a video: `video` by its id, else the one whose picture
    /// last had the focus, else the only one with an id. From its end, it
    /// plays from the start.
    pub fn toggle(&self, video: Option<&str>) -> Result<Json, String> {
        self.run(video, Session::toggle)
    }

    pub fn play(&self, video: Option<&str>) -> Result<Json, String> {
        self.run(video, Session::play)
    }

    pub fn pause(&self, video: Option<&str>) -> Result<Json, String> {
        self.run(video, Session::pause)
    }

    /// Moves to a position in seconds, kept within the video.
    pub fn seek_to(&self, video: Option<&str>, seconds: f64) -> Result<Json, String> {
        self.run(video, |s| s.seek_to(seconds))
    }

    /// Moves forward, or back with a negative number.
    pub fn seek_by(&self, video: Option<&str>, seconds: f64) -> Result<Json, String> {
        self.run(video, |s| s.seek_to(s.position.as_secs_f64() + seconds))
    }

    /// Sets the volume, from 0 to 1, and gives the sound back.
    pub fn set_volume(&self, video: Option<&str>, volume: f32) -> Result<Json, String> {
        self.run(video, |s| s.set_volume(volume))
    }

    /// Changes the volume heard by an amount.
    pub fn volume_by(&self, video: Option<&str>, by: f32) -> Result<Json, String> {
        self.run(video, |s| s.set_volume(s.audible_volume() + by))
    }

    /// Mutes or gives the sound back; `None` switches.
    pub fn set_muted(&self, video: Option<&str>, muted: Option<bool>) -> Result<Json, String> {
        self.run(video, |s| {
            let muted = muted.unwrap_or(!s.muted);
            s.set_muted(muted)
        })
    }

    /// Forgets a video: its player stops, and drawn again it starts afresh.
    /// Whether there was one.
    pub fn forget(&self, video: &str) -> bool {
        let gone = {
            let mut reg = lock(&self.registry);
            let before = reg.sessions.len();
            let mut gone = Vec::new();
            reg.sessions.retain(|_, s| {
                if s.name.as_deref() == Some(video) {
                    gone.push(s.player.take());
                    false
                } else {
                    true
                }
            });
            if reg.focused.as_deref() == Some(video) {
                reg.focused = None;
            }
            let found = reg.sessions.len() != before;
            drop(reg);
            drop(gone);
            found
        };
        self.ctx.request_repaint();
        gone
    }

    /// Once per pass of a window: stops the players of its videos not drawn
    /// for a few passes, and keeps passes coming until they are stopped.
    fn sweep(&self) {
        let now = self.ctx.cumulative_pass_nr();
        let viewport = self.ctx.viewport_id();
        let mut stopped = Vec::new();
        let mut waiting = false;
        {
            let mut reg = lock(&self.registry);
            for s in reg.sessions.values_mut() {
                if s.viewport != viewport || s.player.is_none() {
                    continue;
                }
                let unseen = now.saturating_sub(s.pass);
                if unseen >= HIDDEN_PASSES {
                    stopped.push(s.player.take());
                    s.texture = None;
                    s.stopped = true;
                    if s.status.state != VideoState::Ended {
                        s.status.state = VideoState::Paused;
                    }
                } else if unseen > 0 {
                    waiting = true;
                }
            }
        }
        drop(stopped);
        if waiting {
            self.ctx.request_repaint();
        }
    }
}

/// egui's hook running the sweep at the end of each pass.
struct Sweep;

impl egui::Plugin for Sweep {
    fn debug_name(&self) -> &'static str {
        "egui_pop.video.sweep"
    }

    fn on_end_pass(&mut self, ui: &mut egui::Ui) {
        Videos::of(ui.ctx()).sweep();
    }
}
