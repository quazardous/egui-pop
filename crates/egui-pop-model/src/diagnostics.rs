//! Diagnostics (ADR 0003): what was ignored or changed during
//! registration, with what happens instead. Each one is logged through the
//! `log` facade and kept in a list tests and the debug bridge read.

use std::fmt;

use serde::Serialize;

use crate::ids::OwnerId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Something was adjusted; the result is still what was most likely
    /// meant.
    Warning,
    /// Something was refused or not shown.
    Error,
}

/// One diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub level: Level,
    /// A stable code tests match on: `duplicate-id`, `unknown-zone`...
    pub code: &'static str,
    /// Who registered the thing concerned.
    pub owner: Option<OwnerId>,
    /// What it concerns: an id, a path.
    pub subject: String,
    /// What happened and what happens instead.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(o) = &self.owner {
            write!(f, "[{o}] ")?;
        }
        write!(f, "{} ({}): {}", self.subject, self.code, self.message)
    }
}

/// The diagnostics collected so far.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records and logs a diagnostic.
    pub fn push(&mut self, d: Diagnostic) {
        match d.level {
            Level::Warning => log::warn!(target: "egui_pop", "{d}"),
            Level::Error => log::error!(target: "egui_pop", "{d}"),
        }
        self.items.push(d);
    }

    /// Records a warning.
    pub fn warn(
        &mut self,
        code: &'static str,
        owner: Option<&OwnerId>,
        subject: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.push(Diagnostic {
            level: Level::Warning,
            code,
            owner: owner.cloned(),
            subject: subject.into(),
            message: message.into(),
        });
    }

    /// Records an error.
    pub fn error(
        &mut self,
        code: &'static str,
        owner: Option<&OwnerId>,
        subject: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.push(Diagnostic {
            level: Level::Error,
            code,
            owner: owner.cloned(),
            subject: subject.into(),
            message: message.into(),
        });
    }

    pub fn all(&self) -> &[Diagnostic] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// The codes, in order: what tests usually compare.
    pub fn codes(&self) -> Vec<&'static str> {
        self.items.iter().map(|d| d.code).collect()
    }

    /// Takes every diagnostic out.
    pub fn take(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.items)
    }

    /// Forgets the diagnostics of one owner (it was removed).
    pub fn remove_owner(&mut self, owner: &OwnerId) {
        self.items.retain(|d| d.owner.as_ref() != Some(owner));
    }

    pub fn extend(&mut self, other: impl IntoIterator<Item = Diagnostic>) {
        for d in other {
            self.push(d);
        }
    }
}
