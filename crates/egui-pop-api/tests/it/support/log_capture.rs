//! The log lines written, captured by one logger for the whole test
//! executable: `log::set_logger` succeeds once per process, and under
//! plain `cargo test` every module's tests share that process.

use std::sync::{Mutex, Once};

/// The log lines written at `Info` or above, by every test of the
/// executable.
pub static LINES: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Info
    }

    fn log(&self, r: &log::Record<'_>) {
        if self.enabled(r.metadata()) {
            LINES
                .lock()
                .unwrap()
                .push((r.level(), r.args().to_string()));
        }
    }

    fn flush(&self) {}
}

/// Installs the capture, once per process.
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        log::set_logger(&Capture).unwrap();
        log::set_max_level(log::LevelFilter::Info);
    });
}
