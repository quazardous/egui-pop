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
//! `font.text-style.*` (steps of the `font.size.*` scale) egui's text
//! styles, `shadow.window` and `shadow.popup` egui's window and popup
//! shadows, and `motion.duration` the animation time.
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

use std::sync::{Arc, Mutex};

use egui::{
    Color32, Context, CornerRadius, Margin, Shadow, Stroke, Style, TextStyle, Theme,
    ThemePreference, Visuals, style::ScrollAnimation,
};

use crate::kit::{Kit, Shapes, State};
use crate::state::Slot;
pub use egui_pop_tokens::{
    BUILT_IN_ACCENTS, BUILT_IN_THEMES, Counterpart, Density, FALLBACK_THEME, POP_THEMES,
    ResolvedTokens, ThemeSpec, Themes, theme_label,
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

/// Spacing in points: the scale's steps (`space.xs` to `space.xl`), and
/// spacing by intent (`space.inline`, `group`, `section`, `page`), each
/// an alias of a step, so densities scale it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Space {
    pub xs: f32,
    pub s: f32,
    pub m: f32,
    pub l: f32,
    pub xl: f32,
    /// Between items on one line.
    pub inline: f32,
    /// Between related groups of items.
    pub group: f32,
    /// Before a section of a panel or a page.
    pub section: f32,
    /// Around a page's content and between its main blocks.
    pub page: f32,
}

/// A text role: what a piece of text is (`text-role.*`), each a size,
/// a colour and a weight over the size scale and the text colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextRole {
    /// A page's or a dialog's title.
    Title,
    /// A section's title in a panel or a page.
    Section,
    /// Ordinary text.
    Body,
    /// Secondary text: hints, descriptions, an empty list's message.
    Muted,
    /// Small print about an item: a date, a size, a count.
    Meta,
}

impl TextRole {
    pub const ALL: [TextRole; 5] = [
        TextRole::Title,
        TextRole::Section,
        TextRole::Body,
        TextRole::Muted,
        TextRole::Meta,
    ];

    /// Its name in the tokens (`title`: `text-role.title.*`).
    pub fn name(self) -> &'static str {
        match self {
            TextRole::Title => "title",
            TextRole::Section => "section",
            TextRole::Body => "body",
            TextRole::Muted => "muted",
            TextRole::Meta => "meta",
        }
    }
}

/// How a text role is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct TextRoleStyle {
    /// In points (a step of `font.size.*`).
    pub size: f32,
    pub color: Color32,
    /// Regular unless the role says otherwise (`fonts::family` draws it).
    pub weight: crate::fonts::Weight,
}

/// Every text role's style (`text-role.*`).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TextRoles {
    pub title: TextRoleStyle,
    pub section: TextRoleStyle,
    pub body: TextRoleStyle,
    pub muted: TextRoleStyle,
    pub meta: TextRoleStyle,
}

impl TextRoles {
    /// A role's style.
    pub fn get(&self, role: TextRole) -> &TextRoleStyle {
        match role {
            TextRole::Title => &self.title,
            TextRole::Section => &self.section,
            TextRole::Body => &self.body,
            TextRole::Muted => &self.muted,
            TextRole::Meta => &self.meta,
        }
    }
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

/// Font sizes in points and families, the first preferred. `small`,
/// `body`, `button`, `heading` and `mono` are egui's text styles
/// (`font.text-style.*`), each a step of the `scale` (`font.size.*`).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Font {
    pub small: f32,
    pub body: f32,
    pub button: f32,
    pub heading: f32,
    pub mono: f32,
    /// The size scale, `xs` to `xl` (`fonts::Step`).
    pub scale: FontScale,
    pub family_ui: Vec<String>,
    pub family_mono: Vec<String>,
    /// How values line their digits up (`font.numeric`).
    pub numeric: crate::fonts::Numeric,
}

/// The size scale's five steps, in points (`font.size.xs` to `xl`).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct FontScale {
    pub xs: f32,
    pub s: f32,
    pub m: f32,
    pub l: f32,
    pub xl: f32,
}

impl FontScale {
    /// A step's size in points.
    pub fn get(&self, step: crate::fonts::Step) -> f32 {
        use crate::fonts::Step;
        match step {
            Step::Xs => self.xs,
            Step::S => self.s,
            Step::M => self.m,
            Step::L => self.l,
            Step::Xl => self.xl,
        }
    }
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
    /// One pulse of a busy LED or a mark calling attention
    /// (`motion.pulse.period`).
    pub pulse_period: f32,
    /// How many times a mark calling attention pulses
    /// (`motion.pulse.count`).
    pub pulse_count: u32,
}

/// Shadows under what lies over the interface (`shadow.*`), as egui's.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ShadowTokens {
    /// Menus, popovers, tooltips (egui's `popup_shadow`).
    pub popup: Shadow,
    /// Dialogs and palettes (egui's `window_shadow`).
    pub window: Shadow,
    /// A side area lying over the centre, its `offset` towards the
    /// centre as for the start side.
    pub side: Shadow,
}

/// The shadow a group of tokens gives (`<prefix>offset-x`, `offset-y`,
/// `blur`, `spread` and `color`; `shadow.window.` or a component's
/// `side-area.shadow-`), as egui's [`Shadow`], rounded to its whole
/// points; `None` when the colour is missing. egui's spread cannot be
/// negative: below 0 it is 0.
pub fn shadow(r: &ResolvedTokens, prefix: &str) -> Option<Shadow> {
    let color = r.color(&format!("{prefix}color"))?;
    let d = |t: &str| r.dimension(&format!("{prefix}{t}")).unwrap_or(0.0);
    let offset = |v: f32| v.round().clamp(-128.0, 127.0) as i8;
    let size = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    Some(Shadow {
        offset: [offset(d("offset-x")), offset(d("offset-y"))],
        blur: size(d("blur")),
        spread: size(d("spread")),
        color: color32(color),
    })
}

/// The system tokens of a resolved theme, typed for egui. Custom widgets
/// read it with [`Tokens::of`] instead of hard-coding a colour or a size;
/// a component's look is in its kit (`Kit::of(ctx).<component>`), and any
/// other token in [`Tokens::resolved`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Tokens {
    pub surface: Surface,
    pub text: Text,
    /// Text by role (`text-role.*`).
    pub text_role: TextRoles,
    pub border: Border,
    pub accent: Accent,
    pub status: Status,
    pub focus_ring: Color32,
    pub selection: Color32,
    pub radius: Radius,
    pub space: Space,
    pub size: Size,
    pub font: Font,
    pub shadow: ShadowTokens,
    /// `motion.duration` in seconds; zero when motion is reduced.
    pub motion_duration: f32,
    /// The other times of motion.
    pub motion: MotionTokens,
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
            text_role: {
                let role = |name: &str| TextRoleStyle {
                    size: d(&format!("text-role.{name}.size")),
                    color: c(&format!("text-role.{name}.color")),
                    weight: crate::fonts::Weight(
                        r.dimension(&format!("text-role.{name}.weight"))
                            .unwrap_or(400.0)
                            .clamp(1.0, 1000.0) as u16,
                    ),
                };
                TextRoles {
                    title: role("title"),
                    section: role("section"),
                    body: role("body"),
                    muted: role("muted"),
                    meta: role("meta"),
                }
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
                inline: d("space.inline"),
                group: d("space.group"),
                section: d("space.section"),
                page: d("space.page"),
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
                small: d("font.text-style.small"),
                body: d("font.text-style.body"),
                button: d("font.text-style.button"),
                heading: d("font.text-style.heading"),
                mono: d("font.text-style.monospace"),
                scale: FontScale {
                    xs: d("font.size.xs"),
                    s: d("font.size.s"),
                    m: d("font.size.m"),
                    l: d("font.size.l"),
                    xl: d("font.size.xl"),
                },
                family_ui: f("font.family.ui"),
                family_mono: f("font.family.mono"),
                numeric: match r.value("font.numeric") {
                    Some(egui_pop_tokens::Value::Other(serde_json::Value::String(s))) => {
                        crate::fonts::Numeric::parse(s).unwrap_or_default()
                    }
                    _ => crate::fonts::Numeric::default(),
                },
            },
            shadow: {
                let sh = |g: &str| shadow(r, &format!("shadow.{g}.")).unwrap_or(Shadow::NONE);
                ShadowTokens {
                    popup: sh("popup"),
                    window: sh("window"),
                    side: sh("side"),
                }
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
                    pulse_period: s("motion.pulse.period"),
                    pulse_count: r
                        .dimension("motion.pulse.count")
                        .unwrap_or(3.0)
                        .clamp(0.0, 100.0) as u32,
                }
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

    /// The tokens of the theme drawn now: of egui's theme of the moment
    /// (`ctx.theme()`, the user's or the system's light or dark mode)
    /// when a pair is installed, else of the theme installed; the
    /// built-in `pop-dark` ones (installed on first use) when no theme
    /// was. Read it while drawing, never keep it across frames: a switch
    /// between light and dark changes it without any install.
    ///
    /// Inside a [`ScopedTheme`]'s `show`, they are the scoped theme's.
    pub fn of(ctx: &Context) -> Arc<Tokens> {
        if let Some(scoped) = ScopedTheme::current(ctx) {
            return scoped.tokens;
        }
        Tokens::of_theme(ctx, ctx.theme())
    }

    /// The tokens installed for one of egui's themes (the same for both
    /// when the theme installed is alone).
    pub fn of_theme(ctx: &Context, theme: Theme) -> Arc<Tokens> {
        variants(ctx).get(theme).clone()
    }

    /// The egui theme this set is drawn under.
    pub fn egui_theme(&self) -> Theme {
        if self.dark_mode() {
            Theme::Dark
        } else {
            Theme::Light
        }
    }
}

/// The tokens installed, one set per egui theme: a theme and its
/// counterpart, or the same theme twice when it is alone.
#[derive(Clone, Debug)]
pub struct Variants {
    pub dark: Arc<Tokens>,
    pub light: Arc<Tokens>,
}

impl Variants {
    /// One theme for both of egui's themes.
    pub fn alone(tokens: Arc<Tokens>) -> Variants {
        Variants {
            dark: tokens.clone(),
            light: tokens,
        }
    }

    /// The set for one of egui's themes.
    pub fn get(&self, theme: Theme) -> &Arc<Tokens> {
        match theme {
            Theme::Dark => &self.dark,
            Theme::Light => &self.light,
        }
    }

    /// Whether a light and a dark theme are installed together: egui's
    /// light or dark mode then chooses between them.
    pub fn paired(&self) -> bool {
        !Arc::ptr_eq(&self.dark, &self.light)
    }
}

/// The tokens installed in a context, per egui theme; the built-in
/// `pop-dark` ones, alone (installed on first use), when no theme was.
pub fn variants(ctx: &Context) -> Variants {
    TOKENS.get_or_insert_with(ctx, || Variants::alone(Arc::new(Tokens::default())))
}

/// Whether the theme installed has a counterpart: a light and a dark
/// variant installed together ([`install_with`]).
pub fn is_paired(ctx: &Context) -> bool {
    variants(ctx).paired()
}

/// Asks for egui's light or dark mode, or the system's
/// (`ThemePreference::System`): with a pair installed, it switches
/// between the two at once, nothing installed again. A theme alone keeps
/// its own mode whatever is asked.
pub fn set_preference(ctx: &Context, preference: ThemePreference) {
    let v = variants(ctx);
    if v.paired() {
        ctx.set_theme(preference);
    } else {
        ctx.set_theme(v.dark.egui_theme());
    }
}

/// The tokens installed.
const TOKENS: Slot<Variants> = Slot::opaque("core.theme.tokens");

/// The scoped theme being drawn, if any.
const SCOPED: Slot<ScopedTheme> = Slot::opaque("core.theme.scoped");

/// A theme drawn in one part of a window without being installed: a
/// theme's tokens, their kit and egui's style for them, applied to a
/// child `Ui` ([`ScopedTheme::show`]). While it draws, [`Tokens::of`]
/// and `Kit::of` give its tokens and kit, so egui-pop's widgets and
/// custom painting follow it as egui's do; the rest of the window keeps
/// the installed theme. A theme editor's preview, a themed island, a
/// theme picker's thumbnails.
///
/// Popups follow it too. egui builds the `Ui` of a popup, a tooltip or
/// a window from the context's style, not from the `Ui` that opened it,
/// so while the scope draws, egui's context style is the scoped theme's
/// as well (put back when it ends, even after a panic): a combo box's
/// list, a tooltip, a context menu or a menu opened inside it, egui's
/// or egui-pop's, is drawn in the scoped theme. A popup drawn later, out
/// of the scope (egui-pop's `menus::popup_at`), records the scope it was
/// opened in and draws within it ([`ScopedTheme::within`]).
///
/// What stays the installed theme's: fonts (egui's font definitions are
/// global, rebuilt with the atlas), egui's light or dark mode
/// (`ctx.theme()`), and what is drawn outside the scope. A change to the
/// context's style made while the scope draws is undone when it ends:
/// install themes between frames, as [`install_tokens`] says. Build it
/// once per theme and keep it (compiling the kit has a cost), never per
/// frame.
///
/// ```
/// use egui_pop_core::theme::{self, ScopedTheme, ThemeSpec, Themes, Tokens};
///
/// let ctx = egui::Context::default();
/// theme::install(&ctx, &ThemeSpec::new("pop-dark"));
/// let light = ScopedTheme::new(&ctx, Tokens::new(Themes::default().resolve(&ThemeSpec::new("pop-light"))));
/// ctx.run_ui(Default::default(), |ui| {
///     light.show(ui, |ui| {
///         assert!(!Tokens::of(ui.ctx()).dark_mode());
///         assert!(!ui.visuals().dark_mode);
///         // What a popup opened here is built from.
///         assert!(!ui.ctx().global_style().visuals.dark_mode);
///     });
///     assert!(Tokens::of(ui.ctx()).dark_mode());
///     assert!(ui.ctx().global_style().visuals.dark_mode);
/// })
/// .textures_delta
/// .clear();
/// ```
#[derive(Clone)]
pub struct ScopedTheme {
    tokens: Arc<Tokens>,
    kit: Arc<Kit>,
    /// The context style made last: the style it was made from, the
    /// scale, and the style. Shared by the clones.
    context_style: Arc<Mutex<Option<ContextStyle>>>,
}

/// A scoped theme's style for the context, and what it was made from.
struct ContextStyle {
    base: Arc<Style>,
    ppp: f32,
    style: Arc<Style>,
}

impl std::fmt::Debug for ScopedTheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedTheme")
            .field("theme", &self.tokens.resolved().spec().label())
            .finish_non_exhaustive()
    }
}

impl ScopedTheme {
    /// A theme to draw scoped, its kit compiled with the shapes
    /// registered in `ctx` (kits' handlers).
    pub fn new(ctx: &Context, tokens: Tokens) -> ScopedTheme {
        let tokens = Arc::new(tokens);
        let kit = crate::kit::compile_in(ctx, &tokens);
        ScopedTheme {
            tokens,
            kit,
            context_style: Arc::default(),
        }
    }

    pub fn tokens(&self) -> &Arc<Tokens> {
        &self.tokens
    }

    pub fn kit(&self) -> &Arc<Kit> {
        &self.kit
    }

    /// The scoped theme drawing now, if any.
    pub fn current(ctx: &Context) -> Option<ScopedTheme> {
        SCOPED.get(ctx)
    }

    /// Draws `add` in a child `Ui` with this theme: egui's style for it,
    /// and [`Tokens::of`] and `Kit::of` giving its tokens and kit until
    /// `add` returns, as [`ScopedTheme::within`] does for the popups
    /// opened in it (scopes nest; the previous one comes back, even when
    /// `add` panics).
    pub fn show<R>(&self, ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        self.within(&ctx, || {
            ui.scope(|ui| {
                let mut style = style_with(&self.tokens, &self.kit, ui.style());
                snap_strokes(&mut style, ppp);
                ui.set_style(style);
                add(ui)
            })
            .inner
        })
    }

    /// Runs `f` with this theme as the context's, without a `Ui` of its
    /// own: [`Tokens::of`] and `Kit::of` give its tokens and kit, and
    /// egui's context style (what egui builds popups, tooltips, menus
    /// and windows from) is its style. Everything comes back when `f`
    /// returns, even when it panics. For a popup drawn after the scope
    /// that opened it has ended: record [`ScopedTheme::current`] when it
    /// opens, and draw it within.
    pub fn within<R>(&self, ctx: &Context, f: impl FnOnce() -> R) -> R {
        // upstream: egui styles a popup's `Ui` from the context, not from
        // the `Ui` that opened it (docs/upstream.md, "popups inside a
        // scoped style").
        struct Restore {
            ctx: Context,
            previous: Option<ScopedTheme>,
            styles: (Arc<Style>, Arc<Style>),
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                let (dark, light) = self.styles.clone();
                self.ctx.options_mut(|o| {
                    o.dark_style = dark;
                    o.light_style = light;
                });
                match self.previous.take() {
                    Some(p) => SCOPED.set(&self.ctx, p),
                    None => {
                        SCOPED.remove(&self.ctx);
                    }
                }
            }
        }
        let style = self.context_style(&ctx.global_style(), ctx.pixels_per_point());
        let previous = SCOPED.get(ctx);
        let styles = ctx.options(|o| (o.dark_style.clone(), o.light_style.clone()));
        SCOPED.set(ctx, self.clone());
        let _restore = Restore {
            ctx: ctx.clone(),
            previous,
            styles,
        };
        // Both of egui's styles: whatever its mode, the scope's is drawn.
        ctx.options_mut(|o| {
            o.dark_style = style.clone();
            o.light_style = style;
        });
        f()
    }

    /// The context's style in this theme, made from `base` (the
    /// context's style outside the scope) at a scale; made again only
    /// when either changes.
    fn context_style(&self, base: &Arc<Style>, ppp: f32) -> Arc<Style> {
        let mut made = crate::state::lock(&self.context_style);
        if let Some(m) = made.as_ref()
            && Arc::ptr_eq(&m.base, base)
            && m.ppp == ppp
        {
            return m.style.clone();
        }
        let mut style = style_with(&self.tokens, &self.kit, base);
        snap_strokes(&mut style, ppp);
        let style = Arc::new(style);
        *made = Some(ContextStyle {
            base: base.clone(),
            ppp,
            style: style.clone(),
        });
        style
    }
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
    v.window_shadow = t.shadow.window;
    v.popup_shadow = t.shadow.popup;
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
    // The same mapping as a `Stock::Button` scope (`Kit::scope`), for
    // every egui widget.
    kit.scoped
        .get(crate::kit::Stock::Button, crate::kit::Tone::Neutral)
        .apply_widgets(&mut v);
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
        (TextStyle::Button, t.font.button),
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

/// Installs typed tokens into a context, alone: egui's dark and light
/// styles both become the theme's and egui's mode is forced to the
/// theme's (so the system's light or dark preference does not swap it
/// out), and [`Tokens::of`] returns them. Call it between frames (at
/// start-up, or when the user picks another theme), not while drawing.
pub fn install_tokens(ctx: &Context, tokens: Tokens) -> Arc<Tokens> {
    install_variants(ctx, tokens, None)
}

/// Installs a theme and, when given one of the other mode, its
/// counterpart: each is written into its own egui style (dark or light)
/// with its own kit, and [`Tokens::of`] and `Kit::of` give the one of
/// egui's theme of the moment. egui's mode is set to the theme's (the
/// first one), as when it is alone, and it is egui's fallback when the
/// system says nothing; [`set_preference`] (or `ctx.set_theme`) then
/// switches between the two, or follows the system, without installing
/// anything again. A counterpart of the same mode is ignored (with a
/// warning): the theme is installed alone. Fonts and icon sizes come
/// from the first theme. Returns the first theme's tokens.
pub fn install_variants(ctx: &Context, tokens: Tokens, counterpart: Option<Tokens>) -> Arc<Tokens> {
    let tokens = Arc::new(tokens);
    let counterpart = counterpart.filter(|c| {
        let other = c.dark_mode() != tokens.dark_mode();
        if !other {
            log::warn!(
                target: "egui_pop",
                "theme: `{}` and `{}` are both {}: installed alone",
                tokens.resolved().spec().base,
                c.resolved().spec().base,
                if tokens.dark_mode() { "dark" } else { "light" }
            );
        }
        other
    });
    let variants = match counterpart {
        None => Variants::alone(tokens.clone()),
        Some(c) if tokens.dark_mode() => Variants {
            dark: tokens.clone(),
            light: Arc::new(c),
        },
        Some(c) => Variants {
            dark: Arc::new(c),
            light: tokens.clone(),
        },
    };
    let mode = tokens.egui_theme();
    ctx.set_theme(mode);
    ctx.options_mut(|o| o.fallback_theme = mode);
    // The kits first: egui's styles are drawn from them.
    let kits = crate::kit::install_variants(ctx, &variants.dark, &variants.light);
    let ppp = ctx.pixels_per_point();
    for theme in [Theme::Dark, Theme::Light] {
        let t = variants.get(theme);
        let kit = kits.get(theme);
        ctx.style_mut_of(theme, |s| {
            *s = style_with(t, kit, s);
            snap_strokes(s, ppp);
        });
    }
    TOKENS.set(ctx, variants);
    crate::fonts::install(ctx, &tokens.font.family_ui, &tokens.font.family_mono);
    crate::icon_set::install(ctx, &tokens);
    tokens
}

/// Resolves a spec against the given themes and installs it. Problems
/// (a broken user theme, a typo in an override) are in
/// `tokens.resolved().problems()`; the theme is installed anyway, with
/// fallbacks.
/// The spec's counterpart, when it has one ([`ThemeSpec::counterpart`],
/// the token files' `org.egui-pop.variant-of`), is installed with it
/// ([`install_variants`]).
pub fn install_with(ctx: &Context, themes: &Themes, spec: &ThemeSpec) -> Arc<Tokens> {
    install_variants(
        ctx,
        Tokens::new(themes.resolve(spec)),
        themes.resolve_counterpart(spec).map(Tokens::new),
    )
}

/// Resolves a spec against the built-in themes and installs it.
pub fn install(ctx: &Context, spec: &ThemeSpec) -> Arc<Tokens> {
    install_with(ctx, &Themes::default(), spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(name: &str) -> Tokens {
        Tokens::new(Themes::default().resolve(&ThemeSpec::new(name)))
    }

    #[test]
    fn scoped_themes_nest_and_come_back_even_after_a_panic() {
        let ctx = Context::default();
        install(&ctx, &ThemeSpec::new("pop-dark"));
        let light = ScopedTheme::new(&ctx, tokens("pop-light"));
        let hc = ScopedTheme::new(&ctx, tokens("pop-high-contrast"));
        let base = |ctx: &Context| Tokens::of(ctx).resolved().spec().base.clone();
        let installed = ctx.style_of(Theme::Dark);
        let mut out = ctx.run_ui(Default::default(), |ui| {
            light.show(ui, |ui| {
                assert_eq!(base(ui.ctx()), "pop-light");
                assert!(Arc::ptr_eq(&Kit::of(ui.ctx()), light.kit()));
                hc.show(ui, |ui| assert_eq!(base(ui.ctx()), "pop-high-contrast"));
                assert_eq!(base(ui.ctx()), "pop-light");
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    hc.show(ui, |_| panic!("a widget fails"));
                }));
                assert!(caught.is_err());
                assert_eq!(base(ui.ctx()), "pop-light");
                // The context's style (popups) is the scope's, back
                // after the panic.
                assert!(!ui.ctx().global_style().visuals.dark_mode);
                assert!(!ui.ctx().style_of(Theme::Dark).visuals.dark_mode);
            });
            assert_eq!(base(ui.ctx()), "pop-dark");
            assert!(ScopedTheme::current(ui.ctx()).is_none());
            assert_eq!(*ui.ctx().style_of(Theme::Dark), *installed);
        });
        out.textures_delta.clear();
    }
}
