//! The debug bridge: collectors that describe a part of the running
//! application, snapshot it as JSON and act on it. Every call returns a
//! [`Feedback`] block, success or not, saying what happened and whether
//! anything changed; actions that change something are flagged
//! `mutates`. Agents, test tools and debug pages use the same bridge.
//!
//! ```
//! use egui_pop_core::debug::{DebugBridge, FnCollector};
//! use serde_json::json;
//!
//! let ctx = egui::Context::default();
//! let mut bridge = DebugBridge::with_core_collectors();
//! bridge.register(
//!     FnCollector::new("clock", "A clock.")
//!         .snapshot(|_| json!({ "ticks": 3 }))
//!         .action("tick", "Advances the clock.", true, |_, _| Ok(json!({ "ticks": 4 }))),
//! ).unwrap();
//! let f = bridge.call(&ctx, "clock", "tick", &json!(null));
//! assert!(f.ok && f.mutates);
//! let f = bridge.call(&ctx, "clock", "explode", &json!(null));
//! assert!(!f.ok);
//! assert!(f.message.contains("tick"), "it lists the actions: {}", f.message);
//! ```

use std::collections::VecDeque;
use std::fmt;

use egui::Context;
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::binding::SharedStore;
use crate::state::lock;
use crate::{a11y, fault, field, focus, overlay, tab, theme};

/// An action a collector offers.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ActionSpec {
    pub name: String,
    pub description: String,
    /// It changes the application.
    pub mutates: bool,
    /// An example of its arguments (`null` when it takes none).
    pub args: Json,
}

/// What a call did. Always returned, success or not.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Feedback {
    pub ok: bool,
    pub collector: String,
    pub action: String,
    /// The action changes the application (whether or not it did).
    pub mutates: bool,
    /// What happened, in a sentence.
    pub message: String,
    /// The action's result.
    pub data: Json,
}

/// A part of the application, seen by the bridge.
pub trait Collector {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn actions(&self) -> Vec<ActionSpec> {
        Vec::new()
    }
    fn snapshot(&self, ctx: &Context) -> Json;
    fn call(&mut self, _ctx: &Context, action: &str, _args: &Json) -> Result<Json, String> {
        Err(format!("no action `{action}`"))
    }
}

type SnapshotFn = Box<dyn Fn(&Context) -> Json>;
type ActionFn = Box<dyn FnMut(&Context, &Json) -> Result<Json, String>>;

/// A collector made of closures.
pub struct FnCollector {
    name: String,
    description: String,
    snapshot: SnapshotFn,
    actions: Vec<(ActionSpec, ActionFn)>,
}

impl fmt::Debug for FnCollector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FnCollector")
            .field("name", &self.name)
            .finish()
    }
}

impl FnCollector {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            snapshot: Box::new(|_| Json::Null),
            actions: Vec::new(),
        }
    }

    pub fn snapshot(mut self, f: impl Fn(&Context) -> Json + 'static) -> Self {
        self.snapshot = Box::new(f);
        self
    }

    pub fn action(
        mut self,
        name: &str,
        description: &str,
        mutates: bool,
        f: impl FnMut(&Context, &Json) -> Result<Json, String> + 'static,
    ) -> Self {
        self.actions.push((
            ActionSpec {
                name: name.into(),
                description: description.into(),
                mutates,
                args: Json::Null,
            },
            Box::new(f),
        ));
        self
    }

    /// Sets the example arguments of the last action added.
    pub fn args(mut self, example: Json) -> Self {
        if let Some((spec, _)) = self.actions.last_mut() {
            spec.args = example;
        }
        self
    }
}

impl Collector for FnCollector {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn actions(&self) -> Vec<ActionSpec> {
        self.actions.iter().map(|(s, _)| s.clone()).collect()
    }

    fn snapshot(&self, ctx: &Context) -> Json {
        (self.snapshot)(ctx)
    }

    fn call(&mut self, ctx: &Context, action: &str, args: &Json) -> Result<Json, String> {
        match self.actions.iter_mut().find(|(s, _)| s.name == action) {
            Some((_, f)) => f(ctx, args),
            None => Err(format!("no action `{action}`")),
        }
    }
}

/// How many feedback blocks the bridge keeps.
pub const KEPT: usize = 64;

/// The collectors, by name.
#[derive(Default)]
pub struct DebugBridge {
    collectors: Vec<Box<dyn Collector>>,
    log: VecDeque<Feedback>,
}

impl fmt::Debug for DebugBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DebugBridge")
            .field("collectors", &self.names())
            .finish()
    }
}

impl DebugBridge {
    pub fn new() -> Self {
        Self::default()
    }

    /// A bridge with the collectors of this crate: screen, focus,
    /// overlays, fields, faults, announcements and theme.
    pub fn with_core_collectors() -> Self {
        let mut b = Self::new();
        for c in core_collectors() {
            b.register(c).expect("distinct names");
        }
        b
    }

    /// Adds a collector; a second one with the same name is refused.
    pub fn register(&mut self, c: impl Collector + 'static) -> Result<(), String> {
        if self.collectors.iter().any(|x| x.name() == c.name()) {
            return Err(format!("a collector `{}` is already registered", c.name()));
        }
        self.collectors.push(Box::new(c));
        Ok(())
    }

    /// Removes a collector; whether there was one.
    pub fn unregister(&mut self, name: &str) -> bool {
        let before = self.collectors.len();
        self.collectors.retain(|c| c.name() != name);
        self.collectors.len() != before
    }

    pub fn names(&self) -> Vec<String> {
        self.collectors
            .iter()
            .map(|c| c.name().to_string())
            .collect()
    }

    /// Every collector, its description and actions.
    pub fn describe(&self) -> Json {
        Json::Array(
            self.collectors
                .iter()
                .map(|c| {
                    json!({
                        "name": c.name(),
                        "description": c.description(),
                        "actions": c.actions(),
                    })
                })
                .collect(),
        )
    }

    /// Every collector's snapshot, by name.
    pub fn snapshot(&self, ctx: &Context) -> Json {
        let mut map = serde_json::Map::new();
        for c in &self.collectors {
            map.insert(c.name().into(), c.snapshot(ctx));
        }
        Json::Object(map)
    }

    /// One collector's snapshot.
    pub fn snapshot_of(&self, ctx: &Context, name: &str) -> Option<Json> {
        self.collectors
            .iter()
            .find(|c| c.name() == name)
            .map(|c| c.snapshot(ctx))
    }

    /// Calls an action. Unknown collectors and actions get a feedback
    /// listing what exists.
    pub fn call(&mut self, ctx: &Context, collector: &str, action: &str, args: &Json) -> Feedback {
        let names = self.names();
        let feedback = match self.collectors.iter_mut().find(|c| c.name() == collector) {
            None => Feedback {
                ok: false,
                collector: collector.into(),
                action: action.into(),
                mutates: false,
                message: format!(
                    "no collector `{collector}`; there are: {}",
                    names.join(", ")
                ),
                data: Json::Null,
            },
            Some(c) => {
                let actions = c.actions();
                match actions.iter().find(|a| a.name == action) {
                    None => Feedback {
                        ok: false,
                        collector: collector.into(),
                        action: action.into(),
                        mutates: false,
                        message: format!(
                            "`{collector}` has no action `{action}`; it has: {}",
                            actions
                                .iter()
                                .map(|a| a.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        data: Json::Null,
                    },
                    Some(spec) => {
                        let mutates = spec.mutates;
                        match c.call(ctx, action, args) {
                            Ok(data) => Feedback {
                                ok: true,
                                collector: collector.into(),
                                action: action.into(),
                                mutates,
                                message: if mutates {
                                    format!("`{collector}.{action}` done; the application changed")
                                } else {
                                    format!("`{collector}.{action}` done; nothing changed")
                                },
                                data,
                            },
                            Err(e) => Feedback {
                                ok: false,
                                collector: collector.into(),
                                action: action.into(),
                                mutates,
                                message: format!("`{collector}.{action}` failed: {e}"),
                                data: Json::Null,
                            },
                        }
                    }
                }
            }
        };
        if feedback.mutates && feedback.ok {
            ctx.request_repaint();
        }
        if self.log.len() == KEPT {
            self.log.pop_front();
        }
        self.log.push_back(feedback.clone());
        feedback
    }

    /// The last feedback blocks, oldest first.
    pub fn log(&self) -> impl Iterator<Item = &Feedback> {
        self.log.iter()
    }
}

/// The collectors of this crate.
pub fn core_collectors() -> Vec<FnCollector> {
    vec![
        FnCollector::new(
            "screen",
            "Each window's scale (native, interface zoom, pixels per point), its rectangles in points and pixels, maximized and full screen.",
        )
        .snapshot(|ctx| {
            // Read before `input`: the context is locked inside it.
            let zoom = ctx.zoom_factor();
            let windows: Vec<Json> = ctx.input(|i| {
                i.raw
                    .viewports
                    .iter()
                    .map(|(id, v)| {
                        let native = v.native_pixels_per_point.unwrap_or(1.0);
                        let ppp = native * zoom;
                        let rect = |r: Option<egui::Rect>| {
                            r.map(|r| {
                                json!({
                                    "points": [r.min.x, r.min.y, r.width(), r.height()],
                                    "pixels": [r.min.x * ppp, r.min.y * ppp, r.width() * ppp, r.height() * ppp],
                                })
                            })
                        };
                        json!({
                            "viewport": if *id == egui::ViewportId::ROOT { "root".to_string() } else { format!("{id:?}") },
                            "title": v.title,
                            "native": native,
                            "pixels_per_point": ppp,
                            "inner": rect(v.inner_rect),
                            "outer": rect(v.outer_rect),
                            "monitor": v.monitor_size.map(|s| [s.x, s.y]),
                            "maximized": v.maximized,
                            "fullscreen": v.fullscreen,
                        })
                    })
                    .collect()
            });
            json!({ "zoom": zoom, "windows": windows })
        })
        .action(
            "set_interface_zoom",
            "Sets egui's zoom factor (`factor`); an egui-pop application's interface zoom setting takes it back at the next frame, so prefer its `interface.zoom` step.",
            true,
            |ctx, args| {
                let f = args.get("factor").and_then(Json::as_f64).ok_or("`factor` is missing")? as f32;
                if !(0.25..=4.0).contains(&f) {
                    return Err(format!("{f} is outside 0.25 to 4"));
                }
                ctx.set_zoom_factor(f);
                Ok(json!({ "zoom": f }))
            },
        )
        .args(json!({ "factor": 1.25 })),
        FnCollector::new(
            "focus",
            "The focused widget, the focus chain and the scopes drawn.",
        )
        .snapshot(|ctx| {
            json!({
                "focused": focus::focused(ctx).map(|id| format!("{id:?}")),
                "chain": focus::chain_names(ctx),
                "mode": crate::focus_mode::get(ctx).report(),
                "scopes": focus::scopes(ctx),
                "context": focus::when_context(ctx, &Default::default())
                    .iter()
                    .map(|(k, v, d)| json!({ "key": k, "value": v, "depth": d }))
                    .collect::<Vec<_>>(),
            })
        })
        .action("clear", "Takes the keyboard focus away.", true, |ctx, _| {
            let had = focus::focused(ctx).is_some();
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
            Ok(json!({ "had_focus": had }))
        }),
        FnCollector::new(
            "overlays",
            "The overlay stack, bottom first, and the overlays closed lately.",
        )
        .snapshot(|ctx| {
            json!({
                "stack": overlay::stack(ctx),
                "recently_closed": overlay::recently_closed(ctx),
            })
        })
        .action(
            "close_top",
            "Closes the top overlay (as Escape would, even if it stays on Escape).",
            true,
            |ctx, _| {
                let top = overlay::stack(ctx).last().cloned();
                match top {
                    Some(o) => {
                        overlay::close(ctx, o.id);
                        Ok(json!({ "closed": o.label }))
                    }
                    None => Err("no overlay is open".into()),
                }
            },
        ),
        FnCollector::new(
            "fields",
            "Every input drawn in the last pass: its label, kind and state (modified, required, invalid with its message, disabled with its reason, read-only, pending), and the Tab scopes with their order.",
        )
        .snapshot(|ctx| {
            json!({
                "fields": field::describe(ctx),
                "tab_scopes": tab::scopes(ctx),
            })
        }),
        FnCollector::new("faults", "Panels and blocks showing an error card.")
            .snapshot(|ctx| json!(fault::faults(ctx))),
        FnCollector::new(
            "announcements",
            "What was announced to screen readers lately.",
        )
        .snapshot(|ctx| json!(a11y::announcements(ctx)))
        .action(
            "announce",
            "Announces a text politely.",
            true,
            |ctx, args| {
                let text = args
                    .get("text")
                    .and_then(Json::as_str)
                    .ok_or("`text` is missing")?;
                a11y::announce(ctx, text, a11y::Politeness::Polite);
                Ok(Json::Null)
            },
        )
        .args(json!({ "text": "Hello" })),
        FnCollector::new("theme", "The installed theme and its problems.").snapshot(|ctx| {
            let t = theme::Tokens::of(ctx);
            let r = t.resolved();
            json!({
                "spec": {
                    "base": r.spec().base,
                    "density": r.spec().density.name(),
                    "accent": r.spec().accent,
                    "reduced_motion": r.spec().reduced_motion,
                },
                "dark": t.dark_mode(),
                "problems": r.problems().iter().map(|p| p.to_string()).collect::<Vec<_>>(),
            })
        }),
    ]
}

/// A collector over a value store: paths, values, gestures; get, set,
/// undo.
pub fn store_collector(store: SharedStore) -> FnCollector {
    let (s1, s2, s3, s4) = (store.clone(), store.clone(), store.clone(), store);
    FnCollector::new(
        "store",
        "The value store: every path, its value and who changed it.",
    )
    .snapshot(move |_| {
        let s = lock(&s1);
        json!({
            "seq": s.seq(),
            "values": s.list("").iter().map(|(p, e)| json!({
                "path": p,
                "value": e.value.to_json(),
                "default": e.descriptor.default.to_json(),
                "owner": e.owner,
                "origin": e.origin,
                "held_by": s.held_by(p),
            })).collect::<Vec<_>>(),
            "undo": s.undo_steps().iter().map(|st| st.label.clone()).collect::<Vec<_>>(),
            "pending": s.pending(),
        })
    })
    .action(
        "get",
        "Reads a value with its descriptor.",
        false,
        move |_, args| {
            let path = args
                .get("path")
                .and_then(Json::as_str)
                .ok_or("`path` is missing")?;
            let s = lock(&s2);
            let e = s.get(path).ok_or_else(|| format!("no value at `{path}`"))?;
            Ok(json!({ "value": e.value.to_json(), "descriptor": e.descriptor }))
        },
    )
    .args(json!({ "path": "settings/x" }))
    .action(
        "set",
        "Writes a value as an agent (`path`, `value`).",
        true,
        move |_, args| {
            use egui_pop_model::commands::Origin;
            use egui_pop_model::values::{Change, Set, Value};
            let path = args
                .get("path")
                .and_then(Json::as_str)
                .ok_or("`path` is missing")?;
            let v = args.get("value").ok_or("`value` is missing")?;
            let mut s = lock(&s3);
            let ty = s
                .get(path)
                .ok_or_else(|| format!("no value at `{path}`"))?
                .descriptor
                .ty;
            let value = Value::from_json(ty, v)?;
            let a = s
                .set(Set::new(
                    path,
                    Change::To(value),
                    Origin::Agent("debug-bridge".into()),
                ))
                .map_err(|r| r.to_string())?;
            Ok(json!({ "value": a.value.to_json(), "clamped": a.clamped, "changed": a.changed }))
        },
    )
    .args(json!({ "path": "settings/x", "value": 1 }))
    .action(
        "undo",
        "Undoes the last change of values.",
        true,
        move |_, _| {
            let label = lock(&s4)
                .undo(&egui_pop_model::commands::Origin::Agent(
                    "debug-bridge".into(),
                ))
                .map_err(|r| r.to_string())?;
            Ok(json!({ "undone": label }))
        },
    )
}
