//! The administration app's model (`docs/design/admin-forms.md`): a
//! declarative form tree for everything an application shows that is not
//! its work surface (settings, tools, tasks).
//!
//! A component never draws its own widgets: it declares a [`Manifest`]
//! (scopes, sections, groups and their items: fields, actions, statuses,
//! callouts) and the framework renders it with a closed set of bricks.
//! Manifests are data with names: Rust builders produce them and they
//! serialise to JSON, so an out-of-process plugin can send one and an
//! agent can read one. Hooks are named here and registered in Rust
//! ([`hooks`]); they compute, they never draw.
//!
//! - [`Manifest`]: a versioned contribution, checked ([`Manifest::check`])
//!   and merged into a [`Tree`];
//! - [`generate`]: the pages of today's settings, from their descriptors,
//!   with no hand-written page;
//! - [`layers`]: the value scopes a field may be set in (app, library,
//!   workspace, and the run's overrides, shown, never edited);
//! - [`template`]: an action's argument template (`${field}`);
//! - [`wizard`]: the wizard kit, declared steps (each a form over a
//!   draft) with their validation, a summary and a command run on finish.

pub mod generate;
pub mod hooks;
pub mod layers;
pub mod template;
pub mod transfer;
pub mod wizard;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::diagnostics::Diagnostics;
use crate::when::{When, WhenContext};
pub use layers::ValueScope;
pub use wizard::{Check, Finish, Flow, Step, Wizard};

/// The manifest format this framework reads. A newer one is refused.
pub const FORMAT: u32 = 1;

/// What every node carries: a stable id (never translated, unique among
/// its siblings), a title and description (translated through the
/// strings catalogue), keywords for search, an icon, an order, and the
/// `when` and `enabled` clauses (the `when` language).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub order: i32,
    /// Shown when it holds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// Greyed, with its reason, when it does not hold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<String>,
    /// Why it is greyed when `enabled` does not hold (a tooltip).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Meta {
    pub fn new(id: &str, title: &str) -> Meta {
        Meta {
            id: id.into(),
            title: title.into(),
            ..Meta::default()
        }
    }
}

/// A family of sections: a tab with an icon on the horizontal axis.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    #[serde(flatten)]
    pub meta: Meta,
}

/// An entry of the left panel and the page it shows, inside a scope.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Section {
    #[serde(flatten)]
    pub meta: Meta,
    /// The scope it is in (its id).
    pub scope: String,
    /// A section it sits under (one level, no deeper).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default)]
    pub groups: Vec<Group>,
}

impl Section {
    /// Its path, `scope/section`.
    pub fn path(&self) -> String {
        format!("{}/{}", self.scope, self.meta.id)
    }
}

/// A heading and its rows in an inset box, collapsible.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Group {
    #[serde(flatten)]
    pub meta: Meta,
    #[serde(default)]
    pub items: Vec<Item>,
}

/// A row of a group: every field, action, status and callout is one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum Item {
    Field(Field),
    Action(Action),
    Status(Status),
    Callout(Callout),
    /// A registered widget kind: the declared exception.
    Widget(Widget),
    /// Records as list items: a title, a subtitle, a trailing value.
    List(Records),
    /// Records as a table: a column per key.
    Table(Records),
}

impl Item {
    pub fn meta(&self) -> &Meta {
        match self {
            Item::Field(f) => &f.meta,
            Item::Action(a) => &a.meta,
            Item::Status(s) => &s.meta,
            Item::Callout(c) => &c.meta,
            Item::Widget(w) => &w.meta,
            Item::List(r) | Item::Table(r) => &r.meta,
        }
    }
}

/// The `when` and `enabled` clauses, for every node's builder.
pub trait Clauses: Sized {
    fn meta_mut(&mut self) -> &mut Meta;

    /// Shown only when the clause holds (the `when` language; the
    /// section's values as `form.<field id>`).
    fn when(mut self, clause: &str) -> Self {
        self.meta_mut().when = Some(clause.into());
        self
    }

    /// Greyed, with its reason, when the clause does not hold.
    fn enabled(mut self, clause: &str, reason: &str) -> Self {
        let m = self.meta_mut();
        m.enabled = Some(clause.into());
        m.reason = Some(reason.into());
        self
    }
}

macro_rules! clauses {
    ($($t:ty),*) => {$(
        impl Clauses for $t {
            fn meta_mut(&mut self) -> &mut Meta {
                &mut self.meta
            }
        }
    )*};
}

clauses!(Field, Action, Status, Callout, Records, RowField);

/// The ten field kinds. The list is closed: a new one is a decision in
/// the design note, not in a plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldKind {
    Toggle,
    Number,
    Choice,
    Multi,
    Text,
    LongText,
    Path,
    List,
    Colour,
    Keys,
}

impl FieldKind {
    pub const ALL: [FieldKind; 10] = [
        FieldKind::Toggle,
        FieldKind::Number,
        FieldKind::Choice,
        FieldKind::Multi,
        FieldKind::Text,
        FieldKind::LongText,
        FieldKind::Path,
        FieldKind::List,
        FieldKind::Colour,
        FieldKind::Keys,
    ];

    /// The kind a descriptor's type renders as.
    pub fn of(d: &crate::values::Descriptor) -> FieldKind {
        use crate::values::Type;
        match d.ty {
            Type::Bool => FieldKind::Toggle,
            Type::Float | Type::Int => FieldKind::Number,
            Type::Choice => FieldKind::Choice,
            Type::Text if d.path.is_some() => FieldKind::Path,
            Type::Text => FieldKind::Text,
            Type::Floats | Type::Ints => FieldKind::List,
        }
    }
}

/// Where a field's value lives.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum Bind {
    /// A declared setting (`settings/<id>`), its descriptor from the
    /// declaration.
    Setting { setting: String },
    /// A task's draft (`tasks/<task>/draft/<key>`), not a setting.
    Draft { task: String, key: String },
}

/// A value bound to the store or a draft.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    #[serde(flatten)]
    pub meta: Meta,
    pub bind: Bind,
    /// `None`: from the descriptor's type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<FieldKind>,
    /// Where its value may be set; empty: wherever the application
    /// offers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<ValueScope>,
    /// Help shown under it (the descriptor's description otherwise).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub help: String,
    /// A change takes effect after a restart.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restart: bool,
    /// Hooks by name ([`hooks`]).
    #[serde(default, skip_serializing_if = "FieldHooks::is_empty")]
    pub hooks: FieldHooks,
    /// A draft field's own descriptor: its default, choices, range and
    /// whether it is required (a setting's comes from its declaration).
    #[serde(default, skip_serializing_if = "Input::is_empty")]
    pub input: Input,
}

/// What a draft field declares of its value, in place of a setting's
/// descriptor: its default, its choices (value and label), its range,
/// and whether it may be left empty. The checks it allows need no hook
/// ([`Field::check`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Input {
    #[serde(skip_serializing_if = "Json::is_null")]
    pub default: Json,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<RowChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    /// A path field's chooser: a file to open, or to save under a
    /// proposed name ("Browse..." beside the field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<FileInput>,
}

/// What a path field's Browse chooses.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileInput {
    /// A file to write (the chooser asks before replacing one), else one
    /// to read.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub save: bool,
    /// The name proposed when saving.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The extensions offered, without the dot (none: any file).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

impl Input {
    pub fn is_empty(&self) -> bool {
        *self == Input::default()
    }
}

/// The hooks a field names.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FieldHooks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

impl FieldHooks {
    pub fn is_empty(&self) -> bool {
        *self == FieldHooks::default()
    }
}

impl Field {
    /// A field bound to a declared setting; its id is the setting's.
    pub fn setting(id: &str) -> Field {
        Field {
            meta: Meta::new(id, ""),
            bind: Bind::Setting { setting: id.into() },
            kind: None,
            scopes: Vec::new(),
            help: String::new(),
            restart: false,
            hooks: FieldHooks::default(),
            input: Input::default(),
        }
    }

    /// A field of a task's draft.
    pub fn draft(task: &str, key: &str, title: &str, kind: FieldKind) -> Field {
        Field {
            meta: Meta::new(key, title),
            bind: Bind::Draft {
                task: task.into(),
                key: key.into(),
            },
            kind: Some(kind),
            ..Field::setting(key)
        }
    }

    pub fn kind(mut self, kind: FieldKind) -> Field {
        self.kind = Some(kind);
        self
    }

    pub fn scopes(mut self, scopes: &[ValueScope]) -> Field {
        self.scopes = scopes.to_vec();
        self
    }

    pub fn help(mut self, help: &str) -> Field {
        self.help = help.into();
        self
    }

    pub fn restart(mut self) -> Field {
        self.restart = true;
        self
    }

    pub fn validate(mut self, hook: &str) -> Field {
        self.hooks.validate = Some(hook.into());
        self
    }

    pub fn choices(mut self, hook: &str) -> Field {
        self.hooks.choices = Some(hook.into());
        self
    }

    pub fn preview(mut self, hook: &str) -> Field {
        self.hooks.preview = Some(hook.into());
        self
    }

    pub fn transform(mut self, hook: &str) -> Field {
        self.hooks.transform = Some(hook.into());
        self
    }

    /// A draft field's value at the start.
    pub fn default_value(mut self, value: Json) -> Field {
        self.input.default = value;
        self
    }

    /// A draft field's choice: the value kept and its label.
    pub fn choice(mut self, value: Json, label: &str) -> Field {
        self.input.choices.push(RowChoice {
            value,
            label: label.into(),
        });
        self
    }

    /// A draft number's range.
    pub fn range(mut self, min: f64, max: f64) -> Field {
        self.input.min = Some(min);
        self.input.max = Some(max);
        self
    }

    /// A draft path field whose Browse chooses a file to read.
    pub fn open_file(mut self, extensions: &[&str]) -> Field {
        self.kind = Some(FieldKind::Path);
        self.input.file = Some(FileInput {
            save: false,
            name: String::new(),
            extensions: extensions.iter().map(|e| e.to_string()).collect(),
        });
        self
    }

    /// A draft path field whose Browse chooses a file to write, `name`
    /// proposed.
    pub fn save_file(mut self, name: &str, extensions: &[&str]) -> Field {
        self.kind = Some(FieldKind::Path);
        self.input.file = Some(FileInput {
            save: true,
            name: name.into(),
            extensions: extensions.iter().map(|e| e.to_string()).collect(),
        });
        self
    }

    /// A draft field that may not be left empty.
    pub fn required(mut self) -> Field {
        self.input.required = true;
        self
    }

    /// The draft key it is bound to, if any.
    pub fn draft_key(&self) -> Option<&str> {
        match &self.bind {
            Bind::Draft { key, .. } => Some(key),
            Bind::Setting { .. } => None,
        }
    }

    /// What its input says of a value: empty when required, out of its
    /// range, not one of its choices. `None`: fine.
    pub fn check(&self, value: &Json) -> Option<Check> {
        let i = &self.input;
        let empty = match value {
            Json::Null => true,
            Json::String(s) => s.trim().is_empty(),
            Json::Array(a) => a.is_empty(),
            _ => false,
        };
        if empty {
            return i.required.then_some(Check::Required);
        }
        if let Some(n) = value.as_f64() {
            if let Some(min) = i.min.filter(|m| n < *m) {
                return Some(Check::Below(min));
            }
            if let Some(max) = i.max.filter(|m| n > *m) {
                return Some(Check::Above(max));
            }
        }
        // A multi's value is the choices ticked among those offered: one
        // no longer offered is not ticked, not an error.
        if self.kind != Some(FieldKind::Multi)
            && !i.choices.is_empty()
            && !i.choices.iter().any(|c| c.value == *value)
        {
            return Some(Check::NotAChoice);
        }
        None
    }

    /// A value as the summary shows it: its choice's label, else written
    /// out (a toggle's words are the shell's).
    pub fn shown(&self, value: &Json) -> String {
        let one = |v: &Json| {
            self.input
                .choices
                .iter()
                .find(|c| c.value == *v)
                .map_or_else(|| cell_text(v), |c| c.label.clone())
        };
        match value {
            Json::Array(a) if self.kind == Some(FieldKind::Multi) => {
                a.iter().map(one).collect::<Vec<_>>().join(", ")
            }
            v => one(v),
        }
    }

    /// The setting it is bound to, if any.
    pub fn setting_id(&self) -> Option<&str> {
        match &self.bind {
            Bind::Setting { setting } => Some(setting),
            Bind::Draft { .. } => None,
        }
    }
}

/// How an action's button looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStyle {
    /// One per page at most.
    Primary,
    #[default]
    Secondary,
    Danger,
    Link,
}

/// A command and its argument template: a button or a link. The
/// command's declaration gives the title, icon, keys and `when` the
/// action does not set, so an action and the palette entry are one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Action {
    #[serde(flatten)]
    pub meta: Meta,
    pub command: String,
    /// `${field}`, `${row.<key>}`, `${selection}` ([`template`]).
    #[serde(default, skip_serializing_if = "Json::is_null")]
    pub args: Json,
    #[serde(default)]
    pub style: ActionStyle,
    /// A `confirm` hook by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
    /// A confirmation asked before the command runs (its texts may name
    /// the form's and the row's values, `${row.name}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<Confirmation>,
}

/// A question asked before an action's command runs: a title, what
/// happens, the verb of the button that goes on, and the danger style
/// (the focus starts on Cancel).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Confirmation {
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    pub verb: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub danger: bool,
}

impl Confirmation {
    pub fn new(title: &str, text: &str, verb: &str) -> Confirmation {
        Confirmation {
            title: title.into(),
            text: text.into(),
            verb: verb.into(),
            danger: false,
        }
    }

    pub fn danger(mut self) -> Confirmation {
        self.danger = true;
        self
    }

    /// Its texts with the values they name filled in.
    pub fn filled(&self, values: &hooks::Values) -> Confirmation {
        let fill = |s: &str| match template::expand(&Json::String(s.into()), values).0 {
            Json::String(s) => s,
            other => cell_text(&other),
        };
        Confirmation {
            title: fill(&self.title),
            text: fill(&self.text),
            verb: fill(&self.verb),
            danger: self.danger,
        }
    }
}

impl Action {
    pub fn command(command: &str) -> Action {
        Action {
            meta: Meta::new(command, ""),
            command: command.into(),
            args: Json::Null,
            style: ActionStyle::Secondary,
            confirm: None,
            confirmation: None,
        }
    }

    pub fn title(mut self, title: &str) -> Action {
        self.meta.title = title.into();
        self
    }

    pub fn args(mut self, args: Json) -> Action {
        self.args = args;
        self
    }

    pub fn primary(mut self) -> Action {
        self.style = ActionStyle::Primary;
        self
    }

    pub fn danger(mut self) -> Action {
        self.style = ActionStyle::Danger;
        self
    }

    pub fn confirm(mut self, hook: &str) -> Action {
        self.confirm = Some(hook.into());
        self
    }

    /// Asks this before running.
    pub fn confirmation(mut self, confirmation: Confirmation) -> Action {
        self.confirmation = Some(confirmation);
        self
    }
}

/// A tone: a callout's, a status's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    #[default]
    Info,
    Success,
    Warning,
    Danger,
}

/// Where a status reads its value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum StatusSource {
    /// A `read_only` command and a JSON pointer into its answer.
    Command { command: String, pointer: String },
    /// A `status` hook registered in Rust.
    Hook { hook: String },
}

/// A read-out: a label, a value, a tone. Read again after the events
/// named, never per frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Status {
    #[serde(flatten)]
    pub meta: Meta,
    pub source: StatusSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refresh: Vec<String>,
}

impl Status {
    pub fn command(id: &str, title: &str, command: &str, pointer: &str) -> Status {
        Status {
            meta: Meta::new(id, title),
            source: StatusSource::Command {
                command: command.into(),
                pointer: pointer.into(),
            },
            refresh: Vec::new(),
        }
    }

    pub fn hook(id: &str, title: &str, hook: &str) -> Status {
        Status {
            meta: Meta::new(id, title),
            source: StatusSource::Hook { hook: hook.into() },
            refresh: Vec::new(),
        }
    }

    pub fn refresh(mut self, event: &str) -> Status {
        self.refresh.push(event.into());
        self
    }
}

/// A message with a tone and an optional action, at the top of a page or
/// group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Callout {
    #[serde(flatten)]
    pub meta: Meta,
    pub tone: Tone,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Action>,
}

impl Callout {
    pub fn new(id: &str, tone: Tone, text: &str) -> Callout {
        Callout {
            meta: Meta::new(id, ""),
            tone,
            text: text.into(),
            action: None,
        }
    }

    pub fn action(mut self, action: Action) -> Callout {
        self.action = Some(action);
        self
    }
}

/// Where a list's or table's records come from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum RecordSource {
    /// A `read_only` command whose answer, at the JSON pointer, is an
    /// array of objects (`""`: the whole answer).
    Command {
        command: String,
        #[serde(default, skip_serializing_if = "Json::is_null")]
        args: Json,
        #[serde(default)]
        pointer: String,
    },
    /// A `records` hook registered in Rust.
    Hook { hook: String },
}

/// A column of a table, or a part of a list item: the record's key it
/// shows and its heading.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub key: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "CellKind::is_text")]
    pub kind: CellKind,
    /// A meter's words, from this key of the record (`120 GB free of
    /// 1 TB`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// How a cell shows its value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellKind {
    /// Written out ([`cell_text`]).
    #[default]
    Text,
    /// A bar filled by a share from 0 to 1 (used of total, done of all),
    /// with its words over it; text when the value is not a number.
    Meter,
}

impl CellKind {
    pub fn is_text(&self) -> bool {
        *self == CellKind::Text
    }
}

/// A meter's share: a number clamped to 0 to 1, none for anything else.
pub fn meter_share(v: &Json) -> Option<f32> {
    v.as_f64().map(|x| x.clamp(0.0, 1.0) as f32)
}

/// A choice of a row field: the value written and its label.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowChoice {
    pub value: Json,
    pub label: String,
}

/// An editable cell: a record's value shown as a field (a toggle, a
/// text, a choice) and written through a command whose arguments name
/// the record (`${row.<key>}`) and the new value (`${value}`). In a
/// table it takes the place of the column of its key, or adds one; in a
/// list it shows under the record's title. Its `when` and `enabled`
/// read the record as `row.<key>`; its description is its tooltip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowField {
    #[serde(flatten)]
    pub meta: Meta,
    /// The record's key it shows.
    pub key: String,
    /// `toggle`, `text` or `choice`; another kind shows as text.
    pub kind: FieldKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<RowChoice>,
    /// The choices from the record: an array of `{value, label}` at this
    /// key (destinations that depend on the record).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices_key: Option<String>,
    pub command: String,
    #[serde(default, skip_serializing_if = "Json::is_null")]
    pub args: Json,
}

impl RowField {
    fn new(kind: FieldKind, key: &str, title: &str, command: &str, args: Json) -> RowField {
        RowField {
            meta: Meta::new(key, title),
            key: key.into(),
            kind,
            choices: Vec::new(),
            choices_key: None,
            command: command.into(),
            args,
        }
    }

    /// A switch over a boolean.
    pub fn toggle(key: &str, title: &str, command: &str, args: Json) -> RowField {
        RowField::new(FieldKind::Toggle, key, title, command, args)
    }

    /// A text, written when the field is left or Enter is pressed.
    pub fn text(key: &str, title: &str, command: &str, args: Json) -> RowField {
        RowField::new(FieldKind::Text, key, title, command, args)
    }

    /// One of the choices (value, label).
    pub fn choice(
        key: &str,
        title: &str,
        choices: &[(Json, &str)],
        command: &str,
        args: Json,
    ) -> RowField {
        let mut f = RowField::new(FieldKind::Choice, key, title, command, args);
        f.choices = choices
            .iter()
            .map(|(value, label)| RowChoice {
                value: value.clone(),
                label: (*label).into(),
            })
            .collect();
        f
    }

    /// The choices read from the record at `key`.
    pub fn choices_from(mut self, key: &str) -> RowField {
        self.choices_key = Some(key.into());
        self
    }

    /// Its id, when two fields show the same key.
    pub fn id(mut self, id: &str) -> RowField {
        self.meta.id = id.into();
        self
    }

    /// Its tooltip.
    pub fn help(mut self, text: &str) -> RowField {
        self.meta.description = text.into();
        self
    }

    /// The choices offered for a record: the declared ones, then those
    /// the record holds.
    pub fn choices_of(&self, row: &Json) -> Vec<RowChoice> {
        let mut out = self.choices.clone();
        if let Some(k) = &self.choices_key {
            out.extend(
                row[k.as_str()]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| serde_json::from_value::<RowChoice>(c.clone()).ok()),
            );
        }
        out
    }

    /// The command's arguments for a new value: the template filled with
    /// the values (the record's as `row.<key>`) and `value`.
    pub fn write_args(&self, values: &hooks::Values, value: Json) -> Json {
        let mut v = values.clone();
        v.insert("value".into(), value);
        template::expand(&self.args, &v).0
    }
}

/// What an empty source shows: a message and an optional action.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Empty {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Action>,
}

/// A record list or table: rows read from a source, each with its
/// actions (`${row.<key>}` in their arguments).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Records {
    #[serde(flatten)]
    pub meta: Meta,
    pub source: RecordSource,
    /// The record's id (its key in a row action's `${row.<key>}`).
    pub key: String,
    /// A list: the title and subtitle keys; a table: unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle_key: Option<String>,
    /// A table's columns; a list's trailing values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<Column>,
    /// A key whose value (`info`, `success`, `warning`, `danger`) tones
    /// the row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_fields: Vec<RowField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty: Option<Empty>,
    /// Event kinds after which it is read again (`storage.*`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refresh: Vec<String>,
}

impl Records {
    fn new(id: &str, title: &str, source: RecordSource, key: &str) -> Records {
        Records {
            meta: Meta::new(id, title),
            source,
            key: key.into(),
            title_key: None,
            subtitle_key: None,
            columns: Vec::new(),
            tone_key: None,
            row_fields: Vec::new(),
            row_actions: Vec::new(),
            empty: None,
            refresh: Vec::new(),
        }
    }

    /// Records a `read_only` command answers (an array at `pointer`).
    pub fn command(id: &str, title: &str, command: &str, pointer: &str, key: &str) -> Records {
        Records::new(
            id,
            title,
            RecordSource::Command {
                command: command.into(),
                args: Json::Null,
                pointer: pointer.into(),
            },
            key,
        )
    }

    /// Records a `records` hook gives.
    pub fn hook(id: &str, title: &str, hook: &str, key: &str) -> Records {
        Records::new(id, title, RecordSource::Hook { hook: hook.into() }, key)
    }

    /// The read command's arguments.
    pub fn args(mut self, args: Json) -> Records {
        if let RecordSource::Command { args: a, .. } = &mut self.source {
            *a = args;
        }
        self
    }

    /// A list item's title and subtitle keys.
    pub fn titles(mut self, title: &str, subtitle: Option<&str>) -> Records {
        self.title_key = Some(title.into());
        self.subtitle_key = subtitle.map(Into::into);
        self
    }

    pub fn column(mut self, key: &str, title: &str) -> Records {
        self.columns.push(Column {
            key: key.into(),
            title: title.into(),
            kind: CellKind::Text,
            text: None,
        });
        self
    }

    /// A meter column: the share at `key` (0 to 1), the words at `text`.
    pub fn meter(mut self, key: &str, title: &str, text: &str) -> Records {
        self.columns.push(Column {
            key: key.into(),
            title: title.into(),
            kind: CellKind::Meter,
            text: Some(text.into()),
        });
        self
    }

    /// An editable cell ([`RowField`]).
    pub fn row_field(mut self, field: RowField) -> Records {
        self.row_fields.push(field);
        self
    }

    /// The row field shown in place of a column, by its key.
    pub fn field_for(&self, key: &str) -> Option<&RowField> {
        self.row_fields.iter().find(|f| f.key == key)
    }

    /// The row fields with no column of their key: a table adds a column
    /// for each.
    pub fn extra_fields(&self) -> impl Iterator<Item = &RowField> {
        self.row_fields
            .iter()
            .filter(|f| !self.columns.iter().any(|c| c.key == f.key))
    }

    pub fn tone(mut self, key: &str) -> Records {
        self.tone_key = Some(key.into());
        self
    }

    pub fn row_action(mut self, action: Action) -> Records {
        self.row_actions.push(action);
        self
    }

    pub fn empty(mut self, text: &str, action: Option<Action>) -> Records {
        self.empty = Some(Empty {
            text: text.into(),
            action,
        });
        self
    }

    pub fn refresh(mut self, event: &str) -> Records {
        self.refresh.push(event.into());
        self
    }

    /// The rows in a source's answer: the array at the pointer, its
    /// objects only; an object there is one record (a form over one
    /// record, as a strategy).
    pub fn rows_of(answer: &Json, pointer: &str) -> Vec<Json> {
        match answer.pointer(pointer) {
            Some(Json::Array(a)) => a.iter().filter(|r| r.is_object()).cloned().collect(),
            Some(o @ Json::Object(_)) => vec![o.clone()],
            _ => Vec::new(),
        }
    }

    /// The values a row's actions and fields read: the form's, and the
    /// record's as `row.<key>`.
    pub fn row_values(values: &hooks::Values, row: &Json) -> hooks::Values {
        let mut v = values.clone();
        if let Some(o) = row.as_object() {
            for (k, x) in o {
                v.insert(format!("row.{k}"), x.clone());
            }
        }
        v
    }
}

/// The context a row's clauses read: the section's, and the record's
/// values as `row.<key>` (booleans `true` or `false`, texts as they are).
pub fn row_context(mut cx: WhenContext, row: &Json) -> WhenContext {
    if let Some(o) = row.as_object() {
        for (k, v) in o {
            cx.set(&format!("row.{k}"), &cell_text(v));
        }
    }
    cx
}

/// A record value as shown in a cell: text as it is, numbers and
/// booleans written out, nothing for null.
pub fn cell_text(v: &Json) -> String {
    match v {
        Json::Null => String::new(),
        Json::String(s) => s.clone(),
        Json::Array(a) => a.iter().map(cell_text).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

/// A tone from its name (a record's tone key, a status's answer).
pub fn tone_named(name: &str) -> Option<Tone> {
    match name {
        "info" => Some(Tone::Info),
        "success" | "ok" | "fine" => Some(Tone::Success),
        "warning" | "warn" => Some(Tone::Warning),
        "danger" | "error" | "problem" => Some(Tone::Danger),
        _ => None,
    }
}

/// The context a section's clauses read: the application's keys, and the
/// section's values as `form.<field id>` (booleans `true` or `false`,
/// texts as they are, numbers written out).
pub fn form_context(mut cx: WhenContext, values: &hooks::Values) -> WhenContext {
    for (k, v) in values {
        cx.set(&format!("form.{k}"), &cell_text(v));
    }
    cx
}

/// Whether a clause holds (none: it does). One that does not parse
/// holds: [`Manifest::check`] reports it.
pub fn holds(clause: Option<&str>, cx: &WhenContext) -> bool {
    clause.is_none_or(|c| When::parse(c).map_or(true, |w| w.eval(cx)))
}

/// A registered widget kind and its options: the declared exception.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Widget {
    #[serde(flatten)]
    pub meta: Meta,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Json::is_null")]
    pub options: Json,
}

impl Widget {
    /// The settings it edits, its options' `settings` (setting ids).
    pub fn settings(&self) -> impl Iterator<Item = &str> {
        self.options
            .get("settings")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(Json::as_str)
    }
}

/// One owner's contribution: scopes and sections, each section placed in
/// its scope. Versioned ([`FORMAT`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<Scope>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<Section>,
    /// Multi-step flows ([`wizard`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wizards: Vec<Wizard>,
}

/// Why a manifest cannot be read.
#[derive(Clone, Debug, PartialEq)]
pub enum ManifestError {
    /// Written for a newer framework.
    Newer { version: u32, supported: u32 },
    /// Not a manifest.
    Parse(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Newer { version, supported } => write!(
                f,
                "manifest format {version} is newer than this framework's ({supported})"
            ),
            ManifestError::Parse(e) => write!(f, "not a manifest: {e}"),
        }
    }
}

impl std::error::Error for ManifestError {}

/// A section being built ([`Manifest::section`]).
#[derive(Debug)]
pub struct SectionBuilder<'a> {
    section: &'a mut Section,
}

impl SectionBuilder<'_> {
    /// Opens a group (a heading and its rows); the items after it go in
    /// it.
    pub fn group(&mut self, id: &str, title: &str) -> &mut Self {
        let order = self.section.groups.len() as i32;
        self.section.groups.push(Group {
            meta: Meta {
                order,
                ..Meta::new(id, title)
            },
            items: Vec::new(),
        });
        self
    }

    fn push(&mut self, item: Item) -> &mut Self {
        if self.section.groups.is_empty() {
            self.group("main", "");
        }
        if let Some(g) = self.section.groups.last_mut() {
            g.items.push(item);
        }
        self
    }

    pub fn field(&mut self, field: Field) -> &mut Self {
        self.push(Item::Field(field))
    }

    pub fn action(&mut self, action: Action) -> &mut Self {
        self.push(Item::Action(action))
    }

    pub fn status(&mut self, status: Status) -> &mut Self {
        self.push(Item::Status(status))
    }

    pub fn callout(&mut self, callout: Callout) -> &mut Self {
        self.push(Item::Callout(callout))
    }

    pub fn widget(&mut self, id: &str, kind: &str, options: Json) -> &mut Self {
        self.push(Item::Widget(Widget {
            meta: Meta::new(id, ""),
            kind: kind.into(),
            options,
        }))
    }

    /// A record list.
    pub fn list(&mut self, records: Records) -> &mut Self {
        self.push(Item::List(records))
    }

    /// A record table.
    pub fn table(&mut self, records: Records) -> &mut Self {
        self.push(Item::Table(records))
    }

    /// The last group's `when` clause.
    pub fn group_when(&mut self, clause: &str) -> &mut Self {
        if let Some(g) = self.section.groups.last_mut() {
            g.meta.when = Some(clause.into());
        }
        self
    }

    /// The section's `when` clause.
    pub fn when(&mut self, clause: &str) -> &mut Self {
        self.section.meta.when = Some(clause.into());
        self
    }

    pub fn description(&mut self, text: &str) -> &mut Self {
        self.section.meta.description = text.into();
        self
    }

    pub fn keywords(&mut self, words: &[&str]) -> &mut Self {
        self.section.meta.keywords = words.iter().map(|w| w.to_string()).collect();
        self
    }

    pub fn under(&mut self, parent: &str) -> &mut Self {
        self.section.parent = Some(parent.into());
        self
    }
}

impl Manifest {
    /// An empty manifest of a format version.
    pub fn new(version: u32) -> Manifest {
        Manifest {
            version,
            scopes: Vec::new(),
            sections: Vec::new(),
            wizards: Vec::new(),
        }
    }

    /// A wizard ([`wizard`]): opened by `admin.wizard.open` or an action.
    pub fn wizard(mut self, wizard: Wizard) -> Manifest {
        self.wizards.push(wizard);
        self
    }

    /// A scope placed among the others by `order` (the default scopes
    /// are 0, 10, 20 and 110: [`generate::SCOPES`]).
    pub fn scope_at(mut self, id: &str, title: &str, icon: &str, order: i32) -> Manifest {
        self = self.scope(id, title, icon);
        if let Some(s) = self.scopes.last_mut() {
            s.meta.order = order;
        }
        self
    }

    /// A scope: a tab with an icon.
    pub fn scope(mut self, id: &str, title: &str, icon: &str) -> Manifest {
        let order = self.scopes.len() as i32;
        self.scopes.push(Scope {
            meta: Meta {
                icon: Some(icon.into()),
                order,
                ..Meta::new(id, title)
            },
        });
        self
    }

    /// A section at `scope/section`, filled by `build`.
    pub fn section(
        mut self,
        path: &str,
        title: &str,
        build: impl FnOnce(&mut SectionBuilder<'_>),
    ) -> Manifest {
        let (scope, id) = path.split_once('/').unwrap_or(("general", path));
        let mut section = Section {
            meta: Meta {
                order: self.sections.len() as i32,
                ..Meta::new(id, title)
            },
            scope: scope.into(),
            parent: None,
            groups: Vec::new(),
        };
        build(&mut SectionBuilder {
            section: &mut section,
        });
        self.sections.push(section);
        self
    }

    pub fn to_json(&self) -> Json {
        serde_json::to_value(self).unwrap_or(Json::Null)
    }

    /// The settings its fields bind, and those its widgets edit (their
    /// options' `settings`): the generated pages leave them out, so a
    /// setting a manifest places shows once, where it is placed.
    pub fn bound_settings(&self) -> impl Iterator<Item = &str> {
        self.sections
            .iter()
            .flat_map(|s| &s.groups)
            .flat_map(|g| &g.items)
            .flat_map(|i| -> Box<dyn Iterator<Item = &str> + '_> {
                match i {
                    Item::Field(f) => Box::new(f.setting_id().into_iter()),
                    Item::Widget(w) => Box::new(w.settings()),
                    _ => Box::new(std::iter::empty()),
                }
            })
    }

    /// Reads a manifest, refusing one newer than [`FORMAT`]. An older one
    /// would be migrated here (there is none yet).
    pub fn from_json(json: &Json) -> Result<Manifest, ManifestError> {
        let version =
            json.get("version")
                .and_then(Json::as_u64)
                .ok_or_else(|| ManifestError::Parse("no `version`".into()))? as u32;
        if version > FORMAT {
            return Err(ManifestError::Newer {
                version,
                supported: FORMAT,
            });
        }
        serde_json::from_value(json.clone()).map_err(|e| ManifestError::Parse(e.to_string()))
    }

    /// Checks it: ids unique among siblings, sections in a scope that
    /// exists (`scopes` names the scopes known elsewhere), settings bound
    /// that exist. What fails is reported and left out, never a panic.
    pub fn check(
        &mut self,
        scopes: &[String],
        setting_exists: &dyn Fn(&str) -> bool,
    ) -> Diagnostics {
        let mut d = Diagnostics::new();
        let mut seen: Vec<String> = Vec::new();
        self.scopes.retain(|s| {
            if seen.contains(&s.meta.id) {
                d.warn(
                    "admin-duplicate",
                    None,
                    &s.meta.id,
                    "a scope with this id is already declared: left out",
                );
                return false;
            }
            seen.push(s.meta.id.clone());
            true
        });
        let known: Vec<String> = seen.iter().chain(scopes).cloned().collect();
        let mut paths: Vec<String> = Vec::new();
        self.sections.retain_mut(|s| {
            if !known.contains(&s.scope) {
                d.warn(
                    "admin-no-scope",
                    None,
                    s.path(),
                    format!("no scope `{}`: left out", s.scope),
                );
                return false;
            }
            if paths.contains(&s.path()) {
                d.warn(
                    "admin-duplicate",
                    None,
                    s.path(),
                    "a section with this path is already declared: left out",
                );
                return false;
            }
            paths.push(s.path());
            let at = s.path();
            bad_clauses(&mut d, &at, &mut s.meta);
            for g in &mut s.groups {
                bad_clauses(&mut d, &at, &mut g.meta);
                let mut ids: Vec<String> = Vec::new();
                g.items.retain(|item| {
                    let id = item.meta().id.clone();
                    if ids.contains(&id) {
                        d.warn(
                            "admin-duplicate",
                            None,
                            format!("{}/{}/{id}", s.scope, s.meta.id),
                            "an item with this id is already in the group: left out",
                        );
                        return false;
                    }
                    ids.push(id.clone());
                    for clause in [&item.meta().when, &item.meta().enabled]
                        .into_iter()
                        .flatten()
                    {
                        if let Err(e) = When::parse(clause) {
                            d.warn(
                                "admin-bad-clause",
                                None,
                                format!("{}/{}/{id}", s.scope, s.meta.id),
                                format!("`{clause}` does not parse ({e}): it always holds"),
                            );
                        }
                    }
                    if let Item::Field(f) = item
                        && let Some(setting) = f.setting_id()
                        && !setting_exists(setting)
                    {
                        d.warn(
                            "admin-no-setting",
                            None,
                            format!("{}/{}/{id}", s.scope, s.meta.id),
                            format!("no setting `{setting}`: left out"),
                        );
                        return false;
                    }
                    true
                });
            }
            true
        });
        let mut ids: Vec<String> = Vec::new();
        self.wizards.retain_mut(|w| {
            if ids.contains(&w.meta.id) {
                d.warn(
                    "admin-duplicate",
                    None,
                    &w.meta.id,
                    "a wizard with this id is already declared: left out",
                );
                return false;
            }
            ids.push(w.meta.id.clone());
            w.check(&mut d, setting_exists);
            true
        });
        d
    }
}

/// Reports a section's or group's clause that does not parse, and drops
/// it (it would always hold).
pub(crate) fn bad_clauses(d: &mut Diagnostics, at: &str, meta: &mut Meta) {
    for clause in [&mut meta.when, &mut meta.enabled] {
        if let Some(c) = clause.as_deref()
            && let Err(e) = When::parse(c)
        {
            d.warn(
                "admin-bad-clause",
                None,
                at,
                format!("`{c}` does not parse ({e}): left out"),
            );
            *clause = None;
        }
    }
}

/// The resolved tree: every manifest merged, scopes and sections in
/// order.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Tree {
    pub scopes: Vec<Scope>,
    pub sections: Vec<Section>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub wizards: Vec<Wizard>,
}

/// Where a deep link leads: `scope/section/key` (any tail may be left
/// out), or a setting id alone, resolved to its first place.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Place {
    pub scope: String,
    pub section: Option<String>,
    /// An item's id (a setting's id for a generated field).
    pub key: Option<String>,
}

impl Tree {
    /// Merges a manifest: its scopes after the ones there, its sections
    /// appended (a section at a path already there adds its groups to
    /// it).
    pub fn merge(&mut self, m: Manifest) {
        for s in m.scopes {
            if !self.scopes.iter().any(|x| x.meta.id == s.meta.id) {
                self.scopes.push(s);
            }
        }
        for s in m.sections {
            match self.sections.iter_mut().find(|x| x.path() == s.path()) {
                Some(x) => x.groups.extend(s.groups),
                None => self.sections.push(s),
            }
        }
        for w in m.wizards {
            if !self.wizards.iter().any(|x| x.meta.id == w.meta.id) {
                self.wizards.push(w);
            }
        }
        self.scopes.sort_by_key(|s| s.meta.order);
        self.sections.sort_by_key(|s| s.meta.order);
    }

    /// Scopes with at least one section.
    pub fn shown_scopes(&self) -> Vec<&Scope> {
        self.scopes
            .iter()
            .filter(|s| self.sections.iter().any(|x| x.scope == s.meta.id))
            .collect()
    }

    /// A scope's sections, in order.
    pub fn sections_of(&self, scope: &str) -> Vec<&Section> {
        self.sections.iter().filter(|s| s.scope == scope).collect()
    }

    pub fn section(&self, path: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.path() == path)
    }

    pub fn wizard(&self, id: &str) -> Option<&Wizard> {
        self.wizards.iter().find(|w| w.meta.id == id)
    }

    /// Resolves a deep link: `scope`, `scope/section`,
    /// `scope/section/key`, or a setting id alone (its first place: a
    /// field bound to it, or a widget that edits it).
    pub fn resolve(&self, link: &str) -> Option<Place> {
        let parts: Vec<&str> = link.splitn(3, '/').collect();
        if self.scopes.iter().any(|s| s.meta.id == parts[0]) {
            let section = parts.get(1).map(|s| s.to_string());
            if let Some(sec) = &section
                && self.section(&format!("{}/{sec}", parts[0])).is_none()
            {
                return None;
            }
            return Some(Place {
                scope: parts[0].into(),
                section,
                key: parts.get(2).map(|s| s.to_string()),
            });
        }
        self.sections.iter().find_map(|s| {
            s.groups
                .iter()
                .flat_map(|g| &g.items)
                .any(|i| match i {
                    Item::Field(f) => f.setting_id() == Some(link),
                    Item::Widget(w) => w.settings().any(|x| x == link),
                    _ => false,
                })
                .then(|| Place {
                    scope: s.scope.clone(),
                    section: Some(s.meta.id.clone()),
                    key: Some(link.into()),
                })
        })
    }

    /// Every field bound to a setting, with its section's path.
    pub fn fields(&self) -> Vec<(String, &Field)> {
        self.sections
            .iter()
            .flat_map(|s| {
                s.groups.iter().flat_map(move |g| {
                    g.items.iter().filter_map(move |i| match i {
                        Item::Field(f) => Some((s.path(), f)),
                        _ => None,
                    })
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Manifest {
        Manifest::new(FORMAT)
            .scope("backup", "Backup", "shield")
            .section("backup/strategy", "Strategy", |s| {
                s.description("How many copies, and where.")
                    .group("copies", "Copies")
                    .field(Field::setting("backup.copies").scopes(&[ValueScope::Library]))
                    .field(
                        Field::draft("import", "into", "Into", FieldKind::Path)
                            .validate("writable"),
                    )
                    .status(Status::command(
                        "last",
                        "Last backup",
                        "backup.status",
                        "/last",
                    ))
                    .action(
                        Action::command("library.backupNow")
                            .primary()
                            .args(json!({"copies": "${copies}"})),
                    )
                    .callout(Callout::new("hint", Tone::Warning, "No disk yet."))
                    .widget("browser", "file-browser", json!({"modes": ["grid"]}));
            })
    }

    #[test]
    fn a_widget_claims_the_settings_it_edits() {
        let m = Manifest::new(FORMAT).section("interface/fonts", "Fonts", |s| {
            s.field(Field::setting("text.size")).widget(
                "fonts",
                "fonts",
                json!({ "settings": ["text.font_ui", "text.font_mono"] }),
            );
        });
        let bound: Vec<&str> = m.bound_settings().collect();
        assert_eq!(bound, ["text.size", "text.font_ui", "text.font_mono"]);
    }

    #[test]
    fn a_manifest_round_trips_through_json() {
        let m = sample();
        let json = m.to_json();
        assert_eq!(json["version"], FORMAT);
        assert_eq!(
            json["sections"][0]["groups"][0]["items"][0]["node"],
            "field"
        );
        assert_eq!(Manifest::from_json(&json).unwrap(), m);
        let text = serde_json::to_string(&m).unwrap();
        let back: Manifest = serde_json::from_str(&text).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn a_newer_manifest_is_refused() {
        let mut json = sample().to_json();
        json["version"] = json!(FORMAT + 1);
        assert!(matches!(
            Manifest::from_json(&json),
            Err(ManifestError::Newer { .. })
        ));
        assert!(matches!(
            Manifest::from_json(&json!({})),
            Err(ManifestError::Parse(_))
        ));
    }

    #[test]
    fn the_check_leaves_out_what_does_not_resolve() {
        let mut m = sample().section("nowhere/x", "X", |s| {
            s.field(Field::setting("a"));
        });
        let d = m.check(&[], &|id| id != "backup.copies");
        assert_eq!(m.sections.len(), 1);
        // The unknown setting is left out; the draft field stays.
        assert_eq!(m.sections[0].groups[0].items.len(), 5);
        let codes: Vec<&str> = d.all().iter().map(|x| x.code).collect();
        assert!(codes.contains(&"admin-no-scope"), "{codes:?}");
        assert!(codes.contains(&"admin-no-setting"), "{codes:?}");
    }

    #[test]
    fn deep_links_resolve_to_a_place() {
        let mut t = Tree::default();
        t.merge(sample());
        let p = t.resolve("backup/strategy/copies").unwrap();
        assert_eq!(p.section.as_deref(), Some("strategy"));
        assert_eq!(p.key.as_deref(), Some("copies"));
        let p = t.resolve("backup.copies").unwrap();
        assert_eq!(
            (p.scope.as_str(), p.key.as_deref()),
            ("backup", Some("backup.copies"))
        );
        assert!(t.resolve("backup/none").is_none());
        assert!(t.resolve("nothing").is_none());
        assert_eq!(t.shown_scopes().len(), 1);
        // A setting a widget edits resolves to the widget's section.
        t.merge(Manifest::new(FORMAT).section("backup/disks", "Disks", |s| {
            s.widget(
                "page",
                "settings-page",
                json!({ "settings": ["backup.disk"] }),
            );
        }));
        let p = t.resolve("backup.disk").unwrap();
        assert_eq!(
            (p.section.as_deref(), p.key.as_deref()),
            (Some("disks"), Some("backup.disk"))
        );
    }

    #[test]
    fn records_read_their_rows_and_round_trip() {
        let r = Records::command("volumes", "Volumes", "storage.volumes", "/volumes", "id")
            .column("name", "Name")
            .tone("level")
            .row_action(Action::command("storage.forget").args(json!({"volume": "${row.id}"})))
            .empty("No disk yet.", None)
            .when("form.copies > 0");
        let m = Manifest::new(FORMAT)
            .scope("storage", "Storage", "disk")
            .section("storage/volumes", "Volumes", |s| {
                s.table(r.clone());
            });
        assert_eq!(Manifest::from_json(&m.to_json()).unwrap(), m);
        assert_eq!(
            m.to_json()["sections"][0]["groups"][0]["items"][0]["node"],
            "table"
        );
        let answer = json!({"volumes": [{"id": "a", "name": "Disk"}, 3]});
        assert_eq!(Records::rows_of(&answer, "/volumes").len(), 1);
        assert!(Records::rows_of(&answer, "/none").is_empty());
        assert_eq!(cell_text(&json!(["a", 2])), "a, 2");
        assert_eq!(tone_named("problem"), Some(Tone::Danger));
    }

    #[test]
    fn clauses_read_the_forms_values_and_bad_ones_are_reported() {
        let mut v = hooks::Values::new();
        v.insert("copy".into(), json!(true));
        v.insert("mode".into(), json!("fast"));
        let cx = form_context(WhenContext::new(), &v);
        assert!(holds(Some("form.copy"), &cx));
        assert!(holds(Some("form.mode == fast"), &cx));
        assert!(!holds(Some("!form.copy"), &cx));
        assert!(holds(None, &cx));
        let mut m = Manifest::new(FORMAT)
            .scope("a", "A", "x")
            .section("a/b", "B", |s| {
                s.when("((")
                    .action(Action::command("x").enabled("(((", "never"));
            });
        let d = m.check(&[], &|_| true);
        let codes: Vec<&str> = d.all().iter().map(|x| x.code).collect();
        assert_eq!(codes, ["admin-bad-clause", "admin-bad-clause"]);
        assert!(m.sections[0].meta.when.is_none());
    }

    #[test]
    fn row_fields_fill_their_command_with_the_row_and_the_value() {
        let f = RowField::text(
            "name",
            "Name",
            "storage.volumeSet",
            json!({"volume": "${row.id}", "name": "${value}"}),
        );
        let row = json!({"id": "d1", "name": "Disk", "dests": [{"value": "a", "label": "A"}, 3]});
        let v = Records::row_values(&hooks::Values::new(), &row);
        assert_eq!(
            f.write_args(&v, json!("Studio")),
            json!({"volume": "d1", "name": "Studio"})
        );
        let c = RowField::choice("dest", "To", &[(json!("any"), "Any")], "x", Json::Null)
            .choices_from("dests");
        let labels: Vec<String> = c.choices_of(&row).into_iter().map(|c| c.label).collect();
        assert_eq!(labels, ["Any", "A"]);
        let r = Records::command("v", "", "c", "", "id")
            .column("name", "Name")
            .meter("share", "Space", "spaceText")
            .row_field(f)
            .row_field(RowField::toggle("raid", "RAID", "x", Json::Null).when("row.online"));
        assert!(r.field_for("name").is_some());
        assert_eq!(r.extra_fields().count(), 1);
        assert_eq!(
            Manifest::from_json(&Manifest::new(FORMAT).to_json())
                .unwrap()
                .version,
            FORMAT
        );
        let cx = row_context(WhenContext::new(), &json!({"online": true, "weak": false}));
        assert!(holds(Some("row.online"), &cx));
        assert!(!holds(Some("row.weak"), &cx));
        assert_eq!(meter_share(&json!(1.5)), Some(1.0));
        assert_eq!(meter_share(&json!("x")), None);
        // One object at the pointer is one record.
        assert_eq!(
            Records::rows_of(&json!({"s": {"set": true}}), "/s").len(),
            1
        );
        let m = Manifest::new(FORMAT)
            .scope("s", "S", "x")
            .section("s/v", "V", |s| {
                s.table(r.clone());
            });
        assert_eq!(Manifest::from_json(&m.to_json()).unwrap(), m);
    }

    #[test]
    fn a_confirmation_names_the_row() {
        let a = Action::command("storage.forget")
            .confirmation(Confirmation::new("Forget ${row.name}?", "It goes.", "Forget").danger());
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(json["confirmation"]["danger"], true);
        let back: Action = serde_json::from_value(json).unwrap();
        assert_eq!(back, a);
        let v = Records::row_values(&hooks::Values::new(), &json!({"name": "Disk"}));
        let c = a.confirmation.unwrap().filled(&v);
        assert_eq!(c.title, "Forget Disk?");
        assert!(c.danger);
    }

    #[test]
    fn there_are_ten_field_kinds() {
        assert_eq!(FieldKind::ALL.len(), 10);
        let names: Vec<String> = FieldKind::ALL
            .iter()
            .map(|k| {
                serde_json::to_value(k)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert!(names.contains(&"long-text".to_string()));
    }
}
