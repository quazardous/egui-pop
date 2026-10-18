//! A field's state, one model for every input (text fields, numbers,
//! paths, sliders): whether it is modified, required, invalid, disabled
//! (and why), read-only or pending.
//!
//! The state is data: a widget draws it (an edge, a marker and an icon
//! per state, never colour alone), gives it to assistive technologies and
//! reports it to agents. Which states a field has comes from the
//! application (validation, what the field depends on); [`FieldState`]
//! only says how they combine.
//!
//! ```
//! use egui_pop_model::field::{Baseline, FieldState, Mark};
//!
//! let state = FieldState::new()
//!     .required()
//!     .modified(Baseline::Default)
//!     .invalid("Give a name");
//! assert!(state.is_editable());
//! assert_eq!(state.edge(), Some(Mark::Invalid));
//! assert_eq!(state.marks(), vec![Mark::Invalid, Mark::Modified, Mark::Required]);
//!
//! let off = FieldState::new().disabled("Needs an open image");
//! assert!(!off.is_focusable());
//! assert_eq!(off.disabled_reason(), Some("Needs an open image"));
//! ```

use serde::Serialize;

/// What a modified field differs from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Baseline {
    /// The value last saved (a document, a settings file).
    Saved,
    /// The value's default.
    Default,
}

impl Baseline {
    pub fn name(self) -> &'static str {
        match self {
            Baseline::Saved => "saved",
            Baseline::Default => "default",
        }
    }
}

/// How a field is modified.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Modified {
    /// What it differs from.
    pub against: Baseline,
    /// The field offers "reset to default" (only when it differs from
    /// its default, and it is editable).
    pub resettable: bool,
}

/// One state a field shows, for its marker and icon. The order of the
/// variants is the order of precedence: the first state a field has
/// gives its edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mark {
    /// Cannot be focused nor changed, for a reason.
    Disabled,
    /// The value is refused, with a message.
    Invalid,
    /// The value is being checked (an asynchronous validation).
    Pending,
    /// Differs from the saved or default value.
    Modified,
    /// Must be given.
    Required,
    /// Can be focused, selected and copied, not changed.
    ReadOnly,
}

impl Mark {
    /// Every mark, in order of precedence.
    pub const ALL: [Mark; 6] = [
        Mark::Disabled,
        Mark::Invalid,
        Mark::Pending,
        Mark::Modified,
        Mark::Required,
        Mark::ReadOnly,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Mark::Disabled => "disabled",
            Mark::Invalid => "invalid",
            Mark::Pending => "pending",
            Mark::Modified => "modified",
            Mark::Required => "required",
            Mark::ReadOnly => "read-only",
        }
    }
}

/// A field's state. The default is a plain field: editable, optional,
/// valid, unchanged.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FieldState {
    /// Differs from its saved or default value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<Modified>,
    /// Must be given.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    /// Refused, and why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid: Option<String>,
    /// Cannot be focused nor changed, and why ("Needs an open image").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<String>,
    /// Can be focused, selected and copied, not changed.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    /// Being checked.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub pending: bool,
}

impl FieldState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Modified against a baseline; resettable when the baseline is the
    /// default.
    pub fn modified(mut self, against: Baseline) -> Self {
        self.modified = Some(Modified {
            against,
            resettable: against == Baseline::Default,
        });
        self
    }

    /// Modified or not, from a comparison: differs from `saved` (when
    /// given) or else from `default`. Resettable when it differs from
    /// the default.
    pub fn compare<T: PartialEq>(mut self, value: &T, saved: Option<&T>, default: &T) -> Self {
        let off_default = value != default;
        self.modified = match saved {
            Some(s) if value != s => Some(Modified {
                against: Baseline::Saved,
                resettable: off_default,
            }),
            Some(_) => None,
            None => off_default.then_some(Modified {
                against: Baseline::Default,
                resettable: true,
            }),
        };
        self
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Refused, with the message to show.
    pub fn invalid(mut self, message: impl Into<String>) -> Self {
        self.invalid = Some(message.into());
        self
    }

    /// Refused when `message` is some (a validation's result).
    pub fn validated(mut self, message: Option<String>) -> Self {
        self.invalid = message;
        self
    }

    /// Disabled, with the reason given to the user and to agents.
    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.disabled = Some(reason.into());
        self
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn pending(mut self) -> Self {
        self.pending = true;
        self
    }

    /// The disabled field's reason.
    pub fn disabled_reason(&self) -> Option<&str> {
        self.disabled.as_deref()
    }

    /// The invalid field's message.
    pub fn message(&self) -> Option<&str> {
        self.invalid.as_deref()
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled.is_some()
    }

    /// It takes the keyboard focus (and has a place in the Tab order):
    /// everything but a disabled field. A read-only field does.
    pub fn is_focusable(&self) -> bool {
        !self.is_disabled()
    }

    /// Its value can be changed: neither disabled nor read-only.
    pub fn is_editable(&self) -> bool {
        !self.is_disabled() && !self.read_only
    }

    /// "Reset to default" is offered.
    pub fn is_resettable(&self) -> bool {
        self.is_editable() && self.modified.is_some_and(|m| m.resettable)
    }

    /// Whether the field has a mark.
    pub fn has(&self, mark: Mark) -> bool {
        match mark {
            Mark::Disabled => self.is_disabled(),
            Mark::Invalid => self.invalid.is_some(),
            Mark::Pending => self.pending,
            Mark::Modified => self.modified.is_some(),
            Mark::Required => self.required,
            Mark::ReadOnly => self.read_only,
        }
    }

    /// The marks the field has, in order of precedence.
    pub fn marks(&self) -> Vec<Mark> {
        Mark::ALL.into_iter().filter(|m| self.has(*m)).collect()
    }

    /// The mark that colours the field's edge: the first in order of
    /// precedence. A required field shows its marker beside the label,
    /// not on the edge, and a read-only one has no edge colour: both
    /// leave the edge to the others.
    pub fn edge(&self) -> Option<Mark> {
        self.marks()
            .into_iter()
            .find(|m| !matches!(m, Mark::Required | Mark::ReadOnly))
    }

    /// Nothing to show.
    pub fn is_plain(&self) -> bool {
        self.marks().is_empty()
    }

    /// The state as agents read it: every mark, the reason and the
    /// message.
    pub fn describe(&self) -> serde_json::Value {
        serde_json::json!({
            "marks": self.marks().iter().map(|m| m.name()).collect::<Vec<_>>(),
            "focusable": self.is_focusable(),
            "editable": self.is_editable(),
            "resettable": self.is_resettable(),
            "modified_against": self.modified.map(|m| m.against.name()),
            "required": self.required,
            "invalid": self.invalid,
            "disabled_reason": self.disabled,
            "read_only": self.read_only,
            "pending": self.pending,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_field_has_no_marks() {
        let s = FieldState::new();
        assert!(s.is_plain() && s.is_editable() && s.is_focusable());
        assert_eq!(s.edge(), None);
        assert!(!s.is_resettable());
    }

    #[test]
    fn disabled_is_skipped_and_read_only_is_not() {
        let d = FieldState::new().disabled("Needs an open image");
        assert!(!d.is_focusable() && !d.is_editable());
        assert_eq!(d.edge(), Some(Mark::Disabled));
        let r = FieldState::new().read_only();
        assert!(r.is_focusable() && !r.is_editable());
        assert_eq!(r.edge(), None);
        assert_eq!(r.marks(), vec![Mark::ReadOnly]);
    }

    #[test]
    fn modified_compares_with_the_saved_value_first() {
        let s = FieldState::new().compare(&3, Some(&2), &1);
        assert_eq!(s.modified.unwrap().against, Baseline::Saved);
        assert!(s.is_resettable(), "off its default");
        let s = FieldState::new().compare(&1, Some(&2), &1);
        assert_eq!(s.modified.unwrap().against, Baseline::Saved);
        assert!(!s.is_resettable(), "already the default");
        assert!(
            FieldState::new()
                .compare(&2, Some(&2), &1)
                .modified
                .is_none()
        );
        let s = FieldState::new().compare(&2, None, &1);
        assert_eq!(s.modified.unwrap().against, Baseline::Default);
        assert!(FieldState::new().compare(&1, None, &1).modified.is_none());
    }

    #[test]
    fn a_read_only_field_offers_no_reset() {
        let s = FieldState::new().modified(Baseline::Default).read_only();
        assert!(!s.is_resettable());
    }

    #[test]
    fn the_edge_follows_precedence() {
        let s = FieldState::new()
            .required()
            .pending()
            .modified(Baseline::Saved);
        assert_eq!(s.edge(), Some(Mark::Pending));
        assert_eq!(FieldState::new().required().edge(), None);
    }

    #[test]
    fn agents_read_the_reason_and_the_message() {
        let s = FieldState::new()
            .disabled("Needs an open image")
            .invalid("Too long");
        let d = s.describe();
        assert_eq!(d["disabled_reason"], "Needs an open image");
        assert_eq!(d["invalid"], "Too long");
        assert_eq!(d["marks"][0], "disabled");
        assert_eq!(d["focusable"], false);
    }
}
