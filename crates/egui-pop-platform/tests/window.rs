//! `window::raise`'s contract: the commands it sends the main window,
//! read from a frame's output on a bare context. Whether the session is
//! a Wayland one is read from the environment: the attention request
//! is checked against `wayland()`, whatever it says here. What the
//! window manager then does with the commands needs a display and is
//! not tested.

use egui::{Context, RawInput, UserAttentionType, ViewportCommand, ViewportId};
use egui_pop_platform::window::{raise, wayland};

fn commands(mut f: impl FnMut(&Context)) -> (Vec<ViewportCommand>, bool) {
    let ctx = Context::default();
    // egui's own first frame sends the theme: past it, only what `f` sends.
    ctx.run_ui(RawInput::default(), |_| {})
        .textures_delta
        .clear();
    let mut out = ctx.run_ui(RawInput::default(), |ui| f(ui.ctx()));
    // No renderer takes the font atlas.
    out.textures_delta.clear();
    let root = out
        .viewport_output
        .remove(&ViewportId::ROOT)
        .expect("the root viewport's output");
    (root.commands, root.repaint_delay.is_zero())
}

#[test]
fn raise_shows_restores_and_focuses_the_main_window() {
    let (sent, repaint) = commands(raise);
    let mut want = vec![
        ViewportCommand::Visible(true),
        ViewportCommand::Minimized(false),
        ViewportCommand::Focus,
    ];
    if wayland() {
        // The compositor gives no focus without a token: attention.
        want.push(ViewportCommand::RequestUserAttention(
            UserAttentionType::Informational,
        ));
    }
    assert_eq!(sent, want);
    assert!(repaint, "a repaint is asked for at once");
}

#[test]
fn nothing_is_sent_without_raise() {
    let (sent, _) = commands(|_| {});
    assert!(sent.is_empty(), "{sent:?}");
}

#[test]
fn wayland_is_never_said_off_linux_and_the_bsds() {
    if cfg!(any(windows, target_os = "macos")) {
        assert!(!wayland());
    }
}
