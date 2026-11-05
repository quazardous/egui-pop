//! [`TimeScale`]: how a timeline counts time (seconds, frames with a
//! timecode, bars and beats), converts it, writes it and spaces its ruler.
//!
//! Time itself is always an `f64` in seconds; a scale only reads it in its
//! own units.

use serde::{Deserialize, Serialize};

/// A frame rate, `num / den` frames per second: `25 / 1`, `30000 / 1001`
/// (29.97).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    /// Cinema, 24 frames per second.
    pub const FILM: FrameRate = FrameRate::whole(24);
    /// 25 frames per second.
    pub const PAL: FrameRate = FrameRate::whole(25);
    /// 29.97 frames per second (its timecode is counted without dropping
    /// frames).
    pub const NTSC: FrameRate = FrameRate::new(30000, 1001);

    /// `num / den` frames per second; neither may be 0 (1 is taken).
    pub const fn new(num: u32, den: u32) -> Self {
        Self {
            num: if num == 0 { 1 } else { num },
            den: if den == 0 { 1 } else { den },
        }
    }

    /// A whole number of frames per second.
    pub const fn whole(fps: u32) -> Self {
        Self::new(fps, 1)
    }

    /// Frames per second.
    pub fn fps(self) -> f64 {
        f64::from(self.num.max(1)) / f64::from(self.den.max(1))
    }

    /// One frame, in seconds.
    pub fn frame(self) -> f64 {
        1.0 / self.fps()
    }

    /// The frames a timecode's second counts: the rate rounded (30 for
    /// 29.97; the timecode does not drop frames).
    pub fn nominal(self) -> u32 {
        (self.fps().round() as u32).max(1)
    }
}

/// A tempo and a time signature, for a timeline counted in bars and beats.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tempo {
    /// Beats per minute.
    pub bpm: f64,
    /// The signature's upper number: beats in a bar.
    pub beats_per_bar: u32,
    /// The signature's lower number: the note a beat is (4, a quarter).
    pub beat_unit: u32,
    /// How finely a beat is divided in the text (`bar.beat.tick`).
    pub ticks_per_beat: u32,
    /// Where bar 1 starts, in seconds (a track's first downbeat).
    pub offset: f64,
}

impl Default for Tempo {
    fn default() -> Self {
        Self::new(120.0, 4, 4)
    }
}

impl Tempo {
    /// `bpm` beats per minute, `beats_per_bar / beat_unit`, 960 ticks to a
    /// beat, bar 1 at 0.
    pub fn new(bpm: f64, beats_per_bar: u32, beat_unit: u32) -> Self {
        Self {
            bpm,
            beats_per_bar,
            beat_unit,
            ticks_per_beat: 960,
            offset: 0.0,
        }
    }

    /// Bar 1 starts at `seconds`.
    pub fn offset(mut self, seconds: f64) -> Self {
        self.offset = seconds;
        self
    }

    /// A beat divided in `ticks` in the text.
    pub fn ticks_per_beat(mut self, ticks: u32) -> Self {
        self.ticks_per_beat = ticks.max(1);
        self
    }

    /// One beat, in seconds.
    pub fn beat(&self) -> f64 {
        60.0 / if self.bpm.is_finite() && self.bpm > 0.0 {
            self.bpm
        } else {
            120.0
        }
    }

    /// One bar, in seconds.
    pub fn bar(&self) -> f64 {
        self.beat() * f64::from(self.beats_per_bar.max(1))
    }

    fn tpb(&self) -> i64 {
        i64::from(self.ticks_per_beat.max(1))
    }

    fn bpb(&self) -> i64 {
        i64::from(self.beats_per_bar.max(1))
    }
}

/// How a timeline counts time: its ruler, its text, its steps and its grid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeScale {
    /// Seconds, written `1:02.500` (hours when there are some).
    #[default]
    Seconds,
    /// Frames at a rate, written as a timecode `HH:MM:SS:FF`; a time on
    /// this scale is always a whole frame.
    Frames(FrameRate),
    /// Bars and beats at a tempo, written `bar.beat.tick` from `1.1.000`.
    Beats(Tempo),
}

/// Seconds' ruler steps up to an hour, then whole hours and days.
const SECOND_STEPS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0,
    120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0, 7200.0, 10800.0, 21600.0, 43200.0, 86400.0,
];

/// Whole seconds and above, for frames past a second.
const WHOLE_SECONDS: &[f64] = &[
    1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0, 7200.0,
    10800.0, 21600.0, 43200.0, 86400.0,
];

impl TimeScale {
    /// Frames at `fps` frames per second.
    pub fn frames(fps: u32) -> Self {
        TimeScale::Frames(FrameRate::whole(fps))
    }

    /// Bars of four beats at `bpm`.
    pub fn beats(bpm: f64) -> Self {
        TimeScale::Beats(Tempo::new(bpm, 4, 4))
    }

    /// Its name, for agents: `seconds`, `frames` or `beats`.
    pub fn kind(&self) -> &'static str {
        match self {
            TimeScale::Seconds => "seconds",
            TimeScale::Frames(_) => "frames",
            TimeScale::Beats(_) => "beats",
        }
    }

    /// One of its units in seconds: a second, a frame, a beat.
    pub fn unit(&self) -> f64 {
        match self {
            TimeScale::Seconds => 1.0,
            TimeScale::Frames(r) => r.frame(),
            TimeScale::Beats(t) => t.beat(),
        }
    }

    /// A time in its units: seconds, frames, beats from bar 1.
    pub fn to_units(&self, seconds: f64) -> f64 {
        match self {
            TimeScale::Seconds => seconds,
            TimeScale::Frames(r) => seconds * r.fps(),
            TimeScale::Beats(t) => (seconds - t.offset) / t.beat(),
        }
    }

    /// A time given in its units, in seconds.
    pub fn from_units(&self, units: f64) -> f64 {
        match self {
            TimeScale::Seconds => units,
            TimeScale::Frames(r) => units / r.fps(),
            TimeScale::Beats(t) => t.offset + units * t.beat(),
        }
    }

    /// Where its grid starts: bar 1 for beats, 0 otherwise.
    pub fn origin(&self) -> f64 {
        match self {
            TimeScale::Beats(t) => t.offset,
            _ => 0.0,
        }
    }

    /// What Left and Right move by: a second, a frame, a beat; with Shift
    /// (`big`), ten seconds, a timecode second, a bar.
    pub fn step(&self, big: bool) -> f64 {
        match (self, big) {
            (TimeScale::Seconds, false) => 1.0,
            (TimeScale::Seconds, true) => 10.0,
            (TimeScale::Frames(r), false) => r.frame(),
            (TimeScale::Frames(r), true) => f64::from(r.nominal()) / r.fps(),
            (TimeScale::Beats(t), false) => t.beat(),
            (TimeScale::Beats(t), true) => t.bar(),
        }
    }

    /// The time it can hold nearest to `seconds`: the nearest whole frame on
    /// frames, the time itself otherwise.
    pub fn quantize(&self, seconds: f64) -> f64 {
        match self {
            TimeScale::Frames(r) => (seconds * r.fps()).round() / r.fps(),
            _ => seconds,
        }
    }

    /// The line of a grid `step` apart nearest to `seconds` (the grid starts
    /// at [`origin`](Self::origin)).
    pub fn round_to(&self, seconds: f64, step: f64) -> f64 {
        if !step.is_finite() || step <= 0.0 {
            return self.quantize(seconds);
        }
        let o = self.origin();
        self.quantize(o + ((seconds - o) / step).round() * step)
    }

    /// The ruler's steps from the finest to the coarsest, in seconds; each
    /// lands on the scale's units (whole frames, divisions of a beat, bars).
    pub fn steps(&self) -> Vec<f64> {
        match self {
            TimeScale::Seconds => SECOND_STEPS.to_vec(),
            TimeScale::Frames(r) => {
                let fps = r.fps();
                let n = r.nominal();
                let mut v: Vec<f64> = [1u32, 2, 5, 10]
                    .iter()
                    .filter(|k| **k < n)
                    .map(|k| f64::from(*k) / fps)
                    .collect();
                // A timecode second is `nominal` frames.
                let second = f64::from(n) / fps;
                v.extend(WHOLE_SECONDS.iter().map(|s| s * second));
                v
            }
            TimeScale::Beats(t) => {
                let b = t.beat();
                let mut v: Vec<f64> = [1.0 / 16.0, 1.0 / 8.0, 1.0 / 4.0, 1.0 / 2.0, 1.0]
                    .iter()
                    .map(|k| k * b)
                    .collect();
                let bar = t.bar();
                if t.beats_per_bar > 1 {
                    v.push(bar);
                }
                v.extend(
                    [2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0, 1024.0]
                        .iter()
                        .map(|k| k * bar),
                );
                v
            }
        }
    }

    /// The full text of a time: `1:02.500`, `00:01:02:12`, `17.2.480`.
    pub fn format(&self, seconds: f64) -> String {
        match self {
            TimeScale::Seconds => clock(seconds, 3),
            TimeScale::Frames(r) => timecode(seconds, *r),
            TimeScale::Beats(t) => bars(seconds, t, Precision::Tick),
        }
    }

    /// A ruler's label for a time on a grid `step` apart: no more digits
    /// than the step needs (`0:05`, `0:05.5`; `17`, `17.2`).
    pub fn label(&self, seconds: f64, step: f64) -> String {
        match self {
            TimeScale::Seconds => clock(seconds, decimals(step)),
            TimeScale::Frames(r) => timecode(seconds, *r),
            TimeScale::Beats(t) => {
                let eps = 1e-9;
                let precision = if t.beats_per_bar > 1 && step >= t.bar() - eps {
                    Precision::Bar
                } else if step >= t.beat() - eps {
                    Precision::Beat
                } else {
                    Precision::Tick
                };
                bars(seconds, t, precision)
            }
        }
    }

    /// Reads a time written as [`format`](Self::format) writes it (fewer
    /// fields too: `1:02`, `12.5` seconds; `17.2` for bar 17, beat 2).
    pub fn parse(&self, text: &str) -> Option<f64> {
        let text = text.trim();
        let (negative, body) = match text.strip_prefix('-') {
            Some(rest) => (true, rest.trim()),
            None => (false, text),
        };
        if body.is_empty() {
            return None;
        }
        let sign = if negative { -1.0 } else { 1.0 };
        match self {
            TimeScale::Seconds => parse_clock(body).map(|s| sign * s),
            TimeScale::Frames(r) => {
                let parts: Vec<&str> = body.split(':').collect();
                if parts.len() == 4 {
                    let n: Vec<u64> = parts
                        .iter()
                        .map(|p| p.trim().parse::<u64>().ok())
                        .collect::<Option<_>>()?;
                    let nominal = u64::from(r.nominal());
                    let frames = ((n[0] * 60 + n[1]) * 60 + n[2]) * nominal + n[3];
                    Some(sign * frames as f64 / r.fps())
                } else {
                    parse_clock(body).map(|s| self.quantize(sign * s))
                }
            }
            TimeScale::Beats(t) => {
                let n: Vec<i64> = body
                    .split('.')
                    .map(|p| p.trim().parse::<i64>().ok())
                    .collect::<Option<_>>()?;
                let (bar, beat, tick) = match n.as_slice() {
                    [bar] => (*bar, 1, 0),
                    [bar, beat] => (*bar, *beat, 0),
                    [bar, beat, tick] => (*bar, *beat, *tick),
                    _ => return None,
                };
                let bar = if negative { -bar } else { bar };
                let ticks = ((bar - 1) * t.bpb() + (beat - 1)) * t.tpb() + tick;
                Some(t.offset + ticks as f64 / t.tpb() as f64 * t.beat())
            }
        }
    }
}

/// How finely `bar.beat.tick` is written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Precision {
    Bar,
    Beat,
    Tick,
}

/// The decimals a grid `step` seconds apart needs: 0 from a second, 1 for
/// 0.5 or 0.1, 2 for 0.05.
fn decimals(step: f64) -> u32 {
    if step >= 1.0 - 1e-9 || step.is_nan() || step <= 0.0 {
        return 0;
    }
    (-step.log10() - 1e-9).ceil().clamp(0.0, 9.0) as u32
}

/// Seconds as a clock, `m:ss` or `h:mm:ss`, with `decimals` after the
/// seconds.
fn clock(seconds: f64, decimals: u32) -> String {
    let sign = if seconds < 0.0 && seconds.abs() >= 0.5 * 10f64.powi(-(decimals as i32)) {
        "-"
    } else {
        ""
    };
    let scale = 10u64.pow(decimals);
    let scaled = (seconds.abs() * scale as f64).round() as u64;
    let (whole, frac) = (scaled / scale, scaled % scale);
    let (h, m, s) = (whole / 3600, whole / 60 % 60, whole % 60);
    let mut out = if h > 0 {
        format!("{sign}{h}:{m:02}:{s:02}")
    } else {
        format!("{sign}{m}:{s:02}")
    };
    if decimals > 0 {
        out.push_str(&format!(".{frac:0width$}", width = decimals as usize));
    }
    out
}

/// `h:m:s.f`, `m:s.f` or `s.f`, in seconds.
fn parse_clock(text: &str) -> Option<f64> {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() > 3 {
        return None;
    }
    let mut total = 0.0;
    for (i, part) in parts.iter().enumerate() {
        let part = part.trim();
        let v: f64 = if i + 1 == parts.len() {
            part.parse().ok()?
        } else {
            f64::from(part.parse::<u32>().ok()?)
        };
        if !v.is_finite() || v < 0.0 {
            return None;
        }
        total = total * 60.0 + v;
    }
    Some(total)
}

/// A timecode, `HH:MM:SS:FF`, counted in nominal frames per second.
fn timecode(seconds: f64, rate: FrameRate) -> String {
    let frames = (seconds * rate.fps()).round() as i64;
    let sign = if frames < 0 { "-" } else { "" };
    let n = frames.unsigned_abs();
    let nominal = u64::from(rate.nominal());
    let ff = n % nominal;
    let s = n / nominal;
    format!(
        "{sign}{:02}:{:02}:{:02}:{ff:02}",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

/// `bar.beat.tick` from 1, as finely as `precision`.
fn bars(seconds: f64, tempo: &Tempo, precision: Precision) -> String {
    let tpb = tempo.tpb();
    let bpb = tempo.bpb();
    let ticks = ((seconds - tempo.offset) / tempo.beat() * tpb as f64).round() as i64;
    let beats = ticks.div_euclid(tpb);
    let tick = ticks.rem_euclid(tpb);
    let bar = beats.div_euclid(bpb) + 1;
    let beat = beats.rem_euclid(bpb) + 1;
    let width = (tpb - 1).max(1).to_string().len();
    match precision {
        Precision::Bar => format!("{bar}"),
        Precision::Beat => format!("{bar}.{beat}"),
        Precision::Tick => format!("{bar}.{beat}.{tick:0width$}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_read_and_write_as_a_clock() {
        let s = TimeScale::Seconds;
        assert_eq!(s.format(0.0), "0:00.000");
        assert_eq!(s.format(62.5), "1:02.500");
        assert_eq!(s.format(3723.25), "1:02:03.250");
        assert_eq!(s.format(-1.5), "-0:01.500");
        assert_eq!(s.parse("1:02.5"), Some(62.5));
        assert_eq!(s.parse("1:02:03.25"), Some(3723.25));
        assert_eq!(s.parse("-12.5"), Some(-12.5));
        assert_eq!(s.parse("nope"), None);
        assert_eq!(s.label(5.0, 1.0), "0:05");
        assert_eq!(s.label(5.5, 0.5), "0:05.5");
        assert_eq!(s.label(5.25, 0.05), "0:05.25");
        assert_eq!(s.label(59.96, 0.1), "1:00.0");
    }

    #[test]
    fn frames_are_a_timecode_of_whole_frames() {
        let s = TimeScale::frames(25);
        assert_eq!(s.format(0.0), "00:00:00:00");
        assert_eq!(s.format(62.48), "00:01:02:12");
        assert_eq!(s.format(3600.0 + 0.04), "01:00:00:01");
        assert_eq!(s.parse("00:01:02:12"), Some(62.48));
        assert_eq!(s.to_units(2.0), 50.0);
        assert_eq!(s.from_units(50.0), 2.0);
        assert_eq!(s.quantize(1.01), 1.0);
        assert_eq!(s.quantize(1.03), 1.04);
        assert_eq!(s.step(false), 0.04);
        assert_eq!(s.step(true), 1.0);
        // 29.97 counts 30 frames to its timecode's second.
        let ntsc = TimeScale::Frames(FrameRate::NTSC);
        assert_eq!(ntsc.format(30.0 * 1001.0 / 30000.0), "00:00:01:00");
        assert_eq!(FrameRate::NTSC.nominal(), 30);
    }

    #[test]
    fn beats_count_bars_from_one() {
        // 128 BPM: a beat is 0.46875 s, a bar of four 1.875 s.
        let s = TimeScale::beats(128.0);
        assert_eq!(s.format(0.0), "1.1.000");
        assert_eq!(s.format(1.875), "2.1.000");
        assert_eq!(s.format(1.875 + 0.46875 * 1.5), "2.2.480");
        assert_eq!(s.parse("2.2.480"), Some(1.875 + 0.46875 * 1.5));
        assert_eq!(s.parse("17"), Some(16.0 * 1.875));
        assert_eq!(s.label(1.875, 1.875), "2");
        assert_eq!(s.label(1.875 + 0.46875, 0.46875), "2.2");
        assert_eq!(s.to_units(0.9375), 2.0);
        assert_eq!(s.step(true), 1.875);
        // A first downbeat later in the track moves bar 1 there.
        let offset = TimeScale::Beats(Tempo::new(128.0, 4, 4).offset(0.46875));
        assert_eq!(offset.format(0.46875), "1.1.000");
        assert_eq!(offset.format(0.0), "0.4.000");
        assert_eq!(offset.round_to(0.6, 0.46875), 0.46875);
    }

    #[test]
    fn every_scale_formats_what_it_parses() {
        for s in [
            TimeScale::Seconds,
            TimeScale::frames(25),
            TimeScale::Frames(FrameRate::NTSC),
            TimeScale::beats(128.0),
            TimeScale::Beats(Tempo::new(90.0, 3, 4).offset(0.25)),
        ] {
            for t in [0.0, 0.04, 1.0, 12.48, 61.0, 3725.0] {
                let t = s.quantize(t);
                let back = s.parse(&s.format(t)).unwrap();
                let tolerance = match s {
                    TimeScale::Beats(tempo) => tempo.beat() / f64::from(tempo.ticks_per_beat),
                    _ => 1e-3,
                };
                assert!((back - t).abs() <= tolerance, "{s:?} {t} -> {back}");
            }
        }
    }

    #[test]
    fn steps_grow_and_land_on_units() {
        for s in [
            TimeScale::Seconds,
            TimeScale::frames(25),
            TimeScale::beats(128.0),
        ] {
            let steps = s.steps();
            assert!(steps.windows(2).all(|w| w[0] < w[1]), "{s:?}");
            for step in steps {
                let units = s.to_units(s.origin() + step);
                match s {
                    TimeScale::Frames(_) => assert!((units - units.round()).abs() < 1e-6),
                    TimeScale::Beats(_) => {
                        assert!((units * 16.0 - (units * 16.0).round()).abs() < 1e-6);
                    }
                    TimeScale::Seconds => {}
                }
            }
        }
    }
}
