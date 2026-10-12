//! The focus mode read in-process: each desktop's answer, the plugin
//! setting what egui-pop follows.

#![cfg(feature = "focus")]

use egui_pop_api::App;
use egui_pop_core::focus_mode::{self, FocusMode, SystemFocus};
use egui_pop_platform::focus::{self, FocusModePlugin, Sources};

#[test]
fn each_desktop_answers_its_way() {
    assert_eq!(focus::kwin_policy(""), Some(FocusMode::Click));
    assert_eq!(
        focus::kwin_policy(
            "[Windows]\nFocusPolicy=ClickToFocus\n[Other]\nFocusPolicy=FocusFollowsMouse\n"
        ),
        Some(FocusMode::Click)
    );
    assert_eq!(
        focus::kwin_policy(
            "[Compositing]\nx=1\n[Windows]\nBorderless=true\nFocusPolicy=FocusStrictlyUnderMouse\n"
        ),
        Some(FocusMode::Hover)
    );
    let env = focus::read_with(Some("hover"), "", &Sources::NONE);
    assert_eq!((env.mode, env.from), (FocusMode::Hover, Some("env")));
    if cfg!(all(unix, not(target_os = "macos"))) {
        let gnome = focus::read_with(
            None,
            "GNOME",
            &Sources {
                portal: &|| Some("sloppy".into()),
                ..Sources::NONE
            },
        );
        assert_eq!((gnome.mode, gnome.from), (FocusMode::Hover, Some("GNOME")));
        // KDE without a kwinrc: the portal is asked next.
        let kde = focus::read_with(
            None,
            "KDE",
            &Sources {
                portal: &|| Some("click".into()),
                ..Sources::NONE
            },
        );
        assert_eq!((kde.mode, kde.from), (FocusMode::Click, Some("GNOME")));
        let nothing = focus::read_with(None, "sway", &Sources::NONE);
        assert_eq!(nothing, SystemFocus::default());
    }
}

#[test]
fn the_plugin_sets_what_egui_pop_follows() {
    let ctx = egui::Context::default();
    let mut app = App::builder()
        .plugin(FocusModePlugin::fixed(SystemFocus {
            mode: FocusMode::Hover,
            from: Some("GNOME"),
        }))
        .build(&ctx)
        .unwrap();
    app.logic(&ctx);
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
    out.textures_delta.clear();
    assert_eq!(focus_mode::mode(&ctx), FocusMode::Hover);
}

/// The real system: on this machine's session, whatever it answers,
/// without a helper program. Run with `cargo test -p egui-pop-platform
/// --test focus -- --ignored`.
#[test]
#[ignore = "reads the session's settings"]
fn the_session_answers() {
    let read = focus::read();
    eprintln!("{read:?}");
}
