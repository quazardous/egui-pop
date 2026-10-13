//! The progress page: bars (determinate, indeterminate, toned, with
//! stacked sections, a thin line), rings, meters against thresholds,
//! and spinners, one of them delayed. Motion is frozen in the tests, so
//! the indeterminate ones are the snapshots' fixtures too.

use egui::Ui;
use egui_pop::core::motion;
use egui_pop::core::progress::{Meter, ProgressBar, Ring, Spinner, Tone};

use super::section;

/// The width of the bars.
const BAR: f32 = 320.0;

/// Pretend loading, timed by the motion clock.
#[derive(Debug, Default)]
pub struct State {
    load: Option<(f64, f64)>,
}

pub fn show(ui: &mut Ui, state: &mut State) {
    let ctx = ui.ctx().clone();
    ui.label("Every one is an AccessKit progress indicator with a name; a determinate one has a value and its range, an indeterminate one none. Under reduced motion the indeterminate ones stand still.");
    section(ui, "Spinners");
    ui.add(Spinner::new().label("Loading previews"));
    let now = motion::now(&ctx);
    ui.horizontal_wrapped(|ui| {
        for (label, seconds) in [("Load for 0.2 s", 0.2), ("Load for 2 s", 2.0)] {
            if ui.button(label).clicked() {
                state.load = Some((now, now + seconds));
            }
        }
    });
    let (busy, since) = match state.load {
        Some((from, until)) => (now < until, from),
        None => (false, now),
    };
    if let Some((_, until)) = state.load
        && busy
    {
        motion::wake_at(&ctx, until);
    }
    ui.add(Spinner::new().label("Loading thumbnails").delayed(
        egui::Id::new("gallery.progress.load"),
        since,
        busy,
    ));
    ui.weak("The delayed spinner shows only after 300 ms of loading, then stays at least 500 ms; its space stays taken.");
    section(ui, "Bars");
    ui.add(
        ProgressBar::new(Some(0.42))
            .label("Exporting 12 images")
            .width(BAR)
            .caption(true),
    );
    ui.add(
        ProgressBar::counts(340, 1204)
            .label("Importing photos")
            .section(61.0 / 1204.0, Tone::Danger, "failed")
            .section(20.0 / 1204.0, Tone::Muted, "skipped")
            .text("340 of 1,204, 61 failed, 20 skipped")
            .width(BAR)
            .caption(true),
    );
    ui.add(
        ProgressBar::indeterminate()
            .label("Scanning the library")
            .width(BAR)
            .caption(true),
    );
    ui.add(
        ProgressBar::new(Some(0.66))
            .line()
            .label("Recomputing the preview")
            .width(BAR),
    );
    section(ui, "Tones");
    for (label, fraction, tone) in [
        ("Exported", 1.0, Tone::Success),
        ("Exported, 3 failed", 1.0, Tone::Warning),
        ("Upload failed", 0.35, Tone::Danger),
        ("Indexing", 0.5, Tone::Info),
        ("Paused", 0.5, Tone::Muted),
    ] {
        ui.add(
            ProgressBar::new(Some(fraction))
                .tone(tone)
                .label(label)
                .width(BAR)
                .caption(true),
        );
    }
    section(ui, "Rings");
    ui.horizontal(|ui| {
        ui.add(Ring::new(Some(0.0)).label("Queued"));
        ui.add(Ring::new(Some(0.25)).label("Rendering, a quarter"));
        ui.add(Ring::new(Some(0.75)).label("Rendering, three quarters"));
        ui.add(Ring::new(Some(1.0)).tone(Tone::Success).label("Rendered"));
        ui.add(
            Ring::new(Some(0.4))
                .tone(Tone::Danger)
                .label("Render failed"),
        );
        ui.add(Ring::new(None).label("Waiting for the GPU"));
        ui.add(Ring::new(Some(0.6)).size(32.0).label("Rendering, large"));
    });
    section(ui, "Meters");
    for (label, used) in [("Disk", 48.0), ("Cache", 80.0), ("GPU memory", 95.0)] {
        ui.horizontal(|ui| {
            ui.add(
                Meter::new(used, 0.0, 100.0)
                    .threshold(75.0, Tone::Warning)
                    .threshold(90.0, Tone::Danger)
                    .label(label)
                    .text(format!("{used:.0} GB of 100 GB"))
                    .width(BAR),
            );
            ui.label(format!("{label}: {used:.0} GB of 100 GB"));
        });
    }
}
