//! The mount contract (ADR 0013): a component says what it needs
//! ([`Mountable`]), a host says what it offers ([`HostOffer`]), and
//! [`check`] matches the two with eleven named rules ([`Rule`]).
//!
//! A pair either fits, fits under named conditions, or does not fit,
//! with a reason of one kind: by design (a rule kept on purpose, with
//! what to do instead), an egui-pop limit (work not done yet), or an
//! upstream limit (egui, eframe or another dependency). Each condition
//! and reason is a [`Code`] whose text lives once, in [`CODES`]: the
//! generated `docs/hosts.md`, the reported refusals and the debug
//! bridge all show that text.
//!
//! ```
//! use egui_pop_model::mount::{self, Fit, HostOffer, HostSize, Mountable};
//!
//! let viewport = Mountable::new("viewport", "Image viewport").fills();
//! let status_bar = HostOffer::new("status-bar", "S", "Status bar").size(HostSize::Row);
//! match mount::check(&viewport, &status_bar) {
//!     Fit::No { refusal, .. } => assert_eq!(refusal.code, "D1"),
//!     other => panic!("{other:?}"),
//! }
//! ```

use serde::Serialize;

pub mod page;

/// The height of a line host (a bar, the status bar), in points: a
/// component needing more does not fit there.
pub const ROW_HEIGHT: f32 = 32.0;

/// How a component uses the keyboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Keyboard {
    /// Never.
    None,
    /// While the pointer is over it (Space to pan, the loupe).
    Hover,
    /// While it has the focus.
    #[default]
    Focus,
    /// Every key while it has the focus (a terminal).
    Exclusive,
}

/// A key a component may claim, and a host may own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKey {
    Enter,
    Escape,
    Space,
    Tab,
}

impl ClaimKey {
    /// As a key is written in a chord.
    pub fn name(self) -> &'static str {
        match self {
            ClaimKey::Enter => "Enter",
            ClaimKey::Escape => "Escape",
            ClaimKey::Space => "Space",
            ClaimKey::Tab => "Tab",
        }
    }
}

/// What a component does with the pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Pointer {
    /// Drags inside itself.
    pub drag: bool,
    /// Drags out of itself (into another host).
    pub drag_out: bool,
    pub wheel: bool,
    /// Bitmap cursors of its own (tool cursors).
    pub cursor_images: bool,
}

/// Work done off the interface thread for a component.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Workers {
    #[default]
    None,
    /// A producer the application runs (tiles, thumbnails, a source),
    /// posting to a queue drained when the component is drawn.
    DrainedWhenDrawn,
}

/// The colours a component needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Display {
    #[default]
    Srgb,
    /// Wide gamut or HDR when there is one; sRGB otherwise, with the
    /// reason.
    WideWanted,
    /// Wide gamut or HDR, or nothing (a mastering view).
    WideRequired,
}

/// The size a component needs, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct SizeNeed {
    /// The smallest width and height it can be drawn in.
    pub min: [f32; 2],
    pub preferred: Option<[f32; 2]>,
    pub max: Option<[f32; 2]>,
    /// It takes all the space it is given.
    pub fills: bool,
    /// Width over height, when it keeps one.
    pub aspect: Option<f32>,
}

/// How many of a component an application may show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Instances {
    /// As many as the caller has states for.
    #[default]
    Several,
    /// One state per application, held by a service: two mounts show
    /// the same thing.
    Single,
}

/// What a component needs from the place it is mounted in.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Mountable {
    /// A stable id (`viewport`, `views.tree`).
    pub id: String,
    /// As people call it.
    pub name: String,
    /// The crate it comes from.
    pub from: String,
    pub keyboard: Keyboard,
    /// Keys it uses that a host may own (Enter, Escape...).
    pub claims: Vec<ClaimKey>,
    /// Bindings of the keymap act on it (its surface's or its commands').
    pub keymap: bool,
    /// Its keymap surface, if it has one.
    pub surface: Option<String>,
    pub pointer: Pointer,
    pub workers: Workers,
    /// Its workers must keep delivering while another surface is modal
    /// (they do: workers are the application's threads).
    pub keep_under_modal: bool,
    pub display: Display,
    pub size: SizeNeed,
    pub instances: Instances,
    /// A key its view state (zoom, scroll, selection) is kept under by
    /// the host; `None`: the caller keeps it.
    pub view_state: Option<String>,
    /// It needs the application (an `AppHandle`: services, steps).
    pub app: bool,
    /// Its accessibility role, as a word (`image`, `tree`, `slider`).
    pub role: String,
    /// It is a question that must be answered before going on.
    pub asks: bool,
    /// It holds a session (unsaved text, a running program, a draft)
    /// that must not end by accident.
    pub session: bool,
    /// It opens egui areas of its own (popups, context menus,
    /// completion lists).
    pub popups: bool,
    /// It can be drawn non-interactive (a preview, a thumbnail).
    pub preview: bool,
    /// It can be drawn as a value-descriptor field of a form.
    pub descriptor_field: bool,
}

impl Mountable {
    /// A plain widget: the keyboard when focused, no claims, several
    /// instances, no workers, sRGB, any size.
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            role: "group".into(),
            ..Self::default()
        }
    }

    /// The crate it comes from.
    pub fn crate_name(mut self, name: &str) -> Self {
        self.from = name.into();
        self
    }

    pub fn keyboard(mut self, keyboard: Keyboard) -> Self {
        self.keyboard = keyboard;
        self
    }

    /// Keys it takes for itself.
    pub fn claims(mut self, keys: &[ClaimKey]) -> Self {
        for k in keys {
            if !self.claims.contains(k) {
                self.claims.push(*k);
            }
        }
        self
    }

    /// Bindings of the keymap act on it.
    pub fn keymap(mut self) -> Self {
        self.keymap = true;
        self
    }

    /// Its keymap surface (bindings act on it).
    pub fn surface(mut self, name: &str) -> Self {
        self.surface = Some(name.into());
        self.keymap = true;
        self
    }

    pub fn drag(mut self) -> Self {
        self.pointer.drag = true;
        self
    }

    pub fn drag_out(mut self) -> Self {
        self.pointer.drag_out = true;
        self
    }

    pub fn wheel(mut self) -> Self {
        self.pointer.wheel = true;
        self
    }

    pub fn cursor_images(mut self) -> Self {
        self.pointer.cursor_images = true;
        self
    }

    /// Fed by workers, drained when drawn.
    pub fn workers(mut self) -> Self {
        self.workers = Workers::DrainedWhenDrawn;
        self
    }

    /// Its workers must keep delivering under a modal.
    pub fn keep_under_modal(mut self) -> Self {
        self.keep_under_modal = true;
        self
    }

    pub fn display(mut self, display: Display) -> Self {
        self.display = display;
        self
    }

    /// The smallest size, in points.
    pub fn min(mut self, width: f32, height: f32) -> Self {
        self.size.min = [width.max(0.0), height.max(0.0)];
        self
    }

    pub fn preferred(mut self, width: f32, height: f32) -> Self {
        self.size.preferred = Some([width, height]);
        self
    }

    pub fn max(mut self, width: f32, height: f32) -> Self {
        self.size.max = Some([width, height]);
        self
    }

    /// Takes all the space it is given.
    pub fn fills(mut self) -> Self {
        self.size.fills = true;
        self
    }

    /// Keeps a width over height ratio.
    pub fn aspect(mut self, ratio: f32) -> Self {
        self.size.aspect = Some(ratio);
        self
    }

    /// One state per application.
    pub fn single(mut self) -> Self {
        self.instances = Instances::Single;
        self
    }

    /// Its view state is kept by the host under `key`.
    pub fn view_state(mut self, key: &str) -> Self {
        self.view_state = Some(key.into());
        self
    }

    /// Needs the application.
    pub fn needs_app(mut self) -> Self {
        self.app = true;
        self
    }

    pub fn role(mut self, role: &str) -> Self {
        self.role = role.into();
        self
    }

    /// A question to answer before going on.
    pub fn asks(mut self) -> Self {
        self.asks = true;
        self
    }

    /// Holds a session that must not end by accident.
    pub fn session(mut self) -> Self {
        self.session = true;
        self
    }

    /// Opens egui areas of its own.
    pub fn popups(mut self) -> Self {
        self.popups = true;
        self
    }

    /// Can be drawn non-interactive.
    pub fn preview(mut self) -> Self {
        self.preview = true;
        self
    }

    /// Can be a form's field.
    pub fn descriptor_field(mut self) -> Self {
        self.descriptor_field = true;
        self
    }

    /// Whether it claims a key.
    pub fn claims_key(&self, key: ClaimKey) -> bool {
        self.claims.contains(&key)
    }
}

/// The room a host gives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostSize {
    /// A whole rect (an area, a window).
    #[default]
    Fill,
    /// A size given by the host's own declaration, not by the content
    /// (a custom dialog's size).
    Given,
    /// A bounded width, the height of the content.
    Content,
    /// One row.
    Row,
    /// No `Ui` at all: data only.
    NoUi,
}

/// Who gets the keyboard in a host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusPolicy {
    /// Shared with the rest of the interface, by focus.
    #[default]
    Shared,
    /// Only once clicked into (a palette).
    ClickInto,
    /// Everything, while it is open (a modal).
    ModalExclusive,
    None,
}

/// What closes a transient host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Dismiss {
    Answer,
    Escape,
    OutsideClick,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifetime {
    #[default]
    Persistent,
    Transient,
}

/// Which window a host is in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Window {
    #[default]
    Main,
    Auxiliary,
    /// Where its panel is.
    FollowsPanel,
}

/// What a host keeps across showings and restarts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Keep {
    Place,
    Size,
    ViewValues,
}

/// The colours a host can show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostDisplay {
    #[default]
    Srgb,
    /// Wide gamut and HDR, through the image's own surface.
    Wide,
}

/// What a host offers what is mounted in it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct HostOffer {
    /// A stable id (`panel`, `dialog.custom`).
    pub id: String,
    /// Its column in `docs/hosts.md` (`P`, `F`...).
    pub code: String,
    /// As people call it.
    pub name: String,
    /// Where it is in the code.
    pub source: String,
    pub size: HostSize,
    /// The user can resize it.
    pub resizable: bool,
    pub focus: FocusPolicy,
    /// Keys the host takes before the content (a modal's Enter and
    /// Escape).
    pub owns_keys: Vec<ClaimKey>,
    /// A content claiming a key the host owns gets it (a custom dialog:
    /// Ctrl+Enter accepts when the content claims Enter).
    pub honours_claims: bool,
    pub dismissed_by: Vec<Dismiss>,
    pub lifetime: Lifetime,
    pub window: Window,
    /// The key dispatcher reaches it.
    pub keymap: bool,
    /// Part of the context stack's projection (ADR 0012).
    pub projection: bool,
    pub keeps: Vec<Keep>,
    /// Gives its content a `Ui`.
    pub gives_ui: bool,
    /// Gives its content the application (an `AppHandle`).
    pub gives_app: bool,
    /// Draws its content in a focus scope of its own.
    pub scope: bool,
    /// Draws its content inside a fault boundary.
    pub fault_boundary: bool,
    pub display: HostDisplay,
    /// Bitmap cursors are shown in it.
    pub cursor_images: bool,
    /// Takes any content (not only value-descriptor fields).
    pub free_content: bool,
    /// Its content may stop being drawn while it lives (a tab behind
    /// another, a rolled-up palette).
    pub hides_content: bool,
    /// It can be mounted in a headless test (kittest).
    pub headless: bool,
}

impl HostOffer {
    /// A host filling a rect in the main window, shared focus,
    /// persistent, with the keymap, a `Ui`, the application, a scope and
    /// a fault boundary, sRGB, any content, testable headless.
    pub fn new(id: &str, code: &str, name: &str) -> Self {
        Self {
            id: id.into(),
            code: code.into(),
            name: name.into(),
            keymap: true,
            gives_ui: true,
            gives_app: true,
            scope: true,
            fault_boundary: true,
            cursor_images: true,
            free_content: true,
            headless: true,
            ..Self::default()
        }
    }

    /// Where it is in the code.
    pub fn source(mut self, source: &str) -> Self {
        self.source = source.into();
        self
    }

    pub fn size(mut self, size: HostSize) -> Self {
        self.size = size;
        if size == HostSize::NoUi {
            self.gives_ui = false;
        }
        self
    }

    pub fn resizable(mut self, on: bool) -> Self {
        self.resizable = on;
        self
    }

    pub fn focus(mut self, focus: FocusPolicy) -> Self {
        self.focus = focus;
        self
    }

    /// Keys taken before the content.
    pub fn owns_keys(mut self, keys: &[ClaimKey]) -> Self {
        self.owns_keys = keys.to_vec();
        self
    }

    /// A content claiming a key the host owns gets it.
    pub fn honours_claims(mut self) -> Self {
        self.honours_claims = true;
        self
    }

    pub fn dismissed_by(mut self, by: &[Dismiss]) -> Self {
        self.dismissed_by = by.to_vec();
        self
    }

    pub fn lifetime(mut self, lifetime: Lifetime) -> Self {
        self.lifetime = lifetime;
        self
    }

    pub fn window(mut self, window: Window) -> Self {
        self.window = window;
        self
    }

    pub fn keymap(mut self, on: bool) -> Self {
        self.keymap = on;
        self
    }

    pub fn projection(mut self, on: bool) -> Self {
        self.projection = on;
        self
    }

    pub fn keeps(mut self, keeps: &[Keep]) -> Self {
        self.keeps = keeps.to_vec();
        self
    }

    pub fn gives_ui(mut self, on: bool) -> Self {
        self.gives_ui = on;
        self
    }

    pub fn gives_app(mut self, on: bool) -> Self {
        self.gives_app = on;
        self
    }

    pub fn scope(mut self, on: bool) -> Self {
        self.scope = on;
        self
    }

    pub fn fault_boundary(mut self, on: bool) -> Self {
        self.fault_boundary = on;
        self
    }

    pub fn display(mut self, display: HostDisplay) -> Self {
        self.display = display;
        self
    }

    pub fn cursor_images(mut self, on: bool) -> Self {
        self.cursor_images = on;
        self
    }

    pub fn free_content(mut self, on: bool) -> Self {
        self.free_content = on;
        self
    }

    pub fn hides_content(mut self, on: bool) -> Self {
        self.hides_content = on;
        self
    }

    pub fn headless(mut self, on: bool) -> Self {
        self.headless = on;
        self
    }

    /// Whether it is modal.
    pub fn is_modal(&self) -> bool {
        self.focus == FocusPolicy::ModalExclusive
    }
}

/// The matching rules, in the order they are checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// R1: the size needed against the room given.
    Size,
    /// R2: the keyboard, claimed keys and the keymap against the host's
    /// focus policy, owned keys and keymap reach.
    Keyboard,
    /// R3: sessions and popups against the host's lifetime and what
    /// dismisses it.
    Session,
    /// R4: a question against a host that is not modal.
    Question,
    /// R5: colours and cursors against the host's display.
    Display,
    /// R6: one state per application, mounted where another may be.
    Instances,
    /// R7: the application against a host that gives none.
    Application,
    /// R8: any component against a host with no `Ui`.
    Ui,
    /// R9: workers against a host that hides its content.
    Workers,
    /// R10: a view-state key against a host that keeps nothing.
    ViewState,
    /// R11: free content against a dialog of the four kinds.
    DialogContent,
}

impl Rule {
    pub const ALL: [Rule; 11] = [
        Rule::Size,
        Rule::Keyboard,
        Rule::Session,
        Rule::Question,
        Rule::Display,
        Rule::Instances,
        Rule::Application,
        Rule::Ui,
        Rule::Workers,
        Rule::ViewState,
        Rule::DialogContent,
    ];

    /// Its number, `R1` to `R11`.
    pub fn number(self) -> &'static str {
        match self {
            Rule::Size => "R1",
            Rule::Keyboard => "R2",
            Rule::Session => "R3",
            Rule::Question => "R4",
            Rule::Display => "R5",
            Rule::Instances => "R6",
            Rule::Application => "R7",
            Rule::Ui => "R8",
            Rule::Workers => "R9",
            Rule::ViewState => "R10",
            Rule::DialogContent => "R11",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Rule::Size => "size",
            Rule::Keyboard => "keyboard",
            Rule::Session => "sessions and lifetime",
            Rule::Question => "questions",
            Rule::Display => "display",
            Rule::Instances => "instances",
            Rule::Application => "application",
            Rule::Ui => "a Ui",
            Rule::Workers => "workers",
            Rule::ViewState => "view state",
            Rule::DialogContent => "dialog content",
        }
    }

    /// What it compares.
    pub fn compares(self) -> &'static str {
        match self {
            Rule::Size => {
                "The size needed (minimum, fills) against the room the host gives (a rect, a given size, the content's height, one row)."
            }
            Rule::Keyboard => {
                "The keyboard needed and the keys claimed against the host's focus policy and the keys it owns; bindings of the keymap against the host's keymap reach."
            }
            Rule::Session => {
                "A session (unsaved work, a running program) or popups of the component's own against a transient host and what dismisses it."
            }
            Rule::Question => "A question to answer against a host that is not modal.",
            Rule::Display => {
                "Wide gamut wanted or required, and bitmap cursors, against what the host's window can show."
            }
            Rule::Instances => {
                "One state per application against a host that may show it beside another mount."
            }
            Rule::Application => "A component needing the application against a host giving none.",
            Rule::Ui => "Any component against a host that gives no `Ui`.",
            Rule::Workers => {
                "Workers drained when the component is drawn against a host that can stop drawing it."
            }
            Rule::ViewState => {
                "A view-state key against a host that keeps no view values and is outside the projection."
            }
            Rule::DialogContent => {
                "Free content against a dialog of the four kinds, which takes value-descriptor fields only."
            }
        }
    }
}

/// The kind of a [`Code`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeKind {
    /// Fits, under this condition.
    Condition,
    /// Does not fit, by design.
    Design,
    /// Does not fit: egui-pop does not do it yet.
    Limit,
    /// Does not fit: egui, eframe or another dependency cannot do it.
    Upstream,
}

/// A condition or a reason, written once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Code {
    /// `c1`, `D1`, `E2`, `U2`...
    pub code: &'static str,
    pub kind: CodeKind,
    /// In a few words.
    pub title: &'static str,
    /// The condition, the rule, or the limit.
    pub text: &'static str,
    /// What to do instead (by design), the estimated cost (a limit), or
    /// the entry in `docs/upstream.md` (upstream).
    pub then: &'static str,
}

/// Every condition and reason.
pub const CODES: &[Code] = &[
    Code {
        code: "c1",
        kind: CodeKind::Condition,
        title: "Content-sized host",
        text: "The palette, the raw modal, the popover and the dialogs of the four kinds size themselves from their content: a component that fills the space it is given must be given an explicit size (`.size()`, a `ScrollArea` with a maximum height) or it grows to the window. The user cannot resize a palette yet (E2).",
        then: "",
    },
    Code {
        code: "c2",
        kind: CodeKind::Condition,
        title: "No keymap in this window",
        text: "The key dispatcher reads the main window's input only, so bindings of the keymap (global and surface ones: an editor's find and save, the terminal's clear, the library's commands) do not fire while an auxiliary window has the keyboard. Keys a widget reads itself (arrows, zoom keys, typing) work. Gap E3.",
        then: "",
    },
    Code {
        code: "c3",
        kind: CodeKind::Condition,
        title: "sRGB and system cursors",
        text: "Wide gamut and HDR fall back to sRGB in this host, with the reason reported (U2); bitmap tool cursors fall back to their system cursor (U1).",
        then: "",
    },
    Code {
        code: "c4",
        kind: CodeKind::Condition,
        title: "The host takes its keys first",
        text: "The host owns keys the component claims: Escape closes a modal or a popover before any component sees it, Enter accepts a dialog (ADR 0007's Escape order). A component's own Escape (a graph's deselect, a terminal program's Escape) never arrives; Shift+Escape still leaves the terminal.",
        then: "",
    },
    Code {
        code: "c5",
        kind: CodeKind::Condition,
        title: "Popups inside a transient host",
        text: "The component opens egui areas of its own (completion lists, context menus, drop-downs) outside egui-pop's overlay stack; whether a click in them counts as a click outside the host (which closes a popover) is not checked.",
        then: "",
    },
    Code {
        code: "c6",
        kind: CodeKind::Condition,
        title: "One row",
        text: "A compact size given by the application: a slider at least 120 wide, a small thumbnail, a thin bar, within one row.",
        then: "",
    },
    Code {
        code: "c7",
        kind: CodeKind::Condition,
        title: "One state per application",
        text: "The component draws a service's state: two mounts (a panel and a popover) show the same jobs or the same draft, not two independent instances.",
        then: "",
    },
    Code {
        code: "c8",
        kind: CodeKind::Condition,
        title: "As a descriptor field only",
        text: "In a dialog of the four kinds, the component exists only as a `Form` field made from a value descriptor (a slider from a float or int), not as a free widget; a `Custom` dialog takes it as it is.",
        then: "",
    },
    Code {
        code: "c9",
        kind: CodeKind::Condition,
        title: "Claims honoured by declaration",
        text: "The host owns Enter and Escape, and gives them to a content that claims them: when the content claims Enter, Ctrl+Enter accepts; Escape goes first to a content that claims it and has something to cancel (a drag), then cancels.",
        then: "",
    },
    Code {
        code: "c10",
        kind: CodeKind::Condition,
        title: "A session per showing",
        text: "The host is transient: shown again, the component starts a new session (a new program, an empty draft) unless the application keeps its state; closing it asks first or loses the session.",
        then: "",
    },
    Code {
        code: "c11",
        kind: CodeKind::Condition,
        title: "A queue while hidden",
        text: "The host can stop drawing its content (a tab behind another, a palette hidden by Tab or rolled up) while the component's workers keep delivering: their queue grows until it is drawn again (docs/threading.md, section 2.2).",
        then: "",
    },
    Code {
        code: "c12",
        kind: CodeKind::Condition,
        title: "View state not kept",
        text: "The host keeps no view values and is outside the context stack's projection: the component's zoom, scroll or selection is not kept across places and restarts.",
        then: "",
    },
    Code {
        code: "D1",
        kind: CodeKind::Design,
        title: "Line hosts hold one row",
        text: "The top bar, the caption, the activity bar and the status bar hold indicators and small controls; nothing there takes the keyboard for long or needs more than a row.",
        then: "An item in the line that opens a popover (the jobs centre's status item) or focuses a panel (`<panel>.focus`).",
    },
    Code {
        code: "D2",
        kind: CodeKind::Design,
        title: "Viewport overlays are data",
        text: "Overlays on the image are shapes and handles in image coordinates, drawn by the viewport, their edits handed back as signals; there is no `Ui` to put a component in.",
        then: "A palette floating over the canvas (a navigator, a toolbox), or a heads-up host over the viewport (not built yet: decided for later).",
    },
    Code {
        code: "D3",
        kind: CodeKind::Design,
        title: "Popovers are transient",
        text: "A click outside or Escape closes them; an edit session (graph wiring, an editor with unsaved text, a running program, a theme draft) would end by accident (confirmed 2026-10-14).",
        then: "A panel, a palette, or a dialog that asks before closing.",
    },
    Code {
        code: "D4",
        kind: CodeKind::Design,
        title: "Only a modal asks",
        text: "Palettes are never modal (decided 2026-10-13), nor are panels and popovers: a question that must be answered before going on goes in a modal.",
        then: "A dialog.",
    },
    Code {
        code: "D5",
        kind: CodeKind::Design,
        title: "One dialog at a time",
        text: "Dialogs queue; a dialog does not open another over it.",
        then: "Draw the second choice inline in the dialog (as a form's path field draws the in-app file picker).",
    },
    Code {
        code: "D6",
        kind: CodeKind::Design,
        title: "The four dialog kinds take descriptors",
        text: "`Confirm`, `Form`, `Choose` and `Info` are drawn from data: a form's fields come from value descriptors, and no kind takes free content.",
        then: "A `Custom` dialog, which takes a body and keeps every dialog rule.",
    },
    Code {
        code: "E1",
        kind: CodeKind::Limit,
        title: "No free-content dialog (closed)",
        text: "Closed by `Dialog::Custom` (ADR 0013): a dialog with a body of its own, keeping the dialog rules.",
        then: "Done.",
    },
    Code {
        code: "E2",
        kind: CodeKind::Limit,
        title: "Palettes are not resizable",
        text: "A palette's width is between its panel's `min_size` and `default_size`, its height the content's.",
        then: "Small: a resize grip on the palette's area, the size kept in its place (decided: resizable).",
    },
    Code {
        code: "E3",
        kind: CodeKind::Limit,
        title: "No keymap in auxiliary windows",
        text: "Keys pressed in an auxiliary window are not dispatched.",
        then: "Medium: the dispatcher in each immediate viewport's pass, with that window's focus chain (decided: to fix).",
    },
    Code {
        code: "E4",
        kind: CodeKind::Limit,
        title: "Overlays live in the main window",
        text: "Dialogs, menus, toasts and the command palette are drawn in the main window, and egui's modal blocks its own window only: a dialog opened from an auxiliary window appears in the main one and does not block the auxiliary window.",
        then: "Medium to large: an overlay stack per window, or a modal blocking every window (decided: to fix).",
    },
    Code {
        code: "E5",
        kind: CodeKind::Limit,
        title: "No custom page in the settings window",
        text: "The settings window's pages are generated from declared settings.",
        then: "Small: a page kind drawing a block.",
    },
    Code {
        code: "U1",
        kind: CodeKind::Upstream,
        title: "No custom cursor in immediate viewports",
        text: "eframe shows no custom cursor in an immediate viewport (nor on the web): bitmap cursors fall back to system cursors there.",
        then: "[upstream.md](upstream.md#eframe-no-custom-cursor-on-the-web-or-in-immediate-viewports)",
    },
    Code {
        code: "U2",
        kind: CodeKind::Upstream,
        title: "Wide gamut in the main window only",
        text: "Wide gamut goes through the image's own Wayland surface, made from the main window's handles: eframe gives no surface colour space and winit no colour management, so other windows draw sRGB.",
        then: "[upstream.md](upstream.md#wayland-no-colour-management-through-winit)",
    },
    Code {
        code: "U3",
        kind: CodeKind::Upstream,
        title: "No owner window for egui viewports",
        text: "A palette in a native utility window would float above every application.",
        then: "[workbench.md](workbench.md#palettes-and-toolboxes-never-modal)",
    },
];

/// A condition or a reason by its code.
pub fn code(code: &str) -> Option<&'static Code> {
    CODES.iter().find(|c| c.code == code)
}

/// A condition a fit is under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Condition {
    pub rule: Rule,
    /// Its [`Code`] (`c1`...).
    pub code: &'static str,
}

impl Condition {
    /// Its text.
    pub fn text(&self) -> &'static str {
        code(self.code).map_or("", |c| c.text)
    }
}

/// Why a pair does not fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Refusal {
    pub rule: Rule,
    /// Its [`Code`] (`D1`, `E2`, `U2`...).
    pub code: &'static str,
    pub kind: CodeKind,
}

impl Refusal {
    /// The rule and what to do instead, in one sentence.
    pub fn reason(&self) -> String {
        match code(self.code) {
            Some(c) if !c.then.is_empty() && self.kind == CodeKind::Design => format!(
                "{} ({}, {}): {} Instead: {}",
                c.title,
                self.rule.number(),
                c.code,
                c.text,
                c.then
            ),
            Some(c) => format!("{} ({}, {}): {}", c.title, self.rule.number(), c.code, c.text),
            None => format!("{} ({})", self.rule.name(), self.code),
        }
    }
}

/// Whether a component fits a host.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "fit", rename_all = "snake_case")]
pub enum Fit {
    Yes,
    /// Fits, under every one of these conditions.
    Under { conditions: Vec<Condition> },
    /// Does not fit: the first refusal, and the others found.
    No {
        refusal: Refusal,
        also: Vec<Refusal>,
    },
}

impl Fit {
    /// Fits, with or without conditions.
    pub fn fits(&self) -> bool {
        !matches!(self, Fit::No { .. })
    }

    /// The conditions it fits under (none when it does not fit).
    pub fn conditions(&self) -> &[Condition] {
        match self {
            Fit::Under { conditions } => conditions,
            _ => &[],
        }
    }

    /// The refusal, when it does not fit.
    pub fn refusal(&self) -> Option<&Refusal> {
        match self {
            Fit::No { refusal, .. } => Some(refusal),
            _ => None,
        }
    }

    /// As a cell of the matrix: `yes`, `if c1 c4`, `D1`.
    pub fn cell(&self) -> String {
        match self {
            Fit::Yes => "yes".into(),
            Fit::Under { conditions } => {
                let codes: Vec<&str> = conditions.iter().map(|c| c.code).collect();
                format!("if {}", codes.join(" "))
            }
            Fit::No { refusal, .. } => refusal.code.into(),
        }
    }
}

/// What the rules found, before the worst verdict is taken.
#[derive(Debug, Default)]
struct Found {
    conditions: Vec<Condition>,
    refusals: Vec<Refusal>,
}

impl Found {
    fn condition(&mut self, rule: Rule, code: &'static str) {
        if !self.conditions.iter().any(|c| c.code == code) {
            self.conditions.push(Condition { rule, code });
        }
    }

    fn refuse(&mut self, rule: Rule, code: &'static str, kind: CodeKind) {
        if !self.refusals.iter().any(|r| r.code == code && r.rule == rule) {
            self.refusals.push(Refusal { rule, code, kind });
        }
    }
}

/// Matches a component's needs against a host's offer with the eleven
/// rules: the worst verdict wins, every condition is kept, a refusal
/// names its rule and code.
pub fn check(m: &Mountable, h: &HostOffer) -> Fit {
    let mut f = Found::default();
    ui(m, h, &mut f);
    application(m, h, &mut f);
    dialog_content(m, h, &mut f);
    size(m, h, &mut f);
    keyboard(m, h, &mut f);
    session(m, h, &mut f);
    question(m, h, &mut f);
    display(m, h, &mut f);
    instances(m, h, &mut f);
    workers(m, h, &mut f);
    view_state(m, h, &mut f);
    let mut refusals = f.refusals.into_iter();
    match refusals.next() {
        Some(refusal) => Fit::No {
            refusal,
            also: refusals.collect(),
        },
        None if f.conditions.is_empty() => Fit::Yes,
        None => {
            let mut conditions = f.conditions;
            conditions.sort_by_key(|c| order_of(c.code));
            Fit::Under { conditions }
        }
    }
}

/// A code's place in [`CODES`], to list conditions in a stable order.
fn order_of(code: &str) -> usize {
    CODES
        .iter()
        .position(|c| c.code == code)
        .unwrap_or(usize::MAX)
}

/// R8: a host without a `Ui` takes no component.
fn ui(_: &Mountable, h: &HostOffer, f: &mut Found) {
    if h.size == HostSize::NoUi || !h.gives_ui {
        f.refuse(Rule::Ui, "D2", CodeKind::Design);
    }
}

/// R7: a component needing the application, in a host giving none.
fn application(m: &Mountable, h: &HostOffer, f: &mut Found) {
    if m.app && !h.gives_app {
        f.refuse(Rule::Application, "D2", CodeKind::Design);
    }
}

/// R11: a dialog of the four kinds takes descriptor fields only.
fn dialog_content(m: &Mountable, h: &HostOffer, f: &mut Found) {
    if !h.free_content && h.gives_ui {
        if m.descriptor_field {
            f.condition(Rule::DialogContent, "c8");
        } else {
            f.refuse(Rule::DialogContent, "D6", CodeKind::Design);
        }
    }
}

/// R1: the size needed against the room given.
fn size(m: &Mountable, h: &HostOffer, f: &mut Found) {
    match h.size {
        HostSize::Row => {
            if m.size.fills || m.size.min[1] > ROW_HEIGHT {
                f.refuse(Rule::Size, "D1", CodeKind::Design);
            } else {
                f.condition(Rule::Size, "c6");
            }
        }
        HostSize::Content if m.size.fills => f.condition(Rule::Size, "c1"),
        _ => {}
    }
}

/// R2: the keyboard, claimed keys and the keymap.
fn keyboard(m: &Mountable, h: &HostOffer, f: &mut Found) {
    if h.size == HostSize::Row && m.keyboard == Keyboard::Exclusive {
        f.refuse(Rule::Keyboard, "D1", CodeKind::Design);
    }
    if m.claims.iter().any(|k| h.owns_keys.contains(k)) {
        if h.honours_claims {
            f.condition(Rule::Keyboard, "c9");
        } else {
            f.condition(Rule::Keyboard, "c4");
        }
    } else if m.keyboard == Keyboard::Exclusive && !h.owns_keys.is_empty() && !h.honours_claims
    {
        f.condition(Rule::Keyboard, "c4");
    }
    if m.keymap && !h.keymap {
        f.condition(Rule::Keyboard, "c2");
    }
}

/// R3: sessions and popups against a transient host.
fn session(m: &Mountable, h: &HostOffer, f: &mut Found) {
    let transient = h.lifetime == Lifetime::Transient;
    if m.session && h.dismissed_by.contains(&Dismiss::OutsideClick) {
        f.refuse(Rule::Session, "D3", CodeKind::Design);
    } else if m.session && transient {
        f.condition(Rule::Session, "c10");
    }
    if m.popups && transient {
        f.condition(Rule::Session, "c5");
    }
}

/// R4: only a modal asks.
fn question(m: &Mountable, h: &HostOffer, f: &mut Found) {
    if m.asks && !h.is_modal() {
        f.refuse(Rule::Question, "D4", CodeKind::Design);
    }
}

/// R5: colours and cursors.
fn display(m: &Mountable, h: &HostOffer, f: &mut Found) {
    match (m.display, h.display) {
        (Display::WideWanted, HostDisplay::Srgb) => f.condition(Rule::Display, "c3"),
        (Display::WideRequired, HostDisplay::Srgb) => {
            f.refuse(Rule::Display, "U2", CodeKind::Upstream);
        }
        _ => {}
    }
    if m.pointer.cursor_images && !h.cursor_images {
        f.condition(Rule::Display, "c3");
    }
}

/// R6: one state per application.
fn instances(m: &Mountable, _: &HostOffer, f: &mut Found) {
    if m.instances == Instances::Single {
        f.condition(Rule::Instances, "c7");
    }
}

/// R9: workers in a host that can stop drawing its content.
fn workers(m: &Mountable, h: &HostOffer, f: &mut Found) {
    if m.workers == Workers::DrainedWhenDrawn && h.hides_content {
        f.condition(Rule::Workers, "c11");
    }
}

/// R10: a view-state key in a host that keeps nothing.
fn view_state(m: &Mountable, h: &HostOffer, f: &mut Found) {
    if m.view_state.is_some() && !h.projection && !h.keeps.contains(&Keep::ViewValues) {
        f.condition(Rule::ViewState, "c12");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> HostOffer {
        HostOffer::new("panel", "P", "Docked panel").hides_content(true)
    }

    fn row() -> HostOffer {
        HostOffer::new("status-bar", "S", "Status bar").size(HostSize::Row)
    }

    fn popover() -> HostOffer {
        HostOffer::new("popover", "O", "Popover")
            .size(HostSize::Content)
            .focus(FocusPolicy::ClickInto)
            .owns_keys(&[ClaimKey::Escape])
            .dismissed_by(&[Dismiss::Escape, Dismiss::OutsideClick])
            .lifetime(Lifetime::Transient)
    }

    fn modal() -> HostOffer {
        HostOffer::new("modal", "M", "Raw modal")
            .size(HostSize::Content)
            .focus(FocusPolicy::ModalExclusive)
            .owns_keys(&[ClaimKey::Escape])
            .dismissed_by(&[Dismiss::Escape])
            .lifetime(Lifetime::Transient)
    }

    fn custom_dialog() -> HostOffer {
        HostOffer::new("dialog.custom", "C", "Custom dialog")
            .size(HostSize::Given)
            .focus(FocusPolicy::ModalExclusive)
            .owns_keys(&[ClaimKey::Enter, ClaimKey::Escape])
            .honours_claims()
            .dismissed_by(&[Dismiss::Answer, Dismiss::Escape])
            .lifetime(Lifetime::Transient)
    }

    fn window() -> HostOffer {
        HostOffer::new("window", "W", "Auxiliary window")
            .window(Window::Auxiliary)
            .keymap(false)
            .cursor_images(false)
    }

    fn overlay() -> HostOffer {
        HostOffer::new("viewport.overlay", "V", "Viewport overlay")
            .size(HostSize::NoUi)
            .gives_app(false)
    }

    fn codes(fit: &Fit) -> Vec<&'static str> {
        match fit {
            Fit::Yes => Vec::new(),
            Fit::Under { conditions } => conditions.iter().map(|c| c.code).collect(),
            Fit::No { refusal, also } => std::iter::once(refusal.code)
                .chain(also.iter().map(|r| r.code))
                .collect(),
        }
    }

    #[test]
    fn a_plain_widget_fits_a_panel() {
        let m = Mountable::new("button", "Button");
        assert_eq!(check(&m, &panel()), Fit::Yes);
        assert_eq!(check(&m, &panel()).cell(), "yes");
    }

    #[test]
    fn r1_size_a_filling_component_needs_a_size_in_a_content_host_and_no_row() {
        let m = Mountable::new("grid", "Grid").fills();
        assert_eq!(codes(&check(&m, &popover())), ["c1"]);
        assert_eq!(check(&m, &panel()), Fit::Yes);
        assert_eq!(check(&m, &custom_dialog()), Fit::Yes, "a given size");
        let fit = check(&m, &row());
        assert_eq!(fit.refusal().unwrap().rule, Rule::Size);
        assert_eq!(fit.refusal().unwrap().kind, CodeKind::Design);
        assert_eq!(fit.cell(), "D1");
        // Small enough for a row: under the compact-size condition.
        let slider = Mountable::new("slider", "Slider").min(120.0, 20.0);
        assert_eq!(codes(&check(&slider, &row())), ["c6"]);
        let tall = Mountable::new("card", "Card").min(200.0, 120.0);
        assert_eq!(codes(&check(&tall, &row())), ["D1"]);
    }

    #[test]
    fn r2_keyboard_claims_against_owned_keys_and_the_keymap() {
        let graph = Mountable::new("graph", "Graph").claims(&[ClaimKey::Escape]);
        assert_eq!(codes(&check(&graph, &modal())), ["c4"]);
        assert_eq!(codes(&check(&graph, &custom_dialog())), ["c9"], "honoured");
        assert_eq!(check(&graph, &panel()), Fit::Yes, "the panel owns no key");
        let terminal = Mountable::new("terminal", "Terminal").keyboard(Keyboard::Exclusive);
        assert_eq!(codes(&check(&terminal, &modal())), ["c4"]);
        let fit = check(&terminal.clone().min(0.0, 10.0), &row());
        assert_eq!(fit.refusal().unwrap().rule, Rule::Keyboard);
        let editor = Mountable::new("editor", "Editor").surface("editor");
        assert!(editor.keymap);
        assert_eq!(codes(&check(&editor, &window())), ["c2"]);
        assert_eq!(check(&editor, &panel()), Fit::Yes);
    }

    #[test]
    fn r3_a_session_never_goes_in_a_popover_and_restarts_in_a_transient_host() {
        let terminal = Mountable::new("terminal", "Terminal").session();
        let fit = check(&terminal, &popover());
        assert_eq!(fit.refusal().unwrap().rule, Rule::Session);
        assert_eq!(fit.cell(), "D3");
        assert_eq!(codes(&check(&terminal, &modal())), ["c10"]);
        assert_eq!(check(&terminal, &panel()), Fit::Yes);
        let code = Mountable::new("code", "Code editor").popups();
        assert_eq!(codes(&check(&code, &popover())), ["c5"]);
    }

    #[test]
    fn r4_only_a_modal_asks() {
        let q = Mountable::new("question", "Question").asks();
        assert_eq!(check(&q, &panel()).cell(), "D4");
        assert_eq!(check(&q, &popover()).refusal().unwrap().rule, Rule::Question);
        assert!(check(&q, &modal()).fits());
        assert!(check(&q, &custom_dialog()).fits());
    }

    #[test]
    fn r5_wide_gamut_degrades_when_wanted_and_refuses_upstream_when_required() {
        let wanted = Mountable::new("viewport", "Viewport").display(Display::WideWanted);
        assert_eq!(codes(&check(&wanted, &window())), ["c3"]);
        let wide = panel().display(HostDisplay::Wide);
        assert_eq!(check(&wanted, &wide), Fit::Yes);
        let required = Mountable::new("mastering", "Mastering").display(Display::WideRequired);
        let fit = check(&required, &window());
        assert_eq!(fit.refusal().unwrap().kind, CodeKind::Upstream);
        assert_eq!(fit.cell(), "U2");
        assert!(check(&required, &wide).fits());
        let cursors = Mountable::new("brush", "Brush").cursor_images();
        assert_eq!(codes(&check(&cursors, &window())), ["c3"]);
        assert_eq!(check(&cursors, &panel()), Fit::Yes);
    }

    #[test]
    fn r6_a_single_component_shows_one_state_wherever_it_is() {
        let jobs = Mountable::new("jobs", "Jobs centre").single();
        assert_eq!(codes(&check(&jobs, &panel())), ["c7"]);
        assert_eq!(codes(&check(&jobs, &popover())), ["c7"]);
    }

    #[test]
    fn r7_and_r8_a_data_host_takes_no_component() {
        let plain = Mountable::new("label", "Label");
        let fit = check(&plain, &overlay());
        assert_eq!(fit.refusal().unwrap().rule, Rule::Ui);
        assert_eq!(fit.cell(), "D2");
        let app = Mountable::new("tips", "Tips").needs_app();
        let fit = check(&app, &overlay());
        assert_eq!(codes(&fit), ["D2", "D2"], "R8, then R7");
        match fit {
            Fit::No { also, .. } => assert_eq!(also[0].rule, Rule::Application),
            other => panic!("{other:?}"),
        }
        // A host with a Ui but no application.
        let bare = panel().gives_app(false);
        assert_eq!(check(&app, &bare).refusal().unwrap().rule, Rule::Application);
        assert!(check(&plain, &bare).fits());
    }

    #[test]
    fn r9_workers_queue_while_the_host_hides_the_component() {
        let v = Mountable::new("viewport", "Viewport").workers();
        assert_eq!(codes(&check(&v, &panel())), ["c11"]);
        assert_eq!(check(&v, &custom_dialog()), Fit::Yes, "drawn every frame");
    }

    #[test]
    fn r10_a_view_state_is_kept_only_by_a_host_that_keeps_it() {
        let v = Mountable::new("tree", "Tree").view_state("tree.expanded");
        assert_eq!(codes(&check(&v, &modal())), ["c12"]);
        assert_eq!(check(&v, &panel().projection(true)), Fit::Yes);
        assert_eq!(check(&v, &panel().keeps(&[Keep::ViewValues])), Fit::Yes);
    }

    #[test]
    fn r11_the_four_dialog_kinds_take_descriptor_fields_only() {
        let four = modal()
            .owns_keys(&[ClaimKey::Enter, ClaimKey::Escape])
            .free_content(false);
        let viewport = Mountable::new("viewport", "Viewport");
        let fit = check(&viewport, &four);
        assert_eq!(fit.refusal().unwrap().rule, Rule::DialogContent);
        assert_eq!(fit.cell(), "D6");
        let slider = Mountable::new("slider", "Slider").descriptor_field();
        assert_eq!(codes(&check(&slider, &four)), ["c8"]);
        assert!(check(&viewport, &custom_dialog()).fits());
    }

    #[test]
    fn conditions_are_kept_in_code_order_and_the_worst_verdict_wins() {
        let terminal = Mountable::new("terminal", "Terminal")
            .keyboard(Keyboard::Exclusive)
            .claims(&[ClaimKey::Escape, ClaimKey::Enter])
            .surface("terminal")
            .session()
            .fills()
            .workers();
        let fit = check(&terminal, &modal());
        assert_eq!(fit.cell(), "if c1 c4 c10");
        assert_eq!(check(&terminal, &popover()).cell(), "D3");
        assert_eq!(check(&terminal, &window()).cell(), "if c2");
    }

    #[test]
    fn every_code_used_has_its_text_and_refusals_say_what_to_do_instead() {
        for c in CODES {
            assert!(!c.title.is_empty() && !c.text.is_empty(), "{}", c.code);
            if c.kind != CodeKind::Condition {
                assert!(!c.then.is_empty(), "{}", c.code);
            }
        }
        let fit = check(&Mountable::new("session", "S").session(), &popover());
        let reason = fit.refusal().unwrap().reason();
        assert!(reason.contains("R3"), "{reason}");
        assert!(reason.contains("Instead: A panel"), "{reason}");
        assert_eq!(Rule::ALL.len(), 11);
        let numbers: std::collections::BTreeSet<&str> =
            Rule::ALL.iter().map(|r| r.number()).collect();
        assert_eq!(numbers.len(), 11);
    }
}
