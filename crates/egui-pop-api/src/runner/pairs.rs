//! Which window shows which viewport: plain bookkeeping, no winit, so it
//! is tested on every system.
//!
//! eframe hands out no viewport's window but the main one. A deferred
//! viewport is painted in its own window's `RedrawRequested`: the pass
//! that begins there is that viewport's ([`Pairs::painted`]). An
//! immediate viewport (a detached panel's window) is painted inside its
//! parent's pass, a pass nested in another ([`Pairs::nested`]); its own
//! window's `RedrawRequested` runs no pass at all, eframe repaints the
//! parent instead ([`Pairs::redraw_without_pass`]). Both are queued in
//! the order seen and paired first with first: eframe creates an
//! immediate viewport's window in the pass that first shows it, so the
//! windows' first redraws come in the order the viewports first showed.
//! A window guessed so that later begins a pass of its own was a
//! deferred one: the guess is undone and its viewport waits again.

use egui::ViewportId;

/// A window and the viewport it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pair {
    pub viewport: ViewportId,
    /// The winit window id (`u64::from(WindowId)`).
    pub window: u64,
    /// Its handles were recorded in the per-viewport registries.
    pub recorded: bool,
    /// Paired by order (an immediate viewport), not by a pass seen in
    /// the window's own redraw.
    pub guessed: bool,
}

/// The pairs, and the immediate viewports and windows still unpaired.
#[derive(Debug, Default)]
pub(crate) struct Pairs {
    pairs: Vec<Pair>,
    /// Immediate viewports seen in a nested pass, no window yet.
    viewports: Vec<ViewportId>,
    /// Windows whose redraw ran no pass, no viewport yet.
    windows: Vec<u64>,
}

impl Pairs {
    /// The window showing `viewport`.
    pub fn window_of(&self, viewport: ViewportId) -> Option<u64> {
        self.pairs
            .iter()
            .find(|p| p.viewport == viewport)
            .map(|p| p.window)
    }

    /// The viewports with a window, in the order paired.
    pub fn viewports(&self) -> impl Iterator<Item = ViewportId> + '_ {
        self.pairs.iter().map(|p| p.viewport)
    }

    /// Whether `window` is paired.
    pub fn knows(&self, window: u64) -> bool {
        self.pairs.iter().any(|p| p.window == window)
    }

    /// A pass of `viewport` began in `window`'s own redraw. True when the
    /// pair is new.
    pub fn painted(&mut self, window: u64, viewport: ViewportId) -> bool {
        self.windows.retain(|w| *w != window);
        self.viewports.retain(|v| *v != viewport);
        if let Some(i) = self.pairs.iter().position(|p| p.window == window) {
            let p = self.pairs[i];
            if p.viewport == viewport || !p.guessed {
                return false;
            }
            // A deferred window taken for an immediate one: undone.
            self.pairs.remove(i);
            self.viewports.insert(0, p.viewport);
        }
        if self.pairs.iter().any(|p| p.viewport == viewport) {
            return false;
        }
        self.pairs.push(Pair {
            viewport,
            window,
            recorded: false,
            guessed: false,
        });
        true
    }

    /// A pass of `viewport` began inside another pass: an immediate
    /// viewport. True when it was paired now.
    pub fn nested(&mut self, viewport: ViewportId) -> bool {
        if viewport == ViewportId::ROOT
            || self.pairs.iter().any(|p| p.viewport == viewport)
            || self.viewports.contains(&viewport)
        {
            return false;
        }
        self.viewports.push(viewport);
        self.match_waiting()
    }

    /// `window`'s redraw ran no pass: an immediate viewport's window (or a
    /// hidden one, which a later pass of its own sets right). True when it
    /// was paired now.
    pub fn redraw_without_pass(&mut self, window: u64) -> bool {
        if self.knows(window) || self.windows.contains(&window) {
            return false;
        }
        self.windows.push(window);
        self.match_waiting()
    }

    /// Pairs the waiting viewports and windows, first with first.
    fn match_waiting(&mut self) -> bool {
        let mut any = false;
        while !self.viewports.is_empty() && !self.windows.is_empty() {
            let viewport = self.viewports.remove(0);
            let window = self.windows.remove(0);
            self.pairs.push(Pair {
                viewport,
                window,
                recorded: false,
                guessed: true,
            });
            any = true;
        }
        any
    }

    /// The viewports still open (`open`): the others wait no more.
    pub fn retain_viewports(&mut self, open: &[ViewportId]) {
        self.viewports.retain(|v| open.contains(v));
    }

    /// The pairs whose handles are not recorded yet, marked recorded.
    pub fn take_unrecorded(&mut self) -> Vec<Pair> {
        self.pairs
            .iter_mut()
            .filter(|p| !p.recorded)
            .map(|p| {
                p.recorded = true;
                *p
            })
            .collect()
    }

    /// `window` was destroyed: its pair goes, returned.
    pub fn forget(&mut self, window: u64) -> Vec<Pair> {
        self.windows.retain(|w| *w != window);
        let (gone, kept) = std::mem::take(&mut self.pairs)
            .into_iter()
            .partition(|p| p.window == window);
        self.pairs = kept;
        gone
    }

    /// Everything forgotten (a new run).
    pub fn clear(&mut self) {
        *self = Pairs::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(name: &str) -> ViewportId {
        ViewportId::from_hash_of(name)
    }

    #[test]
    fn a_deferred_window_is_paired_by_its_own_pass() {
        let mut p = Pairs::default();
        assert!(p.painted(1, ViewportId::ROOT));
        assert!(p.painted(2, v("deferred")));
        assert!(!p.painted(2, v("deferred")));
        assert_eq!(p.window_of(v("deferred")), Some(2));
    }

    #[test]
    fn an_immediate_window_is_paired_with_its_nested_pass() {
        let mut p = Pairs::default();
        p.painted(1, ViewportId::ROOT);
        // The viewport first shows inside the main window's pass...
        assert!(!p.nested(v("panel")));
        // ...then its window's redraw runs no pass of its own.
        assert!(p.redraw_without_pass(5));
        assert_eq!(p.window_of(v("panel")), Some(5));
        // The other order pairs too.
        assert!(!p.redraw_without_pass(6));
        assert!(p.nested(v("other")));
        assert_eq!(p.window_of(v("other")), Some(6));
        // Seen again: nothing new.
        assert!(!p.nested(v("panel")));
        assert!(!p.redraw_without_pass(5));
    }

    #[test]
    fn several_immediate_windows_pair_in_the_order_they_came() {
        let mut p = Pairs::default();
        p.nested(v("a"));
        p.nested(v("b"));
        p.redraw_without_pass(10);
        p.redraw_without_pass(11);
        assert_eq!(p.window_of(v("a")), Some(10));
        assert_eq!(p.window_of(v("b")), Some(11));
    }

    #[test]
    fn a_wrong_guess_is_undone_by_the_window_s_own_pass() {
        let mut p = Pairs::default();
        p.nested(v("panel"));
        // A hidden deferred window's redraw ran no pass: taken for the
        // panel's.
        p.redraw_without_pass(7);
        assert_eq!(p.window_of(v("panel")), Some(7));
        // It then paints its own viewport: the panel waits again.
        assert!(p.painted(7, v("deferred")));
        assert_eq!(p.window_of(v("deferred")), Some(7));
        assert_eq!(p.window_of(v("panel")), None);
        p.redraw_without_pass(8);
        assert_eq!(p.window_of(v("panel")), Some(8));
    }

    #[test]
    fn a_destroyed_window_and_a_closed_viewport_go() {
        let mut p = Pairs::default();
        p.nested(v("panel"));
        p.redraw_without_pass(5);
        assert_eq!(p.take_unrecorded().len(), 1);
        assert!(p.take_unrecorded().is_empty());
        let gone = p.forget(5);
        assert_eq!(gone.len(), 1);
        assert_eq!(p.window_of(v("panel")), None);
        // A viewport closed before its window came waits no more.
        p.nested(v("closed"));
        p.retain_viewports(&[ViewportId::ROOT]);
        p.redraw_without_pass(9);
        assert_eq!(p.window_of(v("closed")), None);
        assert_eq!(p.viewports().count(), 0);
    }
}
