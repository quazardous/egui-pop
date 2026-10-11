//! The contract: the system and component tokens every theme provides.

/// The tier of a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// The palette: colour scales and raw values, named by what they are
    /// (`gray.4`, `gold.9`). Widgets never read them.
    Reference,
    /// Roles: what a value is for (`surface.panel`, `text.muted`).
    System,
    /// One component's tokens (`slider.fill`), aliases of system tokens.
    Component,
}

impl Tier {
    pub fn name(self) -> &'static str {
        match self {
            Tier::Reference => "reference",
            Tier::System => "system",
            Tier::Component => "component",
        }
    }
}

/// A token the contract requires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Required {
    pub path: &'static str,
    /// The DTCG `$type` it must have.
    pub ty: &'static str,
    pub tier: Tier,
    pub description: &'static str,
}

const fn sys(path: &'static str, ty: &'static str, description: &'static str) -> Required {
    Required {
        path,
        ty,
        tier: Tier::System,
        description,
    }
}

const fn comp(path: &'static str, ty: &'static str, description: &'static str) -> Required {
    Required {
        path,
        ty,
        tier: Tier::Component,
        description,
    }
}

/// Every token a theme must provide: the system tier and the component
/// tier. A theme missing one, or giving one of another type, gets the
/// built-in `pop-dark` value for it and a reported problem.
pub const CONTRACT: &[Required] = &[
    sys(
        "surface.canvas",
        "color",
        "behind documents, images and graphs",
    ),
    sys(
        "surface.panel",
        "color",
        "panels and side bars, the default background",
    ),
    sys("surface.raised", "color", "controls at rest"),
    sys("surface.hover", "color", "a control under the pointer"),
    sys(
        "surface.active",
        "color",
        "a control being pressed or dragged",
    ),
    sys(
        "surface.overlay",
        "color",
        "windows, menus, popups and dialogs",
    ),
    sys("surface.sunken", "color", "text fields and wells"),
    sys("text.default", "color", "ordinary text"),
    sys("text.muted", "color", "secondary text, hints"),
    sys(
        "text.strong",
        "color",
        "titles, emphasis, text under the pointer",
    ),
    sys("text.on-accent", "color", "text on an accent fill"),
    sys(
        "border.default",
        "color",
        "decorative outlines and separators",
    ),
    sys("border.strong", "color", "outlines that must be seen"),
    sys(
        "accent.default",
        "color",
        "primary actions, checked states, links",
    ),
    sys("accent.hover", "color", "the accent under the pointer"),
    sys("accent.active", "color", "the accent being pressed"),
    sys("accent.subtle", "color", "a quiet accent background"),
    sys("danger", "color", "errors and destructive actions"),
    sys("warning", "color", "warnings"),
    sys("success", "color", "success"),
    sys("info", "color", "information"),
    sys("focus.ring", "color", "the keyboard focus outline"),
    sys("selection", "color", "selected text and items"),
    sys("radius.widget", "dimension", "corner radius of controls"),
    sys(
        "radius.window",
        "dimension",
        "corner radius of windows and menus",
    ),
    sys("space.xs", "dimension", "hairline gaps"),
    sys("space.s", "dimension", "inside controls"),
    sys("space.m", "dimension", "between items"),
    sys("space.l", "dimension", "padding of windows and dialogs"),
    sys("space.xl", "dimension", "between sections"),
    sys(
        "size.control.height",
        "dimension",
        "height of one-line controls",
    ),
    sys(
        "size.icon.inline",
        "dimension",
        "icons in text and controls",
    ),
    sys("size.icon.toolbar", "dimension", "icons in toolbars"),
    sys("size.icon.panel", "dimension", "icons in panel strips"),
    sys("size.dialog.width", "dimension", "width of dialogs"),
    sys("size.toast.width", "dimension", "width of toasts"),
    sys("font.size.small", "dimension", "captions and small print"),
    sys("font.size.body", "dimension", "body text and buttons"),
    sys("font.size.heading", "dimension", "headings"),
    sys("font.size.mono", "dimension", "numbers and code"),
    sys("font.family.ui", "fontFamily", "interface text"),
    sys("font.family.mono", "fontFamily", "numbers and code"),
    sys(
        "motion.duration",
        "duration",
        "length of animations, 0 for none",
    ),
    comp("button.background", "color", "a button at rest"),
    comp(
        "button.background-hover",
        "color",
        "a button under the pointer",
    ),
    comp(
        "button.background-active",
        "color",
        "a button being pressed",
    ),
    comp("button.text", "color", "a button's label"),
    comp("button.border", "color", "a button's outline"),
    comp("button.radius", "dimension", "a button's corners"),
    comp("slider.track", "color", "the slider's rail"),
    comp("slider.fill", "color", "the rail up to the value"),
    comp("slider.thumb", "color", "the handle"),
    comp(
        "menu.item.background-hover",
        "color",
        "a menu item under the pointer",
    ),
    comp("menu.item.text", "color", "a menu item's label"),
    comp("menu.item.text-muted", "color", "a menu item's shortcut"),
    comp("dialog.background", "color", "a dialog's fill"),
    comp("dialog.border", "color", "a dialog's outline"),
    comp("dialog.radius", "dimension", "a dialog's corners"),
    comp("dialog.padding", "dimension", "a dialog's padding"),
    comp("dialog.width", "dimension", "a dialog's width"),
    comp("toast.background", "color", "a toast's fill"),
    comp("toast.border", "color", "a toast's outline"),
    comp("toast.text", "color", "a toast's text"),
    comp("toast.padding", "dimension", "a toast's padding"),
    comp("toast.width", "dimension", "a toast's width"),
];

/// The contract entry for a path.
pub fn required(path: &str) -> Option<&'static Required> {
    CONTRACT.iter().find(|r| r.path == path)
}
