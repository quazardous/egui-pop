//! Commands: every action of the application, named. A menu, a key, the
//! palette, an agent and a test all run the same command by its id, and
//! each run says where it came from.
//!
//! A [`CommandSpec`] is declared once and the rest is derived from it
//! (its key bindings, menu entries, palette entry and agent surface).
//! The registry is generic over the handler type so an upper layer can
//! give handlers whatever context it has; [`FnHandler`] is the plain one.
//!
//! ```
//! use egui_pop_model::commands::{CommandSpec, Commands, Origin};
//! use egui_pop_model::ids::{CommandId, OwnerId};
//! use egui_pop_model::when::WhenContext;
//! use serde_json::json;
//!
//! let mut commands = Commands::new();
//! let id = CommandId::from_static("demo.echo");
//! commands
//!     .register_fn(&OwnerId::FRAMEWORK, CommandSpec::new(id.clone(), "Echo", "Demo"), |args, _| {
//!         Ok(args.clone())
//!     })
//!     .unwrap();
//! let out = commands.run(&id, json!(3), Origin::User, &WhenContext::new());
//! assert_eq!(out, Ok(json!(3)));
//! ```

use std::collections::VecDeque;
use std::fmt;

use serde::Serialize;
use serde_json::Value as Json;

use crate::ids::{CommandId, MenuId, OwnerId};
use crate::registry::Registry;
use crate::when::{When, WhenContext};

/// Who asked for a command: shown in the history, and used to keep an
/// agent from fighting a human's gesture.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "lowercase")]
pub enum Origin {
    /// A person, with the mouse or the keyboard.
    User,
    /// An automated agent, by session.
    Agent(String),
    /// A script, by name.
    Script(String),
    /// A hardware controller, by name.
    Device(String),
    /// Another process or machine.
    Remote(String),
}

/// Where a command's menu entry goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MenuPlacement {
    pub menu: MenuId,
    pub group: String,
    pub order: i32,
}

/// What a command declares.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct CommandSpec {
    /// Stable, never translated: `view.palette`.
    pub id: CommandId,
    pub title: String,
    pub category: String,
    /// What it does, for the palette and agents.
    pub description: String,
    /// Where the command applies; empty: everywhere.
    pub when: String,
    /// Its default key bindings (`Ctrl+K`, `Ctrl+K Ctrl+S`).
    pub keys: Vec<String>,
    /// Its menu entries.
    pub menus: Vec<MenuPlacement>,
    /// Listed in the command palette.
    pub palette: bool,
    /// Offered to agents through the debug bridge.
    pub agent: bool,
    /// A description of its arguments (free JSON, a schema or an
    /// example); `null` when it takes none.
    pub args: Json,
}

impl CommandSpec {
    pub fn new(id: CommandId, title: &str, category: &str) -> Self {
        Self {
            id,
            title: title.into(),
            category: category.into(),
            description: String::new(),
            when: String::new(),
            keys: Vec::new(),
            menus: Vec::new(),
            palette: true,
            agent: true,
            args: Json::Null,
        }
    }

    pub fn when(mut self, when: &str) -> Self {
        self.when = when.into();
        self
    }

    pub fn description(mut self, text: &str) -> Self {
        self.description = text.into();
        self
    }

    /// Adds a default key binding.
    pub fn key(mut self, keys: &str) -> Self {
        self.keys.push(keys.into());
        self
    }

    /// Adds a menu entry.
    pub fn menu(mut self, menu: MenuId, group: &str, order: i32) -> Self {
        self.menus.push(MenuPlacement {
            menu,
            group: group.into(),
            order,
        });
        self
    }

    /// Kept out of the palette.
    pub fn hidden(mut self) -> Self {
        self.palette = false;
        self
    }

    /// Kept away from agents.
    pub fn no_agent(mut self) -> Self {
        self.agent = false;
        self
    }

    /// Describes the arguments.
    pub fn args(mut self, args: Json) -> Self {
        self.args = args;
        self
    }
}

/// The plain handler: arguments and origin in, a JSON result out.
pub type FnHandler = Box<dyn Fn(&Json, &Origin) -> Result<Json, String> + Send + Sync>;

/// Why a command could not be registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegisterError {
    /// Its `when` clause does not parse.
    BadWhen { id: CommandId, error: String },
    /// The id is taken.
    Duplicate { id: CommandId, by: OwnerId },
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegisterError::BadWhen { id, error } => write!(f, "command `{id}`: {error}"),
            RegisterError::Duplicate { id, by } => {
                write!(f, "command `{id}` is already declared by `{by}`")
            }
        }
    }
}

impl std::error::Error for RegisterError {}

/// Why a command did not run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "refused", rename_all = "snake_case")]
pub enum RunRefused {
    Unknown { id: String },
    NotHere { id: String, when: String },
}

impl fmt::Display for RunRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunRefused::Unknown { id } => write!(f, "no command `{id}`"),
            RunRefused::NotHere { id, when } => write!(f, "`{id}` does not apply here ({when})"),
        }
    }
}

/// A registered command.
pub struct Entry<H> {
    pub spec: CommandSpec,
    when: When,
    pub handler: H,
}

impl<H> fmt::Debug for Entry<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry").field("spec", &self.spec).finish()
    }
}

/// One run, as the history keeps it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Run {
    pub id: CommandId,
    pub args: Json,
    pub origin: Origin,
    pub result: Result<Json, String>,
}

/// How many runs the history keeps.
pub const HISTORY: usize = 256;

/// The commands, by id, each owned.
pub struct Commands<H = FnHandler> {
    entries: Registry<CommandId, Entry<H>>,
    history: VecDeque<Run>,
}

impl<H> Default for Commands<H> {
    fn default() -> Self {
        Self {
            entries: Registry::new(),
            history: VecDeque::new(),
        }
    }
}

impl<H> fmt::Debug for Commands<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Commands")
            .field("ids", &self.specs().map(|s| &s.id).collect::<Vec<_>>())
            .field("history", &self.history.len())
            .finish()
    }
}

impl<H> Commands<H> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares a command; a second declaration of an id is refused.
    pub fn register(
        &mut self,
        owner: &OwnerId,
        spec: CommandSpec,
        handler: H,
    ) -> Result<(), RegisterError> {
        let when = When::parse(&spec.when).map_err(|error| RegisterError::BadWhen {
            id: spec.id.clone(),
            error,
        })?;
        let id = spec.id.clone();
        self.entries
            .insert(
                owner,
                id,
                Entry {
                    spec,
                    when,
                    handler,
                },
            )
            .map_err(|t| RegisterError::Duplicate { id: t.id, by: t.by })
    }

    pub fn spec(&self, id: &CommandId) -> Option<&CommandSpec> {
        self.entries.get(id).map(|e| &e.spec)
    }

    pub fn owner(&self, id: &CommandId) -> Option<&OwnerId> {
        self.entries.owner(id)
    }

    /// Every declared command, by id.
    pub fn specs(&self) -> impl Iterator<Item = &CommandSpec> {
        self.entries.iter().map(|(_, s)| &s.item.spec)
    }

    /// Whether a command applies in a context.
    pub fn applies(&self, id: &CommandId, cx: &WhenContext) -> bool {
        self.entries.get(id).is_some_and(|e| e.when.eval(cx))
    }

    /// The commands that apply in a context.
    pub fn available(&self, cx: &WhenContext) -> Vec<&CommandSpec> {
        self.entries
            .iter()
            .filter(|(_, s)| s.item.when.eval(cx))
            .map(|(_, s)| &s.item.spec)
            .collect()
    }

    /// Palette commands whose category, title or id contains every word of
    /// `query`, in that context, best first: a title starting with the
    /// query, then one with a word starting with each query word, then
    /// the rest, each by title.
    pub fn search(&self, query: &str, cx: &WhenContext) -> Vec<&CommandSpec> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let q = query.trim().to_lowercase();
        let mut hits: Vec<(u8, &CommandSpec)> = self
            .available(cx)
            .into_iter()
            .filter(|s| s.palette)
            .filter_map(|s| {
                let hay = format!("{} {} {}", s.category, s.title, s.id).to_lowercase();
                if !words.iter().all(|w| hay.contains(w)) {
                    return None;
                }
                let title = s.title.to_lowercase();
                let rank = if !q.is_empty() && title.starts_with(&q) {
                    0
                } else if words.iter().all(|w| {
                    hay.split(|c: char| !c.is_alphanumeric())
                        .any(|t| t.starts_with(w.as_str()))
                }) {
                    1
                } else {
                    2
                };
                Some((rank, s))
            })
            .collect();
        hits.sort_by(|a, b| (a.0, &a.1.title).cmp(&(b.0, &b.1.title)));
        hits.into_iter().map(|(_, s)| s).collect()
    }

    /// The handler of a command that may run here.
    pub fn prepare(&self, id: &CommandId, cx: &WhenContext) -> Result<&H, RunRefused> {
        let entry = self
            .entries
            .get(id)
            .ok_or_else(|| RunRefused::Unknown { id: id.to_string() })?;
        if entry.when.eval(cx) {
            Ok(&entry.handler)
        } else {
            Err(RunRefused::NotHere {
                id: id.to_string(),
                when: entry.spec.when.clone(),
            })
        }
    }

    /// Keeps a run in the history (the oldest goes past [`HISTORY`]).
    pub fn record(&mut self, run: Run) {
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(run);
    }

    /// The runs kept, oldest first.
    pub fn history(&self) -> impl Iterator<Item = &Run> {
        self.history.iter()
    }

    /// Removes an owner's commands; returns their ids.
    pub fn remove_owner(&mut self, owner: &OwnerId) -> Vec<CommandId> {
        self.entries
            .remove_owner(owner)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Commands<FnHandler> {
    /// Declares a command with a plain handler.
    pub fn register_fn(
        &mut self,
        owner: &OwnerId,
        spec: CommandSpec,
        handler: impl Fn(&Json, &Origin) -> Result<Json, String> + Send + Sync + 'static,
    ) -> Result<(), RegisterError> {
        self.register(owner, spec, Box::new(handler))
    }

    /// Runs a command; refused when unknown or when it does not apply
    /// here. A refusal for an unknown id is not a run and is not kept.
    pub fn run(
        &mut self,
        id: &CommandId,
        args: Json,
        origin: Origin,
        cx: &WhenContext,
    ) -> Result<Json, String> {
        let result = match self.prepare(id, cx) {
            Ok(h) => h(&args, &origin),
            Err(e @ RunRefused::Unknown { .. }) => return Err(e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        self.record(Run {
            id: id.clone(),
            args,
            origin,
            result: result.clone(),
        });
        result
    }
}
