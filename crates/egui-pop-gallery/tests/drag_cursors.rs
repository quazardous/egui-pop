//! The drag cursors' audit: each draggable is hovered, then dragged, and
//! the cursor read hovered, mid-drag over the widget, and mid-drag once
//! the pointer has left it. A draggable never shows the arrow while it is
//! dragged, and keeps its drag's cursor wherever the pointer goes.
//!
//! `cargo test -p egui-pop-gallery --test drag_cursors -- --nocapture`
//! prints the table.

use std::cell::Cell;
use std::rc::Rc;

use egui::{CursorIcon, Event, Modifiers, PointerButton, Pos2, Rect, Ui, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::core::pointer::{self, DragKind};
use egui_pop::core::slider::{RangeSlider, Slider};
use egui_pop::core::widgets::Splitter;
use egui_pop::editors::terminal::{Console, Terminal, TerminalState};
use egui_pop::editors::{CodeEditor, EditorState};
use egui_pop::model::pointer::{PointerMap, profile};
use egui_pop::viewport::curve::CurveEditor;
use egui_pop::viewport::graph::{
    Edge, GraphEditor, GraphMinimap, GraphModel, GraphState, Node, Param, Port, PortType,
};
use egui_pop::viewport::histogram::{Channel, Histogram};
use egui_pop::viewport::navigator::Navigator;
use egui_pop::viewport::wheel::{ColorWheel, WheelValue};
use egui_pop::viewport::{View, Viewport, ViewportState};
use egui_pop::views::timeline::{
    Clip, Follow, Marker, TimeScale, TimeView, Timeline, TimelineState, Track,
};

/// Where a widget was laid out, kept by its pass for the test.
type Spot = Rc<Cell<Rect>>;

/// What the cursor was, hovered, dragged over the widget, dragged off it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Probe {
    hover: CursorIcon,
    drag: CursorIcon,
    off: CursorIcon,
}

/// A draggable to drive: its harness, the button, where the drag starts,
/// where it goes over the widget, and where it ends off it.
struct Case {
    name: &'static str,
    harness: Harness<'static>,
    spot: Spot,
    button: PointerButton,
    modifiers: Modifiers,
    /// Fractions of the widget's rect: start, middle of the drag.
    from: [f32; 2],
    to: [f32; 2],
    /// Points from the widget's rect's bottom right, off it.
    off: [f32; 2],
    want: Probe,
    /// Finds the widget by its accessible name, when its pass does not
    /// know its rect.
    locate: Option<fn(&Harness<'static>) -> Rect>,
}

fn at(rect: Rect, f: [f32; 2]) -> Pos2 {
    pos2(
        rect.left() + rect.width() * f[0],
        rect.top() + rect.height() * f[1],
    )
}

fn cursor(h: &Harness<'_>) -> CursorIcon {
    h.output().platform_output.cursor_icon
}

fn button(h: &Harness<'_>, pos: Pos2, b: PointerButton, pressed: bool, m: Modifiers) {
    h.event_modifiers(
        Event::PointerButton {
            pos,
            button: b,
            pressed,
            modifiers: m,
        },
        m,
    );
}

fn drive(c: &mut Case) -> Probe {
    let h = &mut c.harness;
    h.run_steps(3);
    let rect = c.locate.map_or_else(|| c.spot.get(), |f| f(h));
    let from = at(rect, c.from);
    let to = at(rect, c.to);
    let off = rect.right_bottom() + vec2(c.off[0], c.off[1]);
    h.hover_at(from);
    h.run_steps(3);
    h.hover_at(from);
    h.run_steps(3);
    let hover = cursor(h);
    button(h, from, c.button, true, c.modifiers);
    h.run_steps(3);
    for k in 1..=4 {
        let p = from + (to - from) * (k as f32 / 4.0);
        h.event_modifiers(Event::PointerMoved(p), c.modifiers);
        h.run_steps(3);
    }
    let drag = cursor(h);
    for k in 1..=4 {
        let p = to + (off - to) * (k as f32 / 4.0);
        h.event_modifiers(Event::PointerMoved(p), c.modifiers);
        h.run_steps(3);
    }
    let off_cursor = cursor(h);
    button(h, off, c.button, false, c.modifiers);
    h.run_steps(3);
    h.event(Event::PointerGone);
    h.run_steps(3);
    Probe {
        hover,
        drag,
        off: off_cursor,
    }
}

fn case(
    name: &'static str,
    size: [f32; 2],
    mut add: impl FnMut(&mut Ui) -> Rect + 'static,
) -> (Harness<'static>, Spot) {
    let spot: Spot = Rc::new(Cell::new(Rect::NOTHING));
    let kept = spot.clone();
    let h = Harness::builder()
        .with_size(vec2(size[0], size[1]))
        .build_ui(move |ui| {
            egui_pop::core::focus::install(ui.ctx());
            kept.set(add(ui));
        });
    let _ = name;
    (h, spot)
}

/// The cursors of a kind in the default profile.
fn kind(k: DragKind) -> (CursorIcon, CursorIcon) {
    pointer::drag_cursor_icons(&egui::Context::default(), k)
}

fn want(hover: CursorIcon, drag: CursorIcon) -> Probe {
    Probe {
        hover,
        drag,
        off: drag,
    }
}

fn want_kind(k: DragKind) -> Probe {
    let (hover, drag) = kind(k);
    want(hover, drag)
}

#[allow(clippy::too_many_arguments)]
fn primary(
    name: &'static str,
    size: [f32; 2],
    from: [f32; 2],
    to: [f32; 2],
    off: [f32; 2],
    want: Probe,
    add: impl FnMut(&mut Ui) -> Rect + 'static,
) -> Case {
    let (harness, spot) = case(name, size, add);
    Case {
        name,
        harness,
        spot,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        from,
        to,
        off,
        want,
        locate: None,
    }
}

struct Pipeline {
    nodes: Vec<Node>,
}

impl GraphModel for Pipeline {
    fn nodes(&self) -> Vec<Node> {
        self.nodes.clone()
    }

    fn edges(&self) -> Vec<Edge> {
        Vec::new()
    }

    fn params(&self, _node: &str) -> Vec<Param> {
        Vec::new()
    }
}

fn pipeline() -> Pipeline {
    let image = || PortType::new("image");
    Pipeline {
        nodes: vec![
            Node::new("load", "load", "Load", [0.0, 0.0]).output(Port::new(
                "image",
                "Image",
                image(),
            )),
            Node::new("blur", "blur", "Blur", [260.0, 0.0])
                .input(Port::new("image", "Image", image()))
                .output(Port::new("image", "Image", image())),
        ],
    }
}

/// A graph editor whose spot is the rect of what `pick` says, among the
/// node "Load" (`node`), the canvas (`canvas`) and the editor's minimap.
fn graph(
    name: &'static str,
    what: &'static str,
    map: Option<PointerMap>,
) -> (Harness<'static>, Spot) {
    let model = pipeline();
    let mut state = GraphState::new();
    case(name, [720.0, 420.0], move |ui| {
        if let Some(m) = &map {
            pointer::set_map(ui.ctx(), m.clone());
        }
        let top = ui.cursor().min;
        GraphEditor::new(&model, &mut state)
            .size(vec2(700.0, 300.0))
            .show(ui);
        // The response's rect is the scene's, in graph units.
        let canvas = Rect::from_min_size(top, vec2(700.0, 300.0));
        match what {
            // Found by its name once drawn ([`Case::locate`]).
            "node" => Rect::NOTHING,
            "canvas" => {
                Rect::from_min_size(canvas.left_bottom() - vec2(0.0, 80.0), vec2(80.0, 60.0))
            }
            _ => {
                GraphMinimap::new(&mut state)
                    .size(vec2(160.0, 90.0))
                    .show(ui)
                    .rect
            }
        }
    })
}

/// The timeline's marker "Chorus".
fn marker_rect(h: &Harness<'static>) -> Rect {
    h.get_by(|n| {
        n.role() == egui::accesskit::Role::Button && n.label().is_some_and(|l| l.contains("Chorus"))
    })
    .rect()
}

/// The graph's node "Load".
fn node_rect(h: &Harness<'static>) -> Rect {
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Load")
        .rect()
}

fn cases() -> Vec<Case> {
    let mut v = Vec::new();

    // Core.
    let mut x = 0.5_f64;
    v.push(primary(
        "Slider",
        [300.0, 60.0],
        [0.5, 0.5],
        [0.8, 0.5],
        [40.0, 200.0],
        want_kind(DragKind::Value),
        move |ui| {
            Slider::new(&mut x, "Exposure", 0.0..=1.0)
                .show(ui)
                .response
                .rect
        },
    ));
    let (mut lo, mut hi) = (20.0_f64, 80.0_f64);
    v.push(primary(
        "Range slider",
        [300.0, 60.0],
        [0.15, 0.5],
        [0.3, 0.5],
        [40.0, 200.0],
        want_kind(DragKind::Value),
        move |ui| {
            RangeSlider::new(&mut lo, &mut hi, "Focal", 0.0..=100.0)
                .show(ui)
                .response
                .rect
        },
    ));
    let mut width = 100.0_f32;
    v.push(primary(
        "Splitter",
        [400.0, 200.0],
        [0.5, 0.5],
        [3.0, 0.5],
        [100.0, 50.0],
        want_kind(DragKind::ResizeHorizontal),
        move |ui| {
            let rect = Rect::from_min_size(pos2(100.0, 20.0), vec2(10.0, 100.0));
            let id = egui::Id::unique("splitter");
            Splitter::new(id, "Resize the pane", 50.0, 150.0, 100.0).show(ui, rect, &mut width);
            rect
        },
    ));

    // Views: the timeline, a DJ deck's and an editor's.
    let mut deck = TimelineState::new(TimeScale::beats(120.0), 30.0);
    let t = deck.add_track(Track::new(0, "Deck"));
    deck.add_clip(t, Clip::new(0, 0.0, 30.0, "Song")).unwrap();
    deck.view = TimeView::new(0.0, 8.0);
    deck.playhead = 10.0;
    v.push(primary(
        "Timeline, fixed playhead (DJ deck): content",
        [800.0, 300.0],
        [0.6, 0.8],
        [0.4, 0.8],
        [-200.0, 100.0],
        want_kind(DragKind::Scrub),
        move |ui| {
            Timeline::new(&mut deck)
                .id("deck")
                .follow_mode(Follow::CENTRE)
                .scrub_on_drag(true)
                .nudge_on_drag(true)
                .show(ui)
                .response
                .rect
        },
    ));
    let song = || {
        let mut tl = TimelineState::new(TimeScale::Seconds, 100.0);
        tl.markers.clear();
        tl
    };
    let mut tl = song();
    v.push(primary(
        "Timeline: scrub the playhead",
        [800.0, 300.0],
        [0.3, 0.05],
        [0.5, 0.05],
        [-200.0, 100.0],
        want(CursorIcon::Default, kind(DragKind::Value).1),
        move |ui| Timeline::new(&mut tl).id("song").show(ui).response.rect,
    ));
    let mut tl = song();
    v.push(Case {
        modifiers: Modifiers::SHIFT,
        ..primary(
            "Timeline: Shift+drag a range",
            [800.0, 300.0],
            [0.3, 0.05],
            [0.5, 0.05],
            [-200.0, 100.0],
            want(CursorIcon::Default, kind(DragKind::Select).1),
            move |ui| Timeline::new(&mut tl).id("song").show(ui).response.rect,
        )
    });
    let mut tl = song();
    v.push(Case {
        button: PointerButton::Middle,
        ..primary(
            "Timeline: middle drag pans",
            [800.0, 300.0],
            [0.3, 0.05],
            [0.5, 0.05],
            [-200.0, 100.0],
            want(CursorIcon::Default, kind(DragKind::Pan).1),
            move |ui| Timeline::new(&mut tl).id("song").show(ui).response.rect,
        )
    });
    let mut tl = song();
    tl.add_marker(Marker::new(0, 50.0, "Chorus"));
    v.push(primary(
        "Timeline: a marker",
        [800.0, 300.0],
        [0.5, 0.5],
        [3.0, 0.5],
        [-300.0, 150.0],
        want_kind(DragKind::ResizeHorizontal),
        move |ui| Timeline::new(&mut tl).id("song").show(ui).response.rect,
    ));
    v.last_mut().unwrap().locate = Some(marker_rect);
    let mut tl = song();
    let t = tl.add_track(Track::new(0, "Video"));
    tl.add_clip(t, Clip::new(0, 20.0, 30.0, "Shot")).unwrap();
    v.push(primary(
        "Timeline: a clip",
        [800.0, 300.0],
        [0.35, 0.5],
        [0.45, 0.5],
        [-100.0, 150.0],
        want_kind(DragKind::Move),
        move |ui| {
            let rect = Timeline::new(&mut tl).id("song").show(ui).response.rect;
            // The clip spans 20 s to 50 s on the first track.
            let lane = egui::Rangef::new(rect.top() + 30.0, rect.top() + 60.0);
            Rect::from_x_y_ranges(
                rect.left() + rect.width() * 0.2..=rect.left() + rect.width() * 0.5,
                lane,
            )
        },
    ));

    // The viewport crate.
    let mut points = vec![[0.0_f32, 0.0], [1.0, 1.0]];
    v.push(primary(
        "Curve editor",
        [240.0, 240.0],
        [0.5, 0.5],
        [0.6, 0.3],
        [100.0, 100.0],
        want_kind(DragKind::Point),
        move |ui| {
            CurveEditor::new(&mut points, "Tone curve")
                .size(vec2(200.0, 200.0))
                .show(ui)
                .response
                .rect
        },
    ));
    let mut value = WheelValue::default();
    v.push(primary(
        "Colour wheel",
        [240.0, 240.0],
        [0.5, 0.5],
        [0.7, 0.4],
        [100.0, 100.0],
        want_kind(DragKind::Point),
        move |ui| {
            ColorWheel::offset(&mut value, "Lift")
                .size(160.0)
                .show(ui)
                .response
                .rect
        },
    ));
    let bins: Vec<f32> = (0..64).map(|i| (i * (63 - i)) as f32).collect();
    v.push(primary(
        "Histogram (draggable)",
        [420.0, 160.0],
        [0.5, 0.5],
        [0.7, 0.5],
        [100.0, 100.0],
        want_kind(DragKind::ResizeHorizontal),
        move |ui| {
            Histogram::new([Channel::new("Luminance", &bins, egui::Color32::GRAY)])
                .draggable(true)
                .size(vec2(400.0, 120.0))
                .show(ui)
                .response
                .rect
        },
    ));
    let (mut vs, mut view) = (ViewportState::new(), View::default());
    v.push(primary(
        "Navigator",
        [420.0, 520.0],
        [0.5, 0.5],
        [0.6, 0.6],
        [100.0, 100.0],
        want_kind(DragKind::Pan),
        move |ui| {
            Viewport::new("v")
                .image_size([1200, 800])
                .size(vec2(400.0, 260.0))
                .show(ui, &mut vs, &mut view);
            Navigator::new(&mut vs, &mut view)
                .size(vec2(240.0, 160.0))
                .show(ui)
                .response
                .rect
        },
    ));
    let (mut vs, mut view) = (ViewportState::new(), View::default());
    v.push(primary(
        "Image viewport: pan (its own tool cursors)",
        [420.0, 300.0],
        [0.5, 0.5],
        [0.6, 0.6],
        [10.0, 10.0],
        want(CursorIcon::Default, CursorIcon::Grabbing),
        move |ui| {
            Viewport::new("v")
                .image_size([1200, 800])
                .size(vec2(400.0, 260.0))
                .show(ui, &mut vs, &mut view)
                .response
                .rect
        },
    ));
    let (harness, spot) = graph("Graph: a node", "node", None);
    v.push(Case {
        name: "Graph: a node",
        harness,
        spot,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        from: [0.5, 0.2],
        to: [0.8, 0.6],
        off: [400.0, 300.0],
        want: want_kind(DragKind::Move),
        locate: Some(node_rect),
    });
    let (harness, spot) = graph("Graph: rubber band", "canvas", None);
    v.push(Case {
        name: "Graph: rubber band",
        harness,
        spot,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        from: [0.2, 0.5],
        to: [0.8, 0.2],
        off: [0.0, 300.0],
        want: want(CursorIcon::Default, kind(DragKind::Select).1),
        locate: None,
    });
    let (harness, spot) = graph("Graph: middle drag pans", "canvas", None);
    v.push(Case {
        name: "Graph: middle drag pans",
        harness,
        spot,
        button: PointerButton::Middle,
        modifiers: Modifiers::NONE,
        from: [0.2, 0.5],
        to: [0.8, 0.2],
        off: [0.0, 300.0],
        want: want(CursorIcon::Default, kind(DragKind::Pan).1),
        locate: None,
    });
    let (harness, spot) = graph("Graph: minimap", "minimap", None);
    v.push(Case {
        name: "Graph: minimap",
        harness,
        spot,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        from: [0.5, 0.5],
        to: [0.6, 0.6],
        off: [300.0, 0.0],
        want: want_kind(DragKind::Pan),
        locate: None,
    });

    // Editors.
    let state = EditorState::new("notes", "one two three\nfour five six\nseven");
    v.push(primary(
        "Code editor: select text",
        [600.0, 400.0],
        [0.3, 0.05],
        [0.6, 0.1],
        [-300.0, 200.0],
        want_kind(DragKind::Text),
        move |ui| {
            CodeEditor::new()
                .label("Code")
                .show(ui, &state)
                .response
                .rect
        },
    ));
    let state = TerminalState::new("log", Console::new());
    v.push(primary(
        "Terminal: select text",
        [600.0, 300.0],
        [0.3, 0.2],
        [0.6, 0.3],
        [-300.0, 200.0],
        want_kind(DragKind::Text),
        move |ui| {
            Terminal::new()
                .label("Shell")
                .height(200.0)
                .show(ui, &state)
                .response
                .rect
        },
    ));
    v
}

#[test]
fn no_draggable_shows_the_arrow_while_dragged() {
    let mut table = String::from("| Widget | Hover | Drag | Off the widget |\n|---|---|---|---|\n");
    let mut wrong = Vec::new();
    for mut c in cases() {
        let got = drive(&mut c);
        table.push_str(&format!(
            "| {} | {:?} | {:?} | {:?} |\n",
            c.name, got.hover, got.drag, got.off
        ));
        if got.drag == CursorIcon::Default || got.off == CursorIcon::Default || got != c.want {
            wrong.push(format!("{}: got {got:?}, want {:?}", c.name, c.want));
        }
    }
    println!("{table}");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_profile_changes_the_drag_cursors() {
    let blender = PointerMap::from_profile(profile("blender").unwrap());
    let (harness, spot) = graph("Graph: a node, Blender", "node", Some(blender.clone()));
    let mut c = Case {
        name: "Graph: a node, Blender",
        harness,
        spot,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        from: [0.5, 0.2],
        to: [0.8, 0.6],
        off: [400.0, 300.0],
        want: want(CursorIcon::Default, CursorIcon::Move),
        locate: Some(node_rect),
    };
    assert_eq!(drive(&mut c), c.want);
    let ctx = egui::Context::default();
    pointer::set_map(&ctx, blender);
    assert_eq!(
        pointer::drag_cursor_icons(&ctx, DragKind::Pan),
        (CursorIcon::Default, CursorIcon::Grabbing)
    );
}
