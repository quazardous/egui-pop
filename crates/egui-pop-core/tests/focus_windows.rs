//! The Escape order in a window of its own: a text field there lets go of the
//! focus and swallows the press, and a field in another window stays as it is.
//!
//! kittest has one window, so these tests run an egui context with real
//! viewports, each window's pass with that window's input, as eframe does,
//! without a GPU.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use egui::{Context, Event, Id, Key, Modifiers, Pos2, RawInput, Rect, ViewportBuilder, ViewportId};
use egui_pop_core::focus;

const SCREEN: Rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(480.0, 360.0));

/// What a window saw in its last pass.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Seen {
    focused: bool,
    escape: bool,
    left: bool,
}

type Sights = Rc<RefCell<HashMap<ViewportId, Seen>>>;

fn field(window: ViewportId) -> Id {
    Id::new(("field", window))
}

fn child() -> ViewportId {
    ViewportId::from_hash_of("focus_windows.child")
}

fn draw(ui: &mut egui::Ui, sights: &Sights) {
    let window = ui.ctx().viewport_id();
    let id = field(window);
    let escape = ui.input(|i| i.key_pressed(Key::Escape));
    let left = focus::escape_left(ui.ctx(), id);
    let mut text = String::new();
    let r = ui.add(egui::TextEdit::singleline(&mut text).id(id));
    let seen = Seen {
        focused: r.has_focus(),
        escape,
        left,
    };
    sights.borrow_mut().insert(window, seen);
}

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

fn press(viewport: ViewportId, key: Key) {
    QUEUED.with(|q| {
        q.borrow_mut()
            .entry(viewport)
            .or_default()
            .extend([true, false].map(|pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            }));
    });
}

/// How the second window's passes run.
#[derive(Clone, Copy, Debug)]
enum Kind {
    /// Inside the main window's pass.
    Immediate,
    /// On their own, after the main window's.
    Deferred,
}

struct Run {
    ctx: Context,
    kind: Kind,
    sights: Sights,
    /// The second window's field lets Escape through.
    passing: bool,
}

impl Run {
    fn new(kind: Kind) -> Run {
        let ctx = Context::default();
        ctx.set_embed_viewports(false);
        Context::set_immediate_viewport_renderer(|ctx, viewport| {
            let mut draw = viewport.viewport_ui_cb;
            let mut out = ctx.run_ui(input(viewport.ids.this), |ui| draw(ui));
            out.textures_delta.clear();
        });
        focus::install(&ctx);
        let mut run = Run {
            ctx,
            kind,
            sights: Sights::default(),
            passing: false,
        };
        run.frame();
        run
    }

    /// One pass of each window.
    fn frame(&mut self) {
        let ctx = self.ctx.clone();
        let (kind, sights, passing) = (self.kind, self.sights.clone(), self.passing);
        let second = |ui: &mut egui::Ui| {
            if passing {
                focus::escape_passes(ui.ctx(), field(child()));
            }
            draw(ui, &sights);
        };
        let mut out = ctx.run_ui(input(ViewportId::ROOT), |ui| {
            draw(ui, &sights);
            if let Kind::Immediate = kind {
                let builder = ViewportBuilder::default();
                ui.ctx()
                    .show_viewport_immediate(child(), builder, |ui, _| second(ui));
            }
        });
        out.textures_delta.clear();
        if let Kind::Deferred = kind {
            let mut out = ctx.run_ui(input(child()), |ui| second(ui));
            out.textures_delta.clear();
        }
    }

    fn seen(&self, window: ViewportId) -> Seen {
        self.sights.borrow()[&window]
    }

    /// Tab and a pass or two: the first focusable widget of the window, its
    /// field, takes the focus.
    fn focus_field(&mut self, window: ViewportId) {
        press(window, Key::Tab);
        self.frame();
        self.frame();
        assert!(self.seen(window).focused, "Tab focuses the field");
    }
}

const FOCUSED: Seen = Seen {
    focused: true,
    escape: false,
    left: false,
};

#[test]
fn escape_in_a_window_leaves_that_windows_field() {
    for kind in [Kind::Immediate, Kind::Deferred] {
        let mut r = Run::new(kind);
        r.focus_field(child());
        press(child(), Key::Escape);
        r.frame();
        assert_eq!(
            r.seen(child()),
            Seen {
                focused: false,
                escape: false,
                left: true,
            },
            "{kind:?}: the field lets go and nothing else sees the press"
        );
        press(child(), Key::Escape);
        r.frame();
        assert!(r.seen(child()).escape, "{kind:?}: the second one goes on");
    }
}

#[test]
fn escape_in_a_window_leaves_another_windows_field_alone() {
    for kind in [Kind::Immediate, Kind::Deferred] {
        let mut r = Run::new(kind);
        r.focus_field(ViewportId::ROOT);
        press(child(), Key::Escape);
        r.frame();
        assert_eq!(
            r.seen(child()),
            Seen {
                focused: false,
                escape: true,
                left: false,
            },
            "{kind:?}: the press is for this window, which has no field to leave"
        );
        r.frame();
        assert_eq!(
            r.seen(ViewportId::ROOT),
            FOCUSED,
            "{kind:?}: the main window's field keeps the focus"
        );
    }
}

#[test]
fn escape_in_the_main_window_leaves_its_field_and_not_another_windows() {
    for kind in [Kind::Immediate, Kind::Deferred] {
        let mut r = Run::new(kind);
        r.focus_field(child());
        r.focus_field(ViewportId::ROOT);
        press(ViewportId::ROOT, Key::Escape);
        r.frame();
        assert_eq!(
            r.seen(ViewportId::ROOT),
            Seen {
                focused: false,
                escape: false,
                left: true,
            },
            "{kind:?}: the field lets go and nothing else sees the press"
        );
        assert_eq!(
            r.seen(child()),
            FOCUSED,
            "{kind:?}: the other window's field keeps the focus"
        );
    }
}

#[test]
fn a_field_of_a_window_can_let_escape_through() {
    for kind in [Kind::Immediate, Kind::Deferred] {
        let mut r = Run::new(kind);
        r.passing = true;
        r.focus_field(child());
        press(child(), Key::Escape);
        r.frame();
        assert_eq!(
            r.seen(child()),
            Seen {
                focused: false,
                escape: true,
                left: false,
            },
            "{kind:?}: the press goes on, and egui lets go of the focus"
        );
    }
}
