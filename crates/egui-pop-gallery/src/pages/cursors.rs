//! The cursors page: egui-pop's tool cursors, a viewport whose tool is
//! picked in the playground (a brush's outline follows the zoom), its
//! loupe, and the `cursor.resolve` step: the page's plugin shows a
//! not-allowed cursor for the brushes outside the image.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Key, Ui, vec2};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::imaging::{self, STEP_CURSOR};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::viewport::cursor::{self, Brush, Tool, ToolCursor};
use egui_pop::viewport::demo::Procedural;
use egui_pop::viewport::loupe::Values;
use egui_pop::viewport::{View, Viewport, ViewportState};
use serde_json::json;

use super::section;
use crate::shell::SHELL;

pub const CURSORS: PluginId = PluginId::from_static("gallery.cursors");

/// The demo image's size, in pixels.
pub const IMAGE: [u32; 2] = [2048, 1536];

/// The tools the playground offers, by name.
pub const TOOLS: [&str; 10] = [
    "Zoom",
    "Pan",
    "Picker",
    "Crosshair",
    "Move",
    "Rotate",
    "Brush",
    "Eraser",
    "Mask",
    "Disabled",
];

/// The page's state.
pub struct State {
    pub view: View,
    pub state: ViewportState,
    source: Option<Procedural>,
    /// An index in [`TOOLS`].
    pub tool: usize,
    /// The brushes' radius, in image pixels.
    pub radius: f64,
    pub hardness: f32,
}

impl Default for State {
    fn default() -> Self {
        let mut state = ViewportState::new();
        let loupe = state.loupe_mut();
        loupe.on = true;
        loupe.key = Some(Key::L);
        State {
            view: View::at(2.0, [1024.0, 768.0]),
            state,
            source: None,
            tool: 6,
            radius: 24.0,
            hardness: 0.6,
        }
    }
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("view", &self.view)
            .field("tool", &TOOLS[self.tool])
            .finish()
    }
}

impl State {
    /// The tool picked.
    pub fn tool(&self) -> Tool {
        let brush = Brush::new(self.radius, self.hardness);
        match TOOLS[self.tool] {
            "Zoom" => Tool::Zoom,
            "Pan" => Tool::Pan,
            "Picker" => Tool::Picker,
            "Crosshair" => Tool::Crosshair,
            "Move" => Tool::Move,
            "Rotate" => Tool::Rotate,
            "Brush" => Tool::Brush(brush),
            "Eraser" => Tool::Eraser(brush),
            "Mask" => Tool::Mask(brush),
            _ => Tool::Disabled,
        }
    }
}

/// The page.
#[derive(Debug, Default)]
pub struct CursorsPage;

impl Plugin for CursorsPage {
    fn id(&self) -> PluginId {
        CURSORS
    }

    fn title(&self) -> String {
        "Cursors and loupe".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, imaging::IMAGING_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        // The brushes cannot paint outside the image.
        ctx.replace(STEP_CURSOR, |_, args| {
            let q = &args["query"];
            let brush = matches!(q["tool"].as_str(), Some("brush" | "eraser" | "mask"));
            if brush && q["over_image"] == false {
                Ok(json!({ "tool": "not-allowed" }))
            } else {
                Ok(args["default"].clone())
            }
        });
        let state = Rc::new(RefCell::new(State::default()));
        let s = state.clone();
        ctx.story(
            Story::new(StoryId::from_static("cursors"), "Cursors and loupe", move |ui, app| {
                show(ui, app, &mut state.borrow_mut())
            })
            .description("Tool cursors drawn at the window's scale in the theme's colours, a brush outline that follows the zoom, the loupe on the displayed pixels; each cursor goes through the cursor.resolve step.")
            .playground(move |ui, _| playground(ui, &mut s.borrow_mut()))
            .group("Components", 77),
        )?;
        Ok(())
    }
}

/// The playground: the tool, the brush, the loupe, the cursor settings.
pub fn playground(ui: &mut Ui, s: &mut State) {
    ui.label("Tool");
    ui.horizontal_wrapped(|ui| {
        for (i, name) in TOOLS.iter().enumerate() {
            ui.selectable_value(&mut s.tool, i, *name);
        }
    });
    ui.add(egui::Slider::new(&mut s.radius, 1.0..=200.0).text("Brush radius"));
    ui.add(egui::Slider::new(&mut s.hardness, 0.0..=1.0).text("Brush hardness"));
    let loupe = s.state.loupe_mut();
    ui.checkbox(&mut loupe.on, "Loupe (L)");
    ui.add(egui::Slider::new(&mut loupe.magnification, 4..=16).text("Magnification"));
    ui.checkbox(&mut loupe.grid, "Loupe grid");
    ui.checkbox(&mut loupe.marker, "Loupe centre marker");
    ui.horizontal(|ui| {
        for v in [Values::Display, Values::Scene, Values::Off] {
            ui.selectable_value(&mut loupe.values, v, v.name());
        }
    });
    let mut settings = cursor::settings(ui.ctx());
    let before = settings;
    ui.checkbox(&mut settings.painted, "Painted cursors");
    let mut size = settings.size.unwrap_or_else(cursor::system_size);
    if ui
        .add(egui::Slider::new(&mut size, 16.0..=64.0).text("Cursor size"))
        .changed()
    {
        settings.size = Some(size);
    }
    if settings != before {
        cursor::set_settings(ui.ctx(), settings);
        s.state.refresh_cursor();
    }
}

/// The page.
pub fn show(ui: &mut Ui, app: &AppHandle, s: &mut State) {
    ui.label("Each tool shows its cursor: egui-pop's tool cursors are drawn at the cursor size and the window's scale and handed to the system; a brush shows its outline, sized in image pixels, with its hardness ring. Alt turns zoom in into zoom out and a mask brush's plus into a minus. L turns the loupe on and off. The page's plugin shows a not-allowed cursor for the brushes outside the image.");
    section(ui, "Tool cursors");
    ui.horizontal_wrapped(|ui| {
        for t in ToolCursor::ALL {
            ui.allocate_ui(vec2(76.0, 56.0), |ui| {
                ui.vertical_centered(|ui| {
                    let (_, rect) = ui.allocate_space(vec2(32.0, 32.0));
                    cursor::paint_sample(ui, rect, *t);
                    ui.small(t.name());
                });
            });
        }
    });
    section(ui, "On an image");
    if s.source.is_none() {
        // Computed when asked, on the interface thread: the page settles
        // without waiting on threads.
        s.source = Some(Procedural::new(s.state.sink(), IMAGE, 0));
    }
    let tool = s.tool();
    let size = vec2(
        ui.available_width(),
        (ui.available_height() - 8.0).clamp(220.0, 420.0),
    );
    let mut vp = Viewport::new("cursors-demo")
        .size(size)
        .readout(true)
        .tool(tool)
        .on_cursor(imaging::cursor(app, "gallery"));
    if let Some(src) = s.source.as_ref() {
        vp = vp.source(src);
    }
    vp.show(ui, &mut s.state, &mut s.view);
}
