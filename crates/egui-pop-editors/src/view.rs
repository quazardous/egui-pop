//! The editing surface both editors share: virtualised drawing (only the
//! lines in view are laid out), the keyboard, the pointer, the gutter.

use std::sync::Arc;

use egui::accesskit::Role;
use egui::text::{CCursor, LayoutJob, TextFormat};
use egui::{
    Align2, Color32, Event, EventFilter, FontId, Galley, Id, ImeEvent, Key, Modifiers, Painter,
    Pos2, Rect, Response, ScrollArea, Sense, Shape, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType,
    pos2, vec2,
};
use egui_pop_api::AppHandle;
use egui_pop_core::focus;
use egui_pop_core::icon::Icon;
use egui_pop_core::kit::{Kit, State};
use egui_pop_core::state::Slot;
use egui_pop_core::strings::{tr, tr_args};
use egui_pop_core::theme::Tokens;
use egui_pop_model::ids::StepId;

use crate::assist::Location;
use crate::assist_ui;
use crate::buffer::{Buffer, Change, Pos, Span, byte_at, indent_width};
use crate::find_bar;
use crate::fold;
use crate::highlight::{Kind, Runs};
use crate::history::EditKind;
use crate::markers::{Diff, Severity};
use crate::model::{EditorState, Model, Why};
use crate::selection::Selection;
use crate::steps::{
    self, EDIT_CHANGE, EDIT_SAVE, EDITORS, EVENT_CURSOR_MOVED, EVENT_SELECTION_CHANGED, SURFACE,
};
use crate::strings as s;
use crate::tokens::EditorTokens;

/// The last press on an editor was in its gutter, per editor.
const IN_GUTTER: Slot<bool> = Slot::new("editors.view.in_gutter");

/// What happened in an editor this pass.
#[derive(Clone, Debug, PartialEq)]
pub enum Signal {
    /// The text changed (typing, a paste, an undo).
    Changed { version: u64, kind: String },
    /// The text was saved (the `edit.save` step succeeded).
    Saved { version: u64 },
    /// A breakpoint was set or cleared from the gutter.
    Breakpoint { line: usize, set: bool },
    /// A fold was opened or closed from the gutter.
    Fold { line: usize, folded: bool },
    /// The assist provider says where the symbol is defined (in this
    /// editor, the cursor went there; elsewhere, the application opens
    /// it).
    Definition { location: Location },
    /// A step was vetoed or failed.
    Refused { step: StepId, reason: String },
}

/// What an editor's `show` returns.
#[derive(Debug)]
pub struct EditorOutput {
    /// The text area, the widget holding the keyboard focus.
    pub response: Response,
    pub signals: Vec<Signal>,
}

/// How a surface is drawn.
pub(crate) struct Options<'a> {
    pub code: bool,
    pub line_numbers: bool,
    pub gutter_markers: bool,
    pub label: Option<String>,
    pub app: Option<&'a AppHandle>,
    pub height: Option<f32>,
    pub budget: usize,
    pub status: bool,
}

impl Options<'_> {
    fn origin(&self) -> &'static str {
        "user"
    }
}

/// Draws an editor.
pub(crate) fn show(ui: &mut Ui, state: &EditorState, o: &Options<'_>) -> EditorOutput {
    let id = Id::new(("egui_pop_editors", state.id()));
    if let Some(app) = o.app
        && let Some(reg) = app.service(&EDITORS)
    {
        reg.attach(state);
        if ui.memory(|m| m.has_focus(id)) {
            reg.set_focused(&state.id());
        }
    }
    let kind = if o.code { "code" } else { "text" };
    let mut signals = Vec::new();
    if o.code {
        assist_ui::poll(state, &mut signals);
    }
    let response = focus::scope(ui, SURFACE, &[("editorKind", kind)], |ui| {
        // The kit's frame, its focus ring while the text has the keyboard.
        let focused = ui.memory(|m| m.has_focus(id));
        let look = State::REST.with(State::FOCUS, focused);
        Kit::of(ui.ctx())
            .editor_frame
            .show(ui, look, egui::Shadow::NONE, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if state.lock().find.open {
                    find_bar::show(ui, state, o.app, id, &mut signals);
                }
                let r = text_area(ui, state, o, id, &mut signals);
                if o.status {
                    status(ui, state);
                }
                r
            })
            .inner
    });
    if o.code {
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        assist_ui::surfaces(ui, state, o.app, id, &font, &mut signals);
        publish_moves(state, o.app);
    }
    if !signals.is_empty() {
        ui.ctx().request_repaint();
    }
    EditorOutput { response, signals }
}

/// Publishes `code.cursor_moved` and `code.selection_changed`.
fn publish_moves(state: &EditorState, app: Option<&AppHandle>) {
    let (id, sel, last) = {
        let mut m = state.lock();
        let sel = m.selection;
        let last = m.view.published.replace(sel);
        (m.id.clone(), sel, last)
    };
    let Some(app) = app else {
        return;
    };
    let Some(last) = last else {
        return;
    };
    if last.head != sel.head {
        app.publish(
            "editor",
            None,
            EVENT_CURSOR_MOVED,
            serde_json::json!({ "editor": id, "line": sel.head.line, "col": sel.head.col }),
        );
    }
    if last != sel && (!sel.is_empty() || !last.is_empty()) {
        app.publish(
            "editor",
            None,
            EVENT_SELECTION_CHANGED,
            serde_json::json!({ "editor": id, "anchor": sel.anchor, "head": sel.head }),
        );
    }
}

/// "Line 3, column 5" under the text.
fn status(ui: &mut Ui, state: &EditorState) {
    let head = state.selection().head;
    let text = tr_args(
        ui.ctx(),
        &s::POSITION,
        &[
            ("line", &(head.line + 1).to_string()),
            ("col", &(head.col + 1).to_string()),
        ],
    );
    let t = Tokens::of(ui.ctx());
    ui.add_space(t.space.xs);
    ui.horizontal(|ui| {
        ui.add_space(t.space.s);
        ui.weak(text);
    });
    ui.add_space(t.space.xs);
}

/// The geometry of one pass.
struct Geometry {
    font: FontId,
    row_h: f32,
    space_w: f32,
    /// The gutter's width and its columns' left edges (relative).
    gutter_w: f32,
    bp_x: f32,
    num_x: f32,
    num_w: f32,
    diag_x: f32,
    fold_x: f32,
    col_w: f32,
    pad: f32,
}

fn geometry(ui: &Ui, o: &Options<'_>, lines: usize) -> Geometry {
    let style = ui.style();
    let font = if o.code {
        egui::TextStyle::Monospace.resolve(style)
    } else {
        egui::TextStyle::Body.resolve(style)
    };
    let (row_h, digit_w, space_w) = ui.ctx().fonts_mut(|f| {
        (
            f.row_height(&font).ceil() + 2.0,
            f.glyph_width(&font, '0'),
            f.glyph_width(&font, ' '),
        )
    });
    let pad = Tokens::of(ui.ctx()).space.s;
    let col_w = (row_h * 0.9).round();
    let digits = lines.max(1).to_string().len().max(2) as f32;
    let mut x = 0.0;
    let bp_x = x;
    if o.gutter_markers {
        x += col_w;
    }
    let num_x = x;
    let num_w = if o.line_numbers {
        digits * digit_w + pad
    } else {
        0.0
    };
    x += num_w;
    let diag_x = x;
    if o.gutter_markers {
        x += col_w;
    }
    let fold_x = x;
    if o.code {
        x += col_w;
    }
    let gutter_w = if x > 0.0 { x + pad * 0.5 } else { 0.0 };
    Geometry {
        font,
        row_h,
        space_w,
        gutter_w,
        bp_x,
        num_x,
        num_w,
        diag_x,
        fold_x,
        col_w,
        pad,
    }
}

/// A line laid out with one colour.
fn plain_galley(painter: &Painter, g: &Geometry, text: &str, color: Color32) -> Arc<Galley> {
    painter.layout_job(LayoutJob::simple_singleline(
        text.to_string(),
        g.font.clone(),
        color,
    ))
}

/// A line laid out with its highlight.
fn styled_galley(
    painter: &Painter,
    g: &Geometry,
    text: &str,
    runs: Option<&Runs>,
    t: &EditorTokens,
) -> Arc<Galley> {
    let plain = t.kind(Kind::Plain);
    let Some(runs) = runs else {
        return plain_galley(painter, g, text, plain);
    };
    let mut job = LayoutJob::default();
    let mut at = 0;
    let fmt = |color| TextFormat::simple(g.font.clone(), color);
    for r in runs.iter() {
        let (a, b) = (r.bytes.start.min(text.len()), r.bytes.end.min(text.len()));
        if a < at || !text.is_char_boundary(a) || !text.is_char_boundary(b) {
            return plain_galley(painter, g, text, plain);
        }
        if a > at {
            job.append(&text[at..a], 0.0, fmt(plain));
        }
        if b > a {
            job.append(&text[a..b], 0.0, fmt(t.kind(r.kind)));
        }
        at = b;
    }
    if at < text.len() {
        job.append(&text[at..], 0.0, fmt(plain));
    }
    painter.layout_job(job)
}

/// The x of a column in a laid-out line.
fn x_of(galley: &Galley, col: usize) -> f32 {
    galley.pos_from_cursor(CCursor::new(col)).min.x
}

/// The text area: keys, drawing, pointer.
fn text_area(
    ui: &mut Ui,
    state: &EditorState,
    o: &Options<'_>,
    id: Id,
    signals: &mut Vec<Signal>,
) -> Response {
    profiling::scope!("pop.editor.text_area");
    let ctx = ui.ctx().clone();
    // Seen by egui and egui-pop as a text input: Escape leaves it, keys
    // that type stay with it, the IME is on.
    if egui::text_edit::TextEditState::load(&ctx, id).is_none() {
        egui::text_edit::TextEditState::default().store(&ctx, id);
    }
    if std::mem::take(&mut state.lock().view.focus) {
        ctx.memory_mut(|m| m.request_focus(id));
    }
    let focused = ctx.memory(|m| m.has_focus(id));
    let assist_shown = o.code && state.lock().assist.showing();
    if assist_shown {
        // Escape closes the assist's surfaces before it leaves the editor.
        focus::escape_passes(&ctx, id);
    }
    let lines = state.line_count();
    let g = geometry(ui, o, lines);
    let height = o
        .height
        .unwrap_or_else(|| ui.available_height())
        .max(g.row_h * 3.0);
    let page = ((height / g.row_h).floor() as usize)
        .saturating_sub(1)
        .max(1);
    if focused {
        ctx.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: assist_shown,
                },
            );
        });
        keys(ui, state, o, &g, page, assist_shown, signals);
    }
    let t = EditorTokens::of(&ctx);
    let (rows, longest) = {
        let mut m = state.lock();
        let Model {
            folds,
            buffer,
            version,
            tab,
            ..
        } = &mut *m;
        folds.update(buffer, *version, *tab);
        let rows = folds.row_count(buffer);
        (rows, m.longest())
    };
    let content = vec2(
        g.gutter_w + g.pad + (longest as f32 + 2.0) * g.space_w.max(1.0) * 1.2,
        rows as f32 * g.row_h + g.row_h,
    );
    let reveal = std::mem::take(&mut state.lock().view.reveal);
    let out = ScrollArea::both()
        .id_salt(id.with("scroll"))
        .max_height(height)
        .min_scrolled_height(height)
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let origin = ui.max_rect().min;
            let full = Rect::from_min_size(origin, content.max(viewport.size()));
            ui.allocate_rect(full, Sense::hover());
            // The widget is what is in view (its accessible bounds too).
            let visible = Rect::from_min_size(origin + viewport.min.to_vec2(), viewport.size());
            let response = ui.interact(visible, id, Sense::click_and_drag());
            if response.clicked() && !response.has_focus() {
                response.request_focus();
            }
            area(
                ui, state, o, &g, &t, id, origin, viewport, reveal, focused, &response, signals,
            );
            response
        });
    let response = out.inner;
    let label = o
        .label
        .clone()
        .unwrap_or_else(|| tr(&ctx, if o.code { &s::CODE } else { &s::TEXT }));
    let value = {
        let m = state.lock();
        if m.buffer.byte_len() <= 64 * 1024 {
            m.buffer.text()
        } else {
            m.buffer.line(m.selection.head.line).to_string()
        }
    };
    let enabled = response.enabled();
    response.widget_info(move || {
        let mut info = WidgetInfo::labeled(WidgetType::TextEdit, enabled, &label);
        info.current_text_value = Some(value.clone());
        info
    });
    ctx.accesskit_node_builder(id, |node| node.set_role(Role::MultilineTextInput));
    response
}

/// Draws the rows in view and the gutter, then handles the pointer.
#[allow(clippy::too_many_arguments)]
fn area(
    ui: &mut Ui,
    state: &EditorState,
    o: &Options<'_>,
    g: &Geometry,
    t: &EditorTokens,
    id: Id,
    origin: Pos2,
    viewport: Rect,
    reveal: bool,
    focused: bool,
    response: &Response,
    signals: &mut Vec<Signal>,
) {
    let painter = ui.painter().clone();
    let text_left = origin.x + g.gutter_w + g.pad;
    let gutter_left = origin.x + viewport.min.x;
    let first = (viewport.min.y / g.row_h).floor().max(0.0) as usize;
    let last = (viewport.max.y / g.row_h).ceil() as usize + 1;
    let mut m = state.lock();
    let rows = m.folds.row_count(&m.buffer);
    let last = last.min(rows);
    let shown: Vec<usize> = (first..last).map(|r| m.folds.line_of(r)).collect();
    // Highlight what is in view, by runs of consecutive lines.
    let mut runs: Vec<Option<Runs>> = Vec::with_capacity(shown.len());
    let mut pending = false;
    if o.code {
        let mut i = 0;
        while i < shown.len() {
            let mut j = i + 1;
            while j < shown.len() && shown[j] == shown[j - 1] + 1 {
                j += 1;
            }
            let range = shown[i]..shown[j - 1] + 1;
            let Model {
                highlighter,
                buffer,
                ..
            } = &mut *m;
            let got = {
                profiling::scope!("pop.editor.highlight");
                highlighter.highlight(buffer, range, o.budget)
            };
            pending |= got.iter().any(Option::is_none);
            runs.extend(got);
            i = j;
        }
    } else {
        runs.resize(shown.len(), None);
    }
    if pending {
        ui.ctx().request_repaint();
    }
    let sel = m.selection;
    let span = sel.span();
    let head = sel.head;
    if o.code || m.find.open {
        m.refresh_matches();
    }
    let brackets = if o.code && focused {
        matching_brackets(&m.buffer, head)
    } else {
        None
    };
    let mut galleys: Vec<Arc<Galley>> = Vec::with_capacity(shown.len());
    let width = viewport.width().max(1.0);
    for (k, &line) in shown.iter().enumerate() {
        let row = first + k;
        let y = origin.y + row as f32 * g.row_h;
        let row_rect = Rect::from_min_size(pos2(gutter_left, y), vec2(width, g.row_h));
        let text = m.buffer.line(line);
        let galley = styled_galley(&painter, g, text, runs[k].as_ref(), t);
        let ty = y + (g.row_h - galley.size().y) * 0.5;
        // The cursor's line.
        if o.code && line == head.line && span.is_empty() {
            painter.rect_filled(row_rect, 0.0, t.current_line);
        }
        // Search matches.
        if m.find.open || o.code {
            let at = m.find.matches.partition_point(|s| s.start.line < line);
            for (i, s) in m.find.matches[at..].iter().enumerate() {
                if s.start.line != line {
                    break;
                }
                let r = Rect::from_min_max(
                    pos2(text_left + x_of(&galley, s.start.col), y + 1.0),
                    pos2(text_left + x_of(&galley, s.end.col), y + g.row_h - 1.0),
                );
                painter.rect_filled(r, 2.0, t.search_match);
                if m.find.open && m.find.current == Some(at + i) {
                    painter.rect_stroke(
                        r,
                        2.0,
                        Stroke::new(1.0, t.match_current),
                        StrokeKind::Inside,
                    );
                }
            }
        }
        // The selection.
        if !span.is_empty() && span.start.line <= line && line <= span.end.line {
            let x0 = if line == span.start.line {
                x_of(&galley, span.start.col)
            } else {
                0.0
            };
            let x1 = if line == span.end.line {
                x_of(&galley, span.end.col)
            } else {
                galley.size().x + g.space_w
            };
            painter.rect_filled(
                Rect::from_min_max(pos2(text_left + x0, y), pos2(text_left + x1, y + g.row_h)),
                0.0,
                t.selection,
            );
        }
        // Indentation guides.
        if o.code {
            let unit = indent_width(&m.indent, m.tab).max(1);
            let level = guide_width(&m.buffer, line, m.tab);
            let mut c = 0;
            while c < level {
                let x = (text_left + c as f32 * g.space_w).round() + 0.5;
                painter.line_segment(
                    [pos2(x, y), pos2(x, y + g.row_h)],
                    Stroke::new(1.0, t.indent_guide),
                );
                c += unit;
            }
        }
        // Matching brackets.
        if let Some((a, b)) = brackets {
            for p in [a, b] {
                if p.line == line {
                    let r = Rect::from_min_max(
                        pos2(text_left + x_of(&galley, p.col), y + 1.0),
                        pos2(text_left + x_of(&galley, p.col + 1), y + g.row_h - 1.0),
                    );
                    painter.rect_stroke(r, 1.0, Stroke::new(1.0, t.bracket), StrokeKind::Inside);
                }
            }
        }
        painter.galley(pos2(text_left, ty), galley.clone(), t.kind(Kind::Plain));
        // Diagnostics underlined.
        for d in m.marks.diagnostics.iter().filter(|d| d.line == line) {
            if let Some((a, b)) = d.columns {
                let x0 = text_left + x_of(&galley, a);
                let x1 = text_left + x_of(&galley, b).max(x_of(&galley, a) + g.space_w);
                squiggle(
                    &painter,
                    x0,
                    x1,
                    y + g.row_h - 2.0,
                    severity_color(t, d.severity),
                );
            }
        }
        // What a fold hides.
        if m.folds.is_folded(line) {
            let r = fold_chip(g, text_left + galley.size().x, y);
            painter.rect_filled(r, 3.0, t.current_line);
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                "⋯",
                g.font.clone(),
                t.fold,
            );
        }
        galleys.push(galley);
    }
    // The cursor.
    m.view.cursor_rect = None;
    if focused {
        let row = m.folds.row_of(head.line);
        if (first..last).contains(&row) && shown.get(row - first) == Some(&head.line) {
            let galley = &galleys[row - first];
            let x = text_left + x_of(galley, head.col);
            let y = origin.y + row as f32 * g.row_h;
            let cursor = Rect::from_min_size(pos2(x - 0.5, y + 1.0), vec2(2.0, g.row_h - 2.0));
            painter.rect_filled(cursor, 0.0, t.cursor);
            if let Some((_, _, ghost)) = &m.assist.ghost {
                let ty = y + (g.row_h - galley.size().y) * 0.5;
                assist_ui::paint_ghost(
                    &painter,
                    pos2(x + 1.0, ty),
                    &ghost.text,
                    g.font.clone(),
                    t.fold,
                );
            }
            m.view.cursor_rect = Some(cursor);
            let to_global = ui
                .ctx()
                .layer_transform_to_global(ui.layer_id())
                .unwrap_or_default();
            let area = ui.clip_rect();
            ui.ctx().output_mut(|out| {
                out.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Normal,
                    rect: to_global * area,
                    cursor_rect: to_global * cursor,
                    should_interrupt_composition: false,
                });
            });
        }
    }
    if reveal {
        let row = m.folds.row_of(head.line);
        let line_text = m.buffer.line(head.line);
        let galley = plain_galley(&painter, g, line_text, Color32::WHITE);
        let x = text_left + x_of(&galley, head.col);
        let y = origin.y + row as f32 * g.row_h;
        let r = Rect::from_min_max(
            pos2(x - g.gutter_w - g.pad - 8.0 * g.space_w, y),
            pos2(x + 8.0 * g.space_w, y + g.row_h),
        );
        ui.scroll_to_rect(r, None);
    }
    // The gutter, over text scrolled under it.
    if g.gutter_w > 0.0 {
        let gutter = Rect::from_min_size(
            pos2(gutter_left, origin.y + viewport.min.y),
            vec2(g.gutter_w, viewport.height()),
        );
        painter.rect_filled(gutter, 0.0, t.gutter);
        for (k, &line) in shown.iter().enumerate() {
            let y = origin.y + (first + k) as f32 * g.row_h;
            let cell =
                |x: f32, w: f32| Rect::from_min_size(pos2(gutter_left + x, y), vec2(w, g.row_h));
            if o.gutter_markers && m.marks.has_breakpoint(line) {
                painter.circle_filled(cell(g.bp_x, g.col_w).center(), g.col_w * 0.3, t.breakpoint);
            }
            if o.line_numbers {
                let color = if line == head.line {
                    t.line_number_active
                } else {
                    t.line_number
                };
                painter.text(
                    pos2(
                        gutter_left + g.num_x + g.num_w - g.pad * 0.5,
                        y + g.row_h * 0.5,
                    ),
                    Align2::RIGHT_CENTER,
                    (line + 1).to_string(),
                    g.font.clone(),
                    color,
                );
            }
            if o.gutter_markers
                && let Some(d) = m.marks.worst(line)
            {
                let icon = match d.severity {
                    Severity::Error => Icon::Error,
                    Severity::Warning => Icon::Warning,
                    _ => Icon::Info,
                };
                let r = cell(g.diag_x, g.col_w).shrink(g.col_w * 0.12);
                icon.paint(&painter, r, severity_color(t, d.severity));
            }
            if o.code && (m.folds.is_folded(line) || fold::foldable(&m.buffer, line, m.tab)) {
                let icon = if m.folds.is_folded(line) {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                };
                icon.paint(
                    &painter,
                    cell(g.fold_x, g.col_w).shrink(g.col_w * 0.2),
                    t.fold,
                );
            }
            if let Some(d) = m.marks.diff_of(line) {
                let x = gutter_left + g.gutter_w - 3.0;
                let color = match d {
                    Diff::Added => t.diff_added,
                    Diff::Modified => t.diff_modified,
                    Diff::Removed => t.diff_removed,
                };
                if d == Diff::Removed {
                    let yb = y + g.row_h;
                    painter.add(Shape::convex_polygon(
                        vec![
                            pos2(x - 2.0, yb - 4.0),
                            pos2(x + 3.0, yb),
                            pos2(x - 2.0, yb + 4.0),
                        ],
                        color,
                        Stroke::NONE,
                    ));
                } else {
                    painter.rect_filled(
                        Rect::from_min_size(pos2(x, y), vec2(3.0, g.row_h)),
                        0.0,
                        color,
                    );
                }
            }
        }
    }
    drop(m);
    pointer(
        ui, state, o, g, id, origin, viewport, &shown, first, &galleys, response, signals,
    );
}

/// The pointer: place the cursor, select, open folds, set breakpoints.
#[allow(clippy::too_many_arguments)]
fn pointer(
    ui: &mut Ui,
    state: &EditorState,
    o: &Options<'_>,
    g: &Geometry,
    id: Id,
    origin: Pos2,
    viewport: Rect,
    shown: &[usize],
    first: usize,
    galleys: &[Arc<Galley>],
    response: &Response,
    signals: &mut Vec<Signal>,
) {
    let gutter_left = origin.x + viewport.min.x;
    let text_left = origin.x + g.gutter_w + g.pad;
    let at = |p: Pos2| -> Option<(usize, usize)> {
        let row = ((p.y - origin.y) / g.row_h).floor();
        if row < 0.0 {
            return shown.first().map(|&l| (0, l));
        }
        let k = (row as usize).checked_sub(first)?;
        let k = k.min(shown.len().checked_sub(1)?);
        Some((k, shown[k]))
    };
    let col_at = |k: usize, x: f32| galleys[k].cursor_from_pos(vec2(x - text_left, 0.0)).index.0;
    // Hover: a diagnostic's message.
    if let Some(p) = response.hover_pos()
        && let Some((_, line)) = at(p)
    {
        let m = state.lock();
        let over_text = m.marks.diagnostics.iter().find(|d| {
            d.line == line
                && (p.x < gutter_left + g.gutter_w
                    || d.columns.is_some_and(|(a, b)| {
                        let k = at(p).map_or(0, |(k, _)| k);
                        let c = col_at(k, p.x);
                        a <= c && c <= b
                    }))
        });
        if let Some(d) = over_text {
            let msg = if d.source.is_empty() {
                d.message.clone()
            } else {
                format!("{}: {}", d.source, d.message)
            };
            drop(m);
            response.clone().on_hover_text_at_pointer(msg);
        }
    }
    if o.code {
        let over = response.hover_pos().and_then(|p| {
            let (k, line) = at(p)?;
            (p.x >= text_left).then(|| Pos::new(line, col_at(k, p.x)))
        });
        assist_ui::hover_pointer(state, o.app, ui, over, response);
    }
    let (pressed, mods) = ui.input(|i| (i.pointer.primary_pressed(), i.modifiers));
    let Some(p) = response.interact_pointer_pos() else {
        return;
    };
    let Some((k, line)) = at(p) else {
        return;
    };
    if pressed && response.is_pointer_button_down_on() {
        response.request_focus();
        if p.x < gutter_left + g.gutter_w {
            gutter_click(state, o, g, p.x - gutter_left, line, signals);
            IN_GUTTER.keyed(id).set(ui.ctx(), true);
            return;
        }
        IN_GUTTER.keyed(id).set(ui.ctx(), false);
        // A click on a fold's marker opens it.
        if state.lock().folds.is_folded(line) {
            let chip = fold_chip(
                g,
                text_left + galleys[k].size().x,
                origin.y + (first + k) as f32 * g.row_h,
            );
            if chip.contains(p) {
                state.unfold(line);
                signals.push(Signal::Fold {
                    line,
                    folded: false,
                });
                return;
            }
        }
        let col = col_at(k, p.x);
        let here = Pos::new(line, col);
        let sel = state.selection();
        state.set_selection(sel.moved(here, mods.shift));
        state.lock().view.column_x = None;
        if o.code && mods.command {
            assist_ui::definition(state, o.app);
        }
        return;
    }
    let in_gutter = IN_GUTTER.keyed(id).get(ui.ctx()).unwrap_or(false);
    if in_gutter {
        return;
    }
    let col = col_at(k, p.x.max(text_left));
    let here = Pos::new(line, col);
    if response.triple_clicked() {
        let end = if line + 1 < state.line_count() {
            Pos::new(line + 1, 0)
        } else {
            Pos::new(line, state.lock().buffer.line_len(line))
        };
        state.set_selection(Selection::new(Pos::new(line, 0), end));
    } else if response.double_clicked() {
        let w = state.lock().buffer.word_at(here);
        state.set_selection(Selection::new(w.start, w.end));
    } else if response.dragged() {
        let sel = state.selection();
        if sel.head != here {
            state.lock().select(sel.moved(here, true));
        }
    }
}

/// A click in the gutter: a breakpoint, a fold, or the line selected.
fn gutter_click(
    state: &EditorState,
    o: &Options<'_>,
    g: &Geometry,
    x: f32,
    line: usize,
    signals: &mut Vec<Signal>,
) {
    if o.gutter_markers && x < g.bp_x + g.col_w {
        let set = state.toggle_breakpoint(line);
        signals.push(Signal::Breakpoint { line, set });
    } else if o.code && x >= g.fold_x && x < g.fold_x + g.col_w {
        let folded = state.lock().folds.is_folded(line);
        let done = if folded {
            state.unfold(line)
        } else {
            state.fold(line)
        };
        if done {
            signals.push(Signal::Fold {
                line,
                folded: !folded,
            });
        }
    } else {
        let end = if line + 1 < state.line_count() {
            Pos::new(line + 1, 0)
        } else {
            Pos::new(line, state.lock().buffer.line_len(line))
        };
        state.set_selection(Selection::new(Pos::new(line, 0), end));
    }
}

fn fold_chip(g: &Geometry, x: f32, y: f32) -> Rect {
    Rect::from_min_size(
        pos2(x + g.space_w, y + 2.0),
        vec2(g.space_w * 3.0, g.row_h - 4.0),
    )
}

fn severity_color(t: &EditorTokens, s: Severity) -> Color32 {
    match s {
        Severity::Error => t.error,
        Severity::Warning => t.warning,
        Severity::Info | Severity::Hint => t.info,
    }
}

/// A wavy underline.
fn squiggle(painter: &Painter, x0: f32, x1: f32, y: f32, color: Color32) {
    let step = 2.0;
    let mut pts = Vec::new();
    let mut x = x0;
    let mut up = true;
    while x <= x1 {
        pts.push(pos2(x, if up { y - 1.0 } else { y + 1.0 }));
        x += step;
        up = !up;
    }
    if pts.len() >= 2 {
        painter.add(Shape::line(pts, Stroke::new(1.0, color)));
    }
}

/// How far guides reach on a line: its indentation, or for a blank
/// line the next non-blank line's.
fn guide_width(b: &Buffer, line: usize, tab: usize) -> usize {
    let text = b.line(line);
    if !text.trim().is_empty() {
        return indent_width(b.indent_of(line), tab);
    }
    let end = (line + 100).min(b.line_count());
    (line + 1..end)
        .find(|&i| !b.line(i).trim().is_empty())
        .map_or(0, |i| indent_width(b.indent_of(i), tab))
}

/// The bracket at or before the cursor and its match.
fn matching_brackets(b: &Buffer, at: Pos) -> Option<(Pos, Pos)> {
    const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
    let try_at = |p: Pos| -> Option<(Pos, Pos)> {
        let c = b.char_at(p)?;
        for (open, close) in PAIRS {
            if c == open {
                return scan(b, p, open, close, true).map(|q| (p, q));
            }
            if c == close {
                return scan(b, p, close, open, false).map(|q| (p, q));
            }
        }
        None
    };
    try_at(at).or_else(|| {
        (at.col > 0)
            .then(|| try_at(Pos::new(at.line, at.col - 1)))
            .flatten()
    })
}

/// Looks for the bracket matching the one at `from`, within 5000 lines.
fn scan(b: &Buffer, from: Pos, this: char, other: char, forward: bool) -> Option<Pos> {
    let mut depth = 0usize;
    let limit = 5000;
    if forward {
        for l in from.line..(from.line + limit).min(b.line_count()) {
            let start = if l == from.line { from.col } else { 0 };
            for (c, ch) in b.line(l).chars().enumerate().skip(start) {
                if ch == this {
                    depth += 1;
                } else if ch == other {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Pos::new(l, c));
                    }
                }
            }
        }
    } else {
        let lo = from.line.saturating_sub(limit);
        for l in (lo..=from.line).rev() {
            let chars: Vec<char> = b.line(l).chars().collect();
            let end = if l == from.line {
                from.col + 1
            } else {
                chars.len()
            };
            for c in (0..end.min(chars.len())).rev() {
                let ch = chars[c];
                if ch == this {
                    depth += 1;
                } else if ch == other {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Pos::new(l, c));
                    }
                }
            }
        }
    }
    None
}

/// Applies an edit, reporting what happened.
fn edit(
    state: &EditorState,
    o: &Options<'_>,
    changes: Vec<Change>,
    kind: EditKind,
    after: Option<Selection>,
    signals: &mut Vec<Signal>,
) -> bool {
    if state.lock().read_only {
        return false;
    }
    match state.edit(o.app, changes, Why::Edit(kind), after, o.origin()) {
        Ok(version) => {
            signals.push(Signal::Changed {
                version,
                kind: kind.label().into(),
            });
            true
        }
        Err(reason) => {
            signals.push(Signal::Refused {
                step: EDIT_CHANGE,
                reason,
            });
            false
        }
    }
}

/// The keyboard.
fn keys(
    ui: &mut Ui,
    state: &EditorState,
    o: &Options<'_>,
    g: &Geometry,
    page: usize,
    assist_shown: bool,
    signals: &mut Vec<Signal>,
) {
    let events = ui.input(|i| {
        i.filtered_events(&EventFilter {
            tab: true,
            horizontal_arrows: true,
            vertical_arrows: true,
            escape: assist_shown,
        })
    });
    for ev in events {
        match ev {
            Event::Text(text) if !text.is_empty() => {
                let kind = if text.chars().count() > 1 {
                    EditKind::Paste
                } else {
                    EditKind::Typing
                };
                let span = state.selection().span();
                let changed = edit(
                    state,
                    o,
                    vec![Change::new(span.start, span.end, text.clone())],
                    kind,
                    None,
                    signals,
                );
                if changed && o.code {
                    assist_ui::typed(state, o.app, &text);
                }
            }
            Event::Ime(ImeEvent::Commit(text)) if !text.is_empty() => {
                let span = state.selection().span();
                edit(
                    state,
                    o,
                    vec![Change::new(span.start, span.end, text)],
                    EditKind::Typing,
                    None,
                    signals,
                );
            }
            Event::Copy => {
                let text = copy_text(state);
                ui.ctx().copy_text(text);
            }
            Event::Cut => {
                let text = copy_text(state);
                ui.ctx().copy_text(text);
                let span = cut_span(state);
                edit(
                    state,
                    o,
                    vec![Change::delete(span)],
                    EditKind::Cut,
                    None,
                    signals,
                );
            }
            Event::Paste(text) if !text.is_empty() => {
                let span = state.selection().span();
                let text = text.replace("\r\n", "\n");
                edit(
                    state,
                    o,
                    vec![Change::new(span.start, span.end, text)],
                    EditKind::Paste,
                    None,
                    signals,
                );
            }
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                if o.code && !modifiers.any() && assist_ui::key(state, o.app, key, signals) {
                    continue;
                }
                if o.code && key != Key::Escape {
                    state.lock().assist.ghost = None;
                }
                key_press(ui, state, o, g, page, key, modifiers, signals);
            }
            _ => {}
        }
    }
}

/// What Copy takes: the selection, or the cursor's line when nothing is
/// selected.
fn copy_text(state: &EditorState) -> String {
    let m = state.lock();
    if m.selection.is_empty() {
        format!("{}\n", m.buffer.line(m.selection.head.line))
    } else {
        m.buffer.slice(m.selection.span())
    }
}

/// What Cut removes: the selection, or the cursor's whole line.
fn cut_span(state: &EditorState) -> Span {
    let m = state.lock();
    if m.selection.is_empty() {
        let l = m.selection.head.line;
        if l + 1 < m.buffer.line_count() {
            Span::new(Pos::new(l, 0), Pos::new(l + 1, 0))
        } else if l > 0 {
            Span::new(
                Pos::new(l - 1, m.buffer.line_len(l - 1)),
                Pos::new(l, m.buffer.line_len(l)),
            )
        } else {
            Span::new(Pos::new(0, 0), Pos::new(0, m.buffer.line_len(0)))
        }
    } else {
        m.selection.span()
    }
}

/// One key.
#[allow(clippy::too_many_arguments)]
fn key_press(
    ui: &mut Ui,
    state: &EditorState,
    o: &Options<'_>,
    g: &Geometry,
    page: usize,
    key: Key,
    mods: Modifiers,
    signals: &mut Vec<Signal>,
) {
    let cmd = mods.command || mods.ctrl;
    let word = cmd || mods.alt;
    let shift = mods.shift;
    // Without an application, the editor's own shortcuts (with one, they
    // are commands bound in the keymap, rebindable).
    if o.app.is_none() {
        match (key, cmd, shift) {
            (Key::F, true, false) => return steps::open_find(state, false),
            (Key::H, true, false) => return steps::open_find(state, true),
            (Key::G, true, false) => return steps::open_goto(state),
            (Key::F3, false, s) => {
                steps::find_step(state, !s);
                return;
            }
            (Key::S, true, false) => {
                save(state, o, signals);
                return;
            }
            (Key::F9, false, false) if o.gutter_markers => {
                let line = state.selection().head.line;
                let set = state.toggle_breakpoint(line);
                signals.push(Signal::Breakpoint { line, set });
                return;
            }
            (Key::Space, true, false) if o.code => return assist_ui::complete(state, None),
            (Key::Period, true, false) if o.code => return assist_ui::code_actions(state, None),
            (Key::F12, false, false) if o.code => return assist_ui::definition(state, None),
            (Key::OpenBracket, true, true) if o.code => {
                steps::fold_at(state, state.selection().head.line);
                return;
            }
            (Key::CloseBracket, true, true) if o.code => {
                steps::unfold_at(state, state.selection().head.line);
                return;
            }
            _ => {}
        }
    }
    let (sel, buffer_end) = {
        let m = state.lock();
        (m.selection, m.buffer.end())
    };
    let head = sel.head;
    let span = sel.span();
    let move_to = |to: Pos, keep_x: bool| {
        let mut m = state.lock();
        m.history.seal();
        if !keep_x {
            m.view.column_x = None;
        }
        let s = m.selection.moved(to, shift);
        m.select(s);
    };
    match key {
        Key::Z if cmd => {
            let r = if shift {
                state.redo(o.app, o.origin())
            } else {
                state.undo(o.app, o.origin())
            };
            report(
                r.map(|v| v.map(|v| (v, if shift { "redo" } else { "undo" }))),
                signals,
            );
        }
        Key::Y if cmd => {
            report(
                state
                    .redo(o.app, o.origin())
                    .map(|v| v.map(|v| (v, "redo"))),
                signals,
            );
        }
        Key::A if cmd => {
            state.set_selection(Selection::new(Pos::default(), buffer_end));
        }
        Key::ArrowLeft => {
            let to = if !span.is_empty() && !shift && !word {
                span.start
            } else {
                let m = state.lock();
                if word {
                    m.buffer.word_left(head)
                } else {
                    m.buffer.prev(head)
                }
            };
            move_to(to, false);
        }
        Key::ArrowRight => {
            let to = if !span.is_empty() && !shift && !word {
                span.end
            } else {
                let m = state.lock();
                if word {
                    m.buffer.word_right(head)
                } else {
                    m.buffer.next(head)
                }
            };
            move_to(to, false);
        }
        Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown => {
            let rows = match key {
                Key::PageUp | Key::PageDown => page,
                _ => 1,
            };
            let up = matches!(key, Key::ArrowUp | Key::PageUp);
            let to = vertical(ui, state, g, rows, up);
            move_to(to, true);
        }
        Key::Home => {
            let to = if cmd {
                Pos::default()
            } else {
                let m = state.lock();
                let first = m.buffer.indent_of(head.line).chars().count();
                Pos::new(head.line, if head.col == first { 0 } else { first })
            };
            move_to(to, false);
        }
        Key::End => {
            let to = if cmd {
                buffer_end
            } else {
                Pos::new(head.line, state.lock().buffer.line_len(head.line))
            };
            move_to(to, false);
        }
        Key::Backspace => {
            let del = if !span.is_empty() {
                span
            } else {
                let m = state.lock();
                if word {
                    Span::new(m.buffer.word_left(head), head)
                } else if o.code
                    && head.col > 0
                    && m.buffer.line(head.line)[..byte_at(m.buffer.line(head.line), head.col)]
                        .chars()
                        .all(|c| c == ' ')
                {
                    // Soft tabs: back to the previous indentation stop.
                    let unit = m.indent.chars().count().max(1);
                    let back = (head.col - 1) % unit + 1;
                    Span::new(Pos::new(head.line, head.col - back), head)
                } else {
                    Span::new(m.buffer.prev(head), head)
                }
            };
            if !del.is_empty() {
                edit(
                    state,
                    o,
                    vec![Change::delete(del)],
                    EditKind::Deletion,
                    None,
                    signals,
                );
            }
        }
        Key::Delete => {
            let del = if !span.is_empty() {
                span
            } else {
                let m = state.lock();
                if word {
                    Span::new(head, m.buffer.word_right(head))
                } else {
                    Span::new(head, m.buffer.next(head))
                }
            };
            if !del.is_empty() {
                edit(
                    state,
                    o,
                    vec![Change::delete(del)],
                    EditKind::Deletion,
                    None,
                    signals,
                );
            }
        }
        Key::Enter => {
            let text = {
                let m = state.lock();
                if o.code {
                    let line = m.buffer.line(head.line);
                    let before = &line[..byte_at(line, head.col)];
                    let mut indent = m.buffer.indent_of(head.line).to_string();
                    if indent.len() > before.len() {
                        indent.truncate(before.len());
                    }
                    if before.trim_end().ends_with(['{', '(', '[', ':']) {
                        indent.push_str(&m.indent);
                    }
                    format!("\n{indent}")
                } else {
                    "\n".to_string()
                }
            };
            edit(
                state,
                o,
                vec![Change::new(span.start, span.end, text)],
                EditKind::Typing,
                None,
                signals,
            );
        }
        Key::Tab => {
            if span.start.line != span.end.line || shift {
                indent_lines(state, o, !shift, signals);
            } else {
                let unit = state.lock().indent.clone();
                edit(
                    state,
                    o,
                    vec![Change::new(span.start, span.end, unit)],
                    EditKind::Typing,
                    None,
                    signals,
                );
            }
        }
        _ => {}
    }
}

/// Moves up or down by display rows, keeping the column's x.
fn vertical(ui: &Ui, state: &EditorState, g: &Geometry, rows: usize, up: bool) -> Pos {
    let painter = ui.painter();
    let mut m = state.lock();
    let head = m.selection.head;
    let x = match m.view.column_x {
        Some(x) => x,
        None => {
            let gal = plain_galley(painter, g, m.buffer.line(head.line), Color32::WHITE);
            let x = x_of(&gal, head.col);
            m.view.column_x = Some(x);
            x
        }
    };
    let row = m.folds.row_of(head.line);
    let count = m.folds.row_count(&m.buffer);
    let target = if up {
        if row == 0 {
            return Pos::new(0, 0);
        }
        row.saturating_sub(rows)
    } else {
        if row + 1 >= count {
            let last = m.buffer.line_count() - 1;
            return Pos::new(last, m.buffer.line_len(last));
        }
        (row + rows).min(count - 1)
    };
    let line = m.folds.line_of(target);
    let gal = plain_galley(painter, g, m.buffer.line(line), Color32::WHITE);
    Pos::new(line, gal.cursor_from_pos(vec2(x, 0.0)).index.0)
}

/// Indents or outdents the selected lines.
fn indent_lines(state: &EditorState, o: &Options<'_>, more: bool, signals: &mut Vec<Signal>) {
    let (changes, after) = {
        let m = state.lock();
        let span = m.selection.span();
        let last = if span.end.col == 0 && span.end.line > span.start.line {
            span.end.line - 1
        } else {
            span.end.line
        };
        let unit = m.indent.clone();
        let n = unit.chars().count();
        let mut changes = Vec::new();
        let mut delta = std::collections::HashMap::new();
        for l in span.start.line..=last {
            if more {
                if !m.buffer.line(l).is_empty() {
                    changes.push(Change::insert(Pos::new(l, 0), unit.clone()));
                    delta.insert(l, n as isize);
                }
            } else {
                let lead = m.buffer.indent_of(l);
                let take = if lead.starts_with('\t') {
                    1
                } else {
                    lead.chars().take(n).take_while(|c| *c == ' ').count()
                };
                if take > 0 {
                    changes.push(Change::delete(Span::new(Pos::new(l, 0), Pos::new(l, take))));
                    delta.insert(l, -(take as isize));
                }
            }
        }
        let shift = |p: Pos| {
            let d = delta.get(&p.line).copied().unwrap_or(0);
            Pos::new(p.line, (p.col as isize + d).max(0) as usize)
        };
        let sel = m.selection;
        (changes, Selection::new(shift(sel.anchor), shift(sel.head)))
    };
    if !changes.is_empty() {
        edit(state, o, changes, EditKind::Indent, Some(after), signals);
    }
}

/// Saves through `edit.save`.
pub(crate) fn save(state: &EditorState, o: &Options<'_>, signals: &mut Vec<Signal>) {
    match state.save(o.app, o.origin()) {
        Ok(version) => signals.push(Signal::Saved { version }),
        Err(reason) => signals.push(Signal::Refused {
            step: EDIT_SAVE,
            reason,
        }),
    }
}

fn report(r: Result<Option<(u64, &str)>, String>, signals: &mut Vec<Signal>) {
    match r {
        Ok(Some((version, kind))) => signals.push(Signal::Changed {
            version,
            kind: kind.into(),
        }),
        Ok(None) => {}
        Err(reason) => signals.push(Signal::Refused {
            step: EDIT_CHANGE,
            reason,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_match_across_lines() {
        let b = Buffer::from_text("fn a() {\n    (x[1])\n}");
        assert_eq!(
            matching_brackets(&b, Pos::new(0, 7)),
            Some((Pos::new(0, 7), Pos::new(2, 0)))
        );
        assert_eq!(
            matching_brackets(&b, Pos::new(1, 10)),
            Some((Pos::new(1, 9), Pos::new(1, 4)))
        );
        assert_eq!(matching_brackets(&b, Pos::new(1, 0)), None);
    }

    #[test]
    fn blank_lines_take_the_next_lines_guides() {
        let b = Buffer::from_text("a\n\n        b");
        assert_eq!(guide_width(&b, 1, 4), 8);
    }
}
