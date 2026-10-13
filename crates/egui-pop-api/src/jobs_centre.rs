//! The jobs centre: every job the host runs, as a panel (`Jobs`, closed
//! until asked for) and as a popover from the status bar's jobs item,
//! the same list in both.
//!
//! Each job shows its title, its state, a bar of its estimated overall
//! percentage (stacked with its failed and skipped items), "n of m",
//! the current step and item, the time left, the reason it failed, and
//! buttons that ask the host (pause, resume, cancel, retry) or forget it
//! (dismiss). A batch's children fold under it. Finished jobs go to the
//! history, cleared by hand.
//!
//! A small chevron before each job opens its **log console**: the
//! lines of its `job.log` events. The state is remembered per job
//! (`JobsService::expanded`), so the panel, the popover and the toast
//! agree. egui-pop's application layer draws the lines as plain text;
//! `egui-pop-editors` gives a pseudo-terminal console (its `Terminal`
//! over a `Console` source: colours, scrollback, selection, copy) by
//! providing [`JOB_LOG_VIEW`] (`JobConsolesPlugin`, which the facade
//! offers with its `editors` feature). The layering keeps the
//! application API below the editors: the console is a service the
//! upper layer provides, not a dependency.

use egui::{RichText, Ui};
use egui_pop_core::a11y;
use egui_pop_core::icon::{Icon, Size};
use egui_pop_core::progress::{ProgressBar, Tone};
use egui_pop_core::strings;
use egui_pop_core::theme::Tokens;
use egui_pop_model::ids::PanelId;
use egui_pop_model::jobs::{Job, JobState, Request};

use crate::app::AppHandle;
use crate::jobs::{self, JOBS, JobsService};
use crate::services::ServiceToken;
use crate::strings as txt;

/// The jobs panel.
pub const PANEL_JOBS: PanelId = PanelId::from_static("egui_pop.jobs");

/// How a job's log console is drawn: `f(ui, app, job)`.
pub struct JobLogView(Box<dyn Fn(&mut Ui, &AppHandle, &Job) + Send + Sync>);

impl JobLogView {
    pub fn new(f: impl Fn(&mut Ui, &AppHandle, &Job) + Send + Sync + 'static) -> Self {
        Self(Box::new(f))
    }

    pub fn show(&self, ui: &mut Ui, app: &AppHandle, job: &Job) {
        (self.0)(ui, app, job);
    }
}

impl std::fmt::Debug for JobLogView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JobLogView")
    }
}

/// The log console, provided by an upper layer (`egui-pop-editors`'s
/// pseudo-terminal); without it, plain lines.
pub const JOB_LOG_VIEW: ServiceToken<JobLogView> = ServiceToken::new("egui_pop.jobs.log_view");

/// How tall a log console is, in points.
pub const LOG_HEIGHT: f32 = 160.0;

/// The tone a job's bar takes.
pub fn tone(job: &Job) -> Tone {
    match job.state {
        JobState::Running => Tone::Accent,
        JobState::Paused | JobState::Cancelling | JobState::Cancelled => Tone::Muted,
        JobState::Done if job.failed > 0 => Tone::Warning,
        JobState::Done => Tone::Success,
        JobState::Failed => Tone::Danger,
    }
}

/// A job's state as words.
pub fn state_text(ctx: &egui::Context, job: &Job) -> String {
    let key = match job.state {
        JobState::Running => &txt::JOB_RUNNING,
        JobState::Paused => &txt::JOB_PAUSED,
        JobState::Cancelling => &txt::JOB_CANCELLING,
        JobState::Done => &txt::JOB_DONE,
        JobState::Failed => &txt::JOB_FAILED,
        JobState::Cancelled => &txt::JOB_CANCELLED,
    };
    let mut text = strings::tr(ctx, key);
    if job.requested.is_some() {
        text = strings::tr_args(ctx, &txt::JOB_ASKED, &[("state", &text)]);
    }
    text
}

/// Time left as people say it: seconds under a minute (by fives),
/// minutes otherwise.
pub fn eta_text(ctx: &egui::Context, seconds: f64) -> String {
    if seconds < 60.0 {
        let n = ((seconds / 5.0).ceil() * 5.0).max(5.0);
        strings::tr_args(
            ctx,
            &txt::JOB_ETA_SECONDS,
            &[("n", &strings::number(ctx, n, 0))],
        )
    } else {
        strings::tr_args(
            ctx,
            &txt::JOB_ETA_MINUTES,
            &[("n", &strings::number(ctx, (seconds / 60.0).round(), 0))],
        )
    }
}

/// What a job's line under its bar says: n of m, the step, the item,
/// the time left.
pub fn detail_text(ctx: &egui::Context, job: &Job, now: f64) -> String {
    let mut parts = Vec::new();
    if let Some(p) = job.percent() {
        parts.push(format!("{}%", strings::number(ctx, p.round(), 0)));
    }
    if let Some(total) = job.total {
        parts.push(strings::tr_args(
            ctx,
            &txt::JOB_OF,
            &[
                ("done", &strings::number(ctx, job.done as f64, 0)),
                ("total", &strings::number(ctx, total as f64, 0)),
            ],
        ));
    }
    if job.active() {
        if let Some(step) = job
            .step
            .as_ref()
            .and_then(|s| job.steps.iter().find(|x| &x.id == s))
        {
            parts.push(step.title.clone());
        }
        if let Some(item) = &job.item {
            parts.push(item.clone());
        }
        if let Some(eta) = job.eta(now) {
            parts.push(eta_text(ctx, eta));
        }
    }
    parts.join(" · ")
}

/// A button asking something of the host, named with the job.
fn ask_button(ui: &mut Ui, app: &AppHandle, job: &Job, request: Request) {
    let ctx = ui.ctx().clone();
    let label = strings::tr(
        &ctx,
        match request {
            Request::Pause => &txt::JOB_PAUSE,
            Request::Resume => &txt::JOB_RESUME,
            Request::Cancel => &strings::CANCEL,
            Request::Retry => &strings::RETRY,
        },
    );
    let asked = job.requested.is_some() || app.running(&jobs::command_of(request));
    let r = ui.add_enabled(!asked, egui::Button::new(&label).small());
    let name = strings::tr_args(
        &ctx,
        &txt::JOB_ACTION_NAME,
        &[("action", &label), ("job", &job.title)],
    );
    a11y::name(&r, egui::WidgetType::Button, &name);
    if r.clicked() {
        jobs::ask(app, &job.id, request);
    }
}

/// Lines without their ANSI escapes, for the plain console.
fn plain(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if ('@'..='~').contains(&d) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// The log console as plain lines (no pseudo-terminal provided).
fn plain_log(ui: &mut Ui, jobs: &JobsService, job: &Job) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let lines = jobs.log_from(&job.id, 0);
    egui::Frame::new()
        .fill(t.surface.sunken)
        .corner_radius(t.radius.widget)
        .inner_margin(t.space.s)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("egui_pop_job_log", &job.id))
                .max_height(LOG_HEIGHT)
                .stick_to_bottom(true)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if lines.is_empty() {
                        ui.weak(strings::tr(&ctx, &txt::JOB_NO_LOG));
                    }
                    for l in &lines {
                        ui.label(RichText::new(plain(l)).monospace());
                    }
                });
        });
}

/// Draws one job, its log console when open, and its children.
pub fn job_row(ui: &mut Ui, app: &AppHandle, jobs: &JobsService, job: &Job, depth: usize) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let now = jobs.now();
    let indent = t.space.l * depth as f32;
    let open = jobs.expanded(&job.id);
    ui.horizontal(|ui| {
        ui.add_space(indent);
        let (icon, key) = if open {
            (Icon::ChevronDown, &txt::JOB_HIDE_LOG)
        } else {
            (Icon::ChevronRight, &txt::JOB_SHOW_LOG)
        };
        let name = strings::tr_args(&ctx, key, &[("job", &job.title)]);
        let chevron = ui.add(icon.size(Size::Inline).label(&name).sense_click());
        if chevron.clicked() {
            jobs.set_expanded(&job.id, !open);
        }
        ui.label(RichText::new(&job.title).strong());
        let color = match job.state {
            JobState::Failed => t.status.danger,
            JobState::Done if job.failed > 0 => t.status.warning,
            JobState::Done => t.status.success,
            _ => t.text.muted,
        };
        ui.label(RichText::new(state_text(&ctx, job)).small().color(color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if job.parent.is_some() {
                return;
            }
            if job.active() {
                if job.cancellable && job.state != JobState::Cancelling {
                    ask_button(ui, app, job, Request::Cancel);
                }
                if job.pausable && job.state == JobState::Running {
                    ask_button(ui, app, job, Request::Pause);
                }
                if job.state == JobState::Paused {
                    ask_button(ui, app, job, Request::Resume);
                }
            } else {
                let label = strings::tr(&ctx, &txt::JOB_DISMISS);
                let r = ui.small_button(&label);
                a11y::name(
                    &r,
                    egui::WidgetType::Button,
                    &strings::tr_args(
                        &ctx,
                        &txt::JOB_ACTION_NAME,
                        &[("action", &label), ("job", &job.title)],
                    ),
                );
                if r.clicked() {
                    jobs.dismiss(&job.id);
                }
                if job.retryable && job.state != JobState::Done {
                    ask_button(ui, app, job, Request::Retry);
                }
                for a in &job.actions {
                    if ui.small_button(&a.title).clicked()
                        && let Ok(id) = egui_pop_model::ids::CommandId::new(&a.command)
                    {
                        app.run(id, a.args.clone());
                    }
                }
            }
        });
    });
    let inset = indent + t.size.icon_inline + t.space.s;
    ui.horizontal(|ui| {
        ui.add_space(inset);
        ui.vertical(|ui| {
            let mut bar = ProgressBar::new(job.fraction.map(|f| f as f32))
                .tone(tone(job))
                .label(&job.title);
            if let Some(total) = job.total.filter(|t| *t > 0) {
                let share = |n: u64| (n as f64 / total as f64) as f32;
                if job.failed > 0 {
                    bar = bar.section(share(job.failed), Tone::Danger, "failed");
                }
                if job.skipped > 0 {
                    bar = bar.section(share(job.skipped), Tone::Muted, "skipped");
                }
            }
            let detail = detail_text(&ctx, job, now);
            if !detail.is_empty() {
                bar = bar.text(&detail);
            }
            ui.add(bar);
            if !detail.is_empty() {
                ui.label(RichText::new(&detail).small().color(t.text.muted));
            }
            if let Some(e) = &job.error
                && !e.is_empty()
            {
                ui.label(RichText::new(e).small().color(t.status.danger));
            }
            if job.failed > 0 && !job.errors.is_empty() {
                let failed = strings::tr_args(
                    &ctx,
                    &txt::JOBS_FAILED,
                    &[("count", &job.failed.to_string())],
                );
                egui::CollapsingHeader::new(RichText::new(failed).small().color(t.status.warning))
                    .id_salt(("egui_pop_job_errors", &job.id))
                    .show(ui, |ui| {
                        for e in job.errors.iter().rev().take(20) {
                            ui.label(RichText::new(format!("{}: {}", e.item, e.error)).small());
                        }
                    });
            }
            if let Some(e) = &job.request_error {
                ui.label(
                    RichText::new(strings::tr_args(&ctx, &txt::JOB_REFUSED, &[("error", e)]))
                        .small()
                        .color(t.status.danger),
                );
            }
            if open {
                match app.service(&JOB_LOG_VIEW) {
                    Some(view) => view.show(ui, app, job),
                    None => plain_log(ui, jobs, job),
                }
            }
        });
    });
    for child in jobs.children(&job.id) {
        job_row(ui, app, jobs, &child, depth + 1);
    }
}

/// The list: the jobs going on, then the history.
pub fn show(ui: &mut Ui, app: &AppHandle) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    let Some(jobs) = app.service(&JOBS) else {
        return;
    };
    let roots = jobs.roots();
    if roots.is_empty() {
        ui.weak(strings::tr(&ctx, &txt::JOBS_NONE));
        return;
    }
    let (active, ended): (Vec<&Job>, Vec<&Job>) = roots.iter().partition(|j| j.active());
    for job in &active {
        job_row(ui, app, &jobs, job, 0);
        ui.add_space(t.space.s);
    }
    if !ended.is_empty() {
        ui.add_space(t.space.s);
        ui.horizontal(|ui| {
            ui.label(RichText::new(strings::tr(&ctx, &txt::JOBS_HISTORY)).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(strings::tr(&ctx, &txt::JOBS_CLEAR))
                    .clicked()
                {
                    jobs.clear_finished();
                }
            });
        });
        for job in ended.iter().rev() {
            job_row(ui, app, &jobs, job, 0);
            ui.add_space(t.space.s);
        }
    }
}

/// The popover from the status bar's jobs item: the list, and a way to
/// the panel.
pub(crate) fn popover(ui: &mut Ui, app: &AppHandle, response: &egui::Response) {
    let ctx = ui.ctx().clone();
    let t = Tokens::of(&ctx);
    egui::Popup::from_toggle_button_response(response)
        .id(popover_id())
        .kind(egui::PopupKind::Popup)
        .align(egui::RectAlign::TOP_START)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(t.size.dialog_width.min(420.0));
            ui.horizontal(|ui| {
                ui.label(RichText::new(strings::tr(&ctx, &txt::JOBS_TITLE)).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if app.panels().iter().any(|p| p.id == PANEL_JOBS)
                        && ui
                            .small_button(strings::tr(&ctx, &txt::JOBS_OPEN_PANEL))
                            .clicked()
                    {
                        app.focus_panel(&PANEL_JOBS);
                        egui::Popup::close_id(&ctx, popover_id());
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .auto_shrink([false, true])
                .show(ui, |ui| show(ui, app));
        });
}

/// The popover's id.
pub fn popover_id() -> egui::Id {
    egui::Id::new("egui_pop.jobs.popover")
}

/// Whether the jobs popover is open.
pub fn popover_open(ctx: &egui::Context) -> bool {
    egui::Popup::is_id_open(ctx, popover_id())
}

/// Opens or closes the jobs popover.
pub fn toggle_popover(ctx: &egui::Context) {
    egui::Popup::toggle_id(ctx, popover_id());
}

/// A job's progress as a toast draws it: its bar and, while it goes
/// on, a button to cancel it; once ended, its first action.
pub(crate) fn toast_part(ui: &mut Ui, app: &AppHandle, id: &str) -> Option<bool> {
    let jobs = app.service(&JOBS)?;
    let job = jobs.job(id)?;
    let ctx = ui.ctx().clone();
    ui.add(
        ProgressBar::new(job.fraction.map(|f| f as f32))
            .tone(tone(&job))
            .label(&job.title)
            .text(detail_text(&ctx, &job, jobs.now())),
    );
    ui.horizontal(|ui| {
        ui.label(RichText::new(state_text(&ctx, &job)).small());
        if job.active() && job.cancellable && job.state != JobState::Cancelling {
            ask_button(ui, app, &job, Request::Cancel);
        } else if let Some(a) = job.actions.first()
            && ui.small_button(&a.title).clicked()
            && let Ok(command) = egui_pop_model::ids::CommandId::new(&a.command)
        {
            app.run(command, a.args.clone());
        }
    });
    Some(job.active())
}

#[cfg(test)]
mod tests {
    use super::plain;

    #[test]
    fn the_plain_console_drops_ansi_escapes() {
        assert_eq!(plain("\x1b[1;32mok\x1b[0m done"), "ok done");
    }
}
