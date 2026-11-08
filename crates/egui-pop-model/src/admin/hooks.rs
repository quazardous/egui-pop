//! Form hooks: Rust functions registered by name and named by a manifest
//! ([`super::FieldHooks`], [`super::Action::confirm`],
//! [`super::StatusSource::Hook`]). They compute; they never draw.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use serde_json::Value as Json;

use super::Tone;

/// A form's values, by field id.
pub type Values = BTreeMap<String, Json>;

/// What a `validate` hook says of a field (or of the form, key `""`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Problem {
    pub field: String,
    pub tone: Tone,
    pub message: String,
}

/// A choice a `choices` hook offers: its value, label and hint.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub hint: String,
}

/// A confirmation a `confirm` hook asks for.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Confirm {
    pub message: String,
    pub verb: String,
    pub danger: bool,
}

pub type ValidateFn = Box<dyn Fn(&Values) -> Vec<Problem> + Send + Sync>;
pub type ChoicesFn = Box<dyn Fn(&Values) -> Vec<Choice> + Send + Sync>;
pub type PreviewFn = Box<dyn Fn(&Values) -> String + Send + Sync>;
/// Stored to shown (`false`) or shown to stored (`true`).
pub type TransformFn = Box<dyn Fn(&Json, bool) -> Json + Send + Sync>;
pub type ConfirmFn = Box<dyn Fn(&Values) -> Option<Confirm> + Send + Sync>;
pub type StatusFn = Box<dyn Fn() -> (String, Tone) + Send + Sync>;
/// The records of a list or table, each an object.
pub type RecordsFn = Box<dyn Fn() -> Vec<Json> + Send + Sync>;

/// A hook of one of the kinds the note lists.
pub enum Hook {
    Validate(ValidateFn),
    Choices(ChoicesFn),
    Preview(PreviewFn),
    Transform(TransformFn),
    Confirm(ConfirmFn),
    Status(StatusFn),
    Records(RecordsFn),
}

impl Hook {
    pub fn kind(&self) -> &'static str {
        match self {
            Hook::Validate(_) => "validate",
            Hook::Choices(_) => "choices",
            Hook::Preview(_) => "preview",
            Hook::Transform(_) => "transform",
            Hook::Confirm(_) => "confirm",
            Hook::Status(_) => "status",
            Hook::Records(_) => "records",
        }
    }
}

impl fmt::Debug for Hook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hook::{}", self.kind())
    }
}

/// The registered hooks, by name.
#[derive(Debug, Default)]
pub struct Hooks(BTreeMap<String, Hook>);

impl Hooks {
    pub fn new() -> Hooks {
        Hooks::default()
    }

    /// Registers a hook; `false` when the name is taken (the first
    /// stays).
    pub fn register(&mut self, name: &str, hook: Hook) -> bool {
        if self.0.contains_key(name) {
            return false;
        }
        self.0.insert(name.into(), hook);
        true
    }

    pub fn get(&self, name: &str) -> Option<&Hook> {
        self.0.get(name)
    }

    /// The names and kinds, for agents.
    pub fn list(&self) -> Vec<(&str, &'static str)> {
        self.0.iter().map(|(k, h)| (k.as_str(), h.kind())).collect()
    }

    /// Runs a `validate` hook; nothing when it is missing or of another
    /// kind.
    pub fn validate(&self, name: &str, values: &Values) -> Vec<Problem> {
        match self.get(name) {
            Some(Hook::Validate(f)) => f(values),
            _ => Vec::new(),
        }
    }

    pub fn choices(&self, name: &str, values: &Values) -> Option<Vec<Choice>> {
        match self.get(name) {
            Some(Hook::Choices(f)) => Some(f(values)),
            _ => None,
        }
    }

    pub fn preview(&self, name: &str, values: &Values) -> Option<String> {
        match self.get(name) {
            Some(Hook::Preview(f)) => Some(f(values)),
            _ => None,
        }
    }

    pub fn records(&self, name: &str) -> Option<Vec<Json>> {
        match self.get(name) {
            Some(Hook::Records(f)) => Some(f()),
            _ => None,
        }
    }

    /// Runs a `confirm` hook: the confirmation it asks for, if any.
    pub fn confirm(&self, name: &str, values: &Values) -> Option<Confirm> {
        match self.get(name) {
            Some(Hook::Confirm(f)) => f(values),
            _ => None,
        }
    }

    pub fn status(&self, name: &str) -> Option<(String, Tone)> {
        match self.get(name) {
            Some(Hook::Status(f)) => Some(f()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hooks_compute_by_name() {
        let mut h = Hooks::new();
        assert!(h.register(
            "positive",
            Hook::Validate(Box::new(|v| {
                v.iter()
                    .filter(|(_, x)| x.as_f64().is_some_and(|n| n < 0.0))
                    .map(|(k, _)| Problem {
                        field: k.clone(),
                        tone: Tone::Danger,
                        message: "Must be positive.".into(),
                    })
                    .collect()
            }))
        ));
        assert!(!h.register("positive", Hook::Preview(Box::new(|_| String::new()))));
        let mut v = Values::new();
        v.insert("a".into(), json!(-1));
        v.insert("b".into(), json!(2));
        let p = h.validate("positive", &v);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].field, "a");
        assert!(h.validate("missing", &v).is_empty());
        assert_eq!(h.list(), vec![("positive", "validate")]);
    }
}
