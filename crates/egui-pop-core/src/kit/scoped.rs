//! egui's own widgets restyled in a scope: a kit component's boxes
//! mapped onto egui's visuals (`widgets.*`, the selection, the text
//! field background) for the widgets egui draws itself, so a kit
//! restyles them without egui-pop drawing them again.
//!
//! What egui reads is fixed by egui: four widget states (inactive,
//! hovered, active, open), the selection for a selected button and a
//! focused text field's outline, one background for every text field.
//! Each [`Stock`] kind says which of a component's states go where (its
//! mapping); the result, per tone, is compiled with the kit
//! ([`ScopedKit`]), so a scope only copies values
//! ([`Kit::scope`](super::Kit::scope)).

use egui::style::WidgetVisuals;
use egui::{Color32, CornerRadius, Stroke, Visuals};

use super::{BoxStyle, Component, Kit, State, Tone};

/// The egui widgets a kit restyles in a scope, each through a component
/// of the kit (the `scoped` mapping of its boxes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stock {
    /// egui's `Button` (and the widgets drawn as one: `DragValue`, a
    /// `CollapsingHeader`'s header, a menu button), from `button`, by
    /// tone: rest, hover, active, and selected for an open menu, are
    /// egui's inactive, hovered, active and open states.
    Button,
    /// The box of egui's `ComboBox`, from `combo-box`: as `Button`,
    /// `selected` while its list is open.
    ComboBox,
    /// egui's `TextEdit`, from `field`, by tone: its background is the
    /// resting fill, its outline the rest and hover borders, and while
    /// it has the keyboard the `focus` border (egui's selection stroke).
    Field,
    /// egui's button shown on or off (`Button::selected`,
    /// `selectable_label`), from `toggle-button`: as `Button`, plus its
    /// `selected` fill and text while on (egui's selection colours).
    ToggleButton,
    /// The bars of egui's `ScrollArea`, from `scroll-bar`: the thumb's
    /// `rest`, `hover` and `active` fills are egui's inactive, hovered
    /// and active foreground colours, its `radius` their corners, and
    /// `track-fill` egui's `extreme_bg_color`. Only the bars: the
    /// content is drawn in the style outside the scope
    /// (`UiPop::scroll_area`).
    ScrollBar,
}

impl Stock {
    pub const ALL: [Stock; 5] = [
        Stock::Button,
        Stock::ComboBox,
        Stock::Field,
        Stock::ToggleButton,
        Stock::ScrollBar,
    ];

    /// The kit component it is drawn from.
    pub fn component(self) -> &'static str {
        match self {
            Stock::Button => "button",
            Stock::ComboBox => "combo-box",
            Stock::Field => "field",
            Stock::ToggleButton => "toggle-button",
            Stock::ScrollBar => "scroll-bar",
        }
    }

    fn of(self, kit: &Kit) -> &Component {
        match self {
            Stock::Button => &kit.button,
            Stock::ComboBox => &kit.combo_box,
            Stock::Field => &kit.field,
            Stock::ToggleButton => &kit.toggle_button,
            Stock::ScrollBar => &kit.scroll_bar,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// egui's visuals for a component in one tone, compiled with the kit.
/// [`ScopedVisuals::apply`] writes them into a scope's [`Visuals`]; a
/// field left `None` keeps egui's value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScopedVisuals {
    /// egui's inactive, hovered, active and open widget states, in that
    /// order. Their `expansion` is not the kit's: egui's is kept.
    pub widgets: [WidgetVisuals; 4],
    /// Every state while the scope is disabled, when the component's
    /// `disabled` box differs from its resting one (egui still fades it
    /// by its `disabled_alpha`).
    pub disabled: Option<WidgetVisuals>,
    /// A selected button's fill (egui's `selection.bg_fill`).
    pub selection_fill: Option<Color32>,
    /// A selected button's text, or a focused text field's outline
    /// (egui's `selection.stroke`).
    pub selection_stroke: Option<Stroke>,
    /// Text fields' background (egui's `text_edit_bg_color`).
    pub text_edit_bg: Option<Color32>,
    /// A scroll bar's track (egui's `extreme_bg_color`).
    pub extreme_bg: Option<Color32>,
}

/// A placeholder for a kit compiled without its scoped table (never
/// drawn: [`Kit::compile`] always fills it).
const NONE: ScopedVisuals = ScopedVisuals {
    widgets: [WIDGET_NONE; 4],
    disabled: None,
    selection_fill: None,
    selection_stroke: None,
    text_edit_bg: None,
    extreme_bg: None,
};

const WIDGET_NONE: WidgetVisuals = WidgetVisuals {
    bg_fill: Color32::PLACEHOLDER,
    weak_bg_fill: Color32::PLACEHOLDER,
    bg_stroke: Stroke::NONE,
    corner_radius: CornerRadius::ZERO,
    fg_stroke: Stroke::NONE,
    expansion: 0.0,
};

/// One of egui's widget states drawn as a kit box: its fill, border,
/// text colour and corner radius.
pub(crate) fn widget_visuals(b: &BoxStyle) -> WidgetVisuals {
    WidgetVisuals {
        bg_fill: b.fill,
        weak_bg_fill: b.fill,
        bg_stroke: Stroke::new(b.border_width, b.border_color),
        corner_radius: CornerRadius::same(b.radius.round().clamp(0.0, 255.0) as u8),
        fg_stroke: Stroke::new(1.0, b.text),
        expansion: 0.0,
    }
}

/// `src` into `dst`, keeping egui's expansion, its border snapped to
/// whole physical pixels at `ppp`.
fn set(dst: &mut WidgetVisuals, src: &WidgetVisuals, ppp: f32) {
    let expansion = dst.expansion;
    *dst = *src;
    dst.expansion = expansion;
    dst.bg_stroke.width = crate::scale::snap(dst.bg_stroke.width, ppp);
}

impl ScopedVisuals {
    fn compile(stock: Stock, kit: &Kit, tone: Tone) -> ScopedVisuals {
        let c = stock.of(kit);
        let b = |state: State| c.style_toned(tone, state);
        let rest = b(State::REST);
        let disabled = b(State::DISABLED);
        let mut s = ScopedVisuals {
            widgets: [
                widget_visuals(rest),
                widget_visuals(b(State::HOVER)),
                widget_visuals(b(State::ACTIVE)),
                widget_visuals(b(State::SELECTED)),
            ],
            disabled: (disabled != rest).then(|| widget_visuals(disabled)),
            ..NONE
        };
        match stock {
            Stock::Button | Stock::ComboBox => {}
            Stock::ToggleButton => {
                let on = b(State::SELECTED);
                s.selection_fill = Some(on.fill);
                s.selection_stroke = Some(Stroke::new(1.0, on.text));
            }
            Stock::Field => {
                // egui draws a focused field as `active`, its outline in
                // the selection stroke; it has no open state.
                let focus = b(State::FOCUS);
                s.widgets[2] = widget_visuals(focus);
                s.widgets[3] = widget_visuals(rest);
                s.selection_stroke = Some(Stroke::new(focus.border_width, focus.border_color));
                s.text_edit_bg = Some(rest.fill);
            }
            Stock::ScrollBar => {
                // egui draws the thumb in the state's foreground colour
                // (`ScrollStyle::foreground_color`, set with the theme)
                // and corners, the track in `extreme_bg_color`; a scroll
                // bar has no open state.
                for (i, state) in [State::REST, State::HOVER, State::ACTIVE]
                    .into_iter()
                    .enumerate()
                {
                    s.widgets[i].fg_stroke.color = b(state).fill;
                }
                s.widgets[3] = s.widgets[0];
                s.extreme_bg = Some(kit.scroll.track);
            }
        }
        s
    }

    /// Writes them into a scope's visuals: `enabled` is whether the scope
    /// is (a disabled scope draws every state in the `disabled` box when
    /// the kit gives one), `ppp` the physical pixels per point borders
    /// snap to.
    pub fn apply(&self, v: &mut Visuals, enabled: bool, ppp: f32) {
        let w = &mut v.widgets;
        for (dst, src) in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open]
            .into_iter()
            .zip(&self.widgets)
        {
            let src = match (&self.disabled, enabled) {
                (Some(d), false) => d,
                _ => src,
            };
            set(dst, src, ppp);
        }
        if let Some(fill) = self.selection_fill {
            v.selection.bg_fill = fill;
        }
        if let Some(mut stroke) = self.selection_stroke {
            stroke.width = crate::scale::snap(stroke.width, ppp);
            v.selection.stroke = stroke;
        }
        if let Some(bg) = self.text_edit_bg {
            v.text_edit_bg_color = Some(bg);
        }
        if let Some(bg) = self.extreme_bg {
            v.extreme_bg_color = bg;
        }
    }

    /// Only egui's four widget states (what `theme::visuals_with` gives
    /// every egui widget from `button`), not snapped.
    pub(crate) fn apply_widgets(&self, v: &mut Visuals) {
        let w = &mut v.widgets;
        for (dst, src) in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open]
            .into_iter()
            .zip(&self.widgets)
        {
            let expansion = dst.expansion;
            *dst = *src;
            dst.expansion = expansion;
        }
    }
}

/// The scoped visuals of every [`Stock`] kind, per tone (one entry for
/// a component whose tokens give no tone), compiled with the kit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScopedKit([Box<[ScopedVisuals]>; Stock::ALL.len()]);

impl ScopedKit {
    pub(super) fn compile(kit: &Kit) -> ScopedKit {
        ScopedKit(Stock::ALL.map(|stock| {
            if stock.of(kit).data.states.toned() {
                let mut all = vec![ScopedVisuals::compile(stock, kit, Tone::Neutral)];
                all.extend(
                    Tone::NAMED
                        .iter()
                        .map(|(tone, _)| ScopedVisuals::compile(stock, kit, *tone)),
                );
                all.into_boxed_slice()
            } else {
                Box::new([ScopedVisuals::compile(stock, kit, Tone::Neutral)])
            }
        }))
    }

    /// The visuals of a kind in a tone; a tone its component does not
    /// give is its own box (`Neutral`).
    pub fn get(&self, stock: Stock, tone: Tone) -> &ScopedVisuals {
        let table = &self.0[stock.index()];
        table
            .get(tone as usize)
            .or_else(|| table.first())
            .unwrap_or(&NONE)
    }
}

/// What egui cannot show of a scoped component's boxes: a kit giving it
/// is told, rather than seeing it dropped in silence. `button` and
/// `field` are checked with egui's global widgets (`egui_losses`).
pub(super) fn losses(kit: &Kit) -> Vec<String> {
    let mut lost = Vec::new();
    if kit.scroll_bar.shape != "scoped" {
        lost.push(format!(
            "scroll-bar.shape: egui draws it, so `{}` is not used (only `scoped`)",
            kit.scroll_bar.shape
        ));
    }
    for stock in [Stock::ComboBox, Stock::ToggleButton] {
        let c = stock.of(kit);
        let name = stock.component();
        if c.shape != "scoped" {
            lost.push(format!(
                "{name}.shape: egui draws it, so `{}` is not used (only `scoped`)",
                c.shape
            ));
        }
        for (state, sname) in [
            (State::REST, "rest"),
            (State::HOVER, "hover"),
            (State::ACTIVE, "active"),
            (State::SELECTED, "selected"),
        ] {
            // What it inherits from `button` is reported there.
            let (b, base) = (c.style(state), kit.button.style(state));
            let indicator = |b: &BoxStyle| (b.indicator_size, b.indicator_color);
            let ring = |b: &BoxStyle| (b.ring_width, b.ring_color);
            if b.indicator_size > 0.0
                && b.indicator_color.a() > 0
                && indicator(b) != indicator(base)
            {
                lost.push(format!("{name}.{sname}: egui draws no indicator"));
            }
            if b.ring_width > 0.0 && b.ring_color.a() > 0 && ring(b) != ring(base) {
                lost.push(format!("{name}.{sname}: egui draws no focus ring"));
            }
        }
    }
    let toggle = &kit.toggle_button;
    let (rest, on) = (toggle.style(State::REST), toggle.style(State::SELECTED));
    if (on.border_color, on.border_width, on.radius)
        != (rest.border_color, rest.border_width, rest.radius)
    {
        lost.push(
            "toggle-button.selected: egui keeps the state's outline and corners on a selected button"
                .into(),
        );
    }
    let field = &kit.field;
    let (rest, focus) = (field.style(State::REST), field.style(State::FOCUS));
    if focus.ring_width > 0.0 && focus.ring_color.a() > 0 {
        lost.push("field.focus: egui's text fields draw no focus ring".into());
    }
    for (state, sname) in [(State::HOVER, "hover"), (State::FOCUS, "focus")] {
        if field.style(state).fill != rest.fill {
            lost.push(format!(
                "field.{sname}.fill: egui's text fields keep their resting background"
            ));
        }
    }
    lost
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::Shapes;
    use crate::theme::{self, Tokens};
    use egui_pop_tokens::{ThemeSpec, Themes};

    fn tokens_with(yaml: &str) -> Tokens {
        let mut themes = Themes::default();
        themes
            .add_text("scoped-test", yaml)
            .expect("test theme reads");
        Tokens::new(themes.resolve(&ThemeSpec::new("scoped-test")))
    }

    const RESTYLED: &str = r##"
        $extends: "pop-dark"
        combo-box:
          rest:
            fill: {$type: "color", $value: "#102030"}
            radius: {$type: "dimension", $value: {value: 9, unit: "px"}}
          selected:
            fill: {$type: "color", $value: "#203040"}
        toggle-button:
          selected:
            fill: {$type: "color", $value: "#405060"}
            text: {$type: "color", $value: "#ffeedd"}
        field:
          focus:
            border-color: {$type: "color", $value: "#00ff00"}
            border-width: {$type: "dimension", $value: {value: 2, unit: "px"}}
        button:
          disabled:
            fill: {$type: "color", $value: "#333333"}
        "##;

    fn pass(ctx: &egui::Context, add: impl FnMut(&mut egui::Ui)) {
        let mut out = ctx.run_ui(egui::RawInput::default(), add);
        out.textures_delta.clear();
    }

    #[test]
    fn egui_s_global_widgets_are_the_button_s_scope() {
        let t = Tokens::default();
        let kit = Kit::compile(&t, &Shapes::default());
        let v = theme::visuals_with(&t, &kit);
        let s = kit.scoped.get(Stock::Button, Tone::Neutral);
        assert_eq!(v.widgets.inactive.weak_bg_fill, s.widgets[0].weak_bg_fill);
        assert_eq!(v.widgets.hovered.bg_stroke, s.widgets[1].bg_stroke);
        assert_eq!(v.widgets.open.fg_stroke, s.widgets[3].fg_stroke);
        // A combo box and a toggle button look like egui's by default.
        let combo = kit.scoped.get(Stock::ComboBox, Tone::Neutral);
        assert_eq!(combo.widgets, s.widgets);
        let toggle = kit.scoped.get(Stock::ToggleButton, Tone::Neutral);
        assert_eq!(toggle.selection_fill, Some(v.selection.bg_fill));
        assert_eq!(toggle.selection_stroke, Some(v.selection.stroke));
        let field = kit.scoped.get(Stock::Field, Tone::Neutral);
        assert_eq!(field.selection_stroke, Some(v.selection.stroke));
        assert_eq!(field.text_edit_bg, v.text_edit_bg_color);
        // `button` gives tones, and `combo-box` inherits them.
        let danger = kit.scoped.get(Stock::Button, Tone::Danger);
        assert_ne!(danger, s);
        assert_eq!(
            kit.scoped.get(Stock::ComboBox, Tone::Danger).widgets,
            danger.widgets
        );
    }

    #[test]
    fn a_scope_draws_egui_s_widgets_from_the_kit_and_stays_in_its_scope() {
        let ctx = egui::Context::default();
        theme::install_tokens(&ctx, tokens_with(RESTYLED));
        let kit = Kit::of(&ctx);
        assert!(kit.problems.is_empty(), "{:?}", kit.problems);
        pass(&ctx, |ui| {
            let before = ui.visuals().clone();
            kit.scope(ui, Stock::ComboBox, Tone::Neutral, |ui| {
                let w = &ui.visuals().widgets;
                assert_eq!(w.inactive.weak_bg_fill, Color32::from_rgb(0x10, 0x20, 0x30));
                assert_eq!(w.inactive.corner_radius, CornerRadius::same(9));
                assert_eq!(w.open.weak_bg_fill, Color32::from_rgb(0x20, 0x30, 0x40));
                // egui's expansion is kept.
                assert_eq!(w.hovered.expansion, before.widgets.hovered.expansion);
            });
            kit.scope(ui, Stock::ToggleButton, Tone::Neutral, |ui| {
                let s = ui.visuals().selection;
                assert_eq!(s.bg_fill, Color32::from_rgb(0x40, 0x50, 0x60));
                assert_eq!(s.stroke.color, Color32::from_rgb(0xff, 0xee, 0xdd));
            });
            kit.scope(ui, Stock::Field, Tone::Danger, |ui| {
                let v = ui.visuals();
                assert_eq!(v.selection.stroke.color, Color32::from_rgb(0, 0xff, 0));
                assert_eq!(v.selection.stroke.width, 2.0);
                let danger = kit.field.style_toned(Tone::Danger, State::REST);
                assert_eq!(v.widgets.inactive.bg_stroke.color, danger.border_color);
            });
            ui.add_enabled_ui(false, |ui| {
                kit.scope(ui, Stock::Button, Tone::Neutral, |ui| {
                    let grey = Color32::from_rgb(0x33, 0x33, 0x33);
                    let w = &ui.visuals().widgets;
                    assert_eq!(w.inactive.bg_fill, grey);
                    assert_eq!(w.hovered.bg_fill, grey);
                });
            });
            assert_eq!(*ui.visuals(), before);
            let r = kit.scope(ui, Stock::Button, Tone::Accent, |ui| ui.button("Go"));
            assert!(r.inner.rect.width() > 0.0);
        });
    }

    #[test]
    fn what_egui_cannot_draw_of_a_scoped_component_is_a_problem() {
        let t = tokens_with(
            r##"
            $extends: "pop-dark"
            combo-box:
              shape: {$type: "string", $value: "box"}
              hover:
                ring-color: {$type: "color", $value: "#ff0000"}
                ring-width: {$type: "dimension", $value: {value: 1, unit: "px"}}
            toggle-button:
              selected:
                border-color: {$type: "color", $value: "#ff0000"}
            field:
              focus:
                fill: {$type: "color", $value: "#ff0000"}
            "##,
        );
        let kit = Kit::compile(&t, &Shapes::default());
        let p = kit.problems.join("\n");
        assert!(p.contains("combo-box.shape"), "{p}");
        assert!(
            p.contains("combo-box.hover: egui draws no focus ring"),
            "{p}"
        );
        assert!(p.contains("toggle-button.selected"), "{p}");
        assert!(p.contains("field.focus.fill"), "{p}");
    }
}
