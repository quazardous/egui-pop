//! The lists and tables page: a data table of 10 000 photos (sorted and
//! resized through the value store, cells drawn by the page, a veto on
//! one sort) and a list of keywords.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Color32, Ui};
use egui_pop::api::steps::Before;
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::widgets;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::views::Mode;
use egui_pop::views::list::{self, ListRows, ListState, ListView};
use egui_pop::views::steps::TABLE_SORT;
use egui_pop::views::table::{self, Column, DataTable, Sort, TableRows, TableState};

use super::section;
use crate::shell::SHELL;

pub const TABLE_PAGE: PluginId = PluginId::from_static("gallery.table");

/// Where the table keeps its sort and widths in the store.
pub const PREFIX: &str = "gallery/photos";

/// How many photos the table holds.
pub const PHOTOS: u32 = 10_000;

/// The context key the sort veto reads.
const VETO: &str = "galleryVetoRatingSort";

fn columns() -> Vec<Column> {
    vec![
        Column::new("name", "Name").width(180.0),
        Column::new("size", "Size").width(90.0),
        Column::new("date", "Date").width(110.0),
        Column::new("rating", "Rating").width(100.0),
    ]
}

/// A photo, as the application has it.
#[derive(Clone, Debug)]
pub struct Photo {
    pub id: u32,
    pub name: String,
    pub size_kb: u32,
    pub date: String,
    pub rating: u8,
}

fn photos() -> Vec<Photo> {
    (0..PHOTOS)
        .map(|i| {
            let h = i.wrapping_mul(2_654_435_761);
            Photo {
                id: i,
                name: format!("IMG_{i:05}.raw"),
                size_kb: 18_000 + h % 9_000,
                date: format!("2024-{:02}-{:02}", 1 + (h >> 8) % 12, 1 + (h >> 4) % 28),
                rating: (h % 6) as u8,
            }
        })
        .collect()
}

/// The photos, sorted as the table says.
#[derive(Debug)]
pub struct Photos(pub Vec<Photo>);

impl Photos {
    fn sort(&mut self, sort: Option<&Sort>) {
        let Some(s) = sort else {
            self.0.sort_by_key(|p| p.id);
            return;
        };
        match s.column.as_str() {
            "size" => self.0.sort_by_key(|p| (p.size_kb, p.id)),
            "date" => self
                .0
                .sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id))),
            "rating" => self.0.sort_by_key(|p| (p.rating, p.id)),
            _ => self.0.sort_by(|a, b| a.name.cmp(&b.name)),
        }
        if s.descending {
            self.0.reverse();
        }
    }
}

impl TableRows<u32> for Photos {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn key(&self, row: usize) -> u32 {
        self.0[row].id
    }

    fn label(&self, row: usize) -> String {
        self.0[row].name.clone()
    }

    fn cell(&mut self, ui: &mut Ui, row: usize, column: usize, _selected: bool) {
        let p = &self.0[row];
        match column {
            0 => {
                ui.label(&p.name);
            }
            1 => {
                ui.label(format!("{:.1} MB", f64::from(p.size_kb) / 1000.0));
            }
            2 => {
                ui.label(&p.date);
            }
            _ => {
                // Drawn by the application: five stars, painted.
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(80.0, 14.0), egui::Sense::hover());
                for i in 0..5u8 {
                    let c = rect.left_center() + egui::vec2(8.0 + f32::from(i) * 16.0, 0.0);
                    let color = if i < p.rating {
                        Color32::from_rgb(230, 180, 60)
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    ui.painter().circle_filled(c, 4.5, color);
                }
            }
        }
    }
}

/// Keywords, for the list.
#[derive(Debug)]
pub struct Keywords(pub Vec<String>);

impl ListRows<usize> for Keywords {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn key(&self, row: usize) -> usize {
        row
    }

    fn label(&self, row: usize) -> String {
        self.0[row].clone()
    }
}

/// The page's state.
#[derive(Debug)]
pub struct PageState {
    pub table: TableState<u32>,
    pub photos: Photos,
    pub list: ListState<usize>,
    pub keywords: Keywords,
    sorted_once: bool,
    pub log: Vec<String>,
}

impl Default for PageState {
    fn default() -> Self {
        let words = [
            "animal",
            "architecture",
            "autumn",
            "beach",
            "bird",
            "black and white",
            "bridge",
            "city",
            "cloud",
            "coast",
            "dog",
            "family",
            "flower",
            "forest",
            "garden",
            "lake",
            "landscape",
            "macro",
            "mountain",
            "night",
            "portrait",
            "rain",
            "river",
            "snow",
            "street",
            "sunset",
            "travel",
            "tree",
            "water",
            "winter",
        ];
        Self {
            table: TableState::new(),
            photos: Photos(photos()),
            list: ListState::new(),
            keywords: Keywords(words.iter().map(|w| w.to_string()).collect()),
            sorted_once: false,
            log: Vec::new(),
        }
    }
}

/// The page.
#[derive(Debug, Default)]
pub struct TablePage;

impl Plugin for TablePage {
    fn id(&self) -> PluginId {
        TABLE_PAGE
    }

    fn title(&self) -> String {
        "Lists and tables".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, egui_pop::views::steps::VIEWS]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let owner = ctx.owner().clone();
        table::declare(&ctx.app().store(), &owner, PREFIX, &columns())
            .map_err(|e| PluginError::Failed(e.to_string()))?;
        ctx.before(TABLE_SORT, |app, args| {
            if app.context().get(VETO) == "true" && args["column"] == "rating" {
                Before::Veto("sorting by rating is vetoed on this page".into())
            } else {
                Before::Go
            }
        });
        let page = Rc::new(RefCell::new(PageState::default()));
        ctx.story(
            Story::new(StoryId::from_static("table"), "Lists and tables", move |ui, app| {
                show(ui, app, &mut page.borrow_mut())
            })
            .description("A data table on egui_table: 10 000 rows, sorted and resized through the value store, cells drawn by the application; a list with type-ahead.")
            .group("Components", 78),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle, p: &mut PageState) {
    ui.label("Click a header to sort (up, down, none); drag a header's edge to resize. The sort and the widths live in the value store.");
    let mut veto = app.context().get(VETO) == "true";
    if widgets::checkbox(
        ui,
        &mut veto,
        "Veto sorting by rating (a before-hook on table.sort)",
    )
    .changed()
    {
        app.set_context(VETO, if veto { "true" } else { "false" });
    }
    section(ui, &format!("Photos ({PHOTOS})"));
    let store = app.store();
    let columns = columns();
    let out = DataTable::new("gallery.photos", &columns)
        .label("Photos")
        .mode(Mode::Multiple)
        .app(app)
        .store(&store, PREFIX)
        .height(260.0)
        .show(ui, &mut p.table, &mut p.photos);
    if out.sorted || !p.sorted_once {
        p.photos.sort(p.table.sort());
        p.sorted_once = true;
    }
    if out.sorted {
        p.log.push(format!(
            "table.sort {}",
            match Sort::encode(p.table.sort()) {
                s if s.is_empty() => "(none)".to_string(),
                s => s,
            }
        ));
    }
    for s in out.signals {
        p.log.push(describe(&s));
    }
    ui.label(format!(
        "Sorted by: {}  ·  {} selected",
        p.table
            .sort()
            .map(|s| format!(
                "{}{}",
                s.column,
                if s.descending { " (descending)" } else { "" }
            ))
            .unwrap_or_else(|| "nothing".into()),
        p.table.selection().len()
    ));
    section(ui, "Keywords");
    let out = ListView::new("gallery.keywords")
        .label("Keywords")
        .mode(Mode::Multiple)
        .app(app)
        .height(140.0)
        .show(ui, &mut p.list, &mut p.keywords);
    for s in out.signals {
        p.log.push(describe(&s));
    }
    section(ui, "Signals");
    if p.log.is_empty() {
        ui.weak("Nothing yet.");
    }
    let skip = p.log.len().saturating_sub(4);
    for line in &p.log[skip..] {
        ui.monospace(line);
    }
}

fn describe<K: std::fmt::Debug>(s: &list::Signal<K>) -> String {
    match s {
        list::Signal::Select { keys, .. } => format!("select {keys:?}"),
        list::Signal::Vetoed { step, reason } => format!("{step} vetoed: {reason}"),
    }
}
