//! [`TimelinePlugin`]: the `timeline.*` commands and the `timeline`
//! collector, the agents' way to the [`Timeline`](super::Timeline) widgets
//! given an id; and [`Timelines`], the context's registry of them.
//!
//! | Command | Arguments | Does |
//! |---|---|---|
//! | `timeline.set_time` | `timeline`, `seconds`, `time` (text in the timeline's scale) or `by` | moves the playhead, kept inside the bounds |
//! | `timeline.set_range` | `timeline`, `start` and `end` (seconds), or `clear`; `loop` | sets or clears the range, and whether playback loops round it |
//! | `timeline.add_marker` | `timeline`, `seconds` or `time` (else the playhead), `label`, `kind` (`marker`, `cue`), `color` (`#rrggbb`) | adds a marker; answers its `id` |
//! | `timeline.remove_marker` | `timeline`, `id` | removes a marker |
//! | `timeline.play` | `timeline`, `speed` (1 by default, negative backwards) | asks to play |
//! | `timeline.pause` | `timeline` | asks to pause (K) |
//! | `timeline.toggle_play` | `timeline` | plays forward at 1, or pauses (Space) |
//! | `timeline.shuttle_forward`, `timeline.shuttle_backward` | `timeline` | plays that way, twice as fast again (L, J) |
//! | `timeline.step_forward`, `timeline.step_backward` | `timeline`, `big` | one unit on or back, pausing (Right, Left; `big`: Shift's step) |
//! | `timeline.go_to_start`, `timeline.go_to_end` | `timeline` | the bounds' start or end (Home, End) |
//! | `timeline.toggle_loop` | `timeline` | loops the range, or no longer (Ctrl+L) |
//! | `timeline.add_clip` | `timeline`, `track`, `seconds` or `time` (else the playhead), `length`, `offset`, `label`, `source`, `source_length`, `color` | adds a clip to a track; answers its `id` |
//! | `timeline.move_clip` | `timeline`, `id`, `seconds` or `time`, `track` | moves a clip along its track or to another |
//! | `timeline.trim_clip` | `timeline`, `id`, `start`, `end` (seconds) | moves a clip's start or end |
//! | `timeline.remove_clip` | `timeline`, `id` or `ids` | removes clips |
//! | `timeline.split_clip` | `timeline`, `id` or `ids`, `seconds` or `time` (else the playhead) | splits clips in two there; answers the new ones' ids |
//! | `timeline.ripple_delete` | `timeline`, `id` or `ids` | removes clips and closes their gaps |
//! | `timeline.select_clips` | `timeline`, `ids` | selects clips (an empty list: none) |
//!
//! `timeline` is a widget's id; without it, the timeline that last had the
//! focus, else the only one drawn with an id. A command is checked and
//! answered at once, with what the timeline then holds as the collector
//! shows it; the widget applies it when next drawn, as a step of the history
//! made by the command's origin.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui::{Color32, Context};
use egui_pop_api::{Call, Ctx, Plugin, PluginError};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_core::state::Slot;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, PluginId};
use serde_json::{Value as Json, json};

use super::state::{Marker, MarkerKind, TimeRange, TimelineState};
use super::tracks::{Clip, ClipEdge, ClipMove};
use super::{Op, apply};

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.timeline");

pub const CMD_SET_TIME: CommandId = CommandId::from_static("timeline.set_time");
pub const CMD_SET_RANGE: CommandId = CommandId::from_static("timeline.set_range");
pub const CMD_ADD_MARKER: CommandId = CommandId::from_static("timeline.add_marker");
pub const CMD_REMOVE_MARKER: CommandId = CommandId::from_static("timeline.remove_marker");
pub const CMD_PLAY: CommandId = CommandId::from_static("timeline.play");
pub const CMD_PAUSE: CommandId = CommandId::from_static("timeline.pause");
pub const CMD_ADD_CLIP: CommandId = CommandId::from_static("timeline.add_clip");
pub const CMD_MOVE_CLIP: CommandId = CommandId::from_static("timeline.move_clip");
pub const CMD_TRIM_CLIP: CommandId = CommandId::from_static("timeline.trim_clip");
pub const CMD_REMOVE_CLIP: CommandId = CommandId::from_static("timeline.remove_clip");
pub const CMD_SELECT_CLIPS: CommandId = CommandId::from_static("timeline.select_clips");
pub const CMD_SPLIT_CLIP: CommandId = CommandId::from_static("timeline.split_clip");
pub const CMD_RIPPLE_DELETE: CommandId = CommandId::from_static("timeline.ripple_delete");
pub const CMD_TOGGLE_PLAY: CommandId = CommandId::from_static("timeline.toggle_play");
pub const CMD_SHUTTLE_FORWARD: CommandId = CommandId::from_static("timeline.shuttle_forward");
pub const CMD_SHUTTLE_BACKWARD: CommandId = CommandId::from_static("timeline.shuttle_backward");
pub const CMD_STEP_FORWARD: CommandId = CommandId::from_static("timeline.step_forward");
pub const CMD_STEP_BACKWARD: CommandId = CommandId::from_static("timeline.step_backward");
pub const CMD_GO_TO_START: CommandId = CommandId::from_static("timeline.go_to_start");
pub const CMD_GO_TO_END: CommandId = CommandId::from_static("timeline.go_to_end");
pub const CMD_TOGGLE_LOOP: CommandId = CommandId::from_static("timeline.toggle_loop");

/// Passes a timeline may go undrawn before agents no longer see it.
const GONE_AFTER: u64 = 3;

/// Declares the `timeline.*` commands and the `timeline` collector. Connect
/// it when an application's timelines should be read and driven by agents;
/// the widgets work without it.
#[derive(Debug, Default)]
pub struct TimelinePlugin;

impl Plugin for TimelinePlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Timeline".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        commands(ctx)?;
        let egui = ctx.app().egui();
        ctx.collector(
            FnCollector::new(
                "timeline",
                "The timelines drawn with an id: their scale, view (start, duration, end), bounds, playhead (seconds and text), range and whether it loops, markers (id, time, text, label, kind, colour), the selected marker, the tracks (id, name, kind) and their clips (id, start, end, length, offset, text, label, source, source length, colour), the clips selected, the play state and speed asked; the one that last had the focus.",
            )
            .snapshot(move |_| Timelines::of(&egui).describe()),
        )?;
        Ok(())
    }
}

/// One timeline drawn with an id.
#[derive(Debug)]
struct Entry {
    /// As it was last drawn, with the commands since applied.
    state: TimelineState,
    pass: u64,
    /// Commands waiting for the widget, with who ran them.
    queue: Vec<(Op, Origin)>,
}

#[derive(Debug, Default)]
struct Registry {
    entries: BTreeMap<String, Entry>,
    focused: Option<String>,
}

#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Registry>>);

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        lock(&self.0).fmt(f)
    }
}

const SLOT: Slot<Shared> = Slot::new("views.timeline");

/// The timelines drawn in a context, by id: what the `timeline.*` commands
/// and the `timeline` collector reach.
#[derive(Clone)]
pub struct Timelines {
    ctx: Context,
    registry: Arc<Mutex<Registry>>,
}

impl std::fmt::Debug for Timelines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        lock(&self.registry).fmt(f)
    }
}

impl Timelines {
    /// The context's timelines.
    pub fn of(ctx: &Context) -> Self {
        Self {
            ctx: ctx.clone(),
            registry: SLOT.get_or_insert_with(ctx, Shared::default).0,
        }
    }

    fn live(&self, pass: u64) -> impl Fn(&Entry) -> bool {
        move |e: &Entry| e.pass + GONE_AFTER >= pass
    }

    /// The ids of the timelines drawn lately.
    pub fn ids(&self) -> Vec<String> {
        let pass = self.ctx.cumulative_pass_nr();
        let live = self.live(pass);
        lock(&self.registry)
            .entries
            .iter()
            .filter(|(_, e)| live(e))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// What every timeline drawn lately holds, and the one that last had the
    /// focus.
    pub fn describe(&self) -> Json {
        let pass = self.ctx.cumulative_pass_nr();
        let live = self.live(pass);
        let mut reg = lock(&self.registry);
        reg.entries.retain(|_, e| live(e));
        let timelines: serde_json::Map<String, Json> = reg
            .entries
            .iter()
            .map(|(id, e)| (id.clone(), describe(&e.state)))
            .collect();
        json!({ "timelines": timelines, "focused": reg.focused })
    }

    /// One timeline as the collector shows it.
    pub fn get(&self, timeline: Option<&str>) -> Result<Json, String> {
        let pass = self.ctx.cumulative_pass_nr();
        let mut reg = lock(&self.registry);
        let (_, e) = target(&mut reg, timeline, pass)?;
        Ok(describe(&e.state))
    }

    /// Records a timeline drawn this pass (and whether it has the focus);
    /// gives the commands waiting for it.
    pub(super) fn drawn(&self, id: &str, state: &TimelineState) -> Vec<(Op, Origin)> {
        let pass = self.ctx.cumulative_pass_nr();
        let mut reg = lock(&self.registry);
        let e = reg.entries.entry(id.to_string()).or_insert_with(|| Entry {
            state: state.clone(),
            pass,
            queue: Vec::new(),
        });
        e.pass = pass;
        let queue = std::mem::take(&mut e.queue);
        // What the commands asked shows until the widget applies it.
        if queue.is_empty() {
            e.state.clone_from(state);
        }
        queue
    }

    /// The widget's state at the end of its pass, and whether it has the
    /// focus.
    pub(super) fn update(&self, id: &str, state: &TimelineState, focused: bool) {
        let mut reg = lock(&self.registry);
        if focused {
            reg.focused = Some(id.to_string());
        }
        if let Some(e) = reg.entries.get_mut(id) {
            e.state.clone_from(state);
        }
    }

    /// Checks `op` on a timeline's last state, queues it for the widget and
    /// answers what the timeline then holds.
    pub(super) fn run(
        &self,
        timeline: Option<&str>,
        op: Op,
        origin: &Origin,
    ) -> Result<Json, String> {
        let pass = self.ctx.cumulative_pass_nr();
        let answer = {
            let mut reg = lock(&self.registry);
            let (_, e) = target(&mut reg, timeline, pass)?;
            let op = match op {
                // The id it will take, said in the answer.
                Op::AddMarker(mut m) => {
                    m.id = e.state.next_id();
                    Op::AddMarker(m)
                }
                Op::AddClip { track, mut clip } => {
                    clip.id = e.state.next_clip_id();
                    Op::AddClip { track, clip }
                }
                Op::RemoveMarker(id) if e.state.marker(id).is_none() => {
                    return Err(format!("no marker {id}"));
                }
                op => op,
            };
            let added = match &op {
                Op::AddMarker(m) => Some(m.id),
                Op::AddClip { clip, .. } => Some(clip.id),
                _ => None,
            };
            // Checked on the state the widget will have: a refusal queues
            // nothing.
            let mut tried = e.state.clone();
            let applied = apply(&mut tried, op.clone());
            if let Some(error) = applied.error {
                return Err(error);
            }
            e.state = tried;
            e.queue.push((op, origin.clone()));
            let mut answer = describe(&e.state);
            if let Some(id) = added {
                answer["added"] = json!(id);
            }
            let split: Vec<u64> = applied
                .edits
                .iter()
                .filter_map(|e| match e {
                    super::Edit::ClipSplit { new, .. } => Some(*new),
                    _ => None,
                })
                .collect();
            if !split.is_empty() {
                answer["added"] = json!(split);
            }
            answer
        };
        self.ctx.request_repaint();
        Ok(answer)
    }
}

/// The timeline a command names: `timeline`, else the one that last had the
/// focus, else the only one drawn.
fn target<'r>(
    reg: &'r mut Registry,
    timeline: Option<&str>,
    pass: u64,
) -> Result<(String, &'r mut Entry), String> {
    let live = |e: &Entry| e.pass + GONE_AFTER >= pass;
    let name = match timeline {
        Some(t) => t.to_string(),
        None => match reg
            .focused
            .clone()
            .filter(|f| reg.entries.get(f).is_some_and(live))
        {
            Some(f) => f,
            None => {
                let mut names = reg.entries.iter().filter(|(_, e)| live(e)).map(|(n, _)| n);
                match (names.next(), names.next()) {
                    (Some(only), None) => only.clone(),
                    (None, _) => return Err("no timeline with an id is drawn".into()),
                    _ => {
                        return Err(
                            "several timelines have an id and none had the focus; name one with `timeline`"
                                .into(),
                        );
                    }
                }
            }
        },
    };
    match reg.entries.get_mut(&name) {
        Some(e) if live(e) => Ok((name, e)),
        _ => Err(format!("no timeline `{name}`")),
    }
}

/// `#rrggbb`.
fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

fn parse_hex(text: &str) -> Option<Color32> {
    let h = text.trim().strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// A timeline as agents read it.
pub fn describe(s: &TimelineState) -> Json {
    let text = |t: f64| s.scale.format(t);
    let scale = match s.scale {
        super::TimeScale::Seconds => json!({ "kind": "seconds" }),
        super::TimeScale::Frames(r) => {
            json!({ "kind": "frames", "fps": r.fps(), "rate": [r.num, r.den] })
        }
        super::TimeScale::Beats(t) => json!({
            "kind": "beats",
            "bpm": t.bpm,
            "signature": [t.beats_per_bar, t.beat_unit],
            "offset": t.offset,
        }),
    };
    json!({
        "scale": scale,
        "view": { "start": s.view.start, "duration": s.view.duration, "end": s.view.end() },
        "bounds": s.bounds.map(|b| json!({ "start": b.start, "end": b.end })),
        "playhead": { "seconds": s.playhead, "text": text(s.playhead) },
        "range": s.range.map(|r| json!({
            "start": r.start,
            "end": r.end,
            "text": [text(r.start), text(r.end)],
        })),
        "looping": s.looping,
        "markers": s.markers.iter().map(|m| json!({
            "id": m.id,
            "seconds": m.time,
            "text": text(m.time),
            "label": m.label,
            "kind": m.kind,
            "color": m.color.map(hex),
        })).collect::<Vec<_>>(),
        "selected": s.selected,
        "tracks": s.tracks.iter().map(|t| json!({
            "id": t.id,
            "name": t.name,
            "kind": t.kind,
            "clips": t.clips.iter().map(|c| json!({
                "id": c.id,
                "seconds": c.start,
                "end": c.end(),
                "length": c.length,
                "offset": c.offset,
                "text": [text(c.start), text(c.end())],
                "label": c.label,
                "source": c.source,
                "source_length": c.source_length,
                "color": c.color.map(hex),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "selected_clips": s.selected_clips,
        "play": s.play,
        "speed": s.speed,
    })
}

fn timelines(call: &Call<'_>) -> Timelines {
    Timelines::of(&call.app.egui())
}

/// The `timeline` argument.
fn name(args: &Json) -> Option<&str> {
    args.get("timeline").and_then(Json::as_str)
}

/// A time from `seconds` (a number) or `time` (text in the scale), on the
/// timeline `name`.
fn time_of(t: &Timelines, args: &Json) -> Result<Option<f64>, String> {
    if let Some(s) = args.get("seconds") {
        return s
            .as_f64()
            .map(Some)
            .ok_or_else(|| "`seconds` is a number".into());
    }
    let Some(text) = args.get("time") else {
        return Ok(None);
    };
    let text = text.as_str().ok_or("`time` is text")?;
    scale_of(t, name(args))?
        .parse(text)
        .map(Some)
        .ok_or_else(|| format!("`{text}` is not a time on this timeline"))
}

fn scale_of(t: &Timelines, timeline: Option<&str>) -> Result<super::TimeScale, String> {
    let pass = t.ctx.cumulative_pass_nr();
    let mut reg = lock(&t.registry);
    target(&mut reg, timeline, pass).map(|(_, e)| e.state.scale)
}

/// A transport key's command: its id, title, description and op (given
/// `big`).
type TransportCommand = (CommandId, &'static str, &'static str, fn(bool) -> Op);

fn commands(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    const CAT: &str = "Timeline";
    let timeline = "song (optional: the timeline that last had the focus)";
    ctx.command(
        CommandSpec::new(CMD_SET_TIME, "Move a timeline's playhead", CAT)
            .description("Moves a timeline's playhead (`timeline`, its id) to `seconds`, to `time` (text in the timeline's scale: `1:02.5`, `00:01:02:12`, `17.2.480`), or by `by` seconds; kept inside its bounds.")
            .args(json!({ "timeline": timeline, "seconds": 12.5 })),
        |call| {
            let t = timelines(call);
            let to = match (time_of(&t, call.args)?, call.args.get("by").and_then(Json::as_f64)) {
                (Some(s), _) => Op::SetTime(s),
                (None, Some(by)) => Op::StepBy(by),
                (None, None) => return Err("`seconds`, `time` or `by` is needed".into()),
            };
            t.run(name(call.args), to, call.origin)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_SET_RANGE, "Set a timeline's range", CAT)
            .description("Sets a timeline's range from `start` to `end` (seconds), or clears it (`clear`: true); `loop` says whether playback goes round it. One step of the history.")
            .args(json!({ "timeline": timeline, "start": 4.0, "end": 8.0, "loop": true })),
        |call| {
            let t = timelines(call);
            let args = call.args;
            let tl = name(args);
            let clear = args.get("clear").and_then(Json::as_bool).unwrap_or(false);
            let start = args.get("start").and_then(Json::as_f64);
            let end = args.get("end").and_then(Json::as_f64);
            let looping = args.get("loop").and_then(Json::as_bool);
            let range = match (clear, start, end) {
                (true, _, _) => Some(None),
                (false, Some(a), Some(b)) => Some(Some(TimeRange::new(a, b))),
                (false, None, None) => None,
                _ => return Err("`start` and `end` go together".into()),
            };
            if range.is_none() && looping.is_none() {
                return Err("`start` and `end`, `clear` or `loop` is needed".into());
            }
            let mut answer = Json::Null;
            if let Some(r) = range {
                answer = t.run(tl, Op::SetRange(r), call.origin)?;
            }
            if let Some(l) = looping {
                answer = t.run(tl, Op::SetLoop(l), call.origin)?;
            }
            Ok(answer)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_ADD_MARKER, "Add a marker to a timeline", CAT)
            .description("Adds a marker to a timeline at `seconds` or `time` (else at the playhead), named `label`, of `kind` `marker` or `cue`, in `color` (`#rrggbb`, else the theme's). Answers its `id` in `added`. One step of the history.")
            .args(json!({ "timeline": timeline, "seconds": 30.0, "label": "Drop", "kind": "cue", "color": "#ff8800" })),
        |call| {
            let t = timelines(call);
            let args = call.args;
            let at = match time_of(&t, args)? {
                Some(s) => s,
                None => t.get(name(args))?["playhead"]["seconds"].as_f64().unwrap_or(0.0),
            };
            let label = args.get("label").and_then(Json::as_str).unwrap_or("").to_string();
            let kind = match args.get("kind").and_then(Json::as_str) {
                None | Some("marker") => MarkerKind::Marker,
                Some("cue") => MarkerKind::Cue,
                Some(other) => return Err(format!("`kind` is `marker` or `cue`, not `{other}`")),
            };
            let mut m = Marker::new(0, at, label).kind(kind);
            if let Some(c) = args.get("color").and_then(Json::as_str) {
                m.color = Some(parse_hex(c).ok_or_else(|| format!("`{c}` is not `#rrggbb`"))?);
            }
            t.run(name(args), Op::AddMarker(m), call.origin)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_REMOVE_MARKER, "Remove a marker from a timeline", CAT)
            .description("Removes a timeline's marker `id`. One step of the history.")
            .args(json!({ "timeline": timeline, "id": 1 })),
        |call| {
            let id = call
                .args
                .get("id")
                .and_then(Json::as_u64)
                .ok_or("`id` is a marker's id")?;
            timelines(call).run(name(call.args), Op::RemoveMarker(id), call.origin)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_PLAY, "Play a timeline", CAT)
            .description("Asks a timeline to play at `speed` (1 by default, negative backwards); the application moves the time.")
            .args(json!({ "timeline": timeline, "speed": 1.0 })),
        |call| {
            let speed = call.args.get("speed").and_then(Json::as_f64).unwrap_or(1.0);
            timelines(call).run(name(call.args), Op::Play(speed), call.origin)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_PAUSE, "Pause a timeline", CAT)
            .description("Asks a timeline to pause.")
            .args(json!({ "timeline": timeline })),
        |call| timelines(call).run(name(call.args), Op::Pause, call.origin),
    )?;
    // The transport's keys, one command each (a `Transport`'s buttons run
    // them): what they do is the key's, applied at the timeline's pass.
    let transport: [TransportCommand; 7] = [
        (
            CMD_TOGGLE_PLAY,
            "Play or pause a timeline",
            "Asks a timeline to play forward at 1, or to pause when it plays, as Space does.",
            |_| Op::Toggle,
        ),
        (
            CMD_SHUTTLE_FORWARD,
            "Shuttle a timeline forward",
            "Asks a timeline to play forward at 1, or twice as fast when it already does (up to 32×), as L does.",
            |_| Op::Shuttle(true),
        ),
        (
            CMD_SHUTTLE_BACKWARD,
            "Shuttle a timeline backward",
            "Asks a timeline to play backward at 1, or twice as fast when it already does, as J does.",
            |_| Op::Shuttle(false),
        ),
        (
            CMD_STEP_FORWARD,
            "Step a timeline forward",
            "Moves a timeline's playhead one unit on (a frame, a beat, a second), or Shift's bigger step with `big`, and pauses, as Right does.",
            |big| Op::Step { forward: true, big },
        ),
        (
            CMD_STEP_BACKWARD,
            "Step a timeline backward",
            "Moves a timeline's playhead one unit back, or Shift's bigger step with `big`, and pauses, as Left does.",
            |big| Op::Step {
                forward: false,
                big,
            },
        ),
        (
            CMD_GO_TO_START,
            "Go to a timeline's start",
            "Moves a timeline's playhead to its bounds' start, as Home does.",
            |_| Op::Home,
        ),
        (
            CMD_GO_TO_END,
            "Go to a timeline's end",
            "Moves a timeline's playhead to its bounds' end, as End does.",
            |_| Op::End,
        ),
    ];
    for (id, title, description, op) in transport {
        let args = if matches!(op(false), Op::Step { .. }) {
            json!({ "timeline": timeline, "big": false })
        } else {
            json!({ "timeline": timeline })
        };
        ctx.command(
            CommandSpec::new(id, title, CAT)
                .description(description)
                .args(args),
            move |call| {
                let big = call
                    .args
                    .get("big")
                    .and_then(Json::as_bool)
                    .unwrap_or(false);
                timelines(call).run(name(call.args), op(big), call.origin)
            },
        )?;
    }
    ctx.command(
        CommandSpec::new(CMD_TOGGLE_LOOP, "Loop a timeline's range or not", CAT)
            .description("Turns a timeline's looping round its range on, or off, as Ctrl+L does. One step of the history.")
            .args(json!({ "timeline": timeline })),
        |call| timelines(call).run(name(call.args), Op::ToggleLoop, call.origin),
    )?;
    ctx.command(
        CommandSpec::new(CMD_ADD_CLIP, "Add a clip to a timeline", CAT)
            .description("Adds a clip to a timeline's track `track` (its id), starting at `seconds` or `time` (else at the playhead), `length` seconds long, reading its source from `offset` seconds in; `label`, `source` (what it plays, for the application), `source_length` (trimming stops at the source's ends) and `color` (`#rrggbb`) are optional. Refused where it would overlap a clip. Answers its `id` in `added`. One step of the history.")
            .args(json!({ "timeline": timeline, "track": 1, "seconds": 4.0, "length": 2.5, "label": "Take 2", "source": "take2.mov" })),
        |call| {
            let t = timelines(call);
            let args = call.args;
            let track = args.get("track").and_then(Json::as_u64).ok_or("`track` is a track's id")?;
            let at = match time_of(&t, args)? {
                Some(s) => s,
                None => t.get(name(args))?["playhead"]["seconds"].as_f64().unwrap_or(0.0),
            };
            let length = args.get("length").and_then(Json::as_f64).ok_or("`length` is a number of seconds")?;
            let text = |key: &str| args.get(key).and_then(Json::as_str).unwrap_or("").to_string();
            let mut clip = Clip::new(0, at, length, text("label"))
                .offset(args.get("offset").and_then(Json::as_f64).unwrap_or(0.0))
                .source(text("source"));
            clip.source_length = args.get("source_length").and_then(Json::as_f64);
            if let Some(c) = args.get("color").and_then(Json::as_str) {
                clip.color = Some(parse_hex(c).ok_or_else(|| format!("`{c}` is not `#rrggbb`"))?);
            }
            t.run(name(args), Op::AddClip { track, clip }, call.origin)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_MOVE_CLIP, "Move a clip on a timeline", CAT)
            .description("Moves a timeline's clip `id` to start at `seconds` or `time`, and to track `track` (its id); what is not given stays. Refused where it would overlap a clip or leave the bounds. One step of the history.")
            .args(json!({ "timeline": timeline, "id": 1, "seconds": 8.0, "track": 2 })),
        |call| {
            let t = timelines(call);
            let args = call.args;
            let id = clip_id(args)?;
            let at = time_of(&t, args)?;
            let track = args.get("track").and_then(Json::as_u64);
            let clip = clip_of(&t, name(args), id)?;
            let mv = ClipMove {
                id,
                track: track.unwrap_or(clip.0),
                start: at.unwrap_or(clip.1),
            };
            t.run(name(args), Op::MoveClips(vec![mv]), call.origin)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_TRIM_CLIP, "Trim a clip on a timeline", CAT)
            .description("Moves a timeline's clip `id` start to `start` and its end to `end` (seconds on the timeline; either or both). Its offset into its source follows its start; it stops at its source's ends, its neighbours and the bounds. One step of the history each.")
            .args(json!({ "timeline": timeline, "id": 1, "start": 4.5, "end": 9.0 })),
        |call| {
            let t = timelines(call);
            let args = call.args;
            let id = clip_id(args)?;
            let start = args.get("start").and_then(Json::as_f64);
            let end = args.get("end").and_then(Json::as_f64);
            if start.is_none() && end.is_none() {
                return Err("`start` or `end` is needed".into());
            }
            let mut answer = Json::Null;
            for (edge, time) in [(ClipEdge::Start, start), (ClipEdge::End, end)] {
                if let Some(time) = time {
                    answer = t.run(name(args), Op::TrimClip { id, edge, time }, call.origin)?;
                }
            }
            Ok(answer)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_REMOVE_CLIP, "Remove clips from a timeline", CAT)
            .description("Removes a timeline's clip `id`, or the clips `ids` (one step of the history together).")
            .args(json!({ "timeline": timeline, "id": 1 })),
        |call| {
            let ids = clip_ids(call.args)?;
            timelines(call).run(name(call.args), Op::RemoveClips(ids), call.origin)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_SPLIT_CLIP, "Split clips on a timeline", CAT)
            .description("Splits a timeline's clip `id`, or the clips `ids`, at `seconds` or `time` (else at the playhead): each ends there and a new clip starts there, reading its source on. Refused unless the time is inside each, a frame or more from its ends. Answers the new clips' ids in `added`. One step of the history together.")
            .args(json!({ "timeline": timeline, "id": 1, "seconds": 6.0 })),
        |call| {
            let t = timelines(call);
            let args = call.args;
            let ids = clip_ids(args)?;
            let time = match time_of(&t, args)? {
                Some(time) => time,
                None => playhead_of(&t, name(args))?,
            };
            t.run(name(args), Op::SplitClips { ids, time }, call.origin)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_RIPPLE_DELETE, "Ripple delete clips on a timeline", CAT)
            .description("Removes a timeline's clip `id`, or the clips `ids`, and closes the gaps they leave: the clips after them on their tracks move back. One step of the history together.")
            .args(json!({ "timeline": timeline, "id": 1 })),
        |call| {
            let ids = clip_ids(call.args)?;
            timelines(call).run(name(call.args), Op::RippleRemove(ids), call.origin)
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_SELECT_CLIPS, "Select clips on a timeline", CAT)
            .description("Selects a timeline's clips `ids` (an empty list lets them go); the keys act on the last. Not a step of the history.")
            .args(json!({ "timeline": timeline, "ids": [1, 2] })),
        |call| {
            let ids = clip_ids(call.args)?;
            timelines(call).run(name(call.args), Op::SelectClips(ids), call.origin)
        },
    )?;
    Ok(())
}

/// The `id` argument, a clip's.
fn clip_id(args: &Json) -> Result<u64, String> {
    args.get("id")
        .and_then(Json::as_u64)
        .ok_or_else(|| "`id` is a clip's id".into())
}

/// `ids` (a list of clips' ids), else `id`.
fn clip_ids(args: &Json) -> Result<Vec<u64>, String> {
    match args.get("ids") {
        Some(Json::Array(ids)) => ids
            .iter()
            .map(|v| v.as_u64().ok_or_else(|| "`ids` are clips' ids".to_string()))
            .collect(),
        Some(_) => Err("`ids` is a list of clips' ids".into()),
        None => clip_id(args).map(|id| vec![id]),
    }
}

/// The playhead of the timeline `timeline`.
fn playhead_of(t: &Timelines, timeline: Option<&str>) -> Result<f64, String> {
    let pass = t.ctx.cumulative_pass_nr();
    let mut reg = lock(&t.registry);
    target(&mut reg, timeline, pass).map(|(_, e)| e.state.playhead)
}

/// Clip `id`'s track and start on the timeline `timeline`.
fn clip_of(t: &Timelines, timeline: Option<&str>, id: u64) -> Result<(u64, f64), String> {
    let pass = t.ctx.cumulative_pass_nr();
    let mut reg = lock(&t.registry);
    let (_, e) = target(&mut reg, timeline, pass)?;
    match (e.state.clip_track(id), e.state.clip(id)) {
        (Some(track), Some(c)) => Ok((track.id, c.start)),
        _ => Err(format!("no clip {id}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_read_and_write_as_hex() {
        assert_eq!(parse_hex("#ff8800"), Some(Color32::from_rgb(255, 136, 0)));
        assert_eq!(hex(Color32::from_rgb(255, 136, 0)), "#ff8800");
        assert_eq!(parse_hex("ff8800"), None);
        assert_eq!(parse_hex("#ff88"), None);
    }
}
