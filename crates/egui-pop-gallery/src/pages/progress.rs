//! The progress page: bars (determinate, indeterminate, toned, with
//! stacked sections, a thin line), rings, meters against thresholds,
//! spinners, one of them delayed, and controls bound to a command the
//! host answers later, busy until it does. Motion is frozen in the
//! tests, so the indeterminate ones are the snapshots' fixtures too.

use std::cell::RefCell;
use std::rc::Rc;

use egui::Ui;
use egui_pop::api::busy::CommandButton;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::icon::Icon;
use egui_pop::core::motion;
use egui_pop::core::progress::{Meter, ProgressBar, Ring, Spinner, Tone};
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{CommandId, PluginId, StoryId};
use serde_json::json;

use super::section;
use crate::host::Waiting;
use crate::shell::SHELL;

pub const PROGRESS_PAGE: PluginId = PluginId::from_static("gallery.progress");

/// A command the gallery's host answers later.
pub const CMD_SLOW_EXPORT: CommandId = CommandId::from_static("gallery.slow.export");

/// The width of the bars.
const BAR: f32 = 320.0;

/// Pretend loading, timed by the motion clock.
#[derive(Debug, Default)]
pub struct State {
    load: Option<(f64, f64)>,
}

/// The progress page, and the commands its host answers later.
#[derive(Debug, Default)]
pub struct ProgressPage {
    pub waiting: Waiting,
}

impl Plugin for ProgressPage {
    fn id(&self) -> PluginId {
        PROGRESS_PAGE
    }

    fn title(&self) -> String {
        "Progress".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.declare(
            CommandSpec::new(CMD_SLOW_EXPORT, "Export", "Gallery")
                .description(
                    "Exports the demo images; the gallery's host answers later, when told.",
                )
                .hidden()
                .no_agent(),
        )?;
        let state = Rc::new(RefCell::new(State::default()));
        let waiting = self.waiting.clone();
        ctx.story(
            Story::new(StoryId::from_static("progress"), "Progress", move |ui, app| {
                show(ui, app, &mut state.borrow_mut(), &waiting)
            })
            .description("Progress bars (determinate, indeterminate, toned, with sections, a thin line), rings, meters and spinners, one delayed; controls busy while the host has not answered.")
            .group("Progress and jobs", 132),
        )?;
        Ok(())
    }
}

pub fn show(ui: &mut Ui, app: &AppHandle, state: &mut State, waiting: &Waiting) {
    let ctx = ui.ctx().clone();
    ui.label("Every one is an AccessKit progress indicator with a name; a determinate one has a value and its range, an indeterminate one none. Under reduced motion the indeterminate ones stand still.");
    section(ui, "Busy controls");
    ui.label("The host answers this command later: its button greys at once, shows a spinner in its icon's place after 300 ms, and keeps its width and its name.");
    ui.horizontal_wrapped(|ui| {
        ui.add(CommandButton::new(app, CMD_SLOW_EXPORT).icon(Icon::Save));
        let n = waiting.len();
        if ui
            .add_enabled(n > 0, egui::Button::new("Answer the host's commands"))
            .clicked()
        {
            waiting.answer(&Ok(json!({ "exported": 3 })));
        }
        if ui
            .add_enabled(n > 0, egui::Button::new("Refuse them"))
            .clicked()
        {
            waiting.answer(&Err("the export folder is read-only".into()));
        }
    });
    ui.weak("Menu items bound to such a command do the same, and a status bar indicator shows a delayed spinner while busy (the strips page's job).");
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
        egui::Id::unique("gallery.progress.load"),
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
