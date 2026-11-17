//! A panel's state on the bus (ADR 0029): whether what it shows is
//! current.
//!
//! A panel whose content follows something it does not hold (the photo
//! selected, a document, a source) may show content that no longer
//! matches it: stale until its work starts, updating while it runs,
//! failed when it could not. Whoever knows asks `panel.state`
//! ([`PanelStateSet`], a [`PanelStateAsk`] naming the panel), from any
//! thread; the workbench's listener ([`PANEL_CHROME`]) keeps it, and the
//! panel's chrome shows it: never the panel itself.
//!
//! - **Stale** and **updating**: the content is dimmed and a busy pip
//!   shows in the panel's corner and on its tab, both only once the
//!   state has lasted `motion.spinner.delay` (a fast update shows
//!   nothing), then for at least `motion.spinner.min`.
//! - **Failed**: the content is dimmed at once and a failed pip shows,
//!   the reason in its name and its tip.
//! - **Ready**: nothing; the state is forgotten.
//!
//! The tab holds one mark ([`TabMark`]): when a panel has more than one
//! to show, the one of the highest rank wins, the busy mark first.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use bus_contract::{Bus, ListenOpts, Point, PointKind};
use egui::{Context, Id, Rect, Response, Ui, UiBuilder};
use egui_pop_core::indicator::{Anchor, Indicator, Pip};
use egui_pop_core::{lock, motion, strings};
use serde::{Deserialize, Serialize};

use super::OWNER;
use crate::strings as txt;

/// Whether what a panel shows is current.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum PanelState {
    /// Current: nothing shown.
    #[default]
    Ready,
    /// It shows what no longer matches (the photo changed); its work has
    /// not started yet.
    Stale,
    /// Its work runs.
    Updating,
    /// Its work failed: why, in the person's words.
    Failed { reason: String },
}

impl PanelState {
    /// The state as the journal writes it (`updating`).
    pub fn as_str(&self) -> &'static str {
        match self {
            PanelState::Ready => "ready",
            PanelState::Stale => "stale",
            PanelState::Updating => "updating",
            PanelState::Failed { .. } => "failed",
        }
    }

    /// Whether its work is still to come or runs (stale or updating).
    pub fn is_busy(&self) -> bool {
        matches!(self, PanelState::Stale | PanelState::Updating)
    }
}

/// A panel's state asked: which panel, its state, who asks, and the
/// event that caused it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PanelStateAsk {
    /// The panel's id.
    pub panel: String,
    pub state: PanelState,
    /// Who asks: a plugin's id, a module's.
    pub owner: String,
    /// The journal sequence number of the event that caused it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<u64>,
}

impl PanelStateAsk {
    pub fn new(panel: impl Into<String>, state: PanelState, owner: impl Into<String>) -> Self {
        Self {
            panel: panel.into(),
            state,
            owner: owner.into(),
            cause: None,
        }
    }

    /// The journal event that caused it.
    pub fn cause(mut self, seq: u64) -> Self {
        self.cause = Some(seq);
        self
    }
}

/// A panel's state is asked (`panel.state`); its listeners say what they
/// did, one line each, in the value.
#[derive(Debug)]
pub struct PanelStateSet;
impl Point for PanelStateSet {
    const KIND: PointKind = PointKind::Request;
    const ID: &'static str = "panel.state";
    type Input = PanelStateAsk;
    type Value = Vec<String>;
}

/// The panel state points, in the order [`super::order_dump`] prints them.
pub const PANEL_STATE_POINTS: [&str; 1] = [PanelStateSet::ID];

/// The workbench's listener of `panel.state`: keeps each panel's state
/// for its chrome.
pub const PANEL_CHROME: &str = "workbench.panel_state";

/// The mark a panel's tab holds, one at a time: the highest rank wins
/// ([`TabMark::rank`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TabMark {
    /// Its content is stale or updating (shown after the delay).
    Busy,
    /// Its work failed: why.
    Failed(String),
}

impl TabMark {
    /// Its rank: the busy mark first, then a failure.
    pub fn rank(&self) -> u8 {
        match self {
            TabMark::Busy => 2,
            TabMark::Failed(_) => 1,
        }
    }

    /// Of the marks a tab may show, the one it shows.
    pub fn winner(marks: impl IntoIterator<Item = TabMark>) -> Option<TabMark> {
        marks.into_iter().max_by_key(TabMark::rank)
    }
}

/// A panel's state as kept: the state, and its episode: a new one
/// starts when a wait starts (stale or updating after neither) or a
/// failure is said, and restarts the delay. Ready ends a wait without
/// starting one, so a busy mark shown is kept for its minimum.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Kept {
    pub(crate) state: PanelState,
    pub(crate) episode: u64,
    /// When this episode was asked, or else first drawn (motion clock
    /// seconds).
    pub(crate) since: Option<f64>,
}

/// Every panel's state asked, by panel id.
pub(crate) type States = Arc<Mutex<BTreeMap<String, Kept>>>;

/// Registers the workbench's listener of `panel.state`; `wake` repaints
/// the interface (the state may be asked from another thread).
pub(super) fn install(kernel: &dyn Bus, states: &States, wake: Arc<Mutex<Option<egui::Context>>>) {
    let states = states.clone();
    let listened = kernel.listen::<PanelStateSet>(
        PANEL_CHROME,
        ListenOpts::new().owner(OWNER),
        move |ask, did| {
            let ctx = lock(&wake).clone();
            // When it was asked, by the motion clock: its delay counts
            // from there (from its first pass when no pass ran yet).
            let now = ctx.as_ref().map(motion::now);
            {
                let mut states = lock(&states);
                let entry = states.entry(ask.panel.clone()).or_insert(Kept {
                    state: PanelState::Ready,
                    episode: 0,
                    since: None,
                });
                if entry.state != ask.state {
                    // Stale then updating is one wait: its delay goes on.
                    let starts = match &ask.state {
                        PanelState::Ready => false,
                        PanelState::Failed { .. } => true,
                        _ => !entry.state.is_busy(),
                    };
                    if starts {
                        entry.episode += 1;
                        entry.since = now;
                    }
                    entry.state = ask.state.clone();
                    did.push(format!(
                        "{PANEL_CHROME}: {} {}",
                        ask.panel,
                        ask.state.as_str()
                    ));
                }
            }
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
            Ok(())
        },
    );
    debug_assert!(listened.is_ok(), "{listened:?}");
}

/// What a panel's chrome shows now.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Shown {
    Nothing,
    Busy,
    Failed(String),
}

impl Shown {
    fn mark(self) -> Option<TabMark> {
        match self {
            Shown::Nothing => None,
            Shown::Busy => Some(TabMark::Busy),
            Shown::Failed(why) => Some(TabMark::Failed(why)),
        }
    }
}

/// What `panel`'s chrome shows now: a busy state only once it has lasted
/// the delay, and for the minimum once shown ([`motion::Busy`]); a
/// failure at once.
pub(crate) fn shown(states: &States, ctx: &Context, panel: &str) -> Shown {
    let now = motion::now(ctx);
    let (state, episode, since) = {
        let mut states = lock(states);
        match states.get_mut(panel) {
            Some(k) => (k.state.clone(), k.episode, *k.since.get_or_insert(now)),
            None => return Shown::Nothing,
        }
    };
    let busy = motion::Busy::new(Id::unique(("egui_pop.panel_state", panel, episode)), since)
        .visible(ctx, state.is_busy());
    match state {
        PanelState::Failed { reason } => Shown::Failed(reason),
        _ if busy => Shown::Busy,
        _ => Shown::Nothing,
    }
}

/// The mark `panel`'s chrome shows, in `ui`'s language, by the indicator
/// id given.
fn indicator(ctx: &Context, mark: &TabMark, id: String) -> Indicator {
    match mark {
        TabMark::Busy => Indicator::pip(Pip::Busy)
            .id(id)
            .name(strings::tr(ctx, &txt::PANEL_STATE_UPDATING)),
        TabMark::Failed(why) => Indicator::pip(Pip::Failed).id(id).name(strings::tr_args(
            ctx,
            &txt::PANEL_STATE_FAILED,
            &[("reason", why)],
        )),
    }
}

/// The indicator id of a panel's mark on its content (`panel-state:<id>`)
/// and on its tab (`panel-state-tab:<id>`), for tests and agents.
pub fn mark_id(panel: &str, on_tab: bool) -> String {
    if on_tab {
        format!("panel-state-tab:{panel}")
    } else {
        format!("panel-state:{panel}")
    }
}

/// Over a panel's content just drawn in `rect`: dimmed, and its mark in
/// the top end corner, while it is not current.
pub(crate) fn paint_content(states: &States, ui: &mut Ui, panel: &str, rect: Rect) {
    let ctx = ui.ctx().clone();
    let Some(mark) = shown(states, &ctx, panel).mark() else {
        return;
    };
    let veil = ui.visuals().panel_fill.gamma_multiply(0.6);
    ui.painter_at(rect).rect_filled(rect, 0.0, veil);
    indicator(&ctx, &mark, mark_id(panel, false)).paint_at(ui, rect, Anchor::TopEnd);
}

/// On a panel's tab button: its one mark (the highest rank of those it
/// has), on the tab's top end corner, its meaning added to the tab's
/// name.
pub(crate) fn paint_tab(states: &States, tab: &Response, panel: &str) {
    let ctx = tab.ctx.clone();
    let Some(mark) = TabMark::winner(shown(states, &ctx, panel).mark()) else {
        return;
    };
    let mut ui = Ui::new(
        ctx.clone(),
        tab.id.with("egui_pop.panel_state"),
        UiBuilder::new().layer_id(tab.layer_id).max_rect(tab.rect),
    );
    indicator(&ctx, &mark, mark_id(panel, true))
        .overhang(true)
        .on(&mut ui, tab, Anchor::TopEnd);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_busy_mark_wins_the_tab() {
        let shown = TabMark::winner([TabMark::Failed("no data".into()), TabMark::Busy]);
        assert_eq!(shown, Some(TabMark::Busy));
        assert_eq!(TabMark::winner([]), None);
    }

    #[test]
    fn a_state_reads_as_the_journal_writes_it() {
        let ask = PanelStateAsk::new(
            "scopes.histogram",
            PanelState::Failed {
                reason: "no pixels".into(),
            },
            "helios",
        );
        let json = serde_json::to_value(&ask).unwrap();
        assert_eq!(json["state"]["state"], "failed");
        assert_eq!(json["state"]["reason"], "no pixels");
        assert_eq!(serde_json::from_value::<PanelStateAsk>(json).unwrap(), ask);
    }
}
