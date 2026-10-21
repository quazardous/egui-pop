//! The contract: the system and component tokens every theme provides.

/// The tier of a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// The palette: colour scales and raw values, named by what they are
    /// (`gray.4`, `gold.9`). Widgets never read them.
    Reference,
    /// Roles: what a value is for (`surface.panel`, `text.muted`).
    System,
    /// One component's tokens (`slider-fill.rest.fill`), aliases of system tokens.
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

pub(crate) const fn comp(
    path: &'static str,
    ty: &'static str,
    description: &'static str,
) -> Required {
    Required {
        path,
        ty,
        tier: Tier::Component,
        description,
    }
}

/// The system tier and the components outside kits ([`contract`] adds
/// the kit's).
const CORE: &[Required] = &[
    sys(
        "transparent",
        "color",
        "nothing painted (a part that shows what is behind it)",
    ),
    sys(
        "scrim",
        "color",
        "what a backdrop dims the interface with (at opacity.scrim)",
    ),
    sys(
        "shadow.popup.offset-x",
        "dimension",
        "a menu's or popover's shadow: offset to the right",
    ),
    sys(
        "shadow.popup.offset-y",
        "dimension",
        "a menu's or popover's shadow: offset down",
    ),
    sys(
        "shadow.popup.blur",
        "dimension",
        "a menu's or popover's shadow: penumbra width",
    ),
    sys(
        "shadow.popup.spread",
        "dimension",
        "a menu's or popover's shadow: growth on every side",
    ),
    sys(
        "shadow.popup.color",
        "color",
        "a menu's or popover's shadow: colour",
    ),
    sys(
        "shadow.window.offset-x",
        "dimension",
        "a dialog's or palette's shadow: offset to the right",
    ),
    sys(
        "shadow.window.offset-y",
        "dimension",
        "a dialog's or palette's shadow: offset down",
    ),
    sys(
        "shadow.window.blur",
        "dimension",
        "a dialog's or palette's shadow: penumbra width",
    ),
    sys(
        "shadow.window.spread",
        "dimension",
        "a dialog's or palette's shadow: growth on every side",
    ),
    sys(
        "shadow.window.color",
        "color",
        "a dialog's or palette's shadow: colour",
    ),
    sys(
        "shadow.side.offset-x",
        "dimension",
        "the shadow of a side area lying over the centre: offset towards the centre",
    ),
    sys(
        "shadow.side.offset-y",
        "dimension",
        "the shadow of a side area lying over the centre: offset down",
    ),
    sys(
        "shadow.side.blur",
        "dimension",
        "the shadow of a side area lying over the centre: penumbra width",
    ),
    sys(
        "shadow.side.spread",
        "dimension",
        "the shadow of a side area lying over the centre: growth on every side",
    ),
    sys(
        "shadow.side.color",
        "color",
        "the shadow of a side area lying over the centre: colour",
    ),
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
        "space.inline",
        "dimension",
        "by intent: between items on one line",
    ),
    sys(
        "space.group",
        "dimension",
        "by intent: between related groups",
    ),
    sys("space.section", "dimension", "by intent: before a section"),
    sys(
        "space.page",
        "dimension",
        "by intent: around a page's content",
    ),
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
        "size.title-bar.height",
        "dimension",
        "height of a window's own title bar",
    ),
    sys(
        "size.window-button.width",
        "dimension",
        "width of a title bar's window buttons",
    ),
    sys(
        "size.frame.grip",
        "dimension",
        "width of a window's own resize edges",
    ),
    sys(
        "font.size.xs",
        "dimension",
        "the size scale's smallest step",
    ),
    sys("font.size.s", "dimension", "the size scale's small step"),
    sys(
        "font.size.m",
        "dimension",
        "the size scale's middle step (body text)",
    ),
    sys("font.size.l", "dimension", "the size scale's large step"),
    sys("font.size.xl", "dimension", "the size scale's largest step"),
    sys(
        "font.text-style.small",
        "dimension",
        "egui's small text: captions and small print",
    ),
    sys("font.text-style.body", "dimension", "egui's body text"),
    sys(
        "font.text-style.button",
        "dimension",
        "egui's button labels",
    ),
    sys("font.text-style.heading", "dimension", "egui's headings"),
    sys(
        "font.text-style.monospace",
        "dimension",
        "egui's monospace text: numbers and code",
    ),
    sys("text-role.title.size", "dimension", "a title: its size"),
    sys("text-role.title.color", "color", "a title: its colour"),
    sys("text-role.title.weight", "number", "a title: its weight"),
    sys(
        "text-role.section.size",
        "dimension",
        "a section's title: its size",
    ),
    sys(
        "text-role.section.color",
        "color",
        "a section's title: its colour",
    ),
    sys(
        "text-role.section.weight",
        "number",
        "a section's title: its weight",
    ),
    sys(
        "text-role.body.size",
        "dimension",
        "ordinary text: its size",
    ),
    sys("text-role.body.color", "color", "ordinary text: its colour"),
    sys(
        "text-role.body.weight",
        "number",
        "ordinary text: its weight",
    ),
    sys(
        "text-role.muted.size",
        "dimension",
        "secondary text: its size",
    ),
    sys(
        "text-role.muted.color",
        "color",
        "secondary text: its colour",
    ),
    sys(
        "text-role.muted.weight",
        "number",
        "secondary text: its weight",
    ),
    sys(
        "text-role.meta.size",
        "dimension",
        "small print about an item: its size",
    ),
    sys(
        "text-role.meta.color",
        "color",
        "small print about an item: its colour",
    ),
    sys(
        "text-role.meta.weight",
        "number",
        "small print about an item: its weight",
    ),
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
    sys(
        "motion.pulse.period",
        "duration",
        "one pulse of a busy LED or of a mark calling attention",
    ),
    sys(
        "motion.pulse.count",
        "number",
        "how many times a mark calling attention pulses",
    ),
    comp(
        "palette.magnet",
        "dimension",
        "how close a palette snaps to an edge or another palette",
    ),
    comp(
        "palette.group.gap",
        "dimension",
        "the room between the palettes stacked in a group",
    ),
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
    comp(
        "cursor.fill",
        "color",
        "a tool cursor's strokes, a brush outline",
    ),
    comp(
        "cursor.outline",
        "color",
        "the ring around a cursor's strokes",
    ),
    comp(
        "loupe.background",
        "color",
        "around the loupe's picture, behind its readout",
    ),
    comp("loupe.frame", "color", "the loupe's outline"),
    comp("loupe.grid", "color", "the loupe's pixel grid"),
    comp("loupe.marker", "color", "the loupe's centre pixel"),
    comp("graph.background", "color", "the node graph's canvas"),
    comp("graph.grid", "color", "the node graph's grid lines"),
    comp("graph.node", "color", "a node's body"),
    comp("graph.node-header", "color", "a node's title bar"),
    comp("graph.node-border", "color", "a node's outline"),
    comp("graph.node-selected", "color", "a selected node's outline"),
    comp("graph.wire", "color", "a wire whose type has no colour"),
    comp("graph.wire-preview", "color", "the wire being drawn"),
    comp("graph.wire-refused", "color", "a wire a port refuses"),
    comp("graph.group", "color", "behind a group of nodes"),
    comp("graph.group-border", "color", "a group's outline"),
    comp("graph.band", "color", "the rubber band selecting nodes"),
    comp("graph.type-1", "color", "a port type's colour (1 of 6)"),
    comp("graph.type-2", "color", "a port type's colour (2 of 6)"),
    comp("graph.type-3", "color", "a port type's colour (3 of 6)"),
    comp("graph.type-4", "color", "a port type's colour (4 of 6)"),
    comp("graph.type-5", "color", "a port type's colour (5 of 6)"),
    comp("graph.type-6", "color", "a port type's colour (6 of 6)"),
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
];

/// The version of the contract: which tokens exist, under which names.
///
/// A number of its own, not a crate version: it starts at 1 and goes up
/// by one when a release renames or removes a token of the contract (an
/// added token does not raise it, since an older file still loads). A
/// token file says which contract it was written for in its format
/// stamp ([`crate::FormatStamp`]); a rename comes with an entry in
/// [`RENAMED`], so files written for an older contract still load.
pub const CONTRACT_VERSION: u64 = 1;

/// A token renamed in the contract: `from` is read as `to` in a file
/// written for a contract older than `contract`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Renamed {
    /// The former name.
    pub from: &'static str,
    /// The name now.
    pub to: &'static str,
    /// The contract version that dropped `from` (the [`CONTRACT_VERSION`]
    /// of the release renaming it).
    pub contract: u64,
    /// The release of `egui-pop-tokens` renaming it, `"major.minor"`
    /// (`"0.3"`): the entry stays for [`RENAME_WINDOW`] minor releases
    /// after it ([`Renamed::expired_at`]).
    pub since: &'static str,
}

/// How many minor releases after the one renaming a token its former
/// name is still read: renamed in 0.N, read (with the "update your file"
/// warning) in 0.N, 0.N+1 and 0.N+2, its entry dropped in 0.N+3. After
/// that, the former name loads as a token the base theme does not have
/// (a warning). A test fails when an entry of [`RENAMED`] outlives it.
pub const RENAME_WINDOW: u64 = 2;

impl Renamed {
    /// Whether this entry has outlived its window ([`RENAME_WINDOW`]) in
    /// the release `version` (`"major.minor"`, a patch and a pre-release
    /// ignored): more than two minor releases after [`Renamed::since`].
    /// A major release counts as one more minor release (1.0 follows
    /// 0.N). `None` when either version cannot be read.
    pub fn expired_at(&self, version: &str) -> Option<bool> {
        let (since_major, since_minor) = major_minor(self.since)?;
        let (major, minor) = major_minor(version)?;
        let releases = if major == since_major {
            minor.checked_sub(since_minor)?
        } else if major > since_major {
            // Every major in between counts once, then the new major's
            // minors.
            (major - since_major) + minor
        } else {
            return None;
        };
        Some(releases > RENAME_WINDOW)
    }
}

/// `"1.4"`, `"1.4.2"` or `"1.4.2-rc.1"` as `(1, 4)`.
fn major_minor(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.split(['.', '-', '+']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// The renames between contract versions, oldest first, applied when a
/// file stamped with an older contract is loaded: each former name is
/// read under its new one, with a warning saying to update the file.
/// Each entry stays for [`RENAME_WINDOW`] minor releases after the one
/// that added it. Empty while the contract is at its first version.
pub const RENAMED: &[Renamed] = &[];

mod kit;

/// Every token a theme must provide: the system tier, the component tier
/// and the default kit's components. A theme missing one, or giving one
/// of another type, gets the built-in `pop-dark` value for it and a
/// reported problem.
pub fn contract() -> impl Iterator<Item = &'static Required> {
    CORE.iter().chain(kit::GROUPS.iter().flat_map(|g| g.iter()))
}

/// The default kit's components' part of the [`contract`] (`tab.*`,
/// `title-bar.*`...): their first path segment names the components a kit
/// can describe.
pub fn kit_contract() -> impl Iterator<Item = &'static Required> {
    kit::GROUPS.iter().flat_map(|g| g.iter())
}

/// The contract entry for a path.
pub fn required(path: &str) -> Option<&'static Required> {
    contract().find(|r| r.path == path)
}
