//! The main window's frames owed, for whoever runs the event loop.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! egui paints a window in its `RedrawRequested`, and on Wayland that
//! event comes only with the compositor's frame callback, which a
//! compositor withholds from a window it does not show: GNOME's, for a
//! main window covered by the application's other windows. eframe then
//! runs the application's logic only (`App::logic`), no pass of the main
//! window, until the system shows it again.
//!
//! The main window's pass is not only its own picture: it lets go of a
//! closed window (the backend drops the system windows its last pass no
//! longer declares) and it draws the detached panels' windows (egui's
//! immediate viewports, painted inside it). With the main window covered,
//! a closed Settings window stayed on screen frozen, and a detached
//! panel froze while it was the one shown, until a click or Alt+Tab
//! brought the main window forward.
//!
//! [`MainWake`] keeps what is owed: a repaint asked of the main window
//! (egui's repaint request, for it), or a redraw of a window that ran no
//! pass of its own (an immediate viewport: its pass is the main
//! window's); a pass of the main window pays it. Owed longer than
//! [`GRACE`], the loop paints the main window itself ([`MainWake::due`]),
//! as a `RedrawRequested` the system did not send. Safe where the present
//! does not wait for the compositor (egui-pop's present mode on Wayland,
//! `windows::present_mode`); a FIFO present to a hidden Wayland surface
//! can block, so a loop presenting that way does not paint
//! ([`MainWake::set_enabled`]).
//!
//! The window key `W` is the loop's window id (winit's `WindowId`); the
//! crate's native runner and a host running its own loop use it alike,
//! and the test kit runs it with viewport ids.

use std::time::{Duration, Instant};

/// How long a frame of the main window may be owed before the loop paints
/// it: well over a frame at 30 Hz, so a window the system shows always
/// gets its own redraw first; short enough that a closed window goes
/// without a wait anyone notices.
pub const GRACE: Duration = Duration::from_millis(100);

/// The main window's frame owed, and since when.
#[derive(Debug)]
pub struct MainWake<W> {
    main: Option<W>,
    enabled: bool,
    /// When the frame owed came due, and the main window's passes then.
    owed: Option<(Instant, u64)>,
    /// The main window's passes counted so far.
    passes: u64,
    /// The context's passes, counted by its begin-pass hook.
    counter: Option<PassCounter>,
}

impl<W> Default for MainWake<W> {
    fn default() -> Self {
        Self {
            main: None,
            enabled: true,
            owed: None,
            passes: 0,
            counter: None,
        }
    }
}

impl<W: Copy + PartialEq> MainWake<W> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The main window's id, once the loop knows it.
    pub fn set_main(&mut self, main: W) {
        self.main = Some(main);
    }

    pub fn main(&self) -> Option<W> {
        self.main
    }

    /// Counts the passes of the application's context
    /// ([`PassCounter::install`]): the main window's pay what is owed,
    /// and a redraw that began none drew an immediate viewport
    /// ([`MainWake::redraw`]).
    pub fn set_counter(&mut self, counter: PassCounter) {
        self.counter = Some(counter);
    }

    /// The main window's passes begun, from the counter.
    fn sync(&mut self) {
        if let Some(n) = self.counter.as_ref().map(PassCounter::main) {
            self.main_passes(n);
        }
    }

    /// Runs the system's (or the loop's) redraw of `window` (`run`:
    /// handing `RedrawRequested` to eframe); with a counter, a redraw of
    /// a window other than the main one that began no pass owes the main
    /// window a frame.
    pub fn redraw<R>(&mut self, window: W, run: impl FnOnce() -> R) -> R {
        self.sync();
        let before = self.counter.as_ref().map(PassCounter::all);
        let out = run();
        if let (Some(c), Some(before)) = (&self.counter, before) {
            let own_pass = c.all() > before;
            self.redrawn(window, own_pass, Instant::now());
        }
        self.sync();
        out
    }

    /// Whether the loop paints the main window when its frame is owed
    /// too long (off where a present to a hidden window can block).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// A repaint is asked of the main window, due at `at`: owed from
    /// then, unless a frame is owed already.
    pub fn asked(&mut self, at: Instant) {
        self.sync();
        let passes = self.passes;
        match &mut self.owed {
            Some((since, _)) if *since <= at => {}
            owed => *owed = Some((at, passes)),
        }
    }

    /// A window's redraw ran: `window`, with `passes` main-window passes
    /// begun during it. A redraw of another window that began no pass of
    /// its own (`own_pass` false: an immediate viewport, drawn by the
    /// main window's pass) owes the main window a frame.
    pub fn redrawn(&mut self, window: W, own_pass: bool, now: Instant) {
        if Some(window) != self.main && !own_pass {
            self.asked(now);
        }
    }

    /// A pass of the main window began: what was owed before it is paid.
    pub fn main_pass(&mut self) {
        self.main_passes(self.passes + 1);
    }

    /// The main window's passes begun so far ([`PassCounter::main`]):
    /// what was owed before the last of them is paid.
    pub fn main_passes(&mut self, passes: u64) {
        self.passes = self.passes.max(passes);
        if self.owed.is_some_and(|(_, at)| self.passes > at) {
            self.owed = None;
        }
    }

    /// The main window's passes counted so far.
    pub fn passes(&self) -> u64 {
        self.passes
    }

    /// Whether a frame of the main window is owed.
    pub fn owed(&self) -> bool {
        self.owed.is_some()
    }

    /// When the loop should look again ([`MainWake::due`]): the owed
    /// frame's time plus [`GRACE`].
    pub fn next(&self) -> Option<Instant> {
        if !self.enabled || self.main.is_none() {
            return None;
        }
        self.owed.map(|(since, _)| since + GRACE)
    }

    /// The main window to paint now, its frame owed longer than
    /// [`GRACE`] (the system sent no redraw for it). The loop paints it
    /// at once (a `RedrawRequested` of its own making); the pass that
    /// begins pays it.
    pub fn due(&mut self, now: Instant) -> Option<W> {
        self.sync();
        let at = self.next()?;
        if now < at {
            return None;
        }
        // Painted now: owed again only by what is asked from here on.
        self.owed = None;
        self.main
    }
}

/// The passes begun in a context, counted by a begin-pass hook: the main
/// window's, and every window's (a redraw that began none drew an
/// immediate viewport, whose pass is the main window's).
#[derive(Clone, Debug, Default)]
pub struct PassCounter {
    main: std::sync::Arc<std::sync::atomic::AtomicU64>,
    all: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl PassCounter {
    /// Counts `ctx`'s passes from now on.
    pub fn install(ctx: &egui::Context) -> Self {
        let counter = Self::default();
        let c = counter.clone();
        ctx.on_begin_pass(
            "egui_pop::main_wake",
            std::sync::Arc::new(move |ui: &mut egui::Ui| {
                use std::sync::atomic::Ordering;
                c.all.fetch_add(1, Ordering::Relaxed);
                if ui.ctx().viewport_id() == egui::ViewportId::ROOT {
                    c.main.fetch_add(1, Ordering::Relaxed);
                }
            }),
        );
        counter
    }

    /// The main window's passes begun.
    pub fn main(&self) -> u64 {
        self.main.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Every window's passes begun.
    pub fn all(&self) -> u64 {
        self.all.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// eframe's side: its repaint request for the main window (the event
/// egui's repaint callback sends to the loop) as eframe takes it, a
/// request outdated by a pass since being dropped.
#[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
impl<W: Copy + PartialEq> MainWake<W> {
    /// Notes eframe's `UserEvent::RequestRepaint` for the main window
    /// (`ctx`: the application's context). Call it before handing the
    /// event to eframe.
    pub fn user_event(&mut self, ctx: &egui::Context, event: &eframe::UserEvent) {
        #[allow(irrefutable_let_patterns)]
        if let eframe::UserEvent::RequestRepaint {
            viewport_id,
            when,
            cumulative_pass_nr,
        } = event
            && *viewport_id == egui::ViewportId::ROOT
        {
            let now = ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT);
            if now == *cumulative_pass_nr || now == cumulative_pass_nr + 1 {
                self.asked(*when);
            }
        }
    }
}

/// Whether the loop may paint a window the system does not show with
/// this present mode: not with a FIFO present (eframe's default), which on
/// Wayland waits for the compositor's frame callback that a hidden window
/// does not get; with one that does not wait (egui-pop's choice on
/// Wayland, `windows::present_mode`), or off Wayland, where nothing
/// withholds a window's redraw.
#[cfg(all(feature = "eframe", not(target_arch = "wasm32")))]
pub fn paint_safe(mode: eframe::wgpu::PresentMode) -> bool {
    use eframe::wgpu::PresentMode;
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    !wayland
        || !matches!(
            mode,
            PresentMode::AutoVsync | PresentMode::Fifo | PresentMode::FifoRelaxed
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: u8 = 0;
    const PANEL: u8 = 1;

    fn wake() -> (MainWake<u8>, Instant) {
        let mut w = MainWake::new();
        w.set_main(MAIN);
        (w, Instant::now())
    }

    #[test]
    fn nothing_owed_nothing_painted() {
        let (mut w, t) = wake();
        assert_eq!(w.next(), None);
        assert_eq!(w.due(t + GRACE * 10), None);
    }

    #[test]
    fn a_repaint_the_system_answers_is_not_painted_again() {
        let (mut w, t) = wake();
        w.asked(t);
        // The system's redraw came: a pass of the main window.
        w.main_pass();
        assert!(!w.owed());
        assert_eq!(w.due(t + GRACE * 2), None);
    }

    #[test]
    fn a_repaint_the_system_withholds_is_painted_after_the_grace() {
        let (mut w, t) = wake();
        w.asked(t);
        assert_eq!(w.due(t + GRACE / 2), None, "not before the grace");
        assert_eq!(w.next(), Some(t + GRACE));
        assert_eq!(w.due(t + GRACE), Some(MAIN));
        // Painted: nothing owed until asked again.
        assert_eq!(w.due(t + GRACE * 3), None);
    }

    #[test]
    fn a_pass_begun_before_the_request_does_not_pay_it() {
        let (mut w, t) = wake();
        w.main_pass();
        w.asked(t);
        assert!(w.owed());
        w.main_pass();
        assert!(!w.owed());
    }

    #[test]
    fn a_detached_panels_redraw_owes_the_main_window_a_frame() {
        let (mut w, t) = wake();
        // A deferred window painting itself owes nothing.
        w.redrawn(PANEL, true, t);
        assert!(!w.owed());
        // An immediate one is drawn by the main window's pass.
        w.redrawn(PANEL, false, t);
        assert_eq!(w.due(t + GRACE), Some(MAIN));
        // The main window's own redraw owes nothing more.
        w.redrawn(MAIN, false, t);
        assert!(!w.owed());
    }

    #[test]
    fn a_later_request_keeps_the_first_time_owed() {
        let (mut w, t) = wake();
        w.asked(t);
        w.asked(t + GRACE / 2);
        assert_eq!(w.next(), Some(t + GRACE));
        // A request for later moves nothing earlier.
        let (mut w, t) = wake();
        w.asked(t + GRACE);
        w.asked(t);
        assert_eq!(w.next(), Some(t + GRACE));
    }

    #[test]
    fn disabled_or_without_a_main_window_it_never_paints() {
        let (mut w, t) = wake();
        w.set_enabled(false);
        w.asked(t);
        assert_eq!(w.due(t + GRACE * 2), None);
        let mut w: MainWake<u8> = MainWake::new();
        w.asked(t);
        assert_eq!(w.due(t + GRACE * 2), None);
    }
}
