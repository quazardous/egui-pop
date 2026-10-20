//! The context stack page: a three-level drill-down (an album, a photo,
//! a mask) shown as a breadcrumb, an orientation set for the session and
//! a compare pushed for a moment, with panels and keys that follow, and a
//! guard that refuses to leave a locked photo.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::Ui;
use egui_pop::api::contexts::{self, FrameSpec, Verdict};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::widgets;
use egui_pop::model::commands::CommandSpec;
use egui_pop::model::ids::{CommandId, FrameId, GroupId, PanelId, PluginId, StoryId};
use egui_pop::model::panels::{Area, PanelSpec};
use serde_json::{Value as Json, json};

use super::section;
use crate::shell::SHELL;

pub const CONTEXTS_PAGE: PluginId = PluginId::from_static("gallery.contexts");

pub const ALBUM: FrameId = FrameId::from_static("gallery.album");
pub const PHOTO: FrameId = FrameId::from_static("gallery.photo");
pub const MASK: FrameId = FrameId::from_static("gallery.mask");
pub const CULLING: FrameId = FrameId::from_static("gallery.culling");
pub const RETOUCH: FrameId = FrameId::from_static("gallery.retouch");
pub const REVIEW: FrameId = FrameId::from_static("gallery.review");
pub const COMPARE: FrameId = FrameId::from_static("gallery.compare");
pub const ORIENTATION: GroupId = GroupId::from_static("orientation");

/// Shown while culling.
pub const RATINGS: PanelId = PanelId::from_static("gallery.ratings");
/// Shown while comparing.
pub const SECOND: PanelId = PanelId::from_static("gallery.second");

pub const CMD_PICK: CommandId = CommandId::from_static("gallery.contexts.pick");
pub const CMD_APPROVE: CommandId = CommandId::from_static("gallery.contexts.approve");

/// The context key the page's keys write.
const LAST: &str = "galleryContextKey";

const ALBUMS: [&str; 2] = ["Holidays", "Portraits"];
const MASKS: [&str; 2] = ["Sky", "Face"];

/// The page, its frames, panels and keys.
#[derive(Debug, Default)]
pub struct ContextsPage {
    /// The photos are locked: leaving one is refused by its guard.
    locked: Arc<AtomicBool>,
}

impl Plugin for ContextsPage {
    fn id(&self) -> PluginId {
        CONTEXTS_PAGE
    }

    fn title(&self) -> String {
        "Context stack".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, contexts::PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        ctx.panel(
            PanelSpec::new(RATINGS, "Ratings", Area::EndSide).closed(),
            |ui, app| {
                ui.label("Stars and flags of the photo, shown while culling.");
                ui.label(format!(
                    "P: {}",
                    what_p_does(app.context().get("mode.orientation"))
                ));
            },
        )?
        .panel(
            PanelSpec::new(SECOND, "Second image", Area::Bottom).closed(),
            |ui, _| {
                ui.label("The photo beside another, while comparing.");
            },
        )?;
        let locked = self.locked.clone();
        let album = FrameSpec::place(ALBUM, "Album");
        let photo = FrameSpec::place(PHOTO, "Photo").can_leave(move |_| {
            if locked.load(Ordering::Relaxed) {
                Verdict::Refuse("the photo is locked".into())
            } else {
                Verdict::Go
            }
        });
        let mask = FrameSpec::place(MASK, "Mask").key("galleryMaskOpen", "true");
        let culling = FrameSpec::mode(CULLING, ORIENTATION, "Culling").show(RATINGS);
        let retouch = FrameSpec::mode(RETOUCH, ORIENTATION, "Retouch");
        let review = FrameSpec::mode(REVIEW, ORIENTATION, "Review");
        let compare =
            FrameSpec::mode(COMPARE, GroupId::from_static("compare"), "Compare").show(SECOND);
        ctx.command(
            CommandSpec::new(CMD_PICK, "Pick the photo", "Gallery"),
            |call| {
                call.app.set_context(LAST, "picked");
                Ok(Json::Null)
            },
        )?
        .command(
            CommandSpec::new(CMD_APPROVE, "Approve the photo", "Gallery"),
            |call| {
                call.app.set_context(LAST, "approved");
                Ok(Json::Null)
            },
        )?;
        // Each orientation's own keys: P picks while culling, approves
        // while reviewing.
        ctx.frame_keybinding(&culling, "P", CMD_PICK)?
            .frame_keybinding(&review, "P", CMD_APPROVE)?;
        for f in [album, photo, mask, culling, retouch, review, compare] {
            ctx.frame(f)?;
        }
        let locked = self.locked.clone();
        ctx.story(
            Story::new(StoryId::from_static("contexts"), "Context stack", move |ui, app| {
                show(ui, app, &locked);
            })
            .description("Places (a drill-down, the breadcrumb) and modes (an orientation set, a compare pushed) on one stack owned by the kernel: panels, keys and the view follow, a guard refuses.")
            .group("Framework", 103),
        )?;
        Ok(())
    }
}

fn what_p_does(orientation: &str) -> &'static str {
    match orientation {
        "gallery.culling" => "pick",
        "gallery.review" => "approve",
        _ => "nothing here",
    }
}

fn show(ui: &mut Ui, app: &AppHandle, locked: &AtomicBool) {
    ui.label("Places drill down and come back as they were; modes change how everything behaves without moving. The owner applies or refuses each change; the interface only shows what it says.");
    section(ui, "Where you are");
    contexts::breadcrumb(ui, app);
    contexts::mode_chips(ui, app);
    section(ui, "Orientation, set for the session");
    contexts::mode_switch(ui, app, &ORIENTATION, true);
    section(ui, "Compare, pushed for a moment (Escape pops it)");
    contexts::mode_toggle(ui, app, &COMPARE);
    let mut lock = locked.load(Ordering::Relaxed);
    if widgets::checkbox(
        ui,
        &mut lock,
        "Lock the photos (their guard refuses to leave)",
    )
    .changed()
    {
        locked.store(lock, Ordering::Relaxed);
    }
    let state = contexts::state(app).unwrap_or_default();
    section(ui, "Here");
    let mut enter: Option<(FrameId, Json)> = None;
    match state.places.last().map(|e| (e.frame.as_str(), &e.payload)) {
        None => {
            ui.horizontal_wrapped(|ui| {
                for a in ALBUMS {
                    if ui.button(format!("Open the album {a}")).clicked() {
                        enter = Some((ALBUM, json!(a)));
                    }
                }
            });
        }
        Some(("gallery.album", _)) => {
            contexts::scroll_area(
                ui,
                app,
                "gallery.photos",
                egui::ScrollArea::vertical().max_height(120.0),
                |ui| {
                    for n in 1..=12 {
                        if ui.button(format!("Open photo {n}")).clicked() {
                            enter = Some((PHOTO, json!(n)));
                        }
                    }
                },
            );
        }
        Some(("gallery.photo", _)) => {
            ui.horizontal_wrapped(|ui| {
                for m in MASKS {
                    if ui.button(format!("Open the mask {m}")).clicked() {
                        enter = Some((MASK, json!(m)));
                    }
                }
            });
        }
        Some((_, payload)) => {
            ui.label(format!(
                "Painting the mask {}.",
                payload.as_str().unwrap_or_default()
            ));
        }
    }
    if let Some((frame, payload)) = enter {
        contexts::enter(app, frame, payload);
    }
    section(ui, "Keys here");
    ui.label(format!(
        "P: {}. Alt+Left: back. Last key: {}.",
        what_p_does(app.context().get("mode.orientation")),
        match app.context().get(LAST) {
            "" => "none",
            other => other,
        }
    ));
    ui.weak(format!(
        "Revision {}, projection {}",
        state.rev,
        state.projection_key()
    ));
}
