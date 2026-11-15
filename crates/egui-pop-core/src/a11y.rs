//! Accessibility helpers: names for custom widgets, and an announcer
//! whose messages screen readers read out (an AccessKit live region).
//!
//! ```
//! use egui_pop_core::a11y::{self, Politeness};
//!
//! let ctx = egui::Context::default();
//! a11y::announce(&ctx, "Saved", Politeness::Polite);
//! assert_eq!(a11y::announcements(&ctx)[0].text, "Saved");
//! ```

use egui::accesskit::{Live, Role};
use egui::{Context, Response, Ui, WidgetInfo};
use serde::Serialize;

use crate::state::{lock, shared};

/// Gives a custom widget its role and accessible name.
pub fn name(response: &Response, kind: Role, label: &str) {
    let enabled = response.enabled();
    let label = label.to_string();
    response.widget_info(move || WidgetInfo::labeled(kind, enabled, &label));
}

/// How urgently a message is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Politeness {
    /// When the reader is idle.
    Polite,
    /// At once, interrupting.
    Assertive,
}

/// One message.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Announcement {
    pub text: String,
    pub politeness: Politeness,
    /// Input time it was made at, in seconds.
    pub time: f64,
}

#[derive(Debug, Default)]
struct State {
    all: Vec<Announcement>,
    /// How many messages each politeness was given (polite, assertive),
    /// never trimmed: its parity marks a repeat.
    made: [u64; 2],
}

/// The mark a region's text alternates with, so the same text announced
/// twice in a row still changes the region: a no-break space at its end,
/// which readers do not say.
const REPEAT_MARK: char = '\u{a0}';

/// How many announcements are kept.
pub const KEPT: usize = 32;

/// Queues a message for screen readers.
pub fn announce(ctx: &Context, text: impl Into<String>, politeness: Politeness) {
    let time = ctx.input(|i| i.time);
    let st = shared::<State>(ctx, "core.a11y");
    let mut s = lock(&st);
    s.made[usize::from(politeness == Politeness::Assertive)] += 1;
    s.all.push(Announcement {
        text: text.into(),
        politeness,
        time,
    });
    let len = s.all.len();
    if len > KEPT {
        s.all.drain(..len - KEPT);
    }
    ctx.request_repaint();
}

/// The messages kept, oldest first.
pub fn announcements(ctx: &Context) -> Vec<Announcement> {
    let st = shared::<State>(ctx, "core.a11y");
    lock(&st).all.clone()
}

/// Draws the live regions (one polite, one assertive) holding the latest
/// messages. Invisible; call it once per pass, anywhere (the app shell
/// does). Every other message of a region ends with an invisible no-break
/// space, so the same text announced twice in a row changes the region
/// and is read twice.
pub fn live_regions(ui: &mut Ui) {
    let (all, made) = {
        let st = shared::<State>(ui.ctx(), "core.a11y");
        let s = lock(&st);
        (s.all.clone(), s.made)
    };
    for (politeness, live, salt) in [
        (Politeness::Polite, Live::Polite, "polite"),
        (Politeness::Assertive, Live::Assertive, "assertive"),
    ] {
        let mut text = all
            .iter()
            .rev()
            .find(|a| a.politeness == politeness)
            .map(|a| a.text.clone())
            .unwrap_or_default();
        if made[usize::from(politeness == Politeness::Assertive)] % 2 == 0 && !text.is_empty() {
            text.push(REPEAT_MARK);
        }
        let id = ui.scope_id().with(("egui_pop_live_region", salt));
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::ZERO, egui::Sense::hover());
        let _ = rect;
        ui.ctx().accesskit_node_builder(id, |node| {
            node.set_role(Role::Status);
            node.set_live(live);
            node.set_label(text);
        });
    }
}
