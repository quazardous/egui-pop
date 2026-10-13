//! The zoom and navigator page: a viewport over the procedural demo
//! image, the zoom control above it and the navigator beside it, all
//! three on one viewport state. Their changes run the `zoom.change`
//! step; the page's plugin keeps the zoom at 400 % at most.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Color32, ColorImage, TextureHandle, TextureOptions, Ui, vec2};
use egui_pop::api::steps::Before;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::imaging::{self, STEP_ZOOM};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::viewport::demo::{self, Procedural};
use egui_pop::viewport::navigator::{Navigator, ZoomControl};
use egui_pop::viewport::{View, Viewport, ViewportState};
use serde_json::json;

use crate::shell::SHELL;

pub const NAVIGATOR: PluginId = PluginId::from_static("gallery.navigator");

/// The demo image's size, in pixels.
pub const IMAGE: [u32; 2] = [4096, 3072];

/// The largest zoom the page's before-hook lets through.
pub const MAX_ZOOM: f64 = 4.0;

/// The page's state.
#[derive(Default)]
pub struct State {
    pub view: View,
    pub state: ViewportState,
    source: Option<Procedural>,
    proxy: Option<TextureHandle>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State").field("view", &self.view).finish()
    }
}

/// The page.
#[derive(Debug, Default)]
pub struct NavigatorPage;

impl Plugin for NavigatorPage {
    fn id(&self) -> PluginId {
        NAVIGATOR
    }

    fn title(&self) -> String {
        "Zoom and navigator".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, imaging::IMAGING_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_ZOOM, |_, args| match args["to"]["zoom"].as_f64() {
            Some(z) if z > MAX_ZOOM => {
                let mut args = args.clone();
                args["to"]["zoom"] = json!(MAX_ZOOM);
                Before::Redirect(args)
            }
            _ => Before::Go,
        });
        let state = Rc::new(RefCell::new(State::default()));
        ctx.story(
            Story::new(StoryId::from_static("navigator"), "Zoom and navigator", move |ui, app| {
                show(ui, app, &mut state.borrow_mut())
            })
            .description("A zoom control and a navigator sharing a viewport's state: the zoom ladder, fit and 100 %, the part in view framed on the whole image; each change the zoom.change step.")
            .group("Components", 72),
        )?;
        Ok(())
    }
}

/// The whole demo image, small: the navigator's proxy.
fn proxy() -> ColorImage {
    let size = [192usize, 144];
    let mut px = Vec::with_capacity(size[0] * size[1]);
    for j in 0..size[1] {
        for i in 0..size[0] {
            let x = (i as f64 + 0.5) / size[0] as f64 * f64::from(IMAGE[0]);
            let y = (j as f64 + 0.5) / size[1] as f64 * f64::from(IMAGE[1]);
            let c = demo::sample(x, y, IMAGE, 0.0);
            let e = |v: f32| egui::ecolor::gamma_u8_from_linear_f32(v.clamp(0.0, 1.0));
            px.push(Color32::from_rgb(e(c[0]), e(c[1]), e(c[2])));
        }
    }
    ColorImage::new(size, px)
}

/// The page.
pub fn show(ui: &mut Ui, app: &AppHandle, s: &mut State) {
    ui.label("The zoom control and the navigator share the viewport's state. In the navigator: drag the frame to pan, press elsewhere to go there, the wheel zooms, a double click fits; focused, the arrows pan and + - 0 zoom. The page's plugin keeps the zoom at 400 % at most.");
    if s.source.is_none() {
        // Computed when asked, on the interface thread: the page settles
        // without waiting on threads.
        s.source = Some(Procedural::new(s.state.sink(), IMAGE, 0));
    }
    let proxy = s
        .proxy
        .get_or_insert_with(|| {
            ui.ctx()
                .load_texture("gallery.navigator.proxy", proxy(), TextureOptions::LINEAR)
        })
        .id();
    ZoomControl::new(&mut s.state, &mut s.view)
        .on_change(imaging::zoom(app, "gallery"))
        .show(ui);
    let side = 240.0;
    let width = (ui.available_width() - side - 16.0).max(200.0);
    let height = (ui.available_height() - 40.0).clamp(200.0, 420.0);
    ui.horizontal_top(|ui| {
        let mut vp = Viewport::new("navigator-demo").size(vec2(width, height));
        if let Some(src) = s.source.as_ref() {
            vp = vp.source(src);
        }
        vp.show(ui, &mut s.state, &mut s.view);
        Navigator::new(&mut s.state, &mut s.view)
            .texture(proxy)
            .size(vec2(side, side * 0.75))
            .on_change(imaging::zoom(app, "gallery"))
            .show(ui);
    });
}
