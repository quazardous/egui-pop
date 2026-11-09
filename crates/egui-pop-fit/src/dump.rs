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
        for line in grid {
            out.extend(line);
            out.push('\n');
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
