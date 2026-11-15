//! The selection's item: a string id an application chooses, the same in
//! every widget showing it, without egui or an application (ADR 0012).
//! The application-wide selection that holds these items is
//! `egui_pop_api::selection`; a widget used alone keys its own selection by
//! them.
//!
//! <div class="warning">Experimental: may change in any release
//! (docs/stability.md).</div>

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value as Json;

/// An item of the selection: a string id the application chooses, the
/// same in every widget showing the item (a path, `kind:id`, a number).
///
/// A string rather than a typed key: widgets with different key types (a
/// tree by path, a table by row id, a canvas by entity) meet on it, it
/// goes through the journal, the history and the agents' commands as it
/// is, and it clones for the cost of a reference count.
///
/// It serialises as its string. The impls are written by hand: deriving
/// them over `Arc<str>` needs serde's `rc` feature, which this crate does
/// not turn on (it would build only beside a crate that does).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SelectionItem(Arc<str>);

impl Serialize for SelectionItem {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SelectionItem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::from)
    }
}

impl SelectionItem {
    pub fn new(id: impl Into<Arc<str>>) -> Self {
        Self(id.into())
    }

    /// The item of a widget's key: a string key as it is, any other its
    /// JSON text (`12`, `[1,2]`).
    pub fn from_key<K: Serialize + ?Sized>(key: &K) -> Self {
        match serde_json::to_value(key) {
            Ok(Json::String(s)) => Self::new(s),
            Ok(v) => Self::new(v.to_string()),
            Err(_) => Self::new(""),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SelectionItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl fmt::Display for SelectionItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for SelectionItem {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for SelectionItem {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_as_its_string() {
        let item = SelectionItem::from("photos/a.jpg");
        let text = serde_json::to_string(&item).unwrap();
        assert_eq!(text, "\"photos/a.jpg\"");
        assert_eq!(serde_json::from_str::<SelectionItem>(&text).unwrap(), item);
    }
}
