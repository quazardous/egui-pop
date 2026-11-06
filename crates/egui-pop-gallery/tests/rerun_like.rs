//! The Rerun-like example (`docs/rerun-like.md`): its kit is clean and
//! readable in both variants, the light or dark setting switches them,
//! its keys clash with nothing, a selection made anywhere shows
//! everywhere, and its two snapshots (`rerun-like-dark`,
//! `rerun-like-light`).

use std::cell::RefCell;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::api::App;
use egui_pop::api::workbench::{PANEL_INSPECTOR, PANEL_MAIN, PANEL_SIDE_BAR};
use egui_pop::model::commands::Origin;
use egui_pop::theme::ThemeSpec;
use egui_pop::tokens::contrast::{self, Level};
use egui_pop::tokens::{Themes, kit};
use egui_pop_gallery::rerun_like::data::{ENTITIES, View};
use egui_pop_gallery::rerun_like::{self, DARK, Item, LIGHT, Model, RerunLike};
use egui_pop_test::SharedGpu as _;

const SIZE: egui::Vec2 = egui::vec2(1280.0, 800.0);

fn viewer(spec: ThemeSpec) -> (Harness<'static, App>, Rc<RefCell<Model>>) {
    let plugin = RerunLike::default();
    let model = plugin.model();
    let app = rerun_like::builder(plugin)
        .theme(spec)
        .build(&egui::Context::default())
        .expect("the viewer builds");
    let h = Harness::builder()
        .with_size(SIZE)
        .with_max_steps(8)
        .shared_wgpu()
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    egui_pop::core::motion::freeze(&h.ctx, 0.0);
    (h, model)
}

#[test]
fn the_kit_is_clean_and_readable() {
    let k = rerun_like::kit();
    let problems = kit::check(&k);
    assert!(problems.is_empty(), "{problems:?}");
    // One kit, both variants, each the other's counterpart.
    assert_eq!(k.name, DARK);
    assert_eq!(k.counterpart.as_ref().map(|c| c.name.as_str()), Some(LIGHT));
    let mut themes = Themes::default();
    themes.add_kit(&k);
    assert_eq!(themes.counterpart_of(DARK), Some(LIGHT));
    assert_eq!(themes.counterpart_of(LIGHT), Some(DARK));
    for name in [DARK, LIGHT] {
        let tokens = themes.resolve(&ThemeSpec::new(name));
        let failures = contrast::check(&tokens, Level::Normal);
        assert!(failures.is_empty(), "{name}: {failures:#?}");
        // Their charts' hues, fitted to their surfaces, stay apart.
        let apart = contrast::check_category(&tokens);
        assert!(apart.is_empty(), "{name}: {apart:#?}");
    }
}

#[test]
fn the_light_or_dark_setting_switches_the_variant() {
    let (mut h, _) = viewer(ThemeSpec::new(DARK));
    h.run();
    let base = |h: &Harness<'static, App>| {
        egui_pop::theme::Tokens::of(&h.ctx)
            .resolved()
            .spec()
            .base
            .clone()
    };
    let service = h.state().handle().theme();
    assert_eq!(service.counterpart().as_deref(), Some(LIGHT));
    let names: Vec<String> = service.kits().iter().map(|k| k.name.clone()).collect();
    assert_eq!(names, [DARK, LIGHT]);
    service.set_preference(egui::ThemePreference::Light);
    h.run();
    assert_eq!(base(&h), LIGHT);
    service.set_preference(egui::ThemePreference::Dark);
    h.run();
    assert_eq!(base(&h), DARK);
}

#[test]
fn the_viewer_builds_clean() {
    let (mut h, _) = viewer(ThemeSpec::new(DARK));
    h.run();
    let app = h.state();
    assert!(
        app.handle().diagnostics().is_empty(),
        "{:#?}",
        app.handle().diagnostics()
    );
    egui_pop_test::assert_keymap_clean(app);
    // The viewer's panels are where Rerun has them; the shell's own are not shown.
    let layout = app.handle().layout();
    assert_eq!(layout.areas.len(), rerun_like::layout().areas.len());
    for (area, node) in &rerun_like::layout().areas {
        let now = layout.areas.iter().find(|(a, _)| a == area).map(|(_, n)| n);
        assert_eq!(now, Some(node), "{area:?}");
    }
    for shell in [PANEL_SIDE_BAR, PANEL_MAIN, PANEL_INSPECTOR] {
        assert!(!app.handle().panel_visible(&shell), "{shell}");
    }
}

#[test]
fn a_selection_in_the_tree_shows_everywhere() {
    let (mut h, model) = viewer(ThemeSpec::new(DARK));
    h.run();
    // The user picks /camera/detections in the blueprint.
    let key = rerun_like::entity_key(View::Camera, 4);
    model
        .borrow_mut()
        .tree
        .set_selection([key.clone()], Some(key));
    h.run();
    assert_eq!(model.borrow().selected, Some(Item::Entity(4)));
    // The selection panel shows it.
    h.get_by_label("Archetype");
    assert!(h.query_all_by_label("Boxes2D").count() >= 1);
    // A click on a stream of the time panel selects it, and the tree follows.
    let path = ENTITIES[0].path;
    let rows: Vec<_> = h.query_all_by_label(path).collect();
    assert!(!rows.is_empty());
    model.borrow_mut().select(Some(Item::Entity(0)));
    h.run();
    let tree = model.borrow().tree.selection().keys().to_vec();
    assert_eq!(tree, vec![rerun_like::entity_key(View::TimeSeries, 0)]);
}

#[test]
fn playback_runs_from_commands() {
    let (mut h, model) = viewer(ThemeSpec::new(DARK));
    h.run();
    let start = model.borrow().at();
    let app = h.state_mut();
    let _ = app.run(
        &rerun_like::CMD_STEP_FORWARD,
        serde_json::Value::Null,
        Origin::User,
    );
    assert_eq!(model.borrow().at(), start + 1);
    let _ = app.run(&rerun_like::CMD_PLAY, serde_json::Value::Null, Origin::User);
    assert!(model.borrow().playing);
    let _ = app.run(
        &rerun_like::CMD_RESET,
        serde_json::Value::Null,
        Origin::User,
    );
    assert!(!model.borrow().playing);
    assert_eq!(model.borrow().at(), 0);
}

#[test]
fn snapshots() {
    let mut results = egui_kittest::SnapshotResults::new();
    for (name, spec) in [
        ("rerun-like-dark", egui::ThemePreference::Dark),
        ("rerun-like-light", egui::ThemePreference::Light),
    ] {
        // The kit's variants, as the light or dark setting picks them.
        let (mut h, model) = viewer(ThemeSpec::new(DARK));
        h.state().handle().theme().set_preference(spec);
        h.run();
        // Something selected, as a user would have.
        model.borrow_mut().select(Some(Item::Entity(1)));
        h.run();
        egui_pop_test::assert_all_named(&h);
        results.add(h.try_snapshot(name));
    }
}

#[test]
fn the_plot_takes_rerun_s_grid_and_cursor_head() {
    use egui_pop::core::kit::{Kit, State};
    use egui_pop::views::plot::{CursorHead, CursorLook, GridLook};
    // The default kit: the grid in the muted text at egui_plot's fade, a
    // cursor without a head, the same accent under the pointer.
    let ctx = egui::Context::default();
    egui_pop::theme::install_with(&ctx, &Themes::default(), &ThemeSpec::new("pop-dark"));
    let t = egui_pop::theme::Tokens::of(&ctx);
    let grid = GridLook::of(&ctx);
    assert_eq!((grid.color, grid.fade), (t.text.muted, 0.5));
    assert_eq!(CursorLook::of(&ctx).head, CursorHead::None);
    let kit = Kit::of(&ctx);
    assert_eq!(
        kit.plot_cursor.style(State::HOVER).fill,
        kit.plot_cursor.style(State::REST).fill
    );
    // The Rerun-like kit: Rerun's grid colour and fade, its triangle, the
    // strong text under the pointer.
    for name in [DARK, LIGHT] {
        let (mut h, _) = viewer(ThemeSpec::new(name));
        h.run();
        let t = egui_pop::theme::Tokens::of(&h.ctx);
        let grid = GridLook::of(&h.ctx);
        assert_eq!(Some(grid.color), t.color("gray.4"), "{name}");
        assert_eq!(grid.fade, 0.3, "{name}");
        let look = CursorLook::of(&h.ctx);
        assert_eq!((look.head, look.head_size), (CursorHead::Triangle, 10.0));
        let kit = Kit::of(&h.ctx);
        assert_eq!(kit.plot_cursor.style(State::HOVER).fill, t.text.strong);
        assert_eq!(kit.plot_cursor.style(State::REST).fill, t.text.default);
    }
}

#[test]
fn the_time_series_cursor_is_dragged() {
    let (mut h, model) = viewer(ThemeSpec::new(DARK));
    h.run();
    let start = model.borrow().at();
    let plot = h.get_by_label("Time series plot").rect();
    let a = h.get_by_label("Time cursor").rect().center();
    let b = a + egui::vec2(plot.width() * 0.3, 0.0);
    h.drag_at(a);
    h.run();
    for i in 1..=4 {
        h.hover_at(a + (b - a) * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(b);
    h.run();
    let now = model.borrow().at();
    assert!(now > start + 30, "from frame {start} to {now}");
    assert!(!model.borrow().playing);
}

/// Three empty panels, framed as an application's are (not frameless):
/// one in the start side area, two side by side in the centre.
mod audit {
    use egui_pop::api::workbench::{PANEL_INSPECTOR, PANEL_MAIN, PANEL_SIDE_BAR};
    use egui_pop::api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
    use egui_pop::model::ids::{PanelId, PluginId};
    use egui_pop::model::layout::{Layout, Node};
    use egui_pop::model::panels::{Area, PanelSpec};

    pub const SIDE: &str = "Audit side";
    pub const LEFT: &str = "Audit left";
    pub const RIGHT: &str = "Audit right";

    const P_SIDE: PanelId = PanelId::from_static("audit.side");
    const P_LEFT: PanelId = PanelId::from_static("audit.left");
    const P_RIGHT: PanelId = PanelId::from_static("audit.right");

    struct Panels;

    impl Plugin for Panels {
        fn id(&self) -> PluginId {
            PluginId::from_static("audit")
        }

        fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
            ctx.panel(PanelSpec::new(P_SIDE, SIDE, Area::StartSide), |_, _| {})?
                .panel(PanelSpec::new(P_LEFT, LEFT, Area::Center), |_, _| {})?
                .panel(PanelSpec::new(P_RIGHT, RIGHT, Area::Center), |_, _| {})?;
            Ok(())
        }
    }

    pub fn app(theme: &str) -> App {
        App::builder()
            .layout(Layout {
                areas: vec![
                    (Area::StartSide, Node::tab(P_SIDE)),
                    (
                        Area::Center,
                        Node::Split {
                            vertical: false,
                            ratio: 0.5,
                            a: Box::new(Node::tab(P_LEFT)),
                            b: Box::new(Node::tab(P_RIGHT)),
                        },
                    ),
                ],
                hidden: vec![PANEL_SIDE_BAR, PANEL_MAIN, PANEL_INSPECTOR],
                ..Layout::default()
            })
            .plugins(DefaultPlugins)
            .plugin(Panels)
            .kit(super::rerun_like::kit())
            .theme(egui_pop::theme::ThemeSpec::new(theme))
            .build(&egui::Context::default())
            .expect("the audit app builds")
    }
}

/// The lines a row of pixels crosses between `x` and `x_end` (points):
/// runs of at most 3 points, each lighter than both its neighbours or
/// darker than both. A change of fill from one panel to the next is a
/// step, not a line, and its anti-aliased pixel lies between the two.
fn lines_across(img: &image::RgbaImage, y: f32, x: f32, x_end: f32, ppp: f32) -> usize {
    let row = (y * ppp) as u32;
    let (x0, x1) = (
        (x * ppp).max(0.0) as u32,
        ((x_end * ppp) as u32).min(img.width()),
    );
    let luma = |px: u32| {
        let p = img.get_pixel(px, row).0;
        (2 * i32::from(p[0]) + 5 * i32::from(p[1]) + i32::from(p[2])) / 8
    };
    // Runs of near-equal pixels: (luma, width).
    let mut runs: Vec<(i32, u32)> = Vec::new();
    for px in x0..x1 {
        let l = luma(px);
        match runs.last_mut() {
            Some((at, w)) if (l - *at).abs() <= 3 => *w += 1,
            _ => runs.push((l, 1)),
        }
    }
    let thin = (3.0 * ppp).ceil() as u32;
    runs.windows(3)
        .filter(|w| {
            let (before, (l, width), after) = (w[0].0, w[1], w[2].0);
            width <= thin
                && ((l > before + 4 && l > after + 4) || (l + 4 < before && l + 4 < after))
        })
        .count()
}

#[test]
fn at_most_one_line_between_two_panels_at_rest() {
    // Each theme: (the lines where the side area meets the centre, the
    // lines between the centre's two panels).
    let mut seen = Vec::new();
    for theme in ["pop-dark", DARK, LIGHT] {
        let mut h = Harness::builder()
            .with_size(SIZE)
            .with_max_steps(8)
            .shared_wgpu()
            .build_ui_state(|ui, app: &mut App| app.show(ui), audit::app(theme));
        egui_pop::core::motion::freeze(&h.ctx, 0.0);
        h.run();
        let img = h.render().expect("render");
        let ppp = h.ctx.pixels_per_point();
        // A panel's tab: the shortest node named after it.
        let tab = |name: &str| {
            h.query_all_by_label(name)
                .map(|n| n.rect())
                .min_by(|a, b| a.height().total_cmp(&b.height()))
                .unwrap_or_else(|| panic!("{theme}: no tab {name}"))
        };
        let (left, right) = (tab(audit::LEFT), tab(audit::RIGHT));
        tab(audit::SIDE);
        // A row through the bodies, below the tab bars, above the side
        // area's grip (at the middle of its edge).
        let y = left.bottom() + 60.0;
        let areas = lines_across(&img, y, left.left() - 40.0, left.left() + 6.0, ppp);
        let leaves = lines_across(&img, y, right.left() - 14.0, right.left() + 6.0, ppp);
        seen.push((theme, areas, leaves));
    }
    // The probe sees lines: the default kit outlines each body and draws
    // the side area's separator between them.
    let pop = seen[0];
    assert!(pop.1 >= 2, "pop-dark between the areas: {seen:?}");
    // Rerun-like: the side area's separator alone, and the centre's
    // panels meet with no line until the pointer is over their separator.
    for (theme, areas, leaves) in &seen[1..] {
        assert!(*areas <= 1, "{theme} between the areas: {seen:?}");
        assert!(*leaves <= 1, "{theme} between two panels: {seen:?}");
    }
}

#[test]
fn table_rows_take_rerun_s_quiet_selection() {
    use egui_pop::core::kit::{Kit, State};
    for name in [DARK, LIGHT] {
        let (mut h, _) = viewer(ThemeSpec::new(name));
        h.run();
        let t = egui_pop::theme::Tokens::of(&h.ctx);
        let kit = Kit::of(&h.ctx);
        let row = &kit.table_row;
        // Selected: a quiet fill, not the list rows' solid blue, and a
        // marker on the left in the accent.
        let selected = row.style(State::SELECTED);
        assert_ne!(selected.fill, kit.list_row.style(State::SELECTED).fill);
        assert_eq!(selected.text, t.text.strong, "{name}");
        assert_eq!(
            (selected.indicator_color, selected.indicator_size),
            (t.accent.default, 2.0),
            "{name}"
        );
        assert_eq!(
            row.data.indicator_side,
            egui_pop::core::kit::Side::Left,
            "{name}"
        );
        // Hovered: an outline, no fill; the content keeps its place.
        let hover = row.style(State::HOVER);
        assert_eq!(hover.fill, egui::Color32::TRANSPARENT, "{name}");
        assert_eq!(hover.border_width, 1.0, "{name}");
        assert_eq!(row.style(State::REST).border_width, 0.0, "{name}");
        assert_eq!(row.border_room(1.0), 1.0, "{name}");
        // The cursor of a focused table: the list rows' ring.
        let cursor = row.style(State::SELECTED | State::FOCUS);
        assert_eq!(cursor.fill, selected.fill);
        assert_eq!(
            cursor.ring_width,
            kit.list_row.style(State::FOCUS).ring_width
        );
    }
    // Light: Rerun's Gray.900; dark: Blue.750 at a tenth.
    let (mut h, _) = viewer(ThemeSpec::new(LIGHT));
    h.run();
    let t = egui_pop::theme::Tokens::of(&h.ctx);
    assert_eq!(
        Some(Kit::of(&h.ctx).table_row.style(State::SELECTED).fill),
        t.color("gray.3")
    );
}

#[test]
fn a_log_row_moves_the_time_cursor_and_selects_its_entity() {
    use egui_kittest::kittest::NodeT as _;
    let (mut h, model) = viewer(ThemeSpec::new(DARK));
    h.run();
    h.run();
    let start = model.borrow().at();
    // The robot's heartbeat lines: a row of the text log, named by its
    // frame first.
    let rows: Vec<(String, egui::Rect)> = h
        .query_all_by_label_contains("TRACE /robot/status robot: heartbeat")
        .filter(|n| n.accesskit_node().role() == egui::accesskit::Role::Button)
        .map(|n| (n.accesskit_node().label().unwrap_or_default(), n.rect()))
        .collect();
    let (name, _) = rows
        .iter()
        .find(|(name, _)| !name.starts_with(&format!("{start} ")))
        .expect("a heartbeat row in view")
        .clone();
    let frame: u32 = name
        .split(' ')
        .next()
        .and_then(|f| f.parse().ok())
        .expect("the row starts with its frame");
    h.get_by_role_and_label(egui::accesskit::Role::Button, &name)
        .click();
    h.run();
    h.run();
    assert_eq!(model.borrow().at(), frame);
    let robot = ENTITIES
        .iter()
        .position(|e| e.path == "/robot/status")
        .expect("the robot's entity");
    assert_eq!(model.borrow().selected, Some(Item::Entity(robot)));
}
