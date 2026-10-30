//! The sections page: a pretend side panel stacking three collapsible
//! sections (live, idle and shut sessions), each with its count, its
//! least height and its own scrolling rows; bands fold and open, the
//! line between two open sections resizes them, and a double click on
//! it drops the heights set by hand. Beside it, a stack already pushed
//! (one section set tall by hand, the next one down to its least) and
//! a panel too short for its least heights, which scrolls as a whole,
//! and a stack of a long windowed section over a list that scrolls
//! itself. A last stack has titles that drag and stay where they are when
//! folded, Ctrl held in a drag pulling the sections below as a block (the
//! pointer profile's `gesture.sections.pull`). The playground changes the
//! counts, the capitals and those two options, and forgets the layouts.

use egui::{Stroke, Ui, vec2};
use egui_pop::core::sections::{self, Layout, Section, Sections};
use egui_pop::core::ui::UiPop;
use egui_pop::core::widgets;
use egui_pop::theme::Tokens;
use egui_pop::views::list::{ListRows, ListState, ListView};

/// The stack's name: its memory, and what agents and tests read.
pub const STACK: &str = "gallery.sessions";

/// The stack already pushed.
pub const PUSHED: &str = "gallery.pushed";

/// The stack in a panel too short for it.
pub const SHORT: &str = "gallery.short";

/// The stack of a windowed section and a list.
pub const WINDOWED: &str = "gallery.windowed";

/// The stack whose titles drag and keep their place.
pub const GRABBED: &str = "gallery.grabbed";

/// The pretend panel's size.
const PANEL: egui::Vec2 = vec2(150.0, 420.0);

/// The pushed stack's panel: as tall, narrower (four columns share the page).
const PUSHED_PANEL: egui::Vec2 = vec2(140.0, 420.0);

/// The short panel: room for the bands and a little more.
const SHORT_PANEL: egui::Vec2 = vec2(140.0, 150.0);

/// The windowed stack's panel: under the short one.
const WINDOWED_PANEL: egui::Vec2 = vec2(140.0, 244.0);

/// The stack whose titles drag: narrow, as tall as the others.
const GRABBED_PANEL: egui::Vec2 = vec2(140.0, 420.0);

/// The pushed stack's layout: Inbox set tall, Drafts given way down to
/// its least height, Sent taking what is left.
fn pushed_layout() -> Layout {
    let mut layout = Layout::default();
    layout.set_height("inbox", Some(180.0));
    layout.set_height("drafts", Some(0.0));
    layout
}

/// The rows of the list in a bare section.
#[derive(Debug)]
struct Folders;

impl ListRows<usize> for Folders {
    fn len(&self) -> usize {
        40
    }

    fn key(&self, row: usize) -> usize {
        row
    }

    fn label(&self, row: usize) -> String {
        format!("Folder {}", row + 1)
    }
}

/// The page's state and the playground's options.
#[derive(Debug)]
pub struct State {
    pub live: u64,
    pub shut: u64,
    /// The windowed section's rows.
    pub lines: u64,
    pub capitals: bool,
    /// The last stack's titles drag.
    pub grab_titles: bool,
    /// The last stack's titles stay where they are when folded.
    pub keep_titles: bool,
    /// The list in the bare section.
    folders: ListState<usize>,
    /// What the user last did to the stack.
    pub last: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            live: 3,
            shut: 12,
            lines: 5_000,
            capitals: true,
            grab_titles: true,
            keep_titles: true,
            folders: ListState::new(),
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
    let l = ui.label("Log lines");
    ui.add(egui::DragValue::new(&mut state.lines).range(0..=1_000_000))
        .labelled_by(l.id);
    widgets::checkbox(ui, &mut state.capitals, "Titles in capitals");
    widgets::checkbox(ui, &mut state.grab_titles, "Titles drag");
    widgets::checkbox(ui, &mut state.keep_titles, "Titles stay when folded");
    if ui.button("Forget the layout").clicked() {
        Layout::default().store(ui.ctx(), STACK);
        pushed_layout().store(ui.ctx(), PUSHED);
        Layout::default().store(ui.ctx(), SHORT);
        Layout::default().store(ui.ctx(), WINDOWED);
        Layout::default().store(ui.ctx(), GRABBED);
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

/// A pretend panel of a fixed size, with a caption over it.
fn panel<R>(ui: &mut Ui, caption: &str, size: egui::Vec2, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.vertical(|ui| {
        ui.meta(caption);
        let border = Tokens::of(ui.ctx()).border.default;
        ui.allocate_ui(size, |ui| {
            egui::Frame::NONE
                .stroke(Stroke::new(1.0, border))
                .show(ui, |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    add(ui)
                })
                .inner
        })
        .inner
    })
    .inner
}

pub fn show(ui: &mut Ui, state: &mut State) {
    ui.label("Sections stacked in a panel: a click on a band (or Space and Enter, Left and Right) folds or opens it. The line between two open sections resizes them, with the pointer or Up and Down: the sections give way in turn, the nearest first, each down to its least height. A double click on that line (or Enter) drops the heights set by hand. With Ctrl held, a drag moves the sections below as a block, opening empty room under the last. In the last panel a title drags too, stays where it is when folded, and a double click on it gets the shares back. Each section scrolls its own rows, a wheel stopping at their end; the layout is remembered.");
    ui.add_space(Tokens::of(ui.ctx()).space.group);
    if Layout::load(ui.ctx(), PUSHED).is_none() {
        pushed_layout().store(ui.ctx(), PUSHED);
    }
    let (live, shut, lines, capitals) = (state.live, state.shut, state.lines, state.capitals);
    let (grab, keep) = (state.grab_titles, state.keep_titles);
    let folders = &mut state.folders;
    let response = ui
        .horizontal_top(|ui| {
            let response = panel(ui, "Sized by the list", PANEL, |ui| {
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
            });
            panel(ui, "Pushed by hand", PUSHED_PANEL, |ui| {
                Sections::new(PUSHED).capitals(capitals).show(ui, |stack| {
                    stack.section(Section::new("inbox", "Inbox").count(14), |ui| {
                        rows(ui, "Message", 14, "");
                    });
                    stack.section(Section::new("drafts", "Drafts").count(5), |ui| {
                        rows(ui, "Draft", 5, "");
                    });
                    stack.section(Section::new("sent", "Sent").count(6), |ui| {
                        rows(ui, "Sent", 6, "");
                    });
                });
            });
            ui.vertical(|ui| {
                panel(ui, "Too short: it scrolls", SHORT_PANEL, |ui| {
                    Sections::new(SHORT).capitals(capitals).show(ui, |stack| {
                        stack.section(Section::new("open", "Open files").count(3), |ui| {
                            rows(ui, "File", 3, "");
                        });
                        stack.section(Section::new("outline", "Outline").count(4), |ui| {
                            rows(ui, "Symbol", 4, "");
                        });
                        stack.section(Section::new("timeline", "Timeline").count(2), |ui| {
                            rows(ui, "Change", 2, "");
                        });
                    });
                });
                panel(ui, "Windowed, and a list", WINDOWED_PANEL, |ui| {
                    let height = sections::row_height(ui);
                    Sections::new(WINDOWED)
                        .capitals(capitals)
                        .show(ui, |stack| {
                            stack.rows(
                                Section::new("log", "Log").count(lines),
                                lines as usize,
                                move |ui, range| {
                                    let width = ui.available_width();
                                    for i in range {
                                        ui.allocate_ui_with_layout(
                                            vec2(width, height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| ui.label(format!("Line {}", i + 1)),
                                        );
                                    }
                                },
                            );
                            stack.bare(Section::new("folders", "Folders").count(40), |ui| {
                                ListView::new("gallery.sections.folders")
                                    .label("Folders")
                                    .show(ui, folders, &mut Folders);
                            });
                        });
                });
            });
            panel(ui, "Titles drag and stay", GRABBED_PANEL, |ui| {
                Sections::new(GRABBED)
                    .capitals(capitals)
                    .grab_titles(grab)
                    .keep_titles(keep)
                    .show(ui, |stack| {
                        stack.section(Section::new("tickets", "Tickets").count(7), |ui| {
                            rows(ui, "Ticket", 7, "");
                        });
                        stack.section(Section::new("reviews", "Reviews").count(4), |ui| {
                            rows(ui, "Review", 4, "");
                        });
                        stack.section(Section::new("builds", "Builds").count(3), |ui| {
                            rows(ui, "Build", 3, "");
                        });
                    });
            });
            response
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
