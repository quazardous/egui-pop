//! The fake recording: a few entities on one `frame` timeline, their
//! values computed from the frame, and a text log. Nothing is read from
//! anywhere; the same frame always gives the same data.

use egui::Color32;

/// Frames on the timeline, at [`FPS`].
pub const FRAMES: u32 = 300;
/// Frames per second, for the time readout.
pub const FPS: f32 = 30.0;

/// What an entity holds, and so which view shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Scalar,
    Image,
    Boxes,
    Text,
}

impl Kind {
    pub fn archetype(self) -> &'static str {
        match self {
            Kind::Scalar => "Scalars",
            Kind::Image => "Image",
            Kind::Boxes => "Boxes2D",
            Kind::Text => "TextLog",
        }
    }
}

/// The demo views, each a panel of the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum View {
    TimeSeries,
    Camera,
    Dataframe,
    Logs,
}

impl View {
    pub const ALL: [View; 4] = [View::TimeSeries, View::Camera, View::Dataframe, View::Logs];

    pub fn title(self) -> &'static str {
        match self {
            View::TimeSeries => "Time series",
            View::Camera => "Camera",
            View::Dataframe => "Dataframe",
            View::Logs => "Logs",
        }
    }

    pub fn kind(self) -> &'static str {
        match self {
            View::TimeSeries => "TimeSeriesView",
            View::Camera => "Spatial2DView",
            View::Dataframe => "DataframeView",
            View::Logs => "TextLogView",
        }
    }

    /// Its key in the blueprint tree.
    pub fn key(self) -> String {
        format!("view:{}", self.title())
    }

    pub fn from_key(key: &str) -> Option<View> {
        View::ALL.into_iter().find(|v| v.key() == key)
    }

    /// The entities it shows, by index into [`ENTITIES`].
    pub fn entities(self) -> Vec<usize> {
        ENTITIES
            .iter()
            .enumerate()
            .filter(|(_, e)| e.views.contains(&self))
            .map(|(i, _)| i)
            .collect()
    }
}

/// An entity of the recording.
#[derive(Debug)]
pub struct Entity {
    pub path: &'static str,
    pub kind: Kind,
    /// Its colour in the views (a data colour, not a theme's).
    pub color: Color32,
    pub views: &'static [View],
    /// Every how many frames it is logged.
    pub every: u32,
    pub unit: &'static str,
}

pub const ENTITIES: [Entity; 7] = [
    Entity {
        path: "/sensors/temperature",
        kind: Kind::Scalar,
        color: Color32::from_rgb(0xe8, 0x6a, 0x4f),
        views: &[View::TimeSeries, View::Dataframe],
        every: 1,
        unit: "C",
    },
    Entity {
        path: "/sensors/pressure",
        kind: Kind::Scalar,
        color: Color32::from_rgb(0x4f, 0xb0, 0xe8),
        views: &[View::TimeSeries, View::Dataframe],
        every: 2,
        unit: "kPa",
    },
    Entity {
        path: "/sensors/humidity",
        kind: Kind::Scalar,
        color: Color32::from_rgb(0x7c, 0xd6, 0x6b),
        views: &[View::TimeSeries, View::Dataframe],
        every: 3,
        unit: "%",
    },
    Entity {
        path: "/camera/image",
        kind: Kind::Image,
        color: Color32::from_rgb(0xb7, 0x80, 0xff),
        views: &[View::Camera],
        every: 10,
        unit: "",
    },
    Entity {
        path: "/camera/detections",
        kind: Kind::Boxes,
        color: Color32::from_rgb(0xff, 0xc4, 0x3d),
        views: &[View::Camera],
        every: 5,
        unit: "",
    },
    Entity {
        path: "/robot/status",
        kind: Kind::Text,
        color: Color32::from_rgb(0xcc, 0xcc, 0xcc),
        views: &[View::Logs],
        every: 9,
        unit: "",
    },
    Entity {
        path: "/logs",
        kind: Kind::Text,
        color: Color32::from_rgb(0xcc, 0xcc, 0xcc),
        views: &[View::Logs],
        every: 7,
        unit: "",
    },
];

/// The entity of a path, by index.
pub fn entity_of(path: &str) -> Option<usize> {
    ENTITIES.iter().position(|e| e.path == path)
}

/// A small deterministic noise in -1..1.
fn noise(seed: u32, frame: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9e37_79b9) ^ frame.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x & 0xffff) as f32 / 32768.0 - 1.0
}

/// A scalar entity's value at a frame (the last one logged at or before
/// it).
pub fn scalar(entity: usize, frame: u32) -> f32 {
    let every = ENTITIES[entity].every.max(1);
    let f = (frame / every * every) as f32;
    let t = f / FPS;
    match entity {
        0 => 26.0 + 9.0 * (t * 0.7).sin() + 0.8 * noise(1, f as u32),
        1 => 72.0 + 12.0 * (t * 0.31 + 1.0).cos() + 0.5 * noise(2, f as u32),
        _ => 48.0 + 16.0 * (t * 0.45 + 2.0).sin() * (t * 0.1).cos() + noise(3, f as u32),
    }
}

/// Whether an entity has data at this frame.
pub fn logged_at(entity: usize, frame: u32) -> bool {
    frame.is_multiple_of(ENTITIES[entity].every.max(1))
}

/// A detection box at a frame: its label and its rectangle in the
/// image's normalised coordinates (0 to 1).
pub fn detections(frame: u32) -> Vec<(&'static str, [f32; 4])> {
    let t = (frame / 5 * 5) as f32 / FPS;
    vec![
        ("person", [0.12 + 0.05 * (t * 0.8).sin(), 0.30, 0.16, 0.48]),
        ("cart", [0.55 + 0.12 * (t * 0.4).cos(), 0.55, 0.26, 0.24]),
        (
            "drone",
            [
                0.40 + 0.25 * (t * 0.6).sin(),
                0.10 + 0.04 * (t * 1.3).cos(),
                0.10,
                0.10,
            ],
        ),
    ]
}

/// How severe a log line is: the text log view's six levels.
pub use egui_pop::views::text_log::Level;

/// A line of the text log.
#[derive(Clone, Debug)]
pub struct LogLine {
    pub frame: u32,
    pub entity: usize,
    pub level: Level,
    pub text: String,
}

/// Every line of the recording, in frame order.
pub fn log() -> Vec<LogLine> {
    let mut lines = Vec::new();
    for frame in 0..=FRAMES {
        if frame % 7 == 0 {
            let (level, text) = match (frame / 7) % 6 {
                0 => (
                    Level::Info,
                    format!("tick {}: all sensors nominal", frame / 7),
                ),
                1 => (
                    Level::Debug,
                    format!("camera exposure {:.2} ms", 8.0 + noise(4, frame) * 2.0),
                ),
                2 => (Level::Trace, "planner idle".to_string()),
                3 => (Level::Info, format!("{} detections", 3)),
                4 if frame > 150 => (
                    Level::Warn,
                    format!("pressure drifting: {:.1} kPa", scalar(1, frame)),
                ),
                4 => (Level::Info, "pressure steady".to_string()),
                _ => (Level::Debug, format!("humidity {:.0} %", scalar(2, frame))),
            };
            lines.push(LogLine {
                frame,
                entity: 6,
                level,
                text,
            });
        }
        if frame % 9 == 0 {
            let (level, text) = match (frame / 9) % 5 {
                0 => (Level::Info, "robot: moving to waypoint".to_string()),
                1 => (
                    Level::Debug,
                    format!("robot: joint error {:.3} rad", noise(5, frame).abs() * 0.02),
                ),
                2 if frame == 243 => (
                    Level::Critical,
                    "robot: emergency stop\nreason: the gripper did not answer three times\nstate: arm halted, waiting for a reset".to_string(),
                ),
                2 if frame > 200 => (Level::Error, "robot: gripper did not answer".to_string()),
                2 => (Level::Info, "robot: gripper closed".to_string()),
                3 => (Level::Trace, "robot: heartbeat".to_string()),
                _ => (Level::Info, "robot: waypoint reached".to_string()),
            };
            lines.push(LogLine {
                frame,
                entity: 5,
                level,
                text,
            });
        }
    }
    lines
}

/// The camera's picture: a sky, a ground and a sun, generated once.
pub fn image(width: usize, height: usize) -> egui::ColorImage {
    let mut pixels = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let (u, v) = (x as f32 / width as f32, y as f32 / height as f32);
            let c = if v < 0.62 {
                let k = v / 0.62;
                [40.0 + 60.0 * k, 70.0 + 70.0 * k, 120.0 + 60.0 * k]
            } else {
                let k = (v - 0.62) / 0.38;
                let stripe = if ((u * 24.0 + k * 6.0) as i32) % 2 == 0 {
                    8.0
                } else {
                    0.0
                };
                [
                    70.0 - 30.0 * k + stripe,
                    80.0 - 30.0 * k + stripe,
                    60.0 - 25.0 * k + stripe,
                ]
            };
            let sun = ((u - 0.78).powi(2) + (v - 0.2).powi(2) * 2.0).sqrt();
            let glow = (1.0 - sun * 7.0).clamp(0.0, 1.0);
            pixels.push(Color32::from_rgb(
                (c[0] + 180.0 * glow).min(255.0) as u8,
                (c[1] + 150.0 * glow).min(255.0) as u8,
                (c[2] + 60.0 * glow).min(255.0) as u8,
            ));
        }
    }
    egui::ColorImage::new([width, height], pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_view_shows_something() {
        for v in View::ALL {
            assert!(!v.entities().is_empty(), "{v:?}");
        }
    }

    #[test]
    fn the_log_is_in_frame_order() {
        let log = log();
        assert!(log.windows(2).all(|w| w[0].frame <= w[1].frame));
        assert!(log.iter().any(|l| l.level == Level::Error));
    }
}
