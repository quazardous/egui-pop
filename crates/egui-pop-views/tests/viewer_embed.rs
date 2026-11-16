//! The viewer's YouTube page over a pretend web view, so these run in every
//! build: what the links policy sends there, what the web view is asked to
//! load and where, the `Referer` naming the application, the player's minimum
//! size, its loop turned, the links it hands back, when it closes, and the
//! error page (ADR 0017).

#![cfg(feature = "markdown")]

use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::dialogs::DialogsPlugin;
use egui_pop_api::links::{self, How, LinksPlugin, RecordingOpener, Request};
use egui_pop_api::{App, AppHandle};
use egui_pop_model::commands::Origin;
use egui_pop_views::viewer::{
    Docs, EmbedBackend, EmbedRequest, Embedded, MIN_PLAYER, PANEL, Placement, VIEWER, Viewer,
    ViewerPlugin, embed_url, fits_player, referrer_for,
};
use serde_json::Value;

const VIDEO: &str = "https://www.youtube.com/watch?v=5TGDf7crzXI";
/// The tests' application id, and the `Referer` built from it.
const APP_ID: &str = "org.example.Viewer";
const REFERRER: &str = "https://org.example.viewer/";

/// What the pretend web view was asked, and what it answers.
#[derive(Default)]
struct Log {
    opened: Vec<EmbedRequest>,
    open: usize,
    placed: Vec<Placement>,
    pumps: usize,
    links: Vec<String>,
    closed: bool,
    own_window: bool,
    /// Its web views sit over the panel (as a child on X11), so the panel's
    /// size is the player's.
    in_panel: bool,
    /// `open` answers this.
    refuse: Option<String>,
    /// `pump` answers this.
    fail: Option<String>,
}

#[derive(Clone, Default)]
struct Pretend(Arc<Mutex<Log>>);

impl Pretend {
    fn log(&self) -> std::sync::MutexGuard<'_, Log> {
        self.0.lock().unwrap()
    }
}

struct View(Pretend);

impl Embedded for View {
    fn place(&mut self, place: &Placement) -> Result<(), String> {
        self.0.log().placed.push(*place);
        Ok(())
    }

    fn pump(&mut self) -> Result<(), String> {
        let mut log = self.0.log();
        log.pumps += 1;
        log.fail.clone().map_or(Ok(()), Err)
    }

    fn own_window(&self) -> bool {
        self.0.log().own_window
    }

    fn closed(&self) -> bool {
        self.0.log().closed
    }

    fn take_links(&mut self) -> Vec<String> {
        std::mem::take(&mut self.0.log().links)
    }
}

impl Drop for View {
    fn drop(&mut self) {
        self.0.log().open -= 1;
    }
}

impl EmbedBackend for Pretend {
    fn name(&self) -> &str {
        "pretend"
    }

    fn in_panel(&self, _: &Placement) -> bool {
        self.log().in_panel
    }

    fn open(&self, request: &EmbedRequest) -> Result<Box<dyn Embedded>, String> {
        let mut log = self.log();
        if let Some(e) = &log.refuse {
            return Err(e.clone());
        }
        log.opened.push(request.clone());
        log.open += 1;
        Ok(Box::new(View(self.clone())))
    }
}

fn plugins(pretend: Option<&Pretend>) -> egui_pop_api::AppBuilder {
    let viewer = ViewerPlugin::default().docs(Docs::new().page("other", "# Other"));
    let viewer = match pretend {
        Some(p) => viewer.web_video(Arc::new(p.clone())),
        None => viewer,
    };
    App::builder()
        .app_id(APP_ID)
        .plugin(DialogsPlugin)
        .plugin(LinksPlugin::default())
        .plugin(viewer)
}

fn viewer_of(app: &AppHandle) -> Arc<Viewer> {
    app.service(&VIEWER).unwrap()
}

type Shown<'a> = Harness<'a, Option<App>>;

/// The application in a window of its own, the video open in its viewer.
fn shown<'a>(pretend: &Pretend, target: &str) -> Shown<'a> {
    shown_at(pretend, target, egui::vec2(1000.0, 640.0))
}

fn shown_at<'a>(pretend: &Pretend, target: &str, size: egui::Vec2) -> Shown<'a> {
    let p = pretend.clone();
    let mut h = Harness::builder().with_size(size).build_ui_state(
        move |ui, app: &mut Option<App>| {
            let app = app.get_or_insert_with(|| plugins(Some(&p)).build(ui.ctx()).unwrap());
            app.show(ui);
        },
        None,
    );
    h.run_steps(2);
    let app = handle(&h);
    links::open(&app, Request::new(target));
    h.run_steps(4);
    h
}

fn handle(h: &Shown<'_>) -> AppHandle {
    h.state().as_ref().unwrap().handle().clone()
}

fn embed(h: &Shown<'_>) -> Value {
    viewer_of(&handle(h)).state()["embed"].clone()
}

#[test]
fn a_youtube_link_plays_in_the_viewer_when_it_has_a_web_view() {
    let pretend = Pretend::default();
    let h = shown(&pretend, VIDEO);
    let s = viewer_of(&handle(&h)).state();
    assert_eq!(
        (s["status"].as_str(), s["kind"].as_str()),
        (Some("shown"), Some("youtube")),
        "{s}"
    );
    assert_eq!(s["plays_web_video"], true);
    assert_eq!(s["embed"]["backend"], "pretend");
    assert_eq!(s["embed"]["open"], true);
    let log = pretend.log();
    assert_eq!(log.opened.len(), 1);
    let r = &log.opened[0];
    // The privacy-enhanced player, with a referrer: YouTube's player refuses
    // to play without one.
    assert_eq!(r.url, embed_url("5TGDf7crzXI"));
    assert!(
        r.url
            .starts_with("https://www.youtube-nocookie.com/embed/5TGDf7crzXI?")
    );
    // The application named, as YouTube's terms ask.
    assert_eq!(r.referrer, REFERRER);
    // No X11 window in a test: a window of its own.
    assert_eq!(r.place.parent_x11, None);
    assert!(r.place.rect.width() > 100.0, "{:?}", r.place.rect);
    let ratio = r.place.rect.width() / r.place.rect.height();
    assert!((ratio - 16.0 / 9.0).abs() < 0.05, "{ratio}");
    assert!(log.pumps > 0, "its loop is turned");
}

#[test]
fn the_referrer_is_built_from_the_application_id() {
    assert_eq!(referrer_for(APP_ID).as_deref(), Some(REFERRER));
    assert_eq!(
        referrer_for(" org.egui_pop.Gallery ").as_deref(),
        Some("https://org.egui_pop.gallery/")
    );
    for bad in [
        "",
        "  ",
        "org example",
        "org..example",
        ".org",
        "org/x",
        "org:1",
    ] {
        assert_eq!(referrer_for(bad), None, "`{bad}`");
    }
    let pretend = Pretend::default();
    let app = App::builder()
        .app_id(APP_ID)
        .plugin(LinksPlugin::default())
        .plugin(ViewerPlugin::default().web_video(Arc::new(pretend.clone())))
        .build(&egui::Context::default())
        .unwrap();
    assert_eq!(app.handle().app_id(), APP_ID);
    links::open(app.handle(), Request::new(VIDEO));
    assert_eq!(
        viewer_of(app.handle()).state()["embed"]["referrer"],
        REFERRER
    );
}

#[test]
fn without_an_application_id_youtube_opens_outside() {
    let pretend = Pretend::default();
    let app = App::builder()
        .plugin(LinksPlugin::default())
        .plugin(ViewerPlugin::default().web_video(Arc::new(pretend.clone())))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    assert!(!viewer_of(h).plays_web_video());
    let rec = RecordingOpener::default();
    links::links(h).unwrap().set_opener(Arc::new(rec.clone()));
    links::open(h, Request::new(VIDEO));
    assert_eq!(rec.records().len(), 1);
    assert_eq!(rec.records()[0].how, How::Outside);
    assert!(pretend.log().opened.is_empty());
}

#[test]
fn the_application_may_give_its_own_referrer() {
    let pretend = Pretend::default();
    let app = App::builder()
        .app_id(APP_ID)
        .plugin(LinksPlugin::default())
        .plugin(
            ViewerPlugin::default()
                .web_video(Arc::new(pretend.clone()))
                .embed_referrer("https://app.example.org/"),
        )
        .build(&egui::Context::default())
        .unwrap();
    links::open(app.handle(), Request::new(VIDEO));
    assert_eq!(
        viewer_of(app.handle()).state()["embed"]["referrer"],
        "https://app.example.org/"
    );
}

#[test]
fn without_a_web_view_youtube_and_other_videos_open_outside() {
    for pretend in [None, Some(Pretend::default())] {
        let app = plugins(pretend.as_ref())
            .build(&egui::Context::default())
            .unwrap();
        let h = app.handle();
        if pretend.is_none() {
            // The build's own web view, with the `webview` feature, goes too.
            viewer_of(h).set_embed_backend(None);
        }
        let rec = RecordingOpener::default();
        links::links(h).unwrap().set_opener(Arc::new(rec.clone()));
        let mut outside = vec!["https://example.org/clip.mp4", "https://vimeo.com/1"];
        if pretend.is_none() {
            outside.push(VIDEO);
        }
        for target in outside {
            rec.clear();
            links::open(h, Request::new(target));
            let records = rec.records();
            assert_eq!(records.len(), 1, "{target}: {records:?}");
            assert_eq!(records[0].how, How::Outside);
        }
        assert_eq!(viewer_of(h).state()["status"], "empty");
    }
}

#[test]
fn the_minimum_is_200_by_200_of_the_web_views_pixels() {
    assert_eq!(MIN_PLAYER, 200.0);
    let fits = |w, h, scale| fits_player(egui::vec2(w, h), scale);
    assert!(fits(200.0, 200.0, 1.0));
    assert!(!fits(199.0, 400.0, 1.0));
    assert!(!fits(400.0, 199.0, 1.0));
    // On a 2x screen the web view's 200 pixels are 400 physical ones.
    assert!(!fits(300.0, 300.0, 2.0));
    assert!(fits(400.0, 400.0, 2.0));
    // A scale below 1 never lowers the physical minimum.
    assert!(!fits(150.0, 150.0, 0.5));
}

#[test]
fn a_panel_too_small_shows_the_card_and_the_player_comes_back_when_it_grows() {
    let pretend = Pretend::default();
    pretend.log().in_panel = true;
    // Too low a window for the player's 200 pixels.
    let mut h = shown_at(&pretend, VIDEO, egui::vec2(1000.0, 180.0));
    assert!(pretend.log().opened.is_empty(), "no web view");
    assert_eq!(embed(&h)["too_small"], true);
    assert!(
        h.query_all_by_value("The panel is too small for YouTube's player")
            .next()
            .is_some()
    );
    assert!(h.query_all_by_label("Open outside").next().is_some());
    // Large enough again: the web view opens, at least 200 by 200.
    h.set_size(egui::vec2(1000.0, 640.0));
    h.run_steps(4);
    assert_eq!(embed(&h)["too_small"], false);
    {
        let log = pretend.log();
        assert_eq!(log.opened.len(), 1);
        let r = log.opened[0].place.rect;
        assert!(r.width() >= MIN_PLAYER && r.height() >= MIN_PLAYER, "{r:?}");
    }
    // Smaller again: closed, the card back.
    h.set_size(egui::vec2(1000.0, 180.0));
    h.run_steps(4);
    assert_eq!(pretend.log().open, 0);
    assert_eq!(embed(&h)["too_small"], true);
}

#[test]
fn the_card_opens_the_video_outside() {
    let pretend = Pretend::default();
    pretend.log().in_panel = true;
    // Too low for the player, high enough for the card's button.
    let mut h = shown_at(&pretend, VIDEO, egui::vec2(1000.0, 240.0));
    assert_eq!(embed(&h)["too_small"], true);
    let rec = RecordingOpener::default();
    links::links(&handle(&h))
        .unwrap()
        .set_opener(Arc::new(rec.clone()));
    links::links(&handle(&h)).unwrap().trust("www.youtube.com");
    // The card's button, after the toolbar's.
    h.query_all_by_label("Open outside").last().unwrap().click();
    h.run_steps(3);
    let r = rec.records();
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!((r[0].target.as_str(), r[0].how), (VIDEO, How::Outside));
}

#[test]
fn a_window_of_its_own_does_not_hang_on_the_panels_size() {
    // Its own window keeps itself at the minimum (the backend's part).
    let pretend = Pretend::default();
    let h = shown_at(&pretend, VIDEO, egui::vec2(1000.0, 180.0));
    assert_eq!(pretend.log().opened.len(), 1);
    assert_eq!(embed(&h)["too_small"], false);
}

#[test]
fn ctrl_click_still_opens_youtube_outside() {
    let pretend = Pretend::default();
    let app = plugins(Some(&pretend))
        .build(&egui::Context::default())
        .unwrap();
    let h = app.handle();
    let rec = RecordingOpener::default();
    links::links(h).unwrap().set_opener(Arc::new(rec.clone()));
    links::open(h, Request::new(VIDEO).how(How::Outside));
    assert_eq!(rec.records().len(), 1);
    assert_eq!(viewer_of(h).state()["status"], "empty");
    assert!(pretend.log().opened.is_empty());
}

#[test]
fn the_web_view_follows_the_page() {
    let pretend = Pretend::default();
    let mut h = shown(&pretend, VIDEO);
    let first = pretend.log().opened[0].place.rect;
    h.set_size(egui::vec2(1200.0, 700.0));
    h.run_steps(3);
    let last = pretend.log().placed.last().copied().unwrap();
    assert_ne!(last.rect, first, "moved with the panel");
}

#[test]
fn links_it_was_asked_to_follow_open_outside() {
    let pretend = Pretend::default();
    let mut h = shown(&pretend, VIDEO);
    let rec = RecordingOpener::default();
    links::links(&handle(&h))
        .unwrap()
        .set_opener(Arc::new(rec.clone()));
    // A link from content: confirmed for a host not trusted, as any other.
    links::links(&handle(&h)).unwrap().trust("www.youtube.com");
    pretend
        .log()
        .links
        .push("https://www.youtube.com/watch?v=5TGDf7crzXI".into());
    h.run_steps(3);
    let r = rec.records();
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(r[0].how, How::Outside);
    assert_eq!(viewer_of(&handle(&h)).state()["kind"], "youtube");
}

#[test]
fn leaving_the_page_closes_the_web_view() {
    let pretend = Pretend::default();
    let mut h = shown(&pretend, VIDEO);
    assert_eq!(pretend.log().open, 1);
    links::open(&handle(&h), Request::new("docs:other"));
    h.run_steps(2);
    assert_eq!(pretend.log().open, 0);
    assert_eq!(embed(&h), Value::Null);
}

#[test]
fn hiding_the_panel_closes_the_web_view_and_showing_it_opens_it_again() {
    let pretend = Pretend::default();
    let mut h = shown(&pretend, VIDEO);
    let app = handle(&h);
    app.set_panel_visible(&PANEL, false);
    h.run_steps(5);
    assert_eq!(pretend.log().open, 0, "closed");
    assert_eq!(embed(&h)["open"], false);
    app.set_panel_visible(&PANEL, true);
    h.run_steps(3);
    assert_eq!(pretend.log().open, 1);
    assert_eq!(pretend.log().opened.len(), 2);
}

#[test]
fn its_own_window_closed_by_the_user_opens_again_on_a_click() {
    let pretend = Pretend::default();
    pretend.log().own_window = true;
    let mut h = shown(&pretend, VIDEO);
    assert_eq!(embed(&h)["own_window"], true);
    pretend.log().closed = true;
    h.run_steps(3);
    assert_eq!(pretend.log().open, 0);
    assert_eq!(embed(&h)["closed"], true);
    pretend.log().closed = false;
    h.get_by_label("Show the video").click();
    h.run_steps(3);
    assert_eq!(pretend.log().open, 1);
    assert_eq!(embed(&h)["closed"], false);
}

#[test]
fn a_web_view_that_cannot_open_gives_a_page_with_open_outside() {
    let pretend = Pretend::default();
    pretend.log().refuse = Some("WebKitGTK is missing".into());
    let mut h = shown(&pretend, VIDEO);
    let s = viewer_of(&handle(&h)).state();
    assert_eq!(s["status"], "failed", "{s}");
    assert_eq!(s["error"]["failure"], "playback");
    assert_eq!(s["error"]["detail"], "WebKitGTK is missing");
    assert!(
        h.query_all_by_value("This video cannot be played here")
            .next()
            .is_some()
    );
    let rec = RecordingOpener::default();
    links::links(&handle(&h))
        .unwrap()
        .set_opener(Arc::new(rec.clone()));
    // What its Open outside does (for a host trusted: else confirmed first).
    assert!(h.query_all_by_label("Open outside").next().is_some());
    links::links(&handle(&h)).unwrap().trust("www.youtube.com");
    viewer_of(&handle(&h))
        .open_outside(&handle(&h), &Origin::User)
        .unwrap();
    h.run_steps(2);
    let r = rec.records();
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!((r[0].target.as_str(), r[0].how), (VIDEO, How::Outside));
}

#[test]
fn a_web_view_failing_gives_the_same_page() {
    let pretend = Pretend::default();
    let mut h = shown(&pretend, VIDEO);
    pretend.log().fail = Some("the web process crashed".into());
    h.run_steps(3);
    let s = viewer_of(&handle(&h)).state();
    assert_eq!(s["status"], "failed", "{s}");
    assert_eq!(s["error"]["detail"], "the web process crashed");
    assert_eq!(s["embed"], Value::Null);
    assert_eq!(pretend.log().open, 0);
}

// The web view on the bus (ADR 0029): a native window owned by the viewer,
// a child of the window its page is drawn in, opened by its `window.open`
// and closed, wherever it is dropped, by its `window.close`.

/// The viewer's native windows listed: (id, parent).
fn web_views(app: &AppHandle) -> Vec<(String, Option<String>)> {
    app.windows_list()
        .into_iter()
        .filter(|w| w.kind == egui_pop_api::bus::WindowKind::Native && w.owner == "egui_pop.viewer")
        .map(|w| (w.id, w.parent))
        .collect()
}

fn said(app: &AppHandle, name: &str) -> Vec<egui_pop_api::host::Event> {
    app.events_since(Default::default())
        .into_iter()
        .filter(|e| e.name == name && e.data["owner"] == "egui_pop.viewer")
        .collect()
}

/// The reasons of the viewer's `window.closed`, in order.
fn closed_reasons(app: &AppHandle) -> Vec<String> {
    said(app, "window.closed")
        .iter()
        .map(|e| e.data["reason"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn the_web_view_is_a_native_window_of_the_viewer_while_shown() {
    let pretend = Pretend::default();
    let mut h = shown(&pretend, VIDEO);
    let app = handle(&h);
    let listed = web_views(&app);
    assert_eq!(listed.len(), 1, "{:?}", app.windows_list());
    assert_eq!(listed[0].1.as_deref(), Some("main"));
    let opened = said(&app, "window.opened");
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].data["window"], listed[0].0.as_str());
    assert_eq!(opened[0].data["kind"], "native");
    // Leaving the page: its `window.close`, the view dropped in that call.
    links::open(&app, Request::new("docs:other"));
    h.run_steps(2);
    assert_eq!(pretend.log().open, 0);
    assert!(web_views(&app).is_empty());
    assert_eq!(closed_reasons(&app), ["command"]);
    assert_eq!(
        said(&app, "window.closed")[0].data["window"],
        listed[0].0.as_str()
    );
    assert!(
        app.broken_invariants().is_empty(),
        "{:?}",
        app.broken_invariants()
    );
}

#[test]
fn hiding_its_panel_closes_the_web_view_on_the_bus() {
    let pretend = Pretend::default();
    let mut h = shown(&pretend, VIDEO);
    let app = handle(&h);
    app.set_panel_visible(&PANEL, false);
    h.run_steps(5);
    assert_eq!(pretend.log().open, 0);
    assert!(web_views(&app).is_empty());
    assert_eq!(closed_reasons(&app), ["command"]);
    // Shown again: a new `window.open`.
    app.set_panel_visible(&PANEL, true);
    h.run_steps(3);
    assert_eq!(web_views(&app).len(), 1);
    assert_eq!(said(&app, "window.opened").len(), 2);
}

#[test]
fn a_panel_too_small_closes_the_web_view_on_the_bus() {
    let pretend = Pretend::default();
    pretend.log().in_panel = true;
    let mut h = shown(&pretend, VIDEO);
    let app = handle(&h);
    assert_eq!(web_views(&app).len(), 1);
    h.set_size(egui::vec2(1000.0, 180.0));
    h.run_steps(4);
    assert_eq!(pretend.log().open, 0);
    assert!(web_views(&app).is_empty());
    assert_eq!(closed_reasons(&app), ["command"]);
}

#[test]
fn its_own_window_closed_by_the_user_is_its_window_close() {
    let pretend = Pretend::default();
    pretend.log().own_window = true;
    let mut h = shown(&pretend, VIDEO);
    let app = handle(&h);
    pretend.log().closed = true;
    h.run_steps(3);
    assert!(web_views(&app).is_empty());
    assert_eq!(closed_reasons(&app), ["user"]);
}

#[test]
fn a_web_view_failing_closes_its_window_on_the_bus() {
    let pretend = Pretend::default();
    let mut h = shown(&pretend, VIDEO);
    let app = handle(&h);
    pretend.log().fail = Some("the web process crashed".into());
    h.run_steps(3);
    assert_eq!(pretend.log().open, 0);
    assert!(web_views(&app).is_empty());
    assert_eq!(closed_reasons(&app), ["system"]);
    // One that cannot open leaves nothing listed.
    let pretend = Pretend::default();
    pretend.log().refuse = Some("WebKitGTK is missing".into());
    let h = shown(&pretend, VIDEO);
    assert!(web_views(&handle(&h)).is_empty());
}

#[test]
fn closing_its_window_closes_the_web_view_first() {
    use egui_pop_api::bus::WindowKind;
    let pretend = Pretend::default();
    let p = pretend.clone();
    let mut m =
        egui_pop_test::windows::MultiWindow::new(move |ctx| plugins(Some(&p)).build(ctx).unwrap());
    m.root_pass();
    let app = m.app().clone();
    links::open(&app, Request::new(VIDEO));
    for _ in 0..3 {
        m.root_pass();
    }
    assert_eq!(web_views(&app).len(), 1, "{:?}", app.windows_list());
    // The viewer in a window of its own: its web view is reopened there.
    assert!(app.detach_panel(&PANEL).unwrap().ok());
    for _ in 0..4 {
        m.root_pass();
    }
    let panel = app
        .windows_list()
        .into_iter()
        .find(|w| w.kind == WindowKind::Panel)
        .expect("the viewer's window")
        .id;
    let listed = web_views(&app);
    assert_eq!(listed.len(), 1, "{:?}", app.windows_list());
    assert_eq!(listed[0].1.as_deref(), Some(panel.as_str()));
    assert_eq!(pretend.log().open, 1);
    // Its window closed by the system: the web view's own `window.close`
    // first (`parent-closed`), the view dropped in that call.
    let v = egui_pop_api::workbench::window_id(&PANEL);
    m.request_close(v);
    m.root_pass();
    let all = app.events_since(Default::default());
    let at = |pred: &dyn Fn(&egui_pop_api::host::Event) -> bool| all.iter().position(pred);
    let child = at(&|e| {
        e.name == "window.closed"
            && e.data["window"] == listed[0].0.as_str()
            && e.data["reason"] == "parent-closed"
    });
    let parent = at(&|e| e.name == "window.closed" && e.data["window"] == panel.as_str());
    assert!(child.is_some(), "{:?}", said(&app, "window.closed"));
    assert!(child < parent, "the web view closed before its window");
    assert!(
        web_views(&app)
            .iter()
            .all(|(_, parent)| parent.as_deref() != Some(panel.as_str())),
        "no child left: {:?}",
        app.windows_list()
    );
    assert!(
        app.broken_invariants().is_empty(),
        "{:?}",
        app.broken_invariants()
    );
}

/// The embed module opens and closes its native view only through the bus:
/// the backend is asked after the view's `window.open`, and the view is
/// dropped only by the viewer's listener on `window.close`.
#[test]
fn the_web_view_opens_and_closes_only_through_the_bus() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/viewer/embed.rs"))
        .unwrap();
    let code: Vec<&str> = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect();
    let find = |needle: &str| {
        code.iter()
            .enumerate()
            .filter(|(_, l)| l.contains(needle))
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    };
    let opens = find("backend.open(");
    let asks = find("request_native_open(");
    assert_eq!(opens.len(), 1, "one place opens a web view");
    assert!(
        asks.len() == 1 && asks[0] < opens[0],
        "its `window.open` is asked first: {asks:?} {opens:?}"
    );
    // Where the listener is: from its fn to the next fn.
    let start = find("fn drop_on_close(");
    assert_eq!(start.len(), 1, "the viewer's listener on `window.close`");
    let end = code
        .iter()
        .enumerate()
        .skip(start[0] + 1)
        .find(|(_, l)| l.contains("fn "))
        .map_or(code.len(), |(i, _)| i);
    for needle in [".view.take()", "view = None", "view.take()"] {
        for i in find(needle) {
            assert!(
                (start[0]..end).contains(&i),
                "a web view dropped outside the bus at line {}: {}",
                i + 1,
                code[i]
            );
        }
    }
    assert!(!find("request_window_close(").is_empty());
}
