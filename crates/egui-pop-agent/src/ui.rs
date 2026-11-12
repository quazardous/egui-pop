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
//! Every action takes `window`: an application window's id (`main` for
//! the main window; ADR 0025), whose tree is read and whose input the
//! events go to. Without it, the main window's (its own viewport, with
//! what egui embeds in it). Where egui embeds the application windows in
//! the main one (the web, kittest), a window is its embedded `Window`
//! node and what is in it, and `main` is the main window without them; in
//! a window of its own, the unnamed root node takes the window's title.
//!
//! A node is found by its label (exactly) and its role (`Button`,
//! `Window`, `TextInput`, any case); an action needs exactly one, or
//! `nth`, which of several in reading order (top to bottom, then left to
//! right), from 0.
//! `tree` needs the bridge on or read-only; the others change the
//! application, so they need it on (`BridgeMode::On`).

use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
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
    /// Keep each window's tree at the end of its coming passes.
    pub want_tree: bool,
    /// Each window's last tree kept, and the pass it is from.
    pub trees: HashMap<ViewportId, (u64, Vec<UiNode>)>,
    /// Each window's batches of events, one per frame of that window.
    pub queues: HashMap<ViewportId, VecDeque<Batch>>,
}

impl Shared {
    /// Whether events are still waiting to go in, in any window.
    pub(crate) fn queued(&self) -> bool {
        self.queues.values().any(|q| !q.is_empty())
    }
}

/// Where a `ui` request reads and acts: a viewport, and in it, where
/// egui embeds application windows, one of them or none of them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Scope {
    pub viewport: ViewportId,
    /// Only the embedded window of this title, and what is in it.
    pub within: Option<String>,
    /// Without the embedded windows of these titles.
    pub without: Vec<String>,
    /// An application window's title, the name of its own viewport's
    /// root node (a `Window` node egui leaves unnamed; the title is the
    /// system's title bar), as its embedded window is named.
    pub title: Option<String>,
}

impl Scope {
    /// A whole viewport's tree.
    pub(crate) fn viewport(viewport: ViewportId) -> Self {
        Self {
            viewport,
            within: None,
            without: Vec::new(),
            title: None,
        }
    }

    /// `tree`, its unnamed root named by [`Scope::title`].
    pub(crate) fn named<'t>(&self, tree: &'t [UiNode]) -> Cow<'t, [UiNode]> {
        let Some(title) = &self.title else {
            return Cow::Borrowed(tree);
        };
        let mut tree = tree.to_vec();
        for n in tree
            .iter_mut()
            .filter(|n| n.parent.is_none() && n.label.as_deref().is_none_or(str::is_empty))
        {
            n.label = Some(title.clone());
        }
        Cow::Owned(tree)
    }

    /// The nodes of `tree` in scope.
    pub(crate) fn nodes<'t>(&self, tree: &'t [UiNode]) -> Result<Vec<&'t UiNode>, String> {
        if self.within.is_none() && self.without.is_empty() {
            return Ok(tree.iter().collect());
        }
        let window = |title: &str| -> Vec<u64> {
            tree.iter()
                .filter(|n| n.role == "Window" && n.label.as_deref() == Some(title))
                .map(|n| n.id)
                .collect()
        };
        let parents: HashMap<u64, u64> = tree
            .iter()
            .filter_map(|n| n.parent.map(|p| (n.id, p)))
            .collect();
        let under = |node: u64, roots: &[u64]| {
            let mut at = parents.get(&node).copied();
            while let Some(p) = at {
                if roots.contains(&p) {
                    return true;
                }
                at = parents.get(&p).copied();
            }
            false
        };
        if let Some(title) = &self.within {
            let roots = window(title);
            if roots.is_empty() {
                return Err(format!("the window \"{title}\" is not in the tree"));
            }
            return Ok(tree
                .iter()
                .filter(|n| roots.contains(&n.id) || under(n.id, &roots))
                .collect());
        }
        let roots: Vec<u64> = self.without.iter().flat_map(|t| window(t)).collect();
        Ok(tree
            .iter()
            .filter(|n| !roots.contains(&n.id) && !under(n.id, &roots))
            .collect())
    }
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
        let viewport = input.viewport_id;
        let s = shared(ctx);
        let Ok(mut s) = s.lock() else {
            return;
        };
        let queue = s.queues.get_mut(&viewport);
        let more = match queue {
            Some(q) => {
                if let Some(b) = q.pop_front() {
                    if let Some(m) = b.modifiers {
                        input.events.push(Event::ModifiersChanged(m));
                    }
                    input.events.extend(b.events);
                }
                !q.is_empty()
            }
            None => false,
        };
        if more || s.want_tree {
            ctx.request_repaint_of(viewport);
        }
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let viewport = ctx.viewport_id();
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
            s.trees.insert(viewport, (pass, nodes));
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
    /// Which of several matching nodes, in reading order, from 0.
    pub nth: Option<usize>,
    /// The application window it is for (the main window without).
    pub window: Option<String>,
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
            nth: p
                .get("nth")
                .and_then(Json::as_u64)
                .and_then(|n| usize::try_from(n).ok()),
            window: s("window"),
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

    /// Answers on `tree`, read within `scope`: the result of `tree`, or
    /// the batches an action queues and the result to give once they
    /// went in.
    pub(crate) fn on_tree(
        &self,
        tree: &[UiNode],
        scope: &Scope,
    ) -> Result<(Vec<Batch>, Json), String> {
        if let Action::Key(m, k) = &self.action {
            return Ok((key_batches(*m, *k), json!({ "key": format!("{k:?}") })));
        }
        let tree = scope.named(tree);
        let hits: Vec<&UiNode> = scope
            .nodes(&tree)?
            .into_iter()
            .filter(|n| n.matches(self.label.as_deref(), self.role.as_deref()))
            .collect();
        if self.action == Action::Tree {
            let nodes: Vec<Json> = hits.iter().map(|n| n.to_json()).collect();
            return Ok((Vec::new(), json!({ "count": nodes.len(), "nodes": nodes })));
        }
        let node = match (hits.as_slice(), self.nth) {
            (_, Some(i)) => {
                let mut sorted = hits.clone();
                sorted.sort_by(|a, b| {
                    let at = |n: &UiNode| n.rect.map_or(Pos2::ZERO, |r| r.min);
                    let (a, b) = (at(a), at(b));
                    a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x))
                });
                *sorted
                    .get(i)
                    .ok_or_else(|| format!("no {} #{i}: {} match", self.named(), sorted.len()))?
            }
            ([], None) => return Err(format!("no {} in the tree", self.named())),
            ([one], None) => *one,
            (many, None) => {
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
            nth: None,
            window: None,
        };
        let root = Scope::viewport(ViewportId::ROOT);
        let (b, r) = click(Some("OK"), None).on_tree(&tree, &root).unwrap();
        assert_eq!(b.len(), 3);
        assert_eq!(r["node"]["id"], 1);
        assert!(
            click(None, Some("button"))
                .on_tree(&tree, &root)
                .unwrap_err()
                .contains("2 nodes")
        );
        assert!(click(Some("Nope"), None).on_tree(&tree, &root).is_err());
        let tree_req = Request {
            action: Action::Tree,
            label: None,
            role: Some("BUTTON".into()),
            nth: None,
            window: None,
        };
        let mut second = click(None, Some("Button"));
        second.nth = Some(1);
        assert!(second.on_tree(&tree, &root).is_ok());
        second.nth = Some(2);
        assert!(
            second
                .on_tree(&tree, &root)
                .unwrap_err()
                .contains("2 match")
        );
        assert_eq!(tree_req.on_tree(&tree, &root).unwrap().1["count"], 2);
    }

    #[test]
    fn a_scope_reads_one_embedded_window_or_the_main_one_without_them() {
        let child = |id, parent, role: &str, label: &str| UiNode {
            parent: Some(parent),
            ..node(id, role, label)
        };
        let tree = vec![
            node(1, "Window", "Main"),
            child(2, 1, "Button", "OK"),
            child(3, 1, "Window", "Second"),
            child(4, 3, "GenericContainer", ""),
            child(5, 4, "Button", "OK"),
        ];
        let ids =
            |s: &Scope| -> Vec<u64> { s.nodes(&tree).unwrap().iter().map(|n| n.id).collect() };
        let second = Scope {
            within: Some("Second".into()),
            ..Scope::viewport(ViewportId::ROOT)
        };
        assert_eq!(ids(&second), [3, 4, 5]);
        let main = Scope {
            without: vec!["Second".into()],
            ..Scope::viewport(ViewportId::ROOT)
        };
        assert_eq!(ids(&main), [1, 2]);
        let click = Request::from_params(&json!({ "action": "click", "label": "OK" })).unwrap();
        assert!(
            click
                .on_tree(&tree, &Scope::viewport(ViewportId::ROOT))
                .is_err()
        );
        assert_eq!(click.on_tree(&tree, &second).unwrap().1["node"]["id"], 5);
        assert_eq!(click.on_tree(&tree, &main).unwrap().1["node"]["id"], 2);
        let gone = Scope {
            within: Some("Third".into()),
            ..Scope::viewport(ViewportId::ROOT)
        };
        assert!(click.on_tree(&tree, &gone).unwrap_err().contains("Third"));
        let w = Request::from_params(&json!({ "action": "tree", "window": "second" })).unwrap();
        assert_eq!(w.window.as_deref(), Some("second"));
        // A window of its own: its unnamed root takes its title.
        let own = vec![
            UiNode {
                label: None,
                ..node(1, "Window", "")
            },
            child(2, 1, "Button", "OK"),
        ];
        let titled = Scope {
            title: Some("Second".into()),
            ..Scope::viewport(ViewportId::ROOT)
        };
        let find =
            Request::from_params(&json!({ "action": "tree", "role": "Window", "label": "Second" }))
                .unwrap();
        assert_eq!(find.on_tree(&own, &titled).unwrap().1["count"], 1);
        assert_eq!(
            find.on_tree(&own, &Scope::viewport(ViewportId::ROOT))
                .unwrap()
                .1["count"],
            0
        );
    }
}
