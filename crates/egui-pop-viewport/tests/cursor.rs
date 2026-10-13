//! Cursors: the default for each tool and the keys held, the brush
//! outline against the zoom, the bitmaps, and the viewport showing them.

use egui::{CursorIcon, Event, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_pop_test::viewport::TestSource;
use egui_pop_viewport::cursor::{
    self, Bitmap, Brush, BrushMark, BrushOutline, Cursor, CursorImage, CursorQuery, CursorSettings,
    Shown, Tool, ToolCursor,
};
use egui_pop_viewport::overlay::HandleKind;
use egui_pop_viewport::{View, Viewport, ViewportState};

fn query(tool: Tool) -> CursorQuery {
    CursorQuery::new(tool)
}

#[test]
fn each_tool_has_its_cursor_and_the_keys_and_drags_change_it() {
    let alt = Modifiers::ALT;
    let cases: Vec<(CursorQuery, Cursor)> = vec![
        (query(Tool::None), Cursor::System(CursorIcon::Default)),
        (query(Tool::Zoom), Cursor::Tool(ToolCursor::ZoomIn)),
        (
            CursorQuery {
                modifiers: alt,
                ..query(Tool::Zoom)
            },
            Cursor::Tool(ToolCursor::ZoomOut),
        ),
        (query(Tool::Pan), Cursor::Tool(ToolCursor::Hand)),
        (
            CursorQuery {
                dragging: true,
                ..query(Tool::Pan)
            },
            Cursor::Tool(ToolCursor::Grab),
        ),
        (query(Tool::Picker), Cursor::Tool(ToolCursor::Picker)),
        (query(Tool::Crosshair), Cursor::Tool(ToolCursor::Crosshair)),
        (query(Tool::Rotate), Cursor::Tool(ToolCursor::Rotate)),
        (query(Tool::Disabled), Cursor::Tool(ToolCursor::NotAllowed)),
        (
            query(Tool::Named("lasso".into())),
            Cursor::System(CursorIcon::Default),
        ),
    ];
    for (q, want) in cases {
        assert_eq!(cursor::default_cursor(&q), want, "{q:?}");
    }
}

#[test]
fn panning_and_handles_come_before_the_tool() {
    let panning = CursorQuery {
        panning: true,
        ..query(Tool::Zoom)
    };
    assert_eq!(
        cursor::default_cursor(&panning),
        Cursor::Tool(ToolCursor::Hand)
    );
    let grabbing = CursorQuery {
        dragging: true,
        ..panning
    };
    assert_eq!(
        cursor::default_cursor(&grabbing),
        Cursor::Tool(ToolCursor::Grab)
    );
    let over = CursorQuery {
        handle: Some(HandleKind::Corner),
        ..query(Tool::Picker)
    };
    assert_eq!(
        cursor::default_cursor(&over),
        Cursor::Tool(ToolCursor::Move)
    );
    let rotating = CursorQuery {
        handle: Some(HandleKind::Rotate),
        dragging: true,
        ..query(Tool::None)
    };
    assert_eq!(
        cursor::default_cursor(&rotating),
        Cursor::Tool(ToolCursor::Rotate)
    );
}

#[test]
fn brushes_show_their_outline_and_a_mask_brush_its_sign() {
    let b = Brush::new(30.0, 0.5);
    let outline = |mark| {
        Cursor::Brush(BrushOutline {
            radius: 30.0,
            hardness: 0.5,
            mark,
        })
    };
    assert_eq!(
        cursor::default_cursor(&query(Tool::Brush(b))),
        outline(BrushMark::None)
    );
    assert_eq!(
        cursor::default_cursor(&query(Tool::Eraser(b))),
        outline(BrushMark::Erase)
    );
    assert_eq!(
        cursor::default_cursor(&query(Tool::Mask(b))),
        outline(BrushMark::Add)
    );
    let subtract = CursorQuery {
        modifiers: Modifiers::ALT,
        ..query(Tool::Mask(b))
    };
    assert_eq!(
        cursor::default_cursor(&subtract),
        outline(BrushMark::Subtract)
    );
}

#[test]
fn the_outline_follows_the_zoom_and_gives_way_to_a_crosshair() {
    let ctx = egui::Context::default();
    let brush = Cursor::Brush(BrushOutline {
        radius: 40.0,
        hardness: 0.5,
        mark: BrushMark::None,
    });
    // 40 image pixels at 50 %, one pixel per point: 20 points.
    assert_eq!(
        cursor::shown(&ctx, &brush, 0.5, 1.0),
        Shown::Outline {
            radius: 20.0,
            inner: 10.0,
            mark: BrushMark::None
        }
    );
    // At 200 % on a window of two pixels per point: 40 points.
    match cursor::shown(&ctx, &brush, 2.0, 2.0) {
        Shown::Outline { radius, .. } => assert_eq!(radius, 40.0),
        other => panic!("{other:?}"),
    }
    // At 5 %, two points: too small, a crosshair.
    assert_eq!(
        cursor::shown(&ctx, &brush, 0.05, 1.0),
        Shown::Tool(ToolCursor::Crosshair)
    );
    // A hard brush has no inner ring.
    let hard = Cursor::Brush(BrushOutline {
        radius: 40.0,
        hardness: 1.0,
        mark: BrushMark::None,
    });
    assert!(matches!(
        cursor::shown(&ctx, &hard, 1.0, 1.0),
        Shown::Outline { inner: 0.0, .. }
    ));
}

#[test]
fn with_painted_cursors_off_the_system_cursors_stand_in() {
    let ctx = egui::Context::default();
    cursor::set_settings(
        &ctx,
        CursorSettings {
            painted: false,
            size: None,
        },
    );
    assert_eq!(
        cursor::shown(&ctx, &Cursor::Tool(ToolCursor::ZoomOut), 1.0, 1.0),
        Shown::System(CursorIcon::ZoomOut)
    );
    assert_eq!(
        cursor::shown(&ctx, &Cursor::Tool(ToolCursor::Picker), 1.0, 1.0),
        Shown::System(CursorIcon::Crosshair)
    );
    let brush = Cursor::Brush(BrushOutline {
        radius: 40.0,
        hardness: 0.5,
        mark: BrushMark::Add,
    });
    assert_eq!(
        cursor::shown(&ctx, &brush, 1.0, 1.0),
        Shown::System(CursorIcon::Crosshair)
    );
    cursor::register_image(&ctx, "lasso", CursorImage::new(CursorIcon::Cell));
    assert_eq!(
        cursor::shown(&ctx, &Cursor::Image("lasso".into()), 1.0, 1.0),
        Shown::System(CursorIcon::Cell)
    );
}

#[test]
fn cursors_go_to_json_and_back() {
    for c in [
        Cursor::System(CursorIcon::Crosshair),
        Cursor::Tool(ToolCursor::MaskSubtract),
        Cursor::Image("lasso".into()),
        Cursor::Brush(BrushOutline {
            radius: 12.5,
            hardness: 0.25,
            mark: BrushMark::Erase,
        }),
    ] {
        assert_eq!(Cursor::from_json(&c.to_json()), Some(c.clone()), "{c:?}");
    }
    assert_eq!(
        Cursor::from_json(&serde_json::json!({"tool": "nope"})),
        None
    );
    for t in ToolCursor::ALL {
        assert_eq!(ToolCursor::by_name(t.name()), Some(*t));
    }
}

#[test]
fn tool_bitmaps_are_drawn_at_the_size_asked_with_their_hot_spot_inside() {
    let fill = egui::Color32::WHITE;
    let outline = egui::Color32::BLACK;
    for t in ToolCursor::ALL {
        for side in [24, 36, 48] {
            let rgba = cursor::rasterize(*t, side, fill, outline);
            assert_eq!(rgba.len(), (side * side * 4) as usize);
            let alpha = |x: u32, y: u32| rgba[((y * side + x) * 4 + 3) as usize];
            assert!(rgba.chunks_exact(4).any(|p| p[3] == 255), "{t:?} draws");
            assert_eq!(alpha(0, 0), 0, "{t:?}: the corner is clear");
            let h = t.hotspot();
            assert!((0.0..1.0).contains(&h[0]) && (0.0..1.0).contains(&h[1]));
        }
    }
    // The crosshair's centre is drawn, its strokes ringed in the outline.
    let side = 32;
    let rgba = cursor::rasterize(ToolCursor::Crosshair, side, fill, outline);
    let at = |x: u32, y: u32| {
        let i = ((y * side + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    };
    assert_eq!(at(16, 8), [255, 255, 255, 255], "a stroke, in the fill");
    assert_eq!(at(18, 8)[0], 0, "beside it, the outline");
    assert!(at(18, 8)[3] > 0);
}

#[test]
fn bitmaps_are_checked_and_the_closest_size_is_picked() {
    let bitmap = |side: u16| {
        Bitmap::new(
            vec![0u8; usize::from(side) * usize::from(side) * 4],
            [side, side],
            [1, 1],
        )
        .unwrap()
    };
    assert!(Bitmap::new(vec![0u8; 10], [2, 2], [0, 0]).is_none());
    assert!(Bitmap::new(vec![0u8; 16], [2, 2], [2, 0]).is_none());
    let img = CursorImage::new(CursorIcon::Cell)
        .size(bitmap(24))
        .size(bitmap(48))
        .size(bitmap(36))
        .dark(bitmap(30));
    assert_eq!(img.pick(24, false, 0).unwrap().size[0], 24);
    assert_eq!(img.pick(30, false, 0).unwrap().size[0], 36);
    assert_eq!(img.pick(96, false, 0).unwrap().size[0], 48);
    assert_eq!(img.pick(24, true, 0).unwrap().size[0], 30);
    let animated = img.frame(vec![bitmap(20)]);
    assert_eq!(animated.frame_count(), 2);
    assert_eq!(animated.pick(24, false, 1).unwrap().size[0], 20);
    assert_eq!(animated.pick(24, false, 2).unwrap().size[0], 24);
}

#[test]
fn the_cursor_size_follows_the_system_unless_set() {
    assert_eq!(cursor::parse_size(Some("32")), 32.0);
    assert_eq!(cursor::parse_size(Some(" 48 ")), 48.0);
    assert_eq!(cursor::parse_size(Some("huge")), cursor::DEFAULT_SIZE);
    assert_eq!(cursor::parse_size(Some("4")), cursor::DEFAULT_SIZE);
    assert_eq!(cursor::parse_size(None), cursor::DEFAULT_SIZE);
    let ctx = egui::Context::default();
    cursor::set_settings(
        &ctx,
        CursorSettings {
            painted: true,
            size: Some(40.0),
        },
    );
    assert_eq!(cursor::size(&ctx), 40.0);
}

struct Scene {
    state: ViewportState,
    view: View,
    source: TestSource,
    tool: Tool,
    /// Turns zoom in into a picker (an application's own rule).
    redirect: bool,
}

fn scene(tool: Tool) -> Scene {
    let mut state = ViewportState::new();
    state.set_settle(0.0);
    let source = TestSource::immediate(state.sink(), [1000, 800]);
    Scene {
        state,
        view: View::actual([500.0, 400.0]),
        source,
        tool,
        redirect: false,
    }
}

fn draw(ui: &mut egui::Ui, s: &mut Scene) {
    let redirect = s.redirect;
    Viewport::new("c")
        .source(&s.source)
        .tool(s.tool.clone())
        .on_cursor(move |_, c| match c {
            Cursor::Tool(ToolCursor::ZoomIn) if redirect => Cursor::Tool(ToolCursor::Picker),
            c => c,
        })
        .show(ui, &mut s.state, &mut s.view);
}

fn harness(s: Scene) -> Harness<'static, Scene> {
    Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .build_ui_state(draw, s)
}

fn shown(h: &Harness<'_, Scene>) -> Option<Shown> {
    h.state().state.cursor_shown().cloned()
}

#[test]
fn the_viewport_shows_the_tools_cursor_as_a_bitmap_and_alt_turns_it() {
    let mut h = harness(scene(Tool::Zoom));
    h.run();
    h.hover_at(Pos2::new(200.0, 150.0));
    h.run();
    assert_eq!(shown(&h), Some(Shown::Tool(ToolCursor::ZoomIn)));
    let out = &h.output().platform_output;
    assert!(out.cursor_image.is_some(), "a bitmap for the system");
    assert_eq!(out.cursor_icon, CursorIcon::ZoomIn, "and its fallback");
    h.event(Event::ModifiersChanged(Modifiers::ALT));
    h.hover_at(Pos2::new(201.0, 150.0));
    h.run();
    assert_eq!(shown(&h), Some(Shown::Tool(ToolCursor::ZoomOut)));
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    // Off the viewport: nothing shown, the bitmap cleared.
    h.event(Event::PointerGone);
    h.run();
    assert_eq!(shown(&h), None);
    assert!(h.output().platform_output.cursor_image.is_none());
}

#[test]
fn a_hand_closes_while_the_view_is_dragged() {
    let mut h = harness(scene(Tool::Pan));
    h.run();
    let from = Pos2::new(200.0, 150.0);
    h.hover_at(from);
    h.run();
    assert_eq!(shown(&h), Some(Shown::Tool(ToolCursor::Hand)));
    h.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Middle,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.step();
    for k in 1..=3 {
        h.hover_at(from + vec2(10.0 * k as f32, 0.0));
        h.step();
    }
    assert_eq!(shown(&h), Some(Shown::Tool(ToolCursor::Grab)));
    h.event(Event::PointerButton {
        pos: from + vec2(30.0, 0.0),
        button: PointerButton::Middle,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run();
    assert_eq!(shown(&h), Some(Shown::Tool(ToolCursor::Hand)));
}

#[test]
fn the_hook_gives_another_cursor_and_the_trace_says_so() {
    let mut s = scene(Tool::Zoom);
    s.redirect = true;
    let mut h = harness(s);
    h.run();
    h.hover_at(Pos2::new(200.0, 150.0));
    h.run();
    assert_eq!(shown(&h), Some(Shown::Tool(ToolCursor::Picker)));
    assert!(
        h.state()
            .state
            .trace()
            .any(|t| t.hook == "on_cursor" && t.verdict == "redirect")
    );
}

#[test]
fn a_brush_hides_the_system_cursor_and_its_outline_follows_the_zoom() {
    let mut h = harness(scene(Tool::Brush(Brush::new(25.0, 0.6))));
    h.run();
    h.hover_at(Pos2::new(200.0, 150.0));
    h.run();
    // 100 %, one physical pixel per point: 25 points.
    match shown(&h) {
        Some(Shown::Outline { radius, inner, .. }) => {
            assert_eq!(radius, 25.0);
            assert!((inner - 15.0).abs() < 1e-4);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(h.output().platform_output.cursor_icon, CursorIcon::None);
    assert!(h.output().platform_output.cursor_image.is_none());
    // Twice the zoom, twice the outline.
    h.state_mut().view = View::at(2.0, [500.0, 400.0]);
    h.hover_at(Pos2::new(201.0, 150.0));
    h.run();
    assert!(matches!(shown(&h), Some(Shown::Outline { radius, .. }) if radius == 50.0));
}
