//! Decimation: the points of a long series a plot hands to egui_plot,
//! reduced to what the pixels can show.
//!
//! The approach follows Rerun's time series view (MIT OR Apache-2.0):
//! only the time range in view is read, with a point past each edge so
//! the lines run off the plot, and the points falling into one pixel
//! column are aggregated (Rerun's `MinMax` aggregation) once there are
//! more of them than pixels. Here a column keeps four of its points, in
//! their order: the first, the lowest, the highest and the last (the M4
//! scheme). Every point kept is a real sample, the column's vertical
//! extent is the full series', and the segment joining two columns is the
//! full series' own (the last point of one to the first of the next), so
//! the line drawn covers the same pixels as the full series at this zoom.
//!
//! The functions here take points sorted by x (the plot checks it); the
//! range in view is found by binary search, so a pass costs the points in
//! view, not the series' length.

use std::ops::Range;

/// The x range in view and the pixel columns across it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Window {
    pub x0: f64,
    pub x1: f64,
    pub columns: usize,
}

impl Window {
    pub fn new(x0: f64, x1: f64, columns: usize) -> Self {
        Self {
            x0: x0.min(x1),
            x1: x0.max(x1),
            columns: columns.max(1),
        }
    }

    /// The column `x` falls into, `None` outside the window (or not a
    /// number).
    pub fn column(&self, x: f64) -> Option<usize> {
        if !(x >= self.x0 && x <= self.x1) {
            return None;
        }
        let span = self.x1 - self.x0;
        if span <= 0.0 || !span.is_finite() {
            return Some(0);
        }
        let c = ((x - self.x0) / span * self.columns as f64) as usize;
        Some(c.min(self.columns - 1))
    }
}

/// The indices of `points` (sorted by x) to draw for `window`: those in
/// it and one on each side, found by binary search.
pub(crate) fn span<const N: usize>(points: &[[f64; N]], window: &Window) -> Range<usize> {
    let a = points
        .partition_point(|p| p[0] < window.x0)
        .saturating_sub(1);
    let b = (points.partition_point(|p| p[0] <= window.x1) + 1).min(points.len());
    a..b.max(a)
}

/// A column's points while it is read: indices of its first, lowest,
/// highest and last point.
#[derive(Clone, Copy)]
struct Bucket {
    column: usize,
    first: usize,
    min: usize,
    max: usize,
    last: usize,
}

impl Bucket {
    fn flush(self, points: &[[f64; 2]], out: &mut Vec<[f64; 2]>) {
        let mut keep = [self.first, self.min, self.max, self.last];
        keep.sort_unstable();
        let mut previous = None;
        for i in keep {
            if previous != Some(i) {
                out.push(points[i]);
                previous = Some(i);
            }
        }
    }
}

/// The points of a line or a stepped line (sorted by x) to draw for
/// `window`: in each column its first, lowest, highest and last point,
/// in order. A point outside the window (the one on each side) and a
/// point whose y is not a number (a gap in the line) are kept as they are.
/// At most four points a column, and two more.
pub(crate) fn line(points: &[[f64; 2]], window: &Window, out: &mut Vec<[f64; 2]>) {
    let mut bucket: Option<Bucket> = None;
    for i in span(points, window) {
        let p = points[i];
        let column = if p[1].is_finite() {
            window.column(p[0])
        } else {
            None
        };
        match (column, &mut bucket) {
            (Some(c), Some(b)) if b.column == c => {
                if p[1] < points[b.min][1] {
                    b.min = i;
                }
                if p[1] > points[b.max][1] {
                    b.max = i;
                }
                b.last = i;
            }
            (Some(c), _) => {
                if let Some(b) = bucket.take() {
                    b.flush(points, out);
                }
                bucket = Some(Bucket {
                    column: c,
                    first: i,
                    min: i,
                    max: i,
                    last: i,
                });
            }
            (None, _) => {
                if let Some(b) = bucket.take() {
                    b.flush(points, out);
                }
                out.push(p);
            }
        }
    }
    if let Some(b) = bucket {
        b.flush(points, out);
    }
}

/// The rows of an error band (`[x, min, max]`, sorted by x) to draw for
/// `window`, over the same columns as its series: in each column its
/// first row and its last, as they are, and between them, halfway, the
/// column's lowest minimum and highest maximum. The band is filled
/// between two lines through its rows, so a column covers the same
/// pixels as the full band, and the fill joining two columns is the full
/// band's own. Rows with a value that is not a number are left out (as
/// the plot leaves them out of a band); `min` and `max` come out in
/// order. At most three rows a column, and two more.
pub(crate) fn band(points: &[[f64; 3]], window: &Window, out: &mut Vec<[f64; 3]>) {
    #[derive(Clone, Copy)]
    struct Column {
        index: usize,
        first: [f64; 3],
        last: [f64; 3],
        lo: f64,
        hi: f64,
        rows: usize,
    }
    let flush = |c: Column, out: &mut Vec<[f64; 3]>| {
        out.push(c.first);
        if c.rows > 2 {
            out.push([(c.first[0] + c.last[0]) / 2.0, c.lo, c.hi]);
        }
        if c.rows > 1 {
            out.push(c.last);
        }
    };
    let mut column: Option<Column> = None;
    for q in &points[span(points, window)] {
        if !q.iter().all(|v| v.is_finite()) {
            continue;
        }
        let row = [q[0], q[1].min(q[2]), q[1].max(q[2])];
        match (window.column(q[0]), &mut column) {
            (Some(i), Some(c)) if c.index == i => {
                c.last = row;
                c.lo = c.lo.min(row[1]);
                c.hi = c.hi.max(row[2]);
                c.rows += 1;
            }
            (Some(i), _) => {
                if let Some(c) = column.take() {
                    flush(c, out);
                }
                column = Some(Column {
                    index: i,
                    first: row,
                    last: row,
                    lo: row[1],
                    hi: row[2],
                    rows: 1,
                });
            }
            (None, _) => {
                if let Some(c) = column.take() {
                    flush(c, out);
                }
                out.push(row);
            }
        }
    }
    if let Some(c) = column {
        flush(c, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic noisy signal: `n` samples over `[0, length]`.
    fn signal(n: usize, length: f64) -> Vec<[f64; 2]> {
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        (0..n)
            .map(|i| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let noise = (seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
                let x = i as f64 * length / (n - 1) as f64;
                [x, (x * 0.7).sin() + 0.3 * noise]
            })
            .collect()
    }

    /// Each column's vertical extent, `[min, max]`, of the polyline
    /// through `points`, clipped to the column (a segment crossing a
    /// column's edge counts with its value at the edge).
    fn extents(points: &[[f64; 2]], w: &Window) -> Vec<Option<[f64; 2]>> {
        let mut out = vec![None::<[f64; 2]>; w.columns];
        let width = (w.x1 - w.x0) / w.columns as f64;
        let mut add = |c: usize, y: f64| {
            let e = out[c].get_or_insert([y, y]);
            e[0] = e[0].min(y);
            e[1] = e[1].max(y);
        };
        let at = |a: [f64; 2], b: [f64; 2], x: f64| {
            if b[0] == a[0] {
                a[1]
            } else {
                a[1] + (b[1] - a[1]) * (x - a[0]) / (b[0] - a[0])
            }
        };
        for s in points.windows(2) {
            let (a, b) = (s[0], s[1]);
            let lo = a[0].max(w.x0);
            let hi = b[0].min(w.x1);
            if lo > hi {
                continue;
            }
            let c0 = w.column(lo).unwrap_or(0);
            let c1 = w.column(hi).unwrap_or(w.columns - 1);
            for c in c0..=c1 {
                let l = (w.x0 + c as f64 * width).max(lo);
                let r = (w.x0 + (c + 1) as f64 * width).min(hi);
                add(c, at(a, b, l));
                add(c, at(a, b, r));
            }
        }
        out
    }

    fn assert_same_extents(full: &[[f64; 2]], decimated: &[[f64; 2]], w: &Window) {
        let (a, b) = (extents(full, w), extents(decimated, w));
        for (c, (a, b)) in a.iter().zip(&b).enumerate() {
            match (a, b) {
                (Some(a), Some(b)) => assert!(
                    (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9,
                    "column {c}: full {a:?}, decimated {b:?}"
                ),
                _ => assert_eq!(a, b, "column {c}"),
            }
        }
    }

    #[test]
    fn a_decimated_line_covers_the_same_pixels_as_the_full_one() {
        let full = signal(200_000, 100.0);
        // The whole series across 640 columns.
        let w = Window::new(0.0, 100.0, 640);
        let mut out = Vec::new();
        line(&full, &w, &mut out);
        assert!(out.len() <= 4 * w.columns + 2, "{} points", out.len());
        assert_same_extents(&full, &out, &w);
        // Zoomed in on an odd span: the lines run off both edges.
        let w = Window::new(31.4159, 33.0, 300);
        let mut out = Vec::new();
        line(&full, &w, &mut out);
        assert!(out.first().is_some_and(|p| p[0] < w.x0));
        assert!(out.last().is_some_and(|p| p[0] > w.x1));
        assert_same_extents(&full, &out, &w);
        // Every point kept is one of the series'.
        assert!(out.iter().all(|p| {
            full.binary_search_by(|q| q[0].total_cmp(&p[0]))
                .is_ok_and(|i| full[i] == *p)
        }));
    }

    #[test]
    fn a_stepped_line_decimated_holds_the_same_values() {
        let full = signal(50_000, 10.0);
        let w = Window::new(0.0, 10.0, 200);
        let mut out = Vec::new();
        line(&full, &w, &mut out);
        assert_same_extents(
            &super::super::step_points(&full),
            &super::super::step_points(&out),
            &w,
        );
    }

    #[test]
    fn gaps_and_sparse_lines_stay_as_they_are() {
        // Fewer points than columns: nothing to merge.
        let sparse = signal(50, 10.0);
        let w = Window::new(0.0, 10.0, 400);
        let mut out = Vec::new();
        line(&sparse, &w, &mut out);
        assert_eq!(out, sparse);
        // A gap (a y that is not a number) is kept, so the line breaks.
        let mut gappy = signal(10_000, 10.0);
        gappy[5_000][1] = f64::NAN;
        let mut out = Vec::new();
        line(&gappy, &w, &mut out);
        assert_eq!(out.iter().filter(|p| p[1].is_nan()).count(), 1);
    }

    #[test]
    fn the_points_drawn_stay_bounded_by_the_width() {
        // A benchmark by count, not by time: two million samples across
        // 800 columns give at most four points a column.
        let full = signal(2_000_000, 20.0);
        for (from, to) in [(0.0, 20.0), (3.0, 15.0), (9.99, 10.01)] {
            let w = Window::new(from, to, 800);
            let mut out = Vec::new();
            line(&full, &w, &mut out);
            assert!(out.len() <= 4 * w.columns + 2, "{} points", out.len());
            let steps = super::super::step_points(&out);
            assert!(steps.len() <= 8 * w.columns + 4, "{} points", steps.len());
            let rows: Vec<[f64; 3]> = full
                .iter()
                .map(|p| [p[0], p[1] - 0.1, p[1] + 0.1])
                .collect();
            let mut b = Vec::new();
            band(&rows, &w, &mut b);
            assert!(b.len() <= 3 * w.columns + 2, "{} rows", b.len());
        }
    }

    #[test]
    fn a_decimated_band_covers_the_same_pixels_as_the_full_one() {
        let full = signal(100_000, 50.0);
        let rows: Vec<[f64; 3]> = full
            .iter()
            .map(|p| [p[0], p[1] + 0.2, p[1] - 0.2 - p[0].cos().abs() * 0.1])
            .collect();
        let w = Window::new(5.0, 45.0, 320);
        let mut out = Vec::new();
        band(&rows, &w, &mut out);
        // Each edge of the band, as a line, has the same extents.
        for edge in [1, 2] {
            let pick = |rows: &[[f64; 3]], k: usize| -> Vec<[f64; 2]> {
                rows.iter().map(|r| [r[0], r[k]]).collect()
            };
            let full_edge: Vec<[f64; 2]> = rows
                .iter()
                .map(|r| {
                    [
                        r[0],
                        if edge == 1 {
                            r[1].min(r[2])
                        } else {
                            r[1].max(r[2])
                        },
                    ]
                })
                .collect();
            let (fe, de) = (extents(&full_edge, &w), extents(&pick(&out, edge), &w));
            for (c, (f, d)) in fe.iter().zip(&de).enumerate() {
                let (f, d) = (f.expect("full"), d.expect("decimated"));
                // The lower edge reaches the column's lowest minimum, the
                // upper its highest maximum.
                if edge == 1 {
                    assert!((f[0] - d[0]).abs() < 1e-9, "column {c}: {f:?} {d:?}");
                } else {
                    assert!((f[1] - d[1]).abs() < 1e-9, "column {c}: {f:?} {d:?}");
                }
            }
        }
    }
}
