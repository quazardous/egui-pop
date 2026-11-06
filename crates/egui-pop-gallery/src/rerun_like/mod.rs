//! A Rerun-like viewer on egui-pop: the look and the layout of the Rerun
//! viewer (<https://rerun.io>), built from egui-pop's workbench, kit and
//! components, to see what egui-pop's application model lacks for such an
//! application (`docs/rerun-like.md`). Not pixel-perfect, and no Rerun
//! code: its theme is a kit made from the token values of Rerun's `re_ui`
//! (`examples/kits/rerun-like`, dark and light in one kit).
//!
//! - a top bar: the menus; at its end the recording, play and pause, the
//!   time, the panel toggles;
//! - the Blueprint on the left: the views and their entities, a filter;
//! - the viewport in the centre: four views as egui_stack tabs and splits
//!   (a time series, a camera image with boxes, a dataframe, a text log);
//! - the Selection on the right: the selected entity or view, in sections;
//! - the time panel at the bottom: a ruler, a cursor, play, a row of data
//!   per entity.
//!
//! Selecting anywhere (the tree, a view, the time panel) selects
//! everywhere, and hovering an entity highlights it in every view; the
//! selection is this module's own ([`Model`]), egui-pop having no
//! application-wide selection yet.
//!
//! Run it with `cargo run -p egui-pop-gallery --example rerun_like`.

pub mod data;
mod views;

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use egui_pop::api::app::{MENU_BAR_END, MENU_VIEW};
use egui_pop::api::layout_controls::{
    CMD_TOGGLE_PANEL, CMD_TOGGLE_PRIMARY_SIDE_BAR, CMD_TOGGLE_SECONDARY_SIDE_BAR,
};
use egui_pop::api::palette::CMD_PALETTE;
use egui_pop::api::workbench::{PANEL_INSPECTOR, PANEL_MAIN, PANEL_SIDE_BAR};
use egui_pop::model::keymap::Layer;
use egui_pop::model::layout::{Layout, Node};
use egui_pop::model::menus::MenuItem;
use egui_pop::model::panels::{Area, PanelSpec};
use egui_pop::prelude::*;
use egui_pop::tokens::kit::KitData;
use egui_pop::views::table::TableState;
use egui_pop::views::text_log::TextLogState;
use egui_pop::views::tree::TreeState;

use data::{ENTITIES, FRAMES, View};

/// The dark kit, Rerun's default look.
pub const DARK: &str = "rerun-like-dark";
/// Its light counterpart.
pub const LIGHT: &str = "rerun-like-light";

pub const PANEL_BLUEPRINT: PanelId = PanelId::from_static("rerun.blueprint");
pub const PANEL_SELECTION: PanelId = PanelId::from_static("rerun.selection");
pub const PANEL_TIME: PanelId = PanelId::from_static("rerun.time");

pub const CMD_PLAY: CommandId = CommandId::from_static("rerun.playback.toggle");
pub const CMD_STEP_BACK: CommandId = CommandId::from_static("rerun.playback.step_back");
pub const CMD_STEP_FORWARD: CommandId = CommandId::from_static("rerun.playback.step_forward");
pub const CMD_RESTART: CommandId = CommandId::from_static("rerun.playback.restart");
pub const CMD_LOOP: CommandId = CommandId::from_static("rerun.playback.loop");
pub const CMD_CLEAR_SELECTION: CommandId = CommandId::from_static("rerun.selection.clear");
pub const CMD_RESET: CommandId = CommandId::from_static("rerun.reset");

/// The panel of a view.
pub fn view_panel(view: View) -> PanelId {
    PanelId::new(format!(
        "rerun.view.{}",
        view.title().to_lowercase().replace(' ', "_")
    ))
    .expect("a valid panel id")
}

/// The kit: Rerun-like Dark, carrying Rerun-like Light as its
/// counterpart.
pub fn kit() -> KitData {
    egui_pop::tokens::kit!(
        "../../../../examples/kits/rerun-like",
        tokens = ["shared.tokens.yaml"]
    )
}

/// What is selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Entity(usize),
    View(View),
}

/// The viewer's state, shared by its panels and commands.
pub struct Model {
    /// The time cursor, in frames (fractional while playing).
    pub frame: f32,
    pub playing: bool,
    pub looping: bool,
    pub selected: Option<Item>,
    /// The entity under the pointer in the last pass, in any view.
    pub hovered: Option<usize>,
    hover_now: Option<usize>,
    pass: u64,
    /// Entities hidden from the views (the eye in the blueprint).
    pub hidden: HashSet<usize>,
    pub tree: TreeState<String>,
    tree_selection: Vec<String>,
    pub table: TableState<u32>,
    table_selection: Option<u32>,
    pub log: Vec<data::LogLine>,
    /// The text log view's filters, Follow and expanded lines.
    pub log_state: TextLogState,
    /// The hidden entities the log was last filtered for, sorted.
    log_hidden: Vec<usize>,
    image: Option<egui::TextureHandle>,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Model")
            .field("frame", &self.frame)
            .field("playing", &self.playing)
            .field("selected", &self.selected)
            .field("hovered", &self.hovered)
            .finish_non_exhaustive()
    }
}

impl Default for Model {
    fn default() -> Self {
        let mut tree = TreeState::new();
        for v in View::ALL {
            tree.set_expanded(&v.key(), true);
        }
        Model {
            frame: 120.0,
            playing: false,
            looping: true,
            selected: None,
            hovered: None,
            hover_now: None,
            pass: u64::MAX,
            hidden: HashSet::new(),
            tree,
            tree_selection: Vec::new(),
            table: TableState::new(),
            table_selection: None,
            log: data::log(),
            log_state: TextLogState::new(),
            log_hidden: Vec::new(),
            image: None,
        }
    }
}

impl Model {
    /// The frame the cursor is on.
    pub fn at(&self) -> u32 {
        (self.frame.floor().max(0.0) as u32).min(FRAMES)
    }

    /// Starts a pass: what was hovered in the last one becomes
    /// [`Model::hovered`]. Every panel calls it first; the first of a
    /// pass does the work.
    fn begin(&mut self, ctx: &egui::Context) {
        let pass = ctx.cumulative_pass_nr();
        if pass != self.pass {
            self.pass = pass;
            if self.hovered != self.hover_now {
                ctx.request_repaint();
            }
            self.hovered = self.hover_now.take();
        }
    }

    /// The pointer is over an entity in some view.
    pub fn hover(&mut self, entity: usize) {
        self.hover_now = Some(entity);
    }

    /// Selects an item, everywhere: the tree follows.
    pub fn select(&mut self, item: Option<Item>) {
        self.selected = item;
        let key = match item {
            Some(Item::Entity(e)) => ENTITIES[e].views.first().map(|v| entity_key(*v, e)),
            Some(Item::View(v)) => Some(v.key()),
            None => None,
        };
        let keys: Vec<String> = key.iter().cloned().collect();
        self.tree.set_selection(keys.clone(), key.clone());
        if let Some(k) = &key {
            self.tree.reveal(k);
        }
        self.tree_selection = keys;
    }

    pub fn is_selected(&self, entity: usize) -> bool {
        self.selected == Some(Item::Entity(entity))
    }

    /// How an entity stands out: selected, hovered, or neither.
    pub fn emphasis(&self, entity: usize) -> Emphasis {
        if self.is_selected(entity) {
            Emphasis::Selected
        } else if self.hovered == Some(entity) {
            Emphasis::Hovered
        } else {
            Emphasis::None
        }
    }

    /// Takes the tree's selection when the user changed it.
    fn follow_tree(&mut self) {
        let now: Vec<String> = self.tree.selection().keys().to_vec();
        if now != self.tree_selection {
            self.tree_selection.clone_from(&now);
            self.selected = now.first().and_then(|k| item_of_key(k));
        }
    }

    /// Takes the table's selection when the user changed it: a row moves
    /// the time cursor to its frame.
    fn follow_table(&mut self) {
        let now = self.table.selection().keys().first().copied();
        if now != self.table_selection {
            self.table_selection = now;
            if let Some(frame) = now {
                self.frame = frame as f32;
                self.playing = false;
            }
        }
    }

    /// Moves the cursor while playing.
    fn advance(&mut self, ctx: &egui::Context) {
        if !self.playing {
            return;
        }
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        self.frame += dt * data::FPS;
        if self.frame > FRAMES as f32 {
            if self.looping {
                self.frame = 0.0;
            } else {
                self.frame = FRAMES as f32;
                self.playing = false;
            }
        }
        ctx.request_repaint();
    }

    fn texture(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        self.image
            .get_or_insert_with(|| {
                ctx.load_texture(
                    "rerun.camera",
                    data::image(256, 160),
                    egui::TextureOptions::LINEAR,
                )
            })
            .clone()
    }
}

/// How an entity is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emphasis {
    None,
    Hovered,
    Selected,
}

/// An entity's key in the blueprint tree, under one of its views.
pub fn entity_key(view: View, entity: usize) -> String {
    format!("{}|{}", view.key(), ENTITIES[entity].path)
}

/// The item a tree key stands for.
pub fn item_of_key(key: &str) -> Option<Item> {
    match key.split_once('|') {
        Some((_, path)) => data::entity_of(path).map(Item::Entity),
        None => View::from_key(key).map(Item::View),
    }
}

type Shared = Rc<RefCell<Model>>;

/// The viewer as a plugin.
#[derive(Debug, Default)]
pub struct RerunLike {
    model: Shared,
}

impl RerunLike {
    /// The model, for tests.
    pub fn model(&self) -> Shared {
        self.model.clone()
    }
}

impl Plugin for RerunLike {
    fn id(&self) -> PluginId {
        PluginId::from_static("rerun_like")
    }

    fn title(&self) -> String {
        "Rerun-like viewer".into()
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let m = self.model.clone();
        ctx.panel(
            PanelSpec::new(PANEL_BLUEPRINT, "Blueprint", Area::StartSide)
                .icon("layout-left")
                .default_size(230.0)
                .min_size(160.0),
            move |ui, app| {
                padded(ui, |ui| {
                    let mut m = m.borrow_mut();
                    m.begin(ui.ctx());
                    views::blueprint(ui, app, &mut m);
                    m.follow_tree();
                });
            },
        )?;
        let m = self.model.clone();
        ctx.panel(
            PanelSpec::new(PANEL_SELECTION, "Selection", Area::EndSide)
                .icon("info")
                .default_size(270.0)
                .min_size(180.0),
            move |ui, _| {
                padded(ui, |ui| {
                    let mut m = m.borrow_mut();
                    m.begin(ui.ctx());
                    views::selection(ui, &mut m);
                });
            },
        )?;
        let m = self.model.clone();
        ctx.panel(
            PanelSpec::new(PANEL_TIME, "Time", Area::Bottom)
                .only_in(&[Area::Bottom])
                .default_size(220.0)
                .min_size(90.0),
            move |ui, app| {
                padded(ui, |ui| {
                    let mut m = m.borrow_mut();
                    m.begin(ui.ctx());
                    m.advance(ui.ctx());
                    views::time_panel(ui, app, &mut m);
                });
            },
        )?;
        for view in View::ALL {
            let m = self.model.clone();
            ctx.panel(
                PanelSpec::new(view_panel(view), view.title(), Area::Center)
                    .only_in(&[Area::Center]),
                move |ui, _| {
                    let mut m = m.borrow_mut();
                    m.begin(ui.ctx());
                    match view {
                        View::TimeSeries => views::time_series(ui, &mut m),
                        View::Camera => {
                            let tex = m.texture(ui.ctx());
                            views::camera(ui, &mut m, &tex);
                        }
                        View::Dataframe => {
                            views::dataframe(ui, &mut m);
                            m.follow_table();
                        }
                        View::Logs => padded(ui, |ui| views::logs(ui, &mut m)),
                    }
                },
            )?;
        }
        // The top bar's end, before the panel toggles: the recording,
        // playback and the time.
        let m = self.model.clone();
        ctx.block(
            zones::TOP_BAR_END,
            Block::add(BlockId::from_static("rerun.top_bar"), 0, move |ui, app| {
                let mut m = m.borrow_mut();
                m.begin(ui.ctx());
                views::top_bar(ui, app, &mut m);
            }),
        );
        self.commands(ctx)?;
        Ok(())
    }
}

impl RerunLike {
    fn commands(&self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let in_time = "focusedPanel == 'rerun.time'";
        let m = self.model.clone();
        ctx.command(
            CommandSpec::new(CMD_PLAY, "Toggle play/pause", "Playback")
                .description("Plays the recording from the time cursor, or pauses it.")
                .key("Space")
                .menu(MENU_VIEW, "4_playback", 0),
            move |call| {
                let mut m = m.borrow_mut();
                if !m.playing && m.at() >= FRAMES {
                    m.frame = 0.0;
                }
                m.playing = !m.playing;
                call.app.egui().request_repaint();
                Ok(serde_json::json!({ "playing": m.playing }))
            },
        )?;
        for (id, title, step, key) in [
            (CMD_STEP_BACK, "Step back one frame", -1.0, "Left"),
            (CMD_STEP_FORWARD, "Step forward one frame", 1.0, "Right"),
        ] {
            let m = self.model.clone();
            ctx.command(
                CommandSpec::new(id.clone(), title, "Playback")
                    .description("Moves the time cursor by one frame and pauses.")
                    .menu(MENU_VIEW, "4_playback", if step < 0.0 { 1 } else { 2 }),
                move |_| {
                    let mut m = m.borrow_mut();
                    m.playing = false;
                    m.frame = (m.frame.floor() + step).clamp(0.0, FRAMES as f32);
                    Ok(serde_json::json!({ "frame": m.at() }))
                },
            )?;
            ctx.keybinding(key, id, in_time, Layer::Default)?;
        }
        let m = self.model.clone();
        ctx.command(
            CommandSpec::new(CMD_RESTART, "Restart", "Playback")
                .description("Moves the time cursor to the first frame.")
                .menu(MENU_VIEW, "4_playback", 3),
            move |_| {
                m.borrow_mut().frame = 0.0;
                Ok(serde_json::Value::Null)
            },
        )?;
        ctx.keybinding("Home", CMD_RESTART, in_time, Layer::Default)?;
        let m = self.model.clone();
        ctx.command(
            CommandSpec::new(CMD_LOOP, "Toggle looping", "Playback")
                .description("Starts again from the first frame at the end of the recording.")
                .menu(MENU_VIEW, "4_playback", 4),
            move |_| {
                let mut m = m.borrow_mut();
                m.looping = !m.looping;
                Ok(serde_json::json!({ "looping": m.looping }))
            },
        )?;
        let m = self.model.clone();
        ctx.command(
            CommandSpec::new(CMD_CLEAR_SELECTION, "Clear selection", "Selection")
                .description("Selects nothing."),
            move |_| {
                m.borrow_mut().select(None);
                Ok(serde_json::Value::Null)
            },
        )?;
        let m = self.model.clone();
        ctx.command(
            CommandSpec::new(CMD_RESET, "Reset viewer", "Viewer")
                .description(
                    "Puts the panels back where they were, clears the selection and rewinds.",
                )
                .key("Ctrl+Shift+R")
                .menu(MENU_VIEW, "4_playback", 9),
            move |call| {
                call.app.set_layout(layout());
                let mut m = m.borrow_mut();
                m.select(None);
                m.frame = 0.0;
                m.playing = false;
                m.hidden.clear();
                Ok(serde_json::Value::Null)
            },
        )?;
        // Rerun's keys for what egui-pop already has.
        ctx.keybinding("Ctrl+P", CMD_PALETTE, "", Layer::Default)?;
        ctx.keybinding(
            "Ctrl+Shift+B",
            CMD_TOGGLE_PRIMARY_SIDE_BAR,
            "",
            Layer::Default,
        )?;
        ctx.keybinding(
            "Ctrl+Shift+S",
            CMD_TOGGLE_SECONDARY_SIDE_BAR,
            "",
            Layer::Default,
        )?;
        ctx.keybinding("Ctrl+Shift+T", CMD_TOGGLE_PANEL, "", Layer::Default)?;
        // The panel toggles at the end of the top bar, as Rerun has them.
        for (order, (command, shown)) in [
            (CMD_TOGGLE_PRIMARY_SIDE_BAR, "sideBarVisible"),
            (CMD_TOGGLE_PANEL, "panelVisible"),
            (CMD_TOGGLE_SECONDARY_SIDE_BAR, "auxiliaryBarVisible"),
        ]
        .into_iter()
        .enumerate()
        {
            ctx.menu_item(
                MenuItem::command(MENU_BAR_END, "9_layout", order as i32, command).checked(shown),
            )?;
        }
        Ok(())
    }
}

/// Draws a side or bottom panel's content inside a margin: the areas are
/// frameless, so each panel gives itself the room it wants.
fn padded(ui: &mut egui_pop::egui::Ui, add: impl FnOnce(&mut egui_pop::egui::Ui)) {
    let t = Tokens::of(ui.ctx());
    let m = |v: f32| v.round() as i8;
    egui_pop::egui::Frame::new()
        .inner_margin(egui_pop::egui::Margin::symmetric(
            m(t.space.s),
            m(t.space.xs),
        ))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            add(ui);
        });
}

/// The viewer's layout, the one it starts with and resets to: the
/// blueprint, the selection and the time panel around the viewport,
/// whose four views are a two by two grid of splits; the shell's own
/// panels closed.
pub fn layout() -> Layout {
    let tab = |v: View| Node::tab(view_panel(v));
    let column = |a: View, b: View, ratio: f32| Node::Split {
        vertical: true,
        ratio,
        a: Box::new(tab(a)),
        b: Box::new(tab(b)),
    };
    Layout {
        areas: vec![
            (Area::StartSide, Node::tab(PANEL_BLUEPRINT)),
            (
                Area::Center,
                Node::Split {
                    vertical: false,
                    ratio: 0.5,
                    a: Box::new(column(View::TimeSeries, View::Logs, 0.58)),
                    b: Box::new(column(View::Camera, View::Dataframe, 0.58)),
                },
            ),
            (Area::EndSide, Node::tab(PANEL_SELECTION)),
            (Area::Bottom, Node::tab(PANEL_TIME)),
        ],
        hidden: vec![PANEL_SIDE_BAR, PANEL_MAIN, PANEL_INSPECTOR],
        ..Layout::default()
    }
}

/// The application, before it is built: the default plugins, the views'
/// plugin, the viewer, the kit, Rerun-like Dark first (the light, dark
/// or system setting switches to its counterpart); its layout, every area
/// frameless (Rerun's panels run to the window's edges, a line between
/// them).
pub fn builder(viewer: RerunLike) -> AppBuilder {
    App::builder()
        .name("Rerun-like viewer")
        .layout(layout())
        .frameless(&Area::ALL)
        .plugins(DefaultPlugins)
        .plugin(egui_pop::views::ViewsPlugin)
        .plugin(viewer)
        .kit(kit())
        .theme(ThemeSpec::new(DARK))
}
