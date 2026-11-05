//! The list items page: dense rows on their own (an icon, a label, weak
//! trailing text, action buttons shown on hover, indent guides, an arrow,
//! drag handles), the same rows in a tree, and a property panel like a
//! viewer's selection panel, its names aligned.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use egui::{Color32, Sense, Ui, vec2};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::icon::Icon;
use egui_pop::core::sections::{Section, Sections};
use egui_pop::core::theme::Tokens;
use egui_pop::core::widgets;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::views::item::{ItemAction, ListItem};
use egui_pop::views::property::PropertyRow;
use egui_pop::views::tree::{Node, Sink, TreeSource, TreeState, TreeView};

use super::section;
use crate::shell::SHELL;

pub const LIST_ITEMS_PAGE: PluginId = PluginId::from_static("gallery.list-items");

/// An entity of the pretend scene: its path, its kind, how many points.
#[derive(Clone, Copy, Debug)]
pub struct Entity {
    pub path: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    pub points: u32,
    pub color: Color32,
}

/// The scene's entities, under `world`.
pub const ENTITIES: [Entity; 4] = [
    Entity {
        path: "world/camera",
        name: "camera",
        kind: "Pinhole",
        points: 1,
        color: Color32::from_rgb(90, 129, 255),
    },
    Entity {
        path: "world/points",
        name: "points",
        kind: "Points3D",
        points: 1200,
        color: Color32::from_rgb(230, 180, 60),
    },
    Entity {
        path: "world/boxes",
        name: "boxes",
        kind: "Boxes3D",
        points: 8,
        color: Color32::from_rgb(120, 200, 120),
    },
    Entity {
        path: "world/log",
        name: "log",
        kind: "TextLog",
        points: 0,
        color: Color32::from_rgb(200, 120, 200),
    },
];

/// The tree's source: `world` and its entities.
#[derive(Debug, Default)]
pub struct Scene;

impl TreeSource<String> for Scene {
    fn load(&self, parent: Option<&String>, sink: Sink<String>) {
        sink.deliver(match parent {
            None => vec![Node::branch("world".into(), "world")],
            Some(_) => ENTITIES
                .iter()
                .map(|e| Node::leaf(e.path.into(), e.name))
                .collect(),
        });
    }
}

/// What the page keeps between passes.
#[derive(Debug)]
pub struct PageState {
    pub open: bool,
    pub selected: Option<usize>,
    pub hidden: BTreeSet<usize>,
    pub removed: BTreeSet<usize>,
    pub tree: TreeState<String>,
    pub radius: f32,
    pub fill: bool,
    pub read_only: bool,
    pub log: Vec<String>,
}

impl Default for PageState {
    fn default() -> Self {
        let mut tree = TreeState::new();
        tree.set_expanded(&"world".to_string(), true);
        Self {
            open: true,
            selected: Some(1),
            hidden: BTreeSet::from([2]),
            removed: BTreeSet::new(),
            tree,
            radius: 0.5,
            fill: true,
            read_only: false,
            log: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ListItemsPage;

impl Plugin for ListItemsPage {
    fn id(&self) -> PluginId {
        LIST_ITEMS_PAGE
    }

    fn title(&self) -> String {
        "List items".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, egui_pop::views::steps::VIEWS]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let page = Rc::new(RefCell::new(PageState::default()));
        ctx.story(
            Story::new(StoryId::from_static("list-items"), "List items", move |ui, app| {
                show(ui, app, &mut page.borrow_mut())
            })
            .description("Dense rows: an icon, a label, weak trailing text, action buttons on hover, indent guides and drag handles; the same rows in a tree; property rows in a selection panel, their names aligned.")
            .group("Components", 78),
        )?;
        Ok(())
    }
}

/// The eye of an entity: open while shown, closed (and pinned) while
/// hidden.
fn eye(name: &str, hidden: bool) -> ItemAction {
    if hidden {
        ItemAction::new("visibility", Icon::Hidden, format!("Show {name}")).pinned(true)
    } else {
        ItemAction::new("visibility", Icon::Visible, format!("Hide {name}"))
    }
}

fn show(ui: &mut Ui, app: &AppHandle, p: &mut PageState) {
    ui.label("Hover a row for its actions; Tab walks from a row through its buttons. The arrow folds the scene, the eye hides an entity, the cross removes it.");
    ui.columns(2, |cols| {
        let [left, right] = cols else { return };
        items(left, p);
        panel(right, p);
    });
    section(ui, "In a tree");
    let hidden = p.hidden.clone();
    let mut toggled = Vec::new();
    TreeView::<String>::new("gallery.list-items.tree")
        .label("Scene")
        .app(app)
        .height(5.0 * egui_pop::views::item::row_height(ui))
        .guides(true)
        .icon(|n| Some(if n.branch { Icon::Folder } else { Icon::Dot }.into()))
        .trailing(|n| {
            ENTITIES
                .iter()
                .find(|e| e.path == n.key)
                .map(|e| e.kind.to_string())
        })
        .actions(|n| {
            ENTITIES
                .iter()
                .position(|e| e.path == n.key)
                .map(|i| vec![eye(ENTITIES[i].name, hidden.contains(&i))])
                .unwrap_or_default()
        })
        .on_action(|key, action| toggled.push((key.clone(), action.to_string())))
        .show(ui, &mut p.tree, &Scene);
    for (key, action) in toggled {
        if let Some(i) = ENTITIES.iter().position(|e| e.path == key) {
            flip(&mut p.hidden, i);
        }
        p.log.push(format!("tree.action {action} {key}"));
    }
    section(ui, "Signals");
    if p.log.is_empty() {
        ui.weak("Nothing yet.");
    }
    let skip = p.log.len().saturating_sub(3);
    for line in &p.log[skip..] {
        ui.monospace(line);
    }
}

fn flip(set: &mut BTreeSet<usize>, i: usize) {
    if !set.remove(&i) {
        set.insert(i);
    }
}

/// Standalone items: a collapsing scene, its entities nested with
/// guides, drag handles and actions, and a disabled row.
fn items(ui: &mut Ui, p: &mut PageState) {
    section(ui, "Items");
    let out = ListItem::new("world")
        .icon(Icon::Folder)
        .collapsing(p.open)
        .trailing(format!("{} entities", ENTITIES.len() - p.removed.len()))
        .id_salt("world")
        .show(ui);
    if out.toggled {
        p.open = !p.open;
        p.log.push(format!(
            "toggled world: {}",
            if p.open { "open" } else { "closed" }
        ));
    }
    if p.open {
        for (i, e) in ENTITIES.iter().enumerate() {
            if p.removed.contains(&i) {
                continue;
            }
            let hidden = p.hidden.contains(&i);
            let out = ListItem::new(e.name)
                .icon(Icon::Dot)
                .depth(1)
                .guides(true)
                .drag_handle(true)
                .trailing(e.kind)
                .selected(p.selected == Some(i))
                .action(ItemAction::new(
                    "remove",
                    Icon::Close,
                    format!("Remove {}", e.name),
                ))
                .action(eye(e.name, hidden))
                .id_salt(e.path)
                .show(ui);
            match out.action() {
                Some("visibility") => flip(&mut p.hidden, i),
                Some("remove") => {
                    p.removed.insert(i);
                    if p.selected == Some(i) {
                        p.selected = None;
                    }
                }
                _ => {}
            }
            if let Some(a) = out.action() {
                p.log.push(format!("{a} {}", e.path));
            } else if out.response.clicked() {
                p.selected = Some(i);
                p.log.push(format!("selected {}", e.path));
            }
        }
    }
    let _ = ListItem::new("Archived recording")
        .icon(Icon::File)
        .trailing("read only")
        .disabled(true)
        .id_salt("archived")
        .show(ui);
    if !p.removed.is_empty() && ui.button("Restore the removed").clicked() {
        p.removed.clear();
    }
}

/// A selection panel: the selected entity's properties in sections, the
/// names of each section aligned.
fn panel(ui: &mut Ui, p: &mut PageState) {
    section(ui, "Property rows");
    let Some(i) = p.selected else {
        ui.weak("Nothing selected.");
        return;
    };
    let e = ENTITIES[i];
    let t = Tokens::of(ui.ctx());
    ui.strong(e.name);
    widgets::checkbox(ui, &mut p.read_only, "Read-only");
    let read_only = p.read_only;
    let mut visible = !p.hidden.contains(&i);
    Sections::new("gallery.list-items.selection")
        .capitals(false)
        .show(ui, |stack| {
            stack.section(Section::new("entity", "Entity"), |ui| {
                PropertyRow::new("Path").text(ui, e.path);
                PropertyRow::new("Archetype").text(ui, e.kind);
                PropertyRow::new("Points").text(ui, e.points.to_string());
                PropertyRow::new("Visible")
                    .read_only(read_only)
                    .widget(ui, egui::Checkbox::without_text(&mut visible));
            });
            stack.section(Section::new("visualizer", "Visualizer"), |ui| {
                PropertyRow::new("Color").show(ui, |ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                    ui.painter()
                        .rect_filled(r, t.radius.widget.min(2.0), e.color);
                    ui.label(format!(
                        "#{:02x}{:02x}{:02x}",
                        e.color.r(),
                        e.color.g(),
                        e.color.b()
                    ));
                });
                PropertyRow::new("Radius").read_only(read_only).widget(
                    ui,
                    egui::DragValue::new(&mut p.radius)
                        .speed(0.05)
                        .range(0.0..=10.0),
                );
                PropertyRow::new("Fill")
                    .read_only(read_only)
                    .widget(ui, egui::Checkbox::without_text(&mut p.fill));
                PropertyRow::new("Show labels at every zoom level").text(ui, "no");
            });
        });
    if visible == p.hidden.contains(&i) {
        flip(&mut p.hidden, i);
    }
}
