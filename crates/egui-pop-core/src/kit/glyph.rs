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
