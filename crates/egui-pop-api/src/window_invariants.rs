//! The window invariants: the registry checked against what is on screen
//! ([ADR 0029](https://github.com/quazardous/egui-pop/blob/main/docs/adr/0029-not-on-the-bus.md),
//! slice 2).
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>
//!
//! The registry ([`AppHandle::windows_list`]) is the one list of windows
//! and popups. Five rules say it agrees with what egui and the system
//! show; they are checked by `window.invariants`, the last listener of
//! every window and popup point, and again at the end of every pass of
//! the main window:
//!
//! 1. [`BACKEND`]: the registry's windows are the backend's viewports
//!    (egui's `raw.viewports`), none more, none fewer, past the one pass
//!    the backend needs to create or destroy one. Not checked while the
//!    context embeds its viewports (one system window: kittest, the web).
//! 2. [`PAINTS`]: a shown window painted something within the paint
//!    guard's passes and delay ([`crate::paint_guard::DEFAULT_DELAY`]).
//! 3. [`OWNER`]: no window or popup without an owner.
//! 4. [`POPUP_PARENT`]: a popup's parent exists.
//! 5. [`CHILD_ALIVE`]: a closed parent has no live child.
//!
//! [`CLOSED_LEFT`] is `window.close`'s own: a window it closed is still in
//! the registry once its listeners ran.
//!
//! A broken rule is a visible state, never a silent fix: one error line
//! naming the rule, the window, its owner and its state, one
//! `window.invariant` event on the journal (`rule`, `window`, `owner`,
//! `state`), and an entry in [`AppHandle::broken_invariants`] (the `bus`
//! collector of the debug bridge shows them). It is said once, until it
//! holds again (a line says so). Containment stays the paint guard's (its
//! fallback page, the hiding of a window that outlived its close). The
//! test kit fails a test on one (`egui_pop_test::windows::MultiWindow`,
//! the scenarios' `WindowInvariants`).

use std::collections::{BTreeMap, BTreeSet};

use egui::ViewportId;
use serde::Serialize;
use serde_json::json;

use crate::app::AppHandle;
use crate::bus::{WindowKind, WindowRecord};
use crate::windows::WindowId;

/// Rule 1: the registry's windows are the backend's viewports.
pub const BACKEND: &str = "registry-is-backend";
/// Rule 2: a shown window paints something.
pub const PAINTS: &str = "shown-paints";
/// Rule 3: every window and popup has an owner.
pub const OWNER: &str = "has-owner";
/// Rule 4: a popup's parent exists.
pub const POPUP_PARENT: &str = "popup-has-parent";
/// Rule 5: a closed parent has no live child.
pub const CHILD_ALIVE: &str = "no-child-of-closed-parent";
/// `window.close`'s check: the window it closed left the registry.
pub const CLOSED_LEFT: &str = "closed-left-registry";

/// The five rules, in the ADR's order.
pub const RULES: [&str; 5] = [BACKEND, PAINTS, OWNER, POPUP_PARENT, CHILD_ALIVE];

/// The rules read from the registry alone.
const STRUCTURE: [&str; 3] = [OWNER, POPUP_PARENT, CHILD_ALIVE];

/// The main window's passes a mismatch with the backend may last: the
/// one the backend needs to create or destroy a window.
const BACKEND_GRACE: u32 = 1;

/// How many broken invariants [`AppHandle::broken_invariants`] keeps.
const KEPT: usize = 256;

/// An invariant that did not hold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Broken {
    /// The rule ([`RULES`], [`CLOSED_LEFT`]).
    pub rule: String,
    /// The window or popup's id; a viewport the registry does not hold
    /// is named `viewport <id>`.
    pub window: String,
    /// Its owner as the registry holds it (empty when none, or unknown).
    pub owner: String,
    /// What is wrong, in words.
    pub state: String,
}

impl Broken {
    fn new(rule: &str, window: &str, owner: &str, state: impl Into<String>) -> Self {
        Self {
            rule: rule.to_string(),
            window: window.to_string(),
            owner: owner.to_string(),
            state: state.into(),
        }
    }

    fn key(&self) -> (String, String) {
        (self.rule.clone(), self.window.clone())
    }
}

/// What the checks remember between passes.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// Broken now, by rule and window: said once, until it holds again.
    open: BTreeSet<(String, String)>,
    /// The main window's passes in a row each viewport disagreed with
    /// the backend in, and how: `true` registered without a viewport,
    /// `false` a viewport not registered (a window closed then opened
    /// again in the next pass is two mismatches, each with its grace).
    mismatch: BTreeMap<(ViewportId, bool), u32>,
    /// Every one said, oldest first, at most [`KEPT`].
    said: Vec<Broken>,
}

/// The viewport a window of the registry is shown in by the backend:
/// the main window's, an application window's deferred one, a detached
/// panel's; none for a native window or a popup.
pub(crate) fn viewport_of(kind: WindowKind, id: &str) -> Option<ViewportId> {
    match kind {
        WindowKind::Main => Some(ViewportId::ROOT),
        WindowKind::App => Some(WindowId::new(id).deferred_viewport()),
        WindowKind::Panel => Some(crate::workbench::panel_viewport(
            id.strip_prefix(PANEL_PREFIX).unwrap_or(id),
        )),
        _ => None,
    }
}

/// A detached panel's window id in the registry: `panel:<panel id>`.
pub(crate) const PANEL_PREFIX: &str = "panel:";

/// `window.close`'s check broken: the window it closed is still held.
pub(crate) fn closed_left(req: &crate::bus::WindowRequest) -> Broken {
    Broken::new(
        CLOSED_LEFT,
        &req.window,
        &req.owner,
        format!("closed ({}), still in the registry", req.reason.as_str()),
    )
}

/// Invariant 2 broken (the paint guard's watch says it).
pub(crate) fn painted_nothing(window: &str, owner: &str, state: String) -> Broken {
    Broken::new(PAINTS, window, owner, state)
}

/// Rules 3 to 5, read from the registry alone.
pub(crate) fn structure(records: &[WindowRecord]) -> Vec<Broken> {
    let held = |id: &str| records.iter().any(|r| r.id == id);
    let mut out = Vec::new();
    for r in records {
        if r.owner.trim().is_empty() {
            out.push(Broken::new(
                OWNER,
                &r.id,
                "",
                format!("{:?} without an owner", r.kind),
            ));
        }
        match (r.kind, &r.parent) {
            (WindowKind::Popup, None) => out.push(Broken::new(
                POPUP_PARENT,
                &r.id,
                &r.owner,
                "a popup without a parent",
            )),
            (WindowKind::Popup, Some(p)) if !held(p) => out.push(Broken::new(
                POPUP_PARENT,
                &r.id,
                &r.owner,
                format!("open, its parent `{p}` is not open"),
            )),
            (_, Some(p)) if !held(p) => out.push(Broken::new(
                CHILD_ALIVE,
                &r.id,
                &r.owner,
                format!("open, its parent `{p}` closed"),
            )),
            _ => {}
        }
    }
    out
}

impl AppHandle {
    /// Every window invariant broken since the application started,
    /// oldest first (the last 256): the rule, the window, its owner, its
    /// state. Empty in a healthy application.
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn broken_invariants(&self) -> Vec<Broken> {
        egui_pop_core::lock(&self.0.bus.invariants).said.clone()
    }

    /// The invariants broken now (said and not yet holding again).
    ///
    /// <div class="warning">Experimental: may change in any release
    /// (docs/stability.md).</div>
    pub fn invariants_broken_now(&self) -> Vec<(String, String)> {
        egui_pop_core::lock(&self.0.bus.invariants)
            .open
            .iter()
            .cloned()
            .collect()
    }

    /// One invariant does not hold: said unless already said.
    pub(crate) fn invariant_broken(&self, b: Broken) {
        {
            let mut s = egui_pop_core::lock(&self.0.bus.invariants);
            if !s.open.insert(b.key()) {
                return;
            }
            if s.said.len() == KEPT {
                s.said.remove(0);
            }
            s.said.push(b.clone());
        }
        log::error!(
            target: "egui_pop",
            "window invariant `{}` broken by `{}` (owner {}): {}",
            b.rule,
            b.window,
            if b.owner.is_empty() { "none" } else { &b.owner },
            b.state,
        );
        self.publish(
            "egui_pop",
            None,
            "window.invariant",
            json!({ "rule": b.rule, "window": b.window, "owner": b.owner, "state": b.state }),
        );
    }

    /// An invariant broken by `window` holds again.
    pub(crate) fn invariant_holds(&self, rule: &str, window: &str) {
        let was = egui_pop_core::lock(&self.0.bus.invariants)
            .open
            .remove(&(rule.to_string(), window.to_string()));
        if was {
            log::info!(
                target: "egui_pop",
                "window invariant `{rule}` holds again for `{window}`"
            );
        }
    }

    /// The rules of `rules` found broken now are `found`: the new ones
    /// said, the ones that hold again forgotten.
    fn reconcile(&self, rules: &[&str], found: Vec<Broken>) {
        let now: BTreeSet<(String, String)> = found.iter().map(Broken::key).collect();
        let healed: Vec<(String, String)> = egui_pop_core::lock(&self.0.bus.invariants)
            .open
            .iter()
            .filter(|(r, _)| rules.contains(&r.as_str()))
            .filter(|k| !now.contains(*k))
            .cloned()
            .collect();
        for (rule, window) in healed {
            self.invariant_holds(&rule, &window);
        }
        for b in found {
            self.invariant_broken(b);
        }
    }

    /// `window.invariants` after a request: the rules the registry alone
    /// answers.
    pub(crate) fn check_invariants_after_request(&self) {
        let records = self.windows_list();
        self.reconcile(&STRUCTURE, structure(&records));
    }

    /// The end of the main window's pass: every rule the registry and
    /// the backend answer (the paint rule is the paint guard's, as it
    /// watches each window's passes).
    pub(crate) fn check_invariants_in_pass(&self, ctx: &egui::Context) {
        if ctx.viewport_id() != ViewportId::ROOT {
            return;
        }
        let records = self.windows_list();
        let mut found = structure(&records);
        found.extend(self.backend_mismatch(ctx, &records));
        self.reconcile(&[OWNER, POPUP_PARENT, CHILD_ALIVE, BACKEND], found);
    }

    /// Rule 1: what the registry holds against the viewports the backend
    /// lists, a mismatch broken once it lasts past the grace pass.
    fn backend_mismatch(&self, ctx: &egui::Context, records: &[WindowRecord]) -> Vec<Broken> {
        if ctx.embed_viewports() {
            egui_pop_core::lock(&self.0.bus.invariants).mismatch.clear();
            return Vec::new();
        }
        let expected: BTreeMap<ViewportId, &WindowRecord> = records
            .iter()
            .filter_map(|r| viewport_of(r.kind, &r.id).map(|v| (v, r)))
            .collect();
        let live: BTreeSet<ViewportId> = ctx.input(|i| i.raw.viewports.keys().copied().collect());
        // The main window is the backend's own: always there.
        let mut off: Vec<((ViewportId, bool), Broken)> = Vec::new();
        for (v, r) in &expected {
            if *v != ViewportId::ROOT && !live.contains(v) {
                off.push((
                    (*v, true),
                    Broken::new(
                        BACKEND,
                        &r.id,
                        &r.owner,
                        format!("open in the registry, the backend has no viewport {v:?}"),
                    ),
                ));
            }
        }
        for v in &live {
            if *v != ViewportId::ROOT && !expected.contains_key(v) {
                // An application window closed and still listed is named.
                let window = self
                    .windows_ref()
                    .entries
                    .iter()
                    .find(|e| e.id.deferred_viewport() == *v)
                    .map(|e| e.id.as_str().to_string())
                    .or_else(|| self.closed_window_id(*v))
                    .unwrap_or_else(|| format!("viewport {v:?}"));
                off.push((
                    (*v, false),
                    Broken::new(
                        BACKEND,
                        &window,
                        "",
                        format!("viewport {v:?} shown by the backend, not in the registry"),
                    ),
                ));
            }
        }
        let mut s = egui_pop_core::lock(&self.0.bus.invariants);
        let previous = std::mem::take(&mut s.mismatch);
        let mut out = Vec::new();
        for (v, b) in off {
            let passes = previous.get(&v).copied().unwrap_or(0) + 1;
            s.mismatch.insert(v, passes);
            if passes > BACKEND_GRACE {
                out.push(b);
            }
        }
        out
    }
}
