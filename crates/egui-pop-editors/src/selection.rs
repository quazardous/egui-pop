//! The selection: one range, from where it started (the anchor) to where
//! the cursor is (the head).

use serde::{Deserialize, Serialize};

use crate::buffer::{Pos, Span};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub anchor: Pos,
    pub head: Pos,
}

impl Selection {
    /// A cursor with nothing selected.
    pub const fn caret(at: Pos) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }

    pub const fn new(anchor: Pos, head: Pos) -> Self {
        Self { anchor, head }
    }

    /// The selected text's span.
    pub fn span(&self) -> Span {
        Span::new(self.anchor, self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Moves the head; the anchor follows unless `extend`.
    pub fn moved(&self, to: Pos, extend: bool) -> Self {
        if extend {
            Self::new(self.anchor, to)
        } else {
            Self::caret(to)
        }
    }
}
