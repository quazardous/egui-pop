//! The viewer's video player over GStreamer (feature `video`): `playbin` opens
//! the file and picks the decoders, an `appsink` takes the pictures as RGBA on
//! GStreamer's own threads, and the interface's thread only reads the last
//! one and the bus. This module is the only code that names GStreamer.
//!
//! The binary links GStreamer's libraries, so a machine without them does not
//! start it at all: a package declares them as a dependency. What can be missing
//! at run time and still be reported, on the error page, is what GStreamer
//! loads by name: `playbin` (the base plugins), a demuxer or a decoder.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::Context;
use egui_pop_core::lock;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;

use super::video::{
    VideoBackend, VideoFrame, VideoPlayer, VideoRequest, VideoState, VideoStatus, rgba_image,
};

/// The last picture decoded, waiting for the interface.
type Mailbox = Arc<Mutex<Option<VideoFrame>>>;

/// How long the test clip's pipeline may take.
const CLIP_DEADLINE: gst::ClockTime = gst::ClockTime::from_seconds(60);

/// Videos through the system's GStreamer.
#[derive(Clone, Debug, Default)]
pub struct GStreamer {
    audio_sink: Option<String>,
}

impl GStreamer {
    /// The element a video's sound goes to, by its factory's name
    /// (`pulsesink`, `fakesink` for silence); by default what GStreamer
    /// finds for this system.
    pub fn audio_sink(mut self, element: &str) -> Self {
        self.audio_sink = Some(element.to_string());
        self
    }
}

impl VideoBackend for GStreamer {
    fn name(&self) -> &str {
        "gstreamer"
    }

    fn open(&self, request: &VideoRequest) -> Result<Box<dyn VideoPlayer>, String> {
        gst::init().map_err(|e| format!("GStreamer did not start: {e}"))?;
        let uri = gst::glib::filename_to_uri(&request.path, None)
            .map_err(|e| format!("this path cannot be given to GStreamer: {e}"))?;
        let playbin = gst::ElementFactory::make("playbin").build().map_err(|_| {
            "GStreamer's `playbin` element is missing: install GStreamer's base plugins".to_string()
        })?;
        playbin.set_property("uri", uri.as_str());
        playbin.set_property("volume", f64::from(request.volume.clamp(0.0, 1.0)));
        if let Some(name) = &self.audio_sink {
            let sink = gst::ElementFactory::make(name)
                .build()
                .map_err(|_| format!("GStreamer has no `{name}` element for the sound"))?;
            playbin.set_property("audio-sink", &sink);
        }
        let mailbox = Mailbox::default();
        playbin.set_property("video-sink", picture_sink(&mailbox, &request.egui));
        let bus = playbin
            .bus()
            .ok_or("GStreamer's `playbin` element has no bus")?;
        let wake = request.egui.clone();
        bus.set_sync_handler(move |_, message| {
            use gst::MessageView as M;
            if matches!(
                message.view(),
                M::Error(_)
                    | M::Warning(_)
                    | M::Eos(_)
                    | M::AsyncDone(_)
                    | M::StateChanged(_)
                    | M::DurationChanged(_)
            ) {
                wake.request_repaint();
            }
            gst::BusSyncReply::Pass
        });
        let mut player = Player {
            playbin,
            bus,
            mailbox,
            playing: false,
            ready: false,
            ended: false,
            pending_seek: None,
            note: None,
        };
        // Up to the first picture, on GStreamer's threads: the bus says how
        // it went.
        if player.playbin.set_state(gst::State::Paused).is_err() {
            return Err(player
                .poll()
                .err()
                .unwrap_or_else(|| "GStreamer refused to open this file".to_string()));
        }
        Ok(Box::new(player))
    }
}

/// The element taking the pictures, in RGBA whatever the file holds: only the
/// last one is kept, and each asks for a repaint.
fn picture_sink(mailbox: &Mailbox, egui: &Context) -> gst_app::AppSink {
    let caps = gst_video::VideoCapsBuilder::new()
        .format(gst_video::VideoFormat::Rgba)
        .build();
    let (preroll_box, preroll_egui) = (mailbox.clone(), egui.clone());
    let (sample_box, sample_egui) = (mailbox.clone(), egui.clone());
    let callbacks = gst_app::app_sink::AppSinkCallbacks::builder()
        // The first picture while paused, and the one after a seek.
        .new_preroll(move |sink| {
            let sample = sink.pull_preroll().map_err(|_| gst::FlowError::Eos)?;
            deliver(&sample, &preroll_box, &preroll_egui);
            Ok(gst::FlowSuccess::Ok)
        })
        .new_sample(move |sink| {
            let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
            deliver(&sample, &sample_box, &sample_egui);
            Ok(gst::FlowSuccess::Ok)
        })
        .build();
    gst_app::AppSink::builder()
        .caps(&caps)
        .max_buffers(1)
        .drop(true)
        .callbacks(callbacks)
        .build()
}

fn deliver(sample: &gst::Sample, mailbox: &Mailbox, egui: &Context) {
    if let Some(frame) = frame_of(sample) {
        *lock(mailbox) = Some(frame);
        egui.request_repaint();
    }
}

/// A sample as a picture: `None` when it holds none, which shows as the picture
/// before it staying.
fn frame_of(sample: &gst::Sample) -> Option<VideoFrame> {
    let info = gst_video::VideoInfo::from_caps(sample.caps()?).ok()?;
    let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(sample.buffer()?, &info).ok()?;
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    let stride = usize::try_from(*frame.plane_stride().first()?).ok()?;
    let image = rgba_image(frame.plane_data(0).ok()?, width, height, stride)?;
    let par = info.par();
    let pixel = if par.numer() > 0 && par.denom() > 0 {
        par.numer() as f32 / par.denom() as f32
    } else {
        1.0
    };
    Some(VideoFrame {
        image: Arc::new(image),
        aspect: width as f32 * pixel / height as f32,
    })
}

/// What a failure is, in words the error page can use.
fn describe(error: &gst::glib::Error) -> String {
    use gst::{CoreError as C, ResourceError as R, StreamError as S};
    let text = error.message();
    if error.matches(C::MissingPlugin) || error.matches(S::CodecNotFound) {
        format!("GStreamer lacks a plugin for this file: {text}")
    } else if error.matches(S::TypeNotFound)
        || error.matches(S::WrongType)
        || error.matches(S::Format)
        || error.matches(S::Demux)
        || error.matches(S::Decode)
    {
        format!("this file cannot be read as a video: {text}")
    } else if error.matches(R::NotFound)
        || error.matches(R::OpenRead)
        || error.matches(R::Read)
        || error.matches(R::NotAuthorized)
    {
        format!("this file cannot be read: {text}")
    } else {
        text.to_string()
    }
}

/// A file being played.
struct Player {
    playbin: gst::Element,
    bus: gst::Bus,
    mailbox: Mailbox,
    /// What the user last asked for.
    playing: bool,
    /// The first picture is through: it can seek and say where it is.
    ready: bool,
    ended: bool,
    /// A position asked for before it could seek.
    pending_seek: Option<Duration>,
    note: Option<String>,
}

impl Player {
    fn seek_now(&self, to: Duration) -> Result<(), String> {
        let nanos = u64::try_from(to.as_nanos()).unwrap_or(i64::MAX as u64);
        let flags = gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE;
        self.playbin
            .seek_simple(
                flags,
                gst::ClockTime::from_nseconds(nanos.min(i64::MAX as u64)),
            )
            .map_err(|e| format!("GStreamer could not seek: {e}"))
    }

    fn set_state(&self, state: gst::State) -> Result<(), String> {
        self.playbin
            .set_state(state)
            .map(|_| ())
            .map_err(|e| format!("GStreamer could not change state: {e}"))
    }
}

impl VideoPlayer for Player {
    fn poll(&mut self) -> Result<VideoStatus, String> {
        let bus = self.bus.clone();
        for message in bus.iter() {
            match message.view() {
                gst::MessageView::Error(e) => return Err(describe(&e.error())),
                gst::MessageView::Warning(w) => self.note = Some(w.error().message().to_string()),
                gst::MessageView::Eos(_) => {
                    self.ended = true;
                    self.playing = false;
                    // A seek or Play from there needs it paused, not running
                    // on nothing.
                    self.set_state(gst::State::Paused)?;
                }
                gst::MessageView::AsyncDone(_) => {
                    self.ready = true;
                    if let Some(to) = self.pending_seek.take() {
                        self.seek_now(to)?;
                    }
                }
                _ => {}
            }
        }
        let state = if self.ended {
            VideoState::Ended
        } else if !self.ready {
            VideoState::Loading
        } else if self.playing {
            VideoState::Playing
        } else {
            VideoState::Paused
        };
        let time = |t: gst::ClockTime| Duration::from_nanos(t.nseconds());
        Ok(VideoStatus {
            state,
            position: self.playbin.query_position::<gst::ClockTime>().map(time),
            duration: self.playbin.query_duration::<gst::ClockTime>().map(time),
            note: self.note.clone(),
        })
    }

    fn frame(&mut self) -> Option<VideoFrame> {
        lock(&self.mailbox).take()
    }

    fn play(&mut self) -> Result<(), String> {
        if std::mem::take(&mut self.ended) {
            self.seek_now(Duration::ZERO)?;
        }
        self.playing = true;
        self.set_state(gst::State::Playing)
    }

    fn pause(&mut self) -> Result<(), String> {
        self.playing = false;
        self.set_state(gst::State::Paused)
    }

    fn seek(&mut self, to: Duration) -> Result<(), String> {
        self.ended = false;
        if self.ready {
            self.seek_now(to)
        } else {
            self.pending_seek = Some(to);
            Ok(())
        }
    }

    fn set_volume(&mut self, volume: f32) {
        self.playbin
            .set_property("volume", f64::from(volume.clamp(0.0, 1.0)));
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.bus.unset_sync_handler();
        // Going to Null waits for GStreamer's threads: not on the interface's.
        let playbin = self.playbin.clone();
        let stopping = std::thread::Builder::new()
            .name("egui-pop-video-stop".into())
            .spawn(move || {
                let _ = playbin.set_state(gst::State::Null);
            });
        if stopping.is_err() {
            let _ = self.playbin.set_state(gst::State::Null);
        }
    }
}

/// Writes a small video with sound to a file (raw pictures and samples in an
/// AVI, so decoding needs no codec), with GStreamer's test sources: what the
/// tests and the gallery play, so no binary lives in the repository. It waits
/// until the file is written, on the caller's thread.
pub fn write_test_clip(path: &Path, seconds: u32) -> Result<(), String> {
    gst::init().map_err(|e| format!("GStreamer did not start: {e}"))?;
    let location = path
        .to_str()
        .ok_or("this path cannot be given to GStreamer")?;
    // Ten buffers a second on both sides.
    let buffers = seconds.saturating_mul(10);
    let description = format!(
        "videotestsrc num-buffers={buffers} pattern=smpte \
         ! video/x-raw,format=I420,width=160,height=120,framerate=10/1 \
         ! avimux name=mux ! filesink name=out \
         audiotestsrc num-buffers={buffers} samplesperbuffer=4410 \
         ! audio/x-raw,format=S16LE,rate=44100,channels=1 ! mux."
    );
    let pipeline =
        gst::parse::launch(&description).map_err(|e| format!("GStreamer's plugins: {e}"))?;
    let bin = pipeline
        .clone()
        .downcast::<gst::Bin>()
        .map_err(|_| "the clip's pipeline is not a bin")?;
    bin.by_name("out")
        .ok_or("the clip's pipeline has no file sink")?
        .set_property("location", location);
    let bus = pipeline.bus().ok_or("the clip's pipeline has no bus")?;
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|e| format!("the clip's pipeline did not start: {e}"))?;
    let end = bus.timed_pop_filtered(
        CLIP_DEADLINE,
        &[gst::MessageType::Eos, gst::MessageType::Error],
    );
    let _ = pipeline.set_state(gst::State::Null);
    match end {
        Some(message) => match message.view() {
            gst::MessageView::Error(e) => Err(describe(&e.error())),
            _ => Ok(()),
        },
        None => Err("the clip took too long to write".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_says_what_kind_it_is() {
        let says = |error: gst::glib::Error| describe(&error);
        assert!(
            says(gst::glib::Error::new(
                gst::StreamError::CodecNotFound,
                "no h265"
            ))
            .starts_with("GStreamer lacks a plugin")
        );
        assert!(
            says(gst::glib::Error::new(gst::CoreError::MissingPlugin, "none"))
                .starts_with("GStreamer lacks a plugin")
        );
        assert!(
            says(gst::glib::Error::new(
                gst::StreamError::TypeNotFound,
                "what"
            ))
            .starts_with("this file cannot be read as a video")
        );
        assert!(
            says(gst::glib::Error::new(
                gst::ResourceError::NotAuthorized,
                "no"
            ))
            .starts_with("this file cannot be read:")
        );
        assert_eq!(
            says(gst::glib::Error::new(gst::CoreError::Clock, "as it is")),
            "as it is"
        );
    }
}
