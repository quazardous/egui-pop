//! The linked panels page: a list on the page drives a preview in a panel
//! of its own, which may be closed, moved or detached into a window. The
//! selection is one value in the store, under a path this page names; the
//! preview is slow, so it follows through `latest::Latest`. See
//! `docs/linked-panels.md`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use egui::Ui;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError, ValueWriter};
use egui_pop::core::latest::{Cancel, Latest, Loaded};
use egui_pop::core::lock;
use egui_pop::model::commands::Origin;
use egui_pop::model::ids::{PanelId, PluginId, StoryId};
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop::model::values::{Change, Descriptor, Set, Value};
use egui_pop::views::list::{ListRows, ListState, ListView};

use crate::shell::SHELL;

pub const LINKED_PAGE: PluginId = PluginId::from_static("gallery.linked");

/// The panel showing the preview.
pub const PREVIEW: PanelId = PanelId::from_static("gallery.linked.preview");

/// What is selected: a photo's name, empty for none. The application
/// names the path; another selection would be another path.
pub const SELECTION: &str = "gallery/linked/selection";

/// The photos of the list.
pub const PHOTOS: [&str; 6] = [
    "Harbour at dawn",
    "Market street",
    "Old bridge",
    "Snow on the pass",
    "Night train",
    "Lighthouse",
];

/// How a preview takes its time: so many naps of `NAP`, looking in between
/// whether it is still wanted.
const NAPS: u32 = 8;
const NAP: Duration = Duration::from_millis(15);

#[derive(Debug, Default)]
struct Photos;

impl ListRows<usize> for Photos {
    fn len(&self) -> usize {
        PHOTOS.len()
    }

    fn key(&self, row: usize) -> usize {
        row
    }

    fn label(&self, row: usize) -> String {
        PHOTOS[row].to_string()
    }
}

#[derive(Debug, Default)]
struct PageState {
    list: ListState<usize>,
    rows: Photos,
    /// The stored value as last followed or written.
    followed: String,
    /// Why the last write was refused.
    refused: Option<String>,
}

/// The page and its panel.
#[derive(Debug, Default)]
pub struct LinkedPage;

impl Plugin for LinkedPage {
    fn id(&self) -> PluginId {
        LINKED_PAGE
    }

    fn title(&self) -> String {
        "Linked panels".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, egui_pop::views::steps::VIEWS]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        lock(&ctx.app().store())
            .declare(
                ctx.owner(),
                SELECTION,
                Descriptor::text("The photo selected", ""),
            )
            .map_err(|e| PluginError::Failed(e.to_string()))?;
        let computed = Arc::new(AtomicUsize::new(0));
        let mut latest = Latest::new();
        let counted = computed.clone();
        ctx.panel(
            PanelSpec::new(PREVIEW, "Preview", Area::EndSide)
                .min_size(180.0)
                .closed(),
            move |ui, app| preview(ui, app, &mut latest, &counted),
        )?;
        let writer = ctx.writer();
        let mut page = PageState::default();
        ctx.story(
            Story::new(StoryId::from_static("linked-panels"), "Linked panels", move |ui, app| {
                show(ui, app, &writer, &mut page, &computed)
            })
            .description("A list on the page drives a slow preview in another panel, which may be closed, moved or detached: one value in the store, under a path the application names.")
            .group("Framework", 121),
        )?;
        Ok(())
    }
}

/// What the store says is selected.
fn selected(app: &AppHandle) -> String {
    lock(&app.store())
        .get(SELECTION)
        .and_then(|e| e.value.as_text())
        .unwrap_or_default()
        .to_string()
}

fn show(
    ui: &mut Ui,
    app: &AppHandle,
    writer: &ValueWriter,
    p: &mut PageState,
    computed: &AtomicUsize,
) {
    ui.label("Select a photo: the preview panel follows, wherever it is. Closed, it computes nothing, and it picks up the current photo when it comes back.");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Open the preview").clicked() {
            app.set_panel_visible(&PREVIEW, true);
        }
        if ui.button("Close the preview").clicked() {
            app.set_panel_visible(&PREVIEW, false);
        }
        if ui.button("Detach the preview").clicked() {
            app.detach_panel(&PREVIEW);
        }
        if ui.button("Dock the preview").clicked() {
            app.attach_panel(&PREVIEW);
        }
    });
    // Selected elsewhere (another panel, an agent): the list follows.
    let stored = selected(app);
    if stored != p.followed {
        match PHOTOS.iter().position(|name| *name == stored) {
            Some(row) => {
                p.list.set_selection([row], Some(row));
                p.list.reveal(row);
            }
            None => p.list.set_selection(Vec::new(), None),
        }
        p.followed = stored;
    }
    let _ = ListView::new("gallery.linked.photos")
        .label("Photos to preview")
        .app(app)
        .height(super::fitted_height(ui, 170.0))
        .show(ui, &mut p.list, &mut p.rows);
    // The list's selection goes out when it changed, whatever changed it.
    let now = p
        .list
        .selection()
        .keys()
        .first()
        .map_or("", |row| PHOTOS[*row])
        .to_string();
    if now != p.followed {
        let write = Set::new(
            SELECTION,
            Change::To(Value::Text(now.clone())),
            Origin::User,
        );
        // Not an undo step: a selection is not an edit.
        p.refused = writer.set(write.untracked()).err().map(|r| r.to_string());
        p.followed = now;
        // The panel may be drawn before this page.
        ui.ctx().request_repaint();
    }
    let shown = if p.followed.is_empty() {
        "nothing"
    } else {
        p.followed.as_str()
    };
    ui.label(format!("Selected: {shown}"));
    if let Some(reason) = &p.refused {
        ui.colored_label(ui.visuals().error_fg_color, reason.as_str());
    }
    ui.label(format!(
        "Previews computed: {}",
        computed.load(Ordering::Relaxed)
    ));
}

/// The panel: only its `ui()` asks for a preview, so a closed one asks for
/// nothing.
fn preview(
    ui: &mut Ui,
    app: &AppHandle,
    latest: &mut Latest<String, String>,
    computed: &Arc<AtomicUsize>,
) {
    let photo = selected(app);
    let key = (!photo.is_empty()).then_some(&photo);
    let computed = computed.clone();
    match latest.want(ui.ctx(), key, move |name, cancel| {
        compute(name, cancel, &computed)
    }) {
        Loaded::Nothing => {
            ui.weak("Select a photo on the page.");
        }
        Loaded::Loading { previous } => {
            ui.weak(format!("Loading {photo}…"));
            if let Some(text) = previous {
                ui.weak(text.as_str());
            }
        }
        Loaded::Ready(text) => {
            ui.label(text.as_str());
        }
        Loaded::Failed(reason) => {
            ui.colored_label(ui.visuals().error_fg_color, reason);
        }
    }
}

/// A pretend slow preview, on the worker. The web has no thread and cannot
/// sleep: there it is done at once.
fn compute(name: &str, cancel: &Cancel, computed: &AtomicUsize) -> String {
    computed.fetch_add(1, Ordering::Relaxed);
    if !cfg!(target_arch = "wasm32") {
        for _ in 0..NAPS {
            if cancel.cancelled() {
                break;
            }
            std::thread::sleep(NAP);
        }
    }
    format!("Preview of {name}")
}
