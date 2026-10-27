//! Undo histories, as the interface sees them (ADR 0014): the pure parts
//! of the `History` contract.
//!
//! - A **scope** ([`ScopeId`], described by [`ScopeInfo`]) is one
//!   history: a document's edit, a collection's marks, the application's
//!   values, the settings window. Steps never cross scopes.
//! - A **step** says what it is, not how to undo it ([`StepInfo`]): its
//!   id, scope, [`Label`], origin, group, the place of the context stack
//!   it was made in, its time and whether it is still in progress. The
//!   inverse stays with the owner.
//! - A scope's steps are a [`Line`]: what an undo walks back (oldest
//!   first) and what a redo makes again (the next one last), with the
//!   scope's revision. Redo is linear: a new step drops the redo list.
//! - **Merging** ([`merges`]): repeats on one target (held arrows, wheel
//!   notches, a colour picker's frames) carry the same merge key and are
//!   one step while they come less than [`MERGE_WINDOW`] apart, from the
//!   same origin, the focus unchanged.
//! - **Routing** ([`newest`]): Ctrl+Z, outside a text field and a
//!   focused local scope, undoes the newest step among the scopes active
//!   in the window.
//! - A **jump** ([`plan_jump`]) is a run of undos or redos to a step.
//!
//! ```
//! use egui_pop_model::commands::Origin;
//! use egui_pop_model::history::{Label, Line, MergeMark, ScopeId, StepInfo, merges, plan_jump};
//!
//! let a = MergeMark::new("view/zoom", Origin::User, 10.0);
//! assert!(merges(&a, &MergeMark::new("view/zoom", Origin::User, 10.4)));
//! assert!(!merges(&a, &MergeMark::new("view/zoom", Origin::User, 11.5)));
//!
//! let step = |id| StepInfo::new(id, ScopeId::from_static("app"), Label::text("Zoom"), Origin::User);
//! let line = Line { undo: vec![step(1), step(2)], redo: vec![step(3)], rev: 4 };
//! let jump = plan_jump(&line, 3).unwrap();
//! assert_eq!((jump.redo, jump.count), (true, 1));
//! ```

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::commands::Origin;
use crate::context::Entry;
pub use crate::ids::ScopeId;

/// The application's own history: the value store's steps.
pub const APP: ScopeId = ScopeId::from_static("app");

/// The settings window's history: the values written from its form.
pub const SETTINGS: ScopeId = ScopeId::from_static("settings");

/// How many steps a scope keeps by default, the oldest dropped first.
pub const CAP: usize = 500;

/// How far apart, in seconds, two repeats on one target may come and
/// still be one step.
pub const MERGE_WINDOW: f64 = 1.0;

/// What a step is called: a message id and its arguments, rendered in the
/// interface's language when shown, with its English text for an
/// interface that does not know the message ("Exposure +0.30 EV").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Label {
    /// The text in English (or as the owner wrote it).
    pub text: String,
    /// The message id, when the label is translated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The message's arguments.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
}

impl Label {
    /// A label without a message id.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// A label from a message id, its English text and its arguments
    /// (`{ $name }` in the text is filled from them).
    pub fn message(key: &str, english: &str, args: &[(&str, &str)]) -> Self {
        let args: BTreeMap<String, String> = args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut text = english.to_string();
        for (k, v) in &args {
            text = text.replace(&format!("{{ ${k} }}"), v);
        }
        Self {
            text,
            key: Some(key.into()),
            args,
        }
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// One step of a history, as the interface sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepInfo {
    /// Unique within its owner; never 0 (0 is "before the first step").
    pub id: u64,
    pub scope: ScopeId,
    pub label: Label,
    pub origin: Origin,
    /// The group (an import, an agent's request) it was made under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
    /// The place of the context stack it was made in (ADR 0012), root
    /// first; empty at the root or when the owner does not know.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub place: Vec<Entry>,
    /// When it was made, in seconds of the owner's clock.
    #[serde(default)]
    pub at: f64,
    /// A gesture still going: not undoable yet.
    #[serde(default)]
    pub in_progress: bool,
}

impl StepInfo {
    /// A step made now (`at` 0), at the root, in no group.
    pub fn new(id: u64, scope: ScopeId, label: Label, origin: Origin) -> Self {
        Self {
            id,
            scope,
            label,
            origin,
            group: None,
            place: Vec::new(),
            at: 0.0,
            in_progress: false,
        }
    }
}

/// A history, described.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScopeInfo {
    pub id: ScopeId,
    /// Its title, as the history panel shows it.
    pub title: String,
    /// The focus surfaces and panels that show it: while one of them is
    /// drawn, the scope is active in the window. Empty: always active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub focus: Vec<String>,
    /// Whether it survives a restart.
    #[serde(default)]
    pub durable: bool,
    /// How many steps it keeps in memory.
    pub cap: usize,
    /// A component's own history, kept by the interface (the theme
    /// editor's draft), not the owner's.
    #[serde(default)]
    pub local: bool,
}

impl ScopeInfo {
    /// An owner's scope, always active, kept for the session.
    pub fn new(id: ScopeId, title: &str) -> Self {
        Self {
            id,
            title: title.into(),
            focus: Vec::new(),
            durable: false,
            cap: CAP,
            local: false,
        }
    }

    /// Whether it is active, the surfaces and panels `shown` being drawn.
    pub fn active<S: AsRef<str>>(&self, shown: &[S]) -> bool {
        self.focus.is_empty()
            || self
                .focus
                .iter()
                .any(|f| shown.iter().any(|s| s.as_ref() == f))
    }
}

/// A scope's steps.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Line {
    /// What an undo walks back, oldest first.
    pub undo: Vec<StepInfo>,
    /// What a redo makes again, the next one last.
    pub redo: Vec<StepInfo>,
    /// Moves on with every change of either list.
    pub rev: u64,
}

impl Line {
    /// The step an undo would take back.
    pub fn head(&self) -> Option<&StepInfo> {
        self.undo.last()
    }

    /// The step a redo would make again.
    pub fn next(&self) -> Option<&StepInfo> {
        self.redo.last()
    }

    /// The head's id (0: nothing done).
    pub fn head_id(&self) -> u64 {
        self.head().map_or(0, |s| s.id)
    }
}

/// An undo, a redo or a jump asked of a scope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub scope: ScopeId,
    /// Refused unless the scope's revision is still this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_rev: Option<u64>,
    /// Checked, answered, but nothing is changed.
    #[serde(default)]
    pub dry_run: bool,
}

impl Request {
    pub fn new(scope: ScopeId) -> Self {
        Self {
            scope,
            if_rev: None,
            dry_run: false,
        }
    }

    pub fn if_rev(mut self, rev: u64) -> Self {
        self.if_rev = Some(rev);
        self
    }

    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    /// Refused when the scope moved since `if_rev`.
    pub fn check(&self, rev: u64) -> Result<(), Refusal> {
        match self.if_rev {
            Some(r) if r != rev => Err(Refusal::new(
                RefusalKind::Stale,
                format!("the history moved on (revision {rev}, not {r})"),
            )),
            _ => Ok(()),
        }
    }
}

/// Why an undo, a redo or a jump was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalKind {
    /// Nothing to undo, redo, or no such step.
    Nothing,
    /// No such scope.
    Unknown,
    /// `if_rev` no longer holds.
    Stale,
    /// A gesture in progress holds what it would change.
    Held,
    /// A hook of the interface vetoed it.
    Vetoed,
    /// The owner's own reason (another process, a later step on the
    /// same target).
    Owner,
}

/// A refusal, with its reason as the interface shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Refusal {
    pub kind: RefusalKind,
    pub reason: String,
}

impl Refusal {
    pub fn new(kind: RefusalKind, reason: impl Into<String>) -> Self {
        Self {
            kind,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for Refusal {}

/// What a step carries to be merged with the next: its merge key (the
/// target path, or the component's own key), origin and time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MergeMark {
    pub key: String,
    pub origin: Origin,
    pub at: f64,
}

impl MergeMark {
    pub fn new(key: &str, origin: Origin, at: f64) -> Self {
        Self {
            key: key.into(),
            origin,
            at,
        }
    }
}

/// Whether `next` joins the step that ended with `prev`: the same key and
/// origin, less than [`MERGE_WINDOW`] later. The owner also stops merging
/// when the focus leaves, after an undo or a redo, and across groups.
pub fn merges(prev: &MergeMark, next: &MergeMark) -> bool {
    let gap = next.at - prev.at;
    prev.key == next.key && prev.origin == next.origin && (0.0..MERGE_WINDOW).contains(&gap)
}

/// The scope Ctrl+Z undoes among `lines` (the active scopes): the one
/// whose head is newest, by time then by id; a later scope wins a tie.
/// `None`: nothing to undo in any of them.
pub fn newest<'a>(lines: impl IntoIterator<Item = &'a Line>) -> Option<&'a ScopeId> {
    lines
        .into_iter()
        .filter_map(Line::head)
        .filter(|s| !s.in_progress)
        .fold(None::<&StepInfo>, |best, s| match best {
            Some(b) if (b.at, b.id) > (s.at, s.id) => Some(b),
            _ => Some(s),
        })
        .map(|s| &s.scope)
}

/// A jump, as undos or redos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Jump {
    /// Redos (the step is ahead), else undos.
    pub redo: bool,
    /// How many (0: already there).
    pub count: usize,
}

/// How to bring a scope to step `to`: after it, `to` is the head (0: the
/// start, everything undone).
pub fn plan_jump(line: &Line, to: u64) -> Result<Jump, Refusal> {
    if to == 0 {
        return Ok(Jump {
            redo: false,
            count: line.undo.len(),
        });
    }
    if let Some(i) = line.undo.iter().position(|s| s.id == to) {
        return Ok(Jump {
            redo: false,
            count: line.undo.len() - 1 - i,
        });
    }
    if let Some(i) = line.redo.iter().position(|s| s.id == to) {
        return Ok(Jump {
            redo: true,
            count: line.redo.len() - i,
        });
    }
    Err(Refusal::new(
        RefusalKind::Nothing,
        format!("no step {to} in `{}`", scope_of(line)),
    ))
}

fn scope_of(line: &Line) -> String {
    line.undo
        .first()
        .or(line.redo.first())
        .map_or_else(String::new, |s| s.scope.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: u64, scope: &'static str, at: f64) -> StepInfo {
        let mut s = StepInfo::new(
            id,
            ScopeId::from_static(scope),
            Label::text("x"),
            Origin::User,
        );
        s.at = at;
        s
    }

    #[test]
    fn repeats_merge_within_the_window_on_one_key_and_origin() {
        let a = MergeMark::new("k", Origin::User, 5.0);
        assert!(merges(&a, &MergeMark::new("k", Origin::User, 5.99)));
        assert!(!merges(&a, &MergeMark::new("k", Origin::User, 6.0)));
        assert!(!merges(&a, &MergeMark::new("other", Origin::User, 5.1)));
        assert!(!merges(
            &a,
            &MergeMark::new("k", Origin::Agent("a".into()), 5.1)
        ));
        // A clock going back never merges.
        assert!(!merges(&a, &MergeMark::new("k", Origin::User, 4.0)));
    }

    #[test]
    fn the_newest_head_among_active_scopes_wins() {
        let a = Line {
            undo: vec![step(1, "a", 1.0), step(4, "a", 3.0)],
            ..Line::default()
        };
        let b = Line {
            undo: vec![step(2, "b", 2.0)],
            ..Line::default()
        };
        assert_eq!(newest([&a, &b]).unwrap().as_str(), "a");
        let c = Line {
            undo: vec![step(5, "c", 3.0)],
            ..Line::default()
        };
        // Same time: the higher id.
        assert_eq!(newest([&a, &b, &c]).unwrap().as_str(), "c");
        // A gesture in progress is not undoable.
        let mut d = step(9, "d", 9.0);
        d.in_progress = true;
        let d = Line {
            undo: vec![d],
            ..Line::default()
        };
        assert_eq!(newest([&a, &d]).unwrap().as_str(), "a");
        assert_eq!(newest([&Line::default()]), None);
    }

    #[test]
    fn a_jump_is_undos_or_redos_to_the_step() {
        let line = Line {
            undo: vec![step(1, "a", 0.0), step(2, "a", 0.0), step(3, "a", 0.0)],
            redo: vec![step(5, "a", 0.0), step(4, "a", 0.0)],
            rev: 7,
        };
        assert_eq!(
            plan_jump(&line, 3).unwrap(),
            Jump {
                redo: false,
                count: 0
            }
        );
        assert_eq!(
            plan_jump(&line, 1).unwrap(),
            Jump {
                redo: false,
                count: 2
            }
        );
        assert_eq!(
            plan_jump(&line, 0).unwrap(),
            Jump {
                redo: false,
                count: 3
            }
        );
        assert_eq!(
            plan_jump(&line, 4).unwrap(),
            Jump {
                redo: true,
                count: 1
            }
        );
        assert_eq!(
            plan_jump(&line, 5).unwrap(),
            Jump {
                redo: true,
                count: 2
            }
        );
        assert_eq!(plan_jump(&line, 9).unwrap_err().kind, RefusalKind::Nothing);
    }

    #[test]
    fn a_request_holds_while_the_revision_does() {
        let r = Request::new(APP).if_rev(3);
        assert!(r.check(3).is_ok());
        assert_eq!(r.check(4).unwrap_err().kind, RefusalKind::Stale);
        assert!(Request::new(APP).check(9).is_ok());
    }

    #[test]
    fn a_label_fills_its_arguments_and_keeps_its_message() {
        let l = Label::message("exposure", "Exposure { $ev } EV", &[("ev", "+0.30")]);
        assert_eq!(l.text, "Exposure +0.30 EV");
        assert_eq!(l.key.as_deref(), Some("exposure"));
        let json = serde_json::to_value(&l).unwrap();
        assert_eq!(json["args"]["ev"], "+0.30");
        assert_eq!(
            serde_json::to_value(Label::text("x")).unwrap(),
            serde_json::json!({ "text": "x" })
        );
    }

    #[test]
    fn a_scope_is_active_while_one_of_its_targets_is_shown() {
        let mut s = ScopeInfo::new(APP, "Application");
        assert!(s.active::<&str>(&[]));
        s.focus = vec!["viewer".into()];
        assert!(!s.active(&["library"]));
        assert!(s.active(&["library", "viewer"]));
    }
}
