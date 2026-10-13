//! The terminal widget: draws the emulator's screen from the theme,
//! routes the keyboard to the program, selects, copies, scrolls back,
//! opens links, and gives the program its size.

use egui::accesskit::Role;
use egui::text::{LayoutJob, TextFormat};
use egui::{
    Color32, CornerRadius, CursorIcon, Event, EventFilter, FontId, Id, ImeEvent, Key, Pos2, Rect,
    Response, Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType, pos2, vec2,
};
use egui_pop_api::AppHandle;
use egui_pop_core::focus;
use egui_pop_core::strings::tr;
use egui_pop_core::theme::Tokens;
use egui_pop_model::ids::StepId;

use super::keys;
use super::links::{self, Link};
use super::source::Size;
use super::state::{Cell, InputKind, TerminalState};
use super::steps::{SURFACE, TERMINAL_INPUT, TERMINAL_SPAWN, TERMINALS};
use super::tokens::{TerminalTokens, mix};
use crate::strings as s;

/// What happened in a terminal this pass.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminalSignal {
    /// The widget started the source (its first pass, with autostart).
    Started,
    /// The session ended (the program exited, or it was closed).
    Ended { code: Option<i32> },
    /// A step was vetoed or failed (a refused start, refused input).
    Refused { step: StepId, reason: String },
    /// The selection was copied to the clipboard.
    Copied { chars: usize },
    /// A link was opened (Ctrl+click).
    Link { url: String },
    /// The leave chord (Shift+Escape) gave the focus back.
    Left,
}

/// What a terminal's `show` returns.
#[derive(Debug)]
pub struct TerminalOutput {
    /// The terminal, the widget holding the keyboard focus.
    pub response: Response,
    pub signals: Vec<TerminalSignal>,
}

/// A terminal over a [`TerminalState`]: ANSI colours from the theme's
/// `terminal.*` tokens, scrollback, selection and copy, links, the
/// program told its size.
///
/// **Keys.** While it has the focus, every key goes to the program,
/// Escape and Tab included, before the application's shortcuts, except:
/// Shift+Escape, which leaves the terminal (the focus stays in its
/// surface, as Escape does for a text input); Ctrl+Shift chords, which
/// stay the application's (a terminal cannot tell Ctrl+Shift+K from
/// Ctrl+K) apart from Ctrl+Shift+C, copy; Command chords on macOS.
/// Ctrl+C copies when text is selected and interrupts otherwise; Ctrl+V
/// pastes; Shift+Page Up and Shift+Page Down scroll back.
#[derive(Debug, Default)]
pub struct Terminal<'a> {
    label: Option<String>,
    app: Option<&'a AppHandle>,
    height: Option<f32>,
}

/// One pass's geometry.
struct Geometry {
    inner: Rect,
    font: FontId,
    cell_w: f32,
    row_h: f32,
    rows: u16,
    cols: u16,
}

impl Geometry {
    /// The row and the column boundary nearest a point (columns from 0 to
    /// `cols`: between cells).
    fn at(&self, p: Pos2) -> (u16, u16) {
        let row = ((p.y - self.inner.top()) / self.row_h).floor();
        let col = ((p.x - self.inner.left()) / self.cell_w).round();
        (
            row.clamp(0.0, f32::from(self.rows - 1)) as u16,
            col.clamp(0.0, f32::from(self.cols)) as u16,
        )
    }

    /// The cell under a point.
    fn cell_under(&self, p: Pos2) -> (u16, u16) {
        let (row, _) = self.at(p);
        let col = ((p.x - self.inner.left()) / self.cell_w).floor();
        (row, col.clamp(0.0, f32::from(self.cols - 1)) as u16)
    }

    fn x(&self, col: u16) -> f32 {
        self.inner.left() + f32::from(col) * self.cell_w
    }

    fn y(&self, row: u16) -> f32 {
        self.inner.top() + f32::from(row) * self.row_h
    }
}

/// A run of cells drawn alike.
#[derive(Clone, Copy, PartialEq)]
struct Style {
    fg: Color32,
    bg: Option<Color32>,
    italic: bool,
    underline: bool,
}

impl<'a> Terminal<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Its accessible name ("Terminal" by default).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The application: the terminal's moments become steps, it
    /// registers for the commands and the collectors.
    pub fn app(mut self, app: &'a AppHandle) -> Self {
        self.app = Some(app);
        self
    }

    /// Its height in points (all the height left by default; at least
    /// three rows).
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Draws the terminal.
    pub fn show(self, ui: &mut Ui, state: &TerminalState) -> TerminalOutput {
        let id = Id::new(("egui_pop_terminal", state.id()));
        let ctx = ui.ctx().clone();
        state.set_ctx(&ctx);
        keys::register(&ctx, id);
        if let Some(app) = self.app
            && let Some(reg) = app.service(&TERMINALS)
        {
            reg.attach(state);
            if ctx.memory(|m| m.has_focus(id)) {
                reg.set_focused(&state.id());
            }
        }
        let mut signals = Vec::new();
        if state.wants_start() {
            match state.spawn(self.app, "user") {
                Ok(_) => signals.push(TerminalSignal::Started),
                Err(reason) => signals.push(TerminalSignal::Refused {
                    step: TERMINAL_SPAWN,
                    reason,
                }),
            }
        }
        state.poll(self.app);
        if let Some(code) = state.lock().unseen_end.take() {
            signals.push(TerminalSignal::Ended { code });
        }
        let response = focus::scope(ui, SURFACE, &[], |ui| {
            self.area(ui, state, id, &mut signals)
        });
        if !signals.is_empty() {
            ctx.request_repaint();
        }
        TerminalOutput { response, signals }
    }

    fn area(
        &self,
        ui: &mut Ui,
        state: &TerminalState,
        id: Id,
        signals: &mut Vec<TerminalSignal>,
    ) -> Response {
        let ctx = ui.ctx().clone();
        let t = TerminalTokens::of(&ctx);
        let tokens = Tokens::of(&ctx);
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let (cell_w, row_h) = ctx.fonts_mut(|f| {
            (
                f.glyph_width(&font, 'M').max(1.0),
                f.row_height(&font).ceil().max(1.0),
            )
        });
        let pad = tokens.space.s;
        let width = ui.available_width().max(cell_w * 4.0 + 2.0 * pad);
        let height = self
            .height
            .unwrap_or_else(|| ui.available_height())
            .max(row_h * 3.0 + 2.0 * pad);
        let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let response = ui.interact(rect, id, Sense::click_and_drag());
        let inner = rect.shrink(pad);
        let g = Geometry {
            inner,
            font,
            cell_w,
            row_h,
            rows: ((inner.height() / row_h).floor() as u16).max(1),
            cols: ((inner.width() / cell_w).floor() as u16).max(2),
        };
        state.resize(Size::new(g.rows, g.cols));
        if std::mem::take(&mut state.lock().focus) {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        let focused = ctx.memory(|m| m.has_focus(id));
        let (events, left) = keys::take(&ctx, id);
        if left {
            signals.push(TerminalSignal::Left);
        }
        if focused {
            ctx.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                );
            });
            self.input(ui, state, &g, events, signals);
        }
        wheel(ui, state, &response, &g);
        let link = self.pointer(ui, state, &response, &g, signals);
        paint(ui, state, &g, &t, &tokens, rect, focused, link.as_ref());
        if focused {
            let (row, col) = state.cursor();
            let cursor = Rect::from_min_size(pos2(g.x(col), g.y(row)), vec2(1.0, g.row_h));
            ctx.output_mut(|o| {
                o.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Terminal,
                    rect,
                    cursor_rect: cursor,
                    should_interrupt_composition: false,
                });
            });
        }
        let label = self.label.clone().unwrap_or_else(|| tr(&ctx, &s::TERMINAL));
        let value = state.screen_text();
        let enabled = response.enabled();
        response.widget_info(move || {
            let mut info = WidgetInfo::labeled(WidgetType::TextEdit, enabled, &label);
            info.current_text_value = Some(value.clone());
            info
        });
        ctx.accesskit_node_builder(id, |node| node.set_role(Role::Terminal));
        response
    }

    /// The keys and clipboard events taken for the terminal.
    fn input(
        &self,
        ui: &Ui,
        state: &TerminalState,
        g: &Geometry,
        events: Vec<Event>,
        signals: &mut Vec<TerminalSignal>,
    ) {
        let application = state.lock().parser.screen().application_cursor();
        let page = i64::from(g.rows.saturating_sub(1).max(1));
        // The text an Alt+key press also produces (Alt sends ESC and the
        // key itself).
        let mut skip: Option<char> = None;
        for ev in events {
            let skipping = skip.take();
            match ev {
                Event::Text(text) => {
                    let same = skipping.is_some_and(|c| {
                        text.chars().count() == 1
                            && text.chars().all(|t| t.eq_ignore_ascii_case(&c))
                    });
                    if !same {
                        self.send(state, text, InputKind::Keys, signals);
                    }
                }
                Event::Ime(ImeEvent::Commit(text)) => {
                    self.send(state, text, InputKind::Keys, signals);
                }
                Event::Paste(text) => {
                    let text = text.replace("\r\n", "\r").replace('\n', "\r");
                    self.send(state, text, InputKind::Paste, signals);
                }
                // Copies the selection; with none, interrupts.
                Event::Copy if !copy(ui, state, signals) => {
                    self.send(state, "\x03".into(), InputKind::Keys, signals);
                }
                Event::Cut => self.send(state, "\x18".into(), InputKind::Keys, signals),
                Event::Key {
                    key,
                    pressed: true,
                    modifiers: m,
                    ..
                } => {
                    if m.ctrl && m.shift && key == Key::C {
                        copy(ui, state, signals);
                        continue;
                    }
                    if m.shift && !m.ctrl && !m.alt {
                        match key {
                            Key::PageUp => {
                                state.scroll(page);
                                continue;
                            }
                            Key::PageDown => {
                                state.scroll(-page);
                                continue;
                            }
                            _ => {}
                        }
                    }
                    if let Some(bytes) = keys::encode(key, m, application) {
                        if m.alt && !m.ctrl {
                            skip = bytes.get(1).map(|b| char::from(*b));
                        }
                        let text = String::from_utf8_lossy(&bytes).into_owned();
                        self.send(state, text, InputKind::Keys, signals);
                    }
                }
                _ => {}
            }
        }
    }

    /// Sends input when the source takes it.
    fn send(
        &self,
        state: &TerminalState,
        text: String,
        kind: InputKind,
        signals: &mut Vec<TerminalSignal>,
    ) {
        if text.is_empty() || !state.is_writable() {
            return;
        }
        state.clear_selection();
        if let Err(reason) = state.send(self.app, &text, kind, "user") {
            signals.push(TerminalSignal::Refused {
                step: TERMINAL_INPUT,
                reason,
            });
        }
    }

    /// Selection by dragging (a word on a double click, a line on a
    /// triple click), links under the pointer (Ctrl+click opens): the
    /// link under the pointer, if any.
    fn pointer(
        &self,
        ui: &Ui,
        state: &TerminalState,
        response: &Response,
        g: &Geometry,
        signals: &mut Vec<TerminalSignal>,
    ) -> Option<(u16, Link)> {
        let ctx = ui.ctx();
        let base = {
            let mut m = state.lock();
            m.history() - m.offset()
        };
        let abs = |(row, col): (u16, u16)| Cell::new(base + usize::from(row), col);
        let link = response.hover_pos().and_then(|p| {
            let (row, col) = g.cell_under(p);
            let (chars, cols) = row_chars(state, row, g.cols);
            links::find(&chars)
                .into_iter()
                .find(|l| {
                    let a = cols.get(l.chars.start).copied().unwrap_or(u16::MAX);
                    let b = cols.get(l.chars.end - 1).copied().unwrap_or(0);
                    (a..=b).contains(&col)
                })
                .map(|l| (row, l))
        });
        let ctrl = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
        if link.is_some() && ctrl {
            ctx.set_cursor_icon(CursorIcon::PointingHand);
        } else if response.hovered() {
            ctx.set_cursor_icon(CursorIcon::Text);
        }
        if response.triple_clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let (row, _) = g.cell_under(p);
                state.select(abs((row, 0)), abs((row, g.cols)));
            }
        } else if response.double_clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let (row, col) = g.cell_under(p);
                let (chars, cols) = row_chars(state, row, g.cols);
                if let Some((a, b)) = word_at(&chars, &cols, col) {
                    state.select(abs((row, a)), abs((row, b)));
                }
            }
        } else if response.clicked() {
            match &link {
                Some((_, l)) if ctrl => {
                    ctx.open_url(egui::OpenUrl::new_tab(&l.url));
                    signals.push(TerminalSignal::Link { url: l.url.clone() });
                }
                _ => state.clear_selection(),
            }
        } else if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(p) = ctx.input(|i| i.pointer.press_origin()) {
                let at = abs(g.at(p));
                state.select(at, at);
            }
        } else if response.dragged_by(egui::PointerButton::Primary)
            && let Some(p) = response.interact_pointer_pos()
            && let Some((anchor, _)) = state.selection()
        {
            state.select(anchor, abs(g.at(p)));
        }
        link
    }
}

/// Copies the selection: whether there was one.
fn copy(ui: &Ui, state: &TerminalState, signals: &mut Vec<TerminalSignal>) -> bool {
    match state.selection_text() {
        Some(text) if !text.is_empty() => {
            signals.push(TerminalSignal::Copied {
                chars: text.chars().count(),
            });
            ui.ctx().copy_text(text);
            true
        }
        _ => false,
    }
}

/// The wheel scrolls back through the scrollback.
fn wheel(ui: &Ui, state: &TerminalState, response: &Response, g: &Geometry) {
    if !response.hovered() {
        return;
    }
    let dy = ui.input(|i| i.smooth_scroll_delta.y);
    if dy.abs() < f32::EPSILON {
        return;
    }
    let lines = {
        let mut m = state.lock();
        m.scroll_rest += dy / g.row_h;
        let whole = m.scroll_rest.trunc();
        m.scroll_rest -= whole;
        whole as i64
    };
    if lines != 0 {
        state.scroll(lines);
    }
    ui.ctx().input_mut(|i| i.smooth_scroll_delta.y = 0.0);
}

/// A row's characters in view and the column of each.
fn row_chars(state: &TerminalState, row: u16, cols: u16) -> (Vec<char>, Vec<u16>) {
    let m = state.lock();
    let screen = m.parser.screen();
    let mut chars = Vec::with_capacity(usize::from(cols));
    let mut at = Vec::with_capacity(usize::from(cols));
    for col in 0..cols {
        let Some(cell) = screen.cell(row, col) else {
            break;
        };
        if cell.is_wide_continuation() {
            continue;
        }
        if cell.has_contents() {
            for c in cell.contents().chars() {
                chars.push(c);
                at.push(col);
            }
        } else {
            chars.push(' ');
            at.push(col);
        }
    }
    (chars, at)
}

/// The word around a column (letters, digits and the characters of
/// paths and addresses): its first column and the column after it.
fn word_at(chars: &[char], cols: &[u16], col: u16) -> Option<(u16, u16)> {
    let is_word = |c: char| c.is_alphanumeric() || "_-./~:@%+=#?&".contains(c);
    let i = cols.iter().position(|c| *c >= col)?;
    if !chars.get(i).is_some_and(|c| is_word(*c)) {
        return None;
    }
    let mut a = i;
    while a > 0 && is_word(chars[a - 1]) {
        a -= 1;
    }
    let mut b = i + 1;
    while b < chars.len() && is_word(chars[b]) {
        b += 1;
    }
    Some((cols[a], cols[b - 1] + 1))
}

/// Draws the background, the cells in view, the selection, the cursor
/// and the link under the pointer.
#[allow(clippy::too_many_arguments)]
fn paint(
    ui: &Ui,
    state: &TerminalState,
    g: &Geometry,
    t: &TerminalTokens,
    tokens: &Tokens,
    rect: Rect,
    focused: bool,
    link: Option<&(u16, Link)>,
) {
    // Holds the model from its lock to the end: a source's thread waits.
    profiling::scope!("pop.terminal.paint");
    let painter = ui.painter_at(rect);
    let border = if focused {
        tokens.focus_ring
    } else {
        tokens.border.default
    };
    painter.rect(
        rect,
        CornerRadius::same(tokens.radius.widget.round() as u8),
        t.background,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    let mut m = state.lock();
    let base = m.history() - m.offset();
    let selection = m.selection_span();
    let writable = m.writable;
    let scrolled = m.offset() > 0;
    let screen = m.parser.screen();
    let selected = |line: usize, col: u16| {
        selection.is_some_and(|(a, b)| Cell::new(line, col) >= a && Cell::new(line, col) < b)
    };
    let draw = |start: u16, cells: u16, style: Style, text: &str, row: u16| {
        let x = g.x(start);
        let y = g.y(row);
        if let Some(bg) = style.bg {
            painter.rect_filled(
                Rect::from_min_size(pos2(x, y), vec2(f32::from(cells) * g.cell_w, g.row_h)),
                0.0,
                bg,
            );
        }
        if text.trim().is_empty() {
            return;
        }
        let mut job = LayoutJob::default();
        job.append(
            text,
            0.0,
            TextFormat {
                font_id: g.font.clone(),
                color: style.fg,
                italics: style.italic,
                underline: if style.underline {
                    Stroke::new(1.0, style.fg)
                } else {
                    Stroke::NONE
                },
                ..TextFormat::default()
            },
        );
        let galley = painter.layout_job(job);
        painter.galley(pos2(x, y), galley, style.fg);
    };
    for row in 0..g.rows {
        let line = base + usize::from(row);
        // The run being built: its first column, width in cells, style
        // and text.
        let mut run: Option<(u16, u16, Style, String)> = None;
        let mut col = 0;
        while col < g.cols {
            let Some(cell) = screen.cell(row, col) else {
                break;
            };
            if cell.is_wide_continuation() {
                col += 1;
                continue;
            }
            let width: u16 = if cell.is_wide() { 2 } else { 1 };
            let style = style_of(cell, t, selected(line, col));
            let text = if cell.has_contents() {
                cell.contents()
            } else {
                " "
            };
            // Non-ASCII glyphs may not be one cell wide in the font: each
            // is drawn at its own column.
            let alone = !text.is_ascii() || width > 1;
            let extends = !alone && run.as_ref().is_some_and(|r| r.2 == style);
            if extends {
                if let Some((_, w, _, buf)) = &mut run {
                    *w += width;
                    buf.push_str(text);
                }
            } else {
                if let Some((start, w, s, buf)) = run.take() {
                    draw(start, w, s, &buf, row);
                }
                if alone {
                    draw(col, width, style, text, row);
                } else {
                    run = Some((col, width, style, text.to_string()));
                }
            }
            col += width;
        }
        if let Some((start, w, s, buf)) = run.take() {
            draw(start, w, s, &buf, row);
        }
    }
    // The cursor, on the live screen of a terminal that takes input.
    let (crow, ccol) = screen.cursor_position();
    if writable && !scrolled && !screen.hide_cursor() && crow < g.rows && ccol < g.cols {
        let r = Rect::from_min_size(pos2(g.x(ccol), g.y(crow)), vec2(g.cell_w, g.row_h));
        if focused {
            painter.rect_filled(r, 0.0, t.cursor);
            if let Some(cell) = screen.cell(crow, ccol)
                && cell.has_contents()
            {
                let galley = painter.layout_no_wrap(
                    cell.contents().to_string(),
                    g.font.clone(),
                    t.background,
                );
                painter.galley(r.min, galley, t.background);
            }
        } else {
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, t.cursor), StrokeKind::Inside);
        }
    }
    drop(m);
    if let Some((row, l)) = link {
        let (_, cols) = row_chars(state, *row, g.cols);
        if let (Some(&a), Some(&b)) = (cols.get(l.chars.start), cols.get(l.chars.end - 1)) {
            let y = g.y(*row) + g.row_h - 1.0;
            painter.line_segment(
                [pos2(g.x(a), y), pos2(g.x(b + 1), y)],
                Stroke::new(1.0, t.link),
            );
        }
    }
}

/// How a cell is drawn: its colours (inverse, dim, bold and the
/// selection applied), italics and underline.
fn style_of(cell: &vt100::Cell, t: &TerminalTokens, selected: bool) -> Style {
    let bold = cell.bold();
    let default_fg = if bold { t.bold } else { t.foreground };
    let mut fg = t.color(cell.fgcolor(), default_fg, bold);
    let mut bg = match cell.bgcolor() {
        vt100::Color::Default => None,
        c => Some(t.color(c, t.background, false)),
    };
    if cell.inverse() {
        let back = bg.unwrap_or(t.background);
        bg = Some(fg);
        fg = back;
    }
    if cell.dim() {
        fg = mix(fg, bg.unwrap_or(t.background), 0.4);
    }
    if selected {
        bg = Some(t.selection);
    }
    Style {
        fg,
        bg,
        italic: cell.italic(),
        underline: cell.underline(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_spans_path_characters() {
        let chars: Vec<char> = "ls ~/src/main.rs now".chars().collect();
        let cols: Vec<u16> = (0..chars.len() as u16).collect();
        assert_eq!(word_at(&chars, &cols, 5), Some((3, 16)));
        assert_eq!(word_at(&chars, &cols, 2), None);
    }
}
