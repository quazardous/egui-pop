//! The Tab order of a focus scope: reading order by default, explicit
//! indices and groups when the layout's order is wrong.
//!
//! - **Reading order**: rows from top to bottom, and in a row from left
//!   to right (in a right-to-left language, from right to left). Two
//!   stops share a row when the second's vertical centre lies within the
//!   first's height.
//! - **Groups**: the stops of a group follow each other, in reading
//!   order (or by their indices), at the place of the group's first
//!   stop.
//! - **Indices**: stops with an explicit `index` come first, by index
//!   (as HTML's positive `tabindex`); the others follow in reading
//!   order. A group with indices takes its smallest.
//! - **Skipped**: a stop that cannot take the focus (a disabled field)
//!   has no place in the order; a read-only one does.
//!
//! ```
//! use egui_pop_model::tab::{order, TabEntry};
//!
//! // Two columns drawn column by column: reading order goes row by row.
//! let stops = [
//!     TabEntry::at([0.0, 0.0, 100.0, 20.0]),   // left, first row
//!     TabEntry::at([0.0, 30.0, 100.0, 20.0]),  // left, second row
//!     TabEntry::at([120.0, 0.0, 100.0, 20.0]), // right, first row
//! ];
//! assert_eq!(order(&stops, false), vec![0, 2, 1]);
//! ```

use serde::Serialize;

/// One Tab stop as drawn.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TabEntry {
    /// `[left, top, width, height]` in points.
    pub rect: [f32; 4],
    /// An explicit place in the order.
    pub index: Option<i32>,
    /// A group whose stops follow each other.
    pub group: Option<String>,
    /// Out of the order (disabled).
    pub skip: bool,
}

impl TabEntry {
    /// A stop at `[left, top, width, height]`.
    pub fn at(rect: [f32; 4]) -> Self {
        Self {
            rect,
            index: None,
            group: None,
            skip: false,
        }
    }

    pub fn index(mut self, index: i32) -> Self {
        self.index = Some(index);
        self
    }

    pub fn group(mut self, group: &str) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn skip(mut self, skip: bool) -> Self {
        self.skip = skip;
        self
    }

    fn top(&self) -> f32 {
        self.rect[1]
    }

    fn bottom(&self) -> f32 {
        self.rect[1] + self.rect[3]
    }

    fn centre_y(&self) -> f32 {
        self.rect[1] + self.rect[3] / 2.0
    }

    fn left(&self) -> f32 {
        self.rect[0]
    }

    fn right(&self) -> f32 {
        self.rect[0] + self.rect[2]
    }
}

/// The stops in reading order (indices into `entries`, skipped ones
/// left out). `rtl` reads each row from right to left.
pub fn reading_order(entries: &[TabEntry], rtl: bool) -> Vec<usize> {
    let mut by_top: Vec<usize> = (0..entries.len()).filter(|&i| !entries[i].skip).collect();
    by_top.sort_by(|&a, &b| {
        let (a, b) = (&entries[a], &entries[b]);
        a.top()
            .total_cmp(&b.top())
            .then(a.left().total_cmp(&b.left()))
    });
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut band = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for i in by_top {
        let e = &entries[i];
        let c = e.centre_y();
        let same_row = !rows.is_empty() && (band.0..=band.1).contains(&c);
        if same_row {
            if let Some(row) = rows.last_mut() {
                row.push(i);
            }
        } else {
            rows.push(vec![i]);
            band = (e.top(), e.bottom());
        }
    }
    let mut out = Vec::new();
    for mut row in rows {
        row.sort_by(|&a, &b| {
            let (a, b) = (&entries[a], &entries[b]);
            if rtl {
                b.right().total_cmp(&a.right())
            } else {
                a.left().total_cmp(&b.left())
            }
        });
        out.extend(row);
    }
    out
}

/// The Tab order of a scope's stops (indices into `entries`, skipped
/// ones left out); see [the rules](self).
pub fn order(entries: &[TabEntry], rtl: bool) -> Vec<usize> {
    let reading = reading_order(entries, rtl);
    let rank = |i: usize| reading.iter().position(|&r| r == i).unwrap_or(usize::MAX);
    // Units: a group (its stops together) or a single stop, at the place
    // of its first stop in reading order.
    let mut units: Vec<Vec<usize>> = Vec::new();
    for &i in &reading {
        match &entries[i].group {
            Some(g) => {
                if let Some(u) = units
                    .iter_mut()
                    .find(|u| entries[u[0]].group.as_deref() == Some(g.as_str()))
                {
                    u.push(i);
                } else {
                    units.push(vec![i]);
                }
            }
            None => units.push(vec![i]),
        }
    }
    for u in &mut units {
        u.sort_by_key(|&i| (entries[i].index.unwrap_or(i32::MAX), rank(i)));
    }
    let unit_index = |u: &Vec<usize>| u.iter().filter_map(|&i| entries[i].index).min();
    // Stable: units without an index keep their reading place.
    units.sort_by_key(|u| match unit_index(u) {
        Some(n) => (0, n),
        None => (1, 0),
    });
    units.into_iter().flatten().collect()
}

/// The stop after `current` in `order` (before it when `back`): `None`
/// at either end unless `cycle`, when it wraps round. A `current` not in
/// the order goes to the first stop (the last when `back`).
pub fn next(order: &[usize], current: Option<usize>, back: bool, cycle: bool) -> Option<usize> {
    if order.is_empty() {
        return None;
    }
    let Some(at) = current.and_then(|c| order.iter().position(|&o| o == c)) else {
        return Some(if back {
            order[order.len() - 1]
        } else {
            order[0]
        });
    };
    let n = order.len();
    if back {
        if at == 0 {
            cycle.then(|| order[n - 1])
        } else {
            Some(order[at - 1])
        }
    } else if at + 1 == n {
        cycle.then(|| order[0])
    } else {
        Some(order[at + 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(y: f32, xs: &[f32]) -> Vec<TabEntry> {
        xs.iter()
            .map(|&x| TabEntry::at([x, y, 50.0, 20.0]))
            .collect()
    }

    #[test]
    fn reading_order_goes_by_rows_then_left_to_right() {
        let mut e = row(40.0, &[100.0, 0.0]);
        e.extend(row(0.0, &[60.0, 0.0]));
        // A taller stop slightly lower still shares the first row.
        e.push(TabEntry::at([200.0, 4.0, 50.0, 30.0]));
        assert_eq!(order(&e, false), vec![3, 2, 4, 1, 0]);
        assert_eq!(order(&e, true), vec![4, 2, 3, 0, 1]);
    }

    #[test]
    fn indices_come_first_then_reading_order() {
        let mut e = row(0.0, &[0.0, 60.0, 120.0]);
        e[2] = e[2].clone().index(1);
        assert_eq!(order(&e, false), vec![2, 0, 1]);
    }

    #[test]
    fn a_group_stays_together_at_its_first_stop() {
        // Two columns, the right one a group: its stops follow each
        // other instead of interleaving by rows.
        let mut e = Vec::new();
        for y in [0.0, 30.0] {
            e.push(TabEntry::at([0.0, y, 50.0, 20.0]));
            e.push(TabEntry::at([100.0, y, 50.0, 20.0]).group("right"));
        }
        assert_eq!(order(&e, false), vec![0, 1, 3, 2]);
    }

    #[test]
    fn skipped_stops_have_no_place() {
        let mut e = row(0.0, &[0.0, 60.0, 120.0]);
        e[1].skip = true;
        assert_eq!(order(&e, false), vec![0, 2]);
    }

    #[test]
    fn next_stops_at_the_ends_unless_it_cycles() {
        let o = [3, 1, 2];
        assert_eq!(next(&o, Some(3), false, false), Some(1));
        assert_eq!(next(&o, Some(2), false, false), None);
        assert_eq!(next(&o, Some(2), false, true), Some(3));
        assert_eq!(next(&o, Some(3), true, true), Some(2));
        assert_eq!(next(&o, Some(3), true, false), None);
        assert_eq!(next(&o, None, false, false), Some(3));
        assert_eq!(next(&o, None, true, false), Some(2));
        assert_eq!(next(&[], None, false, true), None);
    }
}
