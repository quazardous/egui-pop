//! A terminal's keys across real windows: keys go to the terminal focused in
//! the window they were typed in, and leave the other windows alone.
//!
//! kittest has one window, so these tests run an egui context with real
//! viewports, each window's pass with that window's input, as eframe does,
//! without a GPU.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use egui::{Context, Event, Id, Key, Modifiers, Pos2, RawInput, Rect, ViewportBuilder, ViewportId};
use egui_pop_editors::terminal::{
    Sink, Spawn, Terminal, TerminalSignal, TerminalSource, TerminalState,
};
use serde_json::{Value as Json, json};

const SCREEN: Rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(480.0, 360.0));

/// A program that keeps what it is sent.
#[derive(Clone, Default)]
struct Program(Arc<Mutex<Vec<u8>>>);

impl Program {
    fn received(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

impl TerminalSource for Program {
    fn name(&self) -> String {
        "program".into()
    }

    fn writable(&self) -> bool {
        true
    }

    fn start(&mut self, _spawn: &Spawn, _sink: Sink) -> Result<Json, String> {
        Ok(json!({}))
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(())
    }
}

/// What a window holds.
#[derive(Clone)]
enum Content {
    Terminal(TerminalState, Program),
    Field(Rc<RefCell<String>>),
}

#[derive(Clone)]
struct Window {
    id: ViewportId,
    content: Content,
    /// The content takes the focus at its next pass.
    wants_focus: Rc<Cell<bool>>,
    focused: Rc<Cell<bool>>,
    left: Rc<Cell<bool>>,
}

impl Window {
    fn new(id: ViewportId, content: Content) -> Window {
        Window {
            id,
            content,
            wants_focus: Rc::default(),
            focused: Rc::default(),
            left: Rc::default(),
        }
    }

    fn terminal(id: ViewportId) -> Window {
        let program = Program::default();
        let name = format!("terminal {id:?}");
        let state = TerminalState::new(name, program.clone());
        Window::new(id, Content::Terminal(state, program))
    }

    fn field(id: ViewportId) -> Window {
        Window::new(id, Content::Field(Rc::default()))
    }

    fn draw(&self, ui: &mut egui::Ui) {
        let response = match &self.content {
            Content::Terminal(state, _) => {
                let out = Terminal::new().height(100.0).show(ui, state);
                if out.signals.contains(&TerminalSignal::Left) {
                    self.left.set(true);
                }
                out.response
            }
            Content::Field(text) => ui.add(
                egui::TextEdit::singleline(&mut *text.borrow_mut()).id(Id::new(("field", self.id))),
            ),
        };
        if self.wants_focus.take() {
            response.request_focus();
        }
        let id = response.id;
        self.focused.set(ui.ctx().memory(|m| m.has_focus(id)));
    }

    /// What the program received, or what the field holds.
    fn got(&self) -> String {
        match &self.content {
            Content::Terminal(_, program) => program.received(),
            Content::Field(text) => text.borrow().clone(),
        }
    }
}

/// The second window.
fn second() -> ViewportId {
    ViewportId::from_hash_of("terminal_windows.second")
}

/// A third window.
fn third() -> ViewportId {
    ViewportId::from_hash_of("terminal_windows.third")
}

const ROOT: ViewportId = ViewportId::ROOT;

/// How the other windows' passes run.
#[derive(Clone, Copy, Debug)]
enum Kind {
    /// Inside the main window's pass.
    Immediate,
    /// On their own, after the main window's.
    Deferred,
}

const KINDS: [Kind; 2] = [Kind::Immediate, Kind::Deferred];

thread_local! {
    /// Each window's events for its next pass.
    static QUEUED: RefCell<HashMap<ViewportId, Vec<Event>>> = RefCell::default();
}

fn input(viewport: ViewportId) -> RawInput {
    let events = QUEUED.with(|q| q.borrow_mut().remove(&viewport).unwrap_or_default());
    RawInput {
        viewport_id: viewport,
        // egui wants the window it runs among the viewports it is told of.
        viewports: std::iter::once((viewport, Default::default())).collect(),
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    }
}

fn queue(viewport: ViewportId, events: impl IntoIterator<Item = Event>) {
    QUEUED.with(|q| q.borrow_mut().entry(viewport).or_default().extend(events));
}

/// Real keys: a text event for typing, a press and a release for a chord.
fn typed(viewport: ViewportId, text: &str) {
    queue(viewport, [Event::Text(text.into())]);
}

fn chord(viewport: ViewportId, key: Key, modifiers: Modifiers) {
    queue(
        viewport,
        [true, false].map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        }),
    );
}

struct Run {
    ctx: Context,
    kind: Kind,
    /// The main window, then the others, each drawn from when it opened.
    windows: Vec<Window>,
}

impl Run {
    fn new(kind: Kind, root: Window) -> Run {
        let ctx = Context::default();
        ctx.set_embed_viewports(false);
        Context::set_immediate_viewport_renderer(|ctx, viewport| {
            let mut draw = viewport.viewport_ui_cb;
            let mut out = ctx.run_ui(input(viewport.ids.this), |ui| draw(ui));
            out.textures_delta.clear();
        });
        let mut run = Run {
            ctx,
            kind,
            windows: vec![root],
        };
        run.frame();
        run
    }

    /// Opens a window, which starts counting its passes from here.
    fn open(&mut self, window: Window) {
        self.windows.push(window);
        self.frame();
    }

    /// One pass of each window.
    fn frame(&mut self) {
        let ctx = self.ctx.clone();
        let root = self.windows[0].clone();
        let others = self.windows[1..].to_vec();
        let kind = self.kind;
        let mut out = ctx.run_ui(input(ROOT), |ui| {
            root.draw(ui);
            if let Kind::Immediate = kind {
                for w in &others {
                    ui.ctx()
                        .show_viewport_immediate(w.id, ViewportBuilder::default(), |ui, _| {
                            w.draw(ui)
                        });
                }
            }
        });
        out.textures_delta.clear();
        if let Kind::Deferred = kind {
            for w in &others {
                let mut out = ctx.run_ui(input(w.id), |ui| w.draw(ui));
                out.textures_delta.clear();
            }
        }
    }

    /// Two passes of each window: an immediate window's pass runs inside the
    /// main window's, after what the main window drew, so a key wrongly routed
    /// to the main window shows only in its next pass.
    fn settle(&mut self) {
        self.frame();
        self.frame();
    }

    fn window(&self, id: ViewportId) -> &Window {
        self.windows.iter().find(|w| w.id == id).unwrap()
    }

    fn got(&self, id: ViewportId) -> String {
        self.window(id).got()
    }

    fn focused(&self, id: ViewportId) -> bool {
        self.window(id).focused.get()
    }

    fn left(&self, id: ViewportId) -> bool {
        self.window(id).left.get()
    }

    /// Gives a window's content the focus, in that window.
    fn focus(&mut self, id: ViewportId) {
        self.window(id).wants_focus.set(true);
        self.settle();
        assert!(self.focused(id), "the content of {id:?} has the focus");
    }

    /// Two windows, their content focused.
    fn two(kind: Kind, root: Window, other: Window) -> Run {
        let mut r = Run::new(kind, root);
        r.open(other);
        r.focus(ROOT);
        r.focus(second());
        r
    }
}

#[test]
fn keys_go_to_the_terminal_of_the_window_they_were_typed_in() {
    for kind in KINDS {
        let mut r = Run::two(kind, Window::terminal(ROOT), Window::terminal(second()));
        typed(second(), "xy");
        r.settle();
        assert_eq!(r.got(second()), "xy", "{kind:?}");
        assert_eq!(r.got(ROOT), "", "{kind:?}");
        typed(ROOT, "ab");
        r.settle();
        assert_eq!(r.got(ROOT), "ab", "{kind:?}");
        assert_eq!(r.got(second()), "xy", "{kind:?}");
    }
}

#[test]
fn keys_typed_in_a_field_stay_out_of_another_windows_terminal() {
    for kind in KINDS {
        let mut r = Run::two(kind, Window::terminal(ROOT), Window::field(second()));
        typed(second(), "xy");
        r.settle();
        assert_eq!(r.got(second()), "xy", "{kind:?}: the field has them");
        assert_eq!(r.got(ROOT), "", "{kind:?}: the terminal does not");
        chord(second(), Key::ArrowUp, Modifiers::NONE);
        r.settle();
        assert_eq!(r.got(ROOT), "", "{kind:?}: nor a key");
        typed(ROOT, "ab");
        r.settle();
        assert_eq!(r.got(ROOT), "ab", "{kind:?}");
        assert_eq!(r.got(second()), "xy", "{kind:?}");
    }
}

#[test]
fn a_terminal_in_a_second_window_gets_its_keys() {
    for kind in KINDS {
        let mut r = Run::two(kind, Window::field(ROOT), Window::terminal(second()));
        typed(second(), "xy");
        r.settle();
        assert_eq!(r.got(second()), "xy", "{kind:?}: the terminal has them");
        assert_eq!(r.got(ROOT), "", "{kind:?}: the field does not");
        chord(second(), Key::ArrowUp, Modifiers::NONE);
        r.settle();
        assert_eq!(r.got(second()), "xy\x1b[A", "{kind:?}: a key too");
        typed(ROOT, "ab");
        r.settle();
        assert_eq!(r.got(ROOT), "ab", "{kind:?}");
        assert_eq!(r.got(second()), "xy\x1b[A", "{kind:?}");
    }
}

#[test]
fn shift_escape_leaves_the_terminal_of_the_window_it_was_pressed_in() {
    for kind in KINDS {
        let mut r = Run::two(kind, Window::terminal(ROOT), Window::terminal(second()));
        chord(second(), Key::Escape, Modifiers::SHIFT);
        r.settle();
        assert!(r.left(second()), "{kind:?}: this terminal leaves");
        assert!(!r.focused(second()), "{kind:?}: and lets go of the focus");
        assert!(!r.left(ROOT), "{kind:?}: the other one stays");
        assert!(r.focused(ROOT), "{kind:?}: with its focus");
        // The chord went nowhere else, and the keys now find nobody here.
        typed(second(), "x");
        r.settle();
        assert_eq!(r.got(second()), "", "{kind:?}");
        assert_eq!(r.got(ROOT), "", "{kind:?}");

        chord(ROOT, Key::Escape, Modifiers::SHIFT);
        r.settle();
        assert!(r.left(ROOT), "{kind:?}: and it leaves in its own window");
        assert!(!r.focused(ROOT), "{kind:?}");
    }
}

#[test]
fn shift_escape_in_a_window_without_a_terminal_leaves_the_others_alone() {
    for kind in KINDS {
        let mut r = Run::two(kind, Window::terminal(ROOT), Window::field(second()));
        chord(second(), Key::Escape, Modifiers::SHIFT);
        r.settle();
        assert!(!r.left(ROOT), "{kind:?}");
        assert!(r.focused(ROOT), "{kind:?}");
    }
}

/// A window opened later has counted fewer passes than the others: what a
/// terminal drew is kept by window, so it does not matter.
#[test]
fn windows_opened_at_different_times_each_keep_their_keys() {
    for kind in KINDS {
        let mut r = Run::new(kind, Window::terminal(ROOT));
        r.settle();
        r.open(Window::terminal(second()));
        r.settle();
        r.open(Window::terminal(third()));
        r.settle();
        r.focus(ROOT);
        r.focus(second());
        r.focus(third());
        for (window, text) in [(third(), "pq"), (second(), "xy"), (ROOT, "ab")] {
            typed(window, text);
            r.settle();
            assert_eq!(r.got(window), text, "{kind:?}: {window:?}");
        }
    }
}
