//! Keys bound to commands, resolved by the framework alone (ADR 0007).
//!
//! Bindings come in layers: the user's over the workspace's over the
//! framework's defaults over the suggestions components make. Among the
//! bindings of the pressed keys whose `when` holds, the winner is the
//! greatest `(depth, layer, specificity, order)`: the binding testing
//! keys set nearest the focus, then the higher layer, then the clause
//! testing more keys, then the last declared.
//!
//! Two removals: a binding to [`Action::Remove`] (`"-id"` in files) takes
//! a command's bindings away from the layers below, and [`Action::Unbind`]
//! (`null` in files) takes away every binding of exactly those keys from
//! the layers below. Keys are logical: what the key produces, so top-row
//! and keypad digits are the same. A shortcut is one or more chords
//! (`Ctrl+K Ctrl+S`).
//!
//! ```
//! use egui_pop_model::ids::{CommandId, OwnerId};
//! use egui_pop_model::keymap::{Action, Keymap, Keys, Layer, Resolution};
//! use egui_pop_model::when::WhenContext;
//!
//! let mut k = Keymap::new();
//! let fw = OwnerId::FRAMEWORK;
//! k.bind(&fw, Layer::Default, "Ctrl+K Ctrl+S", Action::run("keys.editor"), "").unwrap();
//! let cx = WhenContext::new();
//! assert_eq!(k.resolve(&Keys::parse("Ctrl+K").unwrap(), &cx), Resolution::Pending);
//! let (r, trace) = k.resolve_traced(&Keys::parse("Ctrl+K Ctrl+S").unwrap(), &cx);
//! assert_eq!(r, Resolution::Run(CommandId::from_static("keys.editor")));
//! assert_eq!(trace.candidates.len(), 1);
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostics;
use crate::ids::{CommandId, OwnerId};
use crate::registry::OwnedList;
use crate::when::{When, WhenContext};

/// Where a binding comes from; later variants win.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Suggestion,
    Default,
    Workspace,
    User,
}

/// One chord: modifiers and a logical key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    /// Lowercase key name: `a`, `3`, `f1`, `space`, `tab`, `escape`.
    pub key: String,
}

impl Chord {
    /// A chord without modifiers.
    pub fn key(key: &str) -> Chord {
        Chord {
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
            key: normalize_key(&key.to_lowercase()),
        }
    }

    pub fn parse(text: &str) -> Result<Chord, String> {
        let mut c = Chord::key("");
        let parts: Vec<&str> = text.split('+').collect();
        // `Ctrl++` binds the plus key.
        let (mods, key) = if text.ends_with("++") {
            (&parts[..parts.len() - 2], "+")
        } else {
            let (m, k) = parts.split_at(parts.len() - 1);
            (m, k[0])
        };
        for m in mods {
            match m.to_lowercase().as_str() {
                "ctrl" | "control" => c.ctrl = true,
                "alt" | "option" => c.alt = true,
                "shift" => c.shift = true,
                "meta" | "super" | "cmd" => c.meta = true,
                other => return Err(format!("`{other}` is not a modifier in `{text}`")),
            }
        }
        let key = key.to_lowercase();
        if key.is_empty() {
            return Err(format!("no key in `{text}`"));
        }
        c.key = normalize_key(&key);
        Ok(c)
    }
}

/// Keypad digits are digits; `esc` is `escape`.
fn normalize_key(key: &str) -> String {
    let key = key.strip_prefix("numpad").unwrap_or(key);
    match key {
        "esc" => "escape".into(),
        "return" => "enter".into(),
        "del" => "delete".into(),
        other => other.into(),
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.meta, "Meta"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        let mut k = self.key.clone();
        if let Some(first) = k.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        write!(f, "{k}")
    }
}

/// A key sequence: one or more chords.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Keys(pub Vec<Chord>);

impl Keys {
    pub fn parse(text: &str) -> Result<Keys, String> {
        let chords = text
            .split_whitespace()
            .map(Chord::parse)
            .collect::<Result<Vec<_>, _>>()?;
        if chords.is_empty() {
            return Err("no keys".into());
        }
        Ok(Keys(chords))
    }
}

impl fmt::Display for Keys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(Chord::to_string).collect();
        write!(f, "{}", parts.join(" "))
    }
}

/// What a binding does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "action", content = "command", rename_all = "lowercase")]
pub enum Action {
    /// Runs a command.
    Run(CommandId),
    /// Removes a command's bindings from the layers below.
    Remove(CommandId),
    /// Removes every binding of these keys from the layers below.
    Unbind,
}

impl Action {
    /// Runs a command given by a literal.
    pub fn run(id: &'static str) -> Action {
        Action::Run(CommandId::from_static(id))
    }

    /// As in files: `"id"`, `"-id"` or `null`.
    pub fn parse(command: Option<&str>) -> Result<Action, String> {
        match command {
            None => Ok(Action::Unbind),
            Some(c) => match c.strip_prefix('-') {
                Some(rest) => CommandId::new(rest)
                    .map(Action::Remove)
                    .map_err(|e| e.to_string()),
                None => CommandId::new(c)
                    .map(Action::Run)
                    .map_err(|e| e.to_string()),
            },
        }
    }

    /// As in files.
    pub fn to_file(&self) -> Option<String> {
        match self {
            Action::Run(c) => Some(c.to_string()),
            Action::Remove(c) => Some(format!("-{c}")),
            Action::Unbind => None,
        }
    }

    fn command(&self) -> Option<&CommandId> {
        match self {
            Action::Run(c) => Some(c),
            _ => None,
        }
    }
}

/// Where a binding listens.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyScope {
    /// Whatever has the focus (a text input may keep the key, see
    /// [`Typing`]).
    #[default]
    Global,
    /// Only while the surface (a focus scope of that name) holds the
    /// focus, shown with a focus ring or not.
    Surface(String),
}

impl KeyScope {
    pub fn surface(name: &str) -> KeyScope {
        KeyScope::Surface(name.into())
    }

    /// As in files and traces: `global`, `surface:<name>`.
    pub fn parse(text: &str) -> Result<KeyScope, String> {
        match text.trim() {
            "" | "global" => Ok(KeyScope::Global),
            t => match t.strip_prefix("surface:") {
                Some(n) if !n.is_empty() => Ok(KeyScope::Surface(n.into())),
                _ => Err(format!("`{t}` is neither `global` nor `surface:<name>`")),
            },
        }
    }

    pub fn is_global(&self) -> bool {
        matches!(self, KeyScope::Global)
    }
}

impl fmt::Display for KeyScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyScope::Global => write!(f, "global"),
            KeyScope::Surface(n) => write!(f, "surface:{n}"),
        }
    }
}

impl Serialize for KeyScope {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// Whether a binding fires while a text input has the focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Typing {
    /// `always` for shortcuts with Ctrl, Alt or Meta, for function keys
    /// and for bindings whose clause tests `textInputFocus`;
    /// `unless-typing` for the other keys, which type or edit text.
    #[default]
    Auto,
    /// Fires even while typing.
    Always,
    /// Left to the text input while one has the focus.
    UnlessTyping,
}

impl Typing {
    /// `always` or `unless-typing`, for these keys.
    pub fn effective(self, keys: &Keys) -> Typing {
        match self {
            Typing::Auto => {
                let first = keys.0.first();
                let chorded = first.is_some_and(|c| c.ctrl || c.alt || c.meta);
                let function = first.is_some_and(|c| {
                    c.key.len() > 1
                        && c.key.starts_with('f')
                        && c.key[1..].chars().all(|d| d.is_ascii_digit())
                });
                if chorded || function {
                    Typing::Always
                } else {
                    Typing::UnlessTyping
                }
            }
            other => other,
        }
    }
}

/// How a binding listens: its scope, whether it fires while typing, and
/// whether a modal lets it through.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize)]
pub struct BindOptions {
    pub scope: KeyScope,
    pub typing: Typing,
    /// A global binding that still fires while a modal is open (a modal
    /// otherwise blocks every global binding).
    pub through_modal: bool,
}

impl BindOptions {
    /// Global, typing decided by the keys.
    pub fn global() -> BindOptions {
        BindOptions::default()
    }

    /// Only while that surface has the focus.
    pub fn surface(name: &str) -> BindOptions {
        BindOptions {
            scope: KeyScope::surface(name),
            ..BindOptions::default()
        }
    }

    pub fn typing(mut self, typing: Typing) -> BindOptions {
        self.typing = typing;
        self
    }

    pub fn through_modal(mut self) -> BindOptions {
        self.through_modal = true;
        self
    }
}

/// A binding as declared.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Binding {
    pub keys: Keys,
    pub action: Action,
    pub when: String,
    pub layer: Layer,
    /// Who declared it.
    pub owner: OwnerId,
    /// Where it listens, while typing, through modals.
    pub options: BindOptions,
}

#[derive(Clone, Debug)]
struct Rule {
    binding: Binding,
    when: When,
}

/// What a key sequence does in a context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "command", rename_all = "lowercase")]
pub enum Resolution {
    /// Run this command.
    Run(CommandId),
    /// The keys so far start a longer shortcut: wait for the next chord.
    Pending,
    Nothing,
}

/// Why a candidate binding did or did not win.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum Verdict {
    Won,
    /// Another binding won over it.
    Lost {
        to: String,
        because: &'static str,
    },
    /// Its `when` clause does not hold here.
    WhenFalse,
    /// Its command was removed by a higher layer.
    RemovedBy {
        owner: OwnerId,
        layer: Layer,
    },
    /// Its keys were unbound by a higher layer.
    UnboundBy {
        owner: OwnerId,
        layer: Layer,
    },
    /// It is longer: the keys so far are its start.
    Prefix,
    /// Its surface does not hold the focus.
    SurfaceNotFocused {
        surface: String,
    },
    /// A text input has the focus and keeps the key.
    LeftToTyping,
    /// A modal is open and the binding is global.
    BlockedByModal,
}

/// One candidate in a trace.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Candidate {
    pub binding: Binding,
    /// Its depth in this context: its surface's, or its clause's.
    pub depth: u32,
    pub verdict: Verdict,
}

/// How a key sequence was resolved: every binding considered and why.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Trace {
    pub keys: String,
    pub resolution: Resolution,
    /// The winning binding's scope (`global`, `surface:<name>`).
    pub scope: Option<String>,
    pub candidates: Vec<Candidate>,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Run(c) => write!(f, "{c}"),
            Action::Remove(c) => write!(f, "-{c}"),
            Action::Unbind => write!(f, "(unbind)"),
        }
    }
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Resolution::Run(c) => write!(f, "runs {c}"),
            Resolution::Pending => write!(f, "waits for the next chord"),
            Resolution::Nothing => write!(f, "does nothing"),
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Won => write!(f, "won"),
            Verdict::Lost { to, because } => write!(f, "lost to {to} ({because})"),
            Verdict::WhenFalse => write!(f, "its clause is false here"),
            Verdict::RemovedBy { owner, layer } => {
                write!(f, "removed by {owner} ({layer:?} layer)")
            }
            Verdict::UnboundBy { owner, layer } => {
                write!(f, "unbound by {owner} ({layer:?} layer)")
            }
            Verdict::Prefix => write!(f, "longer: these keys start it"),
            Verdict::SurfaceNotFocused { surface } => {
                write!(f, "its surface `{surface}` does not have the focus")
            }
            Verdict::LeftToTyping => write!(f, "left to the text input"),
            Verdict::BlockedByModal => write!(f, "blocked by a modal"),
        }
    }
}

impl fmt::Display for Trace {
    /// `Alt+Down: runs x`, then one line per binding considered.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.keys, self.resolution)?;
        if let Some(s) = &self.scope {
            write!(f, " ({s})")?;
        }
        for c in &self.candidates {
            write!(f, "\n  {} {}", c.binding.keys, c.binding.action)?;
            if !c.binding.when.is_empty() {
                write!(f, " when {}", c.binding.when)?;
            }
            write!(
                f,
                " ({:?}, {}, depth {}, by {}): {}",
                c.binding.layer, c.binding.options.scope, c.depth, c.binding.owner, c.verdict
            )?;
        }
        Ok(())
    }
}

/// A user's (or workspace's) keymap as saved: the `keymap` document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeymapDoc {
    pub bindings: Vec<DocBinding>,
}

/// A binding as saved.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DocBinding {
    pub keys: String,
    /// `"id"`, `"-id"` to remove a command's bindings, `null` to unbind
    /// the keys.
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub when: String,
    /// `global` (when empty) or `surface:<name>`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(default, skip_serializing_if = "is_auto")]
    pub typing: Typing,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub through_modal: bool,
}

fn is_auto(t: &Typing) -> bool {
    *t == Typing::Auto
}

/// The bindings, by layer.
#[derive(Debug, Default)]
pub struct Keymap {
    rules: OwnedList<Rule>,
}

impl Keymap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a global binding; its keys and clause must parse.
    pub fn bind(
        &mut self,
        owner: &OwnerId,
        layer: Layer,
        keys: &str,
        action: Action,
        when: &str,
    ) -> Result<(), String> {
        self.bind_with(owner, layer, keys, action, when, BindOptions::global())
    }

    /// Adds a binding with its scope and typing rule.
    pub fn bind_with(
        &mut self,
        owner: &OwnerId,
        layer: Layer,
        keys: &str,
        action: Action,
        when: &str,
        options: BindOptions,
    ) -> Result<(), String> {
        let parsed = Keys::parse(keys)?;
        let w = When::parse(when).map_err(|e| format!("`{keys}`: {e}"))?;
        self.rules.push(
            owner,
            Rule {
                binding: Binding {
                    keys: parsed,
                    action,
                    when: when.into(),
                    layer,
                    owner: owner.clone(),
                    options,
                },
                when: w,
            },
        );
        Ok(())
    }

    /// The rule that takes a binding away, if one does.
    fn removed_by(&self, b: &Binding) -> Option<Verdict> {
        for r in self.rules.iter().map(|s| &s.item) {
            if r.binding.layer <= b.layer {
                continue;
            }
            match &r.binding.action {
                Action::Remove(c) if b.action.command() == Some(c) => {
                    return Some(Verdict::RemovedBy {
                        owner: r.binding.owner.clone(),
                        layer: r.binding.layer,
                    });
                }
                Action::Unbind if r.binding.keys == b.keys => {
                    return Some(Verdict::UnboundBy {
                        owner: r.binding.owner.clone(),
                        layer: r.binding.layer,
                    });
                }
                _ => {}
            }
        }
        None
    }

    /// The bindings in force, after removals, with their order.
    fn live(&self) -> impl Iterator<Item = (u64, &Rule)> {
        self.rules
            .iter()
            .filter(|s| {
                matches!(s.item.binding.action, Action::Run(_))
                    && self.removed_by(&s.item.binding).is_none()
            })
            .map(|s| (s.order, &s.item))
    }

    /// Its depth here: its surface's when it has one, else its
    /// clause's.
    fn depth(r: &Rule, cx: &WhenContext) -> u32 {
        let surface = match &r.binding.options.scope {
            KeyScope::Surface(n) => cx.depth_of(&format!("{n}Focus")).unwrap_or(0),
            KeyScope::Global => 0,
        };
        surface.max(r.when.depth(cx))
    }

    fn rank(r: &Rule, order: u64, cx: &WhenContext) -> (u32, Layer, usize, u64) {
        (
            Self::depth(r, cx),
            r.binding.layer,
            r.when.specificity(),
            order,
        )
    }

    /// Why a binding cannot fire here, apart from its clause: its surface
    /// does not have the focus, a text input keeps the key, a modal
    /// blocks it.
    fn blocked(r: &Rule, cx: &WhenContext) -> Option<Verdict> {
        let truthy = |k: &str| !matches!(cx.get(k), "" | "false");
        let o = &r.binding.options;
        if let KeyScope::Surface(n) = &o.scope
            && cx.depth_of(&format!("{n}Focus")).is_none()
        {
            return Some(Verdict::SurfaceNotFocused { surface: n.clone() });
        }
        // A clause testing `textInputFocus` decides for itself.
        let about_typing = r.when.keys().contains(&"textInputFocus");
        if o.typing.effective(&r.binding.keys) == Typing::UnlessTyping
            && !(o.typing == Typing::Auto && about_typing)
            && truthy("textInputFocus")
        {
            return Some(Verdict::LeftToTyping);
        }
        if o.scope.is_global() && !o.through_modal && truthy("modalOpen") {
            return Some(Verdict::BlockedByModal);
        }
        None
    }

    /// Whether a binding may fire here.
    fn fires(r: &Rule, cx: &WhenContext) -> bool {
        Self::blocked(r, cx).is_none() && r.when.eval(cx)
    }

    /// The bindings declared at a layer, in order (removals included).
    pub fn bindings(&self, layer: Layer) -> Vec<&Binding> {
        self.rules
            .iter()
            .filter(|s| s.item.binding.layer == layer)
            .map(|s| &s.item.binding)
            .collect()
    }

    /// Every binding, in declaration order.
    pub fn all(&self) -> impl Iterator<Item = &Binding> {
        self.rules.iter().map(|s| &s.item.binding)
    }

    /// Forgets every binding of a layer (before loading another profile).
    pub fn clear(&mut self, layer: Layer) {
        self.rules.retain(|s| s.item.binding.layer != layer);
    }

    /// Forgets an owner's bindings; returns how many.
    pub fn remove_owner(&mut self, owner: &OwnerId) -> usize {
        self.rules.remove_owner(owner)
    }

    /// What pressing `keys` (the chords so far) does here.
    pub fn resolve(&self, keys: &Keys, cx: &WhenContext) -> Resolution {
        let winner = self
            .live()
            .filter(|(_, r)| &r.binding.keys == keys && Self::fires(r, cx))
            .max_by_key(|(o, r)| Self::rank(r, *o, cx));
        if let Some((_, r)) = winner
            && let Action::Run(c) = &r.binding.action
        {
            return Resolution::Run(c.clone());
        }
        let longer = self.live().any(|(_, r)| {
            r.binding.keys.0.len() > keys.0.len()
                && r.binding.keys.0.starts_with(&keys.0)
                && Self::fires(r, cx)
        });
        if longer {
            Resolution::Pending
        } else {
            Resolution::Nothing
        }
    }

    /// Resolves and says why: every binding of these keys or starting
    /// with them, with its verdict.
    pub fn resolve_traced(&self, keys: &Keys, cx: &WhenContext) -> (Resolution, Trace) {
        let resolution = self.resolve(keys, cx);
        let winner = self
            .live()
            .filter(|(_, r)| &r.binding.keys == keys && Self::fires(r, cx))
            .max_by_key(|(o, r)| Self::rank(r, *o, cx))
            .map(|(o, r)| {
                (
                    o,
                    Self::rank(r, o, cx),
                    r.binding.action.clone(),
                    r.binding.options.scope.to_string(),
                )
            });
        let mut candidates = Vec::new();
        for s in self.rules.iter() {
            let r = &s.item;
            if !matches!(r.binding.action, Action::Run(_)) {
                continue;
            }
            let exact = &r.binding.keys == keys;
            let prefix =
                r.binding.keys.0.len() > keys.0.len() && r.binding.keys.0.starts_with(&keys.0);
            if !exact && !prefix {
                continue;
            }
            let verdict = if let Some(v) = self.removed_by(&r.binding) {
                v
            } else if let Some(v) = Self::blocked(r, cx) {
                v
            } else if !r.when.eval(cx) {
                Verdict::WhenFalse
            } else if prefix {
                Verdict::Prefix
            } else {
                match &winner {
                    Some((o, _, _, _)) if *o == s.order => Verdict::Won,
                    Some((_, rank, action, _)) => {
                        let mine = Self::rank(r, s.order, cx);
                        let because = if rank.0 != mine.0 {
                            "nearer the focus"
                        } else if rank.1 != mine.1 {
                            "higher layer"
                        } else if rank.2 != mine.2 {
                            "more specific clause"
                        } else {
                            "declared later"
                        };
                        Verdict::Lost {
                            to: action.to_file().unwrap_or_default(),
                            because,
                        }
                    }
                    None => Verdict::WhenFalse,
                }
            };
            candidates.push(Candidate {
                binding: r.binding.clone(),
                depth: Self::depth(r, cx),
                verdict,
            });
        }
        let trace = Trace {
            keys: keys.to_string(),
            resolution: resolution.clone(),
            scope: winner.map(|w| w.3),
            candidates,
        };
        (resolution, trace)
    }

    /// The keys bound to a command (what menus and tooltips show).
    pub fn keys_for(&self, command: &CommandId) -> Vec<Keys> {
        let mut keys: Vec<Keys> = self
            .live()
            .filter(|(_, r)| r.binding.action.command() == Some(command))
            .map(|(_, r)| r.binding.keys.clone())
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// Bindings sharing keys with another of the same or a higher layer
    /// that wins over them (ignoring focus depth, which only the moment
    /// decides): what a shortcuts editor shows as conflicts.
    pub fn conflicts(&self) -> Vec<(Binding, Binding)> {
        let live: Vec<(u64, &Rule)> = self.live().collect();
        let mut out = Vec::new();
        for (i, (oa, a)) in live.iter().enumerate() {
            for (ob, b) in &live[i + 1..] {
                if a.binding.keys == b.binding.keys
                    && a.binding.options.scope == b.binding.options.scope
                    && a.binding.action != b.binding.action
                {
                    let ka = (a.binding.layer, a.when.specificity(), *oa);
                    let kb = (b.binding.layer, b.when.specificity(), *ob);
                    let (lose, win) = if ka < kb { (a, b) } else { (b, a) };
                    out.push((lose.binding.clone(), win.binding.clone()));
                }
            }
        }
        out
    }

    /// Loads a saved keymap into a layer, owned by `owner`, replacing
    /// that owner's bindings there. Entries that do not parse are skipped
    /// and reported.
    pub fn load(&mut self, owner: &OwnerId, layer: Layer, doc: &KeymapDoc) -> Diagnostics {
        self.rules
            .retain(|s| !(s.owner == *owner && s.item.binding.layer == layer));
        let mut d = Diagnostics::new();
        for b in &doc.bindings {
            let result = Action::parse(b.command.as_deref()).and_then(|a| {
                let options = BindOptions {
                    scope: KeyScope::parse(&b.scope)?,
                    typing: b.typing,
                    through_modal: b.through_modal,
                };
                self.bind_with(owner, layer, &b.keys, a, &b.when, options)
            });
            if let Err(e) = result {
                d.warn(
                    "bad-binding",
                    Some(owner),
                    &b.keys,
                    format!("{e}: this binding is skipped"),
                );
            }
        }
        d
    }

    /// An owner's bindings of a layer, as saved.
    pub fn export(&self, owner: &OwnerId, layer: Layer) -> KeymapDoc {
        KeymapDoc {
            bindings: self
                .rules
                .iter()
                .filter(|s| s.owner == *owner && s.item.binding.layer == layer)
                .map(|s| {
                    let o = &s.item.binding.options;
                    DocBinding {
                        keys: s.item.binding.keys.to_string(),
                        command: s.item.binding.action.to_file(),
                        when: s.item.binding.when.clone(),
                        scope: if o.scope.is_global() {
                            String::new()
                        } else {
                            o.scope.to_string()
                        },
                        typing: o.typing,
                        through_modal: o.through_modal,
                    }
                })
                .collect(),
        }
    }
}
