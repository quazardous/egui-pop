//! The window frame page: the setting drawing the gallery's own frame, a
//! title bar preview with its hit regions, what a point of it is, and
//! the frame's steps (a veto on closing).

use egui::{Color32, Rect, Sense, Stroke, Ui, vec2};
use egui_pop::api::frame::{self, STEP_CLOSE};
use egui_pop::api::steps::Before;
use egui_pop::api::stories::Story;
use egui_pop::api::zones::{self, Block};
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::frame::{FrameLayout, HitRegions, TitleBar};
use egui_pop::core::kit::Kit;
use egui_pop::core::param::{self, Mode};
use egui_pop::core::state::Slot;
use egui_pop::core::widgets;
use egui_pop::model::ids::{BlockId, PluginId, StoryId};
use egui_pop::theme::Tokens;

use super::section;
use crate::shell::SHELL;

/// The preview's last request.
const LAST: Slot<String> = Slot::new("gallery.frame.last");

pub const FRAME_PAGE: PluginId = PluginId::from_static("gallery.frame");

/// The context key the close veto reads.
const VETO: &str = "galleryVetoClose";

/// The window frame page.
#[derive(Debug, Default)]
pub struct FramePage;

impl Plugin for FramePage {
    fn id(&self) -> PluginId {
        FRAME_PAGE
    }

    fn title(&self) -> String {
        "Window frame".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, PluginId::from_static("egui_pop.frame")]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.before(STEP_CLOSE, |app, _| {
            if app.context().get(VETO) == "true" {
                Before::Veto("the gallery has unsaved work (the page's veto is on)".into())
            } else {
                Before::Go
            }
        });
        // The gallery's logo at the start of its own title bar, the
        // system menu button kept.
        ctx.block(
            zones::CAPTION_START,
            Block::replace(
                frame::BLOCK_MENU,
                BlockId::from_static("gallery.frame.logo"),
                |ui, app| {
                    frame::menu_button(ui, app);
                    logo(ui);
                    ui.strong("egui-pop gallery");
                },
            ),
        );
        ctx.story(
            Story::new(StoryId::from_static("frame"), "Title bar and frame", show)
                .description("A title bar and resize edges drawn by the application, publishing their hit regions in physical pixels; closing, minimizing and maximizing are steps other plugins may veto or replace.")
                .group("Framework", 105),
        )?;
        Ok(())
    }
}

/// The gallery's logo: an accent disc with a ring.
fn logo(ui: &mut Ui) {
    let t = Tokens::of(ui.ctx());
    let side = t.size.icon_toolbar;
    let (r, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let p = ui.painter();
    p.circle_filled(r.center(), side * 0.45, t.accent.default);
    p.circle_stroke(r.center(), side * 0.25, Stroke::new(1.5, t.surface.panel));
}

fn show(ui: &mut Ui, app: &AppHandle) {
    let t = Tokens::of(ui.ctx());
    let store = app.store();
    let descriptor = egui_pop::core::lock(&store)
        .get(&format!("settings/{}", frame::SETTING_FRAME))
        .map(|e| e.descriptor.clone());
    if let Some(d) = descriptor {
        param::param(
            ui,
            &d,
            Mode::Bound(&store, &format!("settings/{}", frame::SETTING_FRAME)),
        );
    }
    ui.weak("`own`: the gallery draws its title bar above the menu bar, with its logo at the start (a block replacing `egui_pop.frame.menu` in the `app.caption-start` zone), and its edges resize the window.");
    let mut veto = app.context().get(VETO) == "true";
    if widgets::checkbox(
        ui,
        &mut veto,
        "Veto closing the window (a before-hook on frame.close)",
    )
    .changed()
    {
        app.set_context(VETO, if veto { "true" } else { "false" });
    }

    section(ui, "Preview");
    let width = ui.available_width().min(560.0);
    let (outer, _) = ui.allocate_exact_size(
        vec2(width, Kit::of(ui.ctx()).title_bar.data.height + 72.0),
        Sense::hover(),
    );
    ui.painter().rect_stroke(
        outer,
        0.0,
        Stroke::new(1.0, t.border.default),
        egui::StrokeKind::Outside,
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(outer));
    let out = TitleBar::new("Untitled — Preview")
        .publish(false)
        .full_screen(false)
        .show(
            &mut child,
            |ui| {
                ui.add(
                    egui_pop::core::icon::Icon::Menu
                        .size(egui_pop::core::icon::Size::Toolbar)
                        .label("Preview menu")
                        .sense_click(),
                );
                ui.strong("App");
            },
            |ui| {
                let _ = ui.small_button("Layout");
            },
        );
    if let Some(r) = out.request {
        LAST.set(ui.ctx(), format!("{r:?}"));
    }
    // The regions as the frame publishes them, over the preview.
    let layout = FrameLayout {
        window: outer,
        grip: egui_pop::core::frame::grip(ui.ctx()),
        resizable: true,
        ..out.layout.clone()
    };
    let ppp = ui.ctx().pixels_per_point();
    // Measured from the preview's corner, as a window's are from its own.
    let local = |r: Rect| r.translate(-outer.min.to_vec2());
    let regions = HitRegions::measure(
        &FrameLayout {
            window: local(outer),
            caption: layout.caption.map(local),
            holes: layout.holes.iter().copied().map(local).collect(),
            minimize: layout.minimize.map(local),
            maximize: layout.maximize.map(local),
            close: layout.close.map(local),
            ..layout.clone()
        },
        ppp,
    );
    let painter = ui.painter_at(outer);
    let outline = |r: Option<Rect>, c: Color32| {
        if let Some(r) = r {
            painter.rect_stroke(
                r.shrink(1.0),
                0.0,
                Stroke::new(1.0, c),
                egui::StrokeKind::Inside,
            );
        }
    };
    outline(layout.caption, t.accent.default);
    for h in &layout.holes {
        outline(Some(*h), t.status.info);
    }
    outline(layout.minimize, t.status.success);
    outline(layout.maximize, t.status.warning);
    outline(layout.close, t.status.danger);
    let grip = layout.grip;
    painter.rect_stroke(
        outer.shrink(grip / 2.0),
        0.0,
        Stroke::new(grip, t.status.warning.gamma_multiply(0.25)),
        egui::StrokeKind::Middle,
    );
    let hover = ui
        .ctx()
        .input(|i| i.pointer.hover_pos())
        .filter(|p| outer.contains(*p));
    let hit = hover.map(|p| {
        let at = (p - outer.min) * ppp;
        regions.hit(at.x.floor() as i32, at.y.floor() as i32)
    });
    ui.horizontal(|ui| {
        ui.label("Under the pointer:");
        ui.monospace(hit.map_or("(outside the preview)".to_string(), |h| format!("{h:?}")));
    });
    let last = LAST.get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label("Last request:");
        ui.monospace(last.unwrap_or_else(|| "(none)".into()));
    });
    ui.weak("Outlines: caption (accent), widgets on the bar (info), minimize, maximize, close; the shaded border is the resize edge, never under 6 physical pixels.");

    section(ui, "Published regions of this window");
    let published = egui_pop::core::frame::regions(ui.ctx());
    ui.monospace(match published {
        Some(r) => serde_json::to_string(&*r).unwrap_or_default(),
        None => "(none: the system draws this window's frame)".into(),
    });

    section(ui, "The frame's steps");
    let traces: Vec<String> = app
        .step_traces()
        .into_iter()
        .filter(|t| t.step.as_str().starts_with("frame."))
        .rev()
        .take(4)
        .map(|t| match (&t.vetoed_by, &t.result) {
            (Some((by, why)), _) => format!("{}: vetoed by {by} ({why})", t.step),
            (None, Some(r)) => format!(
                "{}: ran by {} -> {r:?}",
                t.step,
                t.ran_by.unwrap_or_default()
            ),
            _ => t.step.to_string(),
        })
        .collect();
    if traces.is_empty() {
        ui.weak("Nothing yet: use the window menu (View > Window menu) or the frame's buttons.");
    }
    for line in traces {
        ui.monospace(line);
    }
}
