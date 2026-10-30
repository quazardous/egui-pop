//! The editors' pages: a text editor with a veto on one line, and a code
//! editor with diagnostics, breakpoints, diff marks, folding, 100 000
//! lines on demand and a demo assist provider (completion from a thread,
//! hover, signature help, ghost text, code actions, go to definition).

use std::cell::RefCell;
use std::rc::Rc;

use egui::Ui;
use egui_pop::api::steps::Before;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::widgets;
use egui_pop::editors::assist::{
    self, AssistProvider, CodeAction, CompletionItem, Hover, ItemKind, Location, Reply, Request,
    Signature, SignatureHelp, Suggestion, WordCompletion,
};
use egui_pop::editors::buffer::{Change, Pos, Span};
use egui_pop::editors::markers::{Diagnostic, Diff, Severity};
use egui_pop::editors::steps::{EDIT_CHANGE, EDITORS_PLUGIN};
use egui_pop::editors::{CodeEditor, EditorState, Signal, TextSurface};
use egui_pop::model::ids::{PluginId, StoryId};

use super::section;
use crate::shell::SHELL;

pub const TEXT_PAGE: PluginId = PluginId::from_static("gallery.text_editor");
pub const CODE_PAGE: PluginId = PluginId::from_static("gallery.code_editor");

/// The context key the title veto reads.
const VETO: &str = "galleryVetoTitle";

/// How many lines the big file has.
pub const MANY: usize = 100_000;

const NOTES: &str = "Field notes\n\nThe light came low over the ridge at six, warm and raking.\nShot the old mill twice: once at f/8, once wide open.\n\nTo do:\n- back up the cards\n- tag the mill series\n- print a contact sheet";

pub const CODE: &str = r#"//! Blends two exposures.

/// Mixes `a` and `b` by `t`, from 0 to 1.
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn main() {
    let shadows = 0.25;
    let highlights = 0.75;
    let unused = 3;
    for step in 0..4 {
        let t = step as f32 / 3.0;
        let value = mix(shadows, highlights, t);
        println!("{step}: {value:.2}");
    }
}
"#;

/// The page's log of signals and steps, newest last.
#[derive(Debug, Default)]
struct Log(Vec<String>);

impl Log {
    fn push(&mut self, s: String) {
        self.0.push(s);
        let n = self.0.len();
        if n > 6 {
            self.0.drain(..n - 6);
        }
    }

    fn show(&self, ui: &mut Ui) {
        section(ui, "Signals");
        if self.0.is_empty() {
            ui.weak("Nothing yet.");
        }
        for l in &self.0 {
            ui.monospace(l);
        }
    }
}

fn describe(s: &Signal) -> String {
    match s {
        Signal::Changed { version, kind } => format!("edit.change {kind} -> v{version}"),
        Signal::Saved { version } => format!("edit.save v{version}"),
        Signal::Breakpoint { line, set } => format!(
            "breakpoint line {} {}",
            line + 1,
            if *set { "set" } else { "cleared" }
        ),
        Signal::Fold { line, folded } => format!(
            "fold line {} {}",
            line + 1,
            if *folded { "closed" } else { "opened" }
        ),
        Signal::Definition { location } => format!(
            "definition {} line {}",
            location.editor,
            location.span.start.line + 1
        ),
        Signal::Refused { step, reason } => format!("{step} refused: {reason}"),
    }
}

/// The text editor page.
#[derive(Debug, Default)]
pub struct TextPage;

impl Plugin for TextPage {
    fn id(&self) -> PluginId {
        TEXT_PAGE
    }

    fn title(&self) -> String {
        "Text editor".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, EDITORS_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // The first line is the title: a before-hook may keep it.
        ctx.before(EDIT_CHANGE, |app, args| {
            let touches_title = args["changes"]
                .as_array()
                .is_some_and(|c| c.iter().any(|c| c["from"]["line"] == 0));
            if args["editor"] == "gallery.notes"
                && app.context().get(VETO) == "true"
                && touches_title
            {
                Before::Veto("the title is locked".into())
            } else {
                Before::Go
            }
        });
        let state = EditorState::new("gallery.notes", NOTES);
        let log = Rc::new(RefCell::new(Log::default()));
        ctx.story(
            Story::new(StoryId::from_static("text-editor"), "Text editor", move |ui, app| {
                text_page(ui, app, &state, &mut log.borrow_mut())
            })
            .description("A multi-line text editor in the theme: typing, selection, the clipboard, undo, search and replace (Ctrl+F, Ctrl+H), go to line (Ctrl+G); Escape leaves it; every edit an edit.change step.")
            .group("Components", 78),
        )?;
        Ok(())
    }
}

fn text_page(ui: &mut Ui, app: &AppHandle, state: &EditorState, log: &mut Log) {
    ui.label("Type, select, Ctrl+F to search, Ctrl+H to replace, Ctrl+Z to undo; Escape leaves the editor, Tab then moves on.");
    let mut veto = app.context().get(VETO) == "true";
    if widgets::checkbox(
        ui,
        &mut veto,
        "Lock the title line (a before-hook on edit.change)",
    )
    .changed()
    {
        app.set_context(VETO, if veto { "true" } else { "false" });
    }
    section(ui, "Notebook");
    let out = TextSurface::new()
        .label("Notes")
        .app(app)
        .height(super::fitted_height(ui, 220.0))
        .status(true)
        .show(ui, state);
    for s in &out.signals {
        log.push(describe(s));
    }
    ui.label(format!(
        "{} lines, version {}{}",
        state.line_count(),
        state.version(),
        if state.is_dirty() { ", modified" } else { "" }
    ));
    log.show(ui);
}

/// A demo assist provider: the text's words and Rust's keywords (from a
/// thread, as a language server would answer), hover and signature help
/// from the functions the text defines, ghost text after `mix(`, code
/// actions, and go to definition.
#[derive(Debug, Default)]
pub struct DemoAssist;

const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "for", "while", "loop", "match", "return", "struct", "enum", "impl", "pub",
    "use", "const", "static", "if", "else", "println",
];

/// The word at a position.
fn word_at(state: &EditorState, at: Pos) -> Option<(Span, String)> {
    let line = state.lines(at.line, at.line + 1);
    let chars: Vec<char> = line.chars().collect();
    let is = |c: char| c.is_alphanumeric() || c == '_';
    let mut a = at.col.min(chars.len());
    let mut b = a;
    while a > 0 && is(chars[a - 1]) {
        a -= 1;
    }
    while b < chars.len() && is(chars[b]) {
        b += 1;
    }
    (b > a).then(|| {
        (
            Span::new(Pos::new(at.line, a), Pos::new(at.line, b)),
            chars[a..b].iter().collect(),
        )
    })
}

/// Where `fn name` or `let name` is, and its line.
fn definition_of(state: &EditorState, name: &str) -> Option<(usize, usize, String)> {
    let text = state.text();
    for (i, line) in text.lines().enumerate() {
        for kw in ["fn ", "let "] {
            if let Some(at) = line.find(&format!("{kw}{name}")) {
                let end = at + kw.len() + name.len();
                let next = line[end..].chars().next();
                if next.is_none_or(|c| !(c.is_alphanumeric() || c == '_')) {
                    let col = line[..at + kw.len()].chars().count();
                    return Some((i, col, line.trim().to_string()));
                }
            }
        }
    }
    None
}

impl AssistProvider for DemoAssist {
    fn name(&self) -> String {
        "gallery demo".into()
    }

    fn trigger_characters(&self) -> Vec<char> {
        vec!['.']
    }

    fn suggests_inline(&self) -> bool {
        true
    }

    fn complete(&mut self, request: &Request, reply: Reply<Vec<CompletionItem>>) {
        let (answer, words) = assist::reply_here(request);
        let mut base = WordCompletion::default();
        base.complete(request, words);
        let (_, prefix) = assist::prefix_at(&request.state, request.at);
        let mut items: Vec<CompletionItem> = KEYWORDS
            .iter()
            .filter(|k| k.starts_with(prefix.as_str()) && **k != prefix)
            .map(|k| {
                CompletionItem::new(*k)
                    .kind(ItemKind::Keyword)
                    .detail("keyword")
            })
            .collect();
        items.extend(answer.take().unwrap_or_default());
        let answer = move || reply.deliver(items);
        if cfg!(target_arch = "wasm32") {
            answer();
        } else {
            // From a thread, after a moment, as a server would.
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(30));
                answer();
            });
        }
    }

    fn hover(&mut self, request: &Request, reply: Reply<Option<Hover>>) {
        let found = word_at(&request.state, request.at).and_then(|(span, word)| {
            definition_of(&request.state, &word).map(|(line, _, text)| Hover {
                code: Some(text),
                text: format!("Defined on line {}.", line + 1),
                span: Some(span),
            })
        });
        reply.deliver(found);
    }

    fn signature_help(&mut self, request: &Request, reply: Reply<Option<SignatureHelp>>) {
        let line = request.state.lines(request.at.line, request.at.line + 1);
        let before: String = line.chars().take(request.at.col).collect();
        let Some(open) = before.rfind('(') else {
            return reply.deliver(None);
        };
        let name: String = before[..open]
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let Some((_, _, def)) = definition_of(&request.state, &name) else {
            return reply.deliver(None);
        };
        let label = def.trim_end_matches('{').trim().to_string();
        let mut parameters = Vec::new();
        if let (Some(a), Some(b)) = (label.find('('), label.find(')')) {
            let mut start = a + 1;
            for (i, c) in label[a + 1..b].char_indices() {
                if c == ',' {
                    parameters.push(start..a + 1 + i);
                    start = a + 1 + i + 2;
                }
            }
            parameters.push(start..b);
        }
        let parameter = before[open..].matches(',').count();
        reply.deliver(Some(SignatureHelp {
            signatures: vec![Signature {
                label,
                parameters,
                documentation: String::new(),
            }],
            active: 0,
            parameter,
        }));
    }

    fn code_actions(&mut self, request: &Request, span: Span, reply: Reply<Vec<CodeAction>>) {
        let line = span.start.line;
        let text = request.state.lines(line, line + 1);
        let mut actions = Vec::new();
        if let Some(at) = text.find("let ") {
            let col = text[..at + 4].chars().count();
            if !text[at + 4..].starts_with('_') {
                actions.push(CodeAction {
                    title: "Prefix the variable with an underscore".into(),
                    changes: vec![Change::insert(Pos::new(line, col), "_")],
                    preferred: true,
                });
            }
        }
        let indent: String = text.chars().take_while(|c| c.is_whitespace()).collect();
        actions.push(CodeAction {
            title: "Add a TODO comment above".into(),
            changes: vec![Change::insert(
                Pos::new(line, 0),
                format!("{indent}// TODO\n"),
            )],
            preferred: false,
        });
        reply.deliver(actions);
    }

    fn definition(&mut self, request: &Request, reply: Reply<Vec<Location>>) {
        let found = word_at(&request.state, request.at).and_then(|(_, word)| {
            definition_of(&request.state, &word).map(|(line, col, _)| Location {
                editor: request.editor.clone(),
                span: Span::new(
                    Pos::new(line, col),
                    Pos::new(line, col + word.chars().count()),
                ),
            })
        });
        reply.deliver(found.into_iter().collect());
    }

    fn inline_suggestion(&mut self, request: &Request, reply: Reply<Option<Suggestion>>) {
        let line = request.state.lines(request.at.line, request.at.line + 1);
        let before: String = line.chars().take(request.at.col).collect();
        reply.deliver(before.ends_with("mix(").then(|| Suggestion {
            text: "shadows, highlights, 0.5)".into(),
        }));
    }
}

/// The code editor page's state.
#[derive(Debug)]
pub struct CodePage {
    state: EditorState,
    log: Log,
}

impl Default for CodePage {
    fn default() -> Self {
        let state = EditorState::new("gallery.main.rs", CODE)
            .language("rs")
            .assist(Box::new(DemoAssist));
        state.set_breakpoints(vec![13]);
        state.set_diff(vec![
            (9, Diff::Added),
            (13, Diff::Modified),
            (16, Diff::Removed),
        ]);
        Self {
            state,
            log: Log::default(),
        }
    }
}

/// The checker's findings on the sample.
pub fn diagnostics() -> Vec<Diagnostic> {
    vec![
        Diagnostic::new(10, Severity::Warning, "unused variable: `unused`")
            .columns(8, 14)
            .source("rustc"),
        Diagnostic::new(14, Severity::Info, "consider `{value:>5.2}` to align").source("clippy"),
    ]
}

/// The code editor page.
#[derive(Debug, Default)]
pub struct CodeEditorPage;

impl Plugin for CodeEditorPage {
    fn id(&self) -> PluginId {
        CODE_PAGE
    }

    fn title(&self) -> String {
        "Code editor".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, EDITORS_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let page = Rc::new(RefCell::new(CodePage::default()));
        ctx.story(
            Story::new(StoryId::from_static("code-editor"), "Code editor", move |ui, app| {
                code_page(ui, app, &mut page.borrow_mut())
            })
            .description("A code editor: line numbers, a gutter of diagnostics, breakpoints and diff marks, folding, matching brackets, indentation guides, syntect highlighting, 100 000 lines, and code assist (completion, hover, signature help, ghost text, code actions, go to definition).")
            .group("Components", 79),
        )?;
        Ok(())
    }
}

fn code_page(ui: &mut Ui, app: &AppHandle, p: &mut CodePage) {
    ui.label("Ctrl+Space completes, Ctrl+. offers actions, F12 or Ctrl+click goes to a definition, F9 sets a breakpoint, Ctrl+Shift+[ folds; type `mix(` for signature help and a suggestion Tab accepts.");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Run the checker").clicked() {
            match p.state.set_diagnostics(Some(app), diagnostics()) {
                Ok(n) => p.log.push(format!("code.diagnostics: {n}")),
                Err(e) => p.log.push(format!("code.diagnostics refused: {e}")),
            }
        }
        if ui.button("Clear the diagnostics").clicked() {
            let _ = p.state.set_diagnostics(Some(app), Vec::new());
        }
        if ui.button("Load 100 000 lines").clicked() {
            let mut text = String::from(CODE);
            for i in 0..MANY {
                text.push_str(&format!("const C{i}: u32 = {i}; // line {}\n", i + 19));
            }
            p.state.load(&text);
        }
        if ui.button("Back to the sample").clicked() {
            p.state.load(CODE);
        }
    });
    section(ui, "Sample");
    let out = CodeEditor::new()
        .label("main.rs")
        .app(app)
        .height(super::fitted_height(ui, 320.0))
        .show(ui, &p.state);
    for s in &out.signals {
        p.log.push(describe(s));
    }
    p.log.show(ui);
}
