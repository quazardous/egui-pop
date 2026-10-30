//! The viewer driven through its service: what a page may load, by where
//! the page comes from (ADR 0017).

#![cfg(feature = "markdown")]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop_api::links::{self, LinksPlugin, Policy, RecordingOpener, Request, Source};
use egui_pop_api::{App, AppHandle};
use egui_pop_views::viewer::{Docs, Fetch, Fetched, VIEWER, Viewer, ViewerPlugin};
use serde_json::{Value, json};

const HOST: &str = "https://docs.example.org";

/// A network of a few addresses, keeping what was asked of it.
#[derive(Clone, Default)]
struct Net {
    answers: BTreeMap<String, Vec<u8>>,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Net {
    fn with(mut self, url: &str, bytes: &[u8]) -> Self {
        self.answers.insert(url.to_string(), bytes.to_vec());
        self
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Fetch for Net {
    fn fetch(&self, url: &str, done: Box<dyn FnOnce(Fetched) + Send>) {
        self.asked.lock().unwrap().push(url.to_string());
        done(self.answers.get(url).cloned().ok_or_else(|| "gone".into()));
    }
}

fn plugins(docs: Docs, net: &Net) -> egui_pop_api::AppBuilder {
    App::builder()
        .plugin(LinksPlugin::default().policy(Policy::default().docs_host("docs.example.org")))
        .plugin(
            ViewerPlugin::default()
                .docs(docs)
                .fetch(Arc::new(net.clone())),
        )
}

fn app(docs: Docs, net: &Net) -> App {
    plugins(docs, net).build(&egui::Context::default()).unwrap()
}

fn viewer(app: &AppHandle) -> Arc<Viewer> {
    app.service(&VIEWER).unwrap()
}

/// Opens a page as a click would and takes what the network brought.
fn open(app: &AppHandle, target: &str) -> Value {
    links::open(app, Request::new(target));
    viewer(app).poll(app);
    viewer(app).state()
}

/// A folder of files a page could name, removed with the test.
struct Temp(PathBuf);

impl Temp {
    fn new(test: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("egui-pop-viewer-{}-{test}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// A file in it: its `file:` address and its path.
    fn file(&self, name: &str) -> (String, PathBuf) {
        let path = self.0.join(name);
        std::fs::write(&path, b"not an image, only bytes").unwrap();
        (format!("file://{}", path.display()), path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn image_state(state: &Value, uri: &str) -> Option<String> {
    state["images"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["uri"] == uri)
        .map(|i| i["state"].as_str().unwrap().to_string())
}

fn local_images(state: &Value) -> Vec<String> {
    state["images"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["uri"].as_str())
        .filter(|u| u.to_ascii_lowercase().starts_with("file:"))
        .map(str::to_string)
        .collect()
}

#[test]
fn a_remote_page_never_loads_local_images() {
    let tmp = Temp::new("secret");
    let (file, _) = tmp.file("secret.png");
    let page = format!("{HOST}/guide/page.md");
    let net = Net::default()
        .with(
            &page,
            format!("# Remote\n\n![secret]({file})\n\n![shot](img/shot.png)\n").as_bytes(),
        )
        .with(&format!("{HOST}/guide/img/shot.png"), b"bytes");
    let app = app(Docs::new(), &net);
    let s = open(app.handle(), &page);
    assert_eq!(
        (s["status"].as_str(), s["kind"].as_str()),
        (Some("shown"), Some("markdown"))
    );
    assert_eq!(local_images(&s), Vec::<String>::new(), "{s}");
    assert_eq!(s["dropped"], json!([file]), "said out loud");
    // A relative address joins the page's folder, on its own host.
    assert_eq!(
        image_state(&s, &format!("{HOST}/guide/img/shot.png")).as_deref(),
        Some("ready")
    );
    assert!(
        net.asked().iter().all(|u| u.starts_with(HOST)),
        "{:?}",
        net.asked()
    );
}

#[test]
fn a_remote_page_reaches_no_local_image_however_it_is_spelled() {
    let tmp = Temp::new("spelled");
    let (file, path) = tmp.file("spelled.png");
    let upper = file.replacen("file:", "FILE:", 1);
    for markdown in [
        format!("![a]({upper})"),
        format!("![a][r]\n\n[r]: {file}"),
        format!("[![a]({file})](https://example.org/)"),
        format!("![![inner](x.png)]({file})"),
        format!("- item ![a]({file})"),
        format!("| h |\n|---|\n| ![a]({file}) |"),
        // Not an address the web can join: read as a path by the parser.
        format!("![a](<//[bad{}>)", path.display()),
    ] {
        let page = format!("{HOST}/page.md");
        let net = Net::default().with(&page, markdown.as_bytes());
        let app = app(Docs::new(), &net);
        let s = open(app.handle(), &page);
        assert_eq!(local_images(&s), Vec::<String>::new(), "{markdown}: {s}");
    }
}

#[test]
fn a_local_page_keeps_its_images() {
    let tmp = Temp::new("kept");
    let (file, _) = tmp.file("kept.png");
    let docs = Docs::new()
        .page(
            "guide/start",
            &format!("![shipped](../img/a.png)\n\n![local]({file})"),
        )
        .file("img/a.png", b"bytes".as_slice());
    let app = app(docs, &Net::default());
    let s = open(app.handle(), "docs:guide/start");
    assert_eq!(s["dropped"], json!([]));
    assert_eq!(
        image_state(&s, "docs:img/a.png").as_deref(),
        Some("ready"),
        "{s}"
    );
    assert_eq!(image_state(&s, &file).as_deref(), Some("ready"), "{s}");

    // A local page on disk loads the files beside it too.
    let (md, _) = tmp.file("page.md");
    let (beside, path) = tmp.file("beside.png");
    let name = path.file_name().unwrap().to_str().unwrap();
    std::fs::write(md.strip_prefix("file://").unwrap(), format!("![b]({name})")).unwrap();
    viewer(app.handle()).navigate(app.handle(), &md, true);
    let s = viewer(app.handle()).state();
    assert_eq!(s["dropped"], json!([]));
    assert_eq!(image_state(&s, &beside).as_deref(), Some("ready"), "{s}");
}

#[test]
fn an_image_a_local_page_loaded_stays_out_of_a_remote_page() {
    let tmp = Temp::new("both");
    let (file, _) = tmp.file("both.png");
    let markdown = format!("![both]({file})");
    let page = format!("{HOST}/both.md");
    let net = Net::default().with(&page, markdown.as_bytes());
    let app = app(Docs::new().page("both", &markdown), &net);
    let h = app.handle();
    let s = open(h, "docs:both");
    assert_eq!(image_state(&s, &file).as_deref(), Some("ready"), "{s}");
    let s = open(h, &page);
    assert_eq!(image_state(&s, &file), None, "{s}");
    assert_eq!(s["dropped"], json!([file]));
    // And what a remote page was refused does not follow to a local one.
    viewer(h).back(h);
    let s = viewer(h).state();
    assert_eq!(s["target"], "docs:both");
    assert_eq!(image_state(&s, &file).as_deref(), Some("ready"), "{s}");
    assert_eq!(s["dropped"], json!([]));
}

#[test]
fn what_a_remote_page_was_refused_shows_in_the_bar() {
    let tmp = Temp::new("bar");
    let (file, _) = tmp.file("bar.png");
    let page = format!("{HOST}/bar.md");
    let net = Net::default().with(&page, format!("# Bar\n\n![a secret]({file})").as_bytes());
    let opened = Arc::new(Mutex::new(false));
    let (n, target, once) = (net.clone(), page.clone(), opened.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 640.0))
        .build_ui_state(
            move |ui, app: &mut Option<App>| {
                let app =
                    app.get_or_insert_with(|| plugins(Docs::new(), &n).build(ui.ctx()).unwrap());
                if !std::mem::replace(&mut *once.lock().unwrap(), true) {
                    links::open(app.handle(), Request::new(&target));
                }
                app.show(ui);
            },
            None,
        );
    h.run();
    h.run();
    h.get_by_value(&format!(
        "Not shown here: {file}. Open outside shows the page whole."
    ));
    h.get_by_value("Image not shown: a secret");
}

#[test]
fn a_blocked_image_stays_with_its_page() {
    let page = format!("{HOST}/blocked.md");
    let image = "https://images.example.org/a.png";
    let net = Net::default().with(&page, format!("![a]({image})").as_bytes());
    let app = app(Docs::new().page("other", "# Other"), &net);
    let h = app.handle();
    let s = open(h, &page);
    assert_eq!(image_state(&s, image).as_deref(), Some("blocked"));
    assert_eq!(s["blocked_hosts"], json!(["images.example.org"]));
    let s = open(h, "docs:other");
    assert_eq!(image_state(&s, image), None, "{s}");
    assert_eq!(s["blocked_hosts"], json!([]));
    viewer(h).back(h);
    viewer(h).poll(h);
    let s = viewer(h).state();
    assert_eq!(image_state(&s, image).as_deref(), Some("blocked"), "{s}");
}

#[test]
fn links_in_a_remote_page_go_through_the_links_service() {
    let tmp = Temp::new("links");
    let (file, _) = tmp.file("linked.pdf");
    let page = format!("{HOST}/links.md");
    let next = format!("{HOST}/next.md");
    let net = Net::default()
        .with(
            &page,
            format!(
                "[web](https://trusted.example/a) [local]({file}) [cmd](command:x) \
                 [doc](docs:other) [next](next.md)"
            )
            .as_bytes(),
        )
        .with(&next, b"# Next");
    let app = app(Docs::new().page("other", "# Other"), &net);
    let h = app.handle();
    let rec = RecordingOpener::default();
    let service = links::links(h).unwrap();
    service.set_opener(Arc::new(rec.clone()));
    service.trust("trusted.example");
    let v = viewer(h);
    let s = open(h, &page);
    assert_eq!(s["links"].as_array().unwrap().len(), 5, "{s}");
    assert_eq!(s["dropped"], json!([]), "links are not images: {s}");

    // A trusted host opens, as content.
    v.follow(h, "https://trusted.example/a", false);
    let records = rec.records();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].target, "https://trusted.example/a");
    assert_eq!(records[0].source, Source::Content);

    // A local file waits for the user, a command is refused: neither runs.
    for (dest, status) in [(file.as_str(), "asked"), ("command:x", "refused")] {
        rec.clear();
        v.follow(h, dest, false);
        assert!(rec.records().is_empty(), "{dest}");
        let recent = links::recent(h);
        let last = recent.last().unwrap();
        assert_eq!(last["target"], dest, "{recent:?}");
        assert_eq!(last["status"], status, "{recent:?}");
    }

    // The application's pages and its documentation host stay inside.
    v.follow(h, "next.md", false);
    v.poll(h);
    assert_eq!(v.state()["target"], next);
    v.back(h);
    v.poll(h);
    v.follow(h, "docs:other", false);
    assert_eq!(v.state()["target"], "docs:other");
    assert!(rec.records().is_empty());
}
