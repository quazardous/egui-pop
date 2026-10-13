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
    sys(
        "size.magnet",
        "dimension",
        "how close a dragged palette snaps to an edge",
    ),
    sys(
        "opacity.scrim",
        "number",
        "how much a backdrop behind a modal hides (0 to 1)",
    ),
    sys(
        "size.titlebar.height",
        "dimension",
        "height of a window's own title bar",
    ),
    sys(
        "size.titlebar.button",
        "dimension",
        "width of a title bar's window buttons",
    ),
    sys(
        "size.frame.grip",
        "dimension",
        "width of a window's own resize edges",
    ),
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
    sys(
        "motion.spinner.delay",
        "duration",
        "how long work runs before a busy indicator shows",
    ),
    sys(
        "motion.spinner.min",
        "duration",
        "how long a busy indicator stays once shown",
    ),
    sys("motion.spinner.turn", "duration", "one turn of a spinner"),
    sys(
        "motion.progress.travel",
        "duration",
        "an indeterminate bar's segment crossing its track",
    ),
    sys(
        "motion.skeleton.shimmer",
        "duration",
        "a skeleton's shimmer crossing it",
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
    comp("dialog.header", "color", "a movable dialog's header band"),
    comp("palette.background", "color", "a palette's fill"),
    comp("palette.border", "color", "a palette's outline"),
    comp("palette.header", "color", "a palette's header band"),
    comp(
        "palette.header-text",
        "color",
        "the title on a palette's header band",
    ),
    comp("palette.padding", "dimension", "a palette's padding"),
    comp(
        "palette.magnet",
        "dimension",
        "how close a palette snaps to an edge or another palette",
    ),
    comp(
        "dialog.header-text",
        "color",
        "the title on a movable dialog's header band",
    ),
    comp(
        "dialog.backdrop",
        "number",
        "the backdrop's opacity behind a modal dialog",
    ),
    comp("toast.background", "color", "a toast's fill"),
    comp("toast.border", "color", "a toast's outline"),
    comp("toast.text", "color", "a toast's text"),
    comp("toast.padding", "dimension", "a toast's padding"),
    comp("toast.width", "dimension", "a toast's width"),
    comp(
        "viewport.background",
        "color",
        "around the image in a viewport",
    ),
    comp("viewport.backdrop", "color", "under an image not drawn yet"),
    comp("viewport.frame", "color", "the image's outline"),
    comp("viewport.grid", "color", "the pixel grid at high zoom"),
    comp("viewport.guide", "color", "overlay shapes and guides"),
    comp("viewport.handle", "color", "a handle at rest"),
    comp(
        "viewport.handle-hover",
        "color",
        "a handle under the pointer",
    ),
    comp(
        "viewport.handle-outline",
        "color",
        "the ring around a handle",
    ),
    comp(
        "thumbnail.background",
        "color",
        "behind a thumbnail's image",
    ),
    comp("thumbnail.hover", "color", "a thumbnail under the pointer"),
    comp(
        "thumbnail.selected",
        "color",
        "a selected thumbnail's outline",
    ),
    comp(
        "histogram.background",
        "color",
        "behind a histogram's curves",
    ),
    comp("histogram.grid", "color", "a histogram's quarter lines"),
    comp(
        "histogram.clip-shadows",
        "color",
        "the marker of clipped shadows",
    ),
    comp(
        "histogram.clip-highlights",
        "color",
        "the marker of clipped highlights",
    ),
    comp("curve.background", "color", "behind a curve editor's graph"),
    comp("curve.grid", "color", "a curve editor's quarter lines"),
    comp(
        "curve.identity",
        "color",
        "the diagonal where output equals input",
    ),
    comp("curve.line", "color", "the curve"),
    comp("curve.point", "color", "a curve's point"),
    comp(
        "curve.point-selected",
        "color",
        "the selected or picked point",
    ),
    comp("curve.pick", "color", "the value picked under the pointer"),
    comp("wheel.ring", "color", "a colour wheel's outline"),
    comp("wheel.puck", "color", "a colour wheel's puck"),
    comp(
        "wheel.puck-outline",
        "color",
        "the ring around a wheel's puck",
    ),
    comp(
        "navigator.background",
        "color",
        "behind a navigator's image",
    ),
    comp("navigator.frame", "color", "the part of the image in view"),
    comp(
        "navigator.veil",
        "color",
        "over the part of the image out of view",
    ),
    comp("editor.background", "color", "behind an editor's text"),
    comp("editor.gutter", "color", "behind an editor's line numbers"),
    comp("editor.line-number", "color", "a line number"),
    comp(
        "editor.line-number-active",
        "color",
        "the cursor line's number",
    ),
    comp(
        "editor.current-line",
        "color",
        "the cursor line's background",
    ),
    comp("editor.selection", "color", "selected text's background"),
    comp("editor.cursor", "color", "the text cursor"),
    comp("editor.indent-guide", "color", "indentation guides"),
    comp(
        "editor.bracket",
        "color",
        "a matching bracket pair's outline",
    ),
    comp("editor.match", "color", "a search match's background"),
    comp(
        "editor.match-current",
        "color",
        "the current search match's outline",
    ),
    comp("editor.fold", "color", "fold arrows"),
    comp("editor.breakpoint", "color", "a breakpoint marker"),
    comp("editor.error", "color", "an error marker"),
    comp("editor.warning", "color", "a warning marker"),
    comp("editor.info", "color", "an information marker"),
    comp("editor.diff-added", "color", "a line added"),
    comp("editor.diff-modified", "color", "a line changed"),
    comp("editor.diff-removed", "color", "lines removed"),
    comp("syntax.plain", "color", "code with no kind"),
    comp("syntax.keyword", "color", "keywords"),
    comp("syntax.string", "color", "string literals"),
    comp("syntax.number", "color", "numeric literals"),
    comp("syntax.constant", "color", "other constants"),
    comp("syntax.escape", "color", "escapes in strings"),
    comp("syntax.comment", "color", "comments"),
    comp("syntax.function", "color", "function names"),
    comp("syntax.type", "color", "type names"),
    comp("syntax.macro", "color", "macros and attributes"),
    comp("syntax.tag", "color", "markup tags"),
    comp("syntax.operator", "color", "operators"),
    comp("syntax.punctuation", "color", "punctuation"),
    comp("syntax.invalid", "color", "text the grammar rejects"),
    comp("terminal.background", "color", "behind a terminal's text"),
    comp(
        "terminal.foreground",
        "color",
        "a terminal's text in the default colour",
    ),
    comp(
        "terminal.bold",
        "color",
        "a terminal's bold text in the default colour",
    ),
    comp("terminal.cursor", "color", "a terminal's cursor"),
    comp(
        "terminal.selection",
        "color",
        "a terminal's selected text's background",
    ),
    comp("terminal.link", "color", "a link's underline in a terminal"),
    comp("terminal.ansi.0", "color", "ANSI colour 0 (black)"),
    comp("terminal.ansi.1", "color", "ANSI colour 1 (red)"),
    comp("terminal.ansi.2", "color", "ANSI colour 2 (green)"),
    comp("terminal.ansi.3", "color", "ANSI colour 3 (yellow)"),
    comp("terminal.ansi.4", "color", "ANSI colour 4 (blue)"),
    comp("terminal.ansi.5", "color", "ANSI colour 5 (magenta)"),
    comp("terminal.ansi.6", "color", "ANSI colour 6 (cyan)"),
    comp("terminal.ansi.7", "color", "ANSI colour 7 (white)"),
    comp("terminal.ansi.8", "color", "ANSI colour 8 (bright black)"),
    comp("terminal.ansi.9", "color", "ANSI colour 9 (bright red)"),
    comp("terminal.ansi.10", "color", "ANSI colour 10 (bright green)"),
    comp(
        "terminal.ansi.11",
        "color",
        "ANSI colour 11 (bright yellow)",
    ),
    comp("terminal.ansi.12", "color", "ANSI colour 12 (bright blue)"),
    comp(
        "terminal.ansi.13",
        "color",
        "ANSI colour 13 (bright magenta)",
    ),
    comp("terminal.ansi.14", "color", "ANSI colour 14 (bright cyan)"),
    comp("terminal.ansi.15", "color", "ANSI colour 15 (bright white)"),
    comp("titlebar.background", "color", "a window's own title bar"),
    comp(
        "titlebar.text",
        "color",
        "the title, a button's glyph under the pointer",
    ),
    comp(
        "titlebar.text-muted",
        "color",
        "a window button's glyph at rest",
    ),
    comp(
        "titlebar.button-hover",
        "color",
        "a window button under the pointer",
    ),
    comp(
        "titlebar.close-hover",
        "color",
        "the close button under the pointer",
    ),
    comp(
        "titlebar.close-text",
        "color",
        "the close button's glyph under the pointer",
    ),
    comp("titlebar.height", "dimension", "the title bar's height"),
    comp(
        "titlebar.button-width",
        "dimension",
        "a window button's width",
    ),
    comp("titlebar.grip", "dimension", "the resize edges' width"),
];

/// The contract entry for a path.
pub fn required(path: &str) -> Option<&'static Required> {
    CONTRACT.iter().find(|r| r.path == path)
}
