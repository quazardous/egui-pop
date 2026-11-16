//! A whole tab group dragged by the empty part of its tab bar, as an
//! editor group is dragged by its tab bar in established IDEs: all its
//! tabs move together, in order, its shown tab kept, through the same bus
//! steps as a tab (`workbench.move` for each tab dropped in another area,
//! `workbench.join_window` and `workbench.detach` with `group` across
//! windows). The tab menu's "Move group to window" does the same for
//! keyboard users and where a drag cannot reach a window; a group let go
//! outside every window opens a window of its own. Each window is a real
//! viewport of the test kit's backend, with and without the window
//! positions the system tells (Wayland hides them).

use egui::{Event, Modifiers, Pos2, ViewportId, pos2};
use egui_pop_api::workbench::{self, Change};
use egui_pop_api::{App, Ctx, DefaultPlugins, Plugin, PluginError};
use egui_pop_model::ids::{PanelId, PluginId};
use egui_pop_model::layout::{Layout, Node};
use egui_pop_model::panels::{Area, PanelSpec};
use egui_pop_test::windows::MultiWindow;

const ALPHA: PanelId = PanelId::from_static("demo.alpha");
const BETA: PanelId = PanelId::from_static("demo.beta");
const GAMMA: PanelId = PanelId::from_static("demo.gamma");
const DELTA: PanelId = PanelId::from_static("demo.delta");
const ECHO: PanelId = PanelId::from_static("demo.echo");

/// Alpha, Beta and Gamma as tabs in the bottom area, Delta alone in the
/// end side area, Echo in the centre (detached by the tests that need a
/// window).
struct Demo;

impl Plugin for Demo {
    fn id(&self) -> PluginId {
        PluginId::from_static("demo")
    }

    fn connect(&mut self, ctx: &mut Ctx<'_>) -> Result<(), PluginError> {
        for (id, title, area) in [
            (ALPHA, "Alpha", Area::Bottom),
            (BETA, "Beta", Area::Bottom),
            (GAMMA, "Gamma", Area::Bottom),
            (DELTA, "Delta", Area::EndSide),
            (ECHO, "Echo", Area::Center),
        ] {
            ctx.panel(PanelSpec::new(id, title, area), move |ui, _| {
                ui.label(format!("{title} body"));
            })?;
        }
        Ok(())
    }
}

/// The application, Beta shown among the bottom group's tabs.
fn multi() -> MultiWindow {
    let mut m = MultiWindow::new(|ctx| {
        App::builder()
            .name("Demo")
            .plugins(DefaultPlugins)
            .plugin(Demo)
            .build(ctx)
            .unwrap()
    });
    m.root_pass();
    m.root_pass();
    let (beta, _) = m.node(ViewportId::ROOT, "Beta").expect("Beta's tab");
    m.click(ViewportId::ROOT, beta.center());
    m.root_pass();
    assert_eq!(
        group(&m.app().layout(), &ALPHA),
        (vec![ALPHA, BETA, GAMMA], 1)
    );
    m
}

/// The tab group holding a panel: its panels and the shown one's index.
fn group(layout: &Layout, panel: &PanelId) -> (Vec<PanelId>, usize) {
    layout.group_of(panel).expect("in a tab group")
}

/// Detaches Echo into a window of its own and lets the backend show it.
fn detach_echo(m: &mut MultiWindow) -> ViewportId {
    assert!(m.app().detach_panel(&ECHO).unwrap().ok());
    m.root_pass();
    m.root_pass();
    let v = workbench::window_id(&ECHO);
    assert!(m.shown_immediate().contains(&v));
    v
}

fn send(m: &mut MultiWindow, e: Event) {
    m.queue(ViewportId::ROOT, e);
    m.root_pass();
}

/// The empty part of the bottom group's tab bar, right of Gamma's tab.
fn bar_tail(m: &MultiWindow) -> Pos2 {
    let (gamma, _) = m.node(ViewportId::ROOT, "Gamma").expect("Gamma's tab");
    pos2(gamma.right() + 40.0, gamma.center().y)
}

/// Presses the bottom group's tab bar and drags it to `to` (the main
/// window's points), the button held.
fn drag_group(m: &mut MultiWindow, to: Pos2) {
    let from = bar_tail(m);
    send(m, Event::PointerMoved(from));
    send(
        m,
        Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
    );
    for i in 1..=8 {
        send(
            m,
            Event::PointerMoved(from + (to - from) * (i as f32 / 8.0)),
        );
    }
    m.root_pass();
}

/// Lets the dragged group go at `at` (the main window's points).
fn release(m: &mut MultiWindow, at: Pos2) {
    send(
        m,
        Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    );
    m.root_pass();
    m.root_pass();
}

/// The group is in a detached window, alone there or after `before`, in
/// order, Beta shown; none of its panels is docked.
fn assert_in_window(layout: &Layout, before: &[PanelId]) {
    let window = layout.window_of(&ALPHA).expect("the group is in a window");
    let panels: Vec<PanelId> = window.panels().cloned().collect();
    let mut want = before.to_vec();
    want.extend([ALPHA, BETA, GAMMA]);
    assert_eq!(panels, want);
    assert_eq!(window.shown(), &BETA);
    for p in [ALPHA, BETA, GAMMA] {
        assert_eq!(layout.area_of(&p), None, "{p}");
    }
}

#[test]
fn a_group_dragged_into_another_joins_its_tabs_in_order_its_shown_tab_kept() {
    let mut m = multi();
    let side = m.app().area_rect(Area::EndSide).expect("the end side");
    let to = pos2(side.center().x, side.center().y + 20.0);
    drag_group(&mut m, to);
    let hint = m.app().drop_hint().expect("the drop is hinted");
    assert_eq!(hint.area, "end_side");
    release(&mut m, to);
    let layout = m.app().layout();
    assert_eq!(group(&layout, &ALPHA), (vec![DELTA, ALPHA, BETA, GAMMA], 2));
    for p in [ALPHA, BETA, GAMMA, DELTA] {
        assert_eq!(layout.area_of(&p), Some(Area::EndSide), "{p}");
    }
}

#[test]
fn a_group_dropped_on_an_edge_splits_the_group_there() {
    let mut m = multi();
    let side = m.app().area_rect(Area::EndSide).expect("the end side");
    let to = pos2(side.center().x, side.bottom() - 12.0);
    drag_group(&mut m, to);
    release(&mut m, to);
    let layout = m.app().layout();
    assert_eq!(group(&layout, &ALPHA), (vec![ALPHA, BETA, GAMMA], 1));
    assert_eq!(group(&layout, &DELTA), (vec![DELTA], 0));
    let (_, node) = (layout.areas.iter())
        .find(|(a, _)| *a == Area::EndSide)
        .expect("the end side holds them");
    assert!(
        matches!(node, Node::Split { .. }),
        "Delta and the group side by side: {node:?}"
    );
}

/// Where the system tells where windows are: the group dragged over a
/// detached window joins its tabs whole.
#[test]
fn a_group_dragged_onto_a_detached_window_joins_it_whole() {
    let mut m = multi();
    let v = detach_echo(&mut m);
    m.set_outer_position(ViewportId::ROOT, [0.0, 0.0]);
    m.set_outer_position(v, [700.0, 0.0]);
    m.root_pass();
    m.root_pass();
    let to = pos2(900.0, 200.0);
    drag_group(&mut m, to);
    let over = m.app().window_drop().expect("seen over the window");
    assert_eq!(over.window, ECHO);
    release(&mut m, to);
    assert_in_window(&m.app().layout(), &[ECHO]);
}

/// Where the system hides window positions (Wayland): the window that
/// sees the pointer once the drag lets it go takes the group whole.
#[test]
fn a_group_let_go_over_a_detached_window_joins_it_where_positions_are_hidden() {
    let mut m = multi();
    m.hide_positions();
    let v = detach_echo(&mut m);
    let to = pos2(700.0, 200.0);
    drag_group(&mut m, to);
    assert!(m.app().window_drop().is_none());
    assert_eq!(m.app().window_outlines(), [ECHO]);
    release(&mut m, to);
    send(&mut m, Event::PointerGone);
    m.queue(v, Event::PointerMoved(pos2(120.0, 200.0)));
    m.root_pass();
    m.root_pass();
    let layout = m.app().layout();
    assert_in_window(&layout, &[ECHO]);
    assert_eq!(layout.auxiliary.len(), 1);
}

#[test]
fn a_group_let_go_outside_every_window_opens_a_window_of_its_own() {
    // No detached window, the positions told; then a detached window that
    // never sees the pointer, the positions hidden.
    for hidden in [false, true] {
        let mut m = multi();
        let mut windows = 0;
        if hidden {
            m.hide_positions();
            detach_echo(&mut m);
            windows = 1;
        }
        let to = pos2(700.0, 200.0);
        drag_group(&mut m, to);
        release(&mut m, to);
        send(&mut m, Event::PointerGone);
        // No window takes it in the passes it waits for one.
        for _ in 0..20 {
            m.root_pass();
        }
        let layout = m.app().layout();
        assert_in_window(&layout, &[]);
        assert_eq!(layout.auxiliary.len(), windows + 1, "hidden: {hidden}");
        let v = workbench::window_id(&ALPHA);
        assert!(m.shown_immediate().contains(&v), "hidden: {hidden}");
        assert!(m.names(v).iter().any(|n| n == "Beta body"), "Beta shown");
    }
}

/// The tab menu's "Move group to window" names each detached window (by
/// its panels' titles) and a new window: the keyboard's and Wayland's way.
#[test]
fn the_tab_menu_moves_the_group_into_a_window() {
    for hidden in [false, true] {
        let mut m = multi();
        if hidden {
            m.hide_positions();
        }
        detach_echo(&mut m);
        let (alpha, _) = m.node(ViewportId::ROOT, "Alpha").expect("Alpha's tab");
        let at = alpha.center();
        let secondary = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        send(&mut m, Event::PointerMoved(at));
        send(&mut m, secondary(true));
        send(&mut m, secondary(false));
        m.root_pass();
        let (entry, _) = m
            .node(ViewportId::ROOT, "Move group to window")
            .expect("the tab menu's entry");
        m.click(ViewportId::ROOT, entry.center());
        m.root_pass();
        assert!(
            m.node(ViewportId::ROOT, "New window").is_some(),
            "a new window offered"
        );
        let (echo, _) = m
            .node(ViewportId::ROOT, "Echo")
            .expect("the window named by its panel");
        m.click(ViewportId::ROOT, echo.center());
        m.root_pass();
        assert_in_window(&m.app().layout(), &[ECHO]);
    }
}

/// A group in a window is saved with the layout, and put back as it was.
#[test]
fn a_group_moved_whole_is_saved_and_restored() {
    let mut m = multi();
    let out = m.app().change_workbench(Change::DetachGroup(BETA));
    assert!(out.unwrap().ok());
    m.root_pass();
    m.root_pass();
    let layout = m.app().layout();
    assert_in_window(&layout, &[]);
    let saved = layout.to_json();
    let restored = Layout::from_json(&saved).unwrap();
    assert_eq!(restored, layout);
    // Put in again, the window keeps the group.
    let mut docked = restored.clone();
    for p in [ALPHA, BETA, GAMMA] {
        docked.place(&p, Area::Bottom);
    }
    m.app().set_layout(docked);
    m.root_pass();
    m.app().set_layout(restored);
    m.root_pass();
    m.root_pass();
    assert_in_window(&m.app().layout(), &[]);
    assert!(m.shown_immediate().contains(&workbench::window_id(&ALPHA)));
}
