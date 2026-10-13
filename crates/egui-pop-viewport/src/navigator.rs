//! The zoom control and the navigator (a minimap): two views of a
//! viewport's [`View`], sharing its [`ViewportState`]. What they change
//! goes through the viewport's `before_view` hook, like the viewport's
//! own gestures ([`ViewportState::propose`]), and comes out as the
//! viewport's [`Signal::View`](crate::Signal) with the next frame.
//!
//! - [`ZoomControl`]: zoom out, the zoom in percent, zoom in, fit and
//!   100 %, one step of the zoom ladder at a time.
//! - [`Navigator`]: the whole image (a texture the application gives,
//!   its proxy) and a frame around the part in view. A drag of the frame
//!   pans (Ctrl finer, Shift coarser; Escape puts the view back), a
//!   press outside it centres the view there, the wheel zooms, a double
//!   click fits. Focused, the arrows pan, `+` and `-` zoom, `0` fits.
//!
//! Each has an [`on_change`](Navigator::on_change) hook, before the
//! viewport's: a [`Change`] of the [`View`], a drag's frames with
//! `done: false` and its end once more from the view before it.
//! `egui-pop-imaging` runs the finished ones as the `zoom.change` step
//! of an application.
//!
//! ```
//! use egui_pop_viewport::navigator::{Navigator, ZoomControl};
//! use egui_pop_viewport::{View, ViewportState};
//!
//! fn side_panel(ui: &mut egui::Ui, state: &mut ViewportState, view: &mut View, proxy: egui::TextureId) {
//!     ZoomControl::new(state, view).show(ui);
//!     Navigator::new(state, view).texture(proxy).show(ui);
//! }
//! ```

use egui::{
    Color32, Key, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, TextureId, Ui, Vec2,
    WidgetInfo, WidgetType, pos2, vec2,
};
use egui_pop_core::icon::{Icon, Size};
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;

use crate::change::{self, Cause, Change, Gesture, Hook, Verdict};
use crate::thumbnail::fitted;
use crate::view::{self, Mapping, View, Zoom};
use crate::widget::{ViewportState, zoom_percent};

/// The navigator's colours, from the theme (`navigator.*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavigatorTokens {
    pub background: Color32,
    pub frame: Color32,
    pub veil: Color32,
}

impl NavigatorTokens {
    pub fn of(ctx: &egui::Context) -> NavigatorTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        NavigatorTokens {
            background: c("navigator.background", t.surface.sunken),
            frame: c("navigator.frame", t.accent.default),
            veil: c("navigator.veil", t.surface.canvas),
        }
    }
}

/// What a zoom control or a navigator did in a frame.
#[derive(Debug)]
pub struct ViewOutput {
    /// The navigator, or the zoom control's row; `changed()` when the
    /// view changed.
    pub response: Response,
    /// The changes written this frame, after the hooks.
    pub changes: Vec<Change<View>>,
}

/// Proposes a view: the tool's hook, then the viewport's `before_view`
/// hook. Whether the view changed.
fn apply(
    state: &mut ViewportState,
    view: &mut View,
    to: View,
    cause: Cause,
    done: bool,
    hook: &mut Option<Hook<'_, View>>,
    changes: &mut Vec<Change<View>>,
) -> bool {
    if to == *view {
        return false;
    }
    let proposed = Change {
        from: *view,
        to,
        cause,
        done,
    };
    let Some(c) = change::decide(hook, proposed) else {
        return false;
    };
    if state.propose(view, c.to) {
        changes.push(Change { to: *view, ..c });
        true
    } else {
        false
    }
}

/// A view at another zoom around the view's centre (the image's centre
/// before the viewport has drawn).
fn zoomed(m: Option<Mapping>, f: impl Fn(f64) -> f64) -> Option<View> {
    let m = m?;
    Some(m.zoom_around(m.rect.center(), f(m.zoom)))
}

/// The zoom control: zoom out, the zoom, zoom in, fit, 100 %.
#[derive(Debug)]
pub struct ZoomControl<'a> {
    state: &'a mut ViewportState,
    view: &'a mut View,
    hook: Option<Hook<'a, View>>,
}

impl<'a> ZoomControl<'a> {
    pub fn new(state: &'a mut ViewportState, view: &'a mut View) -> ZoomControl<'a> {
        ZoomControl {
            state,
            view,
            hook: None,
        }
    }

    /// Sees each change before the viewport's hook: allow, veto or
    /// redirect (another view).
    pub fn on_change(mut self, hook: impl FnMut(&Change<View>) -> Verdict<View> + 'a) -> Self {
        self.hook = Some(Hook::new(hook));
        self
    }

    /// Draws the row; says what changed.
    pub fn show(self, ui: &mut Ui) -> ViewOutput {
        let ZoomControl {
            state,
            view,
            mut hook,
        } = self;
        let mut changes = Vec::new();
        let ctx = ui.ctx().clone();
        let tr = |k: &strings::StrKey| strings::tr(&ctx, k);
        let inner = ui.horizontal(|ui| {
            let m = state.mapping();
            let out = ui.add(
                Icon::Minus
                    .size(Size::Toolbar)
                    .label(&tr(&crate::strings::ZOOM_OUT))
                    .sense_click(),
            );
            if out.clicked()
                && let Some(to) = zoomed(m, view::zoom_out)
            {
                apply(
                    state,
                    view,
                    to,
                    Cause::Command,
                    true,
                    &mut hook,
                    &mut changes,
                );
            }
            let percent = zoom_percent(view, state.mapping().as_ref());
            let text = format!("{} %", percent_text(&ctx, percent));
            ui.add(egui::Label::new(text).selectable(false));
            let into = ui.add(
                Icon::Plus
                    .size(Size::Toolbar)
                    .label(&tr(&crate::strings::ZOOM_IN))
                    .sense_click(),
            );
            if into.clicked()
                && let Some(to) = zoomed(m, view::zoom_in)
            {
                apply(
                    state,
                    view,
                    to,
                    Cause::Command,
                    true,
                    &mut hook,
                    &mut changes,
                );
            }
            let fit =
                ui.add(egui::Button::new(tr(&crate::strings::ZOOM_FIT)).selected(view.is_fit()));
            if fit.clicked() {
                apply(
                    state,
                    view,
                    View::fit(),
                    Cause::Command,
                    true,
                    &mut hook,
                    &mut changes,
                );
            }
            let actual = ui.add(
                egui::Button::new(tr(&crate::strings::ZOOM_ACTUAL))
                    .selected(view.zoom == Zoom::Px(1.0)),
            );
            if actual.clicked() {
                let center = m.map_or(view.center, |m| m.center);
                apply(
                    state,
                    view,
                    View::actual(center),
                    Cause::Command,
                    true,
                    &mut hook,
                    &mut changes,
                );
            }
        });
        let mut response = inner.response;
        if !changes.is_empty() {
            response.mark_changed();
        }
        ViewOutput { response, changes }
    }
}

/// A zoom in percent, as the interface writes numbers: whole above 10 %,
/// one decimal below.
pub fn percent_text(ctx: &egui::Context, percent: f64) -> String {
    let decimals = if percent >= 10.0 || percent.fract() == 0.0 {
        0
    } else {
        1
    };
    strings::number(ctx, percent, decimals)
}

/// The navigator: the whole image and the part in view.
#[derive(Debug)]
pub struct Navigator<'a> {
    state: &'a mut ViewportState,
    view: &'a mut View,
    texture: Option<TextureId>,
    image_size: Option<[u32; 2]>,
    size: Option<Vec2>,
    label: Option<String>,
    hook: Option<Hook<'a, View>>,
}

/// A drag under way: the view before it, and the centre the pointer
/// took it to (image pixels).
#[derive(Clone, Copy, Debug)]
struct Grab {
    start: View,
    center: [f64; 2],
    cause: Cause,
}

impl<'a> Navigator<'a> {
    pub fn new(state: &'a mut ViewportState, view: &'a mut View) -> Navigator<'a> {
        Navigator {
            state,
            view,
            texture: None,
            image_size: None,
            size: None,
            label: None,
            hook: None,
        }
    }

    /// The whole image, small (the application's proxy).
    pub fn texture(mut self, texture: TextureId) -> Self {
        self.texture = Some(texture);
        self
    }

    /// The image's size in pixels (by default, the size the viewport
    /// drew last).
    pub fn image_size(mut self, size: [u32; 2]) -> Self {
        self.image_size = Some(size);
        self
    }

    /// The size in points (default: the available width, two thirds as
    /// high).
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = Some(size);
        self
    }

    /// The accessible name (default "Navigator").
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sees each change before the viewport's hook: allow, veto or
    /// redirect (another view).
    pub fn on_change(mut self, hook: impl FnMut(&Change<View>) -> Verdict<View> + 'a) -> Self {
        self.hook = Some(Hook::new(hook));
        self
    }

    /// Draws the navigator; says what changed.
    pub fn show(self, ui: &mut Ui) -> ViewOutput {
        let Navigator {
            state,
            view,
            texture,
            image_size,
            size,
            label,
            mut hook,
        } = self;
        let size = size.unwrap_or_else(|| {
            let w = ui.available_width().clamp(120.0, 320.0);
            vec2(w, (w * 2.0 / 3.0).round())
        });
        let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let m = state.mapping();
        let image = image_size
            .map(|s| [f64::from(s[0]), f64::from(s[1])])
            .or(m.map(|m| m.image))
            .filter(|i| i[0] > 0.0 && i[1] > 0.0);
        let mut changes = Vec::new();
        if let Some(image) = image {
            let fit = fitted(rect.shrink(2.0), [image[0] as u32, image[1] as u32]);
            let scale = f64::from(fit.width()) / image[0];
            let to_image = |p: Pos2| {
                [
                    f64::from(p.x - fit.left()) / scale,
                    f64::from(p.y - fit.top()) / scale,
                ]
            };
            gestures(
                ui,
                &response,
                state,
                view,
                m,
                image,
                scale,
                &to_image,
                &mut hook,
                &mut changes,
            );
            paint(
                ui,
                rect,
                fit,
                &response,
                texture,
                state.mapping().or(m),
                scale,
            );
        } else {
            let t = NavigatorTokens::of(ui.ctx());
            ui.painter()
                .rect_filled(rect, Tokens::of(ui.ctx()).radius.widget, t.background);
        }
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        if !changes.is_empty() {
            response.mark_changed();
        }
        let label = label.unwrap_or_else(|| strings::tr(ui.ctx(), &crate::strings::NAVIGATOR));
        let percent = zoom_percent(view, state.mapping().as_ref());
        let center = match view.zoom {
            Zoom::Fit => image.map_or([0.0, 0.0], |i| [i[0] / 2.0, i[1] / 2.0]),
            Zoom::Px(_) => view.center,
        };
        let value = strings::tr_args(
            ui.ctx(),
            &crate::strings::NAVIGATOR_VALUE,
            &[
                ("zoom", &percent_text(ui.ctx(), percent)),
                ("x", &strings::number(ui.ctx(), center[0], 0)),
                ("y", &strings::number(ui.ctx(), center[1], 0)),
            ],
        );
        let enabled = ui.is_enabled();
        response.widget_info(|| {
            let mut info = WidgetInfo::labeled(WidgetType::Other, enabled, &label);
            info.current_text_value = Some(value.clone());
            info
        });
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_role(egui::accesskit::Role::ScrollView);
        });
        ViewOutput { response, changes }
    }
}

/// The navigator's pointer, wheel and keys.
#[allow(clippy::too_many_arguments)]
fn gestures(
    ui: &mut Ui,
    response: &Response,
    state: &mut ViewportState,
    view: &mut View,
    m: Option<Mapping>,
    image: [f64; 2],
    scale: f64,
    to_image: &dyn Fn(Pos2) -> [f64; 2],
    hook: &mut Option<Hook<'_, View>>,
    changes: &mut Vec<Change<View>>,
) {
    let Some(m) = m else {
        return;
    };
    let grab_id = response.id.with("egui_pop_navigator_grab");
    let mut grab: Option<Grab> = ui.data(|d| d.get_temp(grab_id));
    let at_center = |c: [f64; 2]| View::at(m.zoom, c);
    let visible = m.visible();
    let pressed = ui.input(|i| i.pointer.primary_pressed()) && response.is_pointer_button_down_on();
    if pressed && let Some(pos) = response.interact_pointer_pos() {
        let p = to_image(pos);
        let inside =
            visible.is_some_and(|v| p[0] >= v[0] && p[0] <= v[2] && p[1] >= v[1] && p[1] <= v[3]);
        let mut g = Grab {
            start: *view,
            center: m.center,
            cause: Cause::Drag,
        };
        if !inside {
            g.center = [p[0].clamp(0.0, image[0]), p[1].clamp(0.0, image[1])];
            g.cause = Cause::Click;
            apply(
                state,
                view,
                at_center(g.center),
                Cause::Click,
                false,
                hook,
                changes,
            );
        }
        grab = Some(g);
    }
    match change::gesture(ui, response) {
        Gesture::None => {}
        Gesture::Moved(d) => {
            if let Some(g) = grab.as_mut() {
                g.center = [
                    g.center[0] + f64::from(d.x) / scale,
                    g.center[1] + f64::from(d.y) / scale,
                ];
                g.cause = Cause::Drag;
                apply(
                    state,
                    view,
                    at_center(g.center),
                    Cause::Drag,
                    false,
                    hook,
                    changes,
                );
            }
        }
        Gesture::Ended => {
            if let Some(g) = grab.take() {
                finish(state, view, g, hook, changes);
            }
        }
        Gesture::Cancelled => {
            if let Some(g) = grab.take() {
                *view = g.start;
            }
        }
    }
    if response.clicked()
        && let Some(g) = grab.take()
    {
        finish(state, view, g, hook, changes);
    }
    if response.double_clicked() {
        grab = None;
        apply(state, view, View::fit(), Cause::Reset, true, hook, changes);
    }
    // The wheel zooms around the view's centre, a ladder step a notch.
    if response.hovered() {
        let wheel: f32 = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                    _ => None,
                })
                .sum()
        });
        if wheel != 0.0 {
            let f: fn(f64) -> f64 = if wheel > 0.0 {
                view::zoom_in
            } else {
                view::zoom_out
            };
            if let Some(to) = zoomed(Some(m), f) {
                apply(state, view, to, Cause::Wheel, true, hook, changes);
            }
            ui.input_mut(|i| {
                i.smooth_scroll_delta = Vec2::ZERO;
                i.events
                    .retain(|e| !matches!(e, egui::Event::MouseWheel { .. }));
            });
        }
    }
    if response.has_focus() {
        change::lock_arrows(ui, response.id);
        let [x, y] = change::arrows(ui);
        if x != 0.0 || y != 0.0 {
            // A tenth of the part in view per press (Shift: all of it).
            let span = visible.map_or([image[0], image[1]], |v| [v[2] - v[0], v[3] - v[1]]);
            let s = f64::from(change::scale(ui)) / 10.0;
            let c = [
                m.center[0] + f64::from(x) * span[0] * s,
                m.center[1] - f64::from(y) * span[1] * s,
            ];
            apply(state, view, at_center(c), Cause::Key, true, hook, changes);
        }
        let key = |k: Key| ui.input(|i| i.key_pressed(k));
        let to = if key(Key::Plus) || key(Key::Equals) {
            zoomed(Some(m), view::zoom_in)
        } else if key(Key::Minus) {
            zoomed(Some(m), view::zoom_out)
        } else if key(Key::Num0) {
            Some(View::fit())
        } else {
            None
        };
        if let Some(to) = to {
            apply(state, view, to, Cause::Key, true, hook, changes);
        }
    }
    ui.data_mut(|d| match grab {
        Some(g) => {
            d.insert_temp(grab_id, g);
        }
        None => {
            d.remove::<Grab>(grab_id);
        }
    });
}

/// A drag's end: one done change from the view before it; a veto puts
/// it back.
fn finish(
    state: &mut ViewportState,
    view: &mut View,
    g: Grab,
    hook: &mut Option<Hook<'_, View>>,
    changes: &mut Vec<Change<View>>,
) {
    if g.start == *view {
        return;
    }
    let proposed = Change {
        from: g.start,
        to: *view,
        cause: g.cause,
        done: true,
    };
    match change::decide(hook, proposed) {
        Some(c) => {
            if c.to != *view {
                state.propose(view, c.to);
            }
            changes.push(Change { to: *view, ..c });
        }
        None => *view = g.start,
    }
}

/// The image, the veil over what is out of view, the frame.
fn paint(
    ui: &Ui,
    rect: Rect,
    fit: Rect,
    response: &Response,
    texture: Option<TextureId>,
    m: Option<Mapping>,
    scale: f64,
) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let t = NavigatorTokens::of(ui.ctx());
    let tokens = Tokens::of(ui.ctx());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, tokens.radius.widget, t.background);
    if let Some(tex) = texture {
        painter.image(
            tex,
            fit,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    if let Some(v) = m.and_then(|m| m.visible()) {
        let s = scale as f32;
        let frame = Rect::from_min_max(
            pos2(fit.left() + v[0] as f32 * s, fit.top() + v[1] as f32 * s),
            pos2(fit.left() + v[2] as f32 * s, fit.top() + v[3] as f32 * s),
        );
        // The veil: four bands around the frame.
        let veil = t.veil.gamma_multiply(0.6);
        for band in [
            Rect::from_min_max(fit.min, pos2(fit.right(), frame.top())),
            Rect::from_min_max(pos2(fit.left(), frame.bottom()), fit.max),
            Rect::from_min_max(
                pos2(fit.left(), frame.top()),
                pos2(frame.left(), frame.bottom()),
            ),
            Rect::from_min_max(
                pos2(frame.right(), frame.top()),
                pos2(fit.right(), frame.bottom()),
            ),
        ] {
            if band.is_positive() {
                painter.add(Shape::rect_filled(band, 0.0, veil));
            }
        }
        painter.rect_stroke(frame, 0.0, Stroke::new(1.5, t.frame), StrokeKind::Inside);
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            tokens.radius.widget,
            Stroke::new(2.0, tokens.focus_ring),
            StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percent_has_a_decimal_only_when_small_and_not_whole() {
        let ctx = egui::Context::default();
        assert_eq!(percent_text(&ctx, 100.0), "100");
        assert_eq!(percent_text(&ctx, 100.0 / 3.0), "33");
        assert_eq!(percent_text(&ctx, 100.0 / 12.0), "8.3");
        assert_eq!(percent_text(&ctx, 5.0), "5");
    }
}
