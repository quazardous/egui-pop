//! The text log page: a minute of a robot's log under a timeline sharing
//! its time. The rows at the playhead are highlighted (the kit's
//! `table-row`), a click on a row moves the playhead, the level chips and
//! the text filter narrow the lines, a multi-line body unfolds, and the
//! playground streams lines in (Follow keeps the tail in view) or swaps
//! in a hundred thousand lines.

use egui::Ui;
use egui_pop::core::widgets;
use egui_pop::views::text_log::{Level, LogEntry, TextLog, TextLogState};
use egui_pop::views::timeline::{TimeRange, TimeScale, Timeline, TimelineState};

use super::section;

/// The recording's length, in seconds.
pub const LENGTH: f64 = 60.0;

/// The page's state: the timeline the log follows, the log and its view.
#[derive(Debug)]
pub struct TextLogPageState {
    pub timeline: TimelineState,
    pub log: TextLogState,
    pub entries: Vec<LogEntry>,
    /// A line comes in at every pass.
    pub stream: bool,
    /// A hundred thousand lines instead of the minute's.
    pub large: bool,
    /// The line last picked, as said under the log.
    pub picked: Option<String>,
}

impl Default for TextLogPageState {
    fn default() -> Self {
        let mut timeline = TimelineState::new(TimeScale::Seconds, LENGTH);
        timeline.playhead = 21.0;
        Self {
            timeline,
            log: TextLogState::new(),
            entries: recording(600),
            stream: false,
            large: false,
            picked: None,
        }
    }
}

const ENTITIES: [&str; 5] = [
    "/robot",
    "/robot/arm",
    "/robot/gripper",
    "/camera",
    "/planner",
];

/// Line `i` of a log of `n` lines over the recording's minute: always the
/// same, in time order.
pub fn line(i: usize, n: usize) -> LogEntry {
    let time = i as f64 * LENGTH / n as f64;
    // Tenths of a second for a short log, its lines one each.
    let time = if n <= 1000 {
        (time * 10.0).round() / 10.0
    } else {
        time
    };
    let entity = ENTITIES[(i * 7 + i / 3) % ENTITIES.len()];
    let (level, body) = match i % 23 {
        0 | 9 | 15 => (
            Level::Debug,
            format!("joint error {:.3} rad", (i % 17) as f64 * 0.004),
        ),
        3 | 12 => (Level::Trace, "heartbeat".to_string()),
        6 => (Level::Warn, format!("battery at {} %", 80 - (i % 60))),
        18 if i % 4 == 2 => (
            Level::Error,
            format!("frame {i} dropped: exposure did not settle"),
        ),
        21 if i % 5 == 1 => (
            Level::Critical,
            format!(
                "emergency stop at line {i}\nreason: the gripper did not answer\nstate: arm halted, waiting for a reset"
            ),
        ),
        11 => (
            Level::Info,
            format!(
                "waypoint {} reached\nposition: 1.20, 0.45, 0.80\nheading: 92°",
                i / 23
            ),
        ),
        _ => (Level::Info, format!("moving to waypoint {}", i / 23 + 1)),
    };
    LogEntry::new(time, entity, level, body)
}

/// A log of `n` lines over the recording's minute.
pub fn recording(n: usize) -> Vec<LogEntry> {
    (0..n).map(|i| line(i, n)).collect()
}

pub fn show(ui: &mut Ui, s: &mut TextLogPageState) {
    ui.label("A text log over a timeline's time, after Rerun's text log view: the rows at the playhead are highlighted, a click on a row moves the playhead there. The chips show or hide a level, the field filters the bodies and entities, a multi-line body unfolds with its arrow. With Follow on, the last line stays in view as lines come in, until you scroll up; back at the end, it follows again.");
    if s.stream {
        let n = s.entries.len();
        let last = s.entries.last().map_or(0.0, |e| e.time);
        let mut entry = line(n, n.max(1));
        entry.time = last + 0.1;
        s.entries.push(entry);
        ui.ctx().request_repaint();
    }
    section(ui, "A robot's minute");
    Timeline::new(&mut s.timeline)
        .id("gallery.text-log")
        .label("Recording")
        .show(ui);
    let out = TextLog::new("gallery.text-log", &s.entries)
        .label("Robot log")
        .timeline(&mut s.timeline)
        .height(330.0)
        .show(ui, &mut s.log);
    if let Some(i) = out.picked {
        let e = &s.entries[i];
        s.picked = Some(format!("{} {}", e.level.name(), e.entity));
    }
    ui.label(format!(
        "Playhead {}",
        s.timeline.scale.format(s.timeline.playhead)
    ));
    if let Some(p) = &s.picked {
        ui.label(format!("Picked: {p}"));
    }
}

pub fn playground(ui: &mut Ui, s: &mut TextLogPageState) {
    let mut stream = s.stream;
    if widgets::checkbox(ui, &mut stream, "Stream lines in").changed() {
        s.stream = stream;
    }
    let mut large = s.large;
    if widgets::checkbox(ui, &mut large, "A hundred thousand lines").changed() {
        s.large = large;
        s.entries = recording(if large { 100_000 } else { 600 });
        s.log.refilter();
    }
    if s.stream {
        let end = s.entries.last().map_or(LENGTH, |e| e.time.max(LENGTH));
        s.timeline.bounds = Some(TimeRange::new(0.0, end));
    }
}
