//! The main window's paint guard: a window never stays empty without a
//! word.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! Two holes are watched at the end of each pass of the main window:
//!
//! - **The focus screen without its page.** A wizard run as a focus
//!   screen hides the shell (menus, status bar, centre) under its page;
//!   the page is drawn by the Settings app's block. When the wizard is
//!   stored and no page was drawn in the pass (the Settings closed under
//!   it, its block replaced, its overlay not held), the shell is drawn
//!   again from the next pass on, a warning says so once, and
//!   `window.focus_hole` is published.
//! - **A window that painted nothing.** When the shell added nothing but
//!   backgrounds and lines under the title bar (no text, no image, no
//!   overlay held; without a shell, in the whole pass: a window whose
//!   decorations did not draw either, as in zen mode or under a window's
//!   content moved into the main window) for longer than a delay ([`DEFAULT_DELAY`]), an error
//!   says once what holds the screen (the place stack, the Settings app
//!   and its wizard, the overlays, the layout's areas),
//!   `window.painted_nothing` is published, and a fallback page offers to
//!   reset the arrangement and to go back to the first place. The page
//!   leaves as soon as the shell paints again.
//!
//! The cost is a counter and a scan of the pass's shapes that stops at
//! the first text or image (the menu bar's, in a healthy window).

use std::time::Duration;

use egui::{Align2, Id, LayerId, Order, Shape, Ui};
use egui_pop_core::icon::Icon;
use egui_pop_core::kit::{Kit, State};
use egui_pop_core::overlay;
use egui_pop_core::placeholder::EmptyState;
use egui_pop_core::strings;
use egui_pop_model::context::{Envelope, Request};
use serde_json::{Value as Json, json};
use web_time::Instant;

use crate::app::AppHandle;
use crate::strings as txt;

/// How long the main window may paint nothing before the fallback page.
pub const DEFAULT_DELAY: Duration = Duration::from_secs(2);

/// The passes a window must have painted nothing in, whatever the delay:
/// a pass or two are empty while a place changes.
const MIN_PASSES: u32 = 3;

/// An application window's watch (its own passes, see
/// [`AppHandle::watch_app_window`]).
#[derive(Debug, Default)]
pub(crate) struct WindowWatch {
    /// Since when it paints nothing, and for how many passes.
    empty: Option<(Instant, u32)>,
    /// The fallback page is shown in it.
    shown: bool,
}

/// An application window closed lately, until the backend lets its
/// system window go.
#[derive(Debug)]
pub(crate) struct Closed {
    id: String,
    since: Instant,
    /// The main window's passes that still saw it listed.
    seen: u32,
    /// Its outliving the close was said.
    said: bool,
}

/// A window whose system window outlived it: closed, or never opened,
/// while its viewport still runs passes or is still listed by the
/// backend ([`orphan_pass`], [`AppHandle::watch_closed_windows`]).
#[derive(Debug)]
struct Orphan {
    since: Instant,
    passes: u32,
    said: bool,
}

thread_local! {
    /// The viewports whose pass found no application window, by viewport.
    static ORPHANS: std::cell::RefCell<std::collections::BTreeMap<egui::ViewportId, Orphan>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// Where a window's content starts: each layer's shapes before it.
pub(crate) struct Mark(Vec<(LayerId, usize)>);

/// Marks where a window's content starts, in every layer of its pass.
pub(crate) fn mark(ui: &Ui) -> Mark {
    let ctx = ui.ctx();
    let mut layers: Vec<LayerId> = ctx.memory(|m| m.layer_ids().collect());
    for l in [ui.layer_id(), LayerId::background()] {
        if !layers.contains(&l) {
            layers.push(l);
        }
    }
    Mark(layers.into_iter().map(|l| (l, shapes(ctx, l))).collect())
}

/// Whether a text, an image or a drawing was painted since `mark`, in a
/// layer it knew or in one opened since (an area, a popup).
pub(crate) fn painted_since(ctx: &egui::Context, mark: &Mark) -> bool {
    let layers: Vec<LayerId> = ctx.memory(|m| m.layer_ids().collect());
    let from = |l: LayerId| mark.0.iter().find(|(m, _)| *m == l).map_or(0, |(_, n)| *n);
    mark.0
        .iter()
        .map(|(l, _)| *l)
        .chain(
            layers
                .into_iter()
                .filter(|l| mark.0.iter().all(|(m, _)| m != l)),
        )
        .any(|l| has_content(ctx, l, from(l), None, &[]))
}

/// A closed window's page, drawn by its passes until the backend drops
/// its system window (ADR 0029: one pass of grace draws the window's last
/// page, never nothing).
struct LastPage {
    title: String,
    /// The shapes of the pass it closed in, per layer; `None` when it
    /// closed from elsewhere (the main window's pass, a command).
    shapes: Option<Vec<(LayerId, Vec<egui::epaint::ClippedShape>)>>,
}

thread_local! {
    /// The application windows closed and not yet dropped, by viewport.
    static LAST_PAGES: std::cell::RefCell<std::collections::BTreeMap<egui::ViewportId, LastPage>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// An application window closed (`window.registry`): its passes until
/// the main window's next one draw its last page.
pub(crate) fn closing(viewport: egui::ViewportId, title: String) {
    LAST_PAGES.with_borrow_mut(|p| {
        p.insert(
            viewport,
            LastPage {
                title,
                shapes: None,
            },
        )
    });
}

/// The window closed in this very pass of its own: what the pass painted
/// is its last page.
pub(crate) fn keep_last_page(ctx: &egui::Context, viewport: egui::ViewportId) {
    let layers: Vec<LayerId> = ctx.memory(|m| m.layer_ids().collect());
    let shapes: Vec<(LayerId, Vec<egui::epaint::ClippedShape>)> = ctx.graphics(|g| {
        layers
            .iter()
            .filter_map(|l| {
                let list = g.get(*l)?;
                let shapes: Vec<_> = list.all_entries().cloned().collect();
                (!shapes.is_empty()).then_some((*l, shapes))
            })
            .collect()
    });
    LAST_PAGES.with_borrow_mut(|p| {
        if let Some(page) = p.get_mut(&viewport) {
            page.shapes = Some(shapes);
        }
    });
}

/// The main window's pass no longer declares the window: the backend
/// drops its system window, its last page goes.
pub(crate) fn closed_for_good(viewport: egui::ViewportId) {
    LAST_PAGES.with_borrow_mut(|p| p.remove(&viewport));
}

/// Draws a closed window's last page in a pass of its own: the shapes of
/// the pass it closed in, or, closed from elsewhere, its title on the
/// canvas. Whether it had one.
fn draw_last_page(ui: &mut Ui, viewport: egui::ViewportId) -> bool {
    let Some((title, shapes)) = LAST_PAGES.with_borrow(|p| {
        p.get(&viewport)
            .map(|l| (l.title.clone(), l.shapes.clone()))
    }) else {
        return false;
    };
    let ctx = ui.ctx().clone();
    match shapes {
        Some(shapes) => ctx.graphics_mut(|g| {
            for (layer, shapes) in shapes {
                let list = g.entry(layer);
                for s in shapes {
                    list.add(s.clip_rect, s.shape);
                }
            }
        }),
        None => {
            let t = egui_pop_core::theme::Tokens::of(&ctx);
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 0.0, t.surface.canvas);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                title,
                egui::FontId::proportional(14.0),
                t.text.muted,
            );
        }
    }
    true
}

/// A pass of a viewport with no application window behind it (closed, or
/// never opened). A window closed lately draws its last page; otherwise
/// the pass paints nothing, so its system window shows the clear colour
/// alone. A pass or two run between a close and the main window's next
/// pass, which drops the viewport; past the delay, an error says which
/// window it is, once, and the main window is asked to pass (where a
/// closed window's viewport is dropped).
pub(crate) fn orphan_pass(ui: &mut Ui, viewport: egui::ViewportId, why: &str) {
    let ctx = ui.ctx().clone();
    let drawn = draw_last_page(ui, viewport);
    let now = Instant::now();
    let say = ORPHANS.with_borrow_mut(|o| {
        let e = o.entry(viewport).or_insert(Orphan {
            since: now,
            passes: 0,
            said: false,
        });
        e.passes += 1;
        if e.passes == 1 {
            if drawn {
                log::debug!(
                    target: "egui_pop",
                    "a pass of viewport {viewport:?} after its window went ({why}): it draws \
                     the window's last page until the backend drops it"
                );
            } else {
                // Its system window shows this pass's frame: the clear
                // colour alone, without a title bar (said, to tell a black
                // window).
                log::info!(
                    target: "egui_pop",
                    "a pass of viewport {viewport:?} after its window went ({why}): it paints \
                     nothing, and the window is asked to hide and close"
                );
            }
        }
        let due = !e.said && e.passes >= MIN_PASSES && now.duration_since(e.since) >= DEFAULT_DELAY;
        if due {
            e.said = true;
        }
        due.then_some((e.passes, now.duration_since(e.since)))
    });
    if let Some((passes, since)) = say {
        let title = ctx
            .input(|i| i.viewport().title.clone())
            .unwrap_or_default();
        log::error!(
            target: "egui_pop",
            "the window of viewport {viewport:?} (title {title:?}) outlived its close for \
             {passes} passes ({:.1} s): {why}; it is closed",
            since.as_secs_f32()
        );
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    ctx.request_repaint_of(egui::ViewportId::ROOT);
    ctx.request_repaint_after(DEFAULT_DELAY);
}

/// A viewport has an application window again (opened): its orphan
/// passes are forgotten.
pub(crate) fn forget_orphan(viewport: egui::ViewportId) {
    ORPHANS.with_borrow_mut(|o| o.remove(&viewport));
    LAST_PAGES.with_borrow_mut(|p| p.remove(&viewport));
}

/// The guard's state, per application (the main window's).
#[derive(Debug)]
pub(crate) struct Guard {
    /// A focus screen is asked and no page drew: the shell is drawn.
    focus_hole: bool,
    /// Where the pass starts in the window's layer: before the title
    /// bar, or the content moved into the main window.
    pass_start: Option<(LayerId, usize)>,
    /// Where the shell's content starts in the window's layer: after the
    /// title bar's shapes.
    start: Option<(LayerId, usize)>,
    /// The first pass was said in the log.
    first_said: bool,
    /// What the shell painted in this pass that says it has nothing to
    /// show (the empty centre's "Drop a panel here"): the shapes' ranges
    /// in their layer, not counted as content.
    placeholders: Vec<(LayerId, std::ops::Range<usize>)>,
    /// Since when the window paints nothing, and for how many passes.
    empty: Option<(Instant, u32)>,
    /// The fallback page is shown.
    shown: bool,
    /// `None`: the window is not watched.
    delay: Option<Duration>,
    /// The application windows' watches, by viewport.
    windows: std::collections::BTreeMap<egui::ViewportId, WindowWatch>,
    /// The application windows closed lately, by viewport.
    closed: std::collections::BTreeMap<egui::ViewportId, Closed>,
}

impl Default for Guard {
    fn default() -> Self {
        Guard {
            focus_hole: false,
            pass_start: None,
            start: None,
            first_said: false,
            placeholders: Vec::new(),
            empty: None,
            shown: false,
            delay: Some(DEFAULT_DELAY),
            windows: std::collections::BTreeMap::new(),
            closed: std::collections::BTreeMap::new(),
        }
    }
}

/// Whether a shape is content: a text, an image, a drawing. Backgrounds
/// (plain rectangles) and lines (separators, sashes, outlines) are what
/// an empty window still paints.
fn is_content(shape: &Shape) -> bool {
    match shape {
        Shape::Noop | Shape::LineSegment { .. } => false,
        Shape::Rect(r) => r.brush.is_some(),
        Shape::Vec(v) => v.iter().any(is_content),
        _ => true,
    }
}

/// How many shapes the layer holds.
fn shapes(ctx: &egui::Context, layer: LayerId) -> usize {
    ctx.graphics(|g| g.get(layer).map_or(0, |l| l.next_idx().0))
}

/// Whether the layer holds content from `from` up to `to` (its end
/// without one), the shapes in `skip` (placeholders) left out.
fn has_content(
    ctx: &egui::Context,
    layer: LayerId,
    from: usize,
    to: Option<usize>,
    skip: &[(LayerId, std::ops::Range<usize>)],
) -> bool {
    let skipped = |i: usize| skip.iter().any(|(l, r)| *l == layer && r.contains(&i));
    ctx.graphics(|g| {
        g.get(layer).is_some_and(|l| {
            l.all_entries()
                .enumerate()
                .take(to.unwrap_or(usize::MAX))
                .skip(from)
                .any(|(i, s)| !skipped(i) && is_content(&s.shape))
        })
    })
}

impl AppHandle {
    /// How long the main window may paint nothing (no text, no image,
    /// no overlay) before an error is logged and the fallback page
    /// shown; `None` turns the watch off (an application whose window is
    /// empty on purpose). [`DEFAULT_DELAY`] to start with.
    pub fn set_empty_window_delay(&self, delay: Option<Duration>) {
        let mut g = self.0.paint_guard.borrow_mut();
        g.delay = delay;
        g.empty = None;
    }

    /// Whether the main window shows the fallback page: it painted
    /// nothing for longer than the delay.
    pub fn empty_window(&self) -> bool {
        self.0.paint_guard.borrow().shown
    }

    /// Whether a wizard is stored as a focus screen while no page of it
    /// is drawn: the shell is drawn instead of hidden.
    pub fn focus_screen_hole(&self) -> bool {
        self.0.paint_guard.borrow().focus_hole
    }

    /// Whether the shell hides under a focus screen in this pass: one is
    /// asked, and its page was drawn in the last pass.
    pub(crate) fn hidden_by_focus_screen(&self) -> bool {
        crate::admin::focus_screen(self) && !self.0.paint_guard.borrow().focus_hole
    }

    /// The main window's pass starts here in `ui`'s layer: before the
    /// title bar, the shell, or the content moved into the main window.
    pub(crate) fn mark_pass_start(&self, ui: &Ui) {
        let layer = ui.layer_id();
        let mut g = self.0.paint_guard.borrow_mut();
        g.pass_start = Some((layer, shapes(ui.ctx(), layer)));
        g.start = None;
        g.placeholders.clear();
    }

    /// Draws `add`, a placeholder saying there is nothing to show (the
    /// empty centre's "Drop a panel here"): what it paints is not content
    /// for the guard, so a window showing only placeholders still gets
    /// the fallback page and its way back.
    pub(crate) fn placeholder(&self, ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
        let layer = ui.layer_id();
        let from = shapes(ui.ctx(), layer);
        add(ui);
        let to = shapes(ui.ctx(), layer);
        self.0
            .paint_guard
            .borrow_mut()
            .placeholders
            .push((layer, from..to));
    }

    /// The shell's content starts here in `ui`'s layer (what was painted
    /// before is the title bar's).
    pub(crate) fn mark_shell_start(&self, ui: &Ui) {
        let layer = ui.layer_id();
        self.0.paint_guard.borrow_mut().start = Some((layer, shapes(ui.ctx(), layer)));
    }

    /// The end of the main window's pass, once the shell and the overlay
    /// zones are drawn.
    pub(crate) fn end_shell_pass(&self, ui: &Ui) {
        let ctx = ui.ctx();
        let admin = self.service(&crate::admin::ADMIN);
        let focus = admin.as_ref().is_some_and(|a| a.focus_screen());
        let page = admin.as_ref().is_some_and(|a| a.take_page_drawn());
        let hole = focus && !page;
        let was = std::mem::replace(&mut self.0.paint_guard.borrow_mut().focus_hole, hole);
        if hole && !was {
            let state = self.screen_state(ctx);
            log::warn!(
                target: "egui_pop",
                "a wizard holds the main window as a focus screen and no page of it was drawn: \
                 the shell is drawn instead; {state}"
            );
            self.publish("egui_pop", None, "window.focus_hole", state);
            ctx.request_repaint();
        } else if was && !hole {
            log::info!(target: "egui_pop", "the focus screen's hole is closed");
        }
        self.watch_empty(ui, page);
        // The registry against the backend, at the end of every pass of
        // the main window (ADR 0029, invariant 1, and 3 to 5).
        self.check_invariants_in_pass(ctx);
    }

    fn watch_empty(&self, ui: &Ui, page: bool) {
        let ctx = ui.ctx();
        let (pass_start, start, delay, shown, first_said, skip) = {
            let mut g = self.0.paint_guard.borrow_mut();
            let first = std::mem::replace(&mut g.first_said, true);
            let skip = std::mem::take(&mut g.placeholders);
            (
                g.pass_start.take(),
                g.start.take(),
                g.delay,
                g.shown,
                first,
                skip,
            )
        };
        // Without a shell (a window's content moved into the main window,
        // a pass that stopped before its title bar), the whole pass is
        // looked at: a window without its decorations is watched too.
        let Some((layer, from)) = start.or(pass_start) else {
            return;
        };
        // The title bar drew something: `None` when no shell was drawn.
        let title_bar = match (pass_start, start) {
            (Some((_, a)), Some((_, b))) => Some(has_content(ctx, layer, a, Some(b), &skip)),
            _ => None,
        };
        let content = has_content(ctx, layer, from, None, &skip);
        if !first_said {
            log::info!(
                target: "egui_pop",
                "the main window's first pass: content painted: {content}, title bar painted: \
                 {title_bar:?}, shell drawn: {}; {}",
                start.is_some(),
                self.screen_state(ctx)
            );
        }
        let Some(delay) = delay else {
            return;
        };
        let painted = page || !overlay::stack(ctx).is_empty() || content;
        if painted {
            let mut g = self.0.paint_guard.borrow_mut();
            g.empty = None;
            if shown {
                g.shown = false;
                drop(g);
                log::info!(target: "egui_pop", "the main window paints again");
                self.invariant_holds(
                    crate::window_invariants::PAINTS,
                    crate::windows::WindowId::MAIN,
                );
            }
            return;
        }
        let now = Instant::now();
        let (since, passes) = {
            let mut g = self.0.paint_guard.borrow_mut();
            let e = g.empty.get_or_insert((now, 0));
            e.1 += 1;
            *e
        };
        if !shown {
            if passes < MIN_PASSES || now.duration_since(since) < delay {
                // An idle window runs no pass: come back to look.
                ctx.request_repaint_after(delay.max(Duration::from_millis(50)));
                return;
            }
            self.0.paint_guard.borrow_mut().shown = true;
            let mut state = self.screen_state(ctx);
            state["title_bar_painted"] = json!(title_bar);
            state["shell_drawn"] = json!(start.is_some());
            log::error!(
                target: "egui_pop",
                "the main window painted nothing (title bar painted: {title_bar:?}, shell \
                 drawn: {}) for {passes} passes ({:.1} s): the fallback page is shown; {state}",
                start.is_some(),
                now.duration_since(since).as_secs_f32()
            );
            self.publish("egui_pop", None, "window.painted_nothing", state);
            self.painted_nothing(
                crate::windows::WindowId::MAIN,
                passes,
                now.duration_since(since),
            );
        }
        self.fallback_page(ctx);
    }

    /// What holds the screen, for the log: the place stack and the modes,
    /// the Settings app and its wizard, the overlays held, the layout's
    /// areas with their panels.
    pub fn screen_state(&self, ctx: &egui::Context) -> Json {
        let admin = self.service(&crate::admin::ADMIN);
        let stack = crate::contexts::state(self).map(|s| {
            json!({
                "places": s.places.iter().map(|e| e.frame.to_string()).collect::<Vec<_>>(),
                "modes": s.modes.iter().map(|m| m.frame.to_string()).collect::<Vec<_>>(),
            })
        });
        let overlays: Vec<String> = overlay::stack(ctx)
            .iter()
            .map(|o| o.label.clone())
            .collect();
        let areas: Vec<Json> = {
            let wb = self.workbench_mut();
            wb.areas()
                .into_iter()
                .map(|a| {
                    json!({
                        "area": crate::workbench::area_name(a),
                        "panels": wb.panels_in(a).iter().map(ToString::to_string).collect::<Vec<_>>(),
                    })
                })
                .collect()
        };
        json!({
            "contexts": stack,
            "settings": {
                "open": admin.as_ref().is_some_and(|a| a.is_open()),
                "wizard": admin.as_ref().and_then(|a| a.wizard_flow()).is_some(),
                "focus_screen": admin.as_ref().is_some_and(|a| a.focus_screen()),
                "in_window": crate::admin::in_window(self),
            },
            "overlays": overlays,
            "zen": self.zen(),
            "own_frame": crate::frame::own_frame(self),
            "root_has_content": self.root_has_content(),
            "areas": areas,
        })
    }

    /// The end of an application window's pass (a viewport of its own):
    /// whether its content painted. A window whose passes paint nothing
    /// (no text, no image, no overlay under its title bar) for longer
    /// than the delay gets one error naming it and its state,
    /// `window.painted_nothing` (with `window`), and the fallback page;
    /// `true` while the page is to be drawn ([`AppHandle::app_window_fallback`]).
    pub(crate) fn watch_app_window(
        &self,
        ctx: &egui::Context,
        id: &crate::windows::WindowId,
        painted: bool,
    ) -> bool {
        let viewport = ctx.viewport_id();
        let delay = self.0.paint_guard.borrow().delay;
        let Some(delay) = delay else {
            return false;
        };
        let painted = painted || !overlay::stack(ctx).is_empty();
        let now = Instant::now();
        let (since, passes, shown) = {
            let mut g = self.0.paint_guard.borrow_mut();
            let w = g.windows.entry(viewport).or_default();
            if painted {
                let was = std::mem::take(&mut w.shown);
                w.empty = None;
                drop(g);
                if was {
                    log::info!(target: "egui_pop", "the window `{id}` paints again");
                    self.invariant_holds(crate::window_invariants::PAINTS, id.as_str());
                }
                return false;
            }
            let e = w.empty.get_or_insert((now, 0));
            e.1 += 1;
            (e.0, e.1, w.shown)
        };
        if shown {
            return true;
        }
        if passes < MIN_PASSES || now.duration_since(since) < delay {
            // An idle window runs no pass: come back to look.
            ctx.request_repaint_after(delay.max(Duration::from_millis(50)));
            return false;
        }
        if let Some(w) = self.0.paint_guard.borrow_mut().windows.get_mut(&viewport) {
            w.shown = true;
        }
        let mut state = self.screen_state(ctx);
        let info = self.windows().into_iter().find(|w| w.viewport == viewport);
        state["window"] = json!({
            "id": id,
            "viewport": format!("{viewport:?}"),
            "title": info.as_ref().map(|w| w.title.clone()),
            "focused": info.as_ref().and_then(|w| w.focused),
            "geometry": info.as_ref().and_then(|w| w.geometry),
            "minimized": ctx.input(|i| i.viewport().minimized),
        });
        log::error!(
            target: "egui_pop",
            "the window `{id}` ({viewport:?}) painted nothing for {passes} passes ({:.1} s): the \
             fallback page is shown; {state}",
            now.duration_since(since).as_secs_f32()
        );
        self.publish("egui_pop", None, "window.painted_nothing", state);
        self.painted_nothing(id.as_str(), passes, now.duration_since(since));
        true
    }

    /// Invariant 2 broken: a shown window painted nothing past the
    /// guard's passes and delay.
    fn painted_nothing(&self, window: &str, passes: u32, since: Duration) {
        let owner = self
            .windows_list()
            .into_iter()
            .find(|w| w.id == window)
            .map(|w| w.owner)
            .unwrap_or_default();
        self.invariant_broken(crate::window_invariants::painted_nothing(
            window,
            &owner,
            format!(
                "shown, painted nothing for {passes} passes ({:.1} s)",
                since.as_secs_f32()
            ),
        ));
    }

    /// The fallback page of an application window that painted nothing:
    /// what happened, and a button closing the window. `true` when it is
    /// clicked.
    pub(crate) fn app_window_fallback(&self, ctx: &egui::Context) -> bool {
        let mut close = false;
        egui::Area::new(Id::unique(("egui_pop.empty_app_window", ctx.viewport_id())))
            .order(Order::Foreground)
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                Kit::of(ctx)
                    .fault_card
                    .show(ui, State::REST, egui::Shadow::NONE, |ui| {
                        close = EmptyState::new(
                            Icon::Warning,
                            strings::tr(ctx, &txt::EMPTY_WINDOW_TITLE),
                        )
                        .text(strings::tr(ctx, &txt::EMPTY_WINDOW_BODY))
                        .action(strings::tr(ctx, &strings::CLOSE))
                        .show(ui);
                    });
            });
        if close {
            log::info!(target: "egui_pop", "empty window: closed from its fallback page");
        }
        close
    }

    /// An application window is closed: its watch goes, and its system
    /// window is expected gone by the delay ([`AppHandle::watch_closed_windows`]).
    pub(crate) fn app_window_closed(&self, viewport: egui::ViewportId, id: &str) {
        let mut g = self.0.paint_guard.borrow_mut();
        g.windows.remove(&viewport);
        g.closed.insert(
            viewport,
            Closed {
                id: id.to_string(),
                since: Instant::now(),
                seen: 0,
                said: false,
            },
        );
        drop(g);
        // Closed, it is no longer a shown window that paints nothing.
        self.invariant_holds(crate::window_invariants::PAINTS, id);
    }

    /// The id of the application window closed lately on `viewport`,
    /// while the backend may still list it.
    pub(crate) fn closed_window_id(&self, viewport: egui::ViewportId) -> Option<String> {
        self.0
            .paint_guard
            .borrow()
            .closed
            .get(&viewport)
            .map(|c| c.id.clone())
    }

    /// An application window opens on a viewport: what was kept of an
    /// earlier window of it goes.
    pub(crate) fn app_window_opened(&self, viewport: egui::ViewportId) {
        let mut g = self.0.paint_guard.borrow_mut();
        g.windows.remove(&viewport);
        g.closed.remove(&viewport);
        drop(g);
        forget_orphan(viewport);
    }

    /// From the main window's pass: a closed application window whose
    /// system window the backend still lists past the delay outlived its
    /// close (a window nothing draws in shows the clear colour alone: a
    /// black window). One error names it; the viewports returned are to
    /// be shown hidden for one pass, then dropped.
    pub(crate) fn watch_closed_windows(&self, ctx: &egui::Context) -> Vec<egui::ViewportId> {
        let live: Vec<egui::ViewportId> = ctx.input(|i| i.raw.viewports.keys().copied().collect());
        let now = Instant::now();
        let (late, waiting) = {
            let mut g = self.0.paint_guard.borrow_mut();
            // Gone from the backend: the close is done.
            g.closed.retain(|v, _| live.contains(v));
            let Some(delay) = g.delay else {
                return Vec::new();
            };
            let mut late = Vec::new();
            for (v, c) in &mut g.closed {
                c.seen += 1;
                // The main window's pass right after the close still sees
                // it: the backend drops it after that pass.
                if !c.said && c.seen >= 2 && now.duration_since(c.since) >= delay {
                    late.push((*v, c.id.clone(), now.duration_since(c.since)));
                    c.said = true;
                }
            }
            (late, g.closed.values().any(|c| !c.said))
        };
        if waiting {
            ctx.request_repaint_after(self.0.paint_guard.borrow().delay.unwrap_or(DEFAULT_DELAY));
        }
        late.into_iter()
            .map(|(v, id, since)| {
                let state = self.screen_state(ctx);
                log::error!(
                    target: "egui_pop",
                    "the window `{id}` ({v:?}) is closed, yet its system window is still there \
                     {:.1} s later: it is hidden; {state}",
                    since.as_secs_f32()
                );
                self.publish(
                    "egui_pop",
                    None,
                    "window.outlived",
                    json!({ "window": id, "viewport": format!("{v:?}") }),
                );
                v
            })
            .collect()
    }

    /// The fallback page, over the empty window: what happened, and two
    /// ways out through commands that exist.
    fn fallback_page(&self, ctx: &egui::Context) {
        let deep = crate::contexts::state(self).is_some_and(|s| s.depth() > 0);
        let mut reset = false;
        let mut back = false;
        egui::Area::new(Id::unique("egui_pop.empty_window"))
            .order(Order::Foreground)
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                Kit::of(ctx)
                    .fault_card
                    .show(ui, State::REST, egui::Shadow::NONE, |ui| {
                        reset = EmptyState::new(
                            Icon::Warning,
                            strings::tr(ctx, &txt::EMPTY_WINDOW_TITLE),
                        )
                        .text(strings::tr(ctx, &txt::EMPTY_WINDOW_BODY))
                        .action(strings::tr(ctx, &txt::EMPTY_WINDOW_RESET))
                        .show(ui);
                        if deep {
                            ui.vertical_centered(|ui| {
                                back = ui
                                    .button(strings::tr(ctx, &txt::EMPTY_WINDOW_BACK))
                                    .clicked();
                            });
                        }
                    });
            });
        if reset {
            log::info!(target: "egui_pop", "empty window: the arrangement is reset");
            // The Settings app open with nothing drawn would keep the
            // window: closed with it.
            if let Some(a) = self.service(&crate::admin::ADMIN) {
                a.close(self);
            }
            // Zen mode hides the bars and the title bar: left with it.
            if self.zen() {
                self.run(crate::workbench::CMD_LEAVE_ZEN, Json::Null);
            }
            self.run(crate::arrangements::CMD_RESET, Json::Null);
        }
        if back {
            log::info!(target: "egui_pop", "empty window: back to the first place");
            let _ = crate::contexts::request(
                self,
                Envelope::new(Request::Jump { depth: 0 }),
                &egui_pop_model::commands::Origin::User,
            );
        }
    }
}
