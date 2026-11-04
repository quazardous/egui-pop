//! [`VideoPlugin`]: the `video.*` commands and the `videos` collector, the
//! agents' way to the [`Video`](super::Video) widgets given an id.
//!
//! | Command | Arguments | Does |
//! |---|---|---|
//! | `video.toggle` | `video` | plays or pauses; from the end, plays from the start |
//! | `video.play`, `video.pause` | `video` | plays, pauses |
//! | `video.seek` | `video`, `seconds` or `by` | moves to a position, or by an amount (negative: back), kept within the video |
//! | `video.volume` | `video`, `volume` (0 to 1) or `by` | sets the volume, or changes it; gives the sound back |
//! | `video.mute` | `video`, `muted` (else switched) | mutes or gives the sound back |
//!
//! `video` is a widget's id; without it, the video whose picture last had the
//! focus, else the only one with an id. Each command answers with what the
//! video then holds, as the collector shows it.

use egui_pop_api::{Call, Ctx, Plugin, PluginError};
use egui_pop_core::debug::FnCollector;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::{Value as Json, json};

use super::Videos;

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.video");

pub const CMD_TOGGLE: CommandId = CommandId::from_static("video.toggle");
pub const CMD_PLAY: CommandId = CommandId::from_static("video.play");
pub const CMD_PAUSE: CommandId = CommandId::from_static("video.pause");
pub const CMD_SEEK: CommandId = CommandId::from_static("video.seek");
pub const CMD_VOLUME: CommandId = CommandId::from_static("video.volume");
pub const CMD_MUTE: CommandId = CommandId::from_static("video.mute");

/// Declares the `video.*` commands and the `videos` collector. Connect it
/// when an application's videos should be read and driven by agents; the
/// widgets play without it.
#[derive(Debug, Default)]
pub struct VideoPlugin;

impl Plugin for VideoPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Video".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        commands(ctx)?;
        let egui = ctx.app().egui();
        ctx.collector(
            FnCollector::new(
                "videos",
                "The videos drawn with an id: their file, backend, controls, state, position and length (ms), volume, whether they play, loop or are muted, the loops made, the picture's size, a note or an error; the one that last had the focus, and how many have no id.",
            )
            .snapshot(move |_| Videos::of(&egui).describe()),
        )?;
        Ok(())
    }
}

/// The `video` argument.
fn target(args: &Json) -> Option<&str> {
    args.get("video").and_then(Json::as_str)
}

fn videos(call: &Call<'_>) -> Videos {
    Videos::of(&call.app.egui())
}

fn commands(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    const CAT: &str = "Video";
    let video = "tour (optional: the video that last had the focus)";
    ctx.command(
        CommandSpec::new(CMD_TOGGLE, "Play or pause a video", CAT)
            .description(
                "Plays or pauses a video (`video`, its id); from its end, plays it from the start.",
            )
            .args(json!({ "video": video })),
        |call| videos(call).toggle(target(call.args)),
    )?;
    ctx.command(
        CommandSpec::new(CMD_PLAY, "Play a video", CAT)
            .description("Plays a video (`video`, its id).")
            .args(json!({ "video": video })),
        |call| videos(call).play(target(call.args)),
    )?;
    ctx.command(
        CommandSpec::new(CMD_PAUSE, "Pause a video", CAT)
            .description("Pauses a video (`video`, its id).")
            .args(json!({ "video": video })),
        |call| videos(call).pause(target(call.args)),
    )?;
    ctx.command(
        CommandSpec::new(CMD_SEEK, "Move a video to a time", CAT)
            .description("Moves a video (`video`) to a position (`seconds`), or by an amount (`by`, negative to go back), kept within it.")
            .args(json!({ "video": video, "seconds": 12.5 })),
        |call| {
            let v = videos(call);
            let video = target(call.args);
            match (
                call.args.get("seconds").and_then(Json::as_f64),
                call.args.get("by").and_then(Json::as_f64),
            ) {
                (Some(s), _) => v.seek_to(video, s),
                (None, Some(by)) => v.seek_by(video, by),
                (None, None) => Err("`seconds` or `by` is a number".into()),
            }
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_VOLUME, "Set a video's volume", CAT)
            .description("Sets a video's volume (`volume`, from 0 to 1), or changes it (`by`); either gives the sound back.")
            .args(json!({ "video": video, "volume": 0.5 })),
        |call| {
            let v = videos(call);
            let video = target(call.args);
            match (
                call.args.get("volume").and_then(Json::as_f64),
                call.args.get("by").and_then(Json::as_f64),
            ) {
                (Some(volume), _) => v.set_volume(video, volume as f32),
                (None, Some(by)) => v.volume_by(video, by as f32),
                (None, None) => Err("`volume` or `by` is a number".into()),
            }
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_MUTE, "Mute a video", CAT)
            .description("Mutes a video (`muted`: true) or gives its sound back (false); without `muted`, switches.")
            .args(json!({ "video": video, "muted": true })),
        |call| {
            let muted = call.args.get("muted").and_then(Json::as_bool);
            videos(call).set_muted(target(call.args), muted)
        },
    )?;
    Ok(())
}
