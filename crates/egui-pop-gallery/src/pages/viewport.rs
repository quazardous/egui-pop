//! The viewport page: the generic image viewport, its zoom ladder and
//! fit, pan, the signals it emits and the hooks an application sets.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Ui, vec2};
use egui_pop::api::stories::Story;
use egui_pop::api::{Ctx, Plugin, PluginError};
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::viewport::hooks::Verdict;
use egui_pop::viewport::view::Zoom;
use egui_pop::viewport::{Signal, View, Viewport, ViewportState};

use super::section;
use crate::shell::SHELL;

pub const VIEWPORT: PluginId = PluginId::from_static("gallery.viewport");

/// The size of the demo image, in pixels.
pub const IMAGE: [u32; 2] = [4096, 3072];

/// The page's state: the view, the widget's state, the last signals.
#[derive(Debug, Default)]
pub struct PageState {
    pub view: View,
    pub state: ViewportState,
    pub log: Vec<String>,
    pub hover: Option<[i64; 2]>,
    /// The playground's "at most 800 %" hook.
    pub clamp: bool,
}

/// The viewport page.
#[derive(Debug, Default)]
pub struct ViewportPage;

impl Plugin for ViewportPage {
    fn id(&self) -> PluginId {
        VIEWPORT
    }

    fn title(&self) -> String {
        "Viewport".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let page = Rc::new(RefCell::new(PageState::default()));
        let p = page.clone();
        ctx.collector(egui_pop::viewport::debug::collector())?;
        ctx.story(
            Story::new(StoryId::from_static("viewport"), "Viewport", move |ui, _| {
                show(ui, &mut p.borrow_mut())
            })
            .description("A generic image viewport: zoom in physical pixels (100 % is one image pixel per screen pixel), the zoom ladder, fit, pan, signals and hooks.")
            .group("Components", 70)
            .playground(move |ui, _| playground(ui, &mut page.borrow_mut())),
        )?;
        Ok(())
    }
}

/// The page, a plain function over its state.
pub fn show(ui: &mut Ui, page: &mut PageState) {
    ui.label("Drag or Space-drag to pan (Escape puts the view back), the wheel zooms around the pointer and stops at 100 %, a double click fits; when focused: + and - walk the ladder, 0 fits, 1 is 100 %, 2 is 200 %, the arrows pan.");
    ui.horizontal(|ui| {
        if ui.button("Fit").clicked() {
            page.state.propose(&mut page.view, View::fit());
        }
        if ui.button("100 %").clicked()
            && let Some(m) = page.state.mapping()
        {
            page.state
                .propose(&mut page.view, m.zoom_around(m.rect.center(), 1.0));
        }
        if ui.button("Zoom out").clicked() {
            page.state.zoom_out(&mut page.view);
        }
        if ui.button("Zoom in").clicked() {
            page.state.zoom_in(&mut page.view);
        }
        let zoom = page.state.mapping().map_or(1.0, |m| m.zoom);
        let fit = if page.view.zoom == Zoom::Fit {
            " (fit)"
        } else {
            ""
        };
        ui.label(format!("{:.1} %{fit}", zoom * 100.0));
    });
    let height = (ui.available_height() - 90.0).clamp(240.0, 520.0);
    let out = Viewport::new("gallery")
        .image_size(IMAGE)
        .size(vec2(ui.available_width(), height))
        .show(ui, &mut page.state, &mut page.view);
    page.hover = out.hover_pixel;
    for s in &out.signals {
        let Signal::View { view, phase, cause } = s;
        page.log.push(format!(
            "{} {}: {}",
            cause.name(),
            phase.name(),
            match view.zoom {
                Zoom::Fit => "fit".to_string(),
                Zoom::Px(z) => format!(
                    "{:.1} % at ({:.0}, {:.0})",
                    z * 100.0,
                    view.center[0],
                    view.center[1]
                ),
            }
        ));
    }
    let keep = page.log.len().saturating_sub(3);
    page.log.drain(..keep);
    ui.horizontal(|ui| {
        ui.label(match page.hover {
            Some([x, y]) => format!("Pixel ({x}, {y})"),
            None => "Pixel: outside the image".to_string(),
        });
    });
    section(ui, "Last signals");
    for l in &page.log {
        ui.weak(l);
    }
}

/// The playground: a hook.
pub fn playground(ui: &mut Ui, page: &mut PageState) {
    if ui
        .checkbox(&mut page.clamp, "Hook: never above 800 %")
        .changed()
    {
        page.state.hooks_mut().before_view = if page.clamp {
            Some(Box::new(|change, _| match change.to.zoom {
                Zoom::Px(z) if z > 8.0 => Verdict::Redirect(View::at(8.0, change.to.center)),
                _ => Verdict::Allow,
            }))
        } else {
            None
        };
    }
}
