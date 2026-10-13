//! Each job's log console as a pseudo-terminal: [`JobConsolesPlugin`]
//! gives the jobs centre (`egui_pop_api::jobs_centre`) a [`Terminal`]
//! over a [`Console`] per job, fed from the job's `job.log` lines as they
//! come, so a build's colours, a long log's scrollback, selection and
//! copy work in the panel, the popover and the toast alike.
//!
//! The application API sits below the editors (ADR 0001): it draws plain
//! lines by itself, and this plugin replaces them by providing its
//! `JOB_LOG_VIEW` service. With `TerminalPlugin` connected, each console
//! is also a terminal agents read (`terminal.read`, `job:<id>`).
//!
//! ```
//! use egui_pop_api::App;
//! use egui_pop_api::jobs::JobsPlugin;
//! use egui_pop_editors::terminal::jobs::JobConsolesPlugin;
//!
//! let app = App::builder()
//!     .plugin(JobsPlugin::default())
//!     .plugin(JobConsolesPlugin)
//!     .build(&egui::Context::default())
//!     .unwrap();
//! assert!(app.handle().service(&egui_pop_api::jobs_centre::JOB_LOG_VIEW).is_some());
//! ```

use std::collections::HashMap;
use std::sync::Mutex;

use egui::Ui;
use egui_pop_api::jobs::JOBS;
use egui_pop_api::jobs_centre::{JOB_LOG_VIEW, JobLogView, LOG_HEIGHT};
use egui_pop_api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop_core::lock;
use egui_pop_model::ids::PluginId;
use egui_pop_model::jobs::Job;

use super::steps::TERMINALS;
use super::{Console, Terminal, TerminalState};

/// A job's console: its source, its terminal, how many lines it got.
#[derive(Clone, Debug)]
struct JobConsole {
    console: Console,
    terminal: TerminalState,
    fed: u64,
}

/// The consoles, by job.
#[derive(Debug, Default)]
struct Consoles(Mutex<HashMap<String, JobConsole>>);

impl Consoles {
    /// The job's console, given the lines it has not had yet.
    fn feed(&self, app: &AppHandle, job: &Job) -> TerminalState {
        let mut map = lock(&self.0);
        let c = map.entry(job.id.clone()).or_insert_with(|| {
            let console = Console::new();
            JobConsole {
                terminal: TerminalState::new(format!("job:{}", job.id), console.clone()),
                console,
                fed: 0,
            }
        });
        if job.log_total > c.fed {
            if let Some(jobs) = app.service(&JOBS) {
                for line in jobs.log_from(&job.id, c.fed) {
                    c.console.line(&line);
                }
            }
            c.fed = job.log_total;
        }
        c.terminal.clone()
    }
}

/// The plugin: the jobs centre's log consoles as pseudo-terminals.
#[derive(Clone, Copy, Debug, Default)]
pub struct JobConsolesPlugin;

impl Plugin for JobConsolesPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("egui_pop.job_consoles")
    }

    fn title(&self) -> String {
        "Job consoles".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let consoles = Consoles::default();
        ctx.service(
            &JOB_LOG_VIEW,
            JobLogView::new(move |ui: &mut Ui, app: &AppHandle, job: &Job| {
                let terminal = consoles.feed(app, job);
                let label = egui_pop_core::strings::tr_args(
                    ui.ctx(),
                    &crate::strings::JOB_LOG,
                    &[("job", &job.title)],
                );
                let mut view = Terminal::new().label(label).height(LOG_HEIGHT);
                if app.service(&TERMINALS).is_some() {
                    view = view.app(app);
                }
                view.show(ui, &terminal);
            }),
        )?;
        Ok(())
    }
}
