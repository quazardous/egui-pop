//! Themes for egui: resolved design tokens turned into egui's [`Visuals`]
//! and [`Style`], installed into a [`Context`], and kept there as a typed
//! [`Tokens`] set for custom widgets.
//!
//! ```
//! use egui_pop_core::theme::{self, Density, ThemeSpec, Tokens};
//!
//! let ctx = egui::Context::default();
//! let tokens = theme::install(&ctx, &ThemeSpec::new("pop-light").density(Density::Compact));
//! assert!(tokens.resolved().problems().is_empty());
//! assert_eq!(ctx.global_style().visuals.panel_fill, tokens.surface.panel);
//! // Any widget reads the same set back from the context.
//! assert_eq!(Tokens::of(&ctx).accent.default, tokens.accent.default);
//! ```
//!
//! # Where tokens go
//!
//! egui's own widgets (and other crates' widgets) are drawn from the
//! generated style: `surface.*` become panel, window and field fills,
//! `button.*` the widget states (rest, hovered, pressed), `text.*` the
//! widget strokes, `border.*` outlines and separators, `focus.ring` the
//! stroke of a focused or pressed widget, `selection` the fill behind
//! selected text and items, `space.*` and `size.*` the spacing,
//! `font.size.*` the text styles and `motion.duration` the animation time.
//!
//! egui uses one colour (`selection.stroke`) for both the text of a
//! selected label and the frame of a focused text field: egui-pop gives it
//! `text.strong`, which stays legible on the selection fill. egui's slider
//! paints its rail with the resting widget fill and its optional trailing
//! fill with the selection fill; the `slider.*` tokens are for egui-pop's
//! own slider.
//!
//! Font families (`font.family.*`) become egui's fonts at several weights
//! (`fonts`): faces the application registers, the system's (feature
//! `system-fonts`), then egui's bundled fonts.
//!
//! # Never per widget
//!
//! A theme is installed for the whole context, between frames. Never
//! change the global style in the middle of a frame to theme one widget:
//! read [`Tokens::of`] and paint with its values, or scope a style change
//! to a child `Ui` (`ui.scope(|ui| { ui.style_mut()... })`), which does
//! not leak to the rest of the frame.

use std::sync::Arc;

use egui::{
    Color32, Context, CornerRadius, Id, Margin, Stroke, Style, TextStyle, ThemePreference, Visuals,
    style::ScrollAnimation,
};

use crate::kit::{Kit, Shapes, State};
pub use egui_pop_tokens::{
    BUILT_IN_ACCENTS, BUILT_IN_THEMES, Density, FALLBACK_THEME, POP_THEMES, ResolvedTokens,
    ThemeSpec, Themes, theme_label,
};

/// A token colour as an egui colour.
pub fn color32(c: egui_pop_tokens::Color) -> Color32 {
    let [r, g, b, a] = c.to_srgba_u8();
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// The surfaces, from the deepest to the topmost.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Surface {
    pub canvas: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub active: Color32,
    pub overlay: Color32,
    pub sunken: Color32,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Text {
    pub default: Color32,
    pub muted: Color32,
    pub strong: Color32,
    pub on_accent: Color32,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Border {
    pub default: Color32,
    pub strong: Color32,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Accent {
    pub default: Color32,
    pub hover: Color32,
    pub active: Color32,
    pub subtle: Color32,
}

/// Status colours, legible as text on panels and overlays.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Status {
    pub danger: Color32,
    pub warning: Color32,
    pub success: Color32,
    pub info: Color32,
}

/// Corner radii in points.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Radius {
    pub widget: f32,
    pub window: f32,
}

/// Spacing steps in points.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Space {
    pub xs: f32,
    pub s: f32,
    pub m: f32,
    pub l: f32,
    pub xl: f32,
}

/// Sizes in points.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Size {
    pub control_height: f32,
    pub icon_inline: f32,
    pub icon_toolbar: f32,
    pub icon_panel: f32,
    pub dialog_width: f32,
    pub toast_width: f32,
}

/// Font sizes in points and families, the first preferred.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Font {
    pub small: f32,
    pub body: f32,
    pub heading: f32,
    pub mono: f32,
    pub family_ui: Vec<String>,
    pub family_mono: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ButtonTokens {
    pub background: Color32,
    pub background_hover: Color32,
    pub background_active: Color32,
    pub text: Color32,
    pub border: Color32,
    pub radius: f32,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SliderTokens {
    pub track: Color32,
    pub fill: Color32,
    pub thumb: Color32,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MenuItemTokens {
    pub background_hover: Color32,
    pub text: Color32,
    pub text_muted: Color32,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DialogTokens {
    pub background: Color32,
    pub border: Color32,
    pub radius: f32,
    pub padding: f32,
    pub width: f32,
    /// A movable dialog's header band, and its title.
    pub header: Color32,
    pub header_text: Color32,
    /// How opaque the backdrop behind a modal dialog is (0 to 1).
    pub backdrop: f32,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ToastTokens {
    pub background: Color32,
    pub border: Color32,
    pub text: Color32,
    pub padding: f32,
    pub width: f32,
}

/// Palettes and toolboxes floating over the interface.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PaletteTokens {
    pub background: Color32,
    pub border: Color32,
    pub header: Color32,
    pub header_text: Color32,
    pub padding: f32,
    /// How close a dragged palette snaps to an edge.
    pub magnet: f32,
}

/// Times of motion, in seconds (see `motion`).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MotionTokens {
    /// Nothing shows before work ran this long (`motion.spinner.delay`).
    pub spinner_delay: f32,
    /// A busy indicator, once shown, stays this long
    /// (`motion.spinner.min`).
    pub spinner_min: f32,
    /// One turn of a spinner (`motion.spinner.turn`).
    pub spinner_turn: f32,
    /// An indeterminate bar's segment crossing the track
    /// (`motion.progress.travel`).
    pub progress_travel: f32,
    /// A skeleton's shimmer crossing it (`motion.skeleton.shimmer`).
    pub skeleton_shimmer: f32,
}

/// A window's own title bar and frame.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TitlebarTokens {
    pub background: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub button_hover: Color32,
    pub close_hover: Color32,
    pub close_text: Color32,
    pub height: f32,
    pub button_width: f32,
    /// The resize edges' width.
    pub grip: f32,
}

/// The system and component tokens of a resolved theme, typed for egui.
/// Custom widgets read it with [`Tokens::of`] instead of hard-coding a
/// colour or a size.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Tokens {
    pub surface: Surface,
    pub text: Text,
    pub border: Border,
    pub accent: Accent,
    pub status: Status,
    pub focus_ring: Color32,
    pub selection: Color32,
    pub radius: Radius,
    pub space: Space,
    pub size: Size,
    pub font: Font,
    /// `motion.duration` in seconds; zero when motion is reduced.
    pub motion_duration: f32,
    /// The other times of motion.
    pub motion: MotionTokens,
    pub button: ButtonTokens,
    pub slider: SliderTokens,
    pub menu_item: MenuItemTokens,
    pub dialog: DialogTokens,
    pub toast: ToastTokens,
    pub titlebar: TitlebarTokens,
    pub palette: PaletteTokens,
    resolved: Arc<ResolvedTokens>,
}

impl Default for Tokens {
    /// The built-in `pop-dark` theme.
    fn default() -> Self {
        Tokens::new(Themes::default().resolve(&ThemeSpec::default()))
    }
}

impl Tokens {
    /// Types a resolved theme. Resolution guarantees every contract token,
    /// so nothing here can be missing.
    pub fn new(resolved: ResolvedTokens) -> Tokens {
        let r = &resolved;
        let c = |path: &str| color32(r.color(path).unwrap_or(egui_pop_tokens::Color::MISSING));
        let d = |path: &str| r.dimension(path).unwrap_or(0.0);
        let f = |path: &str| {
            r.font_family(path)
                .map(<[String]>::to_vec)
                .unwrap_or_default()
        };
        Tokens {
            surface: Surface {
                canvas: c("surface.canvas"),
                panel: c("surface.panel"),
                raised: c("surface.raised"),
                hover: c("surface.hover"),
                active: c("surface.active"),
                overlay: c("surface.overlay"),
                sunken: c("surface.sunken"),
            },
            text: Text {
                default: c("text.default"),
                muted: c("text.muted"),
                strong: c("text.strong"),
                on_accent: c("text.on-accent"),
            },
            border: Border {
                default: c("border.default"),
                strong: c("border.strong"),
            },
            accent: Accent {
                default: c("accent.default"),
                hover: c("accent.hover"),
                active: c("accent.active"),
                subtle: c("accent.subtle"),
            },
            status: Status {
                danger: c("danger"),
                warning: c("warning"),
                success: c("success"),
                info: c("info"),
            },
            focus_ring: c("focus.ring"),
            selection: c("selection"),
            radius: Radius {
                widget: d("radius.widget"),
                window: d("radius.window"),
            },
            space: Space {
                xs: d("space.xs"),
                s: d("space.s"),
                m: d("space.m"),
                l: d("space.l"),
                xl: d("space.xl"),
            },
            size: Size {
                control_height: d("size.control.height"),
                icon_inline: d("size.icon.inline"),
                icon_toolbar: d("size.icon.toolbar"),
                icon_panel: d("size.icon.panel"),
                dialog_width: d("size.dialog.width"),
                toast_width: d("size.toast.width"),
            },
            font: Font {
                small: d("font.size.small"),
                body: d("font.size.body"),
                heading: d("font.size.heading"),
                mono: d("font.size.mono"),
                family_ui: f("font.family.ui"),
                family_mono: f("font.family.mono"),
            },
            motion_duration: r.duration_ms("motion.duration").unwrap_or(0.0).max(0.0) / 1000.0,
            motion: {
                let s = |path: &str| r.duration_ms(path).unwrap_or(0.0).max(0.0) / 1000.0;
                MotionTokens {
                    spinner_delay: s("motion.spinner.delay"),
                    spinner_min: s("motion.spinner.min"),
                    spinner_turn: s("motion.spinner.turn"),
                    progress_travel: s("motion.progress.travel"),
                    skeleton_shimmer: s("motion.skeleton.shimmer"),
                }
            },
            button: ButtonTokens {
                background: c("button.rest.fill"),
                background_hover: c("button.hover.fill"),
                background_active: c("button.active.fill"),
                text: c("button.rest.text"),
                border: c("button.rest.border-color"),
                radius: d("button.rest.radius"),
            },
            slider: SliderTokens {
                track: c("slider.track"),
                fill: c("slider.fill"),
                thumb: c("slider.thumb"),
            },
            menu_item: MenuItemTokens {
                background_hover: c("menu-item.hover.fill"),
                text: c("menu-item.rest.text"),
                text_muted: c("menu-item.tone.muted.text"),
            },
            dialog: DialogTokens {
                background: c("dialog.rest.fill"),
                border: c("dialog.rest.border-color"),
                radius: d("dialog.rest.radius"),
                padding: d("dialog.padding-x"),
                width: d("dialog.width"),
                header: c("header-band.rest.fill"),
                header_text: c("dialog-header.rest.text"),
                backdrop: d("backdrop.opacity").clamp(0.0, 1.0),
            },
            toast: ToastTokens {
                background: c("toast.rest.fill"),
                border: c("toast.rest.border-color"),
                text: c("toast.rest.text"),
                padding: d("toast.padding-x"),
                width: d("toast.width"),
            },
            palette: PaletteTokens {
                background: c("palette-window.rest.fill"),
                border: c("palette-window.rest.border-color"),
                header: c("header-band.rest.fill"),
                header_text: c("header-band.rest.text"),
                padding: d("palette-window.padding-x"),
                magnet: d("palette.magnet"),
            },
            titlebar: TitlebarTokens {
                background: c("title-bar.rest.fill"),
                text: c("title-bar.rest.text"),
                text_muted: c("window-button.rest.text"),
                button_hover: c("window-button.hover.fill"),
                close_hover: c("window-close.hover.fill"),
                close_text: c("window-close.hover.text"),
                height: d("title-bar.height"),
                button_width: d("window-button.width"),
                grip: d("window.grip"),
            },
            resolved: Arc::new(resolved),
        }
    }

    /// The full resolved set: every tier, descriptions, aliases, problems.
    pub fn resolved(&self) -> &ResolvedTokens {
        &self.resolved
    }

    /// Any colour token by path, the reference tier included.
    pub fn color(&self, path: &str) -> Option<Color32> {
        self.resolved.color(path).map(color32)
    }

    /// Whether the theme is a dark one (its panels are dark).
    pub fn dark_mode(&self) -> bool {
        self.resolved
            .color("surface.panel")
            .is_some_and(egui_pop_tokens::Color::is_dark)
    }

    /// The tokens installed in a context, or the built-in `pop-dark` ones
    /// (installed on first use) when no theme was.
    pub fn of(ctx: &Context) -> Arc<Tokens> {
        if let Some(t) = ctx.data(|d| d.get_temp::<Arc<Tokens>>(tokens_id())) {
            return t;
        }
        let t = Arc::new(Tokens::default());
        ctx.data_mut(|d| d.insert_temp(tokens_id(), t.clone()));
        t
    }
}

fn tokens_id() -> Id {
    Id::new("egui_pop::theme::Tokens")
}

fn radius(points: f32) -> CornerRadius {
    CornerRadius::same(points.round().clamp(0.0, 255.0) as u8)
}

fn margin(points: f32) -> Margin {
    Margin::same(points.round().clamp(0.0, 127.0) as i8)
}

/// egui's visuals for a theme.
pub fn visuals(t: &Tokens) -> Visuals {
    visuals_with(t, &Kit::compile(t, &Shapes::default()))
}

/// egui's visuals for a theme and its compiled kit: egui's widget states
/// come from the kit's `button` (rest, hover, active, and selected for
/// an open menu), text fields' background from its `field`. What egui
/// cannot draw is in the kit's problems (`Kit::compile`).
pub fn visuals_with(t: &Tokens, kit: &Kit) -> Visuals {
    let mut v = if t.dark_mode() {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = t.surface.panel;
    v.window_fill = t.surface.overlay;
    v.extreme_bg_color = t.surface.sunken;
    v.text_edit_bg_color = Some(kit.field.style(State::REST).fill);
    v.faint_bg_color = t.surface.raised;
    v.code_bg_color = t.surface.raised;
    v.window_stroke = Stroke::new(1.0, t.border.default);
    v.window_corner_radius = radius(t.radius.window);
    v.menu_corner_radius = radius(t.radius.window);
    v.hyperlink_color = t.accent.default;
    v.warn_fg_color = t.status.warning;
    v.error_fg_color = t.status.danger;
    v.weak_text_color = Some(t.text.muted);
    v.selection.bg_fill = t.selection;
    v.selection.stroke = Stroke::new(1.0, t.text.strong);
    v.text_cursor.stroke = Stroke::new(2.0, t.text.strong);
    v.text_cursor.blink = t.motion_duration > 0.0;

    let w = &mut v.widgets;
    let rest = kit.button.style(State::REST);
    w.noninteractive.bg_fill = t.surface.panel;
    w.noninteractive.weak_bg_fill = t.surface.panel;
    w.noninteractive.bg_stroke = Stroke::new(rest.border_width, t.border.default);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text.default);
    w.noninteractive.corner_radius = radius(rest.radius);
    for (widget, state) in [
        (&mut w.inactive, State::REST),
        (&mut w.hovered, State::HOVER),
        (&mut w.active, State::ACTIVE),
        (&mut w.open, State::SELECTED),
    ] {
        let b = kit.button.style(state);
        widget.bg_fill = b.fill;
        widget.weak_bg_fill = b.fill;
        widget.bg_stroke = Stroke::new(b.border_width, b.border_color);
        widget.fg_stroke = Stroke::new(1.0, b.text);
        widget.corner_radius = radius(b.radius);
    }
    v
}

/// egui's style for a theme, built over `base` (whose settings the theme
/// does not decide are kept).
pub fn style(t: &Tokens, base: &Style) -> Style {
    style_with(t, &Kit::compile(t, &Shapes::default()), base)
}

/// [`style`] with a compiled kit (the one installed with the theme).
pub fn style_with(t: &Tokens, kit: &Kit, base: &Style) -> Style {
    let mut style = base.clone();
    style.visuals = visuals_with(t, kit);
    for (text_style, size) in [
        (TextStyle::Small, t.font.small),
        (TextStyle::Body, t.font.body),
        (TextStyle::Button, t.font.body),
        (TextStyle::Heading, t.font.heading),
        (TextStyle::Monospace, t.font.mono),
    ] {
        if let Some(font) = style.text_styles.get_mut(&text_style) {
            font.size = size.max(4.0);
        }
    }
    let s = &t.space;
    let sp = &mut style.spacing;
    sp.item_spacing = egui::vec2(s.m, s.s);
    sp.button_padding = egui::vec2(s.s + s.xs, s.xs);
    sp.window_margin = margin(s.l);
    sp.menu_margin = margin(s.m);
    sp.interact_size.y = t.size.control_height;
    sp.icon_width = t.size.icon_inline;
    sp.icon_width_inner = (t.size.icon_inline * 0.6).round();
    // Below 4px a checkbox or radio reads as glued to its label, which the
    // compact density's 2px small space would do.
    sp.icon_spacing = s.s.max(4.0);
    sp.indent = t.size.icon_toolbar;
    style.animation_time = t.motion_duration;
    if t.motion_duration == 0.0 {
        style.scroll_animation = ScrollAnimation::none();
    }
    style
}

/// Snaps a style's stroke widths to whole physical pixels at `ppp`
/// (`scale::snap`), so 1 point borders stay one sharp pixel at 1.25 or
/// 1.5. `install_tokens` does it for the scale of the window it runs in;
/// egui-pop's application installs the theme again when that scale
/// changes.
pub fn snap_strokes(style: &mut Style, ppp: f32) {
    let snap = |w: f32| crate::scale::snap(w, ppp);
    let v = &mut style.visuals;
    v.window_stroke.width = snap(v.window_stroke.width);
    v.selection.stroke.width = snap(v.selection.stroke.width);
    v.text_cursor.stroke.width = snap(v.text_cursor.stroke.width);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.bg_stroke.width = snap(w.bg_stroke.width);
        w.fg_stroke.width = snap(w.fg_stroke.width);
    }
}

/// Installs typed tokens into a context: egui's dark and light styles both
/// become the theme's (so the system's light or dark preference does not
/// swap it out), and [`Tokens::of`] returns them. Call it between frames
/// (at start-up, or when the user picks another theme), not while drawing.
pub fn install_tokens(ctx: &Context, tokens: Tokens) -> Arc<Tokens> {
    let tokens = Arc::new(tokens);
    ctx.set_theme(if tokens.dark_mode() {
        ThemePreference::Dark
    } else {
        ThemePreference::Light
    });
    // The kit first: egui's style is drawn from it.
    let kit = crate::kit::install(ctx, &tokens);
    let ppp = ctx.pixels_per_point();
    ctx.all_styles_mut(|s| {
        *s = style_with(&tokens, &kit, s);
        snap_strokes(s, ppp);
    });
    ctx.data_mut(|d| d.insert_temp(tokens_id(), tokens.clone()));
    crate::fonts::install(ctx, &tokens.font.family_ui, &tokens.font.family_mono);
    crate::icon_set::install(ctx, &tokens);
    tokens
}

/// Resolves a spec against the given themes and installs it. Problems
/// (a broken user theme, a typo in an override) are in
/// `tokens.resolved().problems()`; the theme is installed anyway, with
/// fallbacks.
pub fn install_with(ctx: &Context, themes: &Themes, spec: &ThemeSpec) -> Arc<Tokens> {
    install_tokens(ctx, Tokens::new(themes.resolve(spec)))
}

/// Resolves a spec against the built-in themes and installs it.
pub fn install(ctx: &Context, spec: &ThemeSpec) -> Arc<Tokens> {
    install_with(ctx, &Themes::default(), spec)
}
