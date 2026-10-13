//! Keyboard navigation shared by the views: the keys a focused view
//! reads, where they move the cursor, and type-ahead.
//!
//! A view reads its keys only while it has the focus, and after the
//! key dispatcher: a key bound to a command in the application's keymap
//! runs the command and never reaches the view.

use egui::{Event, Key, Modifiers, Ui};

/// What a key asks a view to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Nav {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Left,
    Right,
    Space,
    Rename,
    Activate,
    All,
    Text(String),
}

/// The navigation keys pressed this pass, with their modifiers.
pub(crate) fn read(ui: &Ui) -> Vec<(Nav, Modifiers)> {
    ui.input(|i| {
        let mut out = Vec::new();
        for e in &i.events {
            match e {
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    let nav = match key {
                        Key::ArrowUp => Nav::Up,
                        Key::ArrowDown => Nav::Down,
                        Key::PageUp => Nav::PageUp,
                        Key::PageDown => Nav::PageDown,
                        Key::Home => Nav::Home,
                        Key::End => Nav::End,
                        Key::ArrowLeft => Nav::Left,
                        Key::ArrowRight => Nav::Right,
                        Key::Space => Nav::Space,
                        Key::F2 => Nav::Rename,
                        Key::Enter => Nav::Activate,
                        Key::A if modifiers.command => Nav::All,
                        _ => continue,
                    };
                    out.push((nav, *modifiers));
                }
                Event::Text(t) if !i.modifiers.command && !t.trim().is_empty() => {
                    out.push((Nav::Text(t.clone()), i.modifiers));
                }
                _ => {}
            }
        }
        out
    })
}

/// Where a movement key puts the cursor, `page` rows to a page.
pub(crate) fn target(nav: &Nav, cursor: Option<usize>, len: usize, page: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let page = page.max(1);
    Some(match (nav, cursor) {
        (Nav::Home, _) => 0,
        (Nav::End, _) => last,
        (Nav::Up | Nav::PageUp, None) => last,
        (Nav::Down | Nav::PageDown, None) => 0,
        (Nav::Up, Some(c)) => c.saturating_sub(1),
        (Nav::Down, Some(c)) => (c + 1).min(last),
        (Nav::PageUp, Some(c)) => c.saturating_sub(page),
        (Nav::PageDown, Some(c)) => (c + page).min(last),
        _ => return None,
    })
}

/// How long typed letters keep adding to the search, in seconds.
pub const TYPE_AHEAD_DELAY: f64 = 1.0;

/// Typed letters searched among the rows' labels.
#[derive(Clone, Debug, Default)]
pub(crate) struct TypeAhead {
    text: String,
    last: f64,
}

impl TypeAhead {
    /// Adds typed text at time `now`; returns the search.
    pub fn push(&mut self, text: &str, now: f64) -> &str {
        if now - self.last > TYPE_AHEAD_DELAY {
            self.text.clear();
        }
        self.last = now;
        self.text.push_str(&text.to_lowercase());
        &self.text
    }
}

/// The first row from the cursor whose label starts with `search`
/// (ignoring case), wrapping. One letter starts after the cursor, so
/// pressing it again cycles through the rows starting with it.
pub(crate) fn find(
    search: &str,
    cursor: Option<usize>,
    len: usize,
    label_at: &dyn Fn(usize) -> String,
) -> Option<usize> {
    if len == 0 || search.is_empty() {
        return None;
    }
    let same = search.chars().all(|c| search.starts_with(c));
    let start = match cursor {
        Some(c) if same => c + 1,
        Some(c) => c,
        None => 0,
    };
    let needle = if same {
        &search[..search.chars().next().map_or(0, char::len_utf8)]
    } else {
        search
    };
    (0..len)
        .map(|i| (start + i) % len)
        .find(|&i| label_at(i).to_lowercase().starts_with(needle))
}

/// The scroll offset that brings row `row` into a view `height` high,
/// rows `row_h` high, now scrolled to `offset`.
pub(crate) fn reveal(row: usize, row_h: f32, offset: f32, height: f32) -> f32 {
    let top = row as f32 * row_h;
    if top < offset {
        top
    } else if top + row_h > offset + height {
        (top + row_h - height).max(0.0)
    } else {
        offset
    }
}

/// The keys of a focused flat view (a list, a table): the selections
/// they propose, and the row to bring into view.
#[allow(clippy::too_many_arguments)]
pub(crate) fn flat<K: crate::select::Key>(
    ui: &Ui,
    selection: &crate::select::Selection<K>,
    mode: crate::select::Mode,
    len: usize,
    key_at: &dyn Fn(usize) -> K,
    label_at: &dyn Fn(usize) -> String,
    page: usize,
    type_ahead: &mut TypeAhead,
) -> (Vec<crate::select::Proposal<K>>, Option<usize>) {
    let keys = read(ui);
    let mut out = Vec::new();
    let mut reveal = None;
    if keys.is_empty() || len == 0 {
        return (out, reveal);
    }
    let cursor = selection
        .cursor()
        .and_then(|c| (0..len).find(|&i| key_at(i) == *c));
    let now = ui.input(|i| i.time);
    for (k, mods) in keys {
        let to = match &k {
            Nav::Up | Nav::Down | Nav::PageUp | Nav::PageDown | Nav::Home | Nav::End => {
                target(&k, cursor, len, page).map(|j| (j, mods))
            }
            Nav::Space if mods.command => {
                out.extend(selection.toggle_cursor(mode));
                None
            }
            Nav::Space => cursor.map(|c| (c, Modifiers::NONE)),
            Nav::All if mode == crate::select::Mode::Multiple => {
                out.push(selection.all(len, key_at));
                None
            }
            Nav::Text(t) => {
                let search = type_ahead.push(t, now).to_string();
                find(&search, cursor, len, label_at).map(|j| (j, Modifiers::NONE))
            }
            _ => None,
        };
        if let Some((j, mods)) = to {
            out.push(selection.move_to(mode, len, key_at, j, mods));
            reveal = Some(j);
        }
    }
    (out, reveal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_keys_stay_in_the_rows() {
        assert_eq!(target(&Nav::Down, Some(9), 10, 5), Some(9));
        assert_eq!(target(&Nav::PageDown, Some(2), 10, 5), Some(7));
        assert_eq!(target(&Nav::PageUp, Some(2), 10, 5), Some(0));
        assert_eq!(target(&Nav::End, None, 10, 5), Some(9));
        assert_eq!(target(&Nav::Up, None, 0, 5), None);
    }

    #[test]
    fn type_ahead_cycles_on_one_letter_and_narrows_on_more() {
        let labels = ["Apple", "Banana", "Blueberry", "Cherry", "Bean"];
        let at = |i: usize| labels[i].to_string();
        assert_eq!(find("b", Some(0), 5, &at), Some(1));
        assert_eq!(find("b", Some(1), 5, &at), Some(2));
        assert_eq!(find("bb", Some(2), 5, &at), Some(4));
        assert_eq!(find("bl", Some(1), 5, &at), Some(2));
        assert_eq!(find("z", Some(1), 5, &at), None);
        let mut t = TypeAhead::default();
        assert_eq!(t.push("B", 0.0), "b");
        assert_eq!(t.push("l", 0.5), "bl");
        assert_eq!(t.push("c", 2.0), "c");
    }

    #[test]
    fn reveal_scrolls_the_least() {
        assert_eq!(reveal(0, 20.0, 100.0, 200.0), 0.0);
        assert_eq!(reveal(10, 20.0, 0.0, 200.0), 20.0);
        assert_eq!(reveal(6, 20.0, 0.0, 200.0), 0.0);
    }
}
