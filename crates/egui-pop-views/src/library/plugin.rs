//! The library's steps (ADR 0011), its commands (the agent surface), the
//! registry of the libraries drawn with an application, and
//! [`LibraryPlugin`] declaring them.
//!
//! | Step | Arguments | Default |
//! |---|---|---|
//! | `library.select` | `library`, `keys`, `cursor` | sets the selection |
//! | `library.rate` | `library`, `keys`, `rating` (0 to 5) | nothing: the application rates its images (the `Rate` signal) |
//! | `library.label` | `library`, `keys`, `label` (`red`, `yellow`, `green`, `blue`, `purple`), `on` | nothing: the application labels them (`Label`) |
//! | `library.flag` | `library`, `keys`, `flag` (`pick`, `reject`, `none`) | nothing: the application flags them (`Flag`) |
//! | `library.tag` | `library`, `keys`, `add`, `remove` (tag lists) | nothing: the application tags them (`Tag`) |
//! | `library.stack` | `library`, `stack` (its top image's key), `expanded` | expands or collapses the stack in the view |
//! | `library.filter` | `library`, `filter` (a [`Filter`](super::filter::Filter)) | sets the search field's filter |
//!
//! The application owns the marks, the tags and the filtering: a mark's
//! default does nothing but report it (the view's output signal, and the
//! journal); the application applies it from the signal or from an
//! after-hook. A before-hook vetoes a mark (`Veto`) or rewrites it
//! (`Redirect`, a rating capped, a label swapped, other keys).
//!
//! Commands (keys, palette, agents), each acting on `library` (else the
//! library that last had the focus) and on `keys` (else its selection):
//! `library.select` (`keys`, `cursor`), `library.rate` (`rating`),
//! `library.label` (`label`, `on`: else toggled from the cursor's image),
//! `library.flag` (`flag`), `library.tag` (`add`, `remove`),
//! `library.filter` (`filter`). A command queues its step for the view,
//! which runs it at its next pass as if the user had asked. The
//! `libraries` debug collector lists the libraries drawn, their
//! selection and filter.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use egui_pop_api::services::ServiceToken;
use egui_pop_api::steps::{Outcome, StepSpec};
use egui_pop_api::{AppHandle, Call, Ctx, Plugin, PluginError};
use egui_pop_core::debug::FnCollector;
use egui_pop_core::lock;
use egui_pop_model::commands::CommandSpec;
use egui_pop_model::ids::{CommandId, PluginId, StepId};
use serde::de::DeserializeOwned;
use serde_json::{Value as Json, json};

use super::marks::{ColorLabel, Flag, MAX_RATING};

pub const LIBRARY_SELECT: StepId = StepId::from_static("library.select");
pub const LIBRARY_RATE: StepId = StepId::from_static("library.rate");
pub const LIBRARY_LABEL: StepId = StepId::from_static("library.label");
pub const LIBRARY_FLAG: StepId = StepId::from_static("library.flag");
pub const LIBRARY_TAG: StepId = StepId::from_static("library.tag");
pub const LIBRARY_STACK: StepId = StepId::from_static("library.stack");
pub const LIBRARY_FILTER: StepId = StepId::from_static("library.filter");

pub const CMD_SELECT: CommandId = CommandId::from_static("library.select");
pub const CMD_RATE: CommandId = CommandId::from_static("library.rate");
pub const CMD_LABEL: CommandId = CommandId::from_static("library.label");
pub const CMD_FLAG: CommandId = CommandId::from_static("library.flag");
pub const CMD_TAG: CommandId = CommandId::from_static("library.tag");
pub const CMD_FILTER: CommandId = CommandId::from_static("library.filter");

/// The plugin's id.
pub const LIBRARY: PluginId = PluginId::from_static("egui_pop.library");

/// How many requests a library keeps waiting for its view.
const INBOX: usize = 64;

/// A step a command asked a library's view to run.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Request {
    pub step: StepId,
    pub args: Json,
}

#[derive(Debug, Default)]
struct Entry {
    count: usize,
    selection: Vec<Json>,
    cursor: Json,
    filter: Json,
    inbox: VecDeque<Request>,
    filter_inbox: Option<Json>,
}

#[derive(Debug, Default)]
struct Inner {
    entries: BTreeMap<String, Entry>,
    focused: Option<String>,
}

/// The libraries drawn with an application, by id: what commands and
/// the collector reach. A grid, a filmstrip or a search field registers
/// its library when drawn with `.app(...)`.
#[derive(Debug, Default)]
pub struct Libraries {
    inner: Mutex<Inner>,
}

/// The registry, a service of [`LibraryPlugin`].
pub const LIBRARIES: ServiceToken<Libraries> = ServiceToken::new("egui_pop.library");

impl Libraries {
    /// The libraries' ids.
    pub fn ids(&self) -> Vec<String> {
        lock(&self.inner).entries.keys().cloned().collect()
    }

    /// The library that last had the focus.
    pub fn focused(&self) -> Option<String> {
        lock(&self.inner).focused.clone()
    }

    /// What the collector shows: each library's count, selection (the
    /// first hundred keys), cursor and filter.
    pub fn describe(&self) -> Json {
        let i = lock(&self.inner);
        let all: serde_json::Map<String, Json> = i
            .entries
            .iter()
            .map(|(id, e)| {
                (
                    id.clone(),
                    json!({
                        "count": e.count,
                        "selected": e.selection.len(),
                        "selection": e.selection.iter().take(100).collect::<Vec<_>>(),
                        "cursor": e.cursor,
                        "filter": e.filter,
                        "waiting": e.inbox.len(),
                    }),
                )
            })
            .collect();
        json!({ "libraries": all, "focused": i.focused })
    }

    pub(crate) fn publish(&self, id: &str, count: usize, selection: Vec<Json>, cursor: Json) {
        let mut i = lock(&self.inner);
        let e = i.entries.entry(id.to_string()).or_default();
        e.count = count;
        e.selection = selection;
        e.cursor = cursor;
    }

    /// Registers a library (a view drawn for the first time).
    pub(crate) fn touch(&self, id: &str) {
        lock(&self.inner).entries.entry(id.to_string()).or_default();
    }

    pub(crate) fn publish_filter(&self, id: &str, filter: Json) {
        lock(&self.inner)
            .entries
            .entry(id.to_string())
            .or_default()
            .filter = filter;
    }

    pub(crate) fn set_focused(&self, id: &str) {
        let mut i = lock(&self.inner);
        if i.focused.as_deref() != Some(id) {
            i.focused = Some(id.to_string());
        }
    }

    pub(crate) fn take(&self, id: &str) -> Vec<Request> {
        lock(&self.inner)
            .entries
            .get_mut(id)
            .map(|e| e.inbox.drain(..).collect())
            .unwrap_or_default()
    }

    pub(crate) fn take_filter(&self, id: &str) -> Option<Json> {
        lock(&self.inner)
            .entries
            .get_mut(id)
            .and_then(|e| e.filter_inbox.take())
    }

    /// The selection published for a library.
    pub fn selection(&self, id: &str) -> Option<Vec<Json>> {
        lock(&self.inner)
            .entries
            .get(id)
            .map(|e| e.selection.clone())
    }

    /// The library a command acts on.
    fn target(&self, args: &Json) -> Result<String, String> {
        let i = lock(&self.inner);
        match args.get("library").and_then(Json::as_str) {
            Some(id) if i.entries.contains_key(id) => Ok(id.to_string()),
            Some(id) => Err(format!("no library `{id}`")),
            None => i
                .focused
                .clone()
                .ok_or_else(|| "no library has had the focus; name one with `library`".to_string()),
        }
    }

    fn push(&self, id: &str, request: Request) {
        let mut i = lock(&self.inner);
        let e = i.entries.entry(id.to_string()).or_default();
        if request.step == LIBRARY_FILTER {
            e.filter_inbox = Some(request.args["filter"].clone());
            return;
        }
        e.inbox.push_back(request);
        while e.inbox.len() > INBOX {
            e.inbox.pop_front();
        }
    }
}

/// Declares the library's steps, its commands and its collector, and
/// provides the [`Libraries`] registry. Connect it when library views
/// run with an application.
#[derive(Debug, Default)]
pub struct LibraryPlugin;

/// Every step the library declares.
pub fn specs() -> Vec<StepSpec> {
    vec![
        StepSpec::new(LIBRARY_SELECT, "A library's selection changes (a click, the keyboard, a rubber band, a command).")
            .args(json!({ "library": "shoot", "keys": [12, 13], "cursor": 13 })),
        StepSpec::new(LIBRARY_RATE, "Images are rated (0 to 5 stars); the application rates them. A veto keeps the rating, a redirect changes it.")
            .args(json!({ "library": "shoot", "keys": [12], "rating": 3 })),
        StepSpec::new(LIBRARY_LABEL, "A colour label is put on images or taken off; the application labels them.")
            .args(json!({ "library": "shoot", "keys": [12], "label": "red", "on": true })),
        StepSpec::new(LIBRARY_FLAG, "Images are picked, rejected or unflagged; the application flags them.")
            .args(json!({ "library": "shoot", "keys": [12], "flag": "pick" })),
        StepSpec::new(LIBRARY_TAG, "Tags are added to images or taken off; the application tags them.")
            .args(json!({ "library": "shoot", "keys": [12], "add": ["beach"], "remove": [] })),
        StepSpec::new(LIBRARY_STACK, "A stack is expanded or collapsed in a library's views.")
            .args(json!({ "library": "shoot", "stack": 40, "expanded": true })),
        StepSpec::new(LIBRARY_FILTER, "A library's filter changes (the search field, its chips, a command); the application filters its images.")
            .args(json!({ "library": "shoot", "filter": { "text": "beach", "min_rating": 3, "labels": ["red"] } })),
    ]
}

impl Plugin for LibraryPlugin {
    fn id(&self) -> PluginId {
        LIBRARY
    }

    fn title(&self) -> String {
        "Photo library".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for spec in specs() {
            ctx.step(spec)?;
        }
        ctx.service(&LIBRARIES, Libraries::default())?;
        commands(ctx)?;
        let reg = ctx.app().service(&LIBRARIES).expect("just provided");
        ctx.collector(
            FnCollector::new(
                "libraries",
                "The photo libraries drawn: their count, selection, cursor, filter and waiting requests.",
            )
            .snapshot(move |_| reg.describe()),
        )?;
        Ok(())
    }
}

/// Queues `step` for the library a command names; `fill` checks and
/// completes the arguments.
fn queue(
    call: &Call<'_>,
    step: StepId,
    fill: impl FnOnce(&Json) -> Result<Json, String>,
) -> Result<Json, String> {
    let reg = call
        .app
        .service(&LIBRARIES)
        .ok_or("the library plugin is not connected")?;
    let id = reg.target(call.args)?;
    let mut args = fill(call.args)?;
    args["library"] = json!(id);
    reg.push(
        &id,
        Request {
            step: step.clone(),
            args: args.clone(),
        },
    );
    Ok(json!({ "library": id, "queued": step.to_string(), "args": args }))
}

/// Copies `keys` when given.
fn with_keys(args: &Json, mut out: Json) -> Json {
    if let Some(k) = args.get("keys") {
        out["keys"] = k.clone();
    }
    out
}

fn commands(ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
    const CAT: &str = "Library";
    ctx.command(
        CommandSpec::new(CMD_SELECT, "Select images", CAT)
            .description("Selects images of a library by key (`keys`, the cursor on `cursor` or the last key), through its `library.select` step.")
            .args(json!({ "library": "shoot (optional: the library that last had the focus)", "keys": [12, 13], "cursor": 13 })),
        |call| {
            queue(call, LIBRARY_SELECT, |a| {
                let keys = a.get("keys").filter(|k| k.is_array()).ok_or("`keys` is a list")?;
                let cursor = a
                    .get("cursor")
                    .cloned()
                    .or_else(|| keys.as_array().and_then(|k| k.last().cloned()))
                    .unwrap_or(Json::Null);
                Ok(json!({ "keys": keys, "cursor": cursor }))
            })
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_RATE, "Rate images", CAT)
            .description("Rates images (`rating`, 0 to 5) of a library: `keys`, else its selection; through its `library.rate` step.")
            .args(json!({ "library": "shoot", "keys": [12], "rating": 3 })),
        |call| {
            queue(call, LIBRARY_RATE, |a| {
                let rating = a
                    .get("rating")
                    .and_then(Json::as_u64)
                    .filter(|r| *r <= u64::from(MAX_RATING))
                    .ok_or("`rating` is 0 to 5")?;
                Ok(with_keys(a, json!({ "rating": rating })))
            })
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_LABEL, "Label images", CAT)
            .description("Puts a colour label (`label`: red, yellow, green, blue, purple) on images or takes it off (`on`; else toggled from the cursor's image): `keys`, else the selection; through `library.label`.")
            .args(json!({ "library": "shoot", "keys": [12], "label": "red", "on": true })),
        |call| {
            queue(call, LIBRARY_LABEL, |a| {
                let label = a
                    .get("label")
                    .and_then(Json::as_str)
                    .and_then(ColorLabel::parse)
                    .ok_or("`label` is red, yellow, green, blue or purple")?;
                let mut out = with_keys(a, json!({ "label": label.name() }));
                if let Some(on) = a.get("on").and_then(Json::as_bool) {
                    out["on"] = json!(on);
                }
                Ok(out)
            })
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_FLAG, "Flag images", CAT)
            .description("Picks, rejects or unflags images (`flag`: pick, reject, none): `keys`, else the selection; through `library.flag`.")
            .args(json!({ "library": "shoot", "keys": [12], "flag": "pick" })),
        |call| {
            queue(call, LIBRARY_FLAG, |a| {
                let flag = a
                    .get("flag")
                    .and_then(Json::as_str)
                    .and_then(Flag::parse)
                    .ok_or("`flag` is pick, reject or none")?;
                Ok(with_keys(a, json!({ "flag": flag.name() })))
            })
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_TAG, "Tag images", CAT)
            .description("Adds tags (`add`) to images and takes others off (`remove`): `keys`, else the selection; through `library.tag`.")
            .args(json!({ "library": "shoot", "keys": [12], "add": ["beach"], "remove": [] })),
        |call| {
            queue(call, LIBRARY_TAG, |a| {
                let list = |k: &str| -> Result<Vec<String>, String> {
                    match a.get(k) {
                        None => Ok(Vec::new()),
                        Some(v) => parse(v, k),
                    }
                };
                let (add, remove) = (list("add")?, list("remove")?);
                if add.is_empty() && remove.is_empty() {
                    return Err("nothing to add (`add`) or remove (`remove`)".into());
                }
                Ok(with_keys(a, json!({ "add": add, "remove": remove })))
            })
        },
    )?;
    ctx.command(
        CommandSpec::new(CMD_FILTER, "Filter images", CAT)
            .description("Sets a library's filter (`filter`: `text`, `min_rating`, `labels`, `flags`, `tags`, `from`, `to`; `{}` clears it), through its search field's `library.filter` step.")
            .args(json!({ "library": "shoot", "filter": { "text": "beach", "min_rating": 3, "labels": ["red"], "flags": ["pick"], "tags": ["sea"], "from": "2024-06-01", "to": "2024-06-30" } })),
        |call| {
            queue(call, LIBRARY_FILTER, |a| {
                let filter = a.get("filter").ok_or("`filter` is missing")?;
                let parsed: super::filter::Filter = parse(filter, "filter")?;
                Ok(json!({ "filter": parsed }))
            })
        },
    )?;
    Ok(())
}

/// A field of step arguments, typed.
pub(crate) fn parse<T: DeserializeOwned>(v: &Json, what: &str) -> Result<T, String> {
    serde_json::from_value(v.clone()).map_err(|e| format!("{what}: {e}"))
}

/// Runs a step whose default only reports it: the arguments it ran
/// with (a before-hook may have redirected them), or why it did not.
pub(crate) fn report(app: Option<&AppHandle>, step: &StepId, args: Json) -> Result<Json, String> {
    let mut ran = None;
    let outcome = crate::steps::run(app, step, args.clone(), |a| {
        ran = Some(a.clone());
        Ok(a.clone())
    });
    match outcome {
        Outcome::Done(Ok(result)) => {
            Ok(ran.unwrap_or(if result.is_object() { result } else { args }))
        }
        Outcome::Done(Err(reason)) | Outcome::Vetoed { reason, .. } => Err(reason),
    }
}
