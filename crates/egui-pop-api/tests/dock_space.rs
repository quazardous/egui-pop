//! The areas share one egui_stack dock space: a tab dragged from one area
//! to another gets the drop zones it gets inside one (the middle of a
//! group, its four edges, a place in a tab bar), the panels' rules refuse
//! it with their reason, the move is a `workbench.move` step a plugin may
//! veto, and layouts saved before the space read into it.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use egui::{Pos2, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::steps::Before;
use egui_pop_api::workbench::{PANEL_MAIN, STEP_MOVE};
use egui_pop_api::zones::{self, Block};
use egui_pop_api::{App, Ctx, Plugin, PluginError};
use egui_pop_model::ids::{BlockId, PanelId, PluginId};
use egui_pop_model::layout::{Layout, Node};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_stack::DropZone;

const FILES: PanelId = PanelId::from_static("demo.files");
const SEARCH: PanelId = PanelId::from_static("demo.search");
const CONSOLE: PanelId = PanelId::from_static("demo.console");
const PINNED: PanelId = PanelId::from_static("demo.pinned");

struct Demo {
    /// Vetoes the console going to the start side bar.
    veto: bool,
    /// How many `workbench.move` steps were asked for.
    moves: Arc<AtomicUsize>,
}

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        let body = |text: &'static str| {
            move |ui: &mut egui::Ui, _: &egui_pop_api::AppHandle| {
                ui.label(text);
            }
        };
        ctx.panel(
            PanelSpec::new(FILES, "Files", Area::StartSide),
            body("files body"),
        )?
        .panel(
            PanelSpec::new(SEARCH, "Search", Area::StartSide),
            body("search body"),
        )?
        .panel(
            PanelSpec::new(CONSOLE, "Console", Area::Bottom),
            body("console body"),
        )?
        .panel(
            PanelSpec::new(PINNED, "Pinned", Area::Bottom).only_in(&[Area::Bottom]),
            body("pinned body"),
        )?;
        ctx.block(
            zones::MAIN,
            Block::add(BlockId::from_static("demo.main"), 0, |ui, _| {
                ui.label("main body");
            }),
        );
        let veto = self.veto;
        let moves = self.moves.clone();
        ctx.before(STEP_MOVE, move |_, args| {
            moves.fetch_add(1, Ordering::SeqCst);
            if veto && args["panel"] == "demo.console" && args["area"] == "start_side" {
                Before::Veto("the console stays down".into())
            } else {
                Before::Go
            }
        });
        Ok(())
    }
}

fn harness(veto: bool) -> (Harness<'static, App>, Arc<AtomicUsize>) {
    let moves = Arc::new(AtomicUsize::new(0));
    let app = App::builder()
        .plugin(Demo {
            veto,
            moves: moves.clone(),
        })
        .build(&egui::Context::default())
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run();
    (h, moves)
}

/// A tab's rect (the tab, a button, not a body's label).
fn tab(h: &Harness<'_, App>, title: &str) -> Rect {
    h.get_by_role_and_label(egui::accesskit::Role::Button, title)
        .rect()
}

fn area(h: &Harness<'_, App>, area: Area) -> Rect {
    h.state()
        .handle()
        .area_rect(area)
        .expect("the area is drawn")
}

fn tree(h: &Harness<'_, App>, area: Area) -> Option<Node> {
    h.state()
        .handle()
        .layout()
        .areas
        .into_iter()
        .find(|(a, _)| *a == area)
        .map(|(_, n)| n)
}

/// The panels of a tree holding one group of tabs.
fn group(node: Option<Node>) -> Option<Vec<PanelId>> {
    match node? {
        Node::Tabs { panels, .. } => Some(panels),
        _ => None,
    }
}

/// Presses on `from` and drags to `to` a frame at a time, as egui_stack
/// follows a tab's drag; held there.
fn drag(h: &mut Harness<'_, App>, from: Pos2, to: Pos2) {
    let mut send = |e: egui::Event| {
        h.input_mut().events.push(e);
        h.step();
    };
    send(egui::Event::PointerMoved(from));
    send(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    for i in 1..=8 {
        send(egui::Event::PointerMoved(
            from + (to - from) * (i as f32 / 8.0),
        ));
    }
    // Held there: egui_stack keeps a target it found for a moment (its
    // `max_preference_time`) before it takes the next one.
    for _ in 0..4 {
        h.step();
    }
}

fn release(h: &mut Harness<'_, App>, at: Pos2) {
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
}

#[test]
fn a_tab_dropped_in_the_middle_of_a_group_in_another_area_joins_its_tabs() {
    let (mut h, moves) = harness(false);
    let app = h.state().handle().clone();
    let from = tab(&h, "Console").center();
    let to = area(&h, Area::StartSide).center();
    drag(&mut h, from, to);
    let hint = app.drop_hint().expect("the drop zone while dragging");
    assert_eq!((hint.panel.clone(), hint.area), (CONSOLE, "start_side"));
    assert_eq!(hint.zone, DropZone::Center);
    assert_eq!(hint.index, Some(2), "after Files and Search");
    assert!(hint.refusal.is_none());
    assert!(hint.rect.contains(to), "the group the tab would join");
    let snap = app.debug_snapshot();
    assert_eq!(snap["workbench"]["drop_hint"]["zone"], "center");
    assert_eq!(snap["workbench"]["drop_hint"]["index"], 2);
    assert_eq!(snap["workbench"]["dragging"]["from"], "bottom");
    release(&mut h, to);
    assert!(app.drop_hint().is_none());
    assert_eq!(
        group(tree(&h, Area::StartSide)),
        Some(vec![FILES, SEARCH, CONSOLE])
    );
    assert_eq!(group(tree(&h, Area::Bottom)), Some(vec![PINNED]));
    assert_eq!(moves.load(Ordering::SeqCst), 1, "one workbench.move step");
    assert!(h.query_by_label("console body").is_some(), "shown there");
}

#[test]
fn each_edge_of_a_group_in_another_area_splits_it_on_that_side() {
    // The zone, then the split it makes: vertical (one above the other),
    // and whether the console comes first.
    let cases = [
        (DropZone::Left, false, true),
        (DropZone::Right, false, false),
        (DropZone::Top, true, true),
        (DropZone::Bottom, true, false),
    ];
    for (zone, vertical, first) in cases {
        let (mut h, moves) = harness(false);
        let app = h.state().handle().clone();
        let from = tab(&h, "Console").center();
        let r = area(&h, Area::StartSide);
        let to = match zone {
            DropZone::Left => egui::pos2(r.left() + 0.1 * r.width(), r.center().y),
            DropZone::Right => egui::pos2(r.right() - 0.1 * r.width(), r.center().y),
            DropZone::Top => egui::pos2(r.center().x, r.top() + 0.2 * r.height()),
            _ => egui::pos2(r.center().x, r.bottom() - 0.1 * r.height()),
        };
        drag(&mut h, from, to);
        let hint = app.drop_hint().expect("the drop zone while dragging");
        assert_eq!((hint.area, hint.zone), ("start_side", zone), "{zone:?}");
        assert_eq!(hint.index, None, "a split, not a place among tabs");
        assert!(hint.refusal.is_none());
        assert!(
            hint.rect.width() < r.width() || hint.rect.height() < r.height(),
            "{zone:?}: the half of the group a split gives"
        );
        release(&mut h, to);
        let Some(Node::Split {
            vertical: v, a, b, ..
        }) = tree(&h, Area::StartSide)
        else {
            panic!("{zone:?}: {:?}", tree(&h, Area::StartSide));
        };
        assert_eq!(v, vertical, "{zone:?}");
        let (console, rest) = if first { (a, b) } else { (b, a) };
        assert_eq!(group(Some(*console)), Some(vec![CONSOLE]), "{zone:?}");
        assert_eq!(group(Some(*rest)), Some(vec![FILES, SEARCH]), "{zone:?}");
        assert_eq!(moves.load(Ordering::SeqCst), 1, "{zone:?}");
    }
}

#[test]
fn a_tab_dropped_on_a_tab_bar_in_another_area_goes_between_its_tabs() {
    let (mut h, _) = harness(false);
    let app = h.state().handle().clone();
    let from = tab(&h, "Console").center();
    // On the Search tab: the console takes its place, before it.
    let to = tab(&h, "Search").center();
    drag(&mut h, from, to);
    let hint = app.drop_hint().expect("the drop zone while dragging");
    assert_eq!((hint.area, hint.zone), ("start_side", DropZone::Center));
    assert_eq!(hint.index, Some(1), "between Files and Search");
    release(&mut h, to);
    assert_eq!(
        group(tree(&h, Area::StartSide)),
        Some(vec![FILES, CONSOLE, SEARCH])
    );
}

#[test]
fn an_area_a_panel_stays_out_of_refuses_its_tab_with_the_reason() {
    let (mut h, moves) = harness(false);
    let app = h.state().handle().clone();
    let from = tab(&h, "Pinned").center();
    let to = area(&h, Area::StartSide).center();
    drag(&mut h, from, to);
    let hint = app.drop_hint().expect("the refused zone while dragging");
    assert_eq!((hint.panel.clone(), hint.area), (PINNED, "start_side"));
    let why = hint.refusal.expect("refused, with its reason");
    assert!(
        why.contains("Pinned") && why.contains("stays out of"),
        "{why}"
    );
    assert_eq!(
        app.overlay_rects()
            .iter()
            .find(|o| o.kind == egui_pop_api::app::OverlayKind::DropHint)
            .map(|o| o.id.clone()),
        Some("drop:demo.pinned:start_side".to_string())
    );
    release(&mut h, to);
    assert_eq!(app.layout().area_of(&PINNED), Some(Area::Bottom));
    assert_eq!(group(tree(&h, Area::StartSide)), Some(vec![FILES, SEARCH]));
    assert_eq!(moves.load(Ordering::SeqCst), 0, "no step for a refusal");
}

#[test]
fn a_vetoed_move_puts_the_dropped_tab_back() {
    let (mut h, moves) = harness(true);
    let app = h.state().handle().clone();
    let before = app.layout();
    let from = tab(&h, "Console").center();
    let to = area(&h, Area::StartSide).center();
    drag(&mut h, from, to);
    assert!(app.drop_hint().is_some_and(|d| d.refusal.is_none()));
    release(&mut h, to);
    h.run();
    assert_eq!(moves.load(Ordering::SeqCst), 1, "the step was asked");
    assert_eq!(app.layout().areas, before.areas, "back where it was");
    assert!(h.query_by_label("Console").is_some());
}

#[test]
fn a_closed_area_takes_a_tab_dropped_along_its_edge() {
    let (mut h, moves) = harness(false);
    let app = h.state().handle().clone();
    // No inspector block: the end side bar is closed.
    assert_eq!(tree(&h, Area::EndSide), None);
    let from = tab(&h, "Console").center();
    let centre = area(&h, Area::Center);
    let to = egui::pos2(centre.right() - 10.0, centre.center().y);
    drag(&mut h, from, to);
    let hint = app.drop_hint().expect("the closed area offered");
    assert_eq!((hint.area, hint.zone), ("end_side", DropZone::Center));
    assert_eq!(hint.index, Some(0));
    assert!(hint.rect.contains(to));
    release(&mut h, to);
    assert_eq!(group(tree(&h, Area::EndSide)), Some(vec![CONSOLE]));
    assert_eq!(moves.load(Ordering::SeqCst), 1);
    assert!(app.area_rect(Area::EndSide).is_some(), "drawn now");
}

#[test]
fn an_area_drawn_bare_takes_a_dropped_tab_into_its_tabs() {
    let (mut h, moves) = harness(false);
    let app = h.state().handle().clone();
    // The main zone alone in the centre, without a tab bar.
    assert_eq!(group(tree(&h, Area::Center)), Some(vec![PANEL_MAIN]));
    let from = tab(&h, "Console").center();
    let to = area(&h, Area::Center).center();
    drag(&mut h, from, to);
    let hint = app.drop_hint().expect("the bare area offered");
    assert_eq!((hint.area, hint.zone), ("center", DropZone::Center));
    assert_eq!(hint.index, Some(1), "after the main zone");
    release(&mut h, to);
    assert_eq!(
        group(tree(&h, Area::Center)),
        Some(vec![PANEL_MAIN, CONSOLE])
    );
    assert_eq!(moves.load(Ordering::SeqCst), 1);
    // Two tabs now: the main zone's tab shows.
    h.run();
    assert!(h.query_by_label("console body").is_some());
}

#[test]
fn a_detached_window_takes_a_tab_as_a_tab_even_at_its_edge() {
    let (mut h, moves) = harness(false);
    let app = h.state().handle().clone();
    // The console's window opens over the bottom area, below the tabs of
    // the start side bar.
    assert!(app.detach_panel(&CONSOLE).unwrap().ok());
    h.run();
    // kittest embeds the window in the main one: it sees the drag.
    let from = tab(&h, "Search").center();
    let body = h.get_by_label("console body").rect();
    let to = egui::pos2(body.left() + 2.0, body.center().y);
    drag(&mut h, from, to);
    let over = app.window_drop().expect("the window shows its drop zone");
    assert_eq!(over.zone, DropZone::Center, "a window holds tabs, no split");
    assert!(
        app.drop_hint().is_none(),
        "the window shows it, not an area"
    );
    release(&mut h, to);
    let layout = app.layout();
    let window = layout.window_of(&SEARCH).expect("in the window");
    assert_eq!(window.panel, CONSOLE);
    assert_eq!(window.tabs, vec![SEARCH]);
    assert_eq!(layout.area_of(&SEARCH), None, "not also in an area");
    assert_eq!(moves.load(Ordering::SeqCst), 0, "a join, not a move");
}

#[test]
fn a_layout_saved_before_the_space_reads_into_it_and_saves_the_same() {
    let (mut h, _) = harness(false);
    let app = h.state().handle().clone();
    // A version 1 document, from before the areas shared a space: the
    // start side bar split in two, the bottom panel's tabs.
    let v1 = r#"{
        "version": 1,
        "areas": [
            ["editors", {"kind": "tabs", "panels": ["app.main"], "active": 0}],
            ["primary_side_bar", {"kind": "split", "vertical": false, "ratio": 0.4,
                "a": {"kind": "tabs", "panels": ["demo.files"], "active": 0},
                "b": {"kind": "tabs", "panels": ["demo.search"], "active": 0}}],
            ["bottom_panel", {"kind": "tabs", "panels": ["demo.console", "demo.pinned"], "active": 1}]
        ],
        "auxiliary": []
    }"#;
    let old = Layout::from_json(v1).unwrap();
    assert!(app.set_layout(old.clone()).is_empty(), "nothing to repair");
    h.run();
    // Each area's tree is its dock's, as saved.
    let read = app.layout();
    assert_eq!(read.areas, old.areas);
    assert_eq!(Layout::from_json(&read.to_json()).unwrap(), read);
    assert!(h.query_by_label("files body").is_some());
    assert!(h.query_by_label("search body").is_some());
    // And a tab goes from one area into a group of another.
    let from = tab(&h, "Console").center();
    // The middle of the Search group: the right 60% of the area.
    let r = area(&h, Area::StartSide);
    let to = egui::pos2(r.left() + 0.7 * r.width(), r.center().y);
    drag(&mut h, from, to);
    release(&mut h, to);
    let Some(Node::Split { a, b, .. }) = tree(&h, Area::StartSide) else {
        panic!("{:?}", tree(&h, Area::StartSide));
    };
    assert_eq!(group(Some(*a)), Some(vec![FILES]));
    assert_eq!(
        group(Some(*b)),
        Some(vec![SEARCH, CONSOLE]),
        "joined Search"
    );
    assert_eq!(group(tree(&h, Area::Bottom)), Some(vec![PINNED]));
}
