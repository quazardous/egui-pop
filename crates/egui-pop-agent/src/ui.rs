//! The `ui` method: the accessibility tree of the frame just drawn, and
//! input on its nodes (click, type, key, scroll), as raw egui events
//! queued for the next frames.
//!
//! An egui plugin ([`UiInput`]) does both ends: at the end of a pass it
//! keeps the AccessKit tree when someone asked for it, and before a pass
//! it adds the next batch of queued events to egui's raw input. So the
//! input goes through egui as a person's would (hover, press, release),
//! headless in a test harness as in a window on a desktop, and needs no
//! windowing system's help.
//!
//! Params of `ui`:
//!
//! | `action` | Other params | Result |
//! |---|---|---|
//! | `tree` | `label`?, `role`? | `{ "count", "nodes": [..] }`, every node or the matching ones |
//! | `click` | `label`?, `role`? | `{ "node" }`, once the press and release went in |
//! | `type` | `label`?, `role`?, `text` | `{ "node" }`, the node clicked, then the text typed |
//! | `key` | `key` (`Escape`, `Ctrl+K`) | `{ "key" }` |
//! | `scroll` | `label`?, `role`?, `up`? | `{ "node" }`, down unless `up` |
//!
//! A node is found by its label (exactly) and its role (`Button`,
//! `Window`, `TextInput`, any case); an action needs exactly one.
//! `tree` needs the bridge on or read-only; the others change the
//! application, so they need it on (`BridgeMode::On`).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use egui::{
    Context, Event, Id, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2, RawInput, Rect, Ui,
    Vec2, ViewportId,
};
use serde_json::{Value as Json, json};

/// One node of the tree, as `ui.tree` lists it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UiNode {
    pub id: u64,
    pub parent: Option<u64>,
    pub role: String,
    pub label: Option<String>,
    pub value: Option<String>,
    /// In points.
    pub rect: Option<Rect>,
    pub disabled: bool,
}

impl UiNode {
    pub(crate) fn to_json(&self) -> Json {
        let mut n = json!({ "id": self.id, "role": self.role });
        if let Some(p) = self.parent {
            n["parent"] = json!(p);
        }
        if let Some(l) = &self.label {
            n["label"] = json!(l);
        }
        if let Some(v) = &self.value {
            n["value"] = json!(v);
        }
        if let Some(r) = self.rect {
            n["rect"] = json!([r.min.x, r.min.y, r.max.x, r.max.y]);
        }
        if self.disabled {
            n["disabled"] = json!(true);
        }
        n
    }

    fn matches(&self, label: Option<&str>, role: Option<&str>) -> bool {
        label.is_none_or(|l| self.label.as_deref() == Some(l))
            && role.is_none_or(|r| self.role.eq_ignore_ascii_case(r))
    }
}

/// What the UI thread and the egui plugin share, kept in the context's
/// data so a plugin added once serves every agent side.
#[derive(Debug, Default)]
pub(crate) struct Shared {
    /// Keep the tree at the end of the coming passes.
    pub want_tree: bool,
    /// The last tree kept, and the pass it is from.
    pub tree: Option<(u64, Vec<UiNode>)>,
    /// Batches of events, one per frame.
    pub queue: VecDeque<Batch>,
}

/// The events of one frame, and the modifiers held during it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Batch {
    pub events: Vec<Event>,
    pub modifiers: Option<Modifiers>,
}

impl Batch {
    fn of(events: Vec<Event>) -> Self {
        Self {
            events,
            modifiers: None,
        }
    }
}

type SharedRef = Arc<Mutex<Shared>>;

fn shared_id() -> Id {
    Id::unique("egui_pop_agent::ui")
}

/// The shared state of `ctx`, made on first use.
pub(crate) fn shared(ctx: &Context) -> SharedRef {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<SharedRef>(shared_id()).clone())
}

/// Installs the plugin on `ctx` (once per context) and turns AccessKit
/// on: its tree is what a node is found in.
pub(crate) fn install(ctx: &Context) -> SharedRef {
    ctx.add_plugin(UiInput);
    ctx.enable_accesskit();
    shared(ctx)
}

/// The egui plugin: keeps the tree, feeds the queued events.
pub(crate) struct UiInput;

impl egui::Plugin for UiInput {
    fn debug_name(&self) -> &'static str {
        "egui_pop_agent::UiInput"
    }

    fn input_hook(&mut self, ctx: &Context, input: &mut RawInput) {
        if input.viewport_id != ViewportId::ROOT {
            return;
        }
        let s = shared(ctx);
        let Ok(mut s) = s.lock() else {
            return;
        };
        if let Some(b) = s.queue.pop_front() {
            if let Some(m) = b.modifiers {
                input.events.push(Event::ModifiersChanged(m));
            }
            input.events.extend(b.events);
        }
        if !s.queue.is_empty() || s.want_tree {
            ctx.request_repaint();
        }
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if ctx.viewport_id() != ViewportId::ROOT {
            return;
        }
        let s = shared(&ctx);
        let Ok(mut s) = s.lock() else {
            return;
        };
        if !s.want_tree {
            return;
        }
        let pass = ctx.cumulative_pass_nr();
        let nodes = ctx.viewport(|vp| {
            vp.this_pass.accesskit_state.as_ref().map(|a| {
                let mut nodes: Vec<UiNode> = a
                    .nodes
                    .iter()
                    .map(|(id, n)| UiNode {
                        id: id.value(),
                        parent: a.parent_map.get(id).map(|p| p.value()),
                        role: format!("{:?}", n.role()),
                        // egui gives a text label its text as a value:
                        // its name, as a screen reader reads it.
                        label: n
                            .label()
                            .or_else(|| {
                                (n.role() == egui::accesskit::Role::Label)
                                    .then(|| n.value())
                                    .flatten()
                            })
                            .map(String::from),
                        value: n.value().map(String::from),
                        rect: n.bounds().map(|b| {
                            Rect::from_min_max(
                                Pos2::new(b.x0 as f32, b.y0 as f32),
                                Pos2::new(b.x1 as f32, b.y1 as f32),
                            )
                        }),
                        disabled: n.is_disabled(),
                    })
                    .collect();
                nodes.sort_by_key(|n| n.id);
                nodes
            })
        });
        if let Some(nodes) = nodes {
            s.tree = Some((pass, nodes));
        }
    }
}

/// A `ui` request, read from its params.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    Tree,
    Click,
    Type(String),
    Key(Modifiers, Key),
    Scroll { up: bool },
}

/// A `ui` request: what to do, and on which node.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Request {
    pub action: Action,
    pub label: Option<String>,
    pub role: Option<String>,
}

impl Request {
    pub(crate) fn from_params(p: &Json) -> Result<Self, String> {
        let s = |k: &str| p.get(k).and_then(Json::as_str).map(String::from);
        let action = match s("action").as_deref() {
            Some("tree") => Action::Tree,
            Some("click") => Action::Click,
            Some("type") => Action::Type(s("text").ok_or("`type` needs `text`")?),
            Some("key") => {
                let k = s("key").ok_or("`key` needs `key`, as `Ctrl+K`")?;
                let (m, k) = parse_chord(&k)?;
                Action::Key(m, k)
            }
            Some("scroll") => Action::Scroll {
                up: p.get("up").and_then(Json::as_bool).unwrap_or(false),
            },
            Some(other) => {
                return Err(format!(
                    "no ui action `{other}`: tree, click, type, key, scroll"
                ));
            }
            None => return Err("`action` is missing: tree, click, type, key, scroll".into()),
        };
        let r = Self {
            action,
            label: s("label"),
            role: s("role"),
        };
        if r.needs_node() && r.label.is_none() && r.role.is_none() {
            return Err("name the node: `label`, `role` or both".into());
        }
        Ok(r)
    }

    /// Whether it changes the application (anything but `tree`).
    pub(crate) fn writes(&self) -> bool {
        self.action != Action::Tree
    }

    fn needs_node(&self) -> bool {
        matches!(
            self.action,
            Action::Click | Action::Type(_) | Action::Scroll { .. }
        )
    }

    fn named(&self) -> String {
        match (&self.label, &self.role) {
            (Some(l), Some(r)) => format!("{r} \"{l}\""),
            (Some(l), None) => format!("\"{l}\""),
            (None, Some(r)) => r.clone(),
            (None, None) => "any node".into(),
        }
    }

    /// Answers on `tree`: the result of `tree`, or the batches an action
    /// queues and the result to give once they went in.
    pub(crate) fn on_tree(&self, tree: &[UiNode]) -> Result<(Vec<Batch>, Json), String> {
        let hits: Vec<&UiNode> = tree
            .iter()
            .filter(|n| n.matches(self.label.as_deref(), self.role.as_deref()))
            .collect();
        if self.action == Action::Tree {
            let nodes: Vec<Json> = hits.iter().map(|n| n.to_json()).collect();
            return Ok((Vec::new(), json!({ "count": nodes.len(), "nodes": nodes })));
        }
        if let Action::Key(m, k) = &self.action {
            return Ok((key_batches(*m, *k), json!({ "key": format!("{k:?}") })));
        }
        let node = match hits.as_slice() {
            [] => return Err(format!("no {} in the tree", self.named())),
            [one] => *one,
            many => {
                return Err(format!(
                    "{} nodes are {}: name it more closely",
                    many.len(),
                    self.named()
                ));
            }
        };
        let Some(rect) = node.rect else {
            return Err(format!("{} has no bounds", self.named()));
        };
        if node.disabled && !matches!(self.action, Action::Scroll { .. }) {
            return Err(format!("{} is disabled", self.named()));
        }
        let at = rect.center();
        let batches = match &self.action {
            Action::Click => click_batches(at),
            Action::Type(text) => {
                let mut b = click_batches(at);
                b.push(Batch::of(vec![Event::Text(text.clone())]));
                b
            }
            Action::Scroll { up } => {
                let dy = (rect.height() / 2.0).max(40.0);
                vec![
                    Batch::of(vec![Event::PointerMoved(at)]),
                    Batch::of(vec![Event::MouseWheel {
                        unit: MouseWheelUnit::Point,
                        delta: Vec2::new(0.0, if *up { dy } else { -dy }),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                        modifiers: Modifiers::NONE,
                    }]),
                ]
            }
            Action::Tree | Action::Key(..) => Vec::new(),
        };
        Ok((batches, json!({ "node": node.to_json() })))
    }
}

/// Hover, press, release: a frame each, as a person's click.
fn click_batches(at: Pos2) -> Vec<Batch> {
    let button = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    vec![
        Batch::of(vec![Event::PointerMoved(at)]),
        Batch::of(vec![button(true)]),
        Batch::of(vec![button(false)]),
    ]
}

fn key_batches(modifiers: Modifiers, key: Key) -> Vec<Batch> {
    let event = |pressed| Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers,
    };
    vec![
        Batch {
            events: vec![event(true)],
            modifiers: Some(modifiers),
        },
        Batch {
            events: vec![event(false)],
            modifiers: Some(modifiers),
        },
        Batch {
            events: Vec::new(),
            modifiers: Some(Modifiers::NONE),
        },
    ]
}

/// `Ctrl+Shift+K` as modifiers and a key.
pub(crate) fn parse_chord(chord: &str) -> Result<(Modifiers, Key), String> {
    let mut mods = Modifiers::NONE;
    let parts: Vec<&str> = chord.split('+').map(str::trim).collect();
    let (key, held) = parts.split_last().ok_or("an empty key")?;
    for m in held {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods.ctrl = true,
            "shift" => mods.shift = true,
            "alt" | "option" => mods.alt = true,
            "cmd" | "command" => mods.command = true,
            "mac" | "meta" => mods.mac_cmd = true,
            other => return Err(format!("no modifier `{other}` in `{chord}`")),
        }
    }
    if mods.ctrl {
        mods.command = true;
    }
    let key = Key::from_name(key).ok_or_else(|| format!("no key `{key}` in `{chord}`"))?;
    Ok((mods, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u64, role: &str, label: &str) -> UiNode {
        UiNode {
            id,
            parent: None,
            role: role.into(),
            label: Some(label.into()),
            value: None,
            rect: Some(Rect::from_min_max(
                Pos2::new(0.0, 0.0),
                Pos2::new(10.0, 20.0),
            )),
            disabled: false,
        }
    }

    #[test]
    fn requests() {
        let r = Request::from_params(&json!({ "action": "click", "label": "OK" })).unwrap();
        assert_eq!(r.action, Action::Click);
        assert!(r.writes());
        assert!(Request::from_params(&json!({ "action": "click" })).is_err());
        assert!(Request::from_params(&json!({ "action": "type", "label": "x" })).is_err());
        assert!(Request::from_params(&json!({ "action": "fly" })).is_err());
        let t = Request::from_params(&json!({ "action": "tree" })).unwrap();
        assert!(!t.writes());
        let k = Request::from_params(&json!({ "action": "key", "key": "Ctrl+K" })).unwrap();
        assert!(matches!(k.action, Action::Key(m, Key::K) if m.ctrl && m.command));
    }

    #[test]
    fn finds_one_node() {
        let tree = vec![node(1, "Button", "OK"), node(2, "Button", "Cancel")];
        let click = |label: Option<&str>, role: Option<&str>| Request {
            action: Action::Click,
            label: label.map(String::from),
            role: role.map(String::from),
        };
        let (b, r) = click(Some("OK"), None).on_tree(&tree).unwrap();
        assert_eq!(b.len(), 3);
        assert_eq!(r["node"]["id"], 1);
        assert!(
            click(None, Some("button"))
                .on_tree(&tree)
                .unwrap_err()
                .contains("2 nodes")
        );
        assert!(click(Some("Nope"), None).on_tree(&tree).is_err());
        let tree_req = Request {
            action: Action::Tree,
            label: None,
            role: Some("BUTTON".into()),
        };
        assert_eq!(tree_req.on_tree(&tree).unwrap().1["count"], 2);
    }
}
