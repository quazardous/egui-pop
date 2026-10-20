//! The photo library page: 10 000 synthetic images in a thumbnail grid
//! and a filmstrip sharing its selection, thumbnails arriving a pass
//! after they are asked for (some never: placeholders), ratings, labels,
//! flags, tags and stacks owned by the page, a search field with filter
//! chips, the marks of the image under the cursor, and a comparison.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use egui::{Color32, ColorImage, TextureHandle, TextureOptions, Ui};
use egui_pop::api::stories::Story;
use egui_pop::api::{AppHandle, Ctx, Plugin, PluginError};
use egui_pop::core::widgets;
use egui_pop::model::ids::{PluginId, StoryId};
use egui_pop::views::library::compare::{CompareState, CompareView, Pane};
use egui_pop::views::library::filter::{Candidate, Filter, SearchField, SearchState};
use egui_pop::views::library::marks::{ColorLabels, FlagToggle, RatingStars};
use egui_pop::views::library::tags::{TagInput, TagInputState};
use egui_pop::views::library::{
    Badge, Change, ColorLabel, CullKeymap, Flag, Labels, LibraryItems, LibraryState, Marks, Signal,
    Stack, Thumb, ThumbGrid, ThumbSource, Tone, change,
};

use super::section;
use crate::shell::SHELL;

pub const LIBRARY_PAGE: PluginId = PluginId::from_static("gallery.library");

/// The library's name in steps and commands.
pub const LIBRARY: &str = "gallery.library";

/// How many images the page holds.
pub const IMAGES: u32 = 10_000;

const KEYWORDS: [&str; 10] = [
    "beach", "city", "family", "forest", "mountain", "night", "portrait", "sea", "snow", "sunset",
];

/// An image, as the page (the application) has it.
#[derive(Clone, Debug)]
pub struct Photo {
    pub id: u32,
    pub name: String,
    pub marks: Marks,
    pub tags: Vec<String>,
    pub date: String,
    /// Its stack's top and size.
    pub stack: Option<(u32, usize)>,
}

fn photos() -> Vec<Photo> {
    (0..IMAGES)
        .map(|i| {
            let h = i.wrapping_mul(2_654_435_761);
            // Every fiftieth image starts a burst of four, stacked.
            let stack = match i % 50 {
                10..=13 => Some((i - i % 50 + 10, 4)),
                _ => None,
            };
            let labels = match h % 11 {
                0 => Labels::NONE.with(ColorLabel::Red, true),
                1 => Labels::NONE.with(ColorLabel::Green, true),
                2 => Labels::NONE
                    .with(ColorLabel::Blue, true)
                    .with(ColorLabel::Yellow, true),
                _ => Labels::NONE,
            };
            Photo {
                id: i,
                name: format!("DSC_{i:05}"),
                marks: Marks {
                    rating: ((h >> 3) % 6) as u8,
                    labels,
                    flag: match (h >> 7) % 9 {
                        0 => Flag::Pick,
                        1 => Flag::Reject,
                        _ => Flag::None,
                    },
                },
                tags: vec![KEYWORDS[(h >> 11) as usize % KEYWORDS.len()].to_string()],
                date: format!("2024-{:02}-{:02}", 1 + (h >> 8) % 12, 1 + (h >> 4) % 28),
                stack,
            }
        })
        .collect()
}

/// The images in view: all of them, or those the filter keeps (a
/// filter flattens the stacks).
struct Shown<'a> {
    photos: &'a [Photo],
    shown: &'a [u32],
    stacks: bool,
    revision: u64,
}

impl LibraryItems<u32> for Shown<'_> {
    fn len(&self) -> usize {
        self.shown.len()
    }

    fn key(&self, i: usize) -> u32 {
        self.shown[i]
    }

    fn label(&self, i: usize) -> String {
        self.photos[self.shown[i] as usize].name.clone()
    }

    fn marks(&self, i: usize) -> Marks {
        self.photos[self.shown[i] as usize].marks
    }

    fn stack(&self, i: usize) -> Option<Stack<u32>> {
        let p = &self.photos[self.shown[i] as usize];
        p.stack
            .filter(|_| self.stacks)
            .map(|(top, count)| Stack { top, count })
    }

    fn badges(&self, i: usize) -> Vec<Badge> {
        let id = self.shown[i];
        let mut out = Vec::new();
        if id.is_multiple_of(7) {
            out.push(Badge::new("RAW"));
        }
        if id.is_multiple_of(23) {
            out.push(Badge::new("HDR").tone(Tone::Info));
        }
        out
    }

    fn revision(&self) -> u64 {
        self.revision
    }
}

/// Thumbnails: eight painted gradients, each ready the pass after it is
/// asked for; every 17th image never arrives (a skeleton), every 13th
/// shows its average colour first.
struct Thumbs {
    textures: Vec<TextureHandle>,
    asked: RefCell<HashSet<u32>>,
    ready: RefCell<HashSet<u32>>,
    new: Cell<bool>,
}

fn gradient(k: usize) -> ColorImage {
    let a = [
        [214, 120, 72],
        [70, 130, 180],
        [96, 160, 90],
        [200, 170, 70],
        [140, 90, 170],
        [60, 70, 90],
        [220, 200, 180],
        [40, 120, 120],
    ][k % 8];
    let (w, h) = (96usize, 64usize);
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let t = (x + y) as f32 / (w + h) as f32;
            let c = |v: u8| (f32::from(v) * (0.45 + 0.75 * t)).min(255.0) as u8;
            px.push(Color32::from_rgb(c(a[0]), c(a[1]), c(a[2])));
        }
    }
    ColorImage::new([w, h], px)
}

fn average(id: u32) -> Color32 {
    let v = 60 + (id % 5) as u8 * 20;
    Color32::from_rgb(v, v / 2 + 40, 110)
}

impl ThumbSource<u32> for Thumbs {
    fn thumb(&self, key: &u32, _size: u32) -> Thumb {
        if key % 17 == 5 {
            return Thumb::Pending { average: None };
        }
        if self.ready.borrow().contains(key) {
            let t = &self.textures[*key as usize % self.textures.len()];
            return Thumb::Ready {
                texture: t.id(),
                size: [96, 64],
            };
        }
        if self.asked.borrow_mut().insert(*key) {
            self.new.set(true);
        }
        Thumb::Pending {
            average: (key % 13 == 3).then(|| average(*key)),
        }
    }

    fn wanted(&self, keys: &[u32], _size: u32) {
        // What was asked last pass arrives now.
        let mut ready = self.ready.borrow_mut();
        for k in keys {
            if self.asked.borrow().contains(k) {
                ready.insert(*k);
            }
        }
    }
}

/// The page's state.
pub struct PageState {
    photos: Vec<Photo>,
    shown: Vec<u32>,
    filter: Filter,
    revision: u64,
    state: LibraryState<u32>,
    search: SearchState,
    tags: TagInputState,
    compare: CompareState,
    comparing: bool,
    synced: bool,
    auto_advance: bool,
    best: Option<u32>,
    thumbs: Option<Thumbs>,
    log: Vec<String>,
}

impl std::fmt::Debug for PageState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageState")
            .field("shown", &self.shown.len())
            .field("filter", &self.filter)
            .finish_non_exhaustive()
    }
}

impl Default for PageState {
    fn default() -> Self {
        let photos = photos();
        Self {
            shown: photos.iter().map(|p| p.id).collect(),
            photos,
            filter: Filter::default(),
            revision: 0,
            state: LibraryState::new(LIBRARY),
            search: SearchState::new(),
            tags: TagInputState::default(),
            compare: CompareState::new(),
            comparing: false,
            synced: true,
            auto_advance: false,
            best: None,
            thumbs: None,
            log: Vec::new(),
        }
    }
}

impl PageState {
    fn refilter(&mut self) {
        let f = &self.filter;
        self.shown = self
            .photos
            .iter()
            .filter(|p| {
                f.matches(&Candidate {
                    name: &p.name,
                    marks: p.marks,
                    tags: &p.tags,
                    date: Some(&p.date),
                })
            })
            .map(|p| p.id)
            .collect();
        self.revision += 1;
    }

    /// Applies a signal to the page's images; whether marks changed.
    fn apply(&mut self, s: &Signal<u32>) -> bool {
        let mut each = |keys: &[u32], f: &dyn Fn(&mut Photo)| {
            for k in keys {
                if let Some(p) = self.photos.get_mut(*k as usize) {
                    f(p);
                }
            }
        };
        match s {
            Signal::Rate { keys, rating } => each(keys, &|p| p.marks.rating = *rating),
            Signal::Label { keys, label, on } => {
                each(keys, &|p| p.marks.labels = p.marks.labels.with(*label, *on))
            }
            Signal::Flag { keys, flag } => each(keys, &|p| p.marks.flag = *flag),
            Signal::Tag { keys, add, remove } => each(keys, &|p| {
                p.tags.retain(|t| !remove.contains(t));
                for t in add {
                    if !p.tags.contains(t) {
                        p.tags.push(t.clone());
                    }
                }
            }),
            _ => return false,
        }
        true
    }

    fn note(&mut self, s: &Signal<u32>) {
        let line = match s {
            Signal::Select { keys, .. } => format!("library.select: {} selected", keys.len()),
            Signal::Rate { keys, rating } => format!("library.rate: {} to {rating}", keys.len()),
            Signal::Label { keys, label, on } => format!(
                "library.label: {} {} {}",
                keys.len(),
                if *on { "+" } else { "-" },
                label.name()
            ),
            Signal::Flag { keys, flag } => format!("library.flag: {} {}", keys.len(), flag.name()),
            Signal::Tag { keys, add, remove } => {
                format!("library.tag: {} +{add:?} -{remove:?}", keys.len())
            }
            Signal::Stack { top, expanded } => format!(
                "library.stack: {top} {}",
                if *expanded { "expanded" } else { "collapsed" }
            ),
            Signal::Activate { key } => format!("activate {key}"),
            Signal::Zoom { size } => format!("zoom {size:.0}"),
            Signal::Vetoed { step, reason } => format!("{step} vetoed: {reason}"),
        };
        self.log.push(line);
        if self.log.len() > 50 {
            self.log.remove(0);
        }
    }

    fn handle(&mut self, signals: Vec<Signal<u32>>) {
        let mut marked = false;
        for s in &signals {
            marked |= self.apply(s);
            self.note(s);
        }
        if marked && !self.filter.is_empty() {
            self.refilter();
        }
    }

    /// What the marks panel acts on: the selection, else the cursor.
    fn targets(&self) -> Vec<u32> {
        let sel = self.state.selection();
        if sel.is_empty() {
            sel.cursor().copied().into_iter().collect()
        } else {
            sel.keys().to_vec()
        }
    }
}

/// The page.
#[derive(Debug, Default)]
pub struct LibraryPage;

impl Plugin for LibraryPage {
    fn id(&self) -> PluginId {
        LIBRARY_PAGE
    }

    fn title(&self) -> String {
        "Photo library".into()
    }

    fn requires(&self) -> Vec<PluginId> {
        vec![SHELL, egui_pop::views::library::plugin::LIBRARY]
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let page = Rc::new(RefCell::new(PageState::default()));
        ctx.story(
            Story::new(StoryId::from_static("library"), "Photo library", move |ui, app| {
                show(ui, app, &mut page.borrow_mut())
            })
            .description("A photo library for culling: 10 000 images in a virtualised thumbnail grid and a filmstrip, ratings, colour labels, flags and stacks, culling keys with auto-advance, a search field with filter chips, tags, and a comparison of two to four images.")
            .group("Components", 79),
        )?;
        Ok(())
    }
}

fn show(ui: &mut Ui, app: &AppHandle, p: &mut PageState) {
    if p.thumbs.is_none() {
        let textures = (0..8)
            .map(|k| {
                ui.ctx().load_texture(
                    format!("gallery-library-{k}"),
                    gradient(k),
                    TextureOptions::LINEAR,
                )
            })
            .collect();
        p.thumbs = Some(Thumbs {
            textures,
            asked: RefCell::default(),
            ready: RefCell::default(),
            new: Cell::new(false),
        });
    }
    ui.label("Click an image, then 0 to 5 rate it, P picks, X rejects, U unflags, 6 to 9 toggle a label, S opens a stack, + and - zoom. Drag across the cells to select with a band.");
    ui.horizontal(|ui| {
        let out = SearchField::new(LIBRARY)
            .label("Search images")
            .app(app)
            .width(220.0)
            .show(ui, &mut p.search);
        if let Some(f) = out.changed {
            p.filter = f;
            p.refilter();
        }
        widgets::checkbox(ui, &mut p.auto_advance, "Auto-advance");
        if widgets::selectable_label(ui, !p.comparing, "Grid").clicked() {
            p.comparing = false;
        }
        if widgets::selectable_label(ui, p.comparing, "Compare").clicked() {
            p.comparing = true;
        }
    });
    let thumbs = p.thumbs.take().expect("made above");
    if p.comparing {
        compare(ui, p, &thumbs);
    } else {
        library(ui, app, p, &thumbs);
    }
    if thumbs.new.replace(false) {
        // Thumbnails asked for arrive at the next pass.
        ui.ctx().request_repaint();
    }
    p.thumbs = Some(thumbs);
    section(ui, "Signals");
    if p.log.is_empty() {
        ui.weak("Nothing yet.");
    }
    let skip = p.log.len().saturating_sub(3);
    for line in &p.log[skip..] {
        ui.monospace(line);
    }
}

fn library(ui: &mut Ui, app: &AppHandle, p: &mut PageState, thumbs: &Thumbs) {
    section(ui, &format!("Images ({} of {IMAGES})", p.shown.len()));
    let keymap = CullKeymap::lightroom().auto_advance(p.auto_advance);
    let signals = {
        let items = Shown {
            photos: &p.photos,
            shown: &p.shown,
            stacks: p.filter.is_empty(),
            revision: p.revision,
        };
        let mut signals = ThumbGrid::new("grid")
            .label("Images")
            .app(app)
            .keymap(keymap.clone())
            .height(300.0)
            .show(ui, &mut p.state, &items, thumbs)
            .signals;
        signals.extend(
            ThumbGrid::filmstrip("strip")
                .app(app)
                .keymap(keymap)
                .height(96.0)
                .show(ui, &mut p.state, &items, thumbs)
                .signals,
        );
        signals
    };
    p.handle(signals);
    // The marks of the image under the cursor, edited for the selection.
    section(ui, "Marks");
    let Some(cursor) = p.state.selection().cursor().copied() else {
        ui.weak("No image under the cursor.");
        return;
    };
    let photo = p.photos[cursor as usize].clone();
    let targets = p.targets();
    let mut changes = Vec::new();
    ui.horizontal(|ui| {
        ui.label(&photo.name);
        if let Some(r) = RatingStars::new(photo.marks.rating).show(ui).changed {
            changes.push(Change::Rate {
                keys: targets.clone(),
                rating: r,
            });
        }
        if let Some(l) = ColorLabels::new(photo.marks.labels).show(ui).changed {
            for label in ColorLabel::ALL {
                if l.has(label) != photo.marks.labels.has(label) {
                    changes.push(Change::Label {
                        keys: targets.clone(),
                        label,
                        on: l.has(label),
                    });
                }
            }
        }
        if let Some(f) = FlagToggle::new(photo.marks.flag).show(ui).changed {
            changes.push(Change::Flag {
                keys: targets.clone(),
                flag: f,
            });
        }
    });
    let known: Vec<String> = KEYWORDS.iter().map(|k| k.to_string()).collect();
    let out =
        TagInput::new("gallery.library.tags")
            .source(&known)
            .show(ui, &mut p.tags, &photo.tags);
    if !out.added.is_empty() || !out.removed.is_empty() {
        changes.push(Change::Tag {
            keys: targets,
            add: out.added,
            remove: out.removed,
        });
    }
    let signals = changes
        .into_iter()
        .map(|c| change(Some(app), LIBRARY, c))
        .collect();
    p.handle(signals);
}

fn compare(ui: &mut Ui, p: &mut PageState, thumbs: &Thumbs) {
    section(ui, "Compare");
    widgets::checkbox(ui, &mut p.synced, "Sync zoom and pan");
    let sel = p.state.selection().keys();
    let keys: Vec<u32> = if (2..=4).contains(&sel.len()) {
        sel.to_vec()
    } else {
        p.shown.iter().take(3).copied().collect()
    };
    let panes: Vec<Pane<u32>> = keys
        .iter()
        .map(|k| Pane::new(*k, p.photos[*k as usize].name.clone()))
        .collect();
    // The comparison's images are wanted now.
    thumbs.wanted(&keys, 512);
    for k in &keys {
        let _ = thumbs.thumb(k, 512);
    }
    let out = CompareView::new("gallery.compare")
        .synced(p.synced)
        .best(p.best)
        .height(360.0)
        .show(ui, &mut p.compare, &panes, thumbs);
    if let Some(k) = out.best {
        p.best = Some(k);
        p.log.push(format!("best: {}", p.photos[k as usize].name));
    }
}
