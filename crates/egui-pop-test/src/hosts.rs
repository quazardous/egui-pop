//! Mock hosts for mount tests (ADR 0013): each reproduces a host's offer
//! headless, around content drawn each frame: a panel or a story as a
//! whole rect, a palette as an area of bounded width, a line zone or the
//! status bar as one row, a raw modal and a popover through the overlay
//! stack, a dialog of the four kinds as a form, a custom dialog with the
//! content as its body.
//!
//! ```no_run
//! use std::rc::Rc;
//! use egui_pop_api::mounts;
//! use egui_pop_test::hosts::MockHost;
//!
//! fn mount(ctx: &egui::Context, app: &egui_pop_api::AppHandle) {
//!     let draw = |ui: &mut egui::Ui, _: &egui_pop_api::AppHandle| {
//!         ui.label("content");
//!     };
//!     let mut host = MockHost::new(mounts::palette(), Rc::new(draw));
//!     // Each frame, after the application is drawn:
//!     host.show(ctx, app);
//!     // When done:
//!     host.close(app);
//! }
//! ```

use std::fmt;
use std::rc::Rc;

use egui::{Id, Rect, Ui, pos2, vec2};
use egui_pop_api::AppHandle;
use egui_pop_api::dialogs::{Answer, Custom, DIALOGS, Form};
use egui_pop_api::mounts;
use egui_pop_core::overlay::{self, Anchor, Kind, OverlaySpec};
use egui_pop_core::placement::Side;
use egui_pop_model::commands::Origin;
use egui_pop_model::ids::CommandId;
use egui_pop_model::mount::{HostOffer, ROW_HEIGHT};
use egui_pop_model::values::Descriptor;

/// Content drawn in a mock host.
pub type Draw = Rc<dyn Fn(&mut Ui, &AppHandle)>;

/// What a mock dialog's accepting runs (nothing is declared under it:
/// mock dialogs are cancelled).
const MOCK: CommandId = CommandId::from_static("egui_pop_test.mock");

/// A host reproduced from its offer, drawing its content each frame.
pub struct MockHost {
    offer: HostOffer,
    draw: Draw,
    /// The dialog it opened, for the dialog hosts.
    dialog: Option<u64>,
}

impl fmt::Debug for MockHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockHost")
            .field("host", &self.offer.id)
            .field("dialog", &self.dialog)
            .finish_non_exhaustive()
    }
}

impl MockHost {
    pub fn new(offer: HostOffer, draw: Draw) -> Self {
        Self {
            offer,
            draw,
            dialog: None,
        }
    }

    /// Whether a host has a mock: every host that can be mounted
    /// headless and takes a `Ui` (not the auxiliary window, which
    /// kittest embeds in the main one, nor the viewport's overlays).
    pub fn supported(offer: &HostOffer) -> bool {
        offer.headless
            && offer.gives_ui
            && matches!(
                offer.id.as_str(),
                mounts::PANEL
                    | mounts::PALETTE
                    | mounts::DIALOG
                    | mounts::CUSTOM_DIALOG
                    | mounts::MODAL
                    | mounts::POPOVER
                    | mounts::LINE
                    | mounts::STATUS_BAR
                    | mounts::STORY
            )
    }

    pub fn offer(&self) -> &HostOffer {
        &self.offer
    }

    /// Draws the host and its content for this frame (after the
    /// application is drawn); a dialog host opens its dialog once.
    pub fn show(&mut self, ctx: &egui::Context, app: &AppHandle) {
        let draw = self.draw.clone();
        let id = Id::new(("egui_pop_test.mock", self.offer.id.as_str()));
        match self.offer.id.as_str() {
            mounts::PANEL | mounts::STORY => {
                area(ctx, id, |ui| {
                    let size = vec2(640.0, 420.0);
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    draw(ui, app);
                });
            }
            mounts::PALETTE => {
                area(ctx, id, |ui| {
                    ui.set_max_width(320.0);
                    draw(ui, app);
                });
            }
            mounts::LINE | mounts::STATUS_BAR => {
                area(ctx, id, |ui| {
                    let size = vec2(480.0, ROW_HEIGHT);
                    ui.allocate_ui(size, |ui| {
                        ui.set_max_size(size);
                        ui.set_clip_rect(ui.max_rect());
                        ui.horizontal(|ui| draw(ui, app));
                    });
                });
            }
            mounts::MODAL => {
                let spec = OverlaySpec::new(id, "Mock modal", Kind::Modal).width(560.0);
                let mut open = true;
                overlay::show(ctx, &spec, &mut open, None, |ui| draw(ui, app));
            }
            mounts::POPOVER => {
                let spec = OverlaySpec::new(id, "Mock popover", Kind::Popover);
                let anchor = Anchor {
                    rect: Rect::from_min_size(pos2(40.0, 40.0), vec2(80.0, 20.0)),
                    side: Side::Bottom,
                };
                let mut open = true;
                overlay::show(ctx, &spec, &mut open, Some(anchor), |ui| {
                    ui.set_max_width(360.0);
                    draw(ui, app);
                });
            }
            mounts::DIALOG => {
                if self.dialog.is_none()
                    && let Some(d) = app.service(&DIALOGS)
                {
                    // The four kinds take descriptors: a form's field.
                    let form = Form::new("Mock form", "Apply", MOCK)
                        .field("value", Descriptor::float("Value", 0.0, 1.0, 0.5));
                    self.dialog = Some(d.open(form));
                }
            }
            mounts::CUSTOM_DIALOG => {
                if self.dialog.is_none()
                    && let Some(d) = app.service(&DIALOGS)
                {
                    let custom = Custom::new("Mock dialog", "Apply", MOCK).size(560.0, 360.0);
                    let body = draw.clone();
                    self.dialog = d
                        .open_custom(app, custom, move |ui, app, _| body(ui, app))
                        .ok();
                }
            }
            _ => {}
        }
    }

    /// Takes the content out: a dialog it opened is cancelled (on the
    /// next frame); overlays not shown any more close by themselves.
    pub fn close(&mut self, app: &AppHandle) {
        if self.dialog.take().is_some()
            && let Some(d) = app.service(&DIALOGS)
        {
            let _ = d.answer(Answer::Cancel, Origin::User);
        }
    }
}

fn area(ctx: &egui::Context, id: Id, add: impl FnOnce(&mut Ui)) {
    egui::Area::new(id)
        .fixed_pos(pos2(24.0, 48.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::window(ui.style()).show(ui, add);
        });
}
