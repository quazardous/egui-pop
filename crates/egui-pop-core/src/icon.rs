//! Icons by role: a glyph (from whatever icon font the application
//! loaded), an image, or one of a few shapes painted here; sized by role
//! and coloured by tone from the theme's tokens. No icon font is imposed.
//!
//! ```
//! use egui_pop_core::icon::{Icon, Role, Shape, Tone};
//!
//! fn toolbar(ui: &mut egui::Ui) {
//!     let close = Icon::Shape(Shape::Close).role(Role::Toolbar).label("Close");
//!     if ui.add(close.sense_click()).clicked() { /* ... */ }
//!     ui.add(Icon::glyph("★").tone(Tone::Accent).label("Favourite"));
//! }
//! ```

use egui::{
    Color32, FontFamily, FontId, ImageSource, Pos2, Rect, Response, Sense, Stroke, Ui, Widget,
    WidgetInfo, WidgetType, pos2, vec2,
};

use crate::theme::Tokens;

/// Shapes painted with lines and fills, for the symbols egui's default
/// fonts lack.
// upstream: egui's default fonts miss ✓ ▸ ◆ ● (see docs/upstream.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Check,
    ChevronRight,
    ChevronDown,
    Close,
    Plus,
    Minus,
    Dot,
    Diamond,
}

/// What the icon is.
#[derive(Clone, Debug)]
pub enum Icon {
    /// Text in a font family (an icon font the application installed, or
    /// any font).
    Glyph {
        text: String,
        family: FontFamily,
    },
    Image(ImageSource<'static>),
    Shape(Shape),
}

/// Where it is used, which decides its size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    /// In a line of text.
    #[default]
    Inline,
    Toolbar,
    /// In a panel strip.
    Panel,
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

impl Role {
    pub fn size(self, t: &Tokens) -> f32 {
        match self {
            Role::Inline => t.size.icon_inline,
            Role::Toolbar => t.size.icon_toolbar,
            Role::Panel => t.size.icon_panel,
        }
    }
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

impl Icon {
    /// A glyph in the proportional family.
    pub fn glyph(text: &str) -> Icon {
        Icon::Glyph {
            text: text.into(),
            family: FontFamily::Proportional,
        }
    }

    /// Sized for a role.
    pub fn role(self, role: Role) -> IconWidget {
        IconWidget::new(self).role(role)
    }

    /// Coloured by a tone.
    pub fn tone(self, tone: Tone) -> IconWidget {
        IconWidget::new(self).tone(tone)
    }

    /// With an accessible name.
    pub fn label(self, label: &str) -> IconWidget {
        IconWidget::new(self).label(label)
    }

    /// Paints the icon in a square.
    pub fn paint(&self, ui: &Ui, rect: Rect, color: Color32) {
        let p = ui.painter();
        match self {
            Icon::Glyph { text, family } => {
                p.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    FontId::new(rect.height(), family.clone()),
                    color,
                );
            }
            Icon::Image(source) => {
                egui::Image::new(source.clone())
                    .tint(color)
                    .paint_at(ui, rect);
            }
            Icon::Shape(shape) => paint_shape(p, *shape, rect, color),
        }
    }
}

fn paint_shape(p: &egui::Painter, shape: Shape, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.18);
    let w = (rect.width() / 10.0).max(1.25);
    let stroke = Stroke::new(w, color);
    let at = |x: f32, y: f32| -> Pos2 { pos2(r.left() + x * r.width(), r.top() + y * r.height()) };
    match shape {
        Shape::Check => {
            p.line(vec![at(0.05, 0.55), at(0.38, 0.85), at(0.95, 0.15)], stroke);
        }
        Shape::ChevronRight => {
            p.line(vec![at(0.35, 0.1), at(0.75, 0.5), at(0.35, 0.9)], stroke);
        }
        Shape::ChevronDown => {
            p.line(vec![at(0.1, 0.35), at(0.5, 0.75), at(0.9, 0.35)], stroke);
        }
        Shape::Close => {
            p.line_segment([at(0.1, 0.1), at(0.9, 0.9)], stroke);
            p.line_segment([at(0.9, 0.1), at(0.1, 0.9)], stroke);
        }
        Shape::Plus => {
            p.line_segment([at(0.5, 0.05), at(0.5, 0.95)], stroke);
            p.line_segment([at(0.05, 0.5), at(0.95, 0.5)], stroke);
        }
        Shape::Minus => {
            p.line_segment([at(0.05, 0.5), at(0.95, 0.5)], stroke);
        }
        Shape::Dot => {
            p.circle_filled(rect.center(), r.width() * 0.3, color);
        }
        Shape::Diamond => {
            p.add(egui::Shape::convex_polygon(
                vec![at(0.5, 0.0), at(1.0, 0.5), at(0.5, 1.0), at(0.0, 0.5)],
                color,
                Stroke::NONE,
            ));
        }
    }
}

/// An icon as a widget.
#[derive(Clone, Debug)]
#[must_use]
pub struct IconWidget {
    icon: Icon,
    role: Role,
    tone: Tone,
    label: Option<String>,
    sense: Sense,
}

impl IconWidget {
    pub fn new(icon: Icon) -> Self {
        Self {
            icon,
            role: Role::Inline,
            tone: Tone::Default,
            label: None,
            sense: Sense::hover(),
        }
    }

    pub fn role(mut self, role: Role) -> Self {
        self.role = role;
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
        let side = self.role.size(&t);
        let (rect, response) = ui.allocate_exact_size(vec2(side, side), self.sense);
        if ui.is_rect_visible(rect) {
            let mut color = self.tone.color(&t);
            if self.sense.senses_click() && response.hovered() {
                color = t.text.strong;
            }
            if response.has_focus() {
                ui.painter().rect_stroke(
                    rect.expand(1.0),
                    t.radius.widget,
                    Stroke::new(1.5, t.focus_ring),
                    egui::StrokeKind::Outside,
                );
            }
            self.icon.paint(ui, rect, color);
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
