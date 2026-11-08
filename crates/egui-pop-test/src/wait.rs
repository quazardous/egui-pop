//! Waiting in a test: frames are stepped until a condition holds, or
//! until a deadline. The conditions are `bridge.wait`'s
//! ([`egui_pop_api::wait`]): a journal event, the application settled,
//! or the first of several.
//!
//! ```
//! use egui_pop_api::about::{AboutPlugin, AppInfo};
//! use egui_pop_api::{App, DefaultPlugins};
//! use egui_pop_api::wait::{WaitFor, Waited};
//! use egui_pop_test::wait::{Deadline, wait_until};
//!
//! let mut h = egui_pop_test::app(
//!     App::builder()
//!         .plugins(DefaultPlugins)
//!         .plugin(AboutPlugin::new(AppInfo::new("Demo", "1.0.0"))),
//!     egui::vec2(800.0, 600.0),
//! );
//! h.run_ok();
//! let since = h.state().handle().last_event();
//! h.state()
//!     .handle()
//!     .debug_call("commands", "run", &serde_json::json!({ "id": "about.read" }));
//! let done = wait_until(&mut h, &WaitFor::event("command.done"), since, Deadline::default());
//! assert!(matches!(done, Ok(Waited::Event(_))), "{done:?}");
//! ```

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_pop_api::App;
use egui_pop_api::host::Seq;
use egui_pop_api::wait::{WaitFor, Waited};

/// When a wait gives up: after `frames` frames or `time`, whichever
/// comes first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deadline {
    /// The most frames stepped.
    pub frames: u32,
    /// The most wall-clock time.
    pub time: Duration,
}

impl Default for Deadline {
    /// 120 frames or 10 seconds.
    fn default() -> Self {
        Self {
            frames: 120,
            time: Duration::from_secs(10),
        }
    }
}

/// Steps frames until `until` holds for the events after `since`, and
/// says what made it hold; checked before the first frame too. Past the
/// deadline, an error naming the condition, the frames stepped and the
/// journal's last events.
pub fn wait_until(
    h: &mut Harness<'_, App>,
    until: &WaitFor,
    since: Seq,
    deadline: Deadline,
) -> Result<Waited, String> {
    let start = Instant::now();
    let mut frames = 0;
    loop {
        if let Some(w) = h.state().handle().wait_met(until, since) {
            return Ok(w);
        }
        if frames >= deadline.frames || start.elapsed() >= deadline.time {
            let handle = h.state().handle();
            let tail: Vec<String> = handle
                .events_since(since)
                .iter()
                .rev()
                .take(8)
                .map(|e| format!("#{} {} {}", e.seq, e.name, e.data))
                .collect();
            return Err(format!(
                "not met after {frames} frames ({:?}): {}; settled: {}; last events: {tail:?}",
                start.elapsed(),
                until.to_args(),
                handle.settled(),
            ));
        }
        h.step();
        frames += 1;
    }
}

/// [`wait_until`] the application is settled.
pub fn settle(h: &mut Harness<'_, App>, deadline: Deadline) -> Result<(), String> {
    let since = h.state().handle().last_event();
    wait_until(h, &WaitFor::Settled, since, deadline).map(|_| ())
}
