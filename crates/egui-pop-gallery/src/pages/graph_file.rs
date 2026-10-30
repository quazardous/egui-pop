//! The graph file page: the node graph editor over the reference model
//! kept in a YAML file (`graph::yaml::YamlGraph`), the file's text beside
//! it. Each edit runs its `graph.*` step, then lands in the text where
//! it applies; the rest of the file, comments included, stays as written.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{RichText, Ui, vec2};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::imaging;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::viewport::graph::yaml::YamlGraph;
use egui_pop::viewport::graph::{GraphEditor, GraphModel, GraphState, Request};

use crate::shell::SHELL;

pub const GRAPH_FILE: PluginId = PluginId::from_static("gallery.graph-file");

/// The name the editor goes by in steps and commands.
pub const GRAPH: &str = "file";

/// The file the page starts from.
pub const SOURCE: &str = include_str!("graph_file.yaml");

/// The page's state.
#[derive(Debug)]
pub struct State {
    pub graph: YamlGraph,
    pub view: GraphState,
    /// The edits applied since the file was read.
    pub edits: usize,
}

impl Default for State {
    fn default() -> Self {
        State {
            graph: YamlGraph::parse(SOURCE).expect("the gallery's graph file reads"),
            view: GraphState::new(),
            edits: 0,
        }
    }
}

/// The page.
#[derive(Debug, Default)]
pub struct GraphFilePage;

impl Plugin for GraphFilePage {
    fn id(&self) -> PluginId {
        GRAPH_FILE
    }

    fn title(&self) -> String {
        "Graph from a file".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, imaging::IMAGING_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let state = Rc::new(RefCell::new(State::default()));
        let play = state.clone();
        ctx.story(
            Story::new(StoryId::from_static("graph-file"), "Graph from a file", move |ui, app| {
                show(ui, app, &mut state.borrow_mut())
            })
            .description("The node graph editor over a graph kept in a YAML file: each edit lands in the file's text where it applies, its comments and key order kept.")
            .group("Components", 79)
            .playground(move |ui, _| playground(ui, &mut play.borrow_mut())),
        )?;
        Ok(())
    }
}

/// The page.
pub fn show(ui: &mut Ui, app: &AppHandle, s: &mut State) {
    ui.label("Move, wire, delete or add nodes, or drag a parameter in the detail view: the file on the right changes only where the edit lands.");
    let width = ui.available_width().max(480.0);
    let height = (ui.available_height() - 8.0).max(120.0);
    ui.horizontal_top(|ui| {
        let editor = GraphEditor::new(&s.graph, &mut s.view)
            .label("Graph file")
            .size(vec2(width * 0.58, height))
            .snap(Some(10.0));
        let out = imaging::show_graph(app, GRAPH, editor, ui);
        for e in &out.edits {
            if s.graph.apply(e) {
                s.edits += 1;
            }
        }
        ui.vertical(|ui| {
            ui.label(format!(
                "{} nodes, {} wires, format {}, {} edits",
                s.graph.nodes().len(),
                s.graph.edges().len(),
                s.graph.version(),
                s.edits
            ));
            egui::ScrollArea::both()
                .id_salt("graph-file-text")
                .max_height(height - 24.0)
                .show(ui, |ui| {
                    ui.label(RichText::new(s.graph.text()).monospace());
                });
        });
    });
}

/// The playground: the view, and the file read again.
fn playground(ui: &mut Ui, s: &mut State) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Fit").clicked() {
            s.view.request(Request::Fit);
        }
        if ui.button("Read the file again").clicked() {
            *s = State::default();
        }
    });
}
