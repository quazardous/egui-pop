//! The preview dialog page: an image viewport behind, and a button
//! opening an export preview, a custom dialog holding a second viewport
//! over a clear backdrop (ADR 0013). The viewport behind keeps drawing
//! while the dialog is modal; Escape cancels the preview, except during
//! a drag of its view; Enter exports, which the page shows below.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Ui, vec2};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::imaging::preview::{self, Preview};
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{CommandId, PluginId, StoryId};
use egui_pop::viewport::demo::Procedural;
use egui_pop::viewport::{View, Viewport, ViewportState};
use serde_json::json;

use super::section;
use crate::shell::SHELL;

pub const PREVIEW_PAGE: PluginId = PluginId::from_static("gallery.preview");
/// What the preview's primary button runs: keeps its arguments for the
/// page.
pub const CMD_EXPORT: CommandId = CommandId::from_static("gallery.preview.export");

/// The size of the demo image, in pixels.
const IMAGE: [u32; 2] = [3000, 2000];

/// The page's viewport, and why the preview did not open.
#[derive(Debug, Default)]
struct PageState {
    view: View,
    state: ViewportState,
    source: Option<Procedural>,
    error: Option<String>,
}

/// The preview dialog page.
#[derive(Debug, Default)]
pub struct PreviewPage;

impl Plugin for PreviewPage {
    fn id(&self) -> PluginId {
        PREVIEW_PAGE
    }

    fn title(&self) -> String {
        "Preview dialog".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.command(
            CommandSpec::new(CMD_EXPORT, "Export from the preview", "Preview")
                .hidden()
                .no_agent(),
            |call| {
                call.app
                    .set_context("galleryPreviewSent", &call.args.to_string());
                Ok(json!(null))
            },
        )?;
        let page = Rc::new(RefCell::new(PageState::default()));
        ctx.story(
            Story::new(StoryId::from_static("preview"), "Preview dialog", move |ui, app| {
                show(ui, app, &page);
            })
            .description("A custom dialog holding an image viewport over a clear backdrop: the interface behind keeps drawing while it is modal.")
            .group("Components", 78),
        )?;
        Ok(())
    }
}

/// Workers natively; computed on the interface thread on the web. The
/// page computes its tiles when asked, so its snapshot is the same on
/// every run.
fn source(sink: egui_pop::viewport::source::TileSink, shift: f32) -> Procedural {
    let s = Procedural::new(sink, IMAGE, 0);
    s.set_shift(shift);
    s
}

fn show(ui: &mut Ui, app: &AppHandle, page: &Rc<RefCell<PageState>>) {
    let mut p = page.borrow_mut();
    ui.label("The button opens an export preview: a custom dialog with an image viewport of its own, over a clear backdrop. Enter exports, Escape cancels (during a drag of the view, it cancels the drag).");
    if ui.button("Preview the export").clicked() {
        let opened = preview::open(
            app,
            Preview::new("Export preview", "Export", CMD_EXPORT)
                .args(json!({ "format": "png" }))
                .remember("gallery.preview"),
            |sink| Box::new(source(sink, 0.4)),
        );
        p.error = opened.err();
    }
    if let Some(e) = &p.error {
        ui.colored_label(egui_pop::theme::Tokens::of(ui.ctx()).status.danger, e);
    }
    section(ui, "The image behind");
    let PageState {
        view,
        state,
        source: main,
        ..
    } = &mut *p;
    if main.is_none() {
        *main = Some(source(state.sink(), 0.0));
    }
    let height = ui.available_height().clamp(120.0, 320.0);
    let _ = Viewport::new("gallery.preview.main")
        .source(main.as_ref().expect("made above"))
        .label("Image behind the preview")
        .size(vec2(ui.available_width(), height))
        .show(ui, state, view);
    section(ui, "Sent by the last export");
    let sent = app.context().get("galleryPreviewSent").to_string();
    ui.monospace(if sent.is_empty() {
        "(nothing yet)".into()
    } else {
        sent
    });
}
