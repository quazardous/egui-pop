//! Glyphs by role: the small marks widgets draw (a branch's disclosure, a
//! sort direction, a menu's check...) named by what they mean, each mapped
//! by the kit to an icon of its set (`glyph.<role>` tokens). Widgets ask
//! for a role, never draw a shape: a kit turns the tree's triangles into
//! chevrons, or a check into a dot, with data only.

use crate::icon::Icon;
use crate::theme::Tokens;

macro_rules! glyphs {
    ($($(#[$doc:meta])* $role:ident: $name:literal => $default:ident,)*) => {
        /// What a small mark means. The vocabulary is fixed: a widget
        /// needing a new mark adds a role here, never a shape of its own.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Glyph {
            $($(#[$doc])* $role,)*
        }

        impl Glyph {
            /// Every role, in declaration order.
            pub const ALL: &'static [Glyph] = &[$(Glyph::$role,)*];

            /// Its token name (`glyph.<name>`).
            pub fn name(self) -> &'static str {
                match self {
                    $(Glyph::$role => $name,)*
                }
            }

            /// The icon drawn when a kit names none it knows.
            pub fn default_icon(self) -> Icon {
                match self {
                    $(Glyph::$role => Icon::$default,)*
                }
            }
        }
    };
}

glyphs! {
    /// An open branch, an expanded section.
    DisclosureOpen: "disclosure-open" => TriangleDown,
    /// A closed branch, a collapsed section.
    DisclosureClosed: "disclosure-closed" => TriangleRight,
    /// A column sorted ascending.
    SortAscending: "sort-ascending" => TriangleUp,
    /// A column sorted descending.
    SortDescending: "sort-descending" => TriangleDown,
    /// A checked item.
    Check: "check" => Check,
    /// The chosen item of a group.
    Radio: "radio" => Dot,
    /// A partly checked item.
    Indeterminate: "indeterminate" => Minus,
    /// An item of a list.
    Bullet: "bullet" => Dot,
    /// An item opening a submenu.
    Submenu: "submenu" => ChevronRight,
    /// Back, towards the start.
    ChevronStart: "chevron-start" => ChevronLeft,
    /// Forward, towards the end.
    ChevronEnd: "chevron-end" => ChevronRight,
    /// Closes or removes.
    Close: "close" => Close,
    /// More actions.
    More: "more" => More,
    /// Something to grab or to open: a folded area's strip.
    DragHandle: "drag-handle" => Grip,
    /// Expands, adds a level.
    Expand: "expand" => Plus,
    /// Collapses, removes a level.
    Collapse: "collapse" => Minus,
    /// A window's minimize button (drawn by the application).
    WindowMinimize: "window-minimize" => WindowMinimize,
    /// A window's maximize button.
    WindowMaximize: "window-maximize" => WindowMaximize,
    /// A maximized window's restore button.
    WindowRestore: "window-restore" => WindowRestore,
    /// A window's close button.
    WindowClose: "window-close" => WindowClose,
    /// A full screen window's button leaving full screen (where maximize
    /// was).
    WindowExitFullscreen: "window-exit-fullscreen" => ExitFullscreen,
    /// An LED that is off.
    LedOff: "led-off" => Circle,
    /// An LED that is on.
    LedOn: "led-on" => Disc,
    /// A busy LED (it pulses too, a still ring under reduced motion, so it may share the on LED's shape).
    LedBusy: "led-busy" => Disc,
    /// An LED warning.
    LedWarning: "led-warning" => Triangle,
    /// An LED in error.
    LedError: "led-error" => Square,
    /// A modified flag (`corner` fills the corner it sits in).
    FlagModified: "flag-modified" => Corner,
    /// A pinned flag.
    FlagPinned: "flag-pinned" => Diamond,
    /// A required flag.
    FlagRequired: "flag-required" => Asterisk,
    /// An invalid flag.
    FlagInvalid: "flag-invalid" => Triangle,
}

/// The icon of each role, compiled from the kit's tokens: read by index
/// while drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyphs(Vec<Icon>);

impl Default for Glyphs {
    /// Every role's default icon.
    fn default() -> Self {
        Glyphs(Glyph::ALL.iter().map(|g| g.default_icon()).collect())
    }
}

impl Glyphs {
    pub(super) fn compile(t: &Tokens, problems: &mut Vec<String>) -> Glyphs {
        let mut glyphs = Glyphs::read(t, problems);
        glyphs.keep_shapes_distinct(problems);
        glyphs
    }

    /// Two roles that must differ (`egui_pop_tokens::kit::DISTINCT_GLYPHS`)
    /// and draw the same icon go back to their defaults, which differ, so
    /// colour never carries a state alone; a problem says so.
    fn keep_shapes_distinct(&mut self, problems: &mut Vec<String>) {
        let role = |name: &str| Glyph::ALL.iter().copied().find(|g| g.name() == name);
        loop {
            let clash = egui_pop_tokens::kit::DISTINCT_GLYPHS
                .iter()
                .filter_map(|(a, b)| Some((role(a)?, role(b)?)))
                .find(|(a, b)| {
                    self.get(*a) == self.get(*b)
                        && (self.get(*a) != a.default_icon() || self.get(*b) != b.default_icon())
                });
            let Some((a, b)) = clash else { break };
            problems.push(format!(
                "glyph.{} and glyph.{} both draw `{}`: their states would differ by colour alone; `{}` and `{}` are drawn",
                a.name(),
                b.name(),
                self.get(a).name(),
                a.default_icon().name(),
                b.default_icon().name()
            ));
            self.0[a as usize] = a.default_icon();
            self.0[b as usize] = b.default_icon();
        }
    }

    fn read(t: &Tokens, problems: &mut Vec<String>) -> Glyphs {
        Glyphs(
            Glyph::ALL
                .iter()
                .map(|g| {
                    let path = format!("glyph.{}", g.name());
                    match super::string(t, &path) {
                        None => g.default_icon(),
                        Some(name) => Icon::by_name(&name).unwrap_or_else(|| {
                            problems.push(format!(
                                "{path}: no icon named `{name}`, `{}` is drawn",
                                g.default_icon().name()
                            ));
                            g.default_icon()
                        }),
                    }
                })
                .collect(),
        )
    }

    /// The icon of a role.
    pub fn get(&self, glyph: Glyph) -> Icon {
        self.0[glyph as usize]
    }
}
