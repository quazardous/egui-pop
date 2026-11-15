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
    /// Since when the window paints nothing, and for how many passes.
    empty: Option<(Instant, u32)>,
    /// The fallback page is shown.
    shown: bool,
    /// `None`: the window is not watched.
    delay: Option<Duration>,
}

impl Default for Guard {
    fn default() -> Self {
        Guard {
            focus_hole: false,
            pass_start: None,
            start: None,
            first_said: false,
            empty: None,
            shown: false,
            delay: Some(DEFAULT_DELAY),
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
/// without one).
fn has_content(ctx: &egui::Context, layer: LayerId, from: usize, to: Option<usize>) -> bool {
    ctx.graphics(|g| {
        g.get(layer).is_some_and(|l| {
            l.all_entries()
                .take(to.unwrap_or(usize::MAX))
                .skip(from)
                .any(|s| is_content(&s.shape))
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
    }

    fn watch_empty(&self, ui: &Ui, page: bool) {
        let ctx = ui.ctx();
        let (pass_start, start, delay, shown, first_said) = {
            let mut g = self.0.paint_guard.borrow_mut();
            let first = std::mem::replace(&mut g.first_said, true);
            (g.pass_start.take(), g.start.take(), g.delay, g.shown, first)
        };
        // Without a shell (a window's content moved into the main window,
        // a pass that stopped before its title bar), the whole pass is
        // looked at: a window without its decorations is watched too.
        let Some((layer, from)) = start.or(pass_start) else {
            return;
        };
        // The title bar drew something: `None` when no shell was drawn.
        let title_bar = match (pass_start, start) {
            (Some((_, a)), Some((_, b))) => Some(has_content(ctx, layer, a, Some(b))),
            _ => None,
        };
        let content = has_content(ctx, layer, from, None);
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
                log::info!(target: "egui_pop", "the main window paints again");
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
                a.close();
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
