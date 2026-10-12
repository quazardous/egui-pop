//! Dialogs, as a plugin ([`DialogsPlugin`], in `DefaultPlugins`): a
//! question before an action ([`Confirm`]), a form of typed fields
//! ([`Form`], from value descriptors), a choice among options
//! ([`Choose`]) and a notice to acknowledge ([`Info`]). They queue, one
//! shown at a time, modal, drawn from the theme's tokens.
//!
//! Rules: the primary button says what it does ("Delete 3 images", never
//! "OK"); a dangerous one is styled as such and the focus starts on
//! Cancel; Enter accepts (except a danger), Escape cancels. Accepting
//! runs the dialog's command (through the host) with its arguments, the
//! form's values and the choice merged in.
//!
//! A form's path field (a descriptor made with `Descriptor::path`) has a
//! Browse button: the system's chooser with the `native-dialogs`
//! feature, else an in-app picker of folders and files that tests drive.
//!
//! Every open dialog is data: the debug bridge's `dialogs` collector
//! shows the current one and answers it (`answer`), so an agent sees and
//! answers what a person would, with its origin recorded.
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

use egui::{Key, Response, Ui};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_core::overlay::{self, Kind, OverlaySpec};
use egui_pop_core::theme::Tokens;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::{BlockId, CommandId, PluginId};
use egui_pop_model::values::{Descriptor, PathHint, PathKind, Type, Value};
use serde::Serialize;
use serde_json::{Map, Value as Json, json};

use crate::app::AppHandle;
use crate::host::source_of;
use crate::services::ServiceToken;
use crate::zones::{self, Block};
use crate::{Ctx, Plugin, PluginError};

/// The dialogs, as a service.
pub const DIALOGS: ServiceToken<Dialogs> = ServiceToken::new("app.dialogs");

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

/// One option among several; the command gets `choice` (its index) and
/// `label`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Choose {
    pub title: String,
    pub detail: String,
    pub options: Vec<String>,
    pub command: CommandId,
    pub args: Json,
}

impl Choose {
    pub fn new(title: &str, options: &[&str], command: CommandId) -> Self {
        Self {
            title: title.into(),
            detail: String::new(),
            options: options.iter().map(|o| o.to_string()).collect(),
            command,
            args: Json::Null,
        }
    }

    pub fn detail(mut self, detail: &str) -> Self {
        self.detail = detail.into();
        self
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
    Cancel,
}

impl Answer {
    /// As the debug bridge sends it: `{"answer": "accept", "values":
    /// {...}}`, `{"answer": "choose", "index": 1}`, `{"answer":
    /// "cancel"}`.
    pub fn from_json(json: &Json) -> Result<Answer, String> {
        match json.get("answer").and_then(Json::as_str) {
            Some("accept") => Ok(Answer::Accept {
                values: json
                    .get("values")
                    .and_then(Json::as_object)
                    .cloned()
                    .unwrap_or_default(),
            }),
            Some("choose") => json
                .get("index")
                .and_then(Json::as_u64)
                .map(|i| Answer::Choose { index: i as usize })
                .ok_or_else(|| "`index` is missing".into()),
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
    /// Focus placed on its first pass.
    focused: bool,
    /// The in-app picker open for a field: its key and folder.
    picking: Option<(String, PathBuf)>,
    error: Option<String>,
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
    /// Queues a dialog; returns its id.
    pub fn open(&self, dialog: impl Into<Dialog>) -> u64 {
        let mut s = lock(&self.0);
        s.next += 1;
        let id = s.next;
        s.queue.push_back(Open {
            id,
            dialog: dialog.into(),
            focused: false,
            picking: None,
            error: None,
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
                d["waiting"] = json!(s.queue.len() - 1);
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
        PluginId::from_static("egui_pop.dialogs")
    }

    fn title(&self) -> String {
        "Dialogs".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let dialogs = Dialogs::default();
        ctx.service(&DIALOGS, dialogs.clone())?;
        let (a, b) = (dialogs.clone(), dialogs);
        ctx.collector(
            FnCollector::new(
                "dialogs",
                "The dialog shown (kind, title, fields, verb, command) and how many wait; `answer` answers it as an agent.",
            )
            .snapshot(move |_| json!({ "current": a.describe(), "answered": a.answered() }))
            .action("answer", "Answers the dialog shown: accept (with `values`), choose (`index`) or cancel.", true, move |_, args| {
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
        (Dialog::Choose(c), Answer::Choose { index }) => {
            let label = c
                .options
                .get(*index)
                .ok_or_else(|| format!("no option {index}"))?;
            let mut args = match &c.args {
                Json::Object(m) => m.clone(),
                _ => Map::new(),
            };
            args.insert("choice".into(), json!(index));
            args.insert("label".into(), json!(label));
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
    app.publish(
        &source_of(&origin),
        None,
        "dialog.answered",
        json!({ "title": open.dialog.title(), "answer": answer }),
    );
    if let Some((command, args)) = run {
        app.run_as(command, args, origin);
    }
    Ok(())
}

/// Draws the dialog shown, and applies the answers given meanwhile.
fn show(ui: &mut Ui, app: &AppHandle) {
    let Some(d) = app.service(&DIALOGS) else {
        return;
    };
    let given: Vec<(u64, Answer, Origin)> = std::mem::take(&mut lock(&d.0).given);
    for (id, answer, origin) in given {
        if d.current().is_some_and(|(c, _)| c == id)
            && let Err(e) = apply(app, &d, answer, origin)
            && let Some(o) = lock(&d.0).queue.front_mut()
        {
            o.error = Some(e);
        }
    }
    let Some(mut open_dialog) = lock(&d.0).queue.front().cloned() else {
        return;
    };
    let ctx = ui.ctx().clone();
    let spec = OverlaySpec::new(
        ("egui_pop.dialog", open_dialog.id),
        open_dialog.dialog.title(),
        Kind::Modal,
    );
    let mut open = true;
    let mut answer: Option<Answer> = None;
    overlay::show(&ctx, &spec, &mut open, None, |ui| {
        answer = body(ui, &mut open_dialog);
    });
    if !open && answer.is_none() {
        answer = Some(Answer::Cancel);
    }
    match answer {
        Some(a) => {
            if let Err(e) = apply(app, &d, a, Origin::User) {
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

/// A dialog's content and footer; its answer when given.
fn body(ui: &mut Ui, o: &mut Open) -> Option<Answer> {
    let t = Tokens::of(ui.ctx());
    let first = !o.focused;
    o.focused = true;
    let title = o.dialog.title().to_string();
    ui.heading(&title);
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
            let close = ui.button("Close");
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
            for (i, label) in c.options.iter().enumerate() {
                let b = ui.add_sized([ui.available_width(), 0.0], egui::Button::new(label));
                if first && i == 0 {
                    b.request_focus();
                }
                if b.clicked() {
                    answer = Some(Answer::Choose { index: i });
                }
            }
            ui.add_space(t.space.m);
            if ui.button("Cancel").clicked() {
                answer = Some(Answer::Cancel);
            }
        }
        Dialog::Form(f) => {
            let picking = &mut o.picking;
            egui::Grid::new(("egui_pop.form", &f.title))
                .num_columns(2)
                .spacing([t.space.m, t.space.s])
                .show(ui, |ui| {
                    for (n, field) in f.fields.iter_mut().enumerate() {
                        let label = ui.label(&field.descriptor.label);
                        let r = edit_field(ui, field, picking, label.id);
                        if first && n == 0 {
                            r.request_focus();
                        }
                        ui.end_row();
                    }
                });
            if let Some((key, dir)) = picking.clone()
                && let Some(field) = f.fields.iter_mut().find(|x| x.key == key)
            {
                ui.separator();
                let hint = field.descriptor.path.clone().unwrap_or(PathHint {
                    kind: PathKind::File,
                    extensions: Vec::new(),
                });
                match picker(ui, &dir, &hint) {
                    Picked::Stay => {}
                    Picked::Into(d) => *picking = Some((key, d)),
                    Picked::Chosen(p) => {
                        field.value = Value::Text(p.display().to_string());
                        *picking = None;
                    }
                    Picked::Cancel => *picking = None,
                }
            }
            let verb = f.verb.clone();
            if let Some(a) = footer(ui, &verb, false, false) {
                answer = Some(match a {
                    Answer::Accept { .. } => Answer::Accept {
                        values: values_of(&f.fields),
                    },
                    other => other,
                });
            } else if enter && !ui.ctx().text_edit_focused() && o.picking.is_none() {
                answer = Some(Answer::Accept {
                    values: values_of(&f.fields),
                });
            }
        }
    }
    if let Some(e) = &o.error {
        ui.colored_label(t.status.danger, e);
    }
    answer
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
        let cancel = ui.button("Cancel");
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
    picking: &mut Option<(String, PathBuf)>,
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
                    && ui.button("Browse…").clicked()
                {
                    browse(v, &hint, &field.key, picking);
                }
                r
            })
            .inner
        }
        (v, _) => ui.monospace(v.to_json().to_string()),
    };
    r.labelled_by(label)
}

/// The system's chooser when there is one (`native-dialogs`), else the
/// in-app picker.
// The system's chooser writes the value; the in-app picker does it later.
#[allow(clippy::ptr_arg)]
fn browse(value: &mut String, hint: &PathHint, key: &str, picking: &mut Option<(String, PathBuf)>) {
    #[cfg(all(feature = "native-dialogs", not(target_arch = "wasm32")))]
    {
        let start = start_dir(value);
        let mut d = rfd::FileDialog::new().set_directory(&start);
        if !hint.extensions.is_empty() {
            d = d.add_filter("", &hint.extensions);
        }
        let chosen = match hint.kind {
            PathKind::Dir => d.pick_folder(),
            PathKind::File => d.pick_file(),
        };
        if let Some(p) = chosen {
            *value = p.display().to_string();
        }
        let _ = (key, picking);
    }
    #[cfg(not(all(feature = "native-dialogs", not(target_arch = "wasm32"))))]
    {
        let _ = hint;
        *picking = Some((key.to_string(), start_dir(value)));
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

enum Picked {
    Stay,
    Into(PathBuf),
    Chosen(PathBuf),
    Cancel,
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

/// The in-app picker: the folder, its parent, its entries.
fn picker(ui: &mut Ui, dir: &Path, hint: &PathHint) -> Picked {
    let mut out = Picked::Stay;
    ui.horizontal(|ui| {
        if ui.button("Up").clicked()
            && let Some(p) = dir.parent()
        {
            out = Picked::Into(p.to_path_buf());
        }
        ui.monospace(dir.display().to_string());
    });
    egui::ScrollArea::vertical()
        .max_height(200.0)
        .id_salt("egui_pop.picker")
        .show(ui, |ui| {
            for (p, is_dir) in entries(dir, hint) {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let label = if is_dir { format!("{name}/") } else { name };
                if ui.selectable_label(false, label).clicked() {
                    out = if is_dir {
                        Picked::Into(p)
                    } else {
                        Picked::Chosen(p)
                    };
                }
            }
        });
    ui.horizontal(|ui| {
        if hint.kind == PathKind::Dir && ui.button("Choose this folder").clicked() {
            out = Picked::Chosen(dir.to_path_buf());
        }
        if ui.button("Close the picker").clicked() {
            out = Picked::Cancel;
        }
    });
    out
}
