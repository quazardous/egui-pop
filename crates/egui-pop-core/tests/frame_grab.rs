//! The window moved by its own title bar, while the compositor keeps the
//! release of the press that started the move (labwc, mutter on Wayland):
//! egui must not go on holding the button. The real window's check is
//! `tools/wbox/titlebar.py`.

use egui::{Context, Pos2, Rect, ViewportCommand, pos2, vec2};
use egui_pop_core::frame::TitleBar;
use egui_pop_core::theme::{self, ThemeSpec};

/// One pass of a title bar: its minimize button, and whether the window
/// was handed to the compositor (`StartDrag`).
fn pass(ctx: &Context, events: Vec<egui::Event>) -> (Option<Rect>, bool) {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
        events,
        ..Default::default()
    };
    let mut minimize = None;
    let mut full = ctx.run_ui(input, |ui| {
        minimize = TitleBar::new("Title")
            .show(ui, |_| {}, |_| {})
            .layout
            .minimize;
    });
    full.textures_delta.clear();
    let started = full
        .viewport_output
        .values()
        .any(|v| v.commands.contains(&ViewportCommand::StartDrag));
    (minimize, started)
}

/// After the move, with no release, the next pass holds no button and
/// drags nothing, so what the pointer moves over hovers again (#270).
#[test]
fn nothing_is_held_after_a_move_whose_release_never_came() {
    let ctx = Context::default();
    theme::install(&ctx, &ThemeSpec::new("pop-dark"));
    let caption = pos2(300.0, 10.0);
    pass(&ctx, vec![egui::Event::PointerMoved(caption)]);
    pass(
        &ctx,
        vec![egui::Event::PointerButton {
            pos: caption,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let mut started = false;
    let mut minimize = None;
    for dx in [20.0, 40.0] {
        let (m, s) = pass(
            &ctx,
            vec![egui::Event::PointerMoved(caption + vec2(dx, 0.0))],
        );
        started |= s;
        minimize = m;
    }
    assert!(started, "the move went to the compositor");
    // No release: the pointer comes back over the minimize button.
    let button = minimize.expect("a minimize button").center();
    pass(&ctx, vec![egui::Event::PointerMoved(button)]);
    assert!(
        !ctx.input(|i| i.pointer.any_down()),
        "egui still holds the button that started the move"
    );
    assert_eq!(ctx.dragged_id(), None, "the caption is still dragged");
}
