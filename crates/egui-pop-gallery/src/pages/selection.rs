//! The shared selection page: a tree, a list and a small canvas show the
//! same entities and share one selection (`egui_pop::api::selection`).
//! A click in one selects in all three, hovering an entity lights it up
//! everywhere, and Alt+Left and Alt+Right walk back and forward through
//! the past selections. See `docs/selection.md`.

use egui::{Role, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};
use egui_pop::api::selection::{
    self, CMD_BACK, CMD_FORWARD, Gesture, SELECTION, SELECTION_PLUGIN, SelectionItem,
    SelectionService,
};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::theme::Tokens;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::views::Mode;
use egui_pop::views::list::{ListRows, ListState, ListView};
use egui_pop::views::tree::{Node, Sink, TreeSource, TreeState, TreeView};

use super::section;
use crate::shell::SHELL;

pub const SELECTION_PAGE: PluginId = PluginId::from_static("gallery.selection");

/// The tree's id: the source its changes and hovers name.
pub const TREE: &str = "gallery.selection.tree";
/// The list's id.
pub const LIST: &str = "gallery.selection.list";
/// The canvas's id.
pub const CANVAS: &str = "gallery.selection.canvas";

/// The entities: their path (the selection's item), their name and where
/// the canvas draws them (in its unit square).
pub const ENTITIES: [(&str, &str, [f32; 2]); 6] = [
    ("world/camera", "Camera", [0.12, 0.2]),
    ("world/points/a", "Point A", [0.35, 0.65]),
    ("world/points/b", "Point B", [0.5, 0.35]),
    ("world/points/c", "Point C", [0.62, 0.75]),
    ("world/boxes/left", "Left box", [0.8, 0.25]),
    ("world/boxes/right", "Right box", [0.88, 0.7]),
];

/// The tree's branches: path and name.
const BRANCHES: [(&str, &str); 3] = [
    ("world", "World"),
    ("world/points", "Points"),
    ("world/boxes", "Boxes"),
];

fn name_of(path: &str) -> &str {
    ENTITIES
        .iter()
        .map(|(p, n, _)| (*p, *n))
        .chain(BRANCHES)
        .find(|(p, _)| *p == path)
        .map_or(path, |(_, n)| n)
}

/// The scene as a tree: keys are the paths, so the default item of a
/// key (the path itself) is the entity's.
struct Scene;

impl TreeSource<String> for Scene {
    fn load(&self, parent: Option<&String>, sink: Sink<String>) {
        let under = parent.map_or("", String::as_str);
        let depth = |p: &str| p.matches('/').count();
        let child = |p: &str| match parent {
            None => depth(p) == 0,
            Some(_) => p
                .strip_prefix(under)
                .and_then(|r| r.strip_prefix('/'))
                .is_some_and(|r| !r.contains('/')),
        };
        let branches = BRANCHES
            .iter()
            .filter(|(p, _)| child(p))
            .map(|(p, n)| Node::branch(p.to_string(), *n));
        let leaves = ENTITIES
            .iter()
            .filter(|(p, _, _)| child(p))
            .map(|(p, n, _)| Node::leaf(p.to_string(), *n));
        sink.deliver(branches.chain(leaves).collect());
    }
}

/// The entities as a list, by name.
struct Entities;

impl ListRows<usize> for Entities {
    fn len(&self) -> usize {
        ENTITIES.len()
    }

    fn key(&self, row: usize) -> usize {
        row
    }

    fn label(&self, row: usize) -> String {
        ENTITIES[row].1.to_string()
    }

    /// The row's entity, by path: what the tree and the canvas call it.
    fn item(&self, row: usize) -> SelectionItem {
        SelectionItem::from(ENTITIES[row].0)
    }
}

struct PageState {
    tree: TreeState<String>,
    list: ListState<usize>,
    rows: Entities,
}

impl Default for PageState {
    fn default() -> Self {
        let mut tree = TreeState::default();
        for (branch, _) in BRANCHES {
            tree.set_expanded(&branch.to_string(), true);
        }
        Self {
            tree,
            list: ListState::default(),
            rows: Entities,
        }
    }
}

/// The page.
#[derive(Debug, Default)]
pub struct SelectionPage;

impl Plugin for SelectionPage {
    fn id(&self) -> PluginId {
        SELECTION_PAGE
    }

    fn title(&self) -> String {
        "Shared selection".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, egui_pop::views::steps::VIEWS, SELECTION_PLUGIN]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let mut page = PageState::default();
        ctx.story(
            Story::new(StoryId::from_static("selection"), "Shared selection", move |ui, app| {
                show(ui, app, &mut page)
            })
            .description("A tree, a list and a canvas share one selection: a click in one selects in all three, hovering an entity lights it up everywhere, and Alt+Left and Alt+Right go back and forward through the past selections.")
            .group("Framework", 122),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle, p: &mut PageState) {
    let Some(service) = app.service(&SELECTION) else {
        ui.weak("The selection plugin is not connected.");
        return;
    };
    ui.label("Click, Ctrl+click or Shift+click in any of the three: the others follow. Hover an entity to light it up everywhere. In the tree or the list, Alt+Left and Alt+Right go back and forward through the past selections.");
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(service.can_back(), egui::Button::new("Back"))
            .clicked()
        {
            app.run(CMD_BACK, serde_json::Value::Null);
        }
        if ui
            .add_enabled(service.can_forward(), egui::Button::new("Forward"))
            .clicked()
        {
            app.run(CMD_FORWARD, serde_json::Value::Null);
        }
        if ui
            .add_enabled(!service.is_empty(), egui::Button::new("Clear"))
            .clicked()
        {
            service.clear("gallery.selection.clear");
        }
    });
    let height = super::fitted_height(ui, 240.0);
    ui.columns(3, |cols| {
        section(&mut cols[0], "Tree");
        let _ = TreeView::<String>::new(TREE)
            .label("Scene tree")
            .mode(Mode::Multiple)
            .app(app)
            .shared(&service)
            .height(height)
            .show(&mut cols[0], &mut p.tree, &Scene);
        section(&mut cols[1], "List");
        let _ = ListView::new(LIST)
            .label("Entities")
            .mode(Mode::Multiple)
            .app(app)
            .shared(&service)
            .height(height)
            .show(&mut cols[1], &mut p.list, &mut p.rows);
        section(&mut cols[2], "Canvas");
        canvas(&mut cols[2], &service, height);
    });
    let names = |items: Vec<SelectionItem>| {
        items
            .iter()
            .map(|i| name_of(i.as_str()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let items = service.items();
    ui.label(format!(
        "Selected: {}",
        if items.is_empty() {
            "nothing".to_string()
        } else {
            names(items)
        }
    ));
    ui.label(format!(
        "Hovered: {}",
        service
            .hovered()
            .map_or("nothing".to_string(), |h| name_of(h.as_str()).to_string())
    ));
    ui.weak(format!(
        "{} past selections, {} ahead (`selection.back`, `selection.forward`)",
        service.describe()["back"],
        service.describe()["forward"],
    ));
}

/// A canvas of dots, one per entity: an application's own widget on the
/// service, without a view of egui-pop's.
fn canvas(ui: &mut Ui, service: &SelectionService, height: f32) {
    selection::surface(ui, |ui| {
        let size = vec2(ui.available_width(), height);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let t = Tokens::of(ui.ctx());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, t.radius.widget, t.surface.sunken);
        let radius = 9.0;
        let at = |p: [f32; 2]| {
            pos2(
                rect.left() + 16.0 + p[0] * (rect.width() - 32.0),
                rect.top() + 16.0 + p[1] * (rect.height() - 32.0),
            )
        };
        let pointer = response.hover_pos();
        let mut hovering = None;
        for (path, name, p) in ENTITIES {
            let item = SelectionItem::from(path);
            let c = at(p);
            let here = pointer.is_some_and(|q| q.distance(c) <= radius + 2.0);
            if here {
                hovering = Some(item.clone());
            }
            let selected = service.contains(&item);
            let hovered = here || service.is_hovered(&item);
            let fill = if selected {
                t.accent.default
            } else {
                t.surface.raised
            };
            painter.circle_filled(c, radius, fill);
            if hovered {
                painter.circle_stroke(c, radius + 3.0, Stroke::new(2.0, t.focus_ring));
            }
            painter.text(
                c + vec2(0.0, radius + 4.0),
                egui::Align2::CENTER_TOP,
                name,
                egui::TextStyle::Small.resolve(ui.style()),
                t.text.default,
            );
        }
        service.hover(CANVAS, hovering.as_ref());
        if response.clicked() {
            response.request_focus();
            let order: Vec<SelectionItem> = ENTITIES
                .iter()
                .map(|(p, _, _)| SelectionItem::from(*p))
                .collect();
            let gesture = Gesture::of(ui.input(|i| i.modifiers));
            match &hovering {
                Some(item) => {
                    service.click(item, gesture, &order, CANVAS);
                }
                // A click on nothing clears, as in most canvases.
                None if gesture == Gesture::Replace => {
                    service.clear(CANVAS);
                }
                None => {}
            }
        }
        response.widget_info(|| WidgetInfo::labeled(Role::Canvas, true, "Entities on a canvas"));
    });
}
