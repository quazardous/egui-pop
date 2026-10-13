//! Icons by role: an [`Icon`] names what the icon means (`Check`,
//! `Search`, `Warning`), never a codepoint. With the `phosphor` feature
//! the [Phosphor](https://phosphoricons.com) font (through
//! `egui-phosphor`, MIT) is added to the context as a family of its own
//! and icons are drawn from it; without it, or for the marks that are
//! always painted, the icon is painted with lines and fills. Either way
//! the icon is sized by its [`Size`] and coloured by its [`Tone`] from
//! the theme's tokens.
//!
//! A [`Source`] is what an [`IconWidget`] draws: an icon by role, a glyph
//! from any font, or an image.
//!
//! ```
//! use egui_pop_core::icon::{Icon, Size, Tone};
//!
//! fn toolbar(ui: &mut egui::Ui) {
//!     let close = Icon::Close.size(Size::Toolbar).label("Close");
//!     if ui.add(close.sense_click()).clicked() { /* ... */ }
//!     ui.add(Icon::Warning.tone(Tone::Warning).label("Unsaved changes"));
//! }
//! ```

use std::f32::consts::PI;

use egui::{
    Color32, Context, FontFamily, FontId, ImageSource, Painter, Pos2, Rect, Response, Sense,
    Stroke, StrokeKind, Ui, Widget, WidgetInfo, WidgetType, pos2, vec2,
};

use crate::theme::Tokens;

/// The family the icon font is installed under (`phosphor` feature).
pub const FAMILY: &str = "egui-pop-icons";

/// What an icon means. Each has a glyph in the icon font (with the
/// `phosphor` feature) and a painted fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Icon {
    /// A tick: a checked menu item, a passed check.
    Check,
    /// A submenu, a collapsed section.
    ChevronRight,
    /// An open section, a drop-down.
    ChevronDown,
    ChevronUp,
    ChevronLeft,
    /// Close, dismiss, clear.
    Close,
    Plus,
    Minus,
    /// A small filled dot: a changed value, a status.
    Dot,
    /// A small filled diamond: a value moved by someone else.
    Diamond,
    Search,
    Settings,
    Folder,
    File,
    /// Up one folder.
    Up,
    Info,
    Warning,
    Error,
    Success,
    Undo,
    Redo,
    /// Back to the default.
    Reset,
    Copy,
    Delete,
    Keyboard,
    /// The command palette.
    Command,
    /// A menu (a list).
    Menu,
    /// More actions.
    More,
    Edit,
    Help,
    Save,
    Fullscreen,
    ExitFullscreen,
    Notification,
    /// Light and dark.
    Theme,
    Visible,
    Hidden,
    /// Recent items.
    History,
    Home,
    /// Locked: not to be changed.
    Lock,
    /// Unlocked: free to change.
    Unlock,
    /// An eyedropper: pick a value or a colour from the image.
    Picker,
}

impl Icon {
    /// Every icon, in declaration order (the gallery shows them).
    pub const ALL: &'static [Icon] = &[
        Icon::Check,
        Icon::ChevronRight,
        Icon::ChevronDown,
        Icon::ChevronUp,
        Icon::ChevronLeft,
        Icon::Close,
        Icon::Plus,
        Icon::Minus,
        Icon::Dot,
        Icon::Diamond,
        Icon::Search,
        Icon::Settings,
        Icon::Folder,
        Icon::File,
        Icon::Up,
        Icon::Info,
        Icon::Warning,
        Icon::Error,
        Icon::Success,
        Icon::Undo,
        Icon::Redo,
        Icon::Reset,
        Icon::Copy,
        Icon::Delete,
        Icon::Keyboard,
        Icon::Command,
        Icon::Menu,
        Icon::More,
        Icon::Edit,
        Icon::Help,
        Icon::Save,
        Icon::Fullscreen,
        Icon::ExitFullscreen,
        Icon::Notification,
        Icon::Theme,
        Icon::Visible,
        Icon::Hidden,
        Icon::History,
        Icon::Home,
        Icon::Lock,
        Icon::Unlock,
        Icon::Picker,
    ];

    /// The icon of a name (`settings`), as [`Icon::name`] gives it.
    pub fn by_name(name: &str) -> Option<Icon> {
        Icon::ALL.iter().copied().find(|i| i.name() == name)
    }

    /// Its name in kebab case (`chevron-right`): for the debug bridge and
    /// the gallery.
    pub fn name(self) -> &'static str {
        match self {
            Icon::Check => "check",
            Icon::ChevronRight => "chevron-right",
            Icon::ChevronDown => "chevron-down",
            Icon::ChevronUp => "chevron-up",
            Icon::ChevronLeft => "chevron-left",
            Icon::Close => "close",
            Icon::Plus => "plus",
            Icon::Minus => "minus",
            Icon::Dot => "dot",
            Icon::Diamond => "diamond",
            Icon::Search => "search",
            Icon::Settings => "settings",
            Icon::Folder => "folder",
            Icon::File => "file",
            Icon::Up => "up",
            Icon::Info => "info",
            Icon::Warning => "warning",
            Icon::Error => "error",
            Icon::Success => "success",
            Icon::Undo => "undo",
            Icon::Redo => "redo",
            Icon::Reset => "reset",
            Icon::Copy => "copy",
            Icon::Delete => "delete",
            Icon::Keyboard => "keyboard",
            Icon::Command => "command",
            Icon::Menu => "menu",
            Icon::More => "more",
            Icon::Edit => "edit",
            Icon::Help => "help",
            Icon::Save => "save",
            Icon::Fullscreen => "fullscreen",
            Icon::ExitFullscreen => "exit-fullscreen",
            Icon::Notification => "notification",
            Icon::Theme => "theme",
            Icon::Visible => "visible",
            Icon::Hidden => "hidden",
            Icon::History => "history",
            Icon::Home => "home",
            Icon::Lock => "lock",
            Icon::Unlock => "unlock",
            Icon::Picker => "picker",
        }
    }

    /// Its glyph in the icon font; `None` without the `phosphor` feature,
    /// and for the marks that are always painted (dot, diamond).
    pub fn glyph(self) -> Option<&'static str> {
        glyph(self)
    }

    /// Sized for a place.
    pub fn size(self, size: Size) -> IconWidget {
        IconWidget::new(self).size(size)
    }

    /// Coloured by a tone.
    pub fn tone(self, tone: Tone) -> IconWidget {
        IconWidget::new(self).tone(tone)
    }

    /// With an accessible name.
    pub fn label(self, label: &str) -> IconWidget {
        IconWidget::new(self).label(label)
    }

    /// Paints the icon in a square: the font's glyph when the icon font
    /// is installed in this context, else the painted shape.
    pub fn paint(self, painter: &Painter, rect: Rect, color: Color32) {
        match self.glyph() {
            Some(g) if font_ready(painter.ctx()) => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    g,
                    FontId::new(rect.height(), FontFamily::Name(FAMILY.into())),
                    color,
                );
            }
            _ => paint_fallback(painter, self, rect, color),
        }
    }
}

#[cfg(feature = "phosphor")]
fn glyph(icon: Icon) -> Option<&'static str> {
    use egui_phosphor::regular as p;
    Some(match icon {
        Icon::Check => p::CHECK,
        Icon::ChevronRight => p::CARET_RIGHT,
        Icon::ChevronDown => p::CARET_DOWN,
        Icon::ChevronUp => p::CARET_UP,
        Icon::ChevronLeft => p::CARET_LEFT,
        Icon::Close => p::X,
        Icon::Plus => p::PLUS,
        Icon::Minus => p::MINUS,
        Icon::Dot | Icon::Diamond => return None,
        Icon::Search => p::MAGNIFYING_GLASS,
        Icon::Settings => p::GEAR_SIX,
        Icon::Folder => p::FOLDER,
        Icon::File => p::FILE,
        Icon::Up => p::ARROW_UP,
        Icon::Info => p::INFO,
        Icon::Warning => p::WARNING,
        Icon::Error => p::X_CIRCLE,
        Icon::Success => p::CHECK_CIRCLE,
        Icon::Undo => p::ARROW_U_UP_LEFT,
        Icon::Redo => p::ARROW_U_UP_RIGHT,
        Icon::Reset => p::ARROW_COUNTER_CLOCKWISE,
        Icon::Copy => p::COPY,
        Icon::Delete => p::TRASH,
        Icon::Keyboard => p::KEYBOARD,
        Icon::Command => p::COMMAND,
        Icon::Menu => p::LIST,
        Icon::More => p::DOTS_THREE,
        Icon::Edit => p::PENCIL_SIMPLE,
        Icon::Help => p::QUESTION,
        Icon::Save => p::FLOPPY_DISK,
        Icon::Fullscreen => p::CORNERS_OUT,
        Icon::ExitFullscreen => p::CORNERS_IN,
        Icon::Notification => p::BELL,
        Icon::Theme => p::CIRCLE_HALF,
        Icon::Visible => p::EYE,
        Icon::Hidden => p::EYE_SLASH,
        Icon::History => p::CLOCK_COUNTER_CLOCKWISE,
        Icon::Home => p::HOUSE,
        Icon::Lock => p::LOCK_SIMPLE,
        Icon::Unlock => p::LOCK_SIMPLE_OPEN,
        Icon::Picker => p::EYEDROPPER,
    })
}

#[cfg(not(feature = "phosphor"))]
fn glyph(_icon: Icon) -> Option<&'static str> {
    None
}

/// Adds the icon font to a context, in a family of its own (the text
/// fonts are untouched). In use from the next pass; adding it again does
/// nothing. Called by `theme::install`; does nothing without the
/// `phosphor` feature.
pub fn install(ctx: &Context) {
    #[cfg(feature = "phosphor")]
    {
        use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
        ctx.add_font(FontInsert::new(
            FAMILY,
            egui::FontData::from_static(egui_phosphor::Variant::Regular.font_bytes()),
            vec![InsertFontFamily {
                family: FontFamily::Name(FAMILY.into()),
                priority: FontPriority::Highest,
            }],
        ));
    }
    #[cfg(not(feature = "phosphor"))]
    let _ = ctx;
}

/// Whether the icon font is in use in this context.
pub fn font_ready(ctx: &Context) -> bool {
    cfg!(feature = "phosphor")
        && ctx.fonts(|f| {
            f.definitions()
                .families
                .contains_key(&FontFamily::Name(FAMILY.into()))
        })
}

/// Paints an icon's fallback shape, whatever the fonts. Its strokes are
/// whole physical pixels at the painter's scale.
pub fn paint_fallback(p: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.16);
    let len = r.width();
    // The weight shapes are drawn to; strokes snap it to whole physical
    // pixels at the painter's scale, so lines stay crisp.
    let w = (rect.width() / 11.0).max(1.25);
    let ppp = p.pixels_per_point();
    let stroke = Stroke::new(crate::scale::snap(w, ppp), color);
    let at = |x: f32, y: f32| -> Pos2 { pos2(r.left() + x * len, r.top() + y * r.height()) };
    let arc = |from: f32, to: f32, radius: f32, centre: Pos2| -> Vec<Pos2> {
        (0..=16)
            .map(|i| {
                let a = from + (to - from) * i as f32 / 16.0;
                centre + vec2(a.cos(), a.sin()) * radius
            })
            .collect()
    };
    let outline = |pts: Vec<Pos2>| {
        p.add(egui::Shape::closed_line(pts, stroke));
    };
    let frame = |a: Pos2, b: Pos2| {
        p.rect_stroke(Rect::from_min_max(a, b), 1.0, stroke, StrokeKind::Middle);
    };
    let ring = || p.circle_stroke(r.center(), len * 0.48, stroke);
    let point = |x: f32, y: f32| p.circle_filled(at(x, y), w * 0.8, color);
    match icon {
        Icon::Check => {
            p.line(vec![at(0.05, 0.55), at(0.38, 0.85), at(0.95, 0.15)], stroke);
        }
        Icon::ChevronRight => {
            p.line(vec![at(0.35, 0.1), at(0.75, 0.5), at(0.35, 0.9)], stroke);
        }
        Icon::ChevronLeft => {
            p.line(vec![at(0.65, 0.1), at(0.25, 0.5), at(0.65, 0.9)], stroke);
        }
        Icon::ChevronDown => {
            p.line(vec![at(0.1, 0.35), at(0.5, 0.75), at(0.9, 0.35)], stroke);
        }
        Icon::ChevronUp => {
            p.line(vec![at(0.1, 0.65), at(0.5, 0.25), at(0.9, 0.65)], stroke);
        }
        Icon::Close => {
            p.line_segment([at(0.1, 0.1), at(0.9, 0.9)], stroke);
            p.line_segment([at(0.9, 0.1), at(0.1, 0.9)], stroke);
        }
        Icon::Plus => {
            p.line_segment([at(0.5, 0.05), at(0.5, 0.95)], stroke);
            p.line_segment([at(0.05, 0.5), at(0.95, 0.5)], stroke);
        }
        Icon::Minus => {
            p.line_segment([at(0.05, 0.5), at(0.95, 0.5)], stroke);
        }
        Icon::Dot => {
            p.circle_filled(rect.center(), len * 0.3, color);
        }
        Icon::Diamond => {
            p.add(egui::Shape::convex_polygon(
                vec![at(0.5, 0.05), at(0.95, 0.5), at(0.5, 0.95), at(0.05, 0.5)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Search => {
            p.circle_stroke(at(0.42, 0.42), len * 0.32, stroke);
            p.line_segment([at(0.66, 0.66), at(0.95, 0.95)], stroke);
        }
        Icon::Settings => {
            p.circle_stroke(r.center(), len * 0.16, stroke);
            p.circle_stroke(r.center(), len * 0.32, stroke);
            for i in 0..8 {
                let a = i as f32 * PI / 4.0;
                let d = vec2(a.cos(), a.sin());
                p.line_segment(
                    [r.center() + d * len * 0.32, r.center() + d * len * 0.48],
                    stroke,
                );
            }
        }
        Icon::Folder => outline(vec![
            at(0.0, 0.18),
            at(0.38, 0.18),
            at(0.48, 0.3),
            at(1.0, 0.3),
            at(1.0, 0.88),
            at(0.0, 0.88),
        ]),
        Icon::File => {
            outline(vec![
                at(0.15, 0.0),
                at(0.62, 0.0),
                at(0.88, 0.26),
                at(0.88, 1.0),
                at(0.15, 1.0),
            ]);
            p.line(vec![at(0.62, 0.0), at(0.62, 0.26), at(0.88, 0.26)], stroke);
        }
        Icon::Up => {
            p.line_segment([at(0.5, 0.05), at(0.5, 0.95)], stroke);
            p.line(vec![at(0.15, 0.4), at(0.5, 0.05), at(0.85, 0.4)], stroke);
        }
        Icon::Info => {
            ring();
            p.line_segment([at(0.5, 0.45), at(0.5, 0.75)], stroke);
            point(0.5, 0.28);
        }
        Icon::Help => {
            ring();
            p.line(arc(-PI, 0.0, len * 0.16, at(0.5, 0.36)), stroke);
            p.line(vec![at(0.66, 0.36), at(0.5, 0.52), at(0.5, 0.62)], stroke);
            point(0.5, 0.78);
        }
        Icon::Warning => {
            outline(vec![at(0.5, 0.05), at(1.0, 0.92), at(0.0, 0.92)]);
            p.line_segment([at(0.5, 0.38), at(0.5, 0.62)], stroke);
            point(0.5, 0.77);
        }
        Icon::Error => {
            ring();
            p.line_segment([at(0.32, 0.32), at(0.68, 0.68)], stroke);
            p.line_segment([at(0.68, 0.32), at(0.32, 0.68)], stroke);
        }
        Icon::Success => {
            ring();
            p.line(vec![at(0.28, 0.52), at(0.44, 0.68), at(0.74, 0.36)], stroke);
        }
        Icon::Undo | Icon::Redo => {
            let flip = icon == Icon::Redo;
            let x = |v: f32| if flip { 1.0 - v } else { v };
            let (from, to) = if flip {
                (PI * 0.5, PI * 1.5)
            } else {
                (-PI * 0.5, PI * 0.5)
            };
            p.line_segment([at(x(0.1), 0.3), at(x(0.65), 0.3)], stroke);
            p.line(arc(from, to, len * 0.28, at(x(0.65), 0.58)), stroke);
            p.line_segment([at(x(0.65), 0.86), at(x(0.3), 0.86)], stroke);
            p.line(
                vec![at(x(0.3), 0.1), at(x(0.1), 0.3), at(x(0.3), 0.5)],
                stroke,
            );
        }
        Icon::Reset | Icon::History => {
            p.line(arc(-PI * 0.75, PI, len * 0.45, r.center()), stroke);
            let a = -PI * 0.75;
            let start = r.center() + vec2(a.cos(), a.sin()) * len * 0.45;
            p.line(
                vec![
                    start + vec2(0.0, -len * 0.25),
                    start,
                    start + vec2(len * 0.25, 0.0),
                ],
                stroke,
            );
            if icon == Icon::History {
                p.line(vec![at(0.5, 0.25), at(0.5, 0.5), at(0.7, 0.62)], stroke);
            }
        }
        Icon::Copy => {
            frame(at(0.3, 0.3), at(1.0, 1.0));
            p.line(vec![at(0.0, 0.7), at(0.0, 0.0), at(0.7, 0.0)], stroke);
        }
        Icon::Delete => {
            p.line_segment([at(0.0, 0.18), at(1.0, 0.18)], stroke);
            p.line(
                vec![
                    at(0.35, 0.18),
                    at(0.35, 0.02),
                    at(0.65, 0.02),
                    at(0.65, 0.18),
                ],
                stroke,
            );
            outline(vec![
                at(0.12, 0.18),
                at(0.88, 0.18),
                at(0.8, 1.0),
                at(0.2, 1.0),
            ]);
        }
        Icon::Keyboard => {
            frame(at(0.0, 0.2), at(1.0, 0.8));
            for x in [0.22, 0.42, 0.62, 0.8] {
                point(x, 0.4);
            }
            p.line_segment([at(0.28, 0.62), at(0.72, 0.62)], stroke);
        }
        Icon::Command => {
            frame(at(0.32, 0.32), at(0.68, 0.68));
            for (x, y) in [(0.2, 0.2), (0.8, 0.2), (0.2, 0.8), (0.8, 0.8)] {
                p.circle_stroke(at(x, y), len * 0.13, stroke);
            }
        }
        Icon::Menu => {
            for y in [0.2, 0.5, 0.8] {
                p.line_segment([at(0.05, y), at(0.95, y)], stroke);
            }
        }
        Icon::More => {
            for x in [0.15, 0.5, 0.85] {
                p.circle_filled(at(x, 0.5), w * 1.1, color);
            }
        }
        Icon::Edit => outline(vec![
            at(0.7, 0.05),
            at(0.95, 0.3),
            at(0.3, 0.95),
            at(0.05, 0.95),
            at(0.05, 0.7),
        ]),
        Icon::Save => {
            outline(vec![
                at(0.0, 0.0),
                at(0.8, 0.0),
                at(1.0, 0.2),
                at(1.0, 1.0),
                at(0.0, 1.0),
            ]);
            frame(at(0.25, 0.0), at(0.7, 0.3));
            frame(at(0.2, 0.6), at(0.8, 1.0));
        }
        Icon::Fullscreen | Icon::ExitFullscreen => {
            let inward = icon == Icon::ExitFullscreen;
            for (sx, sy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                let dx: f32 = if sx == 0.0 { 1.0 } else { -1.0 };
                let dy: f32 = if sy == 0.0 { 1.0 } else { -1.0 };
                let points = if inward {
                    let (cx, cy) = (sx + dx * 0.35, sy + dy * 0.35);
                    vec![at(sx, cy), at(cx, cy), at(cx, sy)]
                } else {
                    vec![at(sx + dx * 0.35, sy), at(sx, sy), at(sx, sy + dy * 0.35)]
                };
                p.line(points, stroke);
            }
        }
        Icon::Notification => {
            p.line(arc(PI, 2.0 * PI, len * 0.32, at(0.5, 0.5)), stroke);
            p.line(
                vec![
                    at(0.18, 0.5),
                    at(0.18, 0.75),
                    at(0.05, 0.85),
                    at(0.95, 0.85),
                    at(0.82, 0.75),
                    at(0.82, 0.5),
                ],
                stroke,
            );
            point(0.5, 0.98);
        }
        Icon::Theme => {
            ring();
            let mut half = arc(-PI * 0.5, PI * 0.5, len * 0.48, r.center());
            half.push(r.center());
            p.add(egui::Shape::convex_polygon(half, color, Stroke::NONE));
        }
        Icon::Visible | Icon::Hidden => {
            let c = r.center();
            let lid = |up: bool| -> Vec<Pos2> {
                (0..=12)
                    .map(|i| {
                        let t = i as f32 / 12.0;
                        let y = (t * PI).sin() * len * 0.32;
                        pos2(r.left() + t * len, if up { c.y - y } else { c.y + y })
                    })
                    .collect()
            };
            p.line(lid(true), stroke);
            p.line(lid(false), stroke);
            p.circle_stroke(c, len * 0.14, stroke);
            if icon == Icon::Hidden {
                p.line_segment([at(0.1, 0.05), at(0.9, 0.95)], stroke);
            }
        }
        Icon::Home => {
            p.line(vec![at(0.0, 0.48), at(0.5, 0.05), at(1.0, 0.48)], stroke);
            p.line(
                vec![
                    at(0.15, 0.36),
                    at(0.15, 0.95),
                    at(0.85, 0.95),
                    at(0.85, 0.36),
                ],
                stroke,
            );
        }
        Icon::Lock | Icon::Unlock => {
            frame(at(0.1, 0.45), at(0.9, 0.98));
            // The shackle: closed into the body, or lifted and open.
            let (lift, right) = if icon == Icon::Lock {
                (0.0, 0.45)
            } else {
                (0.12, 0.3)
            };
            let top = at(0.5, 0.25 - lift);
            p.line(arc(PI, 2.0 * PI, len * 0.26, top), stroke);
            p.line_segment([at(0.24, 0.25 - lift), at(0.24, 0.45 - lift)], stroke);
            p.line_segment([at(0.76, 0.25 - lift), at(0.76, right - lift)], stroke);
        }
        Icon::Picker => {
            // The tube down to its tip, the collar across it, the bulb.
            p.line_segment([at(0.05, 0.95), at(0.6, 0.4)], stroke);
            p.line_segment([at(0.42, 0.3), at(0.7, 0.58)], stroke);
            p.line_segment(
                [at(0.62, 0.38), at(0.85, 0.15)],
                Stroke::new(crate::scale::snap(w * 2.6, ppp), color),
            );
        }
    }
}

/// What an icon widget draws.
#[derive(Clone, Debug)]
pub enum Source {
    /// An icon by role.
    Icon(Icon),
    /// Text in a font family (an icon font the application installed, or
    /// any font).
    Glyph {
        text: String,
        family: FontFamily,
    },
    Image(ImageSource<'static>),
}

impl From<Icon> for Source {
    fn from(icon: Icon) -> Self {
        Source::Icon(icon)
    }
}

impl Source {
    /// A glyph in the proportional family.
    pub fn glyph(text: &str) -> Source {
        Source::Glyph {
            text: text.into(),
            family: FontFamily::Proportional,
        }
    }

    /// Paints it in a square.
    pub fn paint(&self, ui: &Ui, rect: Rect, color: Color32) {
        match self {
            Source::Icon(i) => i.paint(ui.painter(), rect, color),
            Source::Glyph { text, family } => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    FontId::new(rect.height(), family.clone()),
                    color,
                );
            }
            Source::Image(source) => {
                egui::Image::new(source.clone())
                    .tint(color)
                    .paint_at(ui, rect);
            }
        }
    }
}

/// Where an icon is used, which decides its size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Size {
    /// In a line of text.
    #[default]
    Inline,
    Toolbar,
    /// In a panel strip.
    Panel,
}

impl Size {
    /// Its side in points, from the tokens.
    pub fn points(self, t: &Tokens) -> f32 {
        match self {
            Size::Inline => t.size.icon_inline,
            Size::Toolbar => t.size.icon_toolbar,
            Size::Panel => t.size.icon_panel,
        }
    }
}

/// Its colour, by meaning.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Default,
    Muted,
    Strong,
    Accent,
    OnAccent,
    Danger,
    Warning,
    Success,
    Info,
}

impl Tone {
    pub fn color(self, t: &Tokens) -> Color32 {
        match self {
            Tone::Default => t.text.default,
            Tone::Muted => t.text.muted,
            Tone::Strong => t.text.strong,
            Tone::Accent => t.accent.default,
            Tone::OnAccent => t.text.on_accent,
            Tone::Danger => t.status.danger,
            Tone::Warning => t.status.warning,
            Tone::Success => t.status.success,
            Tone::Info => t.status.info,
        }
    }
}

/// An icon as a widget.
#[derive(Clone, Debug)]
#[must_use]
pub struct IconWidget {
    source: Source,
    size: Size,
    tone: Tone,
    label: Option<String>,
    sense: Sense,
}

impl IconWidget {
    pub fn new(source: impl Into<Source>) -> Self {
        Self {
            source: source.into(),
            size: Size::Inline,
            tone: Tone::Default,
            label: None,
            sense: Sense::hover(),
        }
    }

    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Its accessible name (and tooltip); an icon alone says nothing to a
    /// screen reader without it.
    pub fn label(mut self, label: &str) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Clickable (a button made of the icon alone).
    pub fn sense_click(mut self) -> Self {
        self.sense = Sense::click();
        self
    }
}

impl Widget for IconWidget {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = Tokens::of(ui.ctx());
        let side = self.size.points(&t);
        let (rect, response) = ui.allocate_exact_size(vec2(side, side), self.sense);
        if ui.is_rect_visible(rect) {
            let mut color = self.tone.color(&t);
            if !ui.is_enabled() {
                color = t.text.muted;
            } else if self.sense.senses_click() && response.hovered() {
                color = t.text.strong;
            }
            if response.has_focus() {
                ui.painter().rect_stroke(
                    rect.expand(1.0),
                    t.radius.widget,
                    Stroke::new(crate::scale::stroke(ui.ctx(), 1.5), t.focus_ring),
                    StrokeKind::Outside,
                );
            }
            self.source.paint(ui, rect, color);
        }
        let kind = if self.sense.senses_click() {
            WidgetType::Button
        } else {
            WidgetType::Image
        };
        let enabled = ui.is_enabled();
        let label = self.label.clone().unwrap_or_default();
        response.widget_info(|| WidgetInfo::labeled(kind, enabled, &label));
        match self.label {
            Some(l) if self.sense.senses_click() => response.on_hover_text(l),
            _ => response,
        }
    }
}

/// Allocates a square of `side` points and paints an icon in it, with no
/// interaction: a mark inside another widget (a menu tick, a toast's
/// level).
pub fn mark(ui: &mut Ui, icon: Icon, side: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    if ui.is_rect_visible(rect) {
        icon.paint(ui.painter(), rect, color);
    }
    response
}
