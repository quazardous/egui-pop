//! The sections page: a pretend side panel stacking three collapsible
//! sections (live, idle and shut sessions), each with its count, its
//! least height and its own scrolling rows; bands fold and open, the
//! line between two open sections resizes them, and a double click on
//! it gives the heights back. The playground changes the counts and
//! the capitals, and forgets the layout.

use egui::{Stroke, Ui, vec2};
use egui_pop::core::sections::{Layout, Section, Sections};
use egui_pop::core::ui::UiPop;
use egui_pop::core::widgets;
use egui_pop::theme::Tokens;

/// The stack's name: its memory, and what agents and tests read.
pub const STACK: &str = "gallery.sessions";

/// The pretend panel's size.
const PANEL: egui::Vec2 = vec2(300.0, 420.0);

/// The page's state and the playground's options.
#[derive(Clone, Debug)]
pub struct State {
    pub live: u64,
    pub shut: u64,
    pub capitals: bool,
    /// What the user last did to the stack.
    pub last: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            live: 3,
            shut: 12,
            capitals: true,
            last: None,
        }
    }
}

pub fn playground(ui: &mut Ui, state: &mut State) {
    let l = ui.label("Live sessions");
    ui.add(egui::DragValue::new(&mut state.live).range(0..=200))
        .labelled_by(l.id);
    let l = ui.label("Shut sessions");
    ui.add(egui::DragValue::new(&mut state.shut).range(0..=200))
        .labelled_by(l.id);
    widgets::checkbox(ui, &mut state.capitals, "Titles in capitals");
    if ui.button("Forget the layout").clicked() {
        Layout::default().store(ui.ctx(), STACK);
        state.last = Some("Layout forgotten".into());
    }
}

fn rows(ui: &mut Ui, prefix: &str, n: u64, empty: &str) {
    if n == 0 {
        ui.muted(empty);
    }
    for i in 1..=n {
        ui.label(format!("{prefix} {i}"));
    }
}

pub fn show(ui: &mut Ui, state: &mut State) {
    ui.label("Sections stacked in a panel: a click on a band (or Space and Enter) folds or opens it, the line between two open sections resizes them, a double click on that line gives the heights back. Each section scrolls its own rows and keeps two rows' height at least; the layout is remembered.");
    ui.add_space(Tokens::of(ui.ctx()).space.group);
    let border = Tokens::of(ui.ctx()).border.default;
    let (live, shut, capitals) = (state.live, state.shut, state.capitals);
    let response = ui
        .allocate_ui(PANEL, |ui| {
            egui::Frame::NONE
                .stroke(Stroke::new(1.0, border))
                .show(ui, |ui| {
                    ui.set_min_size(PANEL);
                    ui.set_max_size(PANEL);
                    Sections::new(STACK).capitals(capitals).show(ui, |stack| {
                        stack.section(Section::new("live", "Live").count(live), move |ui| {
                            rows(ui, "Session", live, "No live session");
                        });
                        stack.section(Section::new("idle", "Idle").count(0), |ui| {
                            rows(ui, "Idle", 0, "No idle session");
                        });
                        stack.section(
                            Section::new("shut", "Shut").count(shut).open(false),
                            move |ui| rows(ui, "Closed", shut, "No shut session"),
                        );
                    })
                })
                .inner
        })
        .inner;
    if let Some((key, open)) = &response.toggled {
        state.last = Some(format!("{} {key}", if *open { "Opened" } else { "Folded" }));
    } else if response.reset {
        state.last = Some("Heights given back".into());
    } else if response.resized {
        state.last = Some("Resized".into());
    }
    ui.meta(state.last.as_deref().unwrap_or("Nothing done yet."));
}
