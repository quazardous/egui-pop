//! A plain-text picture of a layout, for snapshot tests and debugging.

use crate::layout::{Layout, Status};
use core::fmt::Write;

const LABELS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

fn label(logical: usize) -> char {
    LABELS.get(logical).map_or('#', |&b| b as char)
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

impl Layout {
    /// Draws the layout as an ASCII grid, then lists every element.
    ///
    /// Each element is drawn with a letter for its logical index (`a` is the
    /// first), empty room with `.`. One character spans the largest number
    /// of twelfths that divides every edge, so a grid of whole rows prints
    /// one line per row. The list gives, in logical order, the form, the
    /// orientation, the slot in twelfths and the status.
    ///
    /// With groups, a gutter left of the picture draws each group's band,
    /// one column per nesting depth (`/` its first line, `|` the middle
    /// ones, `\\` its last, `[` a one-line group), and one line per group
    /// precedes the elements' list: its key, its parent's, its band and
    /// the row its members start at.
    pub fn dump(&self) -> String {
        let rects: Vec<_> = self
            .placements
            .iter()
            .filter_map(|p| p.rect.map(|r| (p.logical, r)))
            .collect();
        let (mut sx, mut sy) = (gcd(12, self.columns), gcd(12, self.rows));
        for (_, r) in &rects {
            sx = gcd(gcd(sx, r.x), r.w);
            sy = gcd(gcd(sy, r.y), r.h);
        }
        let (sx, sy) = (sx.max(1), sy.max(1));
        let (cols, rows) = ((self.columns / sx) as usize, (self.rows / sy) as usize);
        let mut out = String::new();
        let _ = writeln!(
            out,
            "grid {}x{} twelfths, one char = {}x{}",
            self.columns, self.rows, sx, sy
        );
        let mut grid = vec![vec!['.'; cols]; rows];
        for (logical, r) in &rects {
            for line in grid
                .iter_mut()
                .take(((r.y + r.h) / sy) as usize)
                .skip((r.y / sy) as usize)
            {
                for cell in line
                    .iter_mut()
                    .take(((r.x + r.w) / sx) as usize)
                    .skip((r.x / sx) as usize)
                {
                    *cell = label(*logical);
                }
            }
        }
        let depth = |mut g: usize| {
            let mut d = 0;
            while let Some(p) = self.groups[g].parent {
                (d, g) = (d + 1, p);
            }
            d
        };
        let framed: Vec<(usize, usize, u32, u32)> = self
            .groups
            .iter()
            .enumerate()
            .filter_map(|(g, gp)| gp.rect.map(|r| (g, depth(g), r.y / sy, r.bottom() / sy)))
            .collect();
        let gutter = framed.iter().map(|f| f.1 + 1).max().unwrap_or(0);
        for (n, line) in grid.into_iter().enumerate() {
            if gutter > 0 {
                let n = n as u32;
                for d in 0..gutter {
                    let mark = framed
                        .iter()
                        .find(|f| f.1 == d && f.2 <= n && n < f.3)
                        .map_or(' ', |&(_, _, first, end)| {
                            match (n == first, n + 1 == end) {
                                (true, true) => '[',
                                (true, false) => '/',
                                (false, true) => '\\',
                                (false, false) => '|',
                            }
                        });
                    out.push(mark);
                }
                out.push(' ');
            }
            out.extend(line);
            out.push('\n');
        }
        for g in &self.groups {
            let _ = write!(out, "group key={}", g.key);
            if let Some(p) = g.parent {
                let _ = write!(out, " in key={}", self.groups[p].key);
            }
            let _ = match (g.ignored, g.rect) {
                (true, _) => writeln!(out, " ignored"),
                (false, None) => writeln!(out, " empty"),
                (false, Some(r)) => writeln!(
                    out,
                    " at {},{} size {}x{} body at row {}",
                    r.x, r.y, r.w, r.h, g.body_top
                ),
            };
        }
        for p in &self.placements {
            let _ = write!(
                out,
                "{} key={} form={} {}",
                label(p.logical),
                p.key,
                p.form,
                p.orientation
            );
            if let Some(r) = p.rect {
                let _ = write!(out, " at {},{} size {}x{}", r.x, r.y, r.w, r.h);
            }
            let _ = match p.status {
                Status::Placed => writeln!(out),
                Status::Unpinned(reason) => writeln!(out, " unpinned({reason})"),
                Status::Overflow(reason) => writeln!(out, " overflow({reason})"),
            };
        }
        out
    }
}
