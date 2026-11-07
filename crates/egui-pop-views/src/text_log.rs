//! A text log view on the data table: a line per entry, its time, entity,
//! level and body, filtered by level and by text, following the tail, and
//! tied to a time cursor.
//!
//! Its design follows the text log view of the Rerun viewer
//! (<https://rerun.io>, `re_view_text_log`, MIT OR Apache-2.0): the same
//! four columns, the level in its colour (`palette.log.*`), the rows at
//! the time cursor highlighted and brought into view when the cursor
//! moves, a click on a row moving the time cursor to it. No Rerun code is
//! used; the table is egui-pop's [`DataTable`] (on Rerun's `egui_table`).
//!
//! - **Columns**: the time, written in a timeline's scale
//!   ([`TextLog::time_scale`], or a bound timeline's), else as a plain
//!   number; the entity path; the level as a coloured chip; the body, its
//!   first line, a multi-line body expanding to all its lines.
//! - **Filters**: a toggle chip per level, each with its count, and a text
//!   filter matching the body or the entity path, case aside.
//! - **The tail**: with Follow on (the default), the last line stays in
//!   view as lines come in; scrolled up, the view stays where it was, and
//!   scrolled back to the end it follows again.
//! - **The time cursor**: the rows at the latest time not after the
//!   cursor ([`TextLog::time_cursor`], or a bound timeline's playhead,
//!   [`TextLog::timeline`]) are the table's selection, drawn by the kit's
//!   `table-row`; a click or the arrow keys move the cursor to a row's
//!   time ([`TextLogOutput::cursor_moved`]; a bound timeline's playhead is
//!   already there).
//! - **Large logs**: only the rows in view are built, and the filter keeps
//!   its result between passes, scanning only the entries added since
//!   (a log grows at its end; [`TextLogState::refilter`] after anything
//!   else changed).
//!
//! The application owns the entries ([`LogSource`], in time order) and
//! keeps a [`TextLogState`].
//!
//! ```
//! use egui_pop_views::text_log::{Level, LogEntry, TextLog, TextLogState};
//!
//! fn ui(ui: &mut egui::Ui, state: &mut TextLogState, log: &[LogEntry], now: &mut f64) {
//!     let out = TextLog::new("app.log", log)
//!         .label("Application log")
//!         .time_cursor(Some(*now))
//!         .show(ui, state);
//!     if let Some(t) = out.cursor_moved {
//!         *now = t;
//!     }
//! }
//!
//! let log = vec![
//!     LogEntry::new(0.0, "/robot", Level::Info, "started"),
//!     LogEntry::new(1.5, "/robot/arm", Level::Warn, "joint 2 near its limit"),
//! ];
//! # let _ = log;
//! ```

use std::collections::BTreeSet;
use std::ops::Range;

use egui::{
    Align, Color32, FontId, Layout, Rect, Response, RichText, Role, TextStyle, Ui, UiBuilder, pos2,
    vec2,
};
use egui_pop_api::AppHandle;
use egui_pop_core::a11y;
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_core::theme::{LogPalette, Tokens};
use egui_pop_core::widgets::ToggleChip;

use crate::list::Signal;
use crate::select::Mode;
use crate::strings as s;
use crate::table::{Column, DataTable, TableRows, TableState};
use crate::timeline::{TimeScale, TimelineState};

/// How severe a log line is, from trace to critical: the six levels of
/// the `palette.log.*` tokens, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Critical,
}

impl Level {
    /// Every level, from trace to critical.
    pub const ALL: [Level; 6] = [
        Level::Trace,
        Level::Debug,
        Level::Info,
        Level::Warn,
        Level::Error,
        Level::Critical,
    ];

    /// How the log writes it: `TRACE` to `CRITICAL`.
    pub fn name(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::Critical => "CRITICAL",
        }
    }

    /// The level a name gives (`warn`, `WARNING`, `err`...), case aside.
    pub fn parse(name: &str) -> Option<Level> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "trace" => Level::Trace,
            "debug" => Level::Debug,
            "info" => Level::Info,
            "warn" | "warning" => Level::Warn,
            "error" | "err" => Level::Error,
            "critical" | "crit" | "fatal" => Level::Critical,
            _ => return None,
        })
    }

    /// Its colour among the theme's (`palette.log.<level>`).
    pub fn color(self, palette: &LogPalette) -> Color32 {
        palette.levels()[self as usize]
    }
}

/// A log line, as an application may keep them ([`LogSource`] is
/// implemented for a slice of them).
#[derive(Clone, Debug, PartialEq)]
pub struct LogEntry {
    /// When it was logged, on the time cursor's axis (seconds on a
    /// timeline).
    pub time: f64,
    /// The entity it was logged on, a path (`/robot/arm`).
    pub entity: String,
    pub level: Level,
    /// Its text, perhaps on several lines.
    pub body: String,
}

impl LogEntry {
    pub fn new(
        time: f64,
        entity: impl Into<String>,
        level: Level,
        body: impl Into<String>,
    ) -> Self {
        Self {
            time,
            entity: entity.into(),
            level,
            body: body.into(),
        }
    }
}

/// The entries a text log shows, in time order (an entry is never before
/// the one ahead of it); the application owns them. Entries are named by
/// their index, which stays theirs as the log grows at its end.
pub trait LogSource {
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn time(&self, index: usize) -> f64;
    fn entity(&self, index: usize) -> &str;
    fn level(&self, index: usize) -> Level;
    fn body(&self, index: usize) -> &str;
}

impl LogSource for [LogEntry] {
    fn len(&self) -> usize {
        <[LogEntry]>::len(self)
    }

    fn time(&self, index: usize) -> f64 {
        self[index].time
    }

    fn entity(&self, index: usize) -> &str {
        &self[index].entity
    }

    fn level(&self, index: usize) -> Level {
        self[index].level
    }

    fn body(&self, index: usize) -> &str {
        &self[index].body
    }
}

impl LogSource for Vec<LogEntry> {
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    fn time(&self, index: usize) -> f64 {
        self[index].time
    }

    fn entity(&self, index: usize) -> &str {
        &self[index].entity
    }

    fn level(&self, index: usize) -> Level {
        self[index].level
    }

    fn body(&self, index: usize) -> &str {
        &self[index].body
    }
}

/// What the filter was computed for.
#[derive(Clone, Debug, PartialEq)]
struct FilterKey {
    levels: [bool; 6],
    /// The text filter, lower case.
    text: String,
}

/// A text log's state between passes: its filters, Follow, the bodies
/// expanded, the table's own state and the filter's result.
#[derive(Debug)]
pub struct TextLogState {
    table: TableState<usize>,
    levels: [bool; 6],
    text: String,
    follow: bool,
    expanded: BTreeSet<usize>,
    /// The entries shown, by index, in order.
    rows: Vec<usize>,
    /// How many entries the filter has seen.
    scanned: usize,
    /// Per level, the entries matching the text filter.
    counts: [usize; 6],
    key: Option<FilterKey>,
    /// The entry the cursor's rows put the table's cursor on.
    cursor_entry: Option<usize>,
    /// The rows at the time cursor at the last pass (entries).
    cursor_rows: Vec<usize>,
    /// This log moved the time cursor at the last pass: its rows are not
    /// brought into view (the user is looking at them).
    moved_here: bool,
    /// The end to be brought into view (Follow turned on).
    reveal_end: bool,
    /// The last row was in view at the last pass.
    at_end: bool,
}

impl Default for TextLogState {
    fn default() -> Self {
        Self {
            table: TableState::new(),
            levels: [true; 6],
            text: String::new(),
            follow: true,
            expanded: BTreeSet::new(),
            rows: Vec::new(),
            scanned: 0,
            counts: [0; 6],
            key: None,
            cursor_entry: None,
            cursor_rows: Vec::new(),
            moved_here: false,
            reveal_end: false,
            at_end: true,
        }
    }
}

impl TextLogState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether lines of `level` are shown (all are by default).
    pub fn level_shown(&self, level: Level) -> bool {
        self.levels[level as usize]
    }

    /// Shows or hides the lines of `level`.
    pub fn set_level_shown(&mut self, level: Level, on: bool) {
        self.levels[level as usize] = on;
    }

    /// The text filter (empty: none).
    pub fn filter(&self) -> &str {
        &self.text
    }

    /// Shows only the lines whose body or entity path holds `text`, case
    /// aside.
    pub fn set_filter(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    /// Whether Follow is on: the last line kept in view as lines come in.
    pub fn follow(&self) -> bool {
        self.follow
    }

    /// Turns Follow on (bringing the last line into view) or off.
    pub fn set_follow(&mut self, on: bool) {
        if on && !self.follow {
            self.reveal_end = true;
        }
        self.follow = on;
    }

    /// Whether entry `index`'s body shows all its lines.
    pub fn is_expanded(&self, index: usize) -> bool {
        self.expanded.contains(&index)
    }

    /// Shows all the lines of entry `index`'s body, or its first one.
    pub fn set_expanded(&mut self, index: usize, on: bool) {
        if on {
            self.expanded.insert(index);
        } else {
            self.expanded.remove(&index);
        }
    }

    /// The entries shown at the last pass, by index, in order.
    pub fn shown(&self) -> &[usize] {
        &self.rows
    }

    /// How many entries of `level` match the text filter (shown or not).
    pub fn count(&self, level: Level) -> usize {
        self.counts[level as usize]
    }

    /// Filters again from the first entry at the next pass: after the
    /// entries changed otherwise than by growing at their end, or the
    /// application's own filter ([`TextLog::keep`]) changed.
    pub fn refilter(&mut self) {
        self.key = None;
    }

    /// The table's own state (its selection is the rows at the time
    /// cursor).
    pub fn table(&self) -> &TableState<usize> {
        &self.table
    }

    /// Brings the filter up to date with `source`: from scratch when the
    /// filter changed or the source shrank, else the new entries only.
    fn update(&mut self, source: &dyn LogSource, keep: Option<&dyn Fn(usize) -> bool>) {
        let key = FilterKey {
            levels: self.levels,
            text: self.text.to_lowercase(),
        };
        let len = source.len();
        if self.key.as_ref() != Some(&key) || len < self.scanned {
            self.rows.clear();
            self.scanned = 0;
            self.counts = [0; 6];
            self.expanded.retain(|&i| i < len);
        }
        for i in self.scanned..len {
            if keep.is_some_and(|keep| !keep(i)) {
                continue;
            }
            if !key.text.is_empty()
                && !contains_folded(source.body(i), &key.text)
                && !contains_folded(source.entity(i), &key.text)
            {
                continue;
            }
            let level = source.level(i) as usize;
            self.counts[level] += 1;
            if key.levels[level] {
                self.rows.push(i);
            }
        }
        self.scanned = len;
        self.key = Some(key);
    }

    /// The rows (positions in `rows`) at the time cursor: those at the
    /// latest time not after it.
    fn cursor_range(&self, source: &dyn LogSource, cursor: Option<f64>) -> Range<usize> {
        let Some(t) = cursor.filter(|t| t.is_finite()) else {
            return 0..0;
        };
        let end = self.rows.partition_point(|&i| source.time(i) <= t);
        if end == 0 {
            return 0..0;
        }
        let at = source.time(self.rows[end - 1]);
        let start = self.rows[..end].partition_point(|&i| source.time(i) < at);
        start..end
    }
}

/// `text` holds `needle` (lower case), case aside.
fn contains_folded(text: &str, needle: &str) -> bool {
    if needle.is_empty() {
        true
    } else if text.is_ascii() && needle.is_ascii() {
        text.as_bytes()
            .windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
    } else {
        text.to_lowercase().contains(needle)
    }
}

/// How a time is written: in `scale`, else as a plain number (whole
/// numbers without decimals: frame numbers).
pub fn format_time(scale: Option<&TimeScale>, time: f64) -> String {
    match scale {
        Some(scale) => scale.format(time),
        None if time.fract() == 0.0 && time.abs() < 1e15 => format!("{}", time as i64),
        None => format!("{time:.3}"),
    }
}

/// What [`TextLog::show`] returns.
#[derive(Debug)]
#[non_exhaustive]
pub struct TextLogOutput {
    /// The table, as one focusable widget.
    pub response: Response,
    /// A row was clicked or reached with the keys this pass: the time it
    /// moves the cursor to. A bound timeline's playhead is already there.
    pub cursor_moved: Option<f64>,
    /// The entry of that row.
    pub picked: Option<usize>,
    /// The entry under the pointer.
    pub hovered: Option<usize>,
    /// Follow is on and the last line is in view.
    pub following: bool,
    /// How many lines are shown, of how many entries.
    pub shown: usize,
    pub total: usize,
}

/// Decides which entries the application shows, beside the log's own
/// filters: `keep(index)`.
type KeepFn<'a> = dyn Fn(usize) -> bool + 'a;

/// The text log widget.
pub struct TextLog<'a> {
    id: String,
    source: &'a dyn LogSource,
    label: Option<String>,
    time_cursor: Option<f64>,
    timeline: Option<&'a mut TimelineState>,
    scale: Option<TimeScale>,
    keep: Option<&'a KeepFn<'a>>,
    highlight_entity: Option<&'a str>,
    height: Option<f32>,
    toolbar: bool,
    reveal_cursor: bool,
    app: Option<&'a AppHandle>,
}

impl std::fmt::Debug for TextLog<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextLog")
            .field("id", &self.id)
            .field("entries", &self.source.len())
            .field("time_cursor", &self.time_cursor)
            .finish_non_exhaustive()
    }
}

impl<'a> TextLog<'a> {
    /// A text log named `id` (its table's id in steps and signals),
    /// showing `source`.
    pub fn new(id: impl Into<String>, source: &'a dyn LogSource) -> Self {
        Self {
            id: id.into(),
            source,
            label: None,
            time_cursor: None,
            timeline: None,
            scale: None,
            keep: None,
            highlight_entity: None,
            height: None,
            toolbar: true,
            reveal_cursor: true,
            app: None,
        }
    }

    /// Its accessible name (default: "Text log").
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The time cursor, on the entries' time axis; the application passes
    /// it every pass. The rows at the latest time not after it are
    /// highlighted.
    pub fn time_cursor(mut self, time: Option<f64>) -> Self {
        self.time_cursor = time;
        self
    }

    /// Follows a timeline: its playhead is the time cursor, its scale
    /// writes the times, and a row picked moves the playhead.
    pub fn timeline(mut self, state: &'a mut TimelineState) -> Self {
        self.timeline = Some(state);
        self
    }

    /// Writes the times in a timeline's scale (`1:02.500`, a timecode,
    /// bars and beats); a bound timeline gives its own. Without one, a
    /// time is a plain number.
    pub fn time_scale(mut self, scale: TimeScale) -> Self {
        self.scale = Some(scale);
        self
    }

    /// The application's own filter, beside the log's: only the entries
    /// for which `keep(index)` holds are shown. Call
    /// [`TextLogState::refilter`] when its answers change.
    pub fn keep(mut self, keep: &'a KeepFn<'a>) -> Self {
        self.keep = Some(keep);
        self
    }

    /// Tints the entity cell of the rows logged on `path` (an entity
    /// selected or hovered elsewhere).
    pub fn highlight_entity(mut self, path: Option<&'a str>) -> Self {
        self.highlight_entity = path;
        self
    }

    /// Its height, the filters included (default: what is available).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Shows the level chips, the text filter and Follow above the table
    /// (default: on).
    pub fn toolbar(mut self, on: bool) -> Self {
        self.toolbar = on;
        self
    }

    /// Brings the rows at the time cursor into view when the cursor moves
    /// elsewhere than from this log (default: on).
    pub fn reveal_cursor(mut self, on: bool) -> Self {
        self.reveal_cursor = on;
        self
    }

    /// Runs its table's moments as steps of this application.
    pub fn app(mut self, app: &'a AppHandle) -> Self {
        self.app = Some(app);
        self
    }

    pub fn show(self, ui: &mut Ui, state: &mut TextLogState) -> TextLogOutput {
        let TextLog {
            id,
            source,
            label,
            time_cursor,
            mut timeline,
            scale,
            keep,
            highlight_entity,
            height,
            toolbar,
            reveal_cursor,
            app,
        } = self;
        let ctx = ui.ctx().clone();
        let label = label.unwrap_or_else(|| tr(&ctx, &s::TEXT_LOG));
        let top = ui.cursor().top();
        if toolbar {
            toolbar_ui(ui, state, &id);
        }
        state.update(source, keep);

        // The time cursor's rows are the table's selection.
        let cursor = timeline.as_ref().map(|t| t.playhead).or(time_cursor);
        let scale = timeline.as_ref().map(|t| t.scale).or(scale);
        let range = state.cursor_range(source, cursor);
        let at_cursor: Vec<usize> = state.rows[range.clone()].to_vec();
        let moved_here = std::mem::take(&mut state.moved_here);
        if at_cursor != state.cursor_rows {
            if reveal_cursor && !moved_here && !range.is_empty() {
                state.table.reveal(range.end - 1);
            }
            state.cursor_rows = at_cursor.clone();
        }
        let entry = state
            .cursor_entry
            .filter(|e| at_cursor.contains(e))
            .or(at_cursor.last().copied());
        state.cursor_entry = entry;
        if state.table.selection().keys() != at_cursor.as_slice()
            || state.table.selection().cursor() != entry.as_ref()
        {
            state.table.set_selection(at_cursor, entry);
        }
        if std::mem::take(&mut state.reveal_end) && !state.rows.is_empty() {
            state.table.reveal(state.rows.len() - 1);
        }

        // Rows expanded to all their lines, by position.
        let t = Tokens::of(&ctx);
        let body_font = TextStyle::Body.resolve(ui.style());
        let line_h = ui.fonts_mut(|f| f.row_height(&body_font));
        let row_h = crate::rows::height(ui);
        let mut extra: Vec<(usize, f32)> = Vec::new();
        let mut total_extra = 0.0;
        for &e in &state.expanded {
            let Ok(pos) = state.rows.binary_search(&e) else {
                continue;
            };
            let lines = source.body(e).lines().count().max(1);
            if lines > 1 {
                total_extra += (lines - 1) as f32 * line_h;
                extra.push((pos, total_extra));
            }
        }

        let columns = [
            Column::new("time", tr(&ctx, &s::TEXT_LOG_TIME))
                .width(96.0)
                .sortable(false),
            Column::new("entity", tr(&ctx, &s::TEXT_LOG_ENTITY))
                .width(140.0)
                .sortable(false),
            Column::new("level", tr(&ctx, &s::TEXT_LOG_LEVEL))
                .width(84.0)
                .sortable(false),
            Column::new("body", tr(&ctx, &s::TEXT_LOG_BODY))
                .width(480.0)
                .sortable(false),
        ];
        let mut rows = Rows {
            source,
            shown: &state.rows,
            expanded: &state.expanded,
            extra: &extra,
            scale: scale.as_ref(),
            palette: &t.palette.log,
            muted: t.text.muted,
            tint: t.accent.subtle,
            highlight_entity,
            mono: TextStyle::Monospace.resolve(ui.style()),
            body_font,
            row_h,
            toggle: None,
            hovered: None,
        };
        let mut table = DataTable::new(id, &columns)
            .label(label)
            .mode(Mode::Single)
            .stick_to_bottom(state.follow);
        if let Some(app) = app {
            table = table.app(app);
        }
        if let Some(h) = height {
            let used = ui.cursor().top() - top;
            table = table.height((h - used).max(row_h * 2.0));
        }
        let out = table.show(ui, &mut state.table, &mut rows);
        let toggle = rows.toggle.take();
        let hovered = rows.hovered.take();
        drop(rows);
        if let Some(e) = toggle {
            let on = !state.is_expanded(e);
            state.set_expanded(e, on);
            ctx.request_repaint();
        }

        // A row picked moves the time cursor to its time.
        let mut cursor_moved = None;
        let mut picked = None;
        for signal in &out.signals {
            if let Signal::Select {
                cursor: Some(e), ..
            } = signal
                && *e < source.len()
            {
                let time = source.time(*e);
                picked = Some(*e);
                state.cursor_entry = Some(*e);
                state.moved_here = true;
                cursor_moved = Some(match timeline.as_deref_mut() {
                    Some(tl) => {
                        tl.set_playhead(time);
                        tl.playhead
                    }
                    None => time,
                });
                ctx.request_repaint();
            }
        }
        let len = state.rows.len();
        state.at_end = len == 0 || out.visible_rows.end >= len;
        TextLogOutput {
            response: out.response,
            cursor_moved,
            picked,
            hovered,
            following: state.follow && state.at_end,
            shown: len,
            total: source.len(),
        }
    }
}

/// The level chips, the text filter and Follow, on one line.
fn toolbar_ui(ui: &mut Ui, state: &mut TextLogState, id: &str) {
    let ctx = ui.ctx().clone();
    ui.horizontal_wrapped(|ui| {
        for level in Level::ALL {
            let mut on = state.level_shown(level);
            let count = state.count(level);
            let name = tr_args(&ctx, &s::TEXT_LOG_SHOW_LEVEL, &[("level", level.name())]);
            let text = format!("{} {count}", level.name());
            if ui.add(ToggleChip::new(&mut on, text).name(name)).changed() {
                state.set_level_shown(level, on);
            }
        }
        let hint = tr(&ctx, &s::TEXT_LOG_FILTER);
        let mut text = state.filter().to_string();
        let edit = ui.add(
            egui::TextEdit::singleline(&mut text)
                .id(egui::Id::unique(("egui_pop_views::text_log::filter", id)))
                .hint_text(&hint)
                .desired_width(160.0),
        );
        a11y::name(&edit, Role::SearchInput, &hint);
        if edit.changed() {
            state.set_filter(text);
        }
        let mut follow = state.follow();
        let name = tr(&ctx, &s::TEXT_LOG_FOLLOW);
        if ui.add(ToggleChip::new(&mut follow, name)).changed() {
            state.set_follow(follow);
        }
        ui.weak(tr_args(
            &ctx,
            &s::TEXT_LOG_COUNT,
            &[
                ("count", &state.rows.len().to_string()),
                ("total", &state.scanned.to_string()),
            ],
        ));
    });
}

/// The rows the table draws: the entries the filters let through.
struct Rows<'a> {
    source: &'a dyn LogSource,
    shown: &'a [usize],
    expanded: &'a BTreeSet<usize>,
    /// Rows taller than one line, by position, with the extra height of
    /// them all up to and including each.
    extra: &'a [(usize, f32)],
    scale: Option<&'a TimeScale>,
    palette: &'a LogPalette,
    muted: Color32,
    tint: Color32,
    highlight_entity: Option<&'a str>,
    mono: FontId,
    body_font: FontId,
    row_h: f32,
    /// The entry whose body's disclosure was clicked.
    toggle: Option<usize>,
    hovered: Option<usize>,
}

impl Rows<'_> {
    /// The band of a cell holding its first line.
    fn band(ui: &Ui, row_h: f32) -> Rect {
        let max = ui.max_rect();
        Rect::from_min_max(
            pos2(ui.cursor().min.x, max.top()),
            pos2(max.right(), max.top() + row_h.min(max.height())),
        )
    }
}

impl TableRows<usize> for Rows<'_> {
    fn len(&self) -> usize {
        self.shown.len()
    }

    fn key(&self, row: usize) -> usize {
        self.shown[row]
    }

    fn label(&self, row: usize) -> String {
        let e = self.shown[row];
        let body = self.source.body(e);
        format!(
            "{} {} {} {}",
            format_time(self.scale, self.source.time(e)),
            self.source.level(e).name(),
            self.source.entity(e),
            body.lines().next().unwrap_or(body)
        )
    }

    fn cell(&mut self, ui: &mut Ui, row: usize, column: usize, selected: bool) {
        let e = self.shown[row];
        if ui.rect_contains_pointer(ui.max_rect()) {
            self.hovered = Some(e);
        }
        let band = Self::band(ui, self.row_h);
        let muted = if selected {
            ui.visuals().text_color()
        } else {
            self.muted
        };
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(band)
                .layout(Layout::left_to_right(Align::Center)),
        );
        let ui = &mut child;
        // A selectable label senses clicks and sits above the row: a
        // click on a time, an entity or a body would select its text
        // instead of picking the row.
        ui.style_mut().interaction.selectable_labels = false;
        match column {
            0 => {
                let text = format_time(self.scale, self.source.time(e));
                ui.label(RichText::new(text).font(self.mono.clone()).color(muted));
            }
            1 => {
                let path = self.source.entity(e);
                if self.highlight_entity == Some(path) {
                    ui.painter().rect_filled(ui.max_rect(), 0.0, self.tint);
                }
                ui.label(RichText::new(path).color(muted));
            }
            2 => {
                let level = self.source.level(e);
                let color = level.color(self.palette);
                let galley = ui.painter().layout_no_wrap(
                    level.name().to_string(),
                    FontId::new(self.mono.size * 0.9, self.mono.family.clone()),
                    color,
                );
                let size = galley.size() + vec2(8.0, 2.0);
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 3.0, color.gamma_multiply(0.16));
                ui.painter()
                    .galley(rect.min + vec2(4.0, 1.0), galley, color);
            }
            _ => {
                let body = self.source.body(e);
                let mut lines = body.lines();
                let first = lines.next().unwrap_or("");
                let more = lines.count();
                if more > 0 {
                    let open = self.expanded.contains(&e);
                    let name = if open {
                        tr(ui.ctx(), &s::TEXT_LOG_COLLAPSE)
                    } else {
                        tr_args(
                            ui.ctx(),
                            &s::TEXT_LOG_EXPAND,
                            &[("count", &(more + 1).to_string())],
                        )
                    };
                    let r = disclosure(ui, open, self.body_font.size);
                    a11y::name(&r, Role::Button, &name);
                    if r.clicked() {
                        self.toggle = Some(e);
                    }
                    ui.label(RichText::new(first).font(self.body_font.clone()));
                    if open {
                        let rest: Vec<&str> = body.lines().skip(1).collect();
                        let color = ui.visuals().text_color();
                        let galley = ui.painter().layout_no_wrap(
                            rest.join("\n"),
                            self.body_font.clone(),
                            color,
                        );
                        let at = pos2(r.rect.right() + ui.spacing().item_spacing.x, band.bottom());
                        // Under the first line, in the row's own height.
                        ui.painter().galley(at, galley, color);
                    } else {
                        ui.label(
                            RichText::new(tr_args(
                                ui.ctx(),
                                &s::TEXT_LOG_MORE,
                                &[("count", &more.to_string())],
                            ))
                            .color(muted),
                        );
                    }
                } else {
                    ui.label(RichText::new(first).font(self.body_font.clone()));
                }
            }
        }
    }

    fn row_top(&self, row: usize, row_h: f32) -> f32 {
        let before = self.extra.partition_point(|(pos, _)| *pos < row);
        let extra = if before == 0 {
            0.0
        } else {
            self.extra[before - 1].1
        };
        row as f32 * row_h + extra
    }
}

/// A body's disclosure: a small triangle, pointing down when open,
/// right when folded, in the text's colour (strong under the pointer).
fn disclosure(ui: &mut Ui, open: bool, size: f32) -> Response {
    let (rect, r) = ui.allocate_exact_size(vec2(size, size), egui::Sense::click());
    let color = if r.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    let c = rect.center();
    let h = size * 0.3;
    let points = if open {
        vec![
            pos2(c.x - h, c.y - h * 0.6),
            pos2(c.x + h, c.y - h * 0.6),
            pos2(c.x, c.y + h * 0.7),
        ]
    } else {
        vec![
            pos2(c.x - h * 0.6, c.y - h),
            pos2(c.x + h * 0.7, c.y),
            pos2(c.x - h * 0.6, c.y + h),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
    r
}
