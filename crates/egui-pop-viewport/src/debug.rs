//! The viewports seen by egui-pop's debug bridge: register
//! [`collector`] and its `viewport` snapshot lists every viewport drawn
//! (view, mapping, hooks set, what the hooks decided, the last signals);
//! its `set_view` action moves a viewport as a command would (through
//! its `before_view` hook, on the next frame).
//!
//! ```
//! use egui_pop_core::debug::DebugBridge;
//!
//! let mut bridge = DebugBridge::with_core_collectors();
//! bridge.register(egui_pop_viewport::debug::collector()).unwrap();
//! ```

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui::{Context, Id};
use egui_pop_core::debug::FnCollector;
use serde_json::{Value as Json, json};

use crate::view::{Mapping, View, Zoom};
use crate::widget::{Signal, ViewportState};

#[derive(Debug, Default)]
struct Registry {
    snapshots: BTreeMap<String, Json>,
    commands: Vec<(String, View)>,
}

fn registry(ctx: &Context) -> Arc<Mutex<Registry>> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<Mutex<Registry>>>(Id::new("egui_pop_viewport::debug"))
            .clone()
    })
}

fn lock(r: &Mutex<Registry>) -> std::sync::MutexGuard<'_, Registry> {
    r.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A view as JSON.
pub fn view_json(view: &View) -> Json {
    match view.zoom {
        Zoom::Fit => json!({ "fit": true }),
        Zoom::Px(z) => json!({ "zoom": z, "center": view.center }),
    }
}

/// A signal as JSON.
pub fn signal_json(signal: &Signal) -> Json {
    match signal {
        Signal::View { view, phase, cause } => json!({
            "signal": "view",
            "phase": phase.name(),
            "cause": cause.name(),
            "view": view_json(view),
        }),
        Signal::Display { target } => json!({
            "signal": "display",
            "target": format!("{target:?}"),
            "supported": target.is_supported(),
        }),
    }
}

pub(crate) fn publish(
    ctx: &Context,
    name: &str,
    state: &ViewportState,
    view: &View,
    m: &Mapping,
    signals: &[Signal],
) {
    let reg = registry(ctx);
    let mut r = lock(&reg);
    let last_signals = r
        .snapshots
        .get(name)
        .and_then(|s| s["signals"].as_array().cloned())
        .unwrap_or_default();
    let mut all: Vec<Json> = last_signals;
    all.extend(signals.iter().map(signal_json));
    let keep = all.len().saturating_sub(16);
    let trace: Vec<Json> = state
        .trace()
        .map(
            |t| json!({ "time": t.time, "hook": t.hook, "verdict": t.verdict, "detail": t.detail }),
        )
        .collect();
    r.snapshots.insert(
        name.to_string(),
        json!({
            "view": view_json(view),
            "zoom": m.zoom,
            "ppp": m.ppp,
            "origin_px": m.origin,
            "rect_px": m.rect_px,
            "image": m.image,
            "in_gesture": state.in_gesture(),
            "hooks": state.hooks().names(),
            "display": state.display_target().map(|t| format!("{t:?}")),
            "transform": format!("{:?}", state.display_transform()),
            "trace": trace,
            "signals": all[keep..].to_vec(),
        }),
    );
}

pub(crate) fn take_commands(ctx: &Context, name: &str) -> Vec<View> {
    let reg = registry(ctx);
    let mut r = lock(&reg);
    let (mine, rest): (Vec<_>, Vec<_>) = r.commands.drain(..).partition(|(n, _)| n == name);
    r.commands = rest;
    mine.into_iter().map(|(_, v)| v).collect()
}

/// The snapshot of every viewport drawn in `ctx`, by name.
pub fn snapshot(ctx: &Context) -> Json {
    let reg = registry(ctx);
    let r = lock(&reg);
    json!({ "viewports": r.snapshots })
}

/// The `viewport` collector for the debug bridge.
pub fn collector() -> FnCollector {
    FnCollector::new(
        "viewport",
        "Image viewports: view, mapping, hooks and what they decided, signals.",
    )
    .snapshot(snapshot)
    .action(
        "set_view",
        "Moves a viewport (through its before_view hook): {name, fit} or {name, zoom, center}.",
        true,
        |ctx, args| {
            let name = args["name"]
                .as_str()
                .ok_or("`name` is required")?
                .to_string();
            let view = if args["fit"].as_bool() == Some(true) {
                View::fit()
            } else {
                let zoom = args["zoom"].as_f64().ok_or("`zoom` or `fit` is required")?;
                let c = &args["center"];
                View::at(
                    zoom,
                    [c[0].as_f64().unwrap_or(0.0), c[1].as_f64().unwrap_or(0.0)],
                )
            };
            let reg = registry(ctx);
            let mut r = lock(&reg);
            if !r.snapshots.contains_key(&name) {
                return Err(format!("no viewport `{name}` drawn"));
            }
            r.commands.push((name.clone(), view));
            ctx.request_repaint();
            Ok(json!({ "queued": name, "view": view_json(&view) }))
        },
    )
    .args(json!({ "name": "main", "zoom": 1.0, "center": [100.0, 80.0] }))
}
