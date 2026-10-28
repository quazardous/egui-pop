//! The card a tip shows by itself: beside the element it is about (its
//! target, named where it is drawn), never covering it, with a halo
//! around the element; in the bottom corner at the start without one.
//! It never takes the focus and is never modal.

use std::collections::BTreeMap;

use egui::{Color32, Context, Id, LayerId, Order, Rect, Response, Stroke, StrokeKind, Ui};
use egui_pop_core::icon::{Icon, Size};
use egui_pop_core::kit::Kit;
use egui_pop_core::placement::{Placement, Side, place};
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use serde_json::{Value as Json, json};

use super::{TIPS, Tip, Tips, card_in};
use crate::app::AppHandle;
use crate::strings as txt;

/// The targets drawn, by name: the pass each was drawn in and where.
#[derive(Clone, Default)]
struct Targets(BTreeMap<String, (u64, Rect)>);

fn targets_id() -> Id {
    Id::new("egui_pop.tips.targets")
}

fn card_id() -> Id {
    Id::new("egui_pop.tips.card")
}

/// Names a rectangle a tip may point at (a tip's `target`): call it each
/// pass the element is drawn. Names are stable ids
/// (`photos.rating`, `param:exposure`).
pub fn target(ctx: &Context, name: &str, rect: Rect) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Targets>(targets_id())
            .0
            .insert(name.to_string(), (pass, rect));
    });
}

/// Names a widget a tip may point at; returns the response.
pub fn mark(response: Response, name: &str) -> Response {
    target(&response.ctx, name, response.rect);
    response
}

/// Where a target is, when it was drawn in this pass or the last.
pub fn target_rect(ctx: &Context, name: &str) -> Option<Rect> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| {
        d.get_temp::<Targets>(targets_id())
            .and_then(|t| t.0.get(name).copied())
    })
    .filter(|(p, _)| p + 1 >= pass)
    .map(|(_, r)| r)
}

/// Every target named so far in this session (for the lint).
pub fn targets_seen(ctx: &Context) -> Vec<String> {
    ctx.data(|d| {
        d.get_temp::<Targets>(targets_id())
            .map(|t| t.0.keys().cloned().collect())
            .unwrap_or_default()
    })
}

/// The halo's colour: the theme's `tip-card.halo-color` when it has one,
/// else the information status colour (the card's icon's).
pub fn halo_color(t: &Tokens) -> Color32 {
    t.color("tip-card.halo-color").unwrap_or(t.status.info)
}

/// Draws a halo around a target, over the interface and under the card.
fn halo(ctx: &Context, t: &Tokens, rect: Rect) {
    let color = halo_color(t);
    let painter = ctx.layer_painter(LayerId::new(
        Order::Foreground,
        Id::new("egui_pop.tips.halo"),
    ));
    let r = rect.expand(t.space.s);
    let radius = t.radius.widget + t.space.s;
    painter.rect_stroke(
        r.expand(t.space.s),
        radius + t.space.s,
        Stroke::new(t.space.s, color.gamma_multiply(0.35)),
        StrokeKind::Middle,
    );
    painter.rect_stroke(
        r,
        radius,
        Stroke::new(t.space.xs, color),
        StrokeKind::Middle,
    );
}

/// The card's width: the theme's `tip-card.width`, else a toast's.
fn width(ctx: &Context, t: &Tokens) -> f32 {
    t.resolved()
        .dimension("tip-card.width")
        .unwrap_or_else(|| Kit::of(ctx).toast.data.width)
}

/// Space between a target and the card.
fn gap(t: &Tokens) -> f32 {
    t.space.xl + t.space.m
}

/// Where the card goes, `size` big, in `bounds`: beside its target
/// (after it, else where there is room), or at the bottom start.
fn card_rect(anchor: Option<Rect>, size: egui::Vec2, bounds: Rect, gap: f32) -> Rect {
    match anchor {
        Some(a) => place(&Placement::new(a, size, bounds).side(Side::End).gap(gap)).rect,
        None => Rect::from_min_size(egui::pos2(bounds.left(), bounds.bottom() - size.y), size),
    }
}

/// The tip shown, for agents: its fields and where its card and target
/// are.
pub(super) fn describe(app: &AppHandle, tip: &Tip) -> Json {
    let ctx = app.egui();
    let rect = |r: Rect| json!([r.min.x, r.min.y, r.max.x, r.max.y]);
    let card = ctx.memory(|m| m.area_rect(card_id()));
    let target = tip.target.as_deref().and_then(|n| target_rect(&ctx, n));
    json!({
        "id": tip.id,
        "title": tip.title,
        "text": tip.text_shown(&ctx, app),
        "target": tip.target,
        "origin": tip.origin_name(),
        "card": card.map(rect),
        "targetRect": target.map(rect),
    })
}

/// Draws the card of the tip shown, in the active window.
pub(super) fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(tips) = app.service(&TIPS) else {
        return;
    };
    let ctx = ui.ctx().clone();
    if app.active_window() != ctx.viewport_id() {
        return;
    }
    let Some(tip) = tips.current() else {
        return;
    };
    if !tips.enabled(app) {
        tips.not_now(app);
        return;
    }
    show_card(&ctx, app, &tips, &tip);
}

fn show_card(ctx: &Context, app: &AppHandle, tips: &Tips, tip: &Tip) {
    let t = Tokens::of(ctx);
    let w = width(ctx, &t);
    let bounds = ctx.content_rect().shrink(t.space.l);
    let anchor = tip.target.as_deref().and_then(|n| target_rect(ctx, n));
    let size = ctx
        .memory(|m| m.area_rect(card_id()))
        .map_or(egui::vec2(w, w / 3.0), |r| r.size());
    let at = card_rect(anchor, size, bounds, gap(&t));
    if let Some(a) = anchor {
        halo(ctx, &t, a);
    }
    let heading = strings::tr(ctx, &txt::TIPS_DID_YOU_KNOW);
    let title = tip.title_in(ctx);
    let text = tip.text_shown(ctx, app);
    egui::Area::new(card_id())
        .order(Order::Foreground)
        .fixed_pos(at.min)
        .interactable(true)
        .show(ctx, |ui| {
            ui.set_max_width(w);
            let shadow = t.shadow.popup;
            card_in(
                ui,
                shadow,
                &heading,
                &title,
                &text,
                Some(|ui: &mut Ui| {
                    let label = strings::tr(ui.ctx(), &txt::TIPS_NOT_NOW);
                    if ui
                        .add(Icon::Close.size(Size::Inline).label(&label))
                        .clicked()
                    {
                        tips.not_now(app);
                    }
                }),
                |ui| {
                    let got = strings::tr(ui.ctx(), &txt::TIPS_GOT_IT);
                    let name =
                        strings::tr_args(ui.ctx(), &txt::TIPS_GOT_IT_NAME, &[("tip", &title)]);
                    let r = ui.button(&got);
                    egui_pop_core::a11y::name(&r, egui::WidgetType::Button, &name);
                    if r.clicked()
                        && let Err(e) = tips.got_it(app)
                    {
                        log::warn!("tips: {e}");
                    }
                    if ui.button(strings::tr(ui.ctx(), &txt::TIPS_NEXT)).clicked() {
                        tips.show_next(app);
                    }
                    if ui
                        .button(strings::tr(ui.ctx(), &txt::TIPS_NO_MORE))
                        .clicked()
                        && let Err(e) = tips.turn_off(app)
                    {
                        log::warn!("tips: {e}");
                    }
                },
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    #[test]
    fn the_card_goes_beside_its_target_without_covering_it() {
        let bounds = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let size = vec2(300.0, 120.0);
        let target = Rect::from_min_size(pos2(100.0, 100.0), vec2(80.0, 24.0));
        let r = card_rect(Some(target), size, bounds, 24.0);
        assert!(!r.intersects(target));
        assert_eq!(r.left(), target.right() + 24.0);
        // At the end of the window: on the other side.
        let target = Rect::from_min_size(pos2(700.0, 100.0), vec2(80.0, 24.0));
        let r = card_rect(Some(target), size, bounds, 24.0);
        assert!(!r.intersects(target));
        assert!(bounds.contains_rect(r));
        // Without a target: the bottom corner at the start.
        let r = card_rect(None, size, bounds, 24.0);
        assert_eq!(r.left_bottom(), bounds.left_bottom());
    }
}
