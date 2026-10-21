//! The system's text scaling read in-process: each desktop's answer, with
//! fake sources (no real system is read), and the text size it seeds.

#![cfg(feature = "text-scale")]

use egui_pop_api::App;
use egui_pop_api::text::{self, SystemTextScale};
use egui_pop_platform::text_scale::{self, Session, Sources};

#[test]
fn the_variable_and_kde_files_parse() {
    assert_eq!(text_scale::parse_env("1.25"), Some(1.25));
    assert_eq!(text_scale::parse_env(" 150% "), Some(1.5));
    assert_eq!(text_scale::parse_env("0"), None);
    assert_eq!(text_scale::parse_env("big"), None);
    let kcmfonts = "[General]\nforceFontDPI=144\nforceFontDPIWayland=120\n";
    assert_eq!(text_scale::kde_font_dpi(kcmfonts, false), Some(1.5));
    assert_eq!(text_scale::kde_font_dpi(kcmfonts, true), Some(1.25));
    // Only forceFontDPI: it counts on Wayland too.
    assert_eq!(
        text_scale::kde_font_dpi("[General]\nforceFontDPI=120\n", true),
        Some(1.25)
    );
    // 0 is KDE's "not forced"; another group does not count.
    assert_eq!(
        text_scale::kde_font_dpi("[General]\nforceFontDPI=0\n", true),
        None
    );
    assert_eq!(
        text_scale::kde_font_dpi("[Other]\nforceFontDPI=120\n", true),
        None
    );
}

#[test]
fn each_desktop_answers_its_way() {
    let env = text_scale::read_with(Some("125%"), &Session::default(), &Sources::NONE);
    assert_eq!(env, SystemTextScale::new(1.25, "env"));
    let gnome = Sources {
        portal: &|| Some(1.5),
        ..Sources::NONE
    };
    let kde = Sources {
        portal: &|| Some(1.5),
        kdeglobals: &|| Some("[General]\nforceFontDPI=120\n".into()),
        ..Sources::NONE
    };
    let windows = Sources {
        registry: &|| Some(175),
        ..Sources::NONE
    };
    if cfg!(windows) {
        let read = text_scale::read_with(None, &Session::default(), &windows);
        assert_eq!(read, SystemTextScale::new(1.75, "Windows"));
        let nothing = text_scale::read_with(None, &Session::default(), &Sources::NONE);
        assert_eq!(nothing, SystemTextScale::default());
    } else if cfg!(all(unix, not(target_os = "macos"))) {
        let read = text_scale::read_with(None, &Session::wayland("GNOME"), &gnome);
        assert_eq!(read, SystemTextScale::new(1.5, "GNOME"));
        let read = text_scale::read_with(None, &Session::wayland("KDE"), &kde);
        assert_eq!(read, SystemTextScale::new(1.25, "KDE"));
        // KDE without a forced DPI: the portal is asked next.
        let read = text_scale::read_with(None, &Session::wayland("KDE"), &gnome);
        assert_eq!(read, SystemTextScale::new(1.5, "GNOME"));
        // X11: Xft.dpi already carries it, as the display's scale.
        let read = text_scale::read_with(None, &Session::x11("GNOME"), &gnome);
        assert_eq!(read, SystemTextScale::default());
        let read = text_scale::read_with(None, &Session::wayland("sway"), &Sources::NONE);
        assert_eq!(read, SystemTextScale::default());
        // The registry is Windows' alone.
        let read = text_scale::read_with(None, &Session::wayland("GNOME"), &windows);
        assert_eq!(read, SystemTextScale::default());
    }
}

#[test]
fn the_read_factor_seeds_the_text_size() {
    let read = text_scale::read_with(Some("1.3"), &Session::default(), &Sources::NONE);
    let app = App::builder()
        .system_text_scale(read)
        .build(&egui::Context::default())
        .unwrap();
    assert_eq!(text::text_size(app.handle()), 1.3);
    assert!(!text::user_set(app.handle()));
    assert_eq!(
        app.handle().debug_snapshot()["text_scale"]["system"]["from"],
        "env"
    );
}

/// The real system: on this machine's session, whatever it answers.
/// Run with `cargo test -p egui-pop-platform --test text_scale --
/// --ignored`.
#[test]
#[ignore = "reads the session's settings"]
fn the_session_answers() {
    eprintln!("{:?}", text_scale::read());
}
