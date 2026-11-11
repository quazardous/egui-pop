//! Tooltips drawn with the kit's `tooltip` component: its fill, border,
//! radius, padding, shadow, text colour and size, and its placement
//! (`tooltip.placement`: `widget`, under the hovered widget from its
//! start edge as egui's; `pointer`, under the pointer from its position)
//! with `tooltip.offset` between them. At the pointer, `tooltip.offset-x`
//! moves it towards the end side too (its corner `offset-x` across and
//! `offset` down from the pointer), and where the window lacks room on
//! the end side or below, it goes to the pointer's other side, the same
//! distance away, rather than being pushed back under the pointer.
//!
//! egui's own [`egui::Response::on_hover_text`] keeps the popup's look:
//! [`TooltipExt::tip`] is its kit counterpart, with egui's timing (the
//! delay, the grace between neighbours), its accessibility (the text
//! describes the widget) and its tooltip layer.
//!
//! ```
//! use egui_pop_core::tooltip::TooltipExt as _;
//!
//! fn toolbar(ui: &mut egui::Ui) {
//!     ui.button("+ New").tip("Create a ticket");
//! }
//! ```

use egui::{
    Align, Align2, Context, InnerResponse, Pos2, Rect, RectAlign, Response, Style, Ui, Vec2,
    WidgetText, pos2, vec2,
};

use crate::kit::{Kit, State};
use crate::theme::Tokens;

/// Where a tooltip goes (`tooltip.placement`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipPlacement {
    /// Under the hovered widget, from its start edge (egui's).
    #[default]
    Widget,
    /// Under the pointer, from its position.
    Pointer,
}

impl TooltipPlacement {
    /// The theme's (`tooltip.placement`); `widget` when it gives none or
    /// an unknown word.
    pub fn of(ctx: &Context) -> TooltipPlacement {
        match Tokens::of(ctx).resolved().value("tooltip.placement") {
            Some(egui_pop_tokens::Value::Other(serde_json::Value::String(s))) if s == "pointer" => {
                TooltipPlacement::Pointer
            }
            _ => TooltipPlacement::Widget,
        }
    }
}

/// Kit tooltips on a widget's response.
pub trait TooltipExt: Sized {
    /// Shows `text` in the kit's tooltip while the widget is hovered and
    /// enabled, as [`Response::on_hover_text`] does with egui's.
    fn tip(self, text: impl Into<WidgetText>) -> Self;
    /// Shows `text` in the kit's tooltip while the widget is hovered and
    /// disabled (the reason it is), as
    /// [`Response::on_disabled_hover_text`].
    fn disabled_tip(self, text: impl Into<WidgetText>) -> Self;
    /// Shows `add` in the kit's tooltip while the widget is hovered and
    /// enabled, as [`Response::on_hover_ui`].
    fn tip_ui(self, add: impl FnOnce(&mut Ui)) -> Self;
}

impl TooltipExt for Response {
    fn tip(self, text: impl Into<WidgetText>) -> Self {
        let text = text.into();
        describe(&self, text.text());
        show(&self, true, |ui| text_ui(ui, text));
        self
    }

    fn disabled_tip(self, text: impl Into<WidgetText>) -> Self {
        let text = text.into();
        describe(&self, text.text());
        show(&self, false, |ui| text_ui(ui, text));
        self
    }

    fn tip_ui(self, add: impl FnOnce(&mut Ui)) -> Self {
        show(&self, true, add);
        self
    }
}

/// Shows `add` in the kit's tooltip for `response` when egui would show
/// a tooltip for it (`enabled`: for an enabled widget, else for a
/// disabled one); its content's result while shown.
pub fn show<R>(
    response: &Response,
    enabled: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<R>> {
    let ctx = &response.ctx;
    let kit = Kit::of(ctx);
    let tooltip = egui::containers::Tooltip::for_widget(response);
    let open = response.enabled() == enabled
        && egui::containers::Tooltip::should_show_tooltip(response, true);
    if !open {
        return None;
    }
    let data = &kit.tooltip.data;
    let mut tooltip = tooltip.gap(data.offset);
    if TooltipPlacement::of(ctx) == TooltipPlacement::Pointer {
        let pointer = ctx.pointer_hover_pos()?;
        let offset = vec2(offset_x(ctx), data.offset);
        // The size it had last pass (egui sizes it on its first).
        let size =
            egui::AreaState::load(ctx, egui::containers::Tooltip::tooltip_id(response.id, 0))
                .and_then(|s| s.size)
                .unwrap_or(Vec2::ZERO);
        let (pos, corner) = at_pointer(ctx.content_rect(), pointer, offset, size);
        tooltip.popup = tooltip
            .popup
            .anchor(egui::PopupAnchor::Position(pos))
            .align(RectAlign {
                parent: corner,
                child: corner,
            })
            .align_alternatives(&[])
            .gap(0.0);
    }
    // egui frames a tooltip with its popup's look, read from the style
    // its layer starts with: the kit's for the time it is laid out.
    let before = ctx.global_style();
    ctx.set_global_style(styled(&before, &kit, ctx.pixels_per_point()));
    let size = data.font_size;
    let shown = tooltip.show(|ui| {
        if size > 0.0 {
            for font in ui.style_mut().text_styles.values_mut() {
                font.size = size;
            }
        }
        ui.visuals_mut().override_text_color = Some(kit.tooltip.style(State::REST).text);
        add(ui)
    });
    ctx.set_global_style(before);
    shown
}

/// The theme's `tooltip.offset-x`: how far right of the pointer a
/// tooltip at the pointer starts; 0 when not given.
fn offset_x(ctx: &Context) -> f32 {
    Tokens::of(ctx)
        .resolved()
        .dimension("tooltip.offset-x")
        .unwrap_or(0.0)
}

/// Where a tooltip of `size` at `pointer` goes in `window`: the point
/// and which of its corners sits there. Its top left corner `offset`
/// right of and below the pointer; on the pointer's left (its right
/// edge `offset.x` before it) when it would pass the window's right
/// edge and fits on the left, above it likewise.
fn at_pointer(window: Rect, pointer: Pos2, offset: Vec2, size: Vec2) -> (Pos2, Align2) {
    let left = pointer.x + offset.x + size.x > window.right()
        && pointer.x - offset.x - size.x >= window.left();
    let up = pointer.y + offset.y + size.y > window.bottom()
        && pointer.y - offset.y - size.y >= window.top();
    let x = if left {
        pointer.x - offset.x
    } else {
        pointer.x + offset.x
    };
    let y = if up {
        pointer.y - offset.y
    } else {
        pointer.y + offset.y
    };
    let corner = Align2([
        if left { Align::Max } else { Align::Min },
        if up { Align::Max } else { Align::Min },
    ]);
    (pos2(x, y), corner)
}

/// The style a tooltip's layer starts with: `base`, its popup's look the
/// kit's `tooltip`.
fn styled(base: &Style, kit: &Kit, ppp: f32) -> Style {
    let mut s = base.clone();
    let component = &kit.tooltip;
    let frame = component.frame(State::REST, ppp);
    s.visuals.window_fill = frame.fill;
    s.visuals.window_stroke = frame.stroke;
    s.visuals.menu_corner_radius = frame.corner_radius;
    s.visuals.popup_shadow = component.data.shadow;
    s.spacing.menu_margin = frame.inner_margin;
    s
}

fn text_ui(ui: &mut Ui, text: WidgetText) {
    // As egui's: Area's sizing must not shrink a tooltip's text.
    ui.set_max_width(ui.spacing().tooltip_width);
    ui.add(egui::Label::new(text));
}

/// The text describes the widget for assistive technology (its name too
/// when it has none), as egui's `on_hover_text`.
fn describe(response: &Response, text: &str) {
    if response.ctx.has_accesskit_node(response.id) {
        response.ctx.accesskit_node_builder(response.id, |node| {
            if node.description().is_none() {
                node.set_description(text);
            }
            if node.label().is_none() {
                node.set_label(text);
            }
        });
    }
}
