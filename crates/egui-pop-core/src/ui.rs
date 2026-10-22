//! Layout helpers named by role, for an application's own layout code:
//! text by role, spacing by intent, a card and a button in a tone,
//! drawn from the theme's tokens and kit.
//!
//! ```
//! use egui_pop_core::theme::{self, ThemeSpec};
//! use egui_pop_core::ui::{Space, Tone, UiPop};
//!
//! let ctx = egui::Context::default();
//! theme::install(&ctx, &ThemeSpec::new("pop-dark"));
//! let mut out = ctx.run_ui(Default::default(), |ui| {
//!     ui.section_title("Export");
//!     ui.muted("Writes a copy next to the original.");
//!     ui.space_role(Space::Group);
//!     ui.card(Tone::Neutral, |ui| {
//!         ui.label("IMG_0001.raw");
//!         ui.meta("24 MB, 12 October");
//!     });
//!     if ui.button_tone(Tone::Danger, "Delete").clicked() {
//!         // ...
//!     }
//! });
//! out.textures_delta.clear();
//! ```
//!
//! The helpers are few on purpose, so `Ui`'s completion list stays
//! short; [`role_text`] and [`Space::points`] serve the rest.

use egui::scroll_area::ScrollAreaOutput;
use egui::{ComboBox, InnerResponse, Response, RichText, ScrollArea, Shadow, Ui, WidgetText};

use crate::fonts::{self, Role, Weight};
use crate::kit::{Kit, State};
pub use crate::kit::{Stock, Tone};
pub use crate::theme::TextRole;
use crate::theme::Tokens;

/// Spacing by intent (`space.inline`, `group`, `section`, `page`): what
/// the space is for, each an alias of a step of the spacing scale, so
/// densities scale it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Space {
    /// Between items on one line.
    Inline,
    /// Between related groups of items.
    Group,
    /// Before a section of a panel or a page.
    Section,
    /// Around a page's content and between its main blocks.
    Page,
}

impl Space {
    pub const ALL: [Space; 4] = [Space::Inline, Space::Group, Space::Section, Space::Page];

    /// Its token (`space.group`).
    pub fn token(self) -> &'static str {
        match self {
            Space::Inline => "space.inline",
            Space::Group => "space.group",
            Space::Section => "space.section",
            Space::Page => "space.page",
        }
    }

    /// Its size in points in a theme.
    pub fn points(self, tokens: &Tokens) -> f32 {
        let s = &tokens.space;
        match self {
            Space::Inline => s.inline,
            Space::Group => s.group,
            Space::Section => s.section,
            Space::Page => s.page,
        }
    }
}

/// `text` in a role's size, colour and weight (`text-role.*`), for a
/// widget that takes a [`RichText`]. A weight other than regular is drawn
/// with the theme's interface font at that weight once the theme has
/// installed it (`fonts::family`), at the regular weight before.
pub fn role_text(ui: &Ui, role: TextRole, text: impl Into<String>) -> RichText {
    let t = Tokens::of(ui.ctx());
    let style = t.text_role.get(role);
    let rich = RichText::new(text)
        .size(style.size.max(4.0))
        .color(style.color);
    match role_family(ui.ctx(), style.weight) {
        Some(family) => rich.family(family),
        None => rich,
    }
}

/// The family drawing a weight of interface text: `None` for the
/// regular weight (egui's own family, or the preview's text style in
/// `fonts::with_preview`), and while the theme's weighted families are
/// not installed. In `fonts::with_preview`, the preview family.
fn role_family(ctx: &egui::Context, weight: Weight) -> Option<egui::FontFamily> {
    if weight.nearest_installed() == Weight::REGULAR {
        return None;
    }
    if fonts::previewing(ctx) {
        return fonts::preview_family(ctx, Role::Ui, weight);
    }
    let family = fonts::family(Role::Ui, weight);
    // Asking for a family egui has no fonts for panics: the theme's
    // fonts are in use from the pass after they are installed.
    ctx.fonts(|f| f.definitions().families.contains_key(&family))
        .then_some(family)
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for egui::Ui {}
}

/// Layout helpers named by role, on egui's [`Ui`]. Each is a few lines
/// over the theme's tokens and kit; see the [module](self).
pub trait UiPop: sealed::Sealed {
    /// A label in a text role ([`role_text`]).
    fn text_role(&mut self, role: TextRole, text: impl Into<String>) -> Response;

    /// A section's title (`text-role.section`).
    fn section_title(&mut self, text: impl Into<String>) -> Response;

    /// Secondary text: a hint, a description (`text-role.muted`).
    fn muted(&mut self, text: impl Into<String>) -> Response;

    /// Small print about an item: a date, a size (`text-role.meta`).
    fn meta(&mut self, text: impl Into<String>) -> Response;

    /// Space by intent along the layout's direction ([`Space`]); egui's
    /// own `add_space` takes points.
    fn space_role(&mut self, space: Space);

    /// `add` inside the kit's `card` in a tone ([`Tone::Neutral`] for
    /// the plain card).
    fn card<R>(&mut self, tone: Tone, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R>;

    /// egui's button in a tone of the kit's `button` (`button.tone.*`):
    /// a [`Stock::Button`] scope ([`UiPop::scoped`]), so the button keeps
    /// its layout, icon and shortcut text. [`Tone::Neutral`] is the plain
    /// button.
    fn button_tone(&mut self, tone: Tone, text: impl Into<WidgetText>) -> Response;

    /// `add` in a scope where egui's own widgets of a [`Stock`] kind
    /// (buttons, combo boxes, text fields, buttons shown on or off) are
    /// drawn from the kit's component, in a tone: its states become
    /// egui's widget visuals, so egui keeps drawing the widget
    /// ([`Kit::scope`]). Visuals compiled with the kit: the scope only
    /// copies them.
    fn scoped<R>(
        &mut self,
        stock: Stock,
        tone: Tone,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R>;

    /// Shows egui's combo box (`widgets::combo_box`, for the kit's
    /// arrow) with its box drawn from the kit's `combo-box`
    /// ([`Stock::ComboBox`]), `add` its list's content:
    /// `ComboBox::show_ui` in a scope.
    fn combo_box<R>(
        &mut self,
        combo: ComboBox,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>>;

    /// Shows egui's scroll area with its bars drawn from the kit's
    /// `scroll-bar` ([`Stock::ScrollBar`]: the thumb's fills and corners,
    /// the track), `add` its content, drawn in this `Ui`'s own style.
    /// Every scroll area already takes the bar's width, margin,
    /// placement and visibility from the theme (egui's `ScrollStyle`);
    /// this scope adds the colours, which egui shares with buttons.
    fn scroll_area<R>(
        &mut self,
        area: ScrollArea,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> ScrollAreaOutput<R>;
}

impl UiPop for Ui {
    fn text_role(&mut self, role: TextRole, text: impl Into<String>) -> Response {
        let rich = role_text(self, role, text);
        self.label(rich)
    }

    fn section_title(&mut self, text: impl Into<String>) -> Response {
        self.text_role(TextRole::Section, text)
    }

    fn muted(&mut self, text: impl Into<String>) -> Response {
        self.text_role(TextRole::Muted, text)
    }

    fn meta(&mut self, text: impl Into<String>) -> Response {
        self.text_role(TextRole::Meta, text)
    }

    fn space_role(&mut self, space: Space) {
        let points = space.points(&Tokens::of(self.ctx()));
        self.add_space(points);
    }

    fn card<R>(&mut self, tone: Tone, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let kit = Kit::of(self.ctx());
        kit.card
            .show(self, State::REST.toned(tone), Shadow::NONE, add)
    }

    fn button_tone(&mut self, tone: Tone, text: impl Into<WidgetText>) -> Response {
        if tone == Tone::Neutral {
            return self.button(text);
        }
        self.scoped(Stock::Button, tone, |ui| ui.button(text)).inner
    }

    fn scoped<R>(
        &mut self,
        stock: Stock,
        tone: Tone,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        Kit::of(self.ctx()).scope(self, stock, tone, add)
    }

    fn combo_box<R>(
        &mut self,
        combo: ComboBox,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        self.scoped(Stock::ComboBox, Tone::Neutral, |ui| combo.show_ui(ui, add))
            .inner
    }

    fn scroll_area<R>(
        &mut self,
        area: ScrollArea,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> ScrollAreaOutput<R> {
        // upstream: egui's `ScrollStyle` has no colours, the bars take
        // the widget states' (docs/upstream.md, "scroll bar colours").
        // egui paints the bars with the visuals of the `Ui` the area is
        // shown in, and builds its content's `Ui` from it: the content
        // gets the style outside the scope back.
        let style = self.style().clone();
        self.scoped(Stock::ScrollBar, Tone::Neutral, |ui| {
            area.show(ui, |ui| {
                ui.set_style(style);
                add(ui)
            })
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{self, ThemeSpec};

    fn pass(ctx: &egui::Context, add: impl FnMut(&mut Ui)) {
        let mut out = ctx.run_ui(egui::RawInput::default(), add);
        out.textures_delta.clear();
    }

    #[test]
    fn spacing_by_intent_reads_the_density() {
        let ctx = egui::Context::default();
        let t = theme::install(&ctx, &ThemeSpec::new("pop-dark"));
        assert_eq!(Space::Group.points(&t), t.space.m);
        assert_eq!(Space::Page.points(&t), t.space.xl);
        let compact = theme::install(
            &ctx,
            &ThemeSpec::new("pop-dark").density(theme::Density::Compact),
        );
        assert!(Space::Group.points(&compact) < Space::Group.points(&t));
        let mut moved = 0.0;
        pass(&ctx, |ui| {
            let before = ui.cursor().top();
            ui.space_role(Space::Section);
            moved = ui.cursor().top() - before;
        });
        assert_eq!(moved, compact.space.section);
    }

    #[test]
    fn a_role_s_weight_waits_for_the_theme_fonts() {
        let ctx = egui::Context::default();
        // No theme installed: no weighted family, nothing to panic on.
        pass(&ctx, |ui| {
            assert_eq!(role_family(ui.ctx(), Weight::SEMIBOLD), None);
            ui.text_role(TextRole::Title, "Title");
        });
        let t = theme::install(&ctx, &ThemeSpec::new("pop-dark"));
        for _ in 0..2 {
            pass(&ctx, |ui| {
                ui.section_title("Section");
                ui.muted("Muted");
                ui.meta("Meta");
                ui.text_role(TextRole::Title, "Title");
            });
        }
        assert_eq!(
            role_family(&ctx, t.text_role.section.weight),
            Some(fonts::family(Role::Ui, t.text_role.section.weight))
        );
        // Regular roles keep egui's own family.
        assert_eq!(role_family(&ctx, t.text_role.muted.weight), None);
    }

    #[test]
    fn a_toned_button_and_card_show_their_content() {
        let ctx = egui::Context::default();
        theme::install(&ctx, &ThemeSpec::new("pop-light"));
        pass(&ctx, |ui| {
            let fill = ui.visuals().widgets.inactive.weak_bg_fill;
            for tone in [Tone::Neutral, Tone::Accent, Tone::Danger, Tone::Muted] {
                let r = ui.button_tone(tone, "Go");
                assert!(r.rect.width() > 0.0);
            }
            // The tone stays in its scope.
            assert_eq!(ui.visuals().widgets.inactive.weak_bg_fill, fill);
            let inner = ui.card(Tone::Warning, |ui| ui.label("Inside").rect);
            assert!(inner.response.rect.contains_rect(inner.inner));
        });
    }
}
