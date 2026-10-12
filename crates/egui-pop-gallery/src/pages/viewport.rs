//! The viewport page: the generic image viewport over a procedural
//! image computed on worker threads, its zoom ladder and fit, pan, the
//! tiles (proxy then exact), the display transform, the signals it emits
//! and the hooks an application sets.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Ui, vec2};
use egui_pop::api::stories::Story;
use egui_pop::api::{Ctx, Plugin, PluginError};
use egui_pop::core::debug::FnCollector;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::viewport::demo::Procedural;
use egui_pop::viewport::hooks::Verdict;
use egui_pop::viewport::view::Zoom;
use egui_pop::viewport::{Signal, View, Viewport, ViewportState};
use serde_json::json;

use super::section;
use crate::shell::SHELL;

pub const VIEWPORT: PluginId = PluginId::from_static("gallery.viewport");

/// The size of the demo image, in pixels.
pub const IMAGE: [u32; 2] = [4096, 3072];

/// The page's state: the view, the widget's state, the source, the last
/// signals.
#[derive(Debug, Default)]
pub struct PageState {
    pub view: View,
    pub state: ViewportState,
    pub source: Option<Procedural>,
    pub log: Vec<String>,
    pub hover: Option<[i64; 2]>,
    /// The playground's "at most 800 %" hook.
    pub clamp: bool,
    /// The playground's hue shift.
    pub shift: f32,
}

impl PageState {
    /// The source, started on first use.
    pub fn source(&mut self) -> &Procedural {
        let sink = self.state.sink();
        // Worker threads natively; computed on the interface thread on
        // the web, which has none.
        let workers = if cfg!(target_arch = "wasm32") { 0 } else { 2 };
        self.source
            .get_or_insert_with(|| Procedural::new(sink, IMAGE, workers))
    }
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
        ctx.collector(egui_pop::viewport::debug::collector())?;
        let p = page.clone();
        ctx.collector(
            FnCollector::new("gallery.viewport", "The viewport page's demo source.")
                .snapshot(move |_| {
                    let p = p.borrow();
                    json!({
                        "started": p.source.is_some(),
                        "complete": p.state.tiles_complete(),
                        "outstanding": p.state.outstanding(),
                    })
                })
                .action(
                    "wait_idle",
                    "Blocks until the demo source answered every request it took.",
                    false,
                    {
                        let p = page.clone();
                        move |_, _| {
                            if let Some(s) = p.borrow().source.as_ref() {
                                s.wait_idle();
                            }
                            Ok(json!(null))
                        }
                    },
                ),
        )?;
        let p = page.clone();
        ctx.story(
            Story::new(StoryId::from_static("viewport"), "Viewport", move |ui, _| {
                show(ui, &mut p.borrow_mut())
            })
            .description("A generic image viewport: zoom in physical pixels (100 % is one image pixel per screen pixel), the zoom ladder, fit, pan, tiles from an asynchronous source, signals and hooks.")
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
    let height = (ui.available_height() - 150.0).clamp(200.0, 520.0);
    page.source();
    let PageState {
        view,
        state,
        source,
        ..
    } = page;
    let mut vp = Viewport::new("gallery").size(vec2(ui.available_width(), height));
    if let Some(s) = source.as_ref() {
        vp = vp.source(s);
    }
    let out = vp.show(ui, state, view);
    page.hover = out.hover_pixel;
    for s in &out.signals {
        page.log.push(match s {
            Signal::View { view, phase, cause } => format!(
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
            ),
            other => format!("{other:?}"),
        });
    }
    let keep = page.log.len().saturating_sub(3);
    page.log.drain(..keep);
    ui.horizontal(|ui| {
        ui.label(match page.hover {
            Some([x, y]) => format!("Pixel ({x}, {y})"),
            None => "Pixel: outside the image".to_string(),
        });
        ui.separator();
        let st = &page.state;
        ui.label(format!(
            "Tiles: {} held ({:.1} MB), {} waiting{}",
            st.tile_count(),
            st.texture_bytes() as f64 / 1e6,
            st.outstanding(),
            if st.tiles_complete() {
                ", complete"
            } else {
                ""
            }
        ));
    });
    display_section(ui, page);
    section(ui, "Last signals");
    for l in &page.log {
        ui.weak(l);
    }
}

/// The display target and what the display transform makes of a ramp
/// of scene values (the CPU path; the GPU path runs the same stage).
fn display_section(ui: &mut Ui, page: &PageState) {
    let target = page.state.display_target().unwrap_or_default();
    section(ui, "Display");
    ui.label(format!(
        "Display target: {:?} primaries, {:?}, SDR white {} nits, headroom {}{}",
        target.primaries,
        target.transfer,
        target.sdr_white_nits,
        target.headroom,
        if target.is_supported() {
            ""
        } else {
            " (drawn as sRGB)"
        }
    ));
    let scene: Vec<f32> = (0..16)
        .flat_map(|i| {
            let v = (i as f32 / 15.0).powi(2);
            [v, v, v, 1.0]
        })
        .collect();
    let mut out = vec![egui::Color32::BLACK; 16];
    egui_pop::viewport::display::apply_cpu(
        page.state.display_transform().as_ref(),
        &target,
        &scene,
        &mut out,
    );
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for c in out {
            super::swatch(ui, c, vec2(18.0, 14.0));
        }
    });
}

/// The playground: the source's parameter and a hook.
pub fn playground(ui: &mut Ui, page: &mut PageState) {
    let r = ui.add(egui::Slider::new(&mut page.shift, 0.0..=1.0).text("Hue shift"));
    if r.changed() {
        let shift = page.shift;
        page.source().set_shift(shift);
    }
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
