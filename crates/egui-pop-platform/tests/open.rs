//! `open`'s contract (docs/links.md, ADR 0017): the system opener's
//! name and capabilities, what it refuses before starting anything,
//! web pages handed to egui's opener, and the plugin installing it in
//! the links service and giving egui's back. Opening a local file or
//! revealing one starts an external program (`xdg-open`, the file
//! manager): not run here.
#![cfg(feature = "open")]

use egui::{Context, RawInput};
use egui_pop_api::links::{self, Caps, How, Kind, OpenCall, Opener, Opening, Source, Target};
use egui_pop_api::{App, BuildError, DefaultPlugins};
use egui_pop_model::commands::Origin;
use egui_pop_platform::open::{OpenPlugin, PLUGIN, SystemOpener};

/// What the opener answers for `target` opened `how`, and the URLs
/// egui was asked to open in that pass.
fn open(target: &str, how: How) -> (Result<(), String>, Vec<egui::OpenUrl>) {
    let target = Target::parse(target).unwrap();
    let ctx = Context::default();
    let mut answer = None;
    let mut out = ctx.run_ui(RawInput::default(), |ui| {
        let ctx = ui.ctx();
        let call = OpenCall {
            target: &target,
            how,
            kind: Kind::Web,
            source: Source::Declared,
            origin: &Origin::User,
            egui: ctx,
        };
        answer = Some(match SystemOpener.open(&call) {
            Opening::Done(r) => r,
            Opening::Later(_) => panic!("{how:?} started something"),
        });
    });
    out.textures_delta.clear();
    let urls = out
        .platform_output
        .commands
        .into_iter()
        .filter_map(|c| match c {
            egui::OutputCommand::OpenUrl(u) => Some(u),
            _ => None,
        })
        .collect();
    (answer.unwrap(), urls)
}

#[test]
fn it_says_what_it_can_do() {
    assert_eq!(SystemOpener.name(), "system");
    let caps = SystemOpener.caps();
    if cfg!(target_arch = "wasm32") {
        assert_eq!(caps, Caps::WEB);
    } else {
        assert!(caps.web && caps.files && caps.reveal, "{caps:?}");
        assert!(!caps.inside && !caps.video && !caps.web_video, "{caps:?}");
    }
}

#[test]
fn web_pages_and_mail_go_through_eguis_opener() {
    let (r, urls) = open("https://example.org/a", How::Outside);
    assert_eq!(r, Ok(()));
    assert_eq!(urls.len(), 1, "{urls:?}");
    assert_eq!(urls[0].url, "https://example.org/a");
    assert!(urls[0].new_tab);

    let (r, urls) = open("mailto:someone@example.org", How::Outside);
    assert_eq!(r, Ok(()));
    assert_eq!(urls.len(), 1, "{urls:?}");
    assert!(!urls[0].new_tab, "mail opens in the same tab");
}

#[test]
fn what_is_not_local_cannot_be_revealed_and_nothing_starts() {
    for how in [How::Reveal, How::Inside, How::Auto] {
        let (r, urls) = open("https://example.org/a", how);
        assert_eq!(
            r,
            Err("only a local file or folder can be shown in its folder".into()),
            "{how:?}"
        );
        assert!(urls.is_empty(), "{how:?}: {urls:?}");
    }
}

#[test]
fn the_plugin_installs_the_system_opener_and_gives_eguis_back() {
    let mut app = App::builder()
        .plugins(DefaultPlugins)
        .plugin(OpenPlugin)
        .build(&Context::default())
        .unwrap();
    let l = links::links(app.handle()).expect("the links service");
    assert_eq!(l.opener().name(), "system");
    assert_eq!(app.disconnect(&PLUGIN), vec![PLUGIN]);
    assert_eq!(l.opener().name(), "egui");
}

#[test]
fn the_plugin_needs_the_links_service() {
    let e = App::builder()
        .plugin(OpenPlugin)
        .build(&Context::default())
        .expect_err("no links service");
    assert!(matches!(e, BuildError::Order(_)), "{e}");
}
