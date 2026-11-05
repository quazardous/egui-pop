//! The ruler's arithmetic, without egui: which steps its ticks and labels
//! take at a zoom, where they go, and what snapping lands on.

use super::scale::TimeScale;
use super::state::TimeView;

/// The least room between two minor ticks, in points.
pub const MIN_TICK_GAP: f32 = 6.0;
/// The room kept between two labels, in points.
pub const LABEL_GAP: f32 = 10.0;
/// Where a label starts after its tick, in points.
pub const LABEL_INSET: f32 = 3.0;

/// The ruler's steps at a zoom, in seconds: a label at each major tick, a
/// short tick at each minor one (the grid snapping lands on).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ticks {
    pub major: f64,
    pub minor: f64,
}

/// A tick of the ruler.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub time: f64,
    /// Points from the ruler's left edge.
    pub x: f32,
    pub major: bool,
    /// A major tick's label.
    pub label: Option<String>,
}

/// The steps the ruler takes for `view` across `width` points: the finest
/// whose labels, measured by `measure` (their width in points), never
/// overlap; and under it the finest grid that divides it and keeps
/// [`MIN_TICK_GAP`].
pub fn ticks(
    scale: &TimeScale,
    view: &TimeView,
    width: f32,
    measure: &dyn Fn(&str) -> f32,
) -> Ticks {
    let width = width.max(1.0);
    let per_point = view.duration.max(f64::MIN_POSITIVE) / f64::from(width);
    let mut steps = scale.steps();
    // Past the coarsest step, doubling it.
    let mut last = steps.last().copied().unwrap_or(1.0);
    while last / per_point < f64::from(width) {
        last *= 2.0;
        steps.push(last);
    }
    let room = |step: f64| (step / per_point) as f32;
    let widest = |step: f64| {
        // The widest labels sit at the ends (the most digits, a sign).
        [view.start, view.end(), view.start + view.duration / 2.0]
            .iter()
            .map(|t| measure(&scale.label(scale.round_to(*t, step), step)))
            .fold(0.0_f32, f32::max)
    };
    let major = steps
        .iter()
        .copied()
        .find(|s| room(*s) >= MIN_TICK_GAP && room(*s) >= widest(*s) + LABEL_INSET + LABEL_GAP)
        .unwrap_or(last);
    let minor = steps
        .iter()
        .copied()
        .filter(|s| *s <= major && room(*s) >= MIN_TICK_GAP)
        .find(|s| {
            let r = major / s;
            (r - r.round()).abs() < 1e-6
        })
        .unwrap_or(major);
    Ticks { major, minor }
}

/// The ticks in view and their labels.
pub fn marks(scale: &TimeScale, view: &TimeView, width: f32, ticks: Ticks) -> Vec<Mark> {
    let origin = scale.origin();
    let every = (ticks.major / ticks.minor).round().max(1.0) as i64;
    let first = ((view.start - origin) / ticks.minor).floor() as i64;
    let last = ((view.end() - origin) / ticks.minor).ceil() as i64;
    // A tick for every few points at most: never more than the width holds.
    if last - first > i64::from(width as i32) + 2 {
        return Vec::new();
    }
    (first..=last)
        .map(|k| {
            let time = origin + k as f64 * ticks.minor;
            let major = k.rem_euclid(every) == 0;
            Mark {
                time,
                x: view.x_of(time, width),
                major,
                label: major.then(|| scale.label(time, ticks.major)),
            }
        })
        .collect()
}

/// What a time snapped to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snapped {
    /// Nothing: the time as given (on the scale's units).
    Free,
    Grid,
    Marker,
    Playhead,
    /// An end of the range.
    Range,
    /// An edge of a clip.
    Clip,
}

/// Points a time is pulled to, with what each is.
#[derive(Clone, Debug, Default)]
pub struct SnapTargets {
    /// The grid's step (the ruler's minor ticks), if it snaps to the grid.
    pub grid: Option<f64>,
    pub points: Vec<(f64, Snapped)>,
}

/// `time` snapped: to the nearest point within `reach` seconds, else to the
/// grid when there is one, else onto the scale's units.
pub fn snap(scale: &TimeScale, time: f64, targets: &SnapTargets, reach: f64) -> (f64, Snapped) {
    let near = targets
        .points
        .iter()
        .map(|(t, what)| ((t - time).abs(), *t, *what))
        .filter(|(d, _, _)| *d <= reach)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, t, what)) = near {
        return (t, what);
    }
    match targets.grid {
        Some(step) => (scale.round_to(time, step), Snapped::Grid),
        None => (scale.quantize(time), Snapped::Free),
    }
}

/// Where a span `length` long starting near `start` snaps: whichever of its
/// edges is nearer a point within `reach` lands on it; otherwise its start
/// goes onto the grid (or the scale's units). Its new start, and what it
/// snapped to.
pub fn snap_span(
    scale: &TimeScale,
    start: f64,
    length: f64,
    targets: &SnapTargets,
    reach: f64,
) -> (f64, Snapped) {
    let nearest = |t: f64| {
        targets
            .points
            .iter()
            .map(|(p, what)| ((p - t).abs(), *p - t, *what))
            .filter(|(d, _, _)| *d <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0))
    };
    let by_start = nearest(start);
    let by_end = nearest(start + length);
    let pick = match (by_start, by_end) {
        (Some(a), Some(b)) => Some(if b.0 < a.0 { b } else { a }),
        (a, b) => a.or(b),
    };
    if let Some((_, shift, what)) = pick {
        return (start + shift, what);
    }
    match targets.grid {
        Some(step) => (scale.round_to(start, step), Snapped::Grid),
        None => (scale.quantize(start), Snapped::Free),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::scale::{FrameRate, Tempo};

    /// A monospaced font of 7 points a character.
    fn mono(text: &str) -> f32 {
        text.chars().count() as f32 * 7.0
    }

    fn scales() -> Vec<TimeScale> {
        vec![
            TimeScale::Seconds,
            TimeScale::frames(25),
            TimeScale::Frames(FrameRate::NTSC),
            TimeScale::beats(128.0),
            TimeScale::Beats(Tempo::new(90.0, 3, 4).offset(0.3)),
        ]
    }

    #[test]
    fn labels_never_overlap_at_any_zoom() {
        for scale in scales() {
            for width in [180.0_f32, 640.0, 1400.0] {
                let mut duration = 0.05;
                while duration < 200_000.0 {
                    for start in [-duration / 3.0, 0.0, 12.34, 3599.0] {
                        let view = TimeView::new(start, duration);
                        let t = ticks(&scale, &view, width, &mono);
                        let marks = marks(&scale, &view, width, t);
                        let mut end = f32::NEG_INFINITY;
                        let mut labels = 0;
                        for m in marks.iter().filter(|m| m.major) {
                            let label = m.label.as_deref().unwrap();
                            let from = m.x + LABEL_INSET;
                            assert!(
                                from >= end,
                                "{scale:?} {view:?} {width}: `{label}` at {from} over the last label ending at {end}"
                            );
                            end = from + mono(label);
                            if m.x >= 0.0 && m.x <= width {
                                labels += 1;
                            }
                        }
                        // Something to read once there is room for two.
                        if width >= 640.0 {
                            assert!(labels >= 1, "{scale:?} {view:?} {width}: no label");
                        }
                        assert!(t.minor <= t.major && t.minor > 0.0);
                    }
                    duration *= 1.7;
                }
            }
        }
    }

    #[test]
    fn tick_spacing_follows_the_zoom() {
        let s = TimeScale::Seconds;
        let at = |duration: f64| ticks(&s, &TimeView::new(0.0, duration), 600.0, &mono);
        // A minute across 600 points: "0:05" every 5 s (50 points), a tick a second.
        assert_eq!(
            at(60.0),
            Ticks {
                major: 5.0,
                minor: 1.0
            }
        );
        // An hour: "10:00" every 10 minutes ("1:00:00" is too wide for 5).
        assert_eq!(at(3600.0).major, 600.0);
        // Two seconds: tenths labelled.
        assert_eq!(at(2.0).major, 0.2);
        // Frames: never finer than one frame.
        let f = TimeScale::frames(25);
        let t = ticks(&f, &TimeView::new(0.0, 0.2), 600.0, &mono);
        assert!(t.minor >= 0.04 - 1e-12, "{t:?}");
        // Beats at 128 BPM over 8 bars: a label every bar, a tick every
        // half beat.
        let b = TimeScale::beats(128.0);
        let t = ticks(&b, &TimeView::new(0.0, 15.0), 600.0, &mono);
        assert_eq!(
            t,
            Ticks {
                major: 1.875,
                minor: 0.234375
            }
        );
    }

    #[test]
    fn snapping_prefers_near_points_then_the_grid() {
        let s = TimeScale::Seconds;
        let targets = SnapTargets {
            grid: Some(1.0),
            points: vec![(10.3, Snapped::Marker), (10.6, Snapped::Playhead)],
        };
        assert_eq!(snap(&s, 10.4, &targets, 0.2), (10.3, Snapped::Marker));
        assert_eq!(snap(&s, 10.55, &targets, 0.2), (10.6, Snapped::Playhead));
        assert_eq!(snap(&s, 12.4, &targets, 0.2), (12.0, Snapped::Grid));
        let free = SnapTargets::default();
        assert_eq!(snap(&s, 12.4, &free, 0.2), (12.4, Snapped::Free));
        // Frames stay whole even free.
        let f = TimeScale::frames(25);
        assert_eq!(snap(&f, 1.01, &free, 0.2), (1.0, Snapped::Free));
    }

    #[test]
    fn a_span_snaps_by_its_nearer_edge() {
        let s = TimeScale::Seconds;
        let targets = SnapTargets {
            grid: Some(1.0),
            points: vec![(10.0, Snapped::Clip), (20.0, Snapped::Playhead)],
        };
        // Its end is nearer the clip's edge than its start the playhead.
        assert_eq!(snap_span(&s, 6.1, 4.0, &targets, 0.5), (6.0, Snapped::Clip));
        assert_eq!(
            snap_span(&s, 19.8, 4.0, &targets, 0.5),
            (20.0, Snapped::Playhead)
        );
        // Nothing near: its start on the grid.
        assert_eq!(
            snap_span(&s, 13.4, 4.0, &targets, 0.5),
            (13.0, Snapped::Grid)
        );
    }
}
