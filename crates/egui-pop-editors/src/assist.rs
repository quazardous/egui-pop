//! Code assist: what an application (a language server client, an AI
//! assistant, a word list) offers while code is typed, modelled on the
//! Language Server Protocol's requests.
//!
//! An [`AssistProvider`] answers requests: completion (on typing, on a
//! trigger character, or asked with Ctrl+Space), hover, signature help,
//! code actions, go to definition, and inline suggestions (ghost text,
//! accepted with Tab). Diagnostics are not a request: they are pushed
//! with `EditorState::set_diagnostics` (the `code.diagnostics` step).
//!
//! Every request is asynchronous: the provider gets a [`Request`] (its
//! [`RequestId`], the editor's state and version, the cursor) and a
//! [`Reply`] it answers now or later, from any thread; the editor shows
//! the answer at the next pass. A newer request of the same kind cancels
//! the older one ([`Reply::is_cancelled`], [`AssistProvider::cancel`]),
//! and an answer for a version of the text that has moved on is dropped
//! (completion keeps its answer while the word under the cursor is only
//! being extended, and filters it as you type).
//!
//! [`WordCompletion`] is the default provider of a code editor: the
//! words of the text, by prefix.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use egui_pop_core::lock;
use serde::{Deserialize, Serialize};

use crate::buffer::{Change, Pos, Span};
use crate::model::EditorState;

/// A request's number, unique per editor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RequestId(pub u64);

/// Why completion was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase", tag = "by", content = "char")]
pub enum Trigger {
    /// A word character was typed.
    Typing,
    /// One of the provider's trigger characters was typed (`.`, `::`).
    Character(char),
    /// Asked for (Ctrl+Space, the `code.complete` command).
    Explicit,
}

/// What a provider is asked about.
#[derive(Clone, Debug)]
pub struct Request {
    pub id: RequestId,
    /// The editor's id.
    pub editor: String,
    /// The editor itself, to read the text (`text()`, `lines()`); never
    /// locked while the provider is called.
    pub state: EditorState,
    /// The text's version the request is about.
    pub version: u64,
    /// The cursor (completion, signature help, ghost text), the pointer
    /// (hover), or the start of the selection (code actions).
    pub at: Pos,
    pub trigger: Trigger,
}

/// What kind of thing a completion is (LSP's `CompletionItemKind`, in
/// short).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    #[default]
    Text,
    Keyword,
    Function,
    Method,
    Variable,
    Field,
    Type,
    Module,
    Constant,
    Snippet,
}

/// One completion.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionItem {
    /// What the list shows, and what typing filters on.
    pub label: String,
    /// What is inserted (the label when empty).
    #[serde(default)]
    pub insert: String,
    /// A short note shown beside the label (a type, a module).
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub kind: ItemKind,
    /// What it replaces; `None`: the word before the cursor.
    #[serde(default)]
    pub replace: Option<Span>,
}

impl CompletionItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn kind(mut self, kind: ItemKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn insert(mut self, text: impl Into<String>) -> Self {
        self.insert = text.into();
        self
    }

    /// What it inserts.
    pub fn text(&self) -> &str {
        if self.insert.is_empty() {
            &self.label
        } else {
            &self.insert
        }
    }
}

/// What hovering shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hover {
    /// Plain text; a first line in the monospace font when `code`.
    pub text: String,
    #[serde(default)]
    pub code: Option<String>,
    /// The span it is about (underlined while shown).
    #[serde(default)]
    pub span: Option<Span>,
}

/// A callable's signature, as signature help shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// `fn mix(a: f32, b: f32, t: f32) -> f32`.
    pub label: String,
    /// Each parameter's byte range in the label.
    pub parameters: Vec<std::ops::Range<usize>>,
    #[serde(default)]
    pub documentation: String,
}

/// Signature help: the signatures and which parameter is being typed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureHelp {
    pub signatures: Vec<Signature>,
    pub active: usize,
    pub parameter: usize,
}

/// A code action or quick fix: a title and the changes it makes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeAction {
    pub title: String,
    /// Applied in order, as one undo step.
    pub changes: Vec<Change>,
    /// Shown first.
    #[serde(default)]
    pub preferred: bool,
}

/// Where something is defined.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// The editor (or the document, for the application to open).
    pub editor: String,
    pub span: Span,
}

/// An inline suggestion (ghost text) at the cursor, accepted with Tab.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    /// Inserted at the cursor; its first line is drawn after the cursor.
    pub text: String,
}

/// The slot an answer lands in.
#[derive(Debug)]
struct Slot<T> {
    value: Mutex<Option<T>>,
    cancelled: AtomicBool,
}

/// Where a provider sends its answer: `Send`, kept and answered later
/// from any thread, or dropped (no answer).
pub struct Reply<T> {
    id: RequestId,
    slot: Arc<Slot<T>>,
    wake: Option<egui::Context>,
}

impl<T> fmt::Debug for Reply<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Reply({})", self.id.0)
    }
}

impl<T: Send> Reply<T> {
    pub fn id(&self) -> RequestId {
        self.id
    }

    /// Whether a newer request made this one useless.
    pub fn is_cancelled(&self) -> bool {
        self.slot.cancelled.load(Ordering::Relaxed)
    }

    /// Answers, waking the interface.
    pub fn deliver(self, value: T) {
        if self.is_cancelled() {
            return;
        }
        *lock(&self.slot.value) = Some(value);
        if let Some(ctx) = &self.wake {
            ctx.request_repaint();
        }
    }
}

/// A request in flight, on the editor's side.
#[derive(Debug)]
pub(crate) struct Pending<T> {
    pub id: RequestId,
    pub version: u64,
    pub at: Pos,
    slot: Arc<Slot<T>>,
}

impl<T> Pending<T> {
    /// The answer, once.
    pub fn take(&self) -> Option<T> {
        lock(&self.slot.value).take()
    }

    pub fn cancel(&self) {
        self.slot.cancelled.store(true, Ordering::Relaxed);
    }
}

/// A new request and its reply.
pub(crate) fn pair<T>(
    id: RequestId,
    version: u64,
    at: Pos,
    wake: Option<egui::Context>,
) -> (Pending<T>, Reply<T>) {
    let slot = Arc::new(Slot {
        value: Mutex::new(None),
        cancelled: AtomicBool::new(false),
    });
    (
        Pending {
            id,
            version,
            at,
            slot: slot.clone(),
        },
        Reply { id, slot, wake },
    )
}

/// An answer collected in place, to build a provider on another one
/// (ask it with the [`Reply`], read what it said with [`Answer::take`]).
#[derive(Debug)]
pub struct Answer<T>(Pending<T>);

impl<T> Answer<T> {
    /// What the provider answered, if it has.
    pub fn take(&self) -> Option<T> {
        self.0.take()
    }
}

/// A reply for a request, answered here rather than to the editor.
pub fn reply_here<T>(request: &Request) -> (Answer<T>, Reply<T>) {
    let (p, r) = pair(request.id, request.version, request.at, None);
    (Answer(p), r)
}

/// What an application offers while code is typed. Every method has a
/// default that answers nothing, so a provider implements what it knows.
/// Methods are called on the interface thread, with no editor lock held;
/// a slow provider keeps the reply and answers from its own thread.
pub trait AssistProvider: Send {
    /// A name for the debug bridge.
    fn name(&self) -> String;

    /// Characters that ask for completion when typed (`.`, `:`).
    fn trigger_characters(&self) -> Vec<char> {
        Vec::new()
    }

    /// Characters that ask for signature help (`(`, `,`).
    fn signature_characters(&self) -> Vec<char> {
        vec!['(', ',']
    }

    /// Whether typing a word character asks for completion.
    fn complete_while_typing(&self) -> bool {
        true
    }

    /// Whether to ask for an inline suggestion after typing.
    fn suggests_inline(&self) -> bool {
        false
    }

    fn complete(&mut self, request: &Request, reply: Reply<Vec<CompletionItem>>) {
        let _ = request;
        reply.deliver(Vec::new());
    }

    fn hover(&mut self, request: &Request, reply: Reply<Option<Hover>>) {
        let _ = request;
        reply.deliver(None);
    }

    fn signature_help(&mut self, request: &Request, reply: Reply<Option<SignatureHelp>>) {
        let _ = request;
        reply.deliver(None);
    }

    /// Actions for the selection (`span`), or the cursor's line.
    fn code_actions(&mut self, request: &Request, span: Span, reply: Reply<Vec<CodeAction>>) {
        let _ = (request, span);
        reply.deliver(Vec::new());
    }

    fn definition(&mut self, request: &Request, reply: Reply<Vec<Location>>) {
        let _ = request;
        reply.deliver(Vec::new());
    }

    fn inline_suggestion(&mut self, request: &Request, reply: Reply<Option<Suggestion>>) {
        let _ = request;
        reply.deliver(None);
    }

    /// A request was superseded; its reply will be dropped.
    fn cancel(&mut self, id: RequestId) {
        let _ = id;
    }
}

/// The default provider: completes the word before the cursor with the
/// other words of the text (at least three characters, the nearest
/// lines first, at most 5 000 lines around the cursor).
#[derive(Clone, Debug)]
pub struct WordCompletion {
    /// At most this many items.
    pub limit: usize,
}

impl Default for WordCompletion {
    fn default() -> Self {
        Self { limit: 50 }
    }
}

/// The words of a text with their nearest line to `line`.
pub fn words_near(state: &EditorState, line: usize, reach: usize) -> BTreeMap<String, usize> {
    let from = line.saturating_sub(reach);
    let text = state.lines(from, line + reach);
    let mut words: BTreeMap<String, usize> = BTreeMap::new();
    for (i, l) in text.split('\n').enumerate() {
        let dist = (from + i).abs_diff(line);
        for w in l.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            if w.chars().count() >= 3 && !w.starts_with(|c: char| c.is_ascii_digit()) {
                let d = words.entry(w.to_string()).or_insert(dist);
                *d = (*d).min(dist);
            }
        }
    }
    words
}

/// The word before a position (letters, digits, `_`).
pub fn prefix_at(state: &EditorState, at: Pos) -> (Span, String) {
    let line = state.lines(at.line, at.line + 1);
    let chars: Vec<char> = line.chars().collect();
    let end = at.col.min(chars.len());
    let mut start = end;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }
    (
        Span::new(Pos::new(at.line, start), Pos::new(at.line, end)),
        chars[start..end].iter().collect(),
    )
}

impl AssistProvider for WordCompletion {
    fn name(&self) -> String {
        "words".into()
    }

    fn complete(&mut self, request: &Request, reply: Reply<Vec<CompletionItem>>) {
        let (_, prefix) = prefix_at(&request.state, request.at);
        if prefix.is_empty() && request.trigger != Trigger::Explicit {
            return reply.deliver(Vec::new());
        }
        let lower = prefix.to_lowercase();
        let mut words: Vec<(usize, String)> = words_near(&request.state, request.at.line, 5000)
            .into_iter()
            .filter(|(w, _)| *w != prefix && w.to_lowercase().starts_with(&lower))
            .map(|(w, d)| (d, w))
            .collect();
        words.sort();
        reply.deliver(
            words
                .into_iter()
                .take(self.limit)
                .map(|(_, w)| CompletionItem::new(w))
                .collect(),
        );
    }
}

/// Keeps the items whose label starts with (or, failing that, contains)
/// the typed prefix, case-insensitively, prefix matches first.
pub fn filter(items: &[CompletionItem], prefix: &str) -> Vec<usize> {
    let p = prefix.to_lowercase();
    let mut starts = Vec::new();
    let mut contains = Vec::new();
    for (i, it) in items.iter().enumerate() {
        let l = it.label.to_lowercase();
        if l.starts_with(&p) {
            starts.push(i);
        } else if l.contains(&p) {
            contains.push(i);
        }
    }
    starts.extend(contains);
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_complete_by_prefix_nearest_first() {
        let s = EditorState::new("t", "alpha alphabet\nbeta\n\nalpine al");
        let mut p = WordCompletion::default();
        let (pending, reply) = pair(RequestId(1), 0, Pos::new(3, 9), None);
        p.complete(
            &Request {
                id: RequestId(1),
                editor: "t".into(),
                state: s.clone(),
                version: 0,
                at: Pos::new(3, 9),
                trigger: Trigger::Typing,
            },
            reply,
        );
        let items = pending.take().unwrap();
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["alpine", "alpha", "alphabet"]);
        assert_eq!(filter(&items, "alph"), vec![1, 2]);
    }

    #[test]
    fn a_cancelled_reply_is_dropped() {
        let (pending, reply) = pair::<Vec<CompletionItem>>(RequestId(1), 0, Pos::default(), None);
        pending.cancel();
        assert!(reply.is_cancelled());
        reply.deliver(vec![CompletionItem::new("x")]);
        assert!(pending.take().is_none());
    }
}
