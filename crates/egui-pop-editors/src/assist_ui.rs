//! The assist's side in the editor: asking the provider, collecting its
//! answers, the keys of the popups, and the surfaces (the completion
//! list, the hover card, the signature card, the actions list, ghost
//! text), themed from the tokens.

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Id, Key, Order, Pos2, RichText, Stroke, Ui, vec2};
use egui_pop_api::AppHandle;
use egui_pop_core::theme::Tokens;
use serde_json::json;

use crate::assist::{
    self, AssistProvider, CodeAction, CompletionItem, Hover, Pending, Reply, Request, RequestId,
    SignatureHelp, Suggestion, Trigger,
};
use crate::buffer::{Change, Pos, Span};
use crate::history::EditKind;
use crate::model::{EditorState, Why};
use crate::selection::Selection;
use crate::steps::{
    self, CODE_ACCEPT_SUGGESTION, CODE_INSERT_COMPLETION, EVENT_COMPLETION_ACCEPTED,
    EVENT_COMPLETION_REQUESTED,
};
use crate::view::Signal;

/// The completion list on screen.
#[derive(Debug, Default)]
pub(crate) struct Completion {
    pub items: Vec<CompletionItem>,
    /// The items matching what is typed, in order.
    pub shown: Vec<usize>,
    pub selected: usize,
    /// Where the word being completed starts.
    pub start: Pos,
}

/// The code actions list on screen.
#[derive(Debug, Default)]
pub(crate) struct Actions {
    pub items: Vec<CodeAction>,
    pub selected: usize,
}

/// The assist's state in an editor.
#[derive(Default)]
pub(crate) struct AssistState {
    pub provider: Option<Box<dyn AssistProvider>>,
    next: u64,
    pub completion: Option<Completion>,
    completion_req: Option<Pending<Vec<CompletionItem>>>,
    pub hover: Option<(Pos, Hover)>,
    hover_req: Option<Pending<Option<Hover>>>,
    /// Where hover was last asked for.
    pub hover_asked: Option<Pos>,
    pub signature: Option<SignatureHelp>,
    signature_req: Option<Pending<Option<SignatureHelp>>>,
    pub actions: Option<Actions>,
    actions_req: Option<Pending<Vec<CodeAction>>>,
    definition_req: Option<Pending<Vec<assist::Location>>>,
    /// Ghost text: where, for which version, what.
    pub ghost: Option<(Pos, u64, Suggestion)>,
    ghost_req: Option<Pending<Option<Suggestion>>>,
}

impl std::fmt::Debug for AssistState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssistState")
            .field("provider", &self.provider.as_ref().map(|p| p.name()))
            .finish()
    }
}

impl AssistState {
    /// Whether a surface that Escape closes is shown.
    pub fn showing(&self) -> bool {
        self.completion.is_some()
            || self.actions.is_some()
            || self.ghost.is_some()
            || self.signature.is_some()
            || self.hover.is_some()
    }
}

/// Which request to make.
enum Ask {
    Complete(Trigger),
    Hover(Pos),
    Signature,
    Actions,
    Definition,
    Inline,
}

/// Asks the provider, with no lock held while it runs.
fn ask(state: &EditorState, app: Option<&AppHandle>, what: Ask) {
    let ctx = app.map(AppHandle::egui);
    let (mut provider, req) = {
        let mut m = state.lock();
        let Some(p) = m.assist.provider.take() else {
            return;
        };
        m.assist.next += 1;
        let at = match what {
            Ask::Hover(p) => p,
            Ask::Actions => m.selection.span().start,
            _ => m.selection.head,
        };
        let trigger = match what {
            Ask::Complete(t) => t,
            _ => Trigger::Explicit,
        };
        let req = Request {
            id: RequestId(m.assist.next),
            editor: m.id.clone(),
            state: state.clone(),
            version: m.version,
            at,
            trigger,
        };
        (p, req)
    };
    let span = state.selection().span();
    let mut cancelled = Vec::new();
    macro_rules! go {
        ($slot:ident, $call:expr) => {{
            let (pending, reply) = assist::pair(req.id, req.version, req.at, ctx.clone());
            if let Some(old) = state.lock().assist.$slot.take() {
                old.cancel();
                cancelled.push(old.id);
            }
            for id in cancelled.drain(..) {
                provider.cancel(id);
            }
            $call(&mut *provider, &req, reply);
            state.lock().assist.$slot = Some(pending);
        }};
    }
    match what {
        Ask::Complete(trigger) => {
            go!(
                completion_req,
                |p: &mut dyn AssistProvider, r: &Request, reply: Reply<_>| p.complete(r, reply)
            );
            if let Some(app) = app {
                app.publish(
                    "editor",
                    None,
                    EVENT_COMPLETION_REQUESTED,
                    json!({ "editor": req.editor, "id": req.id.0, "trigger": trigger, "at": req.at }),
                );
            }
        }
        Ask::Hover(_) => go!(hover_req, |p: &mut dyn AssistProvider,
                                         r: &Request,
                                         reply| p
            .hover(r, reply)),
        Ask::Signature => {
            go!(signature_req, |p: &mut dyn AssistProvider,
                                r: &Request,
                                reply| p
                .signature_help(r, reply))
        }
        Ask::Actions => {
            go!(actions_req, |p: &mut dyn AssistProvider,
                              r: &Request,
                              reply| p
                .code_actions(r, span, reply))
        }
        Ask::Definition => {
            go!(definition_req, |p: &mut dyn AssistProvider,
                                 r: &Request,
                                 reply| p
                .definition(r, reply))
        }
        Ask::Inline => go!(ghost_req, |p: &mut dyn AssistProvider,
                                       r: &Request,
                                       reply| p
            .inline_suggestion(r, reply)),
    }
    state.lock().assist.provider = Some(provider);
}

/// Asks for completion (Ctrl+Space, the `code.complete` command).
pub(crate) fn complete(state: &EditorState, app: Option<&AppHandle>) {
    ask(state, app, Ask::Complete(Trigger::Explicit));
}

pub(crate) fn code_actions(state: &EditorState, app: Option<&AppHandle>) {
    ask(state, app, Ask::Actions);
}

pub(crate) fn definition(state: &EditorState, app: Option<&AppHandle>) {
    ask(state, app, Ask::Definition);
}

pub(crate) fn hover_at(state: &EditorState, app: Option<&AppHandle>, at: Pos) {
    state.lock().assist.hover_asked = Some(at);
    ask(state, app, Ask::Hover(at));
}

/// After a character was typed: completion, signature help, ghost text.
pub(crate) fn typed(state: &EditorState, app: Option<&AppHandle>, text: &str) {
    let (has, triggers, sig, while_typing, inline, popup) = {
        let mut m = state.lock();
        m.assist.ghost = None;
        match &m.assist.provider {
            Some(p) => (
                true,
                p.trigger_characters(),
                p.signature_characters(),
                p.complete_while_typing(),
                p.suggests_inline(),
                m.assist.completion.is_some(),
            ),
            None => return,
        }
    };
    if !has {
        return;
    }
    let Some(ch) = text.chars().last() else {
        return;
    };
    if ch == ')' {
        state.lock().assist.signature = None;
    }
    if sig.contains(&ch) {
        ask(state, app, Ask::Signature);
    }
    let word = ch.is_alphanumeric() || ch == '_';
    if triggers.contains(&ch) {
        ask(state, app, Ask::Complete(Trigger::Character(ch)));
    } else if word && while_typing && !popup {
        ask(state, app, Ask::Complete(Trigger::Typing));
    } else if !word {
        state.lock().assist.completion = None;
    }
    if inline {
        ask(state, app, Ask::Inline);
    }
}

/// Where the word before a position starts.
fn word_start(b: &crate::buffer::Buffer, at: Pos) -> Pos {
    let chars: Vec<char> = b.line(at.line).chars().collect();
    let mut c = at.col.min(chars.len());
    while c > 0 && (chars[c - 1].is_alphanumeric() || chars[c - 1] == '_') {
        c -= 1;
    }
    Pos::new(at.line, c)
}

/// Collects answers and keeps the surfaces in step with the cursor.
pub(crate) fn poll(state: &EditorState, signals: &mut Vec<Signal>) {
    let mut m = state.lock();
    let head = m.selection.head;
    let version = m.version;
    let crate::model::Model {
        assist: a, buffer, ..
    } = &mut *m;
    if let Some(p) = &a.completion_req
        && let Some(items) = p.take()
    {
        let at = p.at;
        a.completion_req = None;
        if items.is_empty() || head.line != at.line || head < at {
            a.completion = None;
        } else {
            let start = items
                .iter()
                .find_map(|i| i.replace.map(|s| s.start))
                .unwrap_or_else(|| word_start(buffer, at));
            a.completion = Some(Completion {
                items,
                shown: Vec::new(),
                selected: 0,
                start,
            });
        }
    }
    if let Some(p) = &a.hover_req
        && let Some(h) = p.take()
    {
        let at = p.at;
        a.hover_req = None;
        a.hover = h.map(|h| (at, h));
    }
    if let Some(p) = &a.signature_req
        && let Some(s) = p.take()
    {
        a.signature_req = None;
        a.signature = s.filter(|s| !s.signatures.is_empty());
    }
    if let Some(p) = &a.actions_req
        && let Some(items) = p.take()
    {
        a.actions_req = None;
        let mut items = items;
        items.sort_by_key(|i| !i.preferred);
        a.actions = (!items.is_empty()).then_some(Actions { items, selected: 0 });
    }
    if let Some(p) = &a.ghost_req
        && let Some(g) = p.take()
    {
        let (at, v) = (p.at, p.version);
        a.ghost_req = None;
        a.ghost = g
            .filter(|g| !g.text.is_empty() && at == head && v == version)
            .map(|g| (at, v, g));
    }
    let defs = a.definition_req.as_ref().and_then(Pending::take);
    if defs.is_some() {
        a.definition_req = None;
    }
    // Ghost text lives as long as its place and version.
    if let Some((at, v, _)) = &a.ghost
        && (*at != head || *v != version)
    {
        a.ghost = None;
    }
    // Completion filters on the word typed since it opened.
    let id = m.id.clone();
    let line_text = m.buffer.line(head.line).to_string();
    if let Some(c) = &mut m.assist.completion {
        if head.line != c.start.line || head.col < c.start.col {
            m.assist.completion = None;
        } else {
            let prefix: String = line_text
                .chars()
                .skip(c.start.col)
                .take(head.col - c.start.col)
                .collect();
            c.shown = assist::filter(&c.items, &prefix);
            if c.shown.is_empty() {
                m.assist.completion = None;
            } else {
                c.selected = c.selected.min(c.shown.len() - 1);
            }
        }
    }
    if let Some(sig_line) = m.assist.signature.as_ref().map(|_| head.line)
        && m.view.published.is_some_and(|p| p.head.line != sig_line)
    {
        m.assist.signature = None;
    }
    drop(m);
    if let Some(defs) = defs
        && let Some(loc) = defs.into_iter().next()
    {
        if loc.editor == id {
            state.set_selection(Selection::new(loc.span.start, loc.span.end));
        }
        signals.push(Signal::Definition { location: loc });
    }
}

/// The keys of the assist's surfaces; whether the key was taken.
pub(crate) fn key(
    state: &EditorState,
    app: Option<&AppHandle>,
    key: Key,
    signals: &mut Vec<Signal>,
) -> bool {
    let mut m = state.lock();
    if let Some(a) = &mut m.assist.actions {
        let n = a.items.len();
        match key {
            Key::ArrowDown => a.selected = (a.selected + 1) % n,
            Key::ArrowUp => a.selected = (a.selected + n - 1) % n,
            Key::Enter | Key::Tab => {
                let action = a.items[a.selected].clone();
                m.assist.actions = None;
                drop(m);
                apply_action(state, app, action, signals);
            }
            Key::Escape => m.assist.actions = None,
            _ => return false,
        }
        return true;
    }
    if let Some(c) = &mut m.assist.completion {
        let n = c.shown.len().max(1);
        match key {
            Key::ArrowDown => c.selected = (c.selected + 1) % n,
            Key::ArrowUp => c.selected = (c.selected + n - 1) % n,
            Key::PageDown => c.selected = (c.selected + 8).min(n - 1),
            Key::PageUp => c.selected = c.selected.saturating_sub(8),
            Key::Enter | Key::Tab => {
                let Some(&i) = c.shown.get(c.selected) else {
                    return false;
                };
                let item = c.items[i].clone();
                let start = c.start;
                m.assist.completion = None;
                drop(m);
                accept_completion(state, app, item, start, signals);
            }
            Key::Escape => m.assist.completion = None,
            _ => return false,
        }
        return true;
    }
    if m.assist.ghost.is_some() {
        match key {
            Key::Tab => {
                let (at, _, g) = m.assist.ghost.take().expect("checked");
                drop(m);
                accept_suggestion(state, app, at, g, signals);
                return true;
            }
            Key::Escape => {
                m.assist.ghost = None;
                return true;
            }
            _ => {}
        }
    }
    if key == Key::Escape && (m.assist.signature.is_some() || m.assist.hover.is_some()) {
        m.assist.signature = None;
        m.assist.hover = None;
        return true;
    }
    false
}

fn origin() -> &'static str {
    "user"
}

/// Inserts a completion through `code.insert_completion`.
pub(crate) fn accept_completion(
    state: &EditorState,
    app: Option<&AppHandle>,
    item: CompletionItem,
    start: Pos,
    signals: &mut Vec<Signal>,
) {
    let (editor, head) = {
        let m = state.lock();
        (m.id.clone(), m.selection.head)
    };
    let span = item.replace.unwrap_or(Span::new(start, head));
    let args = json!({
        "editor": editor,
        "item": item,
        "from": span.start,
        "to": span.end,
        "text": item.text(),
    });
    let me = state.clone();
    let outcome = steps::run(app, &CODE_INSERT_COMPLETION, args, |args| {
        let from = serde_json::from_value(args["from"].clone()).map_err(|e| e.to_string())?;
        let to = serde_json::from_value(args["to"].clone()).map_err(|e| e.to_string())?;
        let text = args["text"].as_str().unwrap_or("").to_string();
        let v = me.edit(
            app,
            vec![Change::new(from, to, text)],
            Why::Edit(EditKind::Completion),
            None,
            origin(),
        )?;
        Ok(json!({ "version": v }))
    });
    finish(app, outcome, &CODE_INSERT_COMPLETION, signals, |app| {
        app.publish(
            "editor",
            None,
            EVENT_COMPLETION_ACCEPTED,
            json!({ "editor": editor, "label": item.label, "text": item.text() }),
        );
    });
}

/// Inserts ghost text through `code.accept_suggestion`.
fn accept_suggestion(
    state: &EditorState,
    app: Option<&AppHandle>,
    at: Pos,
    g: Suggestion,
    signals: &mut Vec<Signal>,
) {
    let args = json!({ "editor": state.id(), "at": at, "text": g.text });
    let me = state.clone();
    let outcome = steps::run(app, &CODE_ACCEPT_SUGGESTION, args, |args| {
        let at: Pos = serde_json::from_value(args["at"].clone()).map_err(|e| e.to_string())?;
        let text = args["text"].as_str().unwrap_or("").to_string();
        let v = me.edit(
            app,
            vec![Change::insert(at, text)],
            Why::Edit(EditKind::Completion),
            None,
            origin(),
        )?;
        Ok(json!({ "version": v }))
    });
    finish(app, outcome, &CODE_ACCEPT_SUGGESTION, signals, |_| {});
}

/// Applies a code action as one edit.
fn apply_action(
    state: &EditorState,
    app: Option<&AppHandle>,
    action: CodeAction,
    signals: &mut Vec<Signal>,
) {
    match state.edit(
        app,
        action.changes,
        Why::Edit(EditKind::External),
        None,
        origin(),
    ) {
        Ok(version) => signals.push(Signal::Changed {
            version,
            kind: "action".into(),
        }),
        Err(reason) => signals.push(Signal::Refused {
            step: steps::EDIT_CHANGE,
            reason,
        }),
    }
}

fn finish(
    app: Option<&AppHandle>,
    outcome: egui_pop_api::steps::Outcome,
    step: &egui_pop_model::ids::StepId,
    signals: &mut Vec<Signal>,
    then: impl FnOnce(&AppHandle),
) {
    use egui_pop_api::steps::Outcome;
    match outcome {
        Outcome::Done(Ok(v)) => {
            signals.push(Signal::Changed {
                version: v["version"].as_u64().unwrap_or(0),
                kind: "completion".into(),
            });
            if let Some(app) = app {
                then(app);
            }
        }
        Outcome::Done(Err(reason)) | Outcome::Vetoed { reason, .. } => {
            signals.push(Signal::Refused {
                step: step.clone(),
                reason,
            })
        }
    }
}

/// The ghost text's first line, laid out to draw after the cursor.
pub(crate) fn ghost_job(text: &str, font: egui::FontId, color: egui::Color32) -> LayoutJob {
    let first = text.split('\n').next().unwrap_or("");
    let more = text.contains('\n');
    let mut job = LayoutJob::default();
    let fmt = TextFormat {
        font_id: font,
        color,
        italics: true,
        ..TextFormat::default()
    };
    job.append(first, 0.0, fmt.clone());
    if more {
        job.append(" …", 0.0, fmt);
    }
    job
}

/// A themed card (the popups' frame).
fn card(ui: &Ui) -> egui::Frame {
    let t = Tokens::of(ui.ctx());
    egui::Frame::new()
        .fill(t.surface.overlay)
        .stroke(Stroke::new(1.0, t.border.strong))
        .corner_radius(t.radius.widget)
        .inner_margin(t.space.xs)
        .shadow(ui.visuals().popup_shadow)
}

/// Draws the completion list, the actions list, and the cards.
pub(crate) fn surfaces(
    ui: &Ui,
    state: &EditorState,
    app: Option<&AppHandle>,
    id: Id,
    font: &egui::FontId,
    signals: &mut Vec<Signal>,
) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let Some(cursor) = state.lock().view.cursor_rect else {
        return;
    };
    let below = cursor.left_bottom() + vec2(0.0, 2.0);
    let mut accept: Option<(CompletionItem, Pos)> = None;
    let mut run_action: Option<CodeAction> = None;
    {
        let m = state.lock();
        if let Some(c) = &m.assist.completion {
            let rows: Vec<(usize, String, String)> = c
                .shown
                .iter()
                .take(100)
                .map(|&i| (i, c.items[i].label.clone(), c.items[i].detail.clone()))
                .collect();
            let selected = c.selected;
            let start = c.start;
            let items = c.items.clone();
            drop(m);
            egui::Area::new(id.with("completion"))
                .order(Order::Foreground)
                .fixed_pos(below)
                .show(&ctx, |ui| {
                    card(ui).show(ui, |ui| {
                        ui.set_min_width(220.0);
                        egui::ScrollArea::vertical()
                            .max_height(ui.spacing().interact_size.y * 8.0)
                            .show(ui, |ui| {
                                for (k, (i, label, detail)) in rows.iter().enumerate() {
                                    let r = ui.horizontal(|ui| {
                                        let r = ui.selectable_label(
                                            k == selected,
                                            RichText::new(label).font(font.clone()),
                                        );
                                        if !detail.is_empty() {
                                            ui.weak(detail);
                                        }
                                        r
                                    });
                                    if k == selected {
                                        r.inner.scroll_to_me(None);
                                    }
                                    if r.inner.clicked() {
                                        accept = Some((items[*i].clone(), start));
                                    }
                                }
                            });
                    });
                });
        } else if let Some(a) = &m.assist.actions {
            let titles: Vec<String> = a.items.iter().map(|i| i.title.clone()).collect();
            let selected = a.selected;
            let items = a.items.clone();
            drop(m);
            egui::Area::new(id.with("actions"))
                .order(Order::Foreground)
                .fixed_pos(below)
                .show(&ctx, |ui| {
                    card(ui).show(ui, |ui| {
                        for (k, title) in titles.iter().enumerate() {
                            if ui.selectable_label(k == selected, title).clicked() {
                                run_action = Some(items[k].clone());
                            }
                        }
                    });
                });
        }
    }
    if let Some((item, start)) = accept {
        state.lock().assist.completion = None;
        accept_completion(state, app, item, start, signals);
        state.request_focus();
    }
    if let Some(action) = run_action {
        state.lock().assist.actions = None;
        apply_action(state, app, action, signals);
        state.request_focus();
    }
    let m = state.lock();
    if let Some(sig) = &m.assist.signature
        && let Some(s) = sig.signatures.get(sig.active)
    {
        let mut job = LayoutJob::default();
        let normal = TextFormat::simple(font.clone(), t.text.default);
        let strong = TextFormat {
            color: t.accent.default,
            underline: Stroke::new(1.0, t.accent.default),
            ..normal.clone()
        };
        let mut at = 0;
        if let Some(r) = s.parameters.get(sig.parameter)
            && r.end <= s.label.len()
            && s.label.is_char_boundary(r.start)
            && s.label.is_char_boundary(r.end)
        {
            job.append(&s.label[..r.start], 0.0, normal.clone());
            job.append(&s.label[r.clone()], 0.0, strong);
            at = r.end;
        }
        job.append(&s.label[at..], 0.0, normal);
        let doc = s.documentation.clone();
        let above = cursor.left_top() - vec2(0.0, 2.0);
        egui::Area::new(id.with("signature"))
            .order(Order::Foreground)
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(above)
            .interactable(false)
            .show(&ctx, |ui| {
                card(ui).show(ui, |ui| {
                    ui.label(job);
                    if !doc.is_empty() {
                        ui.weak(doc);
                    }
                });
            });
    }
    if let Some((_, h)) = &m.assist.hover {
        let pos = ctx
            .pointer_hover_pos()
            .map_or(below, |p| p + vec2(8.0, 12.0));
        let (text, code) = (h.text.clone(), h.code.clone());
        egui::Area::new(id.with("hover"))
            .order(Order::Tooltip)
            .fixed_pos(pos)
            .interactable(false)
            .show(&ctx, |ui| {
                card(ui).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    if let Some(c) = code {
                        ui.label(RichText::new(c).font(font.clone()));
                    }
                    if !text.is_empty() {
                        ui.label(text);
                    }
                });
            });
    }
}

/// Hover: asks when the pointer rests over text, closes when it leaves.
pub(crate) fn hover_pointer(
    state: &EditorState,
    app: Option<&AppHandle>,
    ui: &Ui,
    over: Option<Pos>,
    response: &egui::Response,
) {
    let resting = ui.input(|i| i.pointer.time_since_last_movement() > 0.5 && !i.pointer.any_down());
    let mut m = state.lock();
    if m.assist.provider.is_none() {
        return;
    }
    match over {
        Some(p) if response.hovered() => {
            if let Some((at, h)) = &m.assist.hover {
                let inside = h.span.map_or(*at == p, |s| s.contains(p) || s.end == p);
                if !inside {
                    m.assist.hover = None;
                }
            }
            if resting && m.assist.hover_asked != Some(p) && m.assist.hover.is_none() {
                drop(m);
                hover_at(state, app, p);
            } else if !resting {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(550));
            }
        }
        _ => {
            m.assist.hover = None;
            m.assist.hover_asked = None;
        }
    }
}

/// The completion list's labels in order (the debug bridge's view).
pub(crate) fn shown_labels_of(m: &crate::model::Model) -> Vec<String> {
    m.assist
        .completion
        .as_ref()
        .map(|c| c.shown.iter().map(|&i| c.items[i].label.clone()).collect())
        .unwrap_or_default()
}

/// Draws ghost text after the cursor.
pub(crate) fn paint_ghost(
    painter: &egui::Painter,
    at: Pos2,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
) {
    let galley = painter.layout_job(ghost_job(text, font, color));
    painter.galley(at, galley, color);
}
