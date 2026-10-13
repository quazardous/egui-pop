//! A terminal's state: the emulator (vt100's screen and scrollback), the
//! source, the session, the selection; shared by the application, the
//! widget and the commands an agent runs.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use egui_pop_api::AppHandle;
use egui_pop_api::steps::Outcome;
use egui_pop_core::lock;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use super::source::{CommandLine, Sink, Size, Spawn, TerminalSource};
use super::steps::{TERMINAL_EXIT, TERMINAL_INPUT, TERMINAL_SPAWN};
use crate::steps::run;

/// How many lines of scrollback a terminal keeps unless told otherwise.
pub const SCROLLBACK: usize = 10_000;

/// What kind of input `terminal.input` carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputKind {
    /// Keys typed in the terminal (text, or a key's escape sequence).
    Keys,
    /// A paste (bracketed when the program asked for it).
    Paste,
    /// Text an agent or the application sends (`terminal.send`).
    Agent,
}

impl InputKind {
    pub fn name(self) -> &'static str {
        match self {
            InputKind::Keys => "keys",
            InputKind::Paste => "paste",
            InputKind::Agent => "agent",
        }
    }
}

/// A place in the terminal's text: `line` counts from the oldest line
/// kept (the scrollback's first, then the screen's), `col` in cells.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Cell {
    pub line: usize,
    pub col: u16,
}

impl Cell {
    pub fn new(line: usize, col: u16) -> Cell {
        Cell { line, col }
    }
}

/// What the emulator reports besides the screen.
#[derive(Debug, Default)]
pub(crate) struct Callbacks {
    pub title: String,
    pub bells: u64,
}

impl vt100::Callbacks for Callbacks {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.title = String::from_utf8_lossy(title).into_owned();
    }

    fn audible_bell(&mut self, _: &mut vt100::Screen) {
        self.bells += 1;
    }
}

pub(crate) struct Model {
    pub id: String,
    pub parser: vt100::Parser<Callbacks>,
    pub scrollback: usize,
    /// Bumped at each start and end: a sink of another session is
    /// ignored.
    pub session: u64,
    /// A start was tried (autostart happens once).
    pub started: bool,
    pub autostart: bool,
    pub running: bool,
    /// How the last session ended (its code when known).
    pub ended: Option<Option<i32>>,
    /// The program ended; `terminal.exit` runs at the next pass.
    pub pending_exit: Option<Option<i32>>,
    /// An end the widget has not reported yet (`TerminalSignal::Ended`).
    pub unseen_end: Option<Option<i32>>,
    /// Bumped by every output.
    pub version: u64,
    pub source_name: String,
    pub writable: bool,
    /// Anchor and head.
    pub selection: Option<(Cell, Cell)>,
    pub ctx: Option<egui::Context>,
    /// Wheel movement not yet a whole line.
    pub scroll_rest: f32,
    /// The widget takes the keyboard focus at its next pass.
    pub focus: bool,
}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Terminal")
            .field("id", &self.id)
            .field("source", &self.source_name)
            .field("size", &self.parser.screen().size())
            .field("session", &self.session)
            .field("running", &self.running)
            .field("ended", &self.ended)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

impl Model {
    pub fn size(&self) -> Size {
        let (rows, cols) = self.parser.screen().size();
        Size { rows, cols }
    }

    /// Asks for a pass of the interface (from any thread).
    pub fn wake(&self) {
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }

    /// Feeds output to the emulator.
    pub fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
        self.version += 1;
        self.wake();
    }

    /// Writes a notice of the terminal's own (dim, on its own line).
    pub fn notice(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let at_start = self.parser.screen().cursor_position().1 == 0;
        let lead = if at_start { "" } else { "\r\n" };
        let s = format!("{lead}\x1b[0;2m{text}\x1b[0m\r\n");
        self.process(s.as_bytes());
    }

    /// How many lines the scrollback holds now.
    pub fn history(&mut self) -> usize {
        let screen = self.parser.screen_mut();
        let at = screen.scrollback();
        screen.set_scrollback(usize::MAX);
        let h = screen.scrollback();
        screen.set_scrollback(at);
        h
    }

    /// How far the view is scrolled back, in lines.
    pub fn offset(&self) -> usize {
        self.parser.screen().scrollback()
    }

    pub fn set_offset(&mut self, lines: usize) {
        self.parser.screen_mut().set_scrollback(lines);
    }

    /// Runs `f` with the screen scrolled so that line `line` (see
    /// [`Cell`]) is in view, at the row it gives; the view comes back.
    pub fn at_line<R>(
        &mut self,
        line: usize,
        f: impl FnOnce(&vt100::Screen, u16) -> R,
    ) -> Option<R> {
        let h = self.history();
        let rows = usize::from(self.size().rows);
        let (offset, row) = if line < h {
            (h - line, 0)
        } else if line - h < rows {
            (0, line - h)
        } else {
            return None;
        };
        let at = self.offset();
        self.set_offset(offset);
        let r = f(self.parser.screen(), row as u16);
        self.set_offset(at);
        Some(r)
    }

    /// Every line kept: the scrollback's, then the screen's, with the
    /// screen's blank lines at the end left out.
    pub fn all_lines(&mut self) -> Vec<String> {
        let h = self.history();
        let size = self.size();
        let rows = usize::from(size.rows);
        let at = self.offset();
        let mut out = Vec::with_capacity(h + rows);
        let mut o = h;
        while o > 0 {
            self.set_offset(o);
            let take = o.min(rows);
            out.extend(self.parser.screen().rows(0, size.cols).take(take));
            o -= take;
        }
        self.set_offset(0);
        out.extend(self.parser.screen().rows(0, size.cols));
        self.set_offset(at);
        while out.last().is_some_and(String::is_empty) {
            out.pop();
        }
        out
    }

    /// The screen's lines, the blank ones at the end left out.
    pub fn screen_lines(&mut self) -> Vec<String> {
        let at = self.offset();
        self.set_offset(0);
        let cols = self.size().cols;
        let mut out: Vec<String> = self.parser.screen().rows(0, cols).collect();
        self.set_offset(at);
        while out.last().is_some_and(String::is_empty) {
            out.pop();
        }
        out
    }

    /// The selection, start first.
    pub fn selection_span(&self) -> Option<(Cell, Cell)> {
        let (a, b) = self.selection?;
        Some(if a <= b { (a, b) } else { (b, a) })
    }

    /// The selected text: soft-wrapped lines are joined, the others end
    /// with a line break.
    pub fn selection_text(&mut self) -> Option<String> {
        let (a, b) = self.selection_span()?;
        if a == b {
            return None;
        }
        let cols = self.size().cols;
        let mut out = String::new();
        for line in a.line..=b.line {
            let from = if line == a.line { a.col } else { 0 };
            let to = if line == b.line { b.col } else { cols };
            let piece = self.at_line(line, |s, row| {
                let text = s.rows(from, to.saturating_sub(from)).nth(usize::from(row));
                (text.unwrap_or_default(), s.row_wrapped(row))
            });
            let Some((text, wrapped)) = piece else {
                break;
            };
            out.push_str(&text);
            if line != b.line && !wrapped {
                out.push('\n');
            }
        }
        Some(out)
    }

    pub fn reset(&mut self) {
        let size = self.size();
        let title = std::mem::take(&mut self.parser.callbacks_mut().title);
        self.parser = vt100::Parser::new_with_callbacks(
            size.rows,
            size.cols,
            self.scrollback,
            Callbacks { title, bells: 0 },
        );
        self.selection = None;
        self.version += 1;
        self.wake();
    }
}

/// A terminal: what it shows and its session, as a handle the
/// application, the [`Terminal`](super::Terminal) widget and the
/// commands an agent runs share. Cheap to clone; usable from any thread
/// (the steps need the interface thread).
///
/// ```
/// use egui_pop_editors::terminal::{Console, TerminalState};
///
/// let log = Console::new();
/// let term = TerminalState::new("log", log.clone()).size(10, 40);
/// term.spawn(None, "user").unwrap();
/// log.write("one\ntwo\n\x1b[1mthree\x1b[0m");
/// assert_eq!(term.lines(1, 3), vec!["two", "three"]);
/// term.clear();
/// assert_eq!(term.text(), "");
/// ```
#[derive(Clone)]
pub struct TerminalState {
    model: Arc<Mutex<Model>>,
    source: Arc<Mutex<Box<dyn TerminalSource>>>,
}

impl fmt::Debug for TerminalState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.lock().fmt(f)
    }
}

impl TerminalState {
    /// A terminal named `id` (in steps, commands and the debug bridge)
    /// over `source`, 24 rows of 80 columns until it is drawn, keeping
    /// [`SCROLLBACK`] lines. It starts the first time it is drawn (see
    /// [`TerminalState::autostart`]).
    pub fn new(id: impl Into<String>, source: impl TerminalSource + 'static) -> TerminalState {
        let source_name = source.name();
        let writable = source.writable();
        let model = Model {
            id: id.into(),
            parser: vt100::Parser::new_with_callbacks(24, 80, SCROLLBACK, Callbacks::default()),
            scrollback: SCROLLBACK,
            session: 0,
            started: false,
            autostart: true,
            running: false,
            ended: None,
            pending_exit: None,
            unseen_end: None,
            version: 0,
            source_name,
            writable,
            selection: None,
            ctx: None,
            scroll_rest: 0.0,
            focus: false,
        };
        TerminalState {
            model: Arc::new(Mutex::new(model)),
            source: Arc::new(Mutex::new(Box::new(source))),
        }
    }

    /// How many lines of scrollback to keep (clears the terminal).
    pub fn scrollback(self, lines: usize) -> TerminalState {
        {
            let mut m = self.lock();
            m.scrollback = lines;
            m.reset();
        }
        self
    }

    /// The size until the widget gives its own (rows and columns of
    /// cells): what the program starts with when started before the
    /// terminal is drawn.
    pub fn size(self, rows: u16, cols: u16) -> TerminalState {
        self.resize(Size::new(rows, cols));
        self
    }

    /// Whether the widget starts the source the first time it is drawn
    /// (true by default); otherwise the application calls
    /// [`TerminalState::spawn`].
    pub fn autostart(self, on: bool) -> TerminalState {
        self.lock().autostart = on;
        self
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Model> {
        lock(&self.model)
    }

    pub fn id(&self) -> String {
        self.lock().id.clone()
    }

    /// Whether two handles are the same terminal.
    pub fn same(&self, other: &TerminalState) -> bool {
        Arc::ptr_eq(&self.model, &other.model)
    }

    /// The source's kind (`pty`, `console`...).
    pub fn source_name(&self) -> String {
        self.lock().source_name.clone()
    }

    /// Whether the terminal sends input (the source is writable).
    pub fn is_writable(&self) -> bool {
        self.lock().writable
    }

    /// Whether a session is running.
    pub fn is_running(&self) -> bool {
        self.lock().running
    }

    /// Whether the last session ended (rather than never started or
    /// still running).
    pub fn has_ended(&self) -> bool {
        self.lock().ended.is_some()
    }

    /// The exit code of the last session, when it ended with one.
    pub fn exit_code(&self) -> Option<i32> {
        self.lock().ended.flatten()
    }

    /// The title the program set (`OSC 0` or `OSC 2`).
    pub fn title(&self) -> String {
        self.lock().parser.callbacks().title.clone()
    }

    /// How many times the program rang the bell.
    pub fn bells(&self) -> u64 {
        self.lock().parser.callbacks().bells
    }

    /// Bumped by every output.
    pub fn version(&self) -> u64 {
        self.lock().version
    }

    pub fn current_size(&self) -> Size {
        self.lock().size()
    }

    /// The cursor on the screen: row and column, from 0.
    pub fn cursor(&self) -> (u16, u16) {
        self.lock().parser.screen().cursor_position()
    }

    /// Resizes the screen and tells the source (a PTY tells the program).
    pub fn resize(&self, size: Size) {
        {
            let mut m = self.lock();
            if m.size() == size {
                return;
            }
            m.parser.screen_mut().set_size(size.rows, size.cols);
            m.version += 1;
        }
        lock(&self.source).resize(size);
    }

    /// Everything kept, scrollback then screen, one line per row (a
    /// soft-wrapped line is several rows), without the blank rows at the
    /// end.
    pub fn text(&self) -> String {
        self.lock().all_lines().join("\n")
    }

    /// Lines `from` to `to` (end excluded) of [`TerminalState::text`].
    pub fn lines(&self, from: usize, to: usize) -> Vec<String> {
        let all = self.lock().all_lines();
        let to = to.min(all.len());
        all.get(from.min(to)..to).unwrap_or_default().to_vec()
    }

    /// How many lines [`TerminalState::text`] has.
    pub fn line_count(&self) -> usize {
        self.lock().all_lines().len()
    }

    /// The screen alone, without the blank rows at the end.
    pub fn screen_text(&self) -> String {
        self.lock().screen_lines().join("\n")
    }

    /// The selected text, if any.
    pub fn selection_text(&self) -> Option<String> {
        self.lock().selection_text()
    }

    /// The selection (anchor, head), if any.
    pub fn selection(&self) -> Option<(Cell, Cell)> {
        self.lock().selection
    }

    /// Selects from `anchor` to `head` (`head` excluded).
    pub fn select(&self, anchor: Cell, head: Cell) {
        let mut m = self.lock();
        m.selection = Some((anchor, head));
        m.wake();
    }

    /// Selects everything kept.
    pub fn select_all(&self) {
        let mut m = self.lock();
        let lines = m.all_lines();
        let last = lines.len().saturating_sub(1);
        let cols = m.size().cols;
        m.selection = Some((Cell::new(0, 0), Cell::new(last, cols)));
    }

    pub fn clear_selection(&self) {
        self.lock().selection = None;
    }

    /// How far the view is scrolled back, in lines (0: the screen).
    pub fn scrolled(&self) -> usize {
        self.lock().offset()
    }

    /// Scrolls the view back (`lines` > 0) or forward, within the
    /// scrollback.
    pub fn scroll(&self, lines: i64) {
        let mut m = self.lock();
        let at = m.offset() as i64;
        let to = (at + lines).max(0) as usize;
        m.set_offset(to);
        m.wake();
    }

    /// Back to the screen (the program's live output).
    pub fn scroll_to_bottom(&self) {
        let mut m = self.lock();
        m.set_offset(0);
        m.scroll_rest = 0.0;
    }

    /// Clears the screen and the scrollback; the program goes on (a
    /// shell's prompt shows again at its next output).
    pub fn clear(&self) {
        self.lock().reset();
    }

    /// Gives the widget the keyboard focus at its next pass.
    pub fn request_focus(&self) {
        let mut m = self.lock();
        m.focus = true;
        m.wake();
    }

    /// Output the application gives directly, as if the program wrote it
    /// (an application running the program in its own kernel replaces
    /// `terminal.spawn` and feeds the output here).
    pub fn feed(&self, bytes: &[u8]) {
        self.lock().process(bytes);
    }

    /// The program the application runs itself ended: `terminal.exit`
    /// runs at the next pass.
    pub fn finished(&self, code: Option<i32>) {
        let mut m = self.lock();
        m.pending_exit = Some(code);
        m.wake();
    }

    /// The state for the debug bridge and `terminal.read`.
    pub fn describe(&self) -> Json {
        let mut m = self.lock();
        let history = m.history();
        let size = m.size();
        let screen = m.parser.screen();
        let (row, col) = screen.cursor_position();
        json!({
            "terminal": m.id,
            "source": m.source_name,
            "writable": m.writable,
            "running": m.running,
            "started": m.started,
            "ended": m.ended.is_some(),
            "exit_code": m.ended.flatten(),
            "rows": size.rows,
            "cols": size.cols,
            "cursor": { "row": row, "col": col },
            "scrollback": history,
            "scrolled": screen.scrollback(),
            "alternate_screen": screen.alternate_screen(),
            "application_cursor": screen.application_cursor(),
            "bracketed_paste": screen.bracketed_paste(),
            "title": m.parser.callbacks().title,
            "bells": m.parser.callbacks().bells,
            "version": m.version,
            "selection": m.selection_span(),
        })
    }

    pub(crate) fn set_ctx(&self, ctx: &egui::Context) {
        let mut m = self.lock();
        if m.ctx.is_none() {
            m.ctx = Some(ctx.clone());
        }
    }

    /// Whether the widget should start the source now.
    pub(crate) fn wants_start(&self) -> bool {
        let m = self.lock();
        m.autostart && !m.started
    }

    /// Starts the source through the `terminal.spawn` step: a before-hook
    /// refuses (`Veto`) or changes the `command` (`Redirect`), a run-hook
    /// starts the program its own way (and feeds the output with
    /// [`TerminalState::feed`]). A terminal already running refuses.
    pub fn spawn(&self, app: Option<&AppHandle>, origin: &str) -> Result<Json, String> {
        if self.is_running() {
            return Err("the terminal is running: close it first".into());
        }
        let command: Option<CommandLine> = lock(&self.source).command();
        let args = {
            let mut m = self.lock();
            m.started = true;
            let size = m.size();
            json!({
                "terminal": m.id,
                "source": m.source_name,
                "command": command,
                "rows": size.rows,
                "cols": size.cols,
                "origin": origin,
            })
        };
        let state = self.clone();
        let outcome = run(app, &TERMINAL_SPAWN, args, move |args| {
            state.start_default(args)
        });
        match result(outcome) {
            Ok(v) => {
                let mut m = self.lock();
                m.running = true;
                m.ended = None;
                m.wake();
                Ok(v)
            }
            Err(e) => {
                let notice = notice_refused(&self.lock().ctx, &e);
                self.lock().notice(&notice);
                Err(e)
            }
        }
    }

    /// `terminal.spawn`'s default: starts the source.
    fn start_default(&self, args: &Json) -> Result<Json, String> {
        let command = match args.get("command") {
            None | Some(Json::Null) => None,
            Some(c) => Some(
                serde_json::from_value::<CommandLine>(c.clone())
                    .map_err(|e| format!("`command`: {e}"))?,
            ),
        };
        let (sink, size) = {
            let mut m = self.lock();
            m.session += 1;
            m.pending_exit = None;
            (Sink::new(&self.model, m.session), m.size())
        };
        lock(&self.source).start(&Spawn { command, size }, sink)
    }

    /// Sends input to the program through the `terminal.input` step: a
    /// before-hook refuses it or rewrites `text`, a run-hook sends it
    /// elsewhere. The view goes back to the screen. Returns how many
    /// bytes were written.
    pub fn send(
        &self,
        app: Option<&AppHandle>,
        text: &str,
        kind: InputKind,
        origin: &str,
    ) -> Result<usize, String> {
        let args = json!({
            "terminal": self.id(),
            "text": text,
            "kind": kind.name(),
            "origin": origin,
        });
        let state = self.clone();
        let outcome = run(app, &TERMINAL_INPUT, args, move |args| {
            state.write_default(args)
        });
        let v = result(outcome)?;
        self.scroll_to_bottom();
        Ok(v["bytes"].as_u64().unwrap_or(0) as usize)
    }

    /// `terminal.input`'s default: writes to the source (a paste
    /// bracketed when the program asked for it).
    fn write_default(&self, args: &Json) -> Result<Json, String> {
        let text = args["text"].as_str().ok_or("`text` is missing")?;
        let (running, bracketed) = {
            let m = self.lock();
            (m.running, m.parser.screen().bracketed_paste())
        };
        if !running {
            return Err("the terminal is not running".into());
        }
        let bytes = if args["kind"] == "paste" && bracketed {
            format!("\x1b[200~{text}\x1b[201~")
        } else {
            text.to_string()
        };
        lock(&self.source).write(bytes.as_bytes())?;
        Ok(json!({ "bytes": bytes.len() }))
    }

    /// Closes the session through the `terminal.exit` step (`reason`
    /// `close`): a before-hook keeps it open (`Veto`); by default the
    /// source stops (a PTY's program is killed) and a notice says so.
    pub fn close(&self, app: Option<&AppHandle>, origin: &str) -> Result<Json, String> {
        let (args, ctx) = {
            let m = self.lock();
            (
                json!({
                    "terminal": m.id,
                    "reason": "close",
                    "code": null,
                    "origin": origin,
                    "restart": false,
                }),
                m.ctx.clone(),
            )
        };
        let mut args = args;
        args["notice"] = json!(notice_closed(&ctx));
        self.end(app, args)
    }

    /// Closes the session if it runs, then starts again.
    pub fn restart(&self, app: Option<&AppHandle>, origin: &str) -> Result<Json, String> {
        if self.is_running() {
            self.close(app, origin)?;
        }
        self.spawn(app, origin)
    }

    /// Runs `terminal.exit` when the program ended (from the widget and
    /// the plugin's pass): its code, if it ended.
    pub(crate) fn poll(&self, app: Option<&AppHandle>) -> Option<Option<i32>> {
        let (code, args) = {
            let mut m = self.lock();
            let code = m.pending_exit.take()?;
            if !m.running {
                return None;
            }
            // A fact, whatever the hooks decide: the program is gone, and
            // late output of that session is dropped.
            m.running = false;
            m.ended = Some(code);
            m.unseen_end = Some(code);
            m.session += 1;
            let notice = notice_exited(&m.ctx, code);
            (
                code,
                json!({
                    "terminal": m.id,
                    "reason": "exited",
                    "code": code,
                    "origin": "program",
                    "restart": false,
                    "notice": notice,
                }),
            )
        };
        // What the source holds for the ended program (a pseudo-terminal)
        // is let go.
        let _ = lock(&self.source).stop();
        let _ = self.end(app, args);
        Some(code)
    }

    fn end(&self, app: Option<&AppHandle>, args: Json) -> Result<Json, String> {
        let state = self.clone();
        let outcome = run(app, &TERMINAL_EXIT, args, move |args| {
            state.end_default(args)
        });
        let v = result(outcome)?;
        if v["restart"] == true {
            self.spawn(app, "restart")?;
        }
        Ok(v)
    }

    /// `terminal.exit`'s default: stops the source (`close`), marks the
    /// session ended, writes the notice.
    fn end_default(&self, args: &Json) -> Result<Json, String> {
        if args["reason"] == "close" {
            lock(&self.source).stop()?;
            let mut m = self.lock();
            m.running = false;
            m.ended = Some(None);
            m.unseen_end = Some(None);
            m.session += 1;
            m.pending_exit = None;
        }
        let notice = args["notice"].as_str().unwrap_or("");
        self.lock().notice(notice);
        Ok(json!({ "ended": true, "restart": args["restart"] == true }))
    }
}

fn result(outcome: Outcome) -> Result<Json, String> {
    match outcome {
        Outcome::Done(r) => r,
        Outcome::Vetoed { reason, .. } => Err(reason),
    }
}

fn notice_exited(ctx: &Option<egui::Context>, code: Option<i32>) -> String {
    use crate::strings as s;
    match (ctx, code) {
        (Some(ctx), Some(c)) => {
            egui_pop_core::strings::tr_args(ctx, &s::TERMINAL_EXITED, &[("code", &c.to_string())])
        }
        (Some(ctx), None) => egui_pop_core::strings::tr(ctx, &s::TERMINAL_ENDED),
        (None, Some(c)) => s::TERMINAL_EXITED
            .fallback
            .replace("{ $code }", &c.to_string()),
        (None, None) => s::TERMINAL_ENDED.fallback.to_string(),
    }
}

fn notice_closed(ctx: &Option<egui::Context>) -> String {
    use crate::strings as s;
    match ctx {
        Some(ctx) => egui_pop_core::strings::tr(ctx, &s::TERMINAL_CLOSED),
        None => s::TERMINAL_CLOSED.fallback.to_string(),
    }
}

fn notice_refused(ctx: &Option<egui::Context>, reason: &str) -> String {
    use crate::strings as s;
    match ctx {
        Some(ctx) => {
            egui_pop_core::strings::tr_args(ctx, &s::TERMINAL_REFUSED, &[("reason", reason)])
        }
        None => s::TERMINAL_REFUSED.fallback.replace("{ $reason }", reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::source::Console;

    #[test]
    fn output_scrolls_into_the_scrollback_and_reads_back_whole() {
        let log = Console::new();
        let t = TerminalState::new("t", log.clone()).size(3, 20);
        t.spawn(None, "user").unwrap();
        for i in 0..10 {
            log.line(&format!("line {i}"));
        }
        assert_eq!(t.line_count(), 10);
        assert_eq!(t.lines(0, 2), vec!["line 0", "line 1"]);
        assert_eq!(t.screen_text(), "line 8\nline 9");
        // Scrolled back, reading is the same and the view stays.
        t.scroll(4);
        assert_eq!(t.scrolled(), 4);
        assert_eq!(t.line_count(), 10);
        assert_eq!(t.scrolled(), 4);
    }

    #[test]
    fn selection_spans_lines_and_joins_soft_wraps() {
        let log = Console::new();
        let t = TerminalState::new("t", log.clone()).size(4, 5);
        t.spawn(None, "user").unwrap();
        log.write("abcdefg\nxy");
        // "abcde" wraps into "fg".
        t.select(Cell::new(0, 2), Cell::new(2, 1));
        assert_eq!(t.selection_text().as_deref(), Some("cdefg\nx"));
    }

    #[test]
    fn a_finished_console_ends_with_a_notice_and_refuses_input() {
        let log = Console::new();
        log.line("done");
        log.finish(Some(3));
        let t = TerminalState::new("t", log.clone());
        t.spawn(None, "user").unwrap();
        assert_eq!(t.poll(None), Some(Some(3)));
        assert!(!t.is_running());
        assert_eq!(t.exit_code(), Some(3));
        assert_eq!(t.text(), "done\nProcess exited with code 3");
        assert!(t.send(None, "x", InputKind::Keys, "user").is_err());
    }

    #[test]
    fn late_output_of_a_closed_session_is_dropped() {
        let log = Console::new();
        let t = TerminalState::new("t", log.clone());
        t.spawn(None, "user").unwrap();
        log.write("a");
        t.close(None, "user").unwrap();
        log.write("b");
        assert_eq!(t.text(), "a\nSession closed");
        // Started again, the console's sink is the new session's.
        t.spawn(None, "user").unwrap();
        log.write("c");
        assert_eq!(t.text(), "a\nSession closed\nbc");
    }
}
