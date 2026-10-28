//! The clipboard (ADR 0015): typed items kept by the application's
//! owner, readable fallbacks on the system clipboard, a paste negotiated
//! between what a copy offers and what the target accepts, and hooks at
//! each boundary.
//!
//! - **The owner** implements [`Clipboard`]: one item per slot (the
//!   user's, and each agent session's own), `put`, `get`, a revision. The
//!   host gives it ([`Host::clipboard`](crate::host::Host::clipboard));
//!   without one, [`ClipboardPlugin`] keeps a [`LocalClipboard`] in
//!   memory, shared by the application's windows. It publishes
//!   `clipboard.changed` (the slot, the type, the revision; never the
//!   payload).
//! - **Sources and targets** are the components': a component registers
//!   an endpoint by name ([`register`], or [`Ctx::clip_endpoint`]) with a
//!   [`ClipSource`] (what a copy takes, what a cut deletes) and a
//!   [`ClipTarget`] (the formats it accepts, how it applies a paste),
//!   and says each pass which widget draws it ([`bind`]), which widgets
//!   draw its elements ([`bind_element`]: a graph's nodes, a tree's
//!   rows, each with its key) or which area it covers ([`bind_area`]):
//!   the endpoint of the focused widget is the one the Edit commands
//!   use, and its source and target receive the element's key.
//! - **Types, transforms and sanitisers** are declared by plugins and go
//!   with them ([`Ctx::clip_type`], [`Ctx::clip_transform`],
//!   [`Ctx::clip_sanitiser`]): a type renders its fallbacks, lists its
//!   parts and migrates its older versions; a transform makes one format
//!   from another (lossy or not); a sanitiser removes what must not cross
//!   a boundary and says what it removed.
//! - **Steps** (ADR 0011): `clipboard.copy` and `clipboard.paste`, which
//!   a before-hook vetoes (a secret field) or redirects (another
//!   selection, another target, another format). A copy is never an
//!   undo step; a cut deletes and copies as one undo step and copies
//!   only once its delete is accepted; a paste is one undo step ("Paste
//!   a colour"), through the host's history group
//!   ([`Host::begin_group`](crate::host::Host::begin_group)).
//! - **Commands**: `edit.cut` (Ctrl+X, Shift+Delete), `edit.copy`
//!   (Ctrl+C, Ctrl+Insert), `edit.paste` (Ctrl+V, Shift+Insert) and
//!   `edit.pasteSpecial` (Ctrl+Shift+V, a choice of every format,
//!   transform and part that fits, each with the target's preview) in
//!   the Edit menu, `when` `!textInputFocus` and the keys `canCut`,
//!   `canCopy`, `canPaste`: greyed when nothing focused answers, and text
//!   fields keep egui's own copy and paste. egui-winit turns the
//!   platform's copy, cut and paste chords into events and swallows the
//!   keys; those events are dispatched to the commands when an endpoint
//!   has the focus (Shift held on a paste: paste special). For agents:
//!   `clipboard.copy`, `clipboard.paste` (with `format`, `transform`,
//!   `parts`, `if_rev` and `dry_run`), `clipboard.offers`,
//!   `clipboard.read`, `clipboard.write`, each in the session's own slot
//!   unless the owner allows otherwise; the `clipboard` collector.
//!
//! **The system clipboard** is reached through a [`SystemBackend`]: a
//! copy gives it every representation it writes, at once; a paste reads
//! every format it can (files, images, HTML, text) as one external item.
//! The default, [`EguiText`], writes text through egui and reads
//! nothing: a paste then knows the system clipboard only by the text the
//! paste key brought, and a paste through the menu or a command uses the
//! owner's item. A host gives its own backend ([`Clipboard::system`]),
//! an application installs one ([`set_backend`]; `egui-pop-platform`'s
//! `clipboard` feature has one over arboard).
//!
//! ```
//! use std::rc::Rc;
//! use std::cell::RefCell;
//! use egui_pop_api::clipboard::{self, ClipSource, ClipTarget, ClipboardPlugin, Paste};
//! use egui_pop_api::{App, AppHandle};
//! use egui_pop_model::clipboard::{ClipItem, FormatId, TEXT};
//! use egui_pop_model::commands::Origin;
//!
//! const NOTE: FormatId = FormatId::from_static("application/x-demo.note+json");
//!
//! /// A note: copied as a typed item, pasted from one or from text.
//! #[derive(Default)]
//! struct Note(RefCell<String>);
//! impl ClipSource for Note {
//!     fn copy(&self, _: &AppHandle) -> Option<ClipItem> {
//!         let text = self.0.borrow().clone();
//!         Some(ClipItem::new(NOTE, 1, serde_json::json!({ "text": text }), "a note")
//!             .fallback(TEXT, serde_json::json!(text)))
//!     }
//! }
//! impl ClipTarget for Note {
//!     fn accepts(&self, _: &AppHandle) -> Vec<FormatId> { vec![NOTE, TEXT] }
//!     fn paste(&self, _: &AppHandle, p: &Paste<'_>) -> Result<serde_json::Value, String> {
//!         let text = p.repr.data["text"].as_str().or(p.repr.text()).unwrap_or("").to_string();
//!         if !p.dry_run { *self.0.borrow_mut() = text.clone(); }
//!         Ok(serde_json::json!({ "set": text }))
//!     }
//! }
//!
//! let app = App::builder().plugin(ClipboardPlugin::default())
//!     .build(&egui::Context::default()).unwrap();
//! let h = app.handle();
//! let (a, b) = (Rc::new(Note::default()), Rc::new(Note::default()));
//! *a.0.borrow_mut() = "hello".into();
//! clipboard::register(h, "a", Some(a.clone()), Some(a.clone()));
//! clipboard::register(h, "b", Some(b.clone()), Some(b.clone()));
//! assert!(clipboard::copy(h, clipboard::CopyRequest::new("a"), &Origin::User).ok());
//! assert!(clipboard::paste(h, clipboard::PasteRequest::new("b"), &Origin::User).ok());
//! assert_eq!(*b.0.borrow(), "hello");
//! ```

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::rc::Rc;

use egui_pop_core::debug::FnCollector;
use egui_pop_core::strings;
use egui_pop_model::clipboard as model;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::ids::{CommandId, OwnerId, PluginId, StepId};
use egui_pop_model::values::GestureId;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use crate::app::{AppHandle, MENU_EDIT};
use crate::hooks::{Lifecycle, On};
use crate::host::{Pending, source_of};
use crate::steps::{Outcome, StepSpec};
use crate::strings as txt;
use crate::{Ctx, CtxError, Plugin, PluginError};

pub use egui_pop_model::clipboard::{
    Boundary, ClipItem, ClipOrigin, FormatId, HTML, PNG, Plan, Refusal, Repr, Slot, Stripped, TEXT,
    TitleKey, TransformSpec, URI_LIST,
};

/// The plugin's id.
pub const PLUGIN: PluginId = PluginId::from_static("egui_pop.clipboard");

/// The Edit menu's group of the clipboard commands, under undo.
pub const MENU_GROUP: &str = "1_clipboard";

/// Cuts the focused endpoint's selection: copied once its delete is
/// accepted, one undo step.
pub const CMD_CUT: CommandId = CommandId::from_static("edit.cut");
/// Copies the focused endpoint's selection.
pub const CMD_COPY: CommandId = CommandId::from_static("edit.copy");
/// Pastes into the focused endpoint, in the richest format it accepts.
pub const CMD_PASTE: CommandId = CommandId::from_static("edit.paste");
/// Pastes into the focused endpoint in a format, through a transform or
/// only some parts, chosen in a dialog showing each one's preview.
pub const CMD_PASTE_SPECIAL: CommandId = CommandId::from_static("edit.pasteSpecial");
/// Copies from an endpoint by name (`source`, `slot`, `cut`,
/// `dry_run`).
pub const CMD_CLIP_COPY: CommandId = CommandId::from_static("clipboard.copy");
/// Pastes into an endpoint by name (`target`, `slot`, `format`,
/// `transform`, `parts`, `if_rev`, `dry_run`).
pub const CMD_CLIP_PASTE: CommandId = CommandId::from_static("clipboard.paste");
/// What a slot holds and the ways it fits a target (`slot`, `target`).
pub const CMD_OFFERS: CommandId = CommandId::from_static("clipboard.offers");
/// A slot's item, payload included (`slot`).
pub const CMD_READ: CommandId = CommandId::from_static("clipboard.read");
/// Puts an item in a slot (`slot`, `format`, `data`, `title`).
pub const CMD_WRITE: CommandId = CommandId::from_static("clipboard.write");

/// A copy (`{ source, slot, cut, dry_run, origin }`): a before-hook
/// vetoes it or redirects it (another source, another slot); its default
/// asks the source for the item, sanitises what goes to the system
/// clipboard, deletes for a cut, and gives the item to the owner. Its
/// result: the type, the title, the revision, what was left out.
pub const STEP_COPY: StepId = StepId::from_static("clipboard.copy");
/// A paste (`{ target, slot, format, transform, parts, if_rev, dry_run,
/// text, origin }`): a before-hook vetoes it or redirects it (another
/// target, another format); its default takes the owner's item (or the
/// external text), sanitises external content, negotiates the format,
/// runs the transform and has the target apply it as one undo step. Its
/// result: the plan, what was lost, the target's changes.
pub const STEP_PASTE: StepId = StepId::from_static("clipboard.paste");

/// A sanitiser of egui-pop's own: control characters and escape
/// sequences removed from external text.
pub const SANITISE_CONTROLS: &str = "egui_pop.controls";
/// A transform of egui-pop's own: HTML reduced to its text (lossy).
pub const HTML_TO_TEXT: &str = "egui_pop.html-to-text";

const NO_PLUGIN: &str = "no clipboard: the ClipboardPlugin is not connected";

/// What the owner answers a `put`.
#[derive(Debug)]
pub enum Answer {
    /// Kept (its new revision), or refused.
    Done(Result<u64, String>),
    /// A remote owner answers later.
    Later(Pending),
}

/// The owner of the clipboard, as the interface reaches it.
pub trait Clipboard {
    /// Keeps an item in a slot, replacing what it held.
    fn put(&mut self, slot: &Slot, item: ClipItem, origin: &Origin) -> Answer;

    /// The item a slot holds.
    fn get(&self, slot: &Slot) -> Option<ClipItem>;

    /// The formats a slot offers.
    fn offers(&self, slot: &Slot) -> Vec<FormatId> {
        self.get(slot).map(|i| i.offers()).unwrap_or_default()
    }

    /// The revision: one more at each `put`.
    fn rev(&self) -> u64;

    /// Whether `origin` may read and write `slot`: by default, the user
    /// any slot, a session its own only ([`default_access`]).
    fn may_use(&self, origin: &Origin, slot: &Slot) -> Result<(), String> {
        default_access(origin, slot)
    }

    /// The events to publish since the last call, as (name, data).
    fn take_events(&mut self) -> Vec<(String, Json)> {
        Vec::new()
    }

    /// The owner's own way to the system clipboard (a kernel writing
    /// its private types there): it wins over the one installed with
    /// [`set_backend`], which wins over egui's text.
    fn system(&mut self) -> Option<&mut dyn SystemBackend> {
        None
    }
}

/// The system clipboard as egui-pop writes and reads it: what one copy
/// leaves there for other applications, what a paste finds there.
///
/// The default, [`EguiText`], writes the text through egui and reads
/// nothing (a paste knows only the text the paste key brought). A host
/// gives its own through [`Clipboard::system`], an application installs
/// one with [`set_backend`] (`egui-pop-platform`'s `clipboard` feature
/// has one over arboard: files, images, HTML and the primary
/// selection).
///
/// Representations follow the formats' usual data: text, HTML and a
/// URI list (`text/uri-list`, one `file://` URI per line, CRLF between)
/// are JSON strings; an image (`image/png`) is a string, the PNG file's
/// bytes in standard base64; a private type is its JSON payload.
pub trait SystemBackend {
    /// What it is called, for the collector ("egui", "arboard").
    fn name(&self) -> String;

    /// Whether it writes a format: a copy gives it every representation
    /// it writes, so a private type crosses only where it can.
    fn writes(&self, format: &FormatId) -> bool {
        *format == TEXT
    }

    /// Writes one copy's representations (richest first, sanitised for
    /// the system boundary) at once, replacing what the system clipboard
    /// held.
    fn write(&mut self, ctx: &egui::Context, reprs: &[Repr]) -> Result<(), String>;

    /// What the system clipboard holds, in each format it reads, richest
    /// first; `None` when it cannot read (a paste then knows only the
    /// text the paste key brought), empty when the clipboard is.
    fn read(&mut self, ctx: &egui::Context) -> Option<Vec<Repr>> {
        let _ = ctx;
        None
    }

    /// The formats a read may bring besides text, so that the paste
    /// command is enabled for a target taking only files or images.
    fn readable(&self) -> Vec<FormatId> {
        Vec::new()
    }

    /// The primary selection's text (X11 and Wayland: select to copy,
    /// middle-click to paste); `None` where there is none.
    fn read_primary(&mut self) -> Option<String> {
        None
    }

    /// Sets the primary selection's text.
    fn write_primary(&mut self, text: &str) -> Result<(), String> {
        let _ = text;
        Err("this clipboard backend has no primary selection".into())
    }

    /// What it says of itself for the collector: its platform path (on
    /// Wayland: XWayland, the data-control protocol, or the core
    /// protocol), what it reads and writes.
    fn describe(&self) -> Json {
        Json::Null
    }
}

/// The default backend: text through egui (`Context::copy_text`, which
/// eframe hands to its own clipboard at the end of the pass), nothing
/// read.
#[derive(Debug, Default, Clone, Copy)]
pub struct EguiText;

impl SystemBackend for EguiText {
    fn name(&self) -> String {
        "egui".into()
    }

    fn write(&mut self, ctx: &egui::Context, reprs: &[Repr]) -> Result<(), String> {
        if let Some(text) = reprs.iter().find(|r| r.format == TEXT).and_then(Repr::text) {
            ctx.copy_text(text.to_string());
        }
        Ok(())
    }

    fn describe(&self) -> Json {
        json!({ "writes": [TEXT], "reads": "the paste key's text" })
    }
}

/// Installs the system clipboard's backend (unless the owner gives its
/// own, [`Clipboard::system`]); before, egui's text ([`EguiText`]).
pub fn set_backend(app: &AppHandle, backend: Box<dyn SystemBackend>) {
    let st = state(app);
    *st.backend.borrow_mut() = Some(backend);
    *st.read_cache.borrow_mut() = None;
}

/// Runs `f` on the system backend: the owner's, else the one installed,
/// else egui's text.
pub fn with_backend<R>(app: &AppHandle, f: impl FnOnce(&mut dyn SystemBackend) -> R) -> R {
    let mut f = Some(f);
    let by_owner = with_owner(app, |c| {
        c.system().map(|b| f.take().expect("called once")(b))
    })
    .flatten();
    if let Some(r) = by_owner {
        return r;
    }
    let f = f.take().expect("called once");
    let st = state(app);
    let mut installed = st.backend.borrow_mut();
    match installed.as_mut() {
        Some(b) => f(b.as_mut()),
        None => f(&mut EguiText),
    }
}

/// What the system clipboard holds now, read once per paste (paste
/// special asks several times) and per pass.
fn system_read(app: &AppHandle) -> Option<Vec<Repr>> {
    let ctx = app.egui();
    let now = ctx.cumulative_pass_nr();
    let st = state(app);
    if let Some((at, reprs)) = st.read_cache.borrow().as_ref()
        && *at == now
    {
        return reprs.clone();
    }
    let reprs = with_backend(app, |b| b.read(&ctx));
    *st.read_cache.borrow_mut() = Some((now, reprs.clone()));
    reprs
}

/// The primary selection's text, through the system backend (opt-in:
/// egui-pop's own widgets do not use it yet).
pub fn primary_text(app: &AppHandle) -> Option<String> {
    with_backend(app, |b| b.read_primary())
}

/// Sets the primary selection's text, through the system backend.
pub fn set_primary_text(app: &AppHandle, text: &str) -> Result<(), String> {
    with_backend(app, |b| b.write_primary(text))
}

/// The default rule: the user may use any slot, a session (an agent, a
/// script) its own only.
pub fn default_access(origin: &Origin, slot: &Slot) -> Result<(), String> {
    if *origin == Origin::User || *slot == Slot::of(origin) {
        Ok(())
    } else {
        Err(format!(
            "`{}` may not use the `{slot}` clipboard: a session has its own unless the host allows it",
            source_of(origin)
        ))
    }
}

/// The default owner: one item per slot, in memory.
#[derive(Debug, Default)]
pub struct LocalClipboard {
    items: BTreeMap<Slot, ClipItem>,
    rev: u64,
    allowed: BTreeSet<String>,
    events: Vec<(String, Json)>,
}

impl LocalClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lets a session (`agent:<name>`) use every slot, the user's
    /// included.
    pub fn allow(&mut self, session: &str) {
        self.allowed.insert(session.into());
    }
}

impl Clipboard for LocalClipboard {
    fn put(&mut self, slot: &Slot, item: ClipItem, _origin: &Origin) -> Answer {
        self.rev += 1;
        self.events.push((
            "clipboard.changed".into(),
            json!({ "slot": slot, "kind": item.kind, "rev": self.rev }),
        ));
        self.items.insert(slot.clone(), item);
        Answer::Done(Ok(self.rev))
    }

    fn get(&self, slot: &Slot) -> Option<ClipItem> {
        self.items.get(slot).cloned()
    }

    fn rev(&self) -> u64 {
        self.rev
    }

    fn may_use(&self, origin: &Origin, slot: &Slot) -> Result<(), String> {
        if self.allowed.contains(&source_of(origin)) {
            return Ok(());
        }
        default_access(origin, slot)
    }

    fn take_events(&mut self) -> Vec<(String, Json)> {
        std::mem::take(&mut self.events)
    }
}

/// A cut's delete: the item copied, who cuts, and the history group its
/// writes carry (`Set::gesture`) so that the delete is one undo step.
#[derive(Debug)]
pub struct Cut<'a> {
    pub item: &'a ClipItem,
    pub origin: &'a Origin,
    pub group: Option<GestureId>,
    /// The element cut (a node, a row): the key its widget was bound
    /// with ([`bind_element`]), or the one a request named.
    pub element: Option<&'a str>,
}

/// What a copy takes from a component.
pub trait ClipSource {
    /// The selection as an item; `None`: nothing to copy.
    fn copy(&self, app: &AppHandle) -> Option<ClipItem>;

    /// What a copy takes from one element of the endpoint (a node, a
    /// row: the key its widget was bound with, [`bind_element`], or the
    /// one a request named); by default [`ClipSource::copy`], whatever
    /// the element.
    fn copy_element(&self, app: &AppHandle, element: Option<&str>) -> Option<ClipItem> {
        let _ = element;
        self.copy(app)
    }

    /// Whether a copy would take something now (`canCopy`).
    fn can_copy(&self, _app: &AppHandle) -> bool {
        true
    }

    /// Whether a cut may delete the selection now (`canCut`); by
    /// default a source copies only.
    fn can_cut(&self, _app: &AppHandle) -> bool {
        false
    }

    /// Deletes what a cut copies; an error refuses the cut, and nothing
    /// is copied.
    fn delete(&self, _app: &AppHandle, _cut: &Cut<'_>) -> Result<(), String> {
        Err("this cannot be cut".into())
    }
}

/// A paste as a target receives it.
#[derive(Debug)]
pub struct Paste<'a> {
    /// The representation, in the format the plan chose (transformed).
    pub repr: &'a Repr,
    pub plan: &'a Plan,
    /// The item it comes from (its title, origin, payload).
    pub item: &'a ClipItem,
    /// The parts asked for (paste special), all when `None`.
    pub parts: Option<&'a [String]>,
    /// Say what would change, change nothing.
    pub dry_run: bool,
    pub origin: &'a Origin,
    /// The history group the target's writes carry (`Set::gesture`):
    /// the paste is one undo step.
    pub group: Option<GestureId>,
    /// The element pasted into (a node, a row): the key its widget was
    /// bound with ([`bind_element`]), or the one a request named.
    pub element: Option<&'a str>,
}

impl Paste<'_> {
    /// Whether a part is asked for.
    pub fn wants(&self, part: &str) -> bool {
        self.parts.is_none_or(|p| p.iter().any(|x| x == part))
    }
}

/// Where a paste goes in a component.
pub trait ClipTarget {
    /// The formats it accepts, richest first.
    fn accepts(&self, app: &AppHandle) -> Vec<FormatId>;

    /// Applies a paste, or for a dry run says what it would change: its
    /// changes as JSON (a dry run's are paste special's preview, a
    /// string reading best).
    fn paste(&self, app: &AppHandle, paste: &Paste<'_>) -> Result<Json, String>;

    /// Applies a paste (never a dry run: paste special's previews and an
    /// agent's plan go through [`ClipTarget::paste`]). A target whose
    /// edit goes through the application's own loop (a network edit, a
    /// viewport command applied next frame) answers [`Pasted::Later`]
    /// with a result to come ([`crate::host::pending`]): its writes still
    /// carry `paste.group`, which stays open until the result arrives,
    /// so the paste remains one undo step; the result is published as
    /// `clipboard.pasted` and a failure shown to the user. By default,
    /// [`ClipTarget::paste`] now.
    fn apply(&self, app: &AppHandle, paste: &Paste<'_>) -> Pasted {
        Pasted::Done(self.paste(app, paste))
    }

    /// What a transform may know of the target before adapting a paste
    /// to it ([`Transform::for_target`]): the image a mask lands on, the
    /// range a value is clamped to, the names a pasted one must not
    /// take. `element` is the element pasted into, if any. Nothing by
    /// default.
    fn context(&self, app: &AppHandle, element: Option<&str>) -> Json {
        let _ = (app, element);
        Json::Null
    }
}

/// What a target answers a paste it applies ([`ClipTarget::apply`]).
#[derive(Debug)]
pub enum Pasted {
    /// Its changes, or why it refused, now.
    Done(Result<Json, String>),
    /// The result comes later (the application's loop applies it).
    Later(Pending),
}

/// A paste whose result is to come: its group stays open until then.
struct Deferred {
    pending: Pending,
    group: Option<GestureId>,
    origin: Origin,
    summary: Json,
}

/// The target a transform adapts a paste to.
#[derive(Clone, Copy, Debug)]
pub struct Destination<'a> {
    /// The endpoint's name.
    pub target: &'a str,
    /// The element pasted into, if any.
    pub element: Option<&'a str>,
    /// What the target says of itself ([`ClipTarget::context`]).
    pub context: &'a Json,
}

type RenderFn = Rc<dyn Fn(&Json) -> Vec<Repr>>;
type PartsFn = Rc<dyn Fn(&Json) -> Vec<Part>>;
type MigrateFn = Rc<dyn Fn(u32, Json) -> Result<Json, String>>;
type TransformFn = Rc<dyn Fn(&Repr, &Destination<'_>) -> Result<Transformed, String>>;
type AppliesFn = Rc<dyn Fn(&Repr, &str) -> bool>;
type SanitiseFn = Rc<dyn Fn(&mut Json) -> Vec<String>>;

/// A part of an item a paste special may take alone (a parameter set's
/// modules).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub id: String,
    pub title: String,
}

impl Part {
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
        }
    }
}

/// A type of item a plugin declares and owns.
#[derive(Clone)]
pub struct ClipType {
    pub format: FormatId,
    /// Its current version: an older item is migrated before a paste.
    pub version: u32,
    /// What one is called ("a colour").
    pub title: String,
    /// Kept inside the owner only: nothing goes to the system clipboard.
    pub internal: bool,
    render: Option<RenderFn>,
    parts: Option<PartsFn>,
    migrate: Option<MigrateFn>,
}

impl fmt::Debug for ClipType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClipType")
            .field("format", &self.format)
            .field("version", &self.version)
            .field("title", &self.title)
            .field("internal", &self.internal)
            .finish_non_exhaustive()
    }
}

impl ClipType {
    pub fn new(format: FormatId, version: u32, title: &str) -> Self {
        Self {
            format,
            version,
            title: title.into(),
            internal: false,
            render: None,
            parts: None,
            migrate: None,
        }
    }

    /// Never written to the system clipboard (a secret, a huge item).
    pub fn internal(mut self) -> Self {
        self.internal = true;
        self
    }

    /// Renders its fallbacks (text always; HTML when it has one), for
    /// items whose source gave none.
    pub fn fallbacks(mut self, f: impl Fn(&Json) -> Vec<Repr> + 'static) -> Self {
        self.render = Some(Rc::new(f));
        self
    }

    /// Lists its parts, for paste special.
    pub fn parts(mut self, f: impl Fn(&Json) -> Vec<Part> + 'static) -> Self {
        self.parts = Some(Rc::new(f));
        self
    }

    /// Migrates a payload made at an older version (given) to the
    /// current one.
    pub fn migrate(mut self, f: impl Fn(u32, Json) -> Result<Json, String> + 'static) -> Self {
        self.migrate = Some(Rc::new(f));
        self
    }
}

/// What a transform made: the data, and what it lost if anything.
#[derive(Clone, Debug, PartialEq)]
pub struct Transformed {
    pub data: Json,
    pub lost: Option<String>,
}

impl Transformed {
    pub fn exact(data: Json) -> Self {
        Self { data, lost: None }
    }

    pub fn lossy(data: Json, lost: &str) -> Self {
        Self {
            data,
            lost: Some(lost.into()),
        }
    }
}

/// A transform a plugin declares and owns.
#[derive(Clone)]
pub struct Transform {
    pub spec: TransformSpec,
    applies: Option<AppliesFn>,
    run: TransformFn,
}

impl fmt::Debug for Transform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transform")
            .field("spec", &self.spec)
            .finish_non_exhaustive()
    }
}

impl Transform {
    pub fn new(
        spec: TransformSpec,
        f: impl Fn(&Repr) -> Result<Transformed, String> + 'static,
    ) -> Self {
        Self {
            spec,
            applies: None,
            run: Rc::new(move |r, _| f(r)),
        }
    }

    /// A transform that adapts to the target: it also receives the
    /// target's name, element and context ([`ClipTarget::context`]): a
    /// mask scaled to the image it lands on, a value clamped to the
    /// target's range, a name made unique. What it loses shows in the
    /// toast and in paste special's preview like any other loss.
    pub fn for_target(
        spec: TransformSpec,
        f: impl Fn(&Repr, &Destination<'_>) -> Result<Transformed, String> + 'static,
    ) -> Self {
        Self {
            spec,
            applies: None,
            run: Rc::new(f),
        }
    }

    /// When it applies: given the representation and the target's name.
    pub fn when(mut self, f: impl Fn(&Repr, &str) -> bool + 'static) -> Self {
        self.applies = Some(Rc::new(f));
        self
    }
}

/// A sanitiser a plugin declares and owns: run on each representation
/// crossing its boundary (of its format, or all), it removes what must
/// not cross and says what it removed.
#[derive(Clone)]
pub struct Sanitiser {
    pub id: String,
    pub boundary: Boundary,
    pub format: Option<FormatId>,
    run: SanitiseFn,
}

impl fmt::Debug for Sanitiser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sanitiser")
            .field("id", &self.id)
            .field("boundary", &self.boundary)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl Sanitiser {
    pub fn new(
        id: &str,
        boundary: Boundary,
        f: impl Fn(&mut Json) -> Vec<String> + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            boundary,
            format: None,
            run: Rc::new(f),
        }
    }

    /// Only for one format.
    pub fn format(mut self, format: FormatId) -> Self {
        self.format = Some(format);
        self
    }
}

/// A component's endpoint: what a copy takes, where a paste goes.
#[derive(Clone)]
struct Endpoint {
    owner: Option<OwnerId>,
    source: Option<Rc<dyn ClipSource>>,
    target: Option<Rc<dyn ClipTarget>>,
    widget: Option<egui::Id>,
    /// An area: any focused widget whose centre lies inside, with the
    /// pass it was bound in.
    area: Option<(egui::Rect, u64)>,
    /// Its elements' widgets: their keys and the pass each was bound in.
    elements: HashMap<egui::Id, (String, u64)>,
}

/// Whether something bound in pass `at` is still current in pass `now`
/// (bound this pass or the one before: a pass may ask before drawing).
fn current(at: u64, now: u64) -> bool {
    now.saturating_sub(at) <= 1
}

/// What the system clipboard holds, as far as the owner knows.
#[derive(Debug, Default)]
struct System {
    /// The format and the marker of what the last copy wrote there (its
    /// text when it wrote some).
    written: Option<(FormatId, u64)>,
    /// The last user copy was internal only: nothing was written.
    internal_only: bool,
    /// The marker of what a paste found there since that copy.
    seen: Option<u64>,
}

/// A representation's marker: its text's, else its data's.
fn mark(r: &Repr) -> u64 {
    match r.text() {
        Some(t) => model::marker(t),
        None => model::marker(&r.data.to_string()),
    }
}

/// The representation a marker is taken from: text when there is some,
/// else the first.
fn marked(reprs: &[Repr]) -> Option<&Repr> {
    reprs.iter().find(|r| r.format == TEXT).or(reprs.first())
}

#[derive(Default)]
struct State {
    connected: Cell<bool>,
    custom: Cell<bool>,
    local: RefCell<LocalClipboard>,
    endpoints: RefCell<BTreeMap<String, Endpoint>>,
    types: RefCell<Vec<(OwnerId, ClipType)>>,
    transforms: RefCell<Vec<(OwnerId, Transform)>>,
    sanitisers: RefCell<Vec<(OwnerId, Sanitiser)>>,
    system: RefCell<System>,
    deferred: RefCell<Vec<Deferred>>,
    backend: RefCell<Option<Box<dyn SystemBackend>>>,
    paste_key: RefCell<PasteKey>,
    /// The last read of the system clipboard, with its pass.
    read_cache: RefCell<Option<(u64, Option<Vec<Repr>>)>>,
}

fn state(app: &AppHandle) -> Rc<State> {
    app.local::<State>()
}

/// Runs `f` on the owner: the host's, else the plugin's
/// `LocalClipboard`. `None` without the plugin, or while the host
/// (owning the clipboard) runs a command.
pub fn with_owner<R>(app: &AppHandle, f: impl FnOnce(&mut dyn Clipboard) -> R) -> Option<R> {
    let st = state(app);
    if !st.connected.get() {
        return None;
    }
    let custom = match app.with_host_mut(|h| h.clipboard().is_some()) {
        Some(c) => {
            st.custom.set(c);
            c
        }
        None => st.custom.get(),
    };
    if custom {
        app.with_host_mut(|h| h.clipboard().map(f)).flatten()
    } else {
        let mut local = st.local.borrow_mut();
        Some(f(&mut *local))
    }
}

/// Lets a session (`agent:<name>`) use the user's slot, when the owner
/// is egui-pop's `LocalClipboard`.
pub fn allow_session(app: &AppHandle, session: &str) {
    state(app).local.borrow_mut().allow(session);
}

/// Registers a component's endpoint by name (replacing one of that
/// name): its source, its target, or both.
pub fn register(
    app: &AppHandle,
    name: &str,
    source: Option<Rc<dyn ClipSource>>,
    target: Option<Rc<dyn ClipTarget>>,
) {
    add_endpoint(app, None, name, source, target);
}

fn add_endpoint(
    app: &AppHandle,
    owner: Option<OwnerId>,
    name: &str,
    source: Option<Rc<dyn ClipSource>>,
    target: Option<Rc<dyn ClipTarget>>,
) {
    let st = state(app);
    let mut eps = st.endpoints.borrow_mut();
    let (widget, area, elements) = eps
        .remove(name)
        .map(|e| (e.widget, e.area, e.elements))
        .unwrap_or_default();
    eps.insert(
        name.into(),
        Endpoint {
            owner,
            source,
            target,
            widget,
            area,
            elements,
        },
    );
}

/// Removes an endpoint.
pub fn unregister(app: &AppHandle, name: &str) {
    state(app).endpoints.borrow_mut().remove(name);
}

/// Says which widget draws an endpoint, each pass it is drawn: while
/// that widget has the focus, the Edit commands use the endpoint.
pub fn bind(app: &AppHandle, name: &str, widget: egui::Id) {
    if let Some(e) = state(app).endpoints.borrow_mut().get_mut(name) {
        e.widget = Some(widget);
    }
}

/// The names of the endpoints registered.
pub fn endpoints(app: &AppHandle) -> Vec<String> {
    state(app).endpoints.borrow().keys().cloned().collect()
}

/// Says that an endpoint covers an area, each pass it is drawn: any
/// focused widget whose centre lies inside it (a node of a graph, a row
/// of a tree, a slider of a list) uses the endpoint. A widget bound
/// alone ([`bind`], [`bind_element`]) wins over an area.
///
/// The focused widget's rectangle is read from egui's lists of widgets,
/// never through `Context::read_response`: building a response for a
/// widget that is not hovered during a press makes egui surrender its
/// focus.
pub fn bind_area(app: &AppHandle, name: &str, area: egui::Rect) {
    let now = app.egui().cumulative_pass_nr();
    if let Some(e) = state(app).endpoints.borrow_mut().get_mut(name) {
        e.area = Some((area, now));
    }
}

/// Says which widget draws one element of an endpoint (a node, a row)
/// and its key, each pass it is drawn: while that widget has the focus,
/// the Edit commands use the endpoint, and its source and target receive
/// the key ([`ClipSource::copy_element`], [`Cut::element`],
/// [`Paste::element`]).
pub fn bind_element(app: &AppHandle, name: &str, widget: egui::Id, key: &str) {
    let now = app.egui().cumulative_pass_nr();
    if let Some(e) = state(app).endpoints.borrow_mut().get_mut(name) {
        e.elements.retain(|_, (_, at)| current(*at, now));
        e.elements.insert(widget, (key.into(), now));
    }
}

/// The key of an endpoint's element whose widget has the focus, if one
/// was bound ([`bind_element`]).
pub fn element(app: &AppHandle, name: &str) -> Option<String> {
    let ctx = app.egui();
    let id = ctx.memory(|m| m.focused())?;
    let now = ctx.cumulative_pass_nr();
    state(app)
        .endpoints
        .borrow()
        .get(name)?
        .elements
        .get(&id)
        .filter(|(_, at)| current(*at, now))
        .map(|(k, _)| k.clone())
}

/// A widget's rectangle from egui's lists of widgets (this pass's, else
/// the last one's), with no side effect on the focus.
fn widget_rect(ctx: &egui::Context, id: egui::Id) -> Option<egui::Rect> {
    ctx.viewport(|v| {
        v.this_pass
            .widgets
            .get(id)
            .or_else(|| v.prev_pass.widgets.get(id))
            .map(|w| w.rect)
    })
}

/// The endpoint whose widget has the focus: one bound to it alone or as
/// an element, else one whose area holds it.
pub fn focused(app: &AppHandle) -> Option<String> {
    let ctx = app.egui();
    let id = ctx.memory(|m| m.focused())?;
    let now = ctx.cumulative_pass_nr();
    let st = state(app);
    let eps = st.endpoints.borrow();
    if let Some((n, _)) = eps.iter().find(|(_, e)| {
        e.widget == Some(id) || e.elements.get(&id).is_some_and(|(_, at)| current(*at, now))
    }) {
        return Some(n.clone());
    }
    let areas: Vec<(&String, egui::Rect)> = eps
        .iter()
        .filter_map(|(n, e)| {
            e.area
                .filter(|(_, at)| current(*at, now))
                .map(|(r, _)| (n, r))
        })
        .collect();
    if areas.is_empty() {
        return None;
    }
    let centre = widget_rect(&ctx, id)?.center();
    // The smallest area holding it: an area nested in another wins.
    areas
        .into_iter()
        .filter(|(_, r)| r.contains(centre))
        .min_by(|a, b| a.1.area().total_cmp(&b.1.area()))
        .map(|(n, _)| n.clone())
}

fn endpoint(app: &AppHandle, name: &str) -> Option<Endpoint> {
    state(app).endpoints.borrow().get(name).cloned()
}

/// The type declared for a format.
pub fn clip_type(app: &AppHandle, format: &FormatId) -> Option<ClipType> {
    state(app)
        .types
        .borrow()
        .iter()
        .find(|(_, t)| t.format == *format)
        .map(|(_, t)| t.clone())
}

/// The transforms declared.
pub fn transforms(app: &AppHandle) -> Vec<TransformSpec> {
    state(app)
        .transforms
        .borrow()
        .iter()
        .map(|(_, t)| t.spec.clone())
        .collect()
}

fn transform(app: &AppHandle, id: &str) -> Option<Transform> {
    state(app)
        .transforms
        .borrow()
        .iter()
        .find(|(_, t)| t.spec.id == id)
        .map(|(_, t)| t.clone())
}

/// Runs the sanitisers of a boundary on representations: what went.
fn sanitise(app: &AppHandle, boundary: Boundary, reprs: &mut [Repr]) -> Vec<Stripped> {
    let sanitisers: Vec<Sanitiser> = state(app)
        .sanitisers
        .borrow()
        .iter()
        .filter(|(_, s)| s.boundary == boundary)
        .map(|(_, s)| s.clone())
        .collect();
    let mut out = Vec::new();
    for r in reprs.iter_mut() {
        for s in sanitisers
            .iter()
            .filter(|s| s.format.as_ref().is_none_or(|f| *f == r.format))
        {
            for what in (s.run)(&mut r.data) {
                out.push(Stripped {
                    by: s.id.clone(),
                    what,
                    boundary,
                    format: r.format.clone(),
                });
            }
        }
    }
    out
}

/// The catalogue key of a type's title: `clip-<slug(format)>.title`.
pub fn type_title_key(format: &FormatId) -> String {
    strings::declared_key(strings::kind::CLIP, format.as_str(), Some("title"))
}

/// What an item is called, in the interface's language: its own message
/// ([`ClipItem::title_key`]), else its type's declared title when the
/// item kept it, else its English title, else its format.
pub fn title_of(app: &AppHandle, item: &ClipItem) -> String {
    let ctx = app.egui();
    if let Some(k) = &item.title_key {
        let args: Vec<(&str, &str)> = k
            .args
            .iter()
            .map(|(a, v)| (a.as_str(), v.as_str()))
            .collect();
        return strings::lookup_args(&ctx, &k.key, &item.title, &args);
    }
    if item.external && item.kind == TEXT {
        return strings::tr(&ctx, &txt::CLIPBOARD_TEXT);
    }
    let ty = clip_type(app, &item.kind);
    match ty {
        Some(t) if item.title.is_empty() || item.title == t.title => {
            strings::lookup(&ctx, &type_title_key(&t.format), &t.title)
        }
        _ if !item.title.is_empty() => item.title.clone(),
        _ => item.kind.to_string(),
    }
}

/// What a format is called, in the interface's language: plain text, a
/// declared type's title, else the format itself.
pub fn format_title(app: &AppHandle, format: &FormatId) -> String {
    let ctx = app.egui();
    if *format == TEXT {
        return strings::tr(&ctx, &txt::CLIPBOARD_TEXT);
    }
    clip_type(app, format).map_or_else(
        || format.to_string(),
        |t| strings::lookup(&ctx, &type_title_key(format), &t.title),
    )
}

/// The types declared, with the plugin owning each (for catalogues:
/// each type's title is a declaration, keyed by [`type_title_key`]).
pub fn types(app: &AppHandle) -> Vec<(OwnerId, ClipType)> {
    state(app).types.borrow().clone()
}

fn capitalised(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn group_begin(app: &AppHandle, origin: &Origin, label: &str) -> Option<GestureId> {
    app.with_host_mut(|h| h.begin_group(origin, label))
        .flatten()
}

fn group_end(app: &AppHandle, group: Option<GestureId>, keep: bool) {
    if let Some(g) = group {
        app.with_host_mut(|h| h.end_group(g, keep));
    }
}

fn publish_events(app: &AppHandle, source: &str, events: Vec<(String, Json)>) {
    for (name, data) in events {
        app.publish(source, None, &name, data);
    }
}

/// A copy, as the `clipboard.copy` step's arguments.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CopyRequest {
    /// The endpoint copied from.
    pub source: String,
    /// The slot written (the origin's own by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<Slot>,
    /// A cut: the source deletes what was copied.
    #[serde(default)]
    pub cut: bool,
    /// Say what would be copied and left out, copy nothing.
    #[serde(default)]
    pub dry_run: bool,
    /// The element copied from (a node, a row); the focused one's when
    /// the Edit commands copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
}

impl CopyRequest {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.into(),
            ..Self::default()
        }
    }

    pub fn cut(mut self) -> Self {
        self.cut = true;
        self
    }

    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    pub fn slot(mut self, slot: Slot) -> Self {
        self.slot = Some(slot);
        self
    }

    /// The element copied from.
    pub fn element(mut self, key: &str) -> Self {
        self.element = Some(key.into());
        self
    }
}

/// A paste, as the `clipboard.paste` step's arguments.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PasteRequest {
    /// The endpoint pasted into.
    pub target: String,
    /// The slot read (the origin's own by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<Slot>,
    /// The format the target should receive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<FormatId>,
    /// The transform to go through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<String>,
    /// Only these parts of the item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<String>>,
    /// Refused unless the owner is at this revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_rev: Option<u64>,
    /// Say what would be pasted and changed, change nothing.
    #[serde(default)]
    pub dry_run: bool,
    /// What the system clipboard held, when the paste key brought it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The element pasted into (a node, a row); the focused one's when
    /// the Edit commands paste.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
    /// The paste key found no text on the system clipboard (it holds
    /// files, an image, or nothing): what it holds is read through the
    /// system backend, and a paste is refused when the backend reads
    /// nothing while the system clipboard no longer holds what the owner
    /// wrote there.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_text: bool,
}

impl PasteRequest {
    pub fn new(target: &str) -> Self {
        Self {
            target: target.into(),
            ..Self::default()
        }
    }

    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    pub fn format(mut self, format: FormatId) -> Self {
        self.format = Some(format);
        self
    }

    pub fn transform(mut self, id: &str) -> Self {
        self.transform = Some(id.into());
        self
    }

    pub fn parts(mut self, parts: &[&str]) -> Self {
        self.parts = Some(parts.iter().map(|p| p.to_string()).collect());
        self
    }

    pub fn if_rev(mut self, rev: u64) -> Self {
        self.if_rev = Some(rev);
        self
    }

    /// The text the system clipboard held (the paste key's).
    pub fn text(mut self, text: &str) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn slot(mut self, slot: Slot) -> Self {
        self.slot = Some(slot);
        self
    }

    /// The element pasted into.
    pub fn element(mut self, key: &str) -> Self {
        self.element = Some(key.into());
        self
    }
}

/// Copies through the [`STEP_COPY`] step. A refusal or a veto asked by
/// the user shows in a toast, and so does what was left out.
pub fn copy(app: &AppHandle, request: CopyRequest, origin: &Origin) -> Outcome {
    let mut args = serde_json::to_value(&request).unwrap_or(Json::Null);
    args["origin"] = json!(source_of(origin));
    let who = origin.clone();
    let out = app.step(&STEP_COPY, args, move |app, args| {
        let req: CopyRequest = parse(args, "a copy")?;
        run_copy(app, &req, &who)
    });
    if *origin == Origin::User {
        match &out {
            Outcome::Done(Ok(v)) => {
                let what: Vec<&str> = v["stripped"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|s| s["what"].as_str()).collect())
                    .unwrap_or_default();
                if !what.is_empty() {
                    toast(
                        app,
                        &txt::CLIPBOARD_COPIED_STRIPPED,
                        "what",
                        &what.join(", "),
                        false,
                    );
                }
            }
            Outcome::Done(Err(e)) => toast(app, &txt::CLIPBOARD_NOT_COPIED, "reason", e, true),
            Outcome::Vetoed { reason, .. } => {
                toast(app, &txt::CLIPBOARD_NOT_COPIED, "reason", reason, true);
            }
        }
    }
    out
}

fn run_copy(app: &AppHandle, req: &CopyRequest, who: &Origin) -> Result<Json, String> {
    let slot = req.slot.clone().unwrap_or_else(|| Slot::of(who));
    with_owner(app, |c| c.may_use(who, &slot)).ok_or(NO_PLUGIN)??;
    let ep = endpoint(app, &req.source).ok_or_else(|| format!("no endpoint `{}`", req.source))?;
    let source = ep
        .source
        .ok_or_else(|| format!("nothing can be copied from `{}`", req.source))?;
    // What the source writes (a cut's delete) is its plugin's own.
    let _scope = ep.owner.as_ref().map(egui_pop_model::values::enter_scope);
    if req.cut && !source.can_cut(app) {
        return Err(format!("`{}` cannot be cut", req.source));
    }
    let mut item = source
        .copy_element(app, req.element.as_deref())
        .ok_or_else(|| "nothing is selected to copy".to_string())?;
    let ty = clip_type(app, &item.kind);
    if item.title.is_empty() {
        item.title = ty.as_ref().map(|t| t.title.clone()).unwrap_or_default();
    }
    if item.fallbacks.is_empty()
        && let Some(render) = ty.as_ref().and_then(|t| t.render.clone())
    {
        item.fallbacks = render(&item.payload);
    }
    item.origin.by = source_of(who);
    item.origin.source = req.source.clone();
    let place = app.context().get("place").to_string();
    item.origin.place = (!place.is_empty()).then_some(place);
    // Inside the owner: its own sanitisers only (none by default).
    let mut own = [Repr::new(item.kind.clone(), item.payload.clone())];
    let mut stripped = sanitise(app, Boundary::Owner, &mut own);
    let [own] = own;
    item.payload = own.data;
    // Towards the system clipboard: the fallbacks, sanitised, and the
    // private type itself where the backend writes it.
    let internal = ty.as_ref().is_some_and(|t| t.internal);
    let to_system = slot == Slot::User && !internal;
    let mut system_out: Vec<Repr> = Vec::new();
    if to_system {
        if with_backend(app, |b| b.writes(&item.kind)) {
            system_out.push(Repr::new(item.kind.clone(), item.payload.clone()));
        }
        system_out.extend(item.fallbacks.iter().cloned());
        stripped.extend(sanitise(app, Boundary::System, &mut system_out));
        system_out.retain(|r| with_backend(app, |b| b.writes(&r.format)));
    }
    item.stripped = stripped;
    let summary = json!({
        "source": req.source,
        "element": req.element,
        "slot": slot,
        "kind": item.kind,
        "title": item.title,
        "offers": item.offers(),
        "stripped": item.stripped,
        "system": to_system,
        "cut": req.cut,
        "dry_run": req.dry_run,
    });
    if req.dry_run {
        return Ok(summary);
    }
    let group = if req.cut {
        let label = strings::tr_args(
            &app.egui(),
            &txt::CLIPBOARD_CUT_STEP,
            &[("title", &title_of(app, &item))],
        );
        let g = group_begin(app, who, &label);
        if let Err(e) = source.delete(
            app,
            &Cut {
                item: &item,
                origin: who,
                group: g,
                element: req.element.as_deref(),
            },
        ) {
            group_end(app, g, false);
            return Err(e);
        }
        g
    } else {
        None
    };
    let (answer, events) = with_owner(app, |c| {
        let a = c.put(&slot, item.clone(), who);
        (a, c.take_events())
    })
    .ok_or(NO_PLUGIN)?;
    publish_events(app, &source_of(who), events);
    let rev = match answer {
        Answer::Done(Ok(rev)) => json!(rev),
        Answer::Done(Err(e)) => {
            // A cut copies only once kept: its delete is put back.
            group_end(app, group, false);
            return Err(e);
        }
        Answer::Later(_) => json!(null),
    };
    group_end(app, group, true);
    let mut summary = summary;
    summary["rev"] = rev;
    if slot == Slot::User {
        let written = marked(&system_out).map(|r| (r.format.clone(), mark(r)));
        let ctx = app.egui();
        let wrote = match &written {
            Some(_) => with_backend(app, |b| b.write(&ctx, &system_out)),
            None => Ok(()),
        };
        let st = state(app);
        *st.read_cache.borrow_mut() = None;
        *st.system.borrow_mut() = match (&written, &wrote) {
            (Some(_), Ok(())) => System {
                written,
                ..System::default()
            },
            // Nothing written (an internal type, or the backend failed):
            // the system clipboard keeps what it held.
            _ => System {
                internal_only: true,
                ..System::default()
            },
        };
        if let Err(e) = wrote {
            log::warn!("the system clipboard was not written: {e}");
            summary["system_error"] = json!(e);
        }
    }
    Ok(summary)
}

/// A paste made ready: the item, the plan, the representation.
struct Ready {
    target: Rc<dyn ClipTarget>,
    /// The plugin that registered the target: its writes are its own.
    owner: Option<OwnerId>,
    item: ClipItem,
    internal: bool,
    plan: Plan,
    repr: Repr,
    lost: Option<String>,
    stripped: Vec<Stripped>,
    element: Option<String>,
}

/// The item a paste takes: the owner's, or the external text the paste
/// key brought when the system clipboard no longer holds what the owner
/// wrote there.
fn current_item(
    app: &AppHandle,
    slot: &Slot,
    text: Option<&str>,
    no_text: bool,
) -> Result<(ClipItem, bool), String> {
    let internal = with_owner(app, |c| c.get(slot)).ok_or(NO_PLUGIN)?;
    let owners = |internal: Option<ClipItem>| {
        internal
            .map(|i| (i, true))
            .ok_or_else(|| "the clipboard is empty".to_string())
    };
    if *slot != Slot::User {
        return owners(internal);
    }
    // What the system clipboard holds: what the backend reads (every
    // format), else the text the paste key brought.
    let read = system_read(app);
    let reprs = match (read, text) {
        (Some(r), _) if !r.is_empty() => r,
        (_, Some(t)) => vec![Repr::new(TEXT, Json::String(t.into()))],
        // The paste key found no text, nothing reads the rest, and what
        // the owner wrote there is gone: something else is there.
        (None, None) if no_text && state(app).system.borrow().written.is_some() => {
            return Err(
                "the system clipboard holds no text, and the clipboard backend reads nothing else"
                    .into(),
            );
        }
        _ => return owners(internal),
    };
    let st = state(app);
    let mut sys = st.system.borrow_mut();
    let ours = internal.is_some()
        && match &sys.written {
            Some((format, m)) => reprs
                .iter()
                .find(|r| r.format == *format)
                .is_some_and(|r| mark(r) == *m),
            None => {
                let m = marked(&reprs).map(mark);
                let ours = sys.internal_only && (sys.seen.is_none() || sys.seen == m);
                if sys.internal_only {
                    sys.seen = m;
                }
                ours
            }
        };
    match internal {
        Some(item) if ours => Ok((item, true)),
        _ => ClipItem::external(reprs)
            .map(|i| (i, false))
            .ok_or_else(|| "the clipboard is empty".into()),
    }
}

fn prepare(app: &AppHandle, req: &PasteRequest, who: &Origin) -> Result<Ready, String> {
    let slot = req.slot.clone().unwrap_or_else(|| Slot::of(who));
    with_owner(app, |c| c.may_use(who, &slot)).ok_or(NO_PLUGIN)??;
    if let Some(want) = req.if_rev {
        let rev = with_owner(app, |c| c.rev()).ok_or(NO_PLUGIN)?;
        if rev != want {
            return Err(format!("the clipboard is at revision {rev}, not {want}"));
        }
    }
    let ep = endpoint(app, &req.target).ok_or_else(|| format!("no endpoint `{}`", req.target))?;
    let target = ep
        .target
        .ok_or_else(|| format!("nothing can be pasted into `{}`", req.target))?;
    let (mut item, internal) = current_item(app, &slot, req.text.as_deref(), req.no_text)?;
    // An item made at an older version of its type is migrated first.
    if let Some(t) = clip_type(app, &item.kind)
        && item.version < t.version
        && let Some(m) = &t.migrate
    {
        item.payload = m(item.version, item.payload.clone())?;
        item.version = t.version;
    }
    // External content is sanitised before any target sees it.
    let mut stripped = Vec::new();
    if item.external {
        let mut reprs: Vec<Repr> =
            std::iter::once(Repr::new(item.kind.clone(), item.payload.clone()))
                .chain(item.fallbacks.iter().cloned())
                .collect();
        stripped = sanitise(app, Boundary::External, &mut reprs);
        let mut reprs = reprs.into_iter();
        if let Some(first) = reprs.next() {
            item.payload = first.data;
        }
        item.fallbacks = reprs.collect();
    }
    let accepts = target.accepts(app);
    let specs: Vec<TransformSpec> = state(app)
        .transforms
        .borrow()
        .iter()
        .filter(|(_, t)| {
            item.repr(&t.spec.from)
                .is_some_and(|r| t.applies.as_ref().is_none_or(|f| f(&r, &req.target)))
        })
        .map(|(_, t)| t.spec.clone())
        .collect();
    let plan = model::choose(
        &item.offers(),
        &accepts,
        &specs,
        req.format.as_ref(),
        req.transform.as_deref(),
    )
    .map_err(|r| {
        if r.offered.is_empty() || req.format.is_some() || req.transform.is_some() {
            r.reason
        } else {
            format!(
                "{} cannot be pasted here ({})",
                capitalised(&title_of(app, &item)),
                r.reason
            )
        }
    })?;
    let mut repr = item
        .repr(&plan.format)
        .ok_or_else(|| format!("the item has no `{}`", plan.format))?;
    let mut lost = None;
    if let Some(id) = &plan.transform {
        let t = transform(app, id).ok_or_else(|| format!("no transform `{id}`"))?;
        let context = target.context(app, req.element.as_deref());
        let dest = Destination {
            target: &req.target,
            element: req.element.as_deref(),
            context: &context,
        };
        let out = (t.run)(&repr, &dest)?;
        repr = Repr::new(plan.to.clone(), out.data);
        lost = out
            .lost
            .or_else(|| t.spec.lossy.then(|| "some information".into()));
    }
    Ok(Ready {
        target,
        owner: ep.owner,
        item,
        internal,
        plan,
        repr,
        lost,
        stripped,
        element: req.element.clone(),
    })
}

/// Pastes through the [`STEP_PASTE`] step. A refusal or a veto asked by
/// the user shows in a toast, and so does what a lossy transform lost.
pub fn paste(app: &AppHandle, request: PasteRequest, origin: &Origin) -> Outcome {
    *state(app).read_cache.borrow_mut() = None;
    let mut args = serde_json::to_value(&request).unwrap_or(Json::Null);
    args["origin"] = json!(source_of(origin));
    let who = origin.clone();
    let out = app.step(&STEP_PASTE, args, move |app, args| {
        let req: PasteRequest = parse(args, "a paste")?;
        run_paste(app, &req, &who)
    });
    if *origin == Origin::User {
        match &out {
            Outcome::Done(Ok(v)) => {
                // A deferred paste says its loss once it is made.
                if let (Some(lost), false) = (v["lost"].as_str(), v["pending"] == json!(true)) {
                    toast(app, &txt::CLIPBOARD_PASTED_LOSSY, "what", lost, false);
                }
            }
            Outcome::Done(Err(e)) => toast(app, &txt::CLIPBOARD_NOT_PASTED, "reason", e, true),
            Outcome::Vetoed { reason, .. } => {
                toast(app, &txt::CLIPBOARD_NOT_PASTED, "reason", reason, true);
            }
        }
    }
    out
}

fn run_paste(app: &AppHandle, req: &PasteRequest, who: &Origin) -> Result<Json, String> {
    let ready = prepare(app, req, who)?;
    // What the target writes is its plugin's own.
    let _scope = ready
        .owner
        .as_ref()
        .map(egui_pop_model::values::enter_scope);
    let group = if req.dry_run {
        None
    } else {
        let label = strings::tr_args(
            &app.egui(),
            &txt::CLIPBOARD_PASTE_STEP,
            &[("title", &title_of(app, &ready.item))],
        );
        group_begin(app, who, &label)
    };
    let paste = Paste {
        repr: &ready.repr,
        plan: &ready.plan,
        item: &ready.item,
        parts: req.parts.as_deref(),
        dry_run: req.dry_run,
        origin: who,
        group,
        element: req.element.as_deref(),
    };
    let answer = if req.dry_run {
        Pasted::Done(ready.target.paste(app, &paste))
    } else {
        ready.target.apply(app, &paste)
    };
    let (changes, later) = match answer {
        Pasted::Done(changes) => {
            group_end(app, group, changes.is_ok());
            (changes?, None)
        }
        Pasted::Later(p) => (Json::Null, Some(p)),
    };
    let summary = json!({
        "target": req.target,
        "element": req.element,
        "kind": ready.item.kind,
        "title": ready.item.title,
        "internal": ready.internal,
        "format": ready.plan.format,
        "to": ready.plan.to,
        "transform": ready.plan.transform,
        "lossy": ready.plan.lossy,
        "lost": ready.lost,
        "stripped": ready.stripped,
        "parts": req.parts,
        "changes": changes,
        "dry_run": req.dry_run,
        "pending": later.is_some(),
    });
    if let Some(pending) = later {
        state(app).deferred.borrow_mut().push(Deferred {
            pending,
            group,
            origin: who.clone(),
            summary: summary.clone(),
        });
        app.egui().request_repaint();
    }
    Ok(summary)
}

/// How many pastes wait for their result ([`Pasted::Later`]).
pub fn pending_pastes(app: &AppHandle) -> usize {
    state(app).deferred.borrow().len()
}

/// Each pass: the deferred pastes whose result arrived close their undo
/// group (kept when they succeeded), publish `clipboard.pasted` (the
/// paste's summary with its `changes`, or its `error`) and, for the
/// user, show a failure or a loss.
fn settle(app: &AppHandle) {
    let arrived: Vec<(Deferred, Result<Json, String>)> = {
        let st = state(app);
        let mut list = st.deferred.borrow_mut();
        let mut out = Vec::new();
        let mut i = 0;
        while i < list.len() {
            match list[i].pending.take() {
                Some(r) => out.push((list.remove(i), r)),
                None => i += 1,
            }
        }
        out
    };
    for (d, result) in arrived {
        group_end(app, d.group, result.is_ok());
        let mut data = d.summary;
        data["pending"] = json!(false);
        match &result {
            Ok(changes) => data["changes"] = changes.clone(),
            Err(e) => data["error"] = json!(e),
        }
        app.publish(
            &source_of(&d.origin),
            None,
            "clipboard.pasted",
            data.clone(),
        );
        if d.origin == Origin::User {
            match &result {
                Ok(_) => {
                    if let Some(lost) = data["lost"].as_str() {
                        toast(app, &txt::CLIPBOARD_PASTED_LOSSY, "what", lost, false);
                    }
                }
                Err(e) => toast(app, &txt::CLIPBOARD_NOT_PASTED, "reason", e, true),
            }
        }
    }
}

/// Every way the clipboard fits a target, as paste special lists them:
/// the plan, its label, the target's preview (a dry run), and the parts.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Way {
    pub plan: Plan,
    pub label: String,
    pub preview: Option<String>,
    /// Only these parts (none: the whole item).
    pub parts: Option<Vec<String>>,
}

/// The ways a slot's item fits a target (`text`: what the paste key
/// brought): each format and transform, then each part alone.
pub fn ways(
    app: &AppHandle,
    target: &str,
    slot: Option<Slot>,
    text: Option<&str>,
    origin: &Origin,
) -> Result<Vec<Way>, String> {
    let base = PasteRequest {
        target: target.into(),
        slot,
        text: text.map(Into::into),
        ..PasteRequest::default()
    };
    ways_for(app, &base, origin)
}

/// The ways a request's item fits its target (its `target`, `slot`,
/// `text` and `element`; its format, transform and parts ignored).
pub fn ways_for(
    app: &AppHandle,
    request: &PasteRequest,
    origin: &Origin,
) -> Result<Vec<Way>, String> {
    *state(app).read_cache.borrow_mut() = None;
    let base = PasteRequest {
        target: request.target.clone(),
        slot: request.slot.clone(),
        text: request.text.clone(),
        element: request.element.clone(),
        no_text: request.no_text,
        dry_run: true,
        ..PasteRequest::default()
    };
    let first = prepare(app, &base, origin)?;
    let specs: Vec<TransformSpec> = transforms(app);
    let accepts = first.target.accepts(app);
    let ctx = app.egui();
    let mut out = Vec::new();
    for plan in model::candidates(&first.item.offers(), &accepts, &specs) {
        let mut req = base.clone();
        req.format = Some(plan.to.clone());
        req.transform = plan.transform.clone();
        let Ok(ready) = prepare(app, &req, origin) else {
            continue;
        };
        let to = format_title(app, &plan.to);
        let mut label = match &plan.transform {
            Some(t) => strings::tr_args(
                &ctx,
                &txt::CLIPBOARD_AS_THROUGH,
                &[("format", &to), ("transform", t)],
            ),
            None => strings::tr_args(&ctx, &txt::CLIPBOARD_AS, &[("format", &to)]),
        };
        if let Some(lost) = &ready.lost {
            label = format!(
                "{label} ({})",
                strings::tr_args(&ctx, &txt::CLIPBOARD_LOSES, &[("what", lost)])
            );
        }
        let preview = preview(app, &ready, None, origin);
        out.push(Way {
            plan,
            label,
            preview,
            parts: None,
        });
    }
    let parts = clip_type(app, &first.item.kind)
        .and_then(|t| t.parts.clone())
        .map(|f| f(&first.item.payload))
        .unwrap_or_default();
    if let Some(best) = out.first().cloned() {
        for part in parts {
            let ids = vec![part.id.clone()];
            out.push(Way {
                plan: best.plan.clone(),
                label: strings::tr_args(&ctx, &txt::CLIPBOARD_ONLY_PART, &[("part", &part.title)]),
                preview: preview(app, &first, Some(&ids), origin),
                parts: Some(ids),
            });
        }
    }
    Ok(out)
}

fn preview(
    app: &AppHandle,
    ready: &Ready,
    parts: Option<&[String]>,
    origin: &Origin,
) -> Option<String> {
    let _scope = ready
        .owner
        .as_ref()
        .map(egui_pop_model::values::enter_scope);
    let v = ready
        .target
        .paste(
            app,
            &Paste {
                repr: &ready.repr,
                plan: &ready.plan,
                item: &ready.item,
                parts,
                dry_run: true,
                origin,
                group: None,
                element: ready.element.as_deref(),
            },
        )
        .ok()?;
    match v {
        Json::Null => None,
        Json::String(s) => Some(s),
        other => Some(other.to_string()),
    }
}

/// Opens paste special for a target: a choice of every way, each with
/// its preview; choosing pastes it.
pub fn paste_special(
    app: &AppHandle,
    target: &str,
    text: Option<&str>,
    origin: &Origin,
) -> Result<Json, String> {
    let mut req = PasteRequest::new(target);
    req.text = text.map(Into::into);
    paste_special_for(app, &req, origin)
}

/// Opens paste special for a request (its `target`, `text` and
/// `element`).
pub fn paste_special_for(
    app: &AppHandle,
    request: &PasteRequest,
    origin: &Origin,
) -> Result<Json, String> {
    let target = request.target.as_str();
    let text = request.text.as_deref();
    let dialogs = app
        .service(&crate::dialogs::DIALOGS)
        .ok_or("paste special needs the dialogs plugin")?;
    let ways = ways_for(app, request, origin)?;
    if ways.is_empty() {
        return Err("nothing on the clipboard fits here".into());
    }
    let ctx = app.egui();
    let options = ways
        .iter()
        .map(|w| {
            let value = json!({
                "format": w.plan.to,
                "transform": w.plan.transform,
                "parts": w.parts,
            });
            let o = crate::dialogs::ChooseOption::new(&w.label, value);
            match &w.preview {
                Some(p) => o.hint(p),
                None => o,
            }
        })
        .collect();
    let choose = crate::dialogs::Choose::with_options(
        &strings::tr(&ctx, &txt::CLIPBOARD_PASTE_SPECIAL),
        options,
        CMD_CLIP_PASTE,
    )
    .args(json!({
        "target": target,
        "text": text,
        "element": request.element,
        "no_text": request.no_text,
    }))
    .value_key("special");
    let id = dialogs.open(choose);
    Ok(json!({ "dialog": id, "ways": ways.len() }))
}

fn parse<T: serde::de::DeserializeOwned>(args: &Json, what: &str) -> Result<T, String> {
    serde_json::from_value(args.clone()).map_err(|e| format!("{what}: {e}"))
}

fn outcome_json(o: Outcome) -> Result<Json, String> {
    match o {
        Outcome::Done(r) => r,
        Outcome::Vetoed { by, reason } => Err(format!("vetoed by {by}: {reason}")),
    }
}

fn toast(app: &AppHandle, key: &strings::StrKey, name: &str, value: &str, warning: bool) {
    if let Some(t) = app.service(&crate::toasts::TOASTS) {
        let text = strings::tr_args(&app.egui(), key, &[(name, value)]);
        t.notify(if warning {
            crate::toasts::Toast::warning(&text)
        } else {
            crate::toasts::Toast::info(&text)
        });
    }
}

/// The context keys `canCopy`, `canCut` and `canPaste` for the focused
/// endpoint (`None` without the plugin). A paste counts text as offered:
/// the system clipboard may hold some.
pub(crate) fn keys(app: &AppHandle) -> Option<[(&'static str, bool); 3]> {
    let st = state(app);
    if !st.connected.get() {
        return None;
    }
    let ep = focused(app).and_then(|n| endpoint(app, &n).map(|e| (n, e)));
    let Some((name, ep)) = ep else {
        return Some([("canCopy", false), ("canCut", false), ("canPaste", false)]);
    };
    let (copy, cut) = match &ep.source {
        Some(s) => {
            let copy = s.can_copy(app);
            (copy, copy && s.can_cut(app))
        }
        None => (false, false),
    };
    let paste = ep.target.as_ref().is_some_and(|t| {
        let mut offers = with_owner(app, |c| c.offers(&Slot::User)).unwrap_or_default();
        for f in std::iter::once(TEXT).chain(with_backend(app, |b| b.readable())) {
            if !offers.contains(&f) {
                offers.push(f);
            }
        }
        let item = with_owner(app, |c| c.get(&Slot::User)).flatten();
        let specs: Vec<TransformSpec> = st
            .transforms
            .borrow()
            .iter()
            .filter(|(_, t)| {
                let r = item
                    .as_ref()
                    .and_then(|i| i.repr(&t.spec.from))
                    .unwrap_or_else(|| Repr::new(t.spec.from.clone(), Json::Null));
                t.applies.as_ref().is_none_or(|f| f(&r, &name))
            })
            .map(|(_, t)| t.spec.clone())
            .collect();
        model::negotiate(&offers, &t.accepts(app), &specs).is_ok()
    });
    Some([("canCopy", copy), ("canCut", cut), ("canPaste", paste)])
}

/// Each pass: the copy, cut and paste events egui-winit made of the
/// platform's chords go to the commands while an endpoint has the focus
/// (and no text field does); Shift held on a paste is paste special.
fn pass(app: &AppHandle) {
    // upstream: egui-winit sends these chords as events and swallows
    // their keys (docs/upstream.md).
    settle(app);
    let ctx = app.egui();
    let silent = watch_paste_key(app, &ctx);
    if ctx.text_edit_focused() || focused(app).is_none() {
        return;
    }
    let mut runs = Vec::new();
    if let Some(shift) = silent {
        let cmd = if shift { CMD_PASTE_SPECIAL } else { CMD_PASTE };
        runs.push((cmd, json!({ "no_text": true })));
    }
    ctx.input_mut(|i| {
        let shift = i.modifiers.shift;
        i.events.retain(|e| match e {
            egui::Event::Copy => {
                runs.push((CMD_COPY, Json::Null));
                false
            }
            egui::Event::Cut => {
                runs.push((CMD_CUT, Json::Null));
                false
            }
            egui::Event::Paste(text) => {
                let cmd = if shift { CMD_PASTE_SPECIAL } else { CMD_PASTE };
                runs.push((cmd, json!({ "text": text })));
                false
            }
            _ => true,
        });
    });
    for (cmd, args) in runs {
        app.run_as(cmd, args, Origin::User);
    }
}

/// What the paste key did, as far as egui's events tell.
#[derive(Debug, Default)]
struct PasteKey {
    /// Its press arrived as a key (not a paste chord).
    down: bool,
    /// egui-winit made a paste of it (the system clipboard held text).
    pasted: bool,
    /// The command modifier was held since its last release.
    command: bool,
}

fn is_paste_key(key: egui::Key, modifiers: egui::Modifiers) -> bool {
    key == egui::Key::Paste
        || key == egui::Key::V
        || (cfg!(target_os = "windows") && modifiers.shift && key == egui::Key::Insert)
}

/// The paste chord egui-winit swallowed without an event: it makes
/// `Event::Paste` only when the system clipboard holds text, and drops
/// the key's press either way, but lets its release through. A release
/// of the paste key whose press egui never saw, with no paste since and
/// the command modifier held meanwhile, is that chord: the paste key
/// with no text on the system clipboard (files, an image). Gives
/// whether Shift was held (paste special) when it is.
///
/// Reads the events only while a pass runs: between passes (eframe's
/// ticks of a hidden window) the input is still the last pass's, read
/// already, and a second read of a release would take it for one whose
/// press egui never saw.
fn watch_paste_key(app: &AppHandle, ctx: &egui::Context) -> Option<bool> {
    crate::app::pass_in_progress(ctx)?;
    let st = state(app);
    let mut k = st.paste_key.borrow_mut();
    ctx.input(|i| {
        if i.modifiers.command {
            k.command = true;
        }
        let mut silent = None;
        for e in &i.events {
            match e {
                egui::Event::Paste(_) => k.pasted = true,
                egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    ..
                } if is_paste_key(*key, *modifiers) => {
                    if *pressed {
                        k.down = true;
                    } else {
                        let chord = *key != egui::Key::V || modifiers.command || k.command;
                        if !k.down && !k.pasted && chord {
                            silent = Some(modifiers.shift);
                        }
                        *k = PasteKey::default();
                    }
                }
                _ => {}
            }
        }
        silent
    })
}

/// The endpoint a command acts on: `args[key]`, else the focused one.
fn endpoint_arg(app: &AppHandle, args: &Json, key: &str) -> Result<String, String> {
    match args.get(key).and_then(Json::as_str) {
        Some(n) => Ok(n.to_string()),
        None => focused(app).ok_or_else(|| "no endpoint has the focus".into()),
    }
}

/// The element a command acts on: `args.element`, else the focused
/// element of the endpoint.
fn element_arg(app: &AppHandle, args: &Json, endpoint: &str) -> Option<String> {
    match args.get("element").and_then(Json::as_str) {
        Some(k) => Some(k.to_string()),
        None => element(app, endpoint),
    }
}

fn slot_arg(args: &Json, origin: &Origin) -> Slot {
    args.get("slot")
        .and_then(Json::as_str)
        .map_or_else(|| Slot::of(origin), Slot::parse)
}

/// A paste request from a command's arguments, with paste special's
/// choice (`special`) merged in.
fn paste_args(args: &Json, target: String) -> Result<PasteRequest, String> {
    let mut a = match args {
        Json::Object(m) => m.clone(),
        _ => serde_json::Map::new(),
    };
    if let Some(Json::Object(s)) = a.remove("special") {
        a.extend(s);
    }
    a.retain(|_, v| !v.is_null());
    a.insert("target".into(), json!(target));
    for k in ["choice", "label"] {
        a.remove(k);
    }
    parse(&Json::Object(a), "a paste")
}

fn describe(app: &AppHandle) -> Json {
    let st = state(app);
    let slots: Vec<Json> = {
        let local = st.local.borrow();
        if st.custom.get() {
            Vec::new()
        } else {
            local
                .items
                .iter()
                .map(|(s, i)| {
                    json!({
                        "slot": s,
                        "kind": i.kind,
                        "title": i.title,
                        "version": i.version,
                        "offers": i.offers(),
                        "stripped": i.stripped,
                        "external": i.external,
                        "origin": i.origin,
                    })
                })
                .collect()
        }
    };
    let focused = focused(app);
    let endpoints: Vec<Json> = st
        .endpoints
        .borrow()
        .iter()
        .map(|(n, e)| {
            json!({
                "name": n,
                "owner": e.owner,
                "source": e.source.is_some(),
                "target": e.target.is_some(),
                "bound": e.widget.is_some(),
                "area": e.area.map(|(r, _)| [r.min.x, r.min.y, r.max.x, r.max.y]),
                "elements": e.elements.len(),
                "focused": focused.as_deref() == Some(n.as_str()),
            })
        })
        .collect();
    json!({
        "rev": with_owner(app, |c| c.rev()),
        "owner": if st.custom.get() { "host" } else { "local" },
        "pending_pastes": st.deferred.borrow().len(),
        "system": with_backend(app, |b| json!({ "backend": b.name(), "about": b.describe() })),
        "slots": slots,
        "endpoints": endpoints,
        "types": st.types.borrow().iter().map(|(o, t)| json!({
            "format": t.format, "version": t.version, "title": t.title,
            "internal": t.internal, "owner": o,
        })).collect::<Vec<_>>(),
        "transforms": st.transforms.borrow().iter().map(|(o, t)| json!({
            "spec": t.spec, "owner": o,
        })).collect::<Vec<_>>(),
        "sanitisers": st.sanitisers.borrow().iter().map(|(o, s)| json!({
            "id": s.id, "boundary": s.boundary, "format": s.format, "owner": o,
        })).collect::<Vec<_>>(),
    })
}

/// The clipboard as a plugin: the default owner (a [`LocalClipboard`],
/// unless the host owns the clipboard), the steps, the Edit commands and
/// keys, the agent commands, the `clipboard` collector, egui-pop's own
/// sanitiser of external text and its HTML to text transform.
#[derive(Debug, Default)]
pub struct ClipboardPlugin;

impl Plugin for ClipboardPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Clipboard".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        state(&ctx.app()).connected.set(true);
        ctx.step(
            StepSpec::new(
                STEP_COPY,
                "A copy from an endpoint; a veto keeps it from being made, a redirect copies another source or into another slot.",
            )
            .args(json!({ "source": "demo.swatches", "slot": "user", "cut": false, "dry_run": false, "origin": "user" })),
        )?
        .step(
            StepSpec::new(
                STEP_PASTE,
                "A paste into an endpoint; a veto keeps it from being made, a redirect pastes into another target or in another format.",
            )
            .args(json!({ "target": "demo.swatches", "format": null, "transform": null, "dry_run": false, "origin": "user" })),
        )?;
        let category = "Edit";
        ctx.command(
            CommandSpec::new(CMD_CUT, "Cut", category)
                .description("Cut the selection: copied once its delete is accepted.")
                .key("Ctrl+X")
                .key("Shift+Delete")
                .when("!textInputFocus && canCut")
                .menu(MENU_EDIT, MENU_GROUP, 0),
            |call| {
                let source = endpoint_arg(call.app, call.args, "source")?;
                let mut req = CopyRequest::new(&source).cut();
                req.element = element_arg(call.app, call.args, &source);
                outcome_json(copy(call.app, req, call.origin))
            },
        )?
        .command(
            CommandSpec::new(CMD_COPY, "Copy", category)
                .description("Copy the selection.")
                .key("Ctrl+C")
                .key("Ctrl+Insert")
                .when("!textInputFocus && canCopy")
                .menu(MENU_EDIT, MENU_GROUP, 1),
            |call| {
                let source = endpoint_arg(call.app, call.args, "source")?;
                let mut req = CopyRequest::new(&source);
                req.element = element_arg(call.app, call.args, &source);
                outcome_json(copy(call.app, req, call.origin))
            },
        )?
        .command(
            CommandSpec::new(CMD_PASTE, "Paste", category)
                .description("Paste in the richest format the focused element accepts.")
                .key("Ctrl+V")
                .key("Shift+Insert")
                .when("!textInputFocus && canPaste")
                .menu(MENU_EDIT, MENU_GROUP, 2),
            |call| {
                let target = endpoint_arg(call.app, call.args, "target")?;
                let mut req = paste_args(call.args, target)?;
                if req.element.is_none() {
                    req.element = element(call.app, &req.target);
                }
                outcome_json(paste(call.app, req, call.origin))
            },
        )?
        .command(
            CommandSpec::new(CMD_PASTE_SPECIAL, "Paste special", category)
                .description("Choose the format, the transform or the parts to paste, each with its preview.")
                .key("Ctrl+Shift+V")
                .when("!textInputFocus && canPaste")
                .menu(MENU_EDIT, MENU_GROUP, 3),
            |call| {
                let target = endpoint_arg(call.app, call.args, "target")?;
                let mut req = paste_args(call.args, target)?;
                if req.element.is_none() {
                    req.element = element(call.app, &req.target);
                }
                if req.format.is_some() || req.transform.is_some() || req.parts.is_some() {
                    return outcome_json(paste(call.app, req, call.origin));
                }
                paste_special_for(call.app, &req, call.origin)
            },
        )?;
        let category = "Clipboard";
        ctx.command(
            CommandSpec::new(CMD_CLIP_COPY, "Copy from an endpoint", category)
                .description("Copies from an endpoint (`source`), one of its elements (`element`), into the session's slot (`slot`); `cut` deletes what was copied; `dry_run` says what would be copied and left out.")
                .args(json!({ "source": "demo.swatches", "cut": false, "dry_run": true })),
            |call| {
                let mut req: CopyRequest = parse(call.args, "a copy")?;
                req.slot = Some(slot_arg(call.args, call.origin));
                outcome_json(copy(call.app, req, call.origin))
            },
        )?
        .command(
            CommandSpec::new(CMD_CLIP_PASTE, "Paste into an endpoint", category)
                .description("Pastes the session's slot (`slot`) into an endpoint (`target`), one of its elements (`element`), in a `format`, through a `transform`, only some `parts`, checked against `if_rev`; `dry_run` says the plan and the changes without making them.")
                .args(json!({ "target": "demo.swatches", "format": null, "transform": null, "if_rev": null, "dry_run": true })),
            |call| {
                let target = call.args["target"]
                    .as_str()
                    .ok_or("`target` is missing")?
                    .to_string();
                let mut req = paste_args(call.args, target)?;
                req.slot = Some(slot_arg(call.args, call.origin));
                outcome_json(paste(call.app, req, call.origin))
            },
        )?
        .command(
            CommandSpec::new(CMD_OFFERS, "What the clipboard offers", category)
                .description("What a slot holds (`slot`): its type, title, formats and revision; with `target`, every way it fits there, each with its preview.")
                .args(json!({ "target": "demo.swatches" })),
            |call| {
                let slot = slot_arg(call.args, call.origin);
                with_owner(call.app, |c| c.may_use(call.origin, &slot)).ok_or(NO_PLUGIN)??;
                let item = with_owner(call.app, |c| c.get(&slot)).flatten();
                let rev = with_owner(call.app, |c| c.rev());
                let ways = match call.args.get("target").and_then(Json::as_str) {
                    Some(t) => {
                        let text = call.args.get("text").and_then(Json::as_str);
                        match ways(call.app, t, Some(slot.clone()), text, call.origin) {
                            Ok(w) => json!(w),
                            Err(e) => json!({ "refused": e }),
                        }
                    }
                    None => Json::Null,
                };
                Ok(json!({
                    "slot": slot,
                    "rev": rev,
                    "item": item.map(|i| json!({
                        "kind": i.kind,
                        "title": i.title,
                        "version": i.version,
                        "offers": i.offers(),
                        "stripped": i.stripped,
                    })),
                    "ways": ways,
                }))
            },
        )?
        .command(
            CommandSpec::new(CMD_READ, "Read the clipboard", category)
                .description("A slot's item (`slot`, the session's own by default), payload included; the user's is refused to an agent unless the host allows it.")
                .args(json!({ "slot": null })),
            |call| {
                let slot = slot_arg(call.args, call.origin);
                with_owner(call.app, |c| c.may_use(call.origin, &slot)).ok_or(NO_PLUGIN)??;
                let item = with_owner(call.app, |c| c.get(&slot)).flatten();
                Ok(json!({ "slot": slot, "item": item }))
            },
        )?
        .command(
            CommandSpec::new(CMD_WRITE, "Write the clipboard", category)
                .description("Puts an item in a slot (`slot`, the session's own by default): its `format`, its `data`, a `title`.")
                .args(json!({ "format": "text/plain", "data": "hello", "title": null })),
            |call| {
                let slot = slot_arg(call.args, call.origin);
                with_owner(call.app, |c| c.may_use(call.origin, &slot)).ok_or(NO_PLUGIN)??;
                let format: FormatId = parse(&call.args["format"], "`format`")?;
                let ty = clip_type(call.app, &format);
                let title = call.args["title"]
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| ty.as_ref().map(|t| t.title.clone()))
                    .unwrap_or_else(|| format.to_string());
                let mut item = ClipItem::new(
                    format,
                    ty.as_ref().map_or(0, |t| t.version),
                    call.args["data"].clone(),
                    &title,
                );
                if let Some(render) = ty.as_ref().and_then(|t| t.render.clone()) {
                    item.fallbacks = render(&item.payload);
                }
                item.origin.by = source_of(call.origin);
                let (answer, events) = with_owner(call.app, |c| {
                    let a = c.put(&slot, item, call.origin);
                    (a, c.take_events())
                })
                .ok_or(NO_PLUGIN)?;
                publish_events(call.app, &source_of(call.origin), events);
                match answer {
                    Answer::Done(Ok(rev)) => Ok(json!({ "slot": slot, "rev": rev })),
                    Answer::Done(Err(e)) => Err(e),
                    Answer::Later(_) => Ok(json!({ "slot": slot, "pending": true })),
                }
            },
        )?;
        // egui-pop's own: external text loses its control characters, and
        // HTML becomes text for a target that does not take HTML.
        ctx.clip_sanitiser(
            Sanitiser::new(SANITISE_CONTROLS, Boundary::External, |data| {
                let Some(text) = data.as_str() else {
                    return Vec::new();
                };
                let (clean, n) = model::strip_controls(text);
                if n == 0 {
                    return Vec::new();
                }
                *data = Json::String(clean);
                vec![format!("{n} control characters")]
            })
            .format(TEXT),
        )?
        .clip_transform(Transform::new(
            TransformSpec::new(HTML_TO_TEXT, HTML, TEXT).lossy(),
            |r| {
                let html = r.text().ok_or("HTML that is not text")?;
                Ok(Transformed::lossy(
                    Json::String(model::html_to_text(html)),
                    "formatting",
                ))
            },
        ))?;
        let w1 = ctx.app().downgrade();
        ctx.collector(
            FnCollector::new(
                "clipboard",
                "The clipboard: the owner (local or the host's), its revision, each slot's item (type, title, formats, what was left out; never the payload), the endpoints (source, target, bound, focused), the types, transforms and sanitisers declared.",
            )
            .snapshot(move |_| match w1.get() {
                Ok(app) => describe(&app),
                Err(_) => Json::Null,
            })
        )?;
        ctx.on(On::Pass, |app, _| pass(app))
            .on(On::Disconnected, |app, ev| {
                if let Lifecycle::Disconnected(p) = ev {
                    remove_owner(app, &OwnerId::from(p));
                }
            });
        Ok(())
    }

    fn disconnect(&mut self, ctx: &mut Ctx<'_>) {
        state(&ctx.app()).connected.set(false);
    }
}

fn remove_owner(app: &AppHandle, owner: &OwnerId) {
    let st = state(app);
    st.types.borrow_mut().retain(|(o, _)| o != owner);
    st.transforms.borrow_mut().retain(|(o, _)| o != owner);
    st.sanitisers.borrow_mut().retain(|(o, _)| o != owner);
    st.endpoints
        .borrow_mut()
        .retain(|_, e| e.owner.as_ref() != Some(owner));
}

impl Ctx<'_> {
    /// Declares a type of clipboard item, owned by this plugin (gone with
    /// it). Its format must not be declared already.
    pub fn clip_type(&mut self, t: ClipType) -> Result<&mut Self, CtxError> {
        let st = state(self.app);
        let mut types = st.types.borrow_mut();
        if let Some((by, _)) = types.iter().find(|(_, x)| x.format == t.format) {
            return Err(CtxError::Clipboard(format!(
                "clipboard type `{}` is already declared by `{by}`",
                t.format
            )));
        }
        types.push((self.owner.clone(), t));
        Ok(self)
    }

    /// Declares a clipboard transform, owned by this plugin.
    pub fn clip_transform(&mut self, t: Transform) -> Result<&mut Self, CtxError> {
        let st = state(self.app);
        let mut list = st.transforms.borrow_mut();
        if let Some((by, _)) = list.iter().find(|(_, x)| x.spec.id == t.spec.id) {
            return Err(CtxError::Clipboard(format!(
                "clipboard transform `{}` is already declared by `{by}`",
                t.spec.id
            )));
        }
        list.push((self.owner.clone(), t));
        Ok(self)
    }

    /// Declares a clipboard sanitiser, owned by this plugin.
    pub fn clip_sanitiser(&mut self, s: Sanitiser) -> Result<&mut Self, CtxError> {
        let st = state(self.app);
        let mut list = st.sanitisers.borrow_mut();
        if let Some((by, _)) = list.iter().find(|(_, x)| x.id == s.id) {
            return Err(CtxError::Clipboard(format!(
                "clipboard sanitiser `{}` is already declared by `{by}`",
                s.id
            )));
        }
        list.push((self.owner.clone(), s));
        Ok(self)
    }

    /// Registers a component's endpoint, owned by this plugin (gone with
    /// it); see [`register`].
    pub fn clip_endpoint(
        &mut self,
        name: &str,
        source: Option<Rc<dyn ClipSource>>,
        target: Option<Rc<dyn ClipTarget>>,
    ) -> &mut Self {
        add_endpoint(self.app, Some(self.owner.clone()), name, source, target);
        self
    }
}
