//! Tooltips drawn with the kit's `tooltip` component: its fill, border,
//! radius, padding, shadow, text colour and size, and its placement
//! (`tooltip.placement`: `widget`, under the hovered widget from its
//! start edge as egui's; `pointer`, under the pointer from its position)
//! with `tooltip.offset` between them.
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

use egui::{Context, InnerResponse, Response, Style, Ui, WidgetText};

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
        tooltip = tooltip.at_pointer();
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
