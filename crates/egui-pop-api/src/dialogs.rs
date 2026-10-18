//! Dialogs, as a plugin ([`DialogsPlugin`], in `DefaultPlugins`): a
//! question before an action ([`Confirm`]), a form of typed fields
//! ([`Form`], from value descriptors), a choice among options, each
//! answering a value of its own, with a filter field ([`Choose`]), and a
//! notice to acknowledge ([`Info`]). They queue, one shown at a time,
//! modal, drawn from the theme's tokens.
//!
//! Rules: the primary button says what it does ("Delete 3 images", never
//! "OK"); a dangerous one is styled as such and the focus starts on
//! Cancel; Enter accepts (except a danger), Escape cancels. Accepting
//! runs the dialog's command (through the host) with its arguments, the
//! form's values and the choice merged in.
//!
//! No header band and no close button by default: Cancel and Escape are
//! the way out. A dialog opened as [`DialogSpec::closable()`] (or every
//! dialog, with the `dialogs.closable` setting) has a ✕ at the right end
//! of its title row, named "Close", which is Escape: the same cancel,
//! through the same step and hooks, refused when Escape is.
//!
//! A dialog opened as [`DialogSpec::movable()`] (or every dialog, with
//! the `dialogs.movable` setting) has a header band instead of its title
//! row: its own tone, a separator, the move cursor, the ✕ when it is
//! closable. Dragging the band moves the dialog, kept inside the window
//! (again when the window is resized); a double click on it centres it
//! again; the `dialogs.move` command (Alt+F7) moves it with the arrow
//! keys (Shift for bigger steps), Enter keeping the place and Escape
//! putting it back. A dialog comes up centred, or where it was left when
//! opened with [`DialogSpec::remember_position()`] (kept per key in the
//! host's store, under `ui/dialogs/<key>`).
//!
//! Its backdrop ([`Backdrop`]) dims the interface (`Dim`, the
//! `backdrop.opacity` token's strength, fading while the dialog is
//! dragged) or leaves it untouched (`Clear`, for a dialog previewing its
//! effect on the image); the dialog stays modal either way, and a click
//! on the backdrop never closes it.
//!
//! A form's path field (a descriptor made with `Descriptor::path`) has a
//! Browse button: the file chooser ([`crate::files`]: the system's, or
//! the in-app picker), its result written into the field when it comes;
//! without the files plugin, the in-app picker inside the dialog.
//!
//! Every open dialog is data: the debug bridge's `dialogs` collector
//! shows the current one (a form's values also as a plain `values` map)
//! and answers it (`answer`), so an agent sees and answers what a person
//! would, with its origin recorded.
//!
//! Steps (ADR 0011): `dialog.show` runs when a dialog comes up (a
//! before-hook may veto it: it is cancelled), `dialog.answer` around each
//! answer (a before-hook may veto it, the dialog staying open with the
//! reason, or redirect it to another answer; a run-hook may answer its
//! own way), `dialog.move` when a dialog is dropped, moved by a key or
//! centred again (a before-hook may veto it, the dialog going back, or
//! redirect it elsewhere). The journal says `dialog.answered`, then
//! `dialog.chosen` (the option's index, label and value) or
//! `dialog.cancelled`, `dialog.filtered` as a choice's filter changes,
//! and `dialog.moved` once a move is done.
//!
//! ```
//! use egui_pop_api::dialogs::{self, Confirm};
//! use egui_pop_api::App;
//! use egui_pop_model::ids::CommandId;
//!
//! let app = App::builder()
//!     .plugin(dialogs::DialogsPlugin)
//!     .build(&egui::Context::default())
//!     .unwrap();
//! let d = app.handle().service(&dialogs::DIALOGS).unwrap();
//! d.open(Confirm::new("Delete 3 images?", "Delete 3 images", CommandId::from_static("doc.delete"))
//!     .detail("They go to the trash.")
//!     .danger());
//! assert_eq!(d.describe()["kind"], "confirm");
//! ```

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use std::sync::{Arc, Mutex};

use egui::{Key, Modifiers, Response, Ui, Vec2};
use egui_pop_core::band;
use egui_pop_core::debug::FnCollector;
use egui_pop_core::kit::{self, Kit};
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Kind, OverlaySpec};
use egui_pop_core::strings::{self, CANCEL, CLOSE};
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::{CommandSpec, Origin};
use egui_pop_model::fuzzy;
use egui_pop_model::ids::{BlockId, CommandId, OwnerId, PluginId, SettingId, StepId};
use egui_pop_model::keymap::BindOptions;
use egui_pop_model::values::{Change, Descriptor, PathHint, PathKind, Set, Type, Value};
use serde::Serialize;
use serde_json::{Map, Value as Json, json};

use crate::app::AppHandle;
use crate::files::{self, FILES, FilePicker, FileRequest, PickerAnswer};
use crate::host::Pending;
use crate::host::source_of;
use crate::services::ServiceToken;
use crate::steps::{Outcome, StepSpec};
use crate::strings as txt;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// The dialogs, as a service.
pub const DIALOGS: ServiceToken<Dialogs> = ServiceToken::new("app.dialogs");

/// A dialog comes up: its arguments are its description; a veto cancels
/// it.
pub const STEP_SHOW: StepId = StepId::from_static("dialog.show");
/// A dialog is answered: `{"dialog": description, "answer": answer,
/// "origin": source}`; a veto keeps it open with the reason, a redirect
/// changes the answer.
pub const STEP_ANSWER: StepId = StepId::from_static("dialog.answer");
/// The setting: every dialog has the close button, as if opened
/// [`DialogSpec::closable()`].
pub const SETTING_CLOSABLE: SettingId = SettingId::from_static("dialogs.closable");
/// A dialog is moved: dropped after a drag, moved by a key, centred
/// again. `{"id", "title", "key", "how": "drag" | "keys" | "recentre",
/// "from": [x, y], "to": [x, y]}`, offsets from the window's centre in
/// points; a veto puts it back where it was, a redirect places it at its
/// own `to`.
pub const STEP_MOVE: StepId = StepId::from_static("dialog.move");
/// The setting: every dialog is movable, as if opened
/// [`DialogSpec::movable()`].
pub const SETTING_MOVABLE: SettingId = SettingId::from_static("dialogs.movable");
/// Moves the dialog shown with the arrow keys (Alt+F7): Shift for bigger
/// steps, Enter keeps the place, Escape puts it back.
pub const CMD_MOVE: CommandId = CommandId::from_static("dialogs.move");

/// The plugin's id, owner of the positions it keeps.
const PLUGIN: PluginId = PluginId::from_static("egui_pop.dialogs");
/// What is left of the backdrop while a dialog is dragged.
const DRAG_DIM: f32 = 0.35;
/// How many arrow steps one Shift+arrow press moves.
const BIG_STEP: f32 = 8.0;

/// A question before an action.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Confirm {
    pub title: String,
    /// The consequences, in one line.
    pub detail: String,
    /// The primary button: what it does.
    pub verb: String,
    /// Destructive: the danger style, the focus starts on Cancel, Enter
    /// does not accept.
    pub danger: bool,
    pub command: CommandId,
    pub args: Json,
}

impl Confirm {
    pub fn new(title: &str, verb: &str, command: CommandId) -> Self {
        Self {
            title: title.into(),
            detail: String::new(),
            verb: verb.into(),
            danger: false,
            command,
            args: Json::Null,
        }
    }

    pub fn detail(mut self, detail: &str) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn args(mut self, args: Json) -> Self {
        self.args = args;
        self
    }
}

/// One field of a form.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Field {
    /// Its key in the command's arguments.
    pub key: String,
    pub descriptor: Descriptor,
    pub value: Value,
}

/// Fields typed by value descriptors, and the command they go to.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub verb: String,
    pub command: CommandId,
    /// Arguments the fields are merged into.
    pub args: Json,
}

impl Form {
    pub fn new(title: &str, verb: &str, command: CommandId) -> Self {
        Self {
            title: title.into(),
            fields: Vec::new(),
            verb: verb.into(),
            command,
            args: Json::Null,
        }
    }

    /// Adds a field starting at its descriptor's default.
    pub fn field(mut self, key: &str, descriptor: Descriptor) -> Self {
        let value = descriptor.default.clone();
        self.fields.push(Field {
            key: key.into(),
            descriptor,
            value,
        });
        self
    }

    /// Adds a field starting at a value.
    pub fn field_at(mut self, key: &str, descriptor: Descriptor, value: Value) -> Self {
        self.fields.push(Field {
            key: key.into(),
            descriptor,
            value,
        });
        self
    }
}

/// One option of a [`Choose`]: what it shows and the value it answers.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChooseOption {
    pub label: String,
    /// What the command gets when it is chosen.
    pub value: Json,
    /// A second line, filtered on too (a path, a size).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hint: String,
}

impl ChooseOption {
    pub fn new(label: &str, value: Json) -> Self {
        Self {
            label: label.into(),
            value,
            hint: String::new(),
        }
    }

    pub fn hint(mut self, hint: &str) -> Self {
        self.hint = hint.into();
        self
    }
}

/// One option among several, under a filter field; the command gets the
/// option's value (under `value_key`, `value` by default), its index
/// (`choice`) and its `label`, merged into `args`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Choose {
    pub title: String,
    pub detail: String,
    pub options: Vec<ChooseOption>,
    pub command: CommandId,
    pub args: Json,
    /// The argument the chosen value goes in.
    pub value_key: String,
}

impl Choose {
    /// Options by label, each answering its label.
    pub fn new(title: &str, options: &[&str], command: CommandId) -> Self {
        Self::with_options(
            title,
            options
                .iter()
                .map(|o| ChooseOption::new(o, json!(o)))
                .collect(),
            command,
        )
    }

    /// Options with values of their own.
    pub fn with_options(title: &str, options: Vec<ChooseOption>, command: CommandId) -> Self {
        Self {
            title: title.into(),
            detail: String::new(),
            options,
            command,
            args: Json::Null,
            value_key: "value".into(),
        }
    }

    /// Adds an option.
    pub fn option(mut self, option: ChooseOption) -> Self {
        self.options.push(option);
        self
    }

    pub fn detail(mut self, detail: &str) -> Self {
        self.detail = detail.into();
        self
    }

    /// Arguments the answer is merged into.
    pub fn args(mut self, args: Json) -> Self {
        self.args = args;
        self
    }

    /// The argument the chosen value goes in (`value` by default).
    pub fn value_key(mut self, key: &str) -> Self {
        self.value_key = key.into();
        self
    }

    /// The options a filter shows, best match first (all, in order,
    /// without a filter): their indexes.
    pub fn visible(&self, filter: &str) -> Vec<usize> {
        if filter.trim().is_empty() {
            return (0..self.options.len()).collect();
        }
        fuzzy::rank(&self.options, filter, |o| format!("{} {}", o.label, o.hint))
            .into_iter()
            .map(|(i, _)| i)
            .collect()
    }
}

/// Something to read and acknowledge.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Info {
    pub title: String,
    pub text: String,
}

impl Info {
    pub fn new(title: &str, text: &str) -> Self {
        Self {
            title: title.into(),
            text: text.into(),
        }
    }
}

/// A dialog.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Dialog {
    Confirm(Confirm),
    Form(Form),
    Choose(Choose),
    Info(Info),
}

impl From<Confirm> for Dialog {
    fn from(d: Confirm) -> Self {
        Dialog::Confirm(d)
    }
}

impl From<Form> for Dialog {
    fn from(d: Form) -> Self {
        Dialog::Form(d)
    }
}

impl From<Choose> for Dialog {
    fn from(d: Choose) -> Self {
        Dialog::Choose(d)
    }
}

impl From<Info> for Dialog {
    fn from(d: Info) -> Self {
        Dialog::Info(d)
    }
}

impl Dialog {
    pub fn title(&self) -> &str {
        match self {
            Dialog::Confirm(d) => &d.title,
            Dialog::Form(d) => &d.title,
            Dialog::Choose(d) => &d.title,
            Dialog::Info(d) => &d.title,
        }
    }
}

/// What a modal dialog does to the interface behind it. Either way the
/// interface is blocked while the dialog is open, and a click on it never
/// closes the dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backdrop {
    /// Dimmed, as much as the `backdrop.opacity` token says (less while
    /// the dialog is dragged).
    #[default]
    Dim,
    /// Left untouched: for a dialog previewing its effect on the image.
    Clear,
}

/// A dialog and how it is presented around its content: what
/// [`Dialogs::open`] takes (any dialog converts into one, presented by
/// default).
///
/// ```
/// use egui_pop_api::dialogs::{Backdrop, DialogSpec, Info};
///
/// let spec = DialogSpec::new(Info::new("Saved", "The document was saved.")).closable(true);
/// assert!(spec.closable);
/// let spec = DialogSpec::new(Info::new("Levels", "Drag to adjust."))
///     .movable(true)
///     .backdrop(Backdrop::Clear)
///     .remember_position("levels");
/// assert_eq!(spec.remember.as_deref(), Some("levels"));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct DialogSpec {
    pub dialog: Dialog,
    /// A ✕ at the right end of the title row, doing what Escape does
    /// (off by default; the [`SETTING_CLOSABLE`] setting turns it on for
    /// every dialog).
    pub closable: bool,
    /// A header band to drag the dialog by (off by default; the
    /// [`SETTING_MOVABLE`] setting turns it on for every dialog).
    pub movable: bool,
    /// What the dialog does to the interface behind it.
    pub backdrop: Backdrop,
    /// The key the dialog's place is kept under: it comes up where it
    /// was left instead of centred.
    pub remember: Option<String>,
}

impl DialogSpec {
    pub fn new(dialog: impl Into<Dialog>) -> Self {
        Self {
            dialog: dialog.into(),
            closable: false,
            movable: false,
            backdrop: Backdrop::Dim,
            remember: None,
        }
    }

    /// With the close button (a notice without Cancel, an application
    /// following the web's convention).
    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }

    /// With a header band to drag it by (a dialog hiding what it acts
    /// on).
    pub fn movable(mut self, movable: bool) -> Self {
        self.movable = movable;
        self
    }

    /// What it does to the interface behind it ([`Backdrop::Dim`] by
    /// default).
    pub fn backdrop(mut self, backdrop: Backdrop) -> Self {
        self.backdrop = backdrop;
        self
    }

    /// Comes up where it was last left, kept under `key` (letters,
    /// digits, `. _ - :`) in the host's store; otherwise centred.
    pub fn remember_position(mut self, key: &str) -> Self {
        self.remember = Some(key.into());
        self
    }
}

impl From<Dialog> for DialogSpec {
    fn from(d: Dialog) -> Self {
        Self::new(d)
    }
}

impl From<Confirm> for DialogSpec {
    fn from(d: Confirm) -> Self {
        Self::new(d)
    }
}

impl From<Form> for DialogSpec {
    fn from(d: Form) -> Self {
        Self::new(d)
    }
}

impl From<Choose> for DialogSpec {
    fn from(d: Choose) -> Self {
        Self::new(d)
    }
}

impl From<Info> for DialogSpec {
    fn from(d: Info) -> Self {
        Self::new(d)
    }
}

/// An answer.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "answer", rename_all = "lowercase")]
pub enum Answer {
    /// The primary action (a form's values as they are, or changed:
    /// `values` by field key).
    Accept {
        values: Map<String, Json>,
    },
    /// One of a choice's options, by index.
    Choose {
        index: usize,
    },
    /// One of a choice's options, by its value.
    Value {
        value: Json,
    },
    Cancel,
}

impl Answer {
    /// As the debug bridge sends it: `{"answer": "accept", "values":
    /// {...}}`, `{"answer": "choose", "index": 1}` or `{"answer":
    /// "choose", "value": ...}`, `{"answer": "cancel"}`.
    pub fn from_json(json: &Json) -> Result<Answer, String> {
        match json.get("answer").and_then(Json::as_str) {
            Some("accept") => Ok(Answer::Accept {
                values: json
                    .get("values")
                    .and_then(Json::as_object)
                    .cloned()
                    .unwrap_or_default(),
            }),
            Some("choose" | "value") => match (json.get("value"), json.get("index")) {
                (Some(v), _) => Ok(Answer::Value { value: v.clone() }),
                (None, Some(i)) => i
                    .as_u64()
                    .map(|i| Answer::Choose { index: i as usize })
                    .ok_or_else(|| "`index` is a number".into()),
                (None, None) => Err("`value` or `index` is missing".into()),
            },
            Some("cancel") => Ok(Answer::Cancel),
            _ => Err("`answer` is accept, choose or cancel".into()),
        }
    }
}

/// An open dialog.
#[derive(Clone, Debug)]
struct Open {
    id: u64,
    dialog: Dialog,
    /// Opened with the close button.
    closable: bool,
    /// Opened movable.
    movable: bool,
    backdrop: Backdrop,
    /// The key its place is kept under.
    remember: Option<String>,
    /// Where it is: its offset from the window's centre, in points.
    offset: Vec2,
    /// Moved by the keys: where it was when that started.
    moving: Option<Vec2>,
    /// Dragged: where it was when the drag started.
    drag_from: Option<Vec2>,
    /// Focus placed on its first pass.
    focused: bool,
    /// The in-app picker open inside the dialog for a field (without the
    /// files plugin): its key and picker.
    picking: Option<(String, FilePicker)>,
    /// A field's Browse asked for a choice (started by the next frame).
    browse: Option<(String, FileRequest)>,
    /// A choice under way for a field: its key, the choice's id and
    /// result to come.
    choosing: Option<(String, u64, Pending)>,
    error: Option<String>,
    /// A choice's filter, and the option highlighted among those shown.
    filter: String,
    highlight: usize,
}

#[derive(Debug, Default)]
struct State {
    next: u64,
    queue: VecDeque<Open>,
    /// Answers given (by the bridge) for the next frame.
    given: Vec<(u64, Answer, Origin)>,
    /// What happened to the last dialogs: title and answer.
    answered: VecDeque<(String, Answer, Origin)>,
}

/// The queue of dialogs.
#[derive(Clone, Debug, Default)]
pub struct Dialogs(Arc<Mutex<State>>);

impl Dialogs {
    /// Queues a dialog (or a [`DialogSpec`]); returns its id.
    pub fn open(&self, dialog: impl Into<DialogSpec>) -> u64 {
        let spec = dialog.into();
        let mut s = lock(&self.0);
        s.next += 1;
        let id = s.next;
        s.queue.push_back(Open {
            id,
            dialog: spec.dialog,
            closable: spec.closable,
            movable: spec.movable,
            backdrop: spec.backdrop,
            remember: spec.remember,
            offset: Vec2::ZERO,
            moving: None,
            drag_from: None,
            focused: false,
            picking: None,
            browse: None,
            choosing: None,
            error: None,
            filter: String::new(),
            highlight: 0,
        });
        id
    }

    /// The dialog shown, with its id.
    pub fn current(&self) -> Option<(u64, Dialog)> {
        lock(&self.0)
            .queue
            .front()
            .map(|o| (o.id, o.dialog.clone()))
    }

    /// Where the dialog shown is: its offset from the window's centre, in
    /// points (zero: centred).
    pub fn offset(&self) -> Option<Vec2> {
        lock(&self.0).queue.front().map(|o| o.offset)
    }

    /// How many dialogs wait, the shown one included.
    pub fn len(&self) -> usize {
        lock(&self.0).queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Answers the shown dialog (applied on the next frame), as `origin`.
    pub fn answer(&self, answer: Answer, origin: Origin) -> Result<(), String> {
        let mut s = lock(&self.0);
        let id = s.queue.front().map(|o| o.id).ok_or("no dialog is open")?;
        s.given.push((id, answer, origin));
        Ok(())
    }

    /// The shown dialog and the queue, as the bridge shows them.
    pub fn describe(&self) -> Json {
        let s = lock(&self.0);
        match s.queue.front() {
            Some(o) => {
                let mut d = serde_json::to_value(&o.dialog).unwrap_or(Json::Null);
                d["id"] = json!(o.id);
                d["closable"] = json!(o.closable);
                d["movable"] = json!(o.movable);
                d["backdrop"] = json!(o.backdrop);
                d["offset"] = json!([o.offset.x, o.offset.y]);
                d["moving"] = json!(o.moving.is_some());
                if let Some(key) = &o.remember {
                    d["remember"] = json!(key);
                }
                d["waiting"] = json!(s.queue.len() - 1);
                match &o.dialog {
                    // The typed values as they are, and plainly by key.
                    Dialog::Form(f) => d["values"] = Json::Object(values_of(&f.fields)),
                    Dialog::Choose(c) => {
                        d["filter"] = json!(o.filter);
                        d["shown"] = json!(c.visible(&o.filter));
                    }
                    _ => {}
                }
                d
            }
            None => Json::Null,
        }
    }

    /// The last answers, oldest first: title, answer, origin.
    pub fn answered(&self) -> Vec<(String, Answer, Origin)> {
        lock(&self.0).answered.iter().cloned().collect()
    }
}

/// The plugin.
#[derive(Debug, Default)]
pub struct DialogsPlugin;

impl Plugin for DialogsPlugin {
    fn id(&self) -> PluginId {
        PLUGIN
    }

    fn title(&self) -> String {
        "Dialogs".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let dialogs = Dialogs::default();
        ctx.service(&DIALOGS, dialogs.clone())?;
        ctx.setting(
            SETTING_CLOSABLE,
            Descriptor::bool("Close button on every dialog", false).description(
                "A ✕ at the right end of each dialog's title row, doing what Escape does.",
            ),
        )?;
        ctx.setting(
            SETTING_MOVABLE,
            Descriptor::bool("Movable dialogs", false).description(
                "Every dialog has a header band to drag it by; Alt+F7 moves it with the arrow keys.",
            ),
        )?;
        let moved = dialogs.clone();
        ctx.command(
            CommandSpec::new(CMD_MOVE, "Move dialog", "Dialogs")
                .description("Moves the dialog shown with the arrow keys; Enter keeps the place, Escape puts it back.")
                .when("modalOpen")
                .key_with("Alt+F7", BindOptions::global().through_modal()),
            move |call| {
                let all = setting(call.app, SETTING_MOVABLE);
                let mut s = lock(&moved.0);
                let o = s.queue.front_mut().ok_or("no dialog is open")?;
                if !(o.movable || all) {
                    return Err("the dialog shown is not movable".into());
                }
                o.moving = Some(o.offset);
                o.drag_from = None;
                call.app.egui().request_repaint();
                Ok(json!({ "id": o.id }))
            },
        )?;
        ctx.step(
            StepSpec::new(
                STEP_MOVE,
                "A dialog is moved (dropped, moved by a key, centred again); a veto puts it back, a redirect places it elsewhere.",
            )
            .args(json!({ "id": 1, "title": "Levels", "key": "levels", "how": "drag", "from": [0.0, 0.0], "to": [120.0, -40.0] })),
        )?;
        ctx.step(
            StepSpec::new(STEP_SHOW, "A dialog comes up; a veto cancels it.")
                .args(json!({ "kind": "confirm", "id": 1, "title": "Delete 3 images?" })),
        )?;
        ctx.step(
            StepSpec::new(
                STEP_ANSWER,
                "A dialog is answered; a veto keeps it open, a redirect changes the answer.",
            )
            .args(json!({ "dialog": { "kind": "choose", "id": 1 }, "answer": { "answer": "choose", "index": 0 }, "origin": "user" })),
        )?;
        let (a, b) = (dialogs.clone(), dialogs);
        ctx.collector(
            FnCollector::new(
                "dialogs",
                "The dialog shown (kind, title, fields, verb, command) and how many wait; `answer` answers it as an agent.",
            )
            .snapshot(move |_| json!({ "current": a.describe(), "answered": a.answered() }))
            .action("answer", "Answers the dialog shown: accept (with `values`), choose (`value` or `index`) or cancel.", true, move |_, args| {
                let answer = Answer::from_json(args)?;
                b.answer(answer, Origin::Agent("debug-bridge".into()))?;
                Ok(json!({ "queued": true }))
            })
            .args(json!({ "answer": "accept", "values": {} })),
        )?;
        ctx.block(
            zones::OVERLAYS,
            Block::add(BlockId::from_static("egui_pop.dialogs"), 10, show),
        );
        Ok(())
    }
}

/// The overlay a dialog is drawn in.
pub(crate) fn overlay_id(dialog: u64) -> egui::Id {
    egui::Id::new(("egui_pop.dialog", dialog))
}

/// How many answers are remembered.
const ANSWERED: usize = 16;

/// Applies an answer to the shown dialog: runs its command if accepted.
fn apply(app: &AppHandle, d: &Dialogs, answer: Answer, origin: Origin) -> Result<(), String> {
    let open = lock(&d.0)
        .queue
        .front()
        .cloned()
        .ok_or("no dialog is open")?;
    let run = match (&open.dialog, &answer) {
        (_, Answer::Cancel) => None,
        (Dialog::Confirm(c), Answer::Accept { .. }) => Some((c.command.clone(), c.args.clone())),
        (Dialog::Info(_), Answer::Accept { .. }) => None,
        (Dialog::Form(f), Answer::Accept { values }) => {
            let mut args = match &f.args {
                Json::Object(m) => m.clone(),
                _ => Map::new(),
            };
            for field in &f.fields {
                let v = match values.get(&field.key) {
                    Some(j) => Value::from_json(field.descriptor.ty, j)
                        .map_err(|e| format!("`{}`: {e}", field.key))?,
                    None => field.value.clone(),
                };
                args.insert(field.key.clone(), v.to_json());
            }
            Some((f.command.clone(), Json::Object(args)))
        }
        (Dialog::Choose(c), Answer::Choose { .. } | Answer::Value { .. }) => {
            let index = match &answer {
                Answer::Choose { index } => *index,
                Answer::Value { value } => c
                    .options
                    .iter()
                    .position(|o| o.value == *value)
                    .ok_or_else(|| format!("no option of value {value}"))?,
                _ => unreachable!("matched above"),
            };
            let option = c
                .options
                .get(index)
                .ok_or_else(|| format!("no option {index}"))?;
            let mut args = match &c.args {
                Json::Object(m) => m.clone(),
                _ => Map::new(),
            };
            args.insert("choice".into(), json!(index));
            args.insert("label".into(), json!(option.label));
            args.insert(c.value_key.clone(), option.value.clone());
            Some((c.command.clone(), Json::Object(args)))
        }
        (_, a) => return Err(format!("{a:?} does not answer this dialog")),
    };
    {
        let mut s = lock(&d.0);
        s.queue.pop_front();
        if s.answered.len() == ANSWERED {
            s.answered.pop_front();
        }
        s.answered.push_back((
            open.dialog.title().to_string(),
            answer.clone(),
            origin.clone(),
        ));
    }
    let source = source_of(&origin);
    app.publish(
        &source,
        None,
        "dialog.answered",
        json!({ "title": open.dialog.title(), "answer": answer }),
    );
    match (&open.dialog, &run) {
        (_, None) if answer == Answer::Cancel => app.publish(
            &source,
            None,
            "dialog.cancelled",
            json!({ "id": open.id, "title": open.dialog.title() }),
        ),
        (Dialog::Choose(c), Some((_, args))) => app.publish(
            &source,
            None,
            "dialog.chosen",
            json!({
                "id": open.id,
                "title": c.title,
                "index": args["choice"],
                "label": args["label"],
                "value": args[c.value_key.as_str()],
            }),
        ),
        _ => {}
    }
    if let Some((command, args)) = run {
        app.run_as(command, args, origin);
    }
    Ok(())
}

/// Answers the shown dialog through the [`STEP_ANSWER`] step: hooks may
/// stop the answer (the reason is the error) or change it.
fn answer_step(app: &AppHandle, d: &Dialogs, answer: Answer, origin: Origin) -> Result<(), String> {
    let args = json!({
        "dialog": d.describe(),
        "answer": answer,
        "origin": source_of(&origin),
    });
    let out = app.step(&STEP_ANSWER, args, |app, args| {
        let answer = Answer::from_json(&args["answer"])?;
        apply(app, d, answer, origin.clone()).map(|()| Json::Null)
    });
    match out {
        Outcome::Done(r) => r.map(|_| ()),
        Outcome::Vetoed { reason, .. } => Err(reason),
    }
}

/// Draws the dialog shown, and applies the answers given meanwhile.
fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(d) = app.service(&DIALOGS) else {
        return;
    };
    let given: Vec<(u64, Answer, Origin)> = std::mem::take(&mut lock(&d.0).given);
    for (id, answer, origin) in given {
        if d.current().is_some_and(|(c, _)| c == id)
            && let Err(e) = answer_step(app, &d, answer, origin)
            && let Some(o) = lock(&d.0).queue.front_mut()
        {
            o.error = Some(e);
        }
    }
    let Some(mut open_dialog) = lock(&d.0).queue.front().cloned() else {
        return;
    };
    let ctx = ui.ctx().clone();
    if !open_dialog.focused {
        // It comes up: hooks may stop it (it is cancelled).
        let out = app.step(&STEP_SHOW, d.describe(), |_, _| Ok(Json::Null));
        if out.vetoed().is_some() {
            if let Err(e) = apply(app, &d, Answer::Cancel, Origin::User) {
                log::warn!(target: "egui_pop", "dialog: {e}");
            }
            return;
        }
        // Where it was left, when it keeps its place; centred otherwise.
        if let Some(key) = &open_dialog.remember {
            open_dialog.offset = remembered(app, key).unwrap_or(Vec2::ZERO);
        }
    }
    let filter = open_dialog.filter.clone();
    let closable = open_dialog.closable || setting(app, SETTING_CLOSABLE);
    let movable = open_dialog.movable || setting(app, SETTING_MOVABLE);
    let id = overlay_id(open_dialog.id);
    if movable {
        // Inside the window, again when the window was resized.
        open_dialog.offset = clamp(&ctx, id, open_dialog.offset);
        if let Some(start) = open_dialog.moving {
            move_by_keys(app, &ctx, &mut open_dialog, start);
        }
    } else {
        open_dialog.offset = Vec2::ZERO;
        open_dialog.moving = None;
        open_dialog.drag_from = None;
    }
    // The dim fades while the dialog is dragged.
    let dragged = ctx.animate_bool_with_time(
        id.with("dragged"),
        open_dialog.drag_from.is_some(),
        Tokens::of(&ctx).motion_duration,
    );
    let dim = match open_dialog.backdrop {
        Backdrop::Dim => 1.0 - (1.0 - DRAG_DIM) * dragged,
        Backdrop::Clear => 0.0,
    };
    let mut spec = OverlaySpec::new(
        ("egui_pop.dialog", open_dialog.id),
        open_dialog.dialog.title(),
        Kind::Modal,
    )
    .offset(open_dialog.offset)
    .dim(dim);
    debug_assert_eq!(spec.id, id);
    let mut open = true;
    let mut answer: Option<Answer> = None;
    // The ✕ is shown only where Escape works.
    let closable = closable && spec.dismiss_on_escape;
    if open_dialog.moving.is_some() {
        // Escape puts the dialog back instead of closing it.
        spec = spec.keep_on_escape();
    }
    let mut closed = false;
    let mut band = Band::default();
    overlay::show(&ctx, &spec, &mut open, None, |ui| {
        let header = Header {
            closable,
            movable,
            closed: &mut closed,
            band: &mut band,
        };
        answer = body(ui, app, &mut open_dialog, header);
    });
    if movable {
        drag(app, &ctx, id, &mut open_dialog, &band);
    }
    // The ✕ is Escape: the overlay dismissed, whatever else the pass
    // said (Enter on the ✕ included), answered as a cancel below.
    if closed {
        open = false;
        answer = None;
    }
    if !open && answer.is_none() {
        answer = Some(Answer::Cancel);
    }
    if open_dialog.filter != filter
        && let Dialog::Choose(c) = &open_dialog.dialog
    {
        app.publish(
            "user",
            None,
            "dialog.filtered",
            json!({
                "id": open_dialog.id,
                "filter": open_dialog.filter,
                "shown": c.visible(&open_dialog.filter).len(),
            }),
        );
    }
    browse(app, &mut open_dialog);
    match answer {
        Some(a) => {
            if let Err(e) = answer_step(app, &d, a, Origin::User) {
                open_dialog.error = Some(e);
                if let Some(o) = lock(&d.0).queue.front_mut() {
                    *o = open_dialog;
                }
            } else {
                overlay::close(&ctx, spec.id);
            }
        }
        None => {
            if let Some(o) = lock(&d.0).queue.front_mut()
                && o.id == open_dialog.id
            {
                *o = open_dialog;
            }
        }
    }
}

/// The values of a form's fields, by key.
fn values_of(fields: &[Field]) -> Map<String, Json> {
    fields
        .iter()
        .map(|f| (f.key.clone(), f.value.to_json()))
        .collect()
}

/// What a dialog's head shows, and what happened to it.
struct Header<'a> {
    closable: bool,
    movable: bool,
    /// Its ✕ was pressed.
    closed: &'a mut bool,
    /// Its header band (when movable).
    band: &'a mut Band,
}

/// A dialog's content and footer; its answer when given.
fn body(ui: &mut Ui, app: &AppHandle, o: &mut Open, header: Header<'_>) -> Option<Answer> {
    let t = Tokens::of(ui.ctx());
    let first = !o.focused;
    o.focused = true;
    let title = o.dialog.title().to_string();
    let Header {
        closable,
        movable,
        closed,
        band,
    } = header;
    let heading = if movable {
        header_band(ui, &title, closable, closed, band)
    } else {
        title_row(ui, &title, closable, closed)
    };
    ui.add_space(t.space.s);
    let mut answer = None;
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    match &mut o.dialog {
        Dialog::Confirm(c) => {
            if !c.detail.is_empty() {
                ui.label(&c.detail);
            }
            let (verb, danger) = (c.verb.clone(), c.danger);
            if let Some(a) = footer(ui, &verb, danger, first) {
                answer = Some(a);
            } else if enter && !danger {
                answer = Some(Answer::Accept { values: Map::new() });
            }
        }
        Dialog::Info(i) => {
            ui.label(&i.text);
            ui.add_space(t.space.m);
            let close = ui.button(strings::tr(ui.ctx(), &CLOSE));
            if first {
                close.request_focus();
            }
            if close.clicked() || enter {
                answer = Some(Answer::Accept { values: Map::new() });
            }
        }
        Dialog::Choose(c) => {
            if !c.detail.is_empty() {
                ui.label(&c.detail);
            }
            ui.add_space(t.space.s);
            // Arrows move the highlight, typing filters, Enter chooses.
            let (down, up) = ui.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                    i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                )
            });
            let field = ui
                .add(
                    egui::TextEdit::singleline(&mut o.filter)
                        .hint_text(strings::tr(ui.ctx(), &txt::DIALOG_FILTER))
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(heading.id);
            if first {
                field.request_focus();
            }
            let shown = c.visible(&o.filter);
            if field.changed() {
                o.highlight = 0;
            }
            if down {
                o.highlight = (o.highlight + 1).min(shown.len().saturating_sub(1));
            }
            if up {
                o.highlight = o.highlight.saturating_sub(1);
            }
            ui.add_space(t.space.s);
            egui::ScrollArea::vertical()
                .id_salt("egui_pop.choose")
                .max_height(320.0)
                .show(ui, |ui| {
                    for (n, &i) in shown.iter().enumerate() {
                        let option = &c.options[i];
                        let text = if option.hint.is_empty() {
                            egui::WidgetText::from(option.label.as_str())
                        } else {
                            egui::WidgetText::from(format!("{}  ·  {}", option.label, option.hint))
                        };
                        let b = ui.add_sized(
                            [ui.available_width(), 0.0],
                            egui::Button::selectable(n == o.highlight, text),
                        );
                        if n == o.highlight && (down || up) {
                            b.scroll_to_me(None);
                        }
                        if b.clicked() {
                            answer = Some(Answer::Choose { index: i });
                        }
                    }
                    if shown.is_empty() {
                        ui.weak(strings::tr(ui.ctx(), &txt::NOTHING_MATCHES));
                    }
                });
            if enter && let Some(&i) = shown.get(o.highlight) {
                answer = Some(Answer::Choose { index: i });
            }
            ui.add_space(t.space.m);
            if ui.button(strings::tr(ui.ctx(), &CANCEL)).clicked() {
                answer = Some(Answer::Cancel);
            }
        }
        Dialog::Form(f) => {
            let browse = &mut o.browse;
            egui::Grid::new(("egui_pop.form", &f.title))
                .num_columns(2)
                .spacing([t.space.m, t.space.s])
                .show(ui, |ui| {
                    for (n, field) in f.fields.iter_mut().enumerate() {
                        let label = ui.label(&field.descriptor.label);
                        let r = edit_field(ui, field, browse, label.id);
                        if first && n == 0 {
                            r.request_focus();
                        }
                        ui.end_row();
                    }
                });
            if let Some((key, picker)) = o.picking.as_mut()
                && let Some(field) = f.fields.iter_mut().find(|x| x.key == *key)
            {
                ui.separator();
                match picker.show(
                    ui,
                    &files::Places {
                        home: files::home(),
                        ..Default::default()
                    },
                ) {
                    Some(PickerAnswer::Chosen(p)) => {
                        if let Some(p) = p.first() {
                            field.value = Value::Text(p.display().to_string());
                        }
                        o.picking = None;
                    }
                    Some(PickerAnswer::Cancel) => o.picking = None,
                    Some(PickerAnswer::Bookmark(..)) | None => {}
                }
            }
            // The file chooser's in-app picker, inside the dialog.
            if let Some(id) = o.choosing.as_ref().map(|(_, id, _)| *id)
                && let Some(files) = app.service(&FILES)
            {
                ui.separator();
                files.show_inline(ui, app, id);
            }
            let verb = f.verb.clone();
            if let Some(a) = footer(ui, &verb, false, false) {
                answer = Some(match a {
                    Answer::Accept { .. } => Answer::Accept {
                        values: values_of(&f.fields),
                    },
                    other => other,
                });
            } else if enter
                && !ui.ctx().text_edit_focused()
                && o.picking.is_none()
                && o.choosing.is_none()
            {
                answer = Some(Answer::Accept {
                    values: values_of(&f.fields),
                });
            }
        }
    }
    if let Some(e) = &o.error {
        ui.colored_label(t.status.danger, e);
    }
    paint_band(ui, band);
    answer
}

/// Where a dialog's header band sits: reaching the dialog's edges, with
/// the dialog header's room below its title.
fn band_style(k: &Kit) -> band::BandStyle {
    band::BandStyle {
        padding: k.dialog.data.padding_x,
        gap: k.dialog_header.data.padding_y,
    }
}

/// The header band: the title and the ✕ (when closable) on the kit's
/// `header-band`, above its line, under the move cursor
/// ([`egui_pop_core::band`]).
fn header_band(
    ui: &mut Ui,
    title: &str,
    closable: bool,
    closed: &mut bool,
    out: &mut Band,
) -> Response {
    let kit = Kit::of(ui.ctx());
    let side = close_side(ui);
    let (heading, b) = band::begin(ui, &band_style(&kit), |ui, state| {
        ui.horizontal_top(|ui| {
            let room = if closable {
                (ui.available_width() - side - ui.spacing().item_spacing.x).max(0.0)
            } else {
                ui.available_width()
            };
            // Not selectable: a press on the title drags the dialog.
            let heading = ui
                .allocate_ui(egui::vec2(room, side), |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(title)
                                .heading()
                                .color(kit.dialog_header.style(state).text),
                        )
                        .wrap()
                        .selectable(false),
                    )
                })
                .inner;
            if closable {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if close_button(ui, side).clicked() {
                        *closed = true;
                    }
                });
            }
            heading
        })
        .inner
    });
    *out = Band(Some(b));
    heading
}

/// Paints the header band, when there is one.
fn paint_band(ui: &Ui, b: &Band) {
    if let Some(b) = &b.0 {
        band::paint(ui, b, &band_style(&Kit::of(ui.ctx())));
    }
}

/// A movable dialog's header band as laid out this pass.
#[derive(Debug, Default)]
struct Band(Option<band::Band>);

/// Whether a boolean setting is on.
fn setting(app: &AppHandle, id: SettingId) -> bool {
    lock(&app.store())
        .get(&format!("settings/{id}"))
        .and_then(|e| e.value.as_bool())
        .unwrap_or(false)
}

/// An offset keeping the dialog inside the window (its size is the last
/// pass's; unchanged before it was drawn).
fn clamp(ctx: &egui::Context, id: egui::Id, offset: Vec2) -> Vec2 {
    let Some(rect) = overlay::rect(ctx, id) else {
        return offset;
    };
    let room = ((ctx.content_rect().size() - rect.size()) / 2.0).max(Vec2::ZERO);
    offset.clamp(-room, room)
}

/// What the pointer did to the band: a drag moves the dialog freely
/// (clamped), the drop goes through the move step; a double click
/// centres it again.
fn drag(app: &AppHandle, ctx: &egui::Context, id: egui::Id, o: &mut Open, band: &Band) {
    let Some(band) = &band.0 else {
        return;
    };
    if band.double_clicked {
        o.drag_from = None;
        o.moving = None;
        let from = o.offset;
        move_to(app, o, "recentre", from, Vec2::ZERO);
        return;
    }
    if band.dragging && o.drag_from.is_none() {
        o.drag_from = Some(o.offset);
        o.moving = None;
    }
    if band.dragging && band.delta != Vec2::ZERO {
        o.offset = clamp(ctx, id, o.offset + band.delta);
        ctx.request_repaint();
    }
    // Dropped (or the drag lost): the move step decides.
    if !band.dragging
        && let Some(from) = o.drag_from.take()
    {
        let to = o.offset;
        move_to(app, o, "drag", from, to);
        ctx.request_repaint();
    }
}

/// Moving by the keys: an arrow moves it one step (Shift: bigger ones),
/// Enter keeps the place, Escape puts it back where it was.
fn move_by_keys(app: &AppHandle, ctx: &egui::Context, o: &mut Open, start: Vec2) {
    let step = Tokens::of(ctx).space.m;
    let (by, enter, escape) = ctx.input_mut(|i| {
        let mut by = Vec2::ZERO;
        for (key, dir) in [
            (Key::ArrowLeft, egui::vec2(-1.0, 0.0)),
            (Key::ArrowRight, egui::vec2(1.0, 0.0)),
            (Key::ArrowUp, egui::vec2(0.0, -1.0)),
            (Key::ArrowDown, egui::vec2(0.0, 1.0)),
        ] {
            // Shift first: a plain match ignores an extra Shift.
            while i.consume_key(Modifiers::SHIFT, key) {
                by += dir * step * BIG_STEP;
            }
            while i.consume_key(Modifiers::NONE, key) {
                by += dir * step;
            }
        }
        (
            by,
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Escape),
        )
    });
    let id = overlay_id(o.id);
    if by != Vec2::ZERO {
        let from = o.offset;
        let to = clamp(ctx, id, from + by);
        move_to(app, o, "keys", from, to);
    }
    if escape {
        let from = o.offset;
        move_to(app, o, "keys", from, start);
        o.moving = None;
    } else if enter {
        o.moving = None;
    }
}

/// Moves the dialog through the [`STEP_MOVE`] step: a veto leaves it
/// where it was, a redirect places it elsewhere. Done, it is published
/// (`dialog.moved`) and its place kept when it keeps one.
fn move_to(app: &AppHandle, o: &mut Open, how: &str, from: Vec2, to: Vec2) {
    if from == to {
        o.offset = to;
        return;
    }
    let args = json!({
        "id": o.id,
        "title": o.dialog.title(),
        "key": o.remember,
        "how": how,
        "from": [from.x, from.y],
        "to": [to.x, to.y],
    });
    let out = app.step(&STEP_MOVE, args, |_, args| Ok(json!({ "to": args["to"] })));
    let placed = match out {
        Outcome::Done(Ok(result)) => vec2_of(&result["to"]),
        Outcome::Done(Err(e)) => {
            log::info!(target: "egui_pop", "dialog.move: {e}");
            None
        }
        Outcome::Vetoed { .. } => None,
    };
    let Some(at) = placed else {
        o.offset = from;
        return;
    };
    o.offset = at;
    app.publish(
        "user",
        None,
        "dialog.moved",
        json!({ "id": o.id, "title": o.dialog.title(), "how": how, "offset": [at.x, at.y] }),
    );
    if let Some(key) = &o.remember {
        remember(app, key, at);
    }
}

/// `[x, y]` as a vector.
fn vec2_of(json: &Json) -> Option<Vec2> {
    let a = json.as_array()?;
    match a.as_slice() {
        [x, y] => Some(egui::vec2(x.as_f64()? as f32, y.as_f64()? as f32)),
        _ => None,
    }
}

/// Where a dialog's place is kept, declared in the host's store (a value
/// saved before is picked up then).
fn position_path(app: &AppHandle, key: &str) -> Option<String> {
    let path = format!("ui/dialogs/{key}");
    let store = app.store();
    let mut s = lock(&store);
    if s.get(&path).is_none()
        && let Err(e) = s.declare(
            &OwnerId::from(&PLUGIN),
            &path,
            Descriptor::floats("Dialog position", 2, -1.0e6, 1.0e6, &[0.0, 0.0])
                .description("A dialog's offset from the window's centre, in points."),
        )
    {
        log::warn!(target: "egui_pop", "dialog position `{key}`: {e}");
        return None;
    }
    Some(path)
}

/// The place kept for a dialog.
fn remembered(app: &AppHandle, key: &str) -> Option<Vec2> {
    let path = position_path(app, key)?;
    match &lock(&app.store()).get(&path)?.value {
        Value::Floats(v) if v.len() == 2 => Some(egui::vec2(v[0] as f32, v[1] as f32)),
        _ => None,
    }
}

/// Keeps a dialog's place (no undo step).
fn remember(app: &AppHandle, key: &str, offset: Vec2) {
    let Some(path) = position_path(app, key) else {
        return;
    };
    let set = Set::new(
        &path,
        Change::To(Value::Floats(vec![
            f64::from(offset.x),
            f64::from(offset.y),
        ])),
        Origin::User,
    )
    .untracked();
    if let Err(e) = lock(&app.store()).set(set) {
        log::warn!(target: "egui_pop", "dialog position `{key}`: {e:?}");
    }
}

/// The title row: the title, and the ✕ at its right end when the dialog
/// is closable. No band behind it: a movable dialog has its header band
/// instead ([`header_band`]).
fn title_row(ui: &mut Ui, title: &str, closable: bool, closed: &mut bool) -> Response {
    let text = egui::RichText::new(title)
        .heading()
        .color(Kit::of(ui.ctx()).dialog_header.style(kit::State::REST).text);
    if !closable {
        return ui.label(text);
    }
    let side = close_side(ui);
    ui.horizontal_top(|ui| {
        // The title wraps before the ✕.
        let room = (ui.available_width() - side - ui.spacing().item_spacing.x).max(0.0);
        let heading = ui
            .allocate_ui(egui::vec2(room, side), |ui| {
                ui.add(egui::Label::new(text).wrap())
            })
            .inner;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if close_button(ui, side).clicked() {
                *closed = true;
            }
        });
        heading
    })
    .inner
}

/// The close button's side: a target of at least 24 points (WCAG 2.5.8),
/// as tall as a control.
fn close_side(ui: &Ui) -> f32 {
    Tokens::of(ui.ctx()).size.control_height.max(24.0)
}

/// The ✕: the kit's close glyph on a `band-icon-button`, named "Close".
/// Never focused when the dialog opens (nothing asks it to be).
fn close_button(ui: &mut Ui, side: f32) -> Response {
    let name = strings::tr(ui.ctx(), &CLOSE);
    let glyph = Kit::of(ui.ctx()).glyph(kit::Glyph::Close);
    band::button(ui, side, glyph, &name, false)
}

/// Cancel and the primary button, right-aligned.
fn footer(ui: &mut Ui, verb: &str, danger: bool, first: bool) -> Option<Answer> {
    let t = Tokens::of(ui.ctx());
    ui.add_space(t.space.m);
    let mut answer = None;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // Danger: the verb and the outline in the danger colour, on the
        // button's own background (no new contrast pair).
        let primary = if danger {
            egui::Button::new(egui::RichText::new(verb).color(t.status.danger))
                .stroke(egui::Stroke::new(1.0, t.status.danger))
        } else {
            egui::Button::new(verb)
        };
        let p = ui.add(primary);
        let cancel = ui.button(strings::tr(ui.ctx(), &CANCEL));
        if first {
            if danger {
                cancel.request_focus();
            } else {
                p.request_focus();
            }
        }
        if p.clicked() {
            answer = Some(Answer::Accept { values: Map::new() });
        }
        if cancel.clicked() {
            answer = Some(Answer::Cancel);
        }
    });
    answer
}

/// A field's editor, labelled by `label`.
fn edit_field(
    ui: &mut Ui,
    field: &mut Field,
    browse: &mut Option<(String, FileRequest)>,
    label: egui::Id,
) -> Response {
    let d = &field.descriptor;
    let r = match (&mut field.value, d.ty) {
        (Value::Bool(v), _) => ui.checkbox(v, ""),
        (Value::Float(v), _) => {
            let (lo, hi) = d.soft.or(d.hard).unwrap_or((0.0, 1.0));
            ui.add(
                egui::DragValue::new(v)
                    .range(lo..=hi)
                    .speed(d.step)
                    .max_decimals(d.precision as usize),
            )
        }
        (Value::Int(v), _) => {
            let (lo, hi) = d.hard.unwrap_or((i64::MIN as f64, i64::MAX as f64));
            ui.add(egui::DragValue::new(v).range(lo as i64..=hi as i64))
        }
        (Value::Choice(v), _) => {
            let mut picked = v.clone();
            let r = egui::ComboBox::from_id_salt(("egui_pop.form.choice", &field.key))
                .selected_text(picked.as_str())
                .show_ui(ui, |ui| {
                    for c in &d.choices {
                        ui.selectable_value(&mut picked, c.clone(), c);
                    }
                })
                .response;
            *v = picked;
            r
        }
        (Value::Text(v), Type::Text) => {
            let path = d.path.clone();
            ui.horizontal(|ui| {
                let r = ui.text_edit_singleline(v);
                egui_pop_core::focus_mode::follow_pointer(&r);
                if let Some(hint) = path
                    && ui
                        .button(strings::tr(ui.ctx(), &txt::DIALOG_BROWSE))
                        .clicked()
                {
                    *browse = Some((
                        field.key.clone(),
                        request_for(&field.descriptor.label, v, &hint),
                    ));
                }
                r
            })
            .inner
        }
        (v, _) => ui.monospace(v.to_json().to_string()),
    };
    r.labelled_by(label)
}

/// The choice a path field's Browse asks for.
fn request_for(label: &str, value: &str, hint: &PathHint) -> FileRequest {
    let mut r = match hint.kind {
        PathKind::Dir => FileRequest::folder(label),
        PathKind::File => FileRequest::open(label),
    }
    .directory(start_dir(value));
    if !hint.extensions.is_empty() {
        let exts: Vec<&str> = hint.extensions.iter().map(String::as_str).collect();
        r = r.filter(&exts.join(", "), &exts);
    }
    r
}

/// Starts the choice a Browse asked for (through the files plugin, else
/// the in-app picker inside the dialog), and writes a finished choice
/// into its field.
fn browse(app: &AppHandle, o: &mut Open) {
    if let Some((key, request)) = o.browse.take() {
        match app.service(&FILES) {
            Some(f) => {
                let (id, pending) = f.choose_inline(app, request);
                o.choosing = Some((key, id, pending));
            }
            None => o.picking = Some((key, FilePicker::new(&request))),
        }
    }
    let Some(result) = o.choosing.as_ref().and_then(|(_, _, p)| p.take()) else {
        return;
    };
    let Some((key, ..)) = o.choosing.take() else {
        return;
    };
    match files::chosen(&result) {
        Ok(files::Chosen::Paths(paths)) => {
            if let (Dialog::Form(f), Some(p)) = (&mut o.dialog, paths.first())
                && let Some(field) = f.fields.iter_mut().find(|x| x.key == key)
            {
                field.value = Value::Text(p.display().to_string());
            }
        }
        Ok(files::Chosen::Cancelled) => {}
        Err(e) => o.error = Some(e),
    }
}

/// Where a chooser starts: the value's folder, else home, else the root.
fn start_dir(value: &str) -> PathBuf {
    let p = Path::new(value);
    if p.is_dir() {
        return p.to_path_buf();
    }
    if let Some(parent) = p.parent().filter(|d| d.is_dir()) {
        return parent.to_path_buf();
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// The entries of a folder a picker lists: folders, then the files the
/// hint accepts (none for a folder picker), by name; hidden ones left
/// out.
pub fn entries(dir: &Path, hint: &PathHint) -> Vec<(PathBuf, bool)> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, bool)> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            !p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
        })
        .filter_map(|p| {
            let is_dir = p.is_dir();
            (is_dir || (hint.kind == PathKind::File && hint.accepts(&p))).then_some((p, is_dir))
        })
        .collect();
    out.sort_by(|a, b| (!a.1, &a.0).cmp(&(!b.1, &b.0)));
    out
}
