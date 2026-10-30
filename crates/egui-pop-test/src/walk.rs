//! A keyboard walk: real key presses through the harness, and what holds
//! the focus after each.
//!
//! Putting the focus on a widget through AccessKit (`.focus()`) shows what
//! the widget does once focused, not that the keyboard gets there. A walk
//! only ever presses keys, so it shows the routes: Tab order, F6, what
//! Escape leaves behind, which stops show nothing or sit out of view.
//!
//! ```
//! use egui_pop_core::theme::ThemeSpec;
//! use egui_pop_test::themed;
//! use egui_pop_test::walk::{self, End, TAB};
//!
//! let mut h = themed(&ThemeSpec::default(), egui::vec2(200.0, 100.0), |ui| {
//!     let _ = ui.button("One");
//!     let _ = ui.button("Two");
//! });
//! h.run();
//! let walk = walk::walk(&mut h, TAB, 10);
//! assert_eq!(walk.names(), ["One", "Two"]);
//! assert_eq!(walk.end, End::Looped { at: 0 });
//! ```

use std::fmt;

use egui::accesskit::Role;
use egui::{Id, Key, Modifiers, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::NodeT as _;

/// A key with its modifiers, as a walk presses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Press {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl Press {
    /// A key with no modifier.
    pub const fn key(key: Key) -> Self {
        Self {
            modifiers: Modifiers::NONE,
            key,
        }
    }

    /// A key with Shift held.
    pub const fn shift(key: Key) -> Self {
        Self {
            modifiers: Modifiers::SHIFT,
            key,
        }
    }
}

pub const TAB: Press = Press::key(Key::Tab);
pub const SHIFT_TAB: Press = Press::shift(Key::Tab);
pub const F6: Press = Press::key(Key::F6);
pub const SHIFT_F6: Press = Press::shift(Key::F6);
pub const ESCAPE: Press = Press::key(Key::Escape);

/// What holds the focus.
#[derive(Clone, Debug, PartialEq)]
pub struct Stop {
    /// egui's id of the widget.
    pub id: Id,
    /// Its AccessKit role; [`Role::Unknown`] when it declares none, or has
    /// no node at all.
    pub role: Role,
    /// Its AccessKit name, empty when it has none.
    pub name: String,
    /// Where it was laid out, in points, in or out of view.
    pub rect: Rect,
    /// Whether any of it is on screen: not scrolled or clipped away, and
    /// inside the window. Another layer drawn over it does not count.
    pub visible: bool,
}

impl fmt::Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} {:?}", self.role, self.name)?;
        if !self.visible {
            write!(f, " (out of view)")?;
        }
        Ok(())
    }
}

/// How a [`walk`] ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// The focus came back to the stop at this index of [`Walk::stops`]
    /// (the last index: the key did not move it).
    Looped { at: usize },
    /// Nothing holds the focus any more.
    Lost,
    /// The limit came first.
    Limit,
}

/// What a [`walk`] saw.
#[derive(Clone, Debug, PartialEq)]
pub struct Walk {
    /// What held the focus before the first press.
    pub from: Option<Stop>,
    /// What held the focus after each press, each stop once.
    pub stops: Vec<Stop>,
    pub end: End,
}

impl Walk {
    /// The stops' names, in order.
    pub fn names(&self) -> Vec<&str> {
        self.stops.iter().map(|s| s.name.as_str()).collect()
    }

    /// The first stop with this name.
    pub fn position(&self, name: &str) -> Option<usize> {
        self.stops.iter().position(|s| s.name == name)
    }
}

impl fmt::Display for Walk {
    /// One line per stop, for reading in a failure.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, s) in self.stops.iter().enumerate() {
            writeln!(f, "{i:3} {s}")?;
        }
        write!(f, "{:?}", self.end)
    }
}

/// What holds the focus now, if anything.
pub fn focused<S>(h: &Harness<'_, S>) -> Option<Stop> {
    let id = h.ctx.memory(|m| m.focused())?;
    let node = h
        .root()
        .children_recursive()
        .find(|n| n.accesskit_node().is_focused());
    let (role, name) = node.map_or((Role::Unknown, String::new()), |n| {
        let a = n.accesskit_node();
        // A control labelled by a text node: the name is that text's value.
        let name = a.label().filter(|l| !l.is_empty()).unwrap_or_else(|| {
            let by = a.labelled_by().filter_map(|l| l.value());
            by.collect::<Vec<_>>().join(" ")
        });
        (a.role(), name)
    });
    let response = h.ctx.read_response(id);
    let screen = h.ctx.content_rect();
    Some(Stop {
        id,
        role,
        name,
        rect: response.as_ref().map_or(Rect::NOTHING, |r| r.rect),
        visible: response.is_some_and(|r| r.interact_rect.intersect(screen).is_positive()),
    })
}

/// Presses a key through the harness, runs until the application settles
/// (or the harness's step limit), and says what holds the focus.
pub fn press<S>(h: &mut Harness<'_, S>, press: Press) -> Option<Stop> {
    h.key_press_modifiers(press.modifiers, press.key);
    let _ = h.run_ok();
    focused(h)
}

/// Presses a key until the stop named `name` holds the focus, at most
/// `limit` times: the keyboard's own way to a widget, where a test would
/// otherwise put the focus on it with `.focus()`. `None` when the key does
/// not get there.
pub fn reach<S>(h: &mut Harness<'_, S>, key: Press, name: &str, limit: usize) -> Option<Stop> {
    (0..limit).find_map(|_| press(h, key).filter(|s| s.name == name))
}

/// Presses a key again and again, at most `limit` times, until the focus
/// comes back to a stop already seen or nothing holds it. The stop held
/// before the first press counts as seen only once the key reaches it:
/// it is listed like any other.
pub fn walk<S>(h: &mut Harness<'_, S>, key: Press, limit: usize) -> Walk {
    let from = focused(h);
    let mut stops: Vec<Stop> = Vec::new();
    for _ in 0..limit {
        let Some(stop) = press(h, key) else {
            return Walk {
                from,
                stops,
                end: End::Lost,
            };
        };
        if let Some(at) = stops.iter().position(|s| s.id == stop.id) {
            return Walk {
                from,
                stops,
                end: End::Looped { at },
            };
        }
        stops.push(stop);
    }
    Walk {
        from,
        stops,
        end: End::Limit,
    }
}
