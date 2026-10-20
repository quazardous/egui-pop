//! The motion page: the motion clock (the interface's time, or frozen
//! and stepped by hand), the motion tokens, a looping animation that
//! stands still under reduced motion, and the busy delay: short work
//! shows nothing, long work shows its indicator at least its minimum.

use egui::{Sense, Stroke, Ui, vec2};
use egui_pop::core::motion::{self, Busy};
use egui_pop::theme::Tokens;

use super::section;

/// Pretend work, timed by the motion clock.
#[derive(Debug, Default)]
pub struct State {
    /// When the work started and when it ends.
    work: Option<(f64, f64)>,
    /// How many times the indicator showed.
    shown: u32,
    was_shown: bool,
}

pub fn show(ui: &mut Ui, state: &mut State) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let now = motion::now(&ctx);
    ui.label("Spinners, travelling bars and shimmer read the motion clock, not the input's time: tests freeze it and step it, and reduced motion gives still frames.");
    section(ui, "The clock");
    ui.horizontal_wrapped(|ui| {
        ui.label(format!(
            "Now: {now:.2} s ({})",
            if motion::controlled(&ctx) {
                "a given clock: it moves only when told"
            } else {
                "the interface's time"
            }
        ));
        if ui.button("Freeze").clicked() {
            motion::freeze(&ctx, now);
        }
        if ui.button("Step 100 ms").clicked() {
            motion::freeze(&ctx, now + 0.1);
        }
        if ui.button("Let it run").clicked() {
            motion::set_clock(&ctx, None);
        }
    });
    section(ui, "Tokens");
    egui::Grid::new("gallery.motion.tokens")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            let rows = [
                ("motion.duration", t.motion_duration),
                ("motion.spinner.delay", t.motion.spinner_delay),
                ("motion.spinner.min", t.motion.spinner_min),
                ("motion.spinner.turn", t.motion.spinner_turn),
                ("motion.progress.travel", t.motion.progress_travel),
                ("motion.skeleton.shimmer", t.motion.skeleton_shimmer),
            ];
            for (name, seconds) in rows {
                ui.monospace(name);
                ui.label(format!("{:.0} ms", seconds * 1000.0));
                ui.end_row();
            }
        });
    section(ui, "A looping animation");
    let reduced = motion::reduced(&ctx);
    let p = motion::phase(&ctx, f64::from(t.motion.progress_travel), 0.5);
    let (rect, _) = ui.allocate_exact_size(vec2(240.0, t.space.l), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, t.border.strong),
    );
    let x = rect.left() + rect.width() * p;
    ui.painter()
        .circle_filled(egui::pos2(x, rect.center().y), t.space.s, t.accent.default);
    motion::animate(&ctx);
    ui.weak(if reduced {
        "Reduced motion: the dot stands still."
    } else {
        "The dot crosses in motion.progress.travel; under reduced motion it stands still."
    });
    section(ui, "The busy delay");
    ui.horizontal_wrapped(|ui| {
        for (label, seconds) in [
            ("Work for 0.2 s", 0.2),
            ("Work for 1 s", 1.0),
            ("Work for 3 s", 3.0),
        ] {
            if ui.button(label).clicked() {
                state.work = Some((now, now + seconds));
            }
        }
    });
    let (busy, since) = match state.work {
        Some((from, until)) => (now < until, from),
        None => (false, now),
    };
    if let Some((_, until)) = state.work
        && busy
    {
        motion::wake_at(&ctx, until);
    }
    let visible = Busy::new(egui::Id::unique("gallery.motion.busy"), since).visible(&ctx, busy);
    if visible && !state.was_shown {
        state.shown += 1;
    }
    state.was_shown = visible;
    ui.label(format!(
        "Work: {}. Busy indicator: {} (shown {} times).",
        if busy { "running" } else { "idle" },
        if visible { "shown" } else { "hidden" },
        state.shown
    ));
    ui.weak("Nothing shows before motion.spinner.delay; once shown, the indicator stays motion.spinner.min, counted from when the work started.");
}
