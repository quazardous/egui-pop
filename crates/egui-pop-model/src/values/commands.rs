//! `value.list`, `value.get`, `value.set`, `value.reset`, `value.undo` and
//! `value.redo`: the store as commands, what menus, keys, scripts and
//! agents run alike. Values are plain JSON (a number, a bool, a string,
//! an array), typed by the path's descriptor.

use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value as Json, json};

use super::{Change, Entry, Set, Store, Value};
use crate::commands::{CommandSpec, Commands, FnHandler, Origin, RegisterError};
use crate::ids::{CommandId, OwnerId};

fn describe(path: &str, e: &Entry) -> Json {
    json!({
        "path": path,
        "value": e.value.to_json(),
        "default": e.descriptor.default.to_json(),
        "changed": e.value != e.descriptor.default,
        "seq": e.seq,
        "origin": e.origin,
        "owner": e.owner,
        "descriptor": e.descriptor,
    })
}

fn path_of(args: &Json) -> Result<&str, String> {
    args.get("path")
        .and_then(Json::as_str)
        .ok_or_else(|| "`path` is missing".to_string())
}

/// What `value.set` asks for: `value`, `step` (fine steps, negative for
/// down), `coarse`, `reset`, `top` or `bottom`.
fn change_of(store: &Store, path: &str, args: &Json) -> Result<Change, String> {
    let n = |key: &str| args.get(key).and_then(Json::as_i64);
    let flag = |key: &str| args.get(key).and_then(Json::as_bool) == Some(true);
    if let Some(v) = args.get("value") {
        let ty = store
            .get(path)
            .ok_or_else(|| format!("no value at `{path}`"))?
            .descriptor
            .ty;
        Ok(Change::To(Value::from_json(ty, v)?))
    } else if let Some(s) = n("step") {
        Ok(Change::Step(s))
    } else if let Some(s) = n("coarse") {
        Ok(Change::Coarse(s))
    } else if flag("reset") {
        Ok(Change::Reset)
    } else if flag("top") {
        Ok(Change::Top)
    } else if flag("bottom") {
        Ok(Change::Bottom)
    } else {
        Err("say what to set: `value`, `step`, `coarse`, `reset`, `top` or `bottom`".into())
    }
}

fn set(store: &Mutex<Store>, args: &Json, origin: &Origin, reset: bool) -> Result<Json, String> {
    let mut store = store.lock().unwrap_or_else(PoisonError::into_inner);
    let path = path_of(args)?;
    let change = if reset {
        Change::Reset
    } else {
        change_of(&store, path, args)?
    };
    let mut s = Set::new(path, change, origin.clone());
    s.force = args.get("force").and_then(Json::as_bool) == Some(true);
    s.if_seq = args.get("if_seq").and_then(Json::as_u64);
    match store.set(s) {
        Ok(a) => Ok(json!({
            "value": a.value.to_json(),
            "clamped": a.clamped,
            "seq": a.seq,
            "changed": a.changed,
        })),
        // The refusal as JSON, current value included, so an agent can
        // read it and decide.
        Err(r) => Err(serde_json::to_string(&r).unwrap_or_else(|_| r.to_string())),
    }
}

/// Declares the `value.*` commands on a registry, over a shared store,
/// owned by `owner`.
pub fn register(
    commands: &mut Commands,
    owner: &OwnerId,
    store: Arc<Mutex<Store>>,
) -> Result<(), RegisterError> {
    register_with(commands, owner, store, |h| h)
}

/// As [`register`], on a registry with another handler type: `wrap`
/// turns each plain handler into one.
pub fn register_with<H>(
    commands: &mut Commands<H>,
    owner: &OwnerId,
    store: Arc<Mutex<Store>>,
    wrap: impl Fn(FnHandler) -> H,
) -> Result<(), RegisterError> {
    let id = CommandId::from_static;
    let s = store.clone();
    commands.register(
        owner,
        CommandSpec::new(id("value.list"), "List values", "Values")
            .description("The values under a path prefix (`prefix`).")
            .args(json!({ "prefix": "settings/" }))
            .hidden(),
        wrap(Box::new(move |args, _| {
            let prefix = args.get("prefix").and_then(Json::as_str).unwrap_or("");
            let store = s.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(Json::Array(
                store
                    .list(prefix)
                    .into_iter()
                    .map(|(p, e)| describe(p, e))
                    .collect(),
            ))
        })),
    )?;
    let s = store.clone();
    commands.register(
        owner,
        CommandSpec::new(id("value.get"), "Read a value", "Values")
            .description("A value with its descriptor (`path`).")
            .args(json!({ "path": "settings/x" }))
            .hidden(),
        wrap(Box::new(move |args, _| {
            let path = path_of(args)?;
            let store = s.lock().unwrap_or_else(PoisonError::into_inner);
            store
                .get(path)
                .map(|e| describe(path, e))
                .ok_or_else(|| format!("no value at `{path}`"))
        })),
    )?;
    let s = store.clone();
    commands.register(
        owner,
        CommandSpec::new(id("value.set"), "Set a value", "Values")
            .description(
                "`path`; `value`, `step`, `coarse`, `reset`, `top` or `bottom`; `force`, `if_seq`.",
            )
            .args(json!({ "path": "settings/x", "value": 1 }))
            .hidden(),
        wrap(Box::new(move |args, origin| set(&s, args, origin, false))),
    )?;
    let s = store.clone();
    commands.register(
        owner,
        CommandSpec::new(id("value.reset"), "Reset a value", "Values")
            .description("Back to its default (`path`).")
            .args(json!({ "path": "settings/x" }))
            .hidden(),
        wrap(Box::new(move |args, origin| set(&s, args, origin, true))),
    )?;
    let s = store.clone();
    commands.register(
        owner,
        CommandSpec::new(id("value.undo"), "Undo", "Values")
            .description("Undo the last change of values."),
        wrap(Box::new(move |_, origin| {
            let undone = s
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .undo(origin)
                .map_err(|r| r.to_string())?;
            Ok(json!({ "undone": undone }))
        })),
    )?;
    let s = store;
    commands.register(
        owner,
        CommandSpec::new(id("value.redo"), "Redo", "Values")
            .description("Redo the last undone change of values."),
        wrap(Box::new(move |_, origin| {
            let redone = s
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .redo(origin)
                .map_err(|r| r.to_string())?;
            Ok(json!({ "redone": redone }))
        })),
    )?;
    Ok(())
}
