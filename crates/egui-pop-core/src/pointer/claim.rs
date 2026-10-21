//! A widget's own use of the secondary button.
//!
//! The secondary button opens a context menu everywhere by default, and
//! a canvas may bind a secondary drag in its pointer profile (Blender /
//! node editors pan the graph with it). A widget that needs the right
//! click for something else (a curve editor deleting a point, a swatch
//! resetting its colour, a canvas of its own binding a right drag)
//! **claims** it on its rect, every pass it is drawn:
//!
//! ```
//! use egui_pop_core::pointer::claim::{self, SecondaryClaim};
//!
//! fn swatch(ui: &mut egui::Ui) {
//!     let r = ui.allocate_response(egui::vec2(24.0, 24.0), egui::Sense::click());
//!     claim::claim_secondary(&r, SecondaryClaim::new("Swatch").click("Resets the colour"));
//!     if r.secondary_clicked() {
//!         // reset
//!     }
//! }
//! ```
//!
//! While the pointer was pressed over a claim, neither a context menu
//! opened through [`context_menu`] (and `egui_pop_api::menus`' context
//! menus) nor a canvas's profile binding for that gesture fires: the
//! widget alone acts. A claim names the click, the drag or both; a
//! claimed click leaves a secondary drag to the canvas, and the other
//! way round.
//!
//! A claim lasts the pass it was made in and the next one, so a parent
//! that reads its response before its children are drawn sees them.
//! Claims under a layer drawn above them (a window, a popup) do not
//! count there.
//!
//! What a claim does, when it says so (a label), is kept as the
//! widget's **use** ([`uses`]): the shortcuts editor lists the uses seen
//! so far, and the `pointer` collector of the debug bridge shows them
//! with the claims in force ([`claims`]). [`declare_use`] lists a use
//! before its widget is first drawn.

use std::collections::BTreeMap;

use egui::{Context, Id, InnerResponse, LayerId, Pos2, Rect, Response, SetOpenCommand, Ui};
use egui_pop_model::keymap::{Button, Pointer};
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::state::Slot;

/// What a widget takes of the secondary button: the click, the drag, or
/// both, each with what it does there.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SecondaryClaim {
    /// The widget, as people know it (`Curve editor`): how the shortcuts
    /// editor and agents name it.
    pub widget: String,
    /// The secondary click is the widget's, doing this (empty: not said).
    pub click: Option<String>,
    /// The secondary drag is the widget's, doing this (empty: not said).
    pub drag: Option<String>,
}

impl SecondaryClaim {
    /// A claim of nothing yet, by a widget of this name.
    pub fn new(widget: impl Into<String>) -> Self {
        Self {
            widget: widget.into(),
            click: None,
            drag: None,
        }
    }

    /// Claims the secondary click, saying what it does (`""`: the
    /// shortcuts editor does not list it).
    pub fn click(mut self, does: impl Into<String>) -> Self {
        self.click = Some(does.into());
        self
    }

    /// Claims the secondary drag, saying what it does.
    pub fn drag(mut self, does: impl Into<String>) -> Self {
        self.drag = Some(does.into());
        self
    }

    /// Whether it claims this gesture.
    pub fn takes(&self, gesture: Secondary) -> bool {
        match gesture {
            Secondary::Click => self.click.is_some(),
            Secondary::Drag => self.drag.is_some(),
        }
    }

    /// Each gesture claimed with what it does (empty when not said).
    pub fn gestures(&self) -> Vec<(Secondary, &str)> {
        [
            (Secondary::Click, &self.click),
            (Secondary::Drag, &self.drag),
        ]
        .into_iter()
        .filter_map(|(g, does)| does.as_deref().map(|d| (g, d)))
        .collect()
    }
}

/// A gesture of the secondary button a widget can claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum Secondary {
    /// A press and release without moving (it opens context menus).
    Click,
    /// A press held while the pointer moves.
    Drag,
}

impl Secondary {
    /// Its pointer gesture: `SecondaryClick`, `SecondaryDrag`.
    pub fn pointer(self) -> Pointer {
        match self {
            Secondary::Click => Pointer::Click(Button::Secondary),
            Secondary::Drag => Pointer::Drag(Button::Secondary),
        }
    }
}

/// One claim in force.
#[derive(Clone, Debug, PartialEq)]
pub struct Claimed {
    /// The claiming widget's egui id.
    pub id: Id,
    pub layer: LayerId,
    /// Its rect, in the window's points (a scene's transform applied).
    pub rect: Rect,
    pub claim: SecondaryClaim,
}

/// One labelled use seen (or declared): what a widget does with a
/// gesture of the secondary button.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WidgetUse {
    pub widget: String,
    /// `SecondaryClick` or `SecondaryDrag`.
    pub gesture: String,
    pub does: String,
}

#[derive(Clone, Debug, Default)]
struct Claims {
    /// The pass `now` was made in.
    pass: u64,
    now: Vec<Claimed>,
    last: Vec<Claimed>,
    /// `(widget, gesture)` → what it does.
    uses: BTreeMap<(String, Secondary), String>,
}

impl Claims {
    /// Moves to `pass`: what was made before it becomes the last pass's
    /// (or goes, when older).
    fn at(&mut self, pass: u64) {
        if self.pass == pass {
            return;
        }
        self.last = if self.pass + 1 == pass {
            std::mem::take(&mut self.now)
        } else {
            Vec::new()
        };
        self.now.clear();
        self.pass = pass;
    }

    /// The claims of this pass, then those of the last one not made
    /// again.
    fn in_force(&self, pass: u64) -> Vec<Claimed> {
        if self.pass == pass {
            let mut out = self.now.clone();
            out.extend(
                self.last
                    .iter()
                    .filter(|c| !self.now.iter().any(|n| n.id == c.id))
                    .cloned(),
            );
            out
        } else if self.pass + 1 == pass {
            self.now.clone()
        } else {
            Vec::new()
        }
    }
}

const CLAIMS: Slot<Claims> = Slot::new("core.pointer.claims");

/// The widget of `response` claims the secondary button on its rect for
/// this pass (and the next): see the [module](self).
pub fn claim_secondary(response: &Response, claim: SecondaryClaim) {
    claim_secondary_rect(
        &response.ctx,
        response.id,
        response.layer_id,
        response.interact_rect,
        claim,
    );
}

/// As [`claim_secondary`], for a rect of a widget's (a handle, a point)
/// rather than its whole response.
pub fn claim_secondary_rect(
    ctx: &Context,
    id: Id,
    layer: LayerId,
    rect: Rect,
    claim: SecondaryClaim,
) {
    if claim.click.is_none() && claim.drag.is_none() {
        return;
    }
    let rect = ctx
        .layer_transform_to_global(layer)
        .map_or(rect, |t| t * rect);
    let pass = ctx.cumulative_pass_nr();
    CLAIMS.update(ctx, |c| {
        c.at(pass);
        for (g, does) in claim.gestures() {
            if !does.is_empty() {
                c.uses.insert((claim.widget.clone(), g), does.to_string());
            }
        }
        c.now.retain(|x| x.id != id);
        c.now.push(Claimed {
            id,
            layer,
            rect,
            claim,
        });
    });
}

/// Lists a widget's use of a gesture of the secondary button before the
/// widget is drawn (an application declaring its own canvas's), for the
/// shortcuts editor.
pub fn declare_use(ctx: &Context, widget: &str, gesture: Secondary, does: &str) {
    CLAIMS.update(ctx, |c| {
        c.uses
            .insert((widget.to_string(), gesture), does.to_string());
    });
}

/// The claims in force: made in this pass, or in the last one.
pub fn claims(ctx: &Context) -> Vec<Claimed> {
    let pass = ctx.cumulative_pass_nr();
    CLAIMS
        .get(ctx)
        .map(|c| c.in_force(pass))
        .unwrap_or_default()
}

/// The uses seen or declared, by widget then gesture.
pub fn uses(ctx: &Context) -> Vec<WidgetUse> {
    CLAIMS
        .get(ctx)
        .map(|c| {
            c.uses
                .iter()
                .map(|((widget, g), does)| WidgetUse {
                    widget: widget.clone(),
                    gesture: g.pointer().name(),
                    does: does.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The claim of a gesture at a point, if a widget made one there (the
/// last made wins).
pub fn claimed_at(ctx: &Context, at: Pos2, gesture: Secondary) -> Option<Claimed> {
    let list = claims(ctx);
    if list.is_empty() {
        return None;
    }
    let top = ctx.layer_id_at(at);
    list.into_iter().rev().find(|c| {
        c.claim.takes(gesture)
            && c.rect.contains(at)
            && !top.is_some_and(|t| t != c.layer && t.order > c.layer.order)
    })
}

/// The claim of a gesture where the pointer was pressed (where it is,
/// once released): what a canvas or a context menu asks before acting
/// on a secondary click or drag.
pub fn claimed(ctx: &Context, gesture: Secondary) -> Option<Claimed> {
    let at = ctx.input(|i| i.pointer.press_origin().or(i.pointer.interact_pos()))?;
    claimed_at(ctx, at, gesture)
}

/// Whether a secondary click on `response` opens its context menu: it
/// was clicked with the secondary button, and no widget claimed that
/// click there.
pub fn opens_context_menu(response: &Response) -> bool {
    response.secondary_clicked() && claimed(&response.ctx, Secondary::Click).is_none()
}

/// egui's [`Response::context_menu`], unless a widget claimed the
/// secondary click where it happened: then the menu does not open.
pub fn context_menu<R>(
    response: &Response,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<R>> {
    let open = if response.secondary_clicked() {
        opens_context_menu(response).then_some(SetOpenCommand::Bool(true))
    } else if response.clicked() {
        Some(SetOpenCommand::Bool(false))
    } else {
        None
    };
    egui::Popup::menu(response)
        .open_memory(open)
        .at_pointer_fixed()
        .show(add)
}

/// The claims in force and the uses seen, as the `pointer` collector
/// shows them.
pub fn report(ctx: &Context) -> Json {
    let claims: Vec<Json> = claims(ctx)
        .into_iter()
        .map(|c| {
            json!({
                "id": format!("{:?}", c.id),
                "widget": c.claim.widget,
                "rect": [c.rect.min.x, c.rect.min.y, c.rect.width(), c.rect.height()],
                "click": c.claim.click,
                "drag": c.claim.drag,
            })
        })
        .collect();
    json!({ "claims": claims, "uses": uses(ctx) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ctx: &Context, rect: Rect, claim: SecondaryClaim) {
        claim_secondary_rect(
            ctx,
            Id::new(&claim.widget),
            LayerId::background(),
            rect,
            claim,
        );
    }

    #[test]
    fn a_claim_covers_its_rect_and_its_gestures_only() {
        let ctx = Context::default();
        let rect = Rect::from_min_size(Pos2::new(10.0, 10.0), egui::vec2(20.0, 20.0));
        at(
            &ctx,
            rect,
            SecondaryClaim::new("curve").click("Deletes a point"),
        );
        let inside = Pos2::new(15.0, 15.0);
        assert!(claimed_at(&ctx, inside, Secondary::Click).is_some());
        assert!(claimed_at(&ctx, inside, Secondary::Drag).is_none());
        assert!(claimed_at(&ctx, Pos2::new(50.0, 50.0), Secondary::Click).is_none());
        assert_eq!(
            uses(&ctx),
            vec![WidgetUse {
                widget: "curve".into(),
                gesture: "SecondaryClick".into(),
                does: "Deletes a point".into(),
            }]
        );
    }

    #[test]
    fn an_unlabelled_claim_is_not_a_use() {
        let ctx = Context::default();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(20.0, 20.0));
        at(&ctx, rect, SecondaryClaim::new("pad").drag(""));
        assert!(claimed_at(&ctx, Pos2::new(5.0, 5.0), Secondary::Drag).is_some());
        assert!(uses(&ctx).is_empty());
    }
}
